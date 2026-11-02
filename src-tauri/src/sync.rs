//! LAN sync: host serves the HTTP API; clients pair, push outbox, pull changes.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri::Manager;

use finance_storage::{db as dbm, repos, Error};

use crate::state::AppState;

pub const API_PORT: u16 = 45322;

/// Start the host HTTP server (if mode == "host") + mDNS announcement.
pub fn start_host(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.db.mode != "host" {
        return;
    }
    announce_mdns(&state.db.instance_id);
    let db = Arc::new(clone_db(&state.db));
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], API_PORT));
    tauri::async_runtime::spawn(async move {
        let _ = finance_api::serve(finance_api::ApiState { db }, addr).await;
    });
}

const MDNS_SERVICE: &str = "_finance._tcp.local.";

fn announce_mdns(instance_id: &str) {
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(d) => d,
        Err(_) => return,
    };
    let host = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "finance-host".into());
    let info = mdns_sd::ServiceInfo::new(
        MDNS_SERVICE,
        &format!("finance-{}", &instance_id[..8]),
        &format!("{host}.local."),
        "",
        API_PORT,
        Option::<std::collections::HashMap<String, String>>::None,
    );
    if let Ok(info) = info {
        let _ = daemon.register(info);
        std::mem::forget(daemon); // keep announcing for the process lifetime
    }
}

/// Client: browse for hosts on the LAN for ~2.5s.
pub fn discover_hosts() -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
        return out;
    };
    let Ok(receiver) = daemon.browse(MDNS_SERVICE) else {
        return out;
    };
    let deadline = std::time::Instant::now() + Duration::from_millis(2500);
    while let Ok(event) =
        receiver.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
            for addr in info.get_addresses() {
                out.push(json!({
                    "name": info.get_fullname(),
                    "host": info.get_hostname(),
                    "url": format!("http://{addr}:{}", info.get_port()),
                }));
            }
        }
    }
    out.dedup_by(|a, b| a["url"] == b["url"]);
    out
}

/// Cheap clone handle for the API task (shares the same connection).
fn clone_db(db: &dbm::Db) -> dbm::Db {
    db.share()
}

/// Client loop: push outbox → pull changes → apply, every 20s.
pub fn start_client(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut backoff = Duration::from_secs(5);
        loop {
            let state = app.state::<AppState>();
            if state.db.mode != "client" {
                return;
            }
            match sync_once(&state).await {
                Ok(true) => backoff = Duration::from_secs(20),
                Ok(false) => tokio::time::sleep(backoff).await,
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(120));
                    continue;
                }
            }
        }
    });
}

fn client_cfg(state: &AppState) -> Result<(String, String), Error> {
    state.db.with_conn(|c| {
        let get = |k: &str| repos::setting_get(c, k);
        Ok((
            get("host_url")?.ok_or_else(|| Error::Other("no host_url".into()))?,
            get("device_token")?.ok_or_else(|| Error::Other("no device_token".into()))?,
        ))
    })
}

async fn sync_once(state: &AppState) -> Result<bool, String> {
    let (host, token) = client_cfg(state).map_err(|e| e.to_string())?;
    let http = reqwest::Client::new();

    // 1. Push outbox.
    let ops: Vec<Value> = state.db.with_conn(|c| {
        let mut st = c.prepare("SELECT op_id, entity, entity_id, op, base_version, payload_json FROM outbox ORDER BY created_at")?;
        let v = st
            .query_map([], |r| {
                Ok(json!({
                    "opId": r.get::<_, String>(0)?, "entity": r.get::<_, String>(1)?, "id": r.get::<_, String>(2)?,
                    "op": r.get::<_, String>(3)?, "baseVersion": r.get::<_, Option<i64>>(4)?,
                    "payload": r.get::<_, Option<String>>(5)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }).map_err(|e| e.to_string())?;

    if !ops.is_empty() {
        let resp = http
            .post(format!("{host}/api/v1/push"))
            .bearer_auth(&token)
            .json(&json!({"ops": ops}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("push {}", resp.status()));
        }
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        let ok_ids: Vec<String> = body["results"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter(|x| x["status"] == "ok")
                    .map(|x| x["id"].as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        state
            .db
            .with_conn(|c| {
                for (i, op) in ops.iter().enumerate() {
                    let id = op["id"].as_str().unwrap_or("");
                    if ok_ids.iter().any(|x| x == id) {
                        c.execute(
                            "DELETE FROM outbox WHERE op_id = ?1",
                            [op["opId"].as_str().unwrap()],
                        )?;
                        c.execute(
                            &format!(
                                "UPDATE {} SET local_dirty=0 WHERE id=?1",
                                op["entity"].as_str().unwrap_or("")
                            ),
                            [id],
                        )?;
                    }
                    let _ = i;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
    }

    // 2. Pull changes.
    let last: i64 = state
        .db
        .with_conn(|c| {
            Ok(repos::setting_get(c, "last_seq")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0))
        })
        .map_err(|e| e.to_string())?;
    let resp = http
        .get(format!("{host}/api/v1/pull?since={last}&limit=2000"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("pull {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;

    state
        .db
        .with_conn(|c| {
            for ch in body["changes"].as_array().unwrap_or(&vec![]) {
                let entity = ch["entity"].as_str().unwrap_or("");
                let Some(e) = finance_storage::Entity::parse(entity) else {
                    continue;
                };
                let Some(row) = ch["row"].clone().as_object().cloned() else {
                    continue;
                };
                // Remote rows arrive fully stamped; skip locally-dirty rows that are
                // still queued (host will resolve via conflict on next push).
                let dirty: i64 = c
                    .query_row(
                        &format!("SELECT local_dirty FROM {} WHERE id=?1", e.table()),
                        [row["id"].as_str().unwrap_or("")],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if dirty == 1 {
                    continue;
                }
                dbm::write_row(c, e.table(), &Value::Object(row))?;
            }
            let max_seq = body["maxSeq"].as_i64().unwrap_or(last);
            repos::setting_set(c, "last_seq", &max_seq.to_string())?;
            // Persist unresolved conflicts the host reported.
            for cf in body["conflicts"].as_array().unwrap_or(&vec![]) {
                c.execute(
                    "INSERT OR REPLACE INTO conflicts (id, entity, entity_id, local_payload, server_payload, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    finance_storage::rusqlite::params![
                        cf["id"].as_str().unwrap_or(""), cf["entity"].as_str().unwrap_or(""),
                        cf["entityId"].as_str().unwrap_or(""), cf["local"].as_str().unwrap_or(""),
                        cf["server"].as_str().unwrap_or(""), dbm::now_iso()
                    ],
                )?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    Ok(!ops.is_empty() || !body["changes"].as_array().unwrap_or(&vec![]).is_empty())
}

// ---------- commands ----------

/// Host: create a 6-digit pairing code for a device (valid 10 min).
pub fn pair_code(state: &AppState, user_id: &str) -> Result<String, String> {
    let code: String = (0..6)
        .map(|_| (uuid::Uuid::new_v4().as_bytes()[0] % 10).to_string())
        .collect();
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    state.db.with_conn(|c| {
        c.execute(
            "INSERT OR REPLACE INTO device_pair_codes (code, user_id, expires_at) VALUES (?1,?2,?3)",
            finance_storage::rusqlite::params![code, user_id, exp],
        )?;
        Ok(())
    }).map_err(|e| e.to_string())?;
    Ok(code)
}

/// Client: pair with a host using URL + code → store token + switch mode.
pub async fn pair_client(
    state: &AppState,
    host: &str,
    code: &str,
    name: &str,
) -> Result<(), String> {
    let http = reqwest::Client::new();
    let resp: Value = http
        .post(format!("{host}/api/v1/pair"))
        .json(&json!({"code": code, "name": name, "platform": std::env::consts::OS}))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let token = resp["token"].as_str().ok_or("pairing failed")?;
    state
        .db
        .with_conn(|c| {
            repos::setting_set(c, "host_url", host)?;
            repos::setting_set(c, "device_token", token)?;
            repos::setting_set(c, "last_seq", "0")
        })
        .map_err(|e| e.to_string())?;
    state.db.set_mode("client").map_err(|e| e.to_string())?;
    Ok(())
}

/// Client: unresolved conflicts (id, entity, both versions).
pub fn conflict_list(state: &AppState) -> Result<Vec<Value>, String> {
    state.db.with_conn(|c| {
        let mut st = c.prepare("SELECT id, entity, entity_id, local_payload, server_payload FROM conflicts WHERE resolved_at IS NULL")?;
        let v = st
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "entity": r.get::<_, String>(1)?, "entityId": r.get::<_, String>(2)?,
                    "local": r.get::<_, String>(3)?, "server": r.get::<_, String>(4)?
                }))
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }).map_err(|e| e.to_string())
}

/// Client: pick a winner and tell the host.
pub async fn conflict_resolve(
    state: &AppState,
    conflict_id: &str,
    winner: &str,
) -> Result<(), String> {
    let (host, token) = client_cfg(state).map_err(|e| e.to_string())?;
    let resp = reqwest::Client::new()
        .post(format!("{host}/api/v1/resolve"))
        .bearer_auth(&token)
        .json(&json!({"conflictId": conflict_id, "winner": winner}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("resolve {}", resp.status()));
    }
    state
        .db
        .with_conn(|c| {
            c.execute(
                "UPDATE conflicts SET resolved_at=?1 WHERE id=?2",
                finance_storage::rusqlite::params![dbm::now_iso(), conflict_id],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

pub fn sync_status(state: &AppState) -> Value {
    let _ = state;
    state.db.with_conn(|c| {
        let host = repos::setting_get(c, "host_url")?;
        let seq = repos::setting_get(c, "last_seq")?;
        let unresolved: i64 = c.query_row("SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
        let pending: i64 = c.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?;
        let devices: i64 = c.query_row("SELECT COUNT(*) FROM devices WHERE revoked_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
        Ok(json!({"mode": state.db.mode, "hostUrl": host, "lastSeq": seq, "pendingOps": pending, "pairedDevices": devices, "conflicts": unresolved}))
    }).unwrap_or(json!({}))
}
