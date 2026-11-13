//! LAN sync: host serves the HTTP API; clients pair, push outbox, pull changes.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri::Manager;

use finance_storage::{db as dbm, repos, Error};

use crate::state::AppState;

pub const API_PORT: u16 = 45322;

const KEY_SERVICE: &str = "by.kostyan.finance";
const DEVICE_TOKEN_ACCOUNT: &str = "device-token";

/// The last sync_once outcome, for the settings UI (esp. schema mismatches).
static LAST_ERROR: Mutex<Option<Value>> = Mutex::new(None);

fn last_error_set(v: Option<Value>) {
    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = v;
}

fn save_device_token(token: &str) -> Result<(), String> {
    keyring::Entry::new(KEY_SERVICE, DEVICE_TOKEN_ACCOUNT)
        .map_err(|e| e.to_string())?
        .set_secret(token.as_bytes())
        .map_err(|e| e.to_string())
}

fn read_device_token() -> Option<String> {
    keyring::Entry::new(KEY_SERVICE, DEVICE_TOKEN_ACCOUNT)
        .ok()?
        .get_secret()
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Moves a pre-keyring `settings.device_token` into the OS keyring. The row is
/// deleted only after the keyring write succeeds — a failure keeps the value
/// where the old code can still read it. Testable with a fake `save`.
fn migrate_legacy_token(
    c: &finance_storage::rusqlite::Connection,
    save: impl Fn(&str) -> Result<(), String>,
) -> Result<bool, Error> {
    let Some(token) = repos::setting_get(c, "device_token")? else {
        return Ok(false);
    };
    if token.is_empty() {
        return Ok(false);
    }
    if save(&token).is_err() {
        return Ok(false); // keep the settings row for the next attempt
    }
    c.execute("DELETE FROM settings WHERE key = 'device_token'", [])?;
    Ok(true)
}

/// One-shot migration at startup; failures are logged, never fatal.
pub fn migrate_device_token(state: &AppState) {
    let r = state
        .db
        .with_conn(|c| migrate_legacy_token(c, save_device_token));
    match r {
        Ok(true) => eprintln!("device token migrated to the OS keyring"),
        Ok(false) => {}
        Err(e) => eprintln!("device token migration failed: {e}"),
    }
}

/// RFC1918 only: loopback, link-local, unspecified and public/CGNAT
/// addresses can never host the sync API. Sorted for stable UI/tests.
pub(crate) fn lan_ipv4s(
    all: impl IntoIterator<Item = std::net::Ipv4Addr>,
) -> Vec<std::net::Ipv4Addr> {
    let mut ips: Vec<std::net::Ipv4Addr> = all.into_iter().filter(|ip| ip.is_private()).collect();
    ips.sort();
    ips.dedup();
    ips
}

/// All IPv4 addresses currently assigned to local interfaces.
fn current_ipv4s() -> Vec<std::net::Ipv4Addr> {
    let mut ips: Vec<std::net::Ipv4Addr> = Vec::new();
    if let Ok(std::net::IpAddr::V4(ip)) = local_ip_address::local_ip() {
        ips.push(ip);
    }
    if let Ok(ifas) = local_ip_address::list_afinet_netifas() {
        for (_, addr) in ifas {
            if let std::net::IpAddr::V4(ip) = addr {
                ips.push(ip);
            }
        }
    }
    ips
}

/// The pairing hub shared by the host API server and the pairing commands.
/// Managed as `Arc<PairingHub>` — every lookup must go through `hub()`; the
/// plain `PairingHub` type is never registered.
pub(crate) fn manage_hub<R: tauri::Runtime>(app: &AppHandle<R>) -> Arc<finance_api::PairingHub> {
    let hub = Arc::new(finance_api::PairingHub::new());
    app.manage(hub.clone());
    hub
}

pub(crate) fn hub<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<Arc<finance_api::PairingHub>> {
    app.try_state::<Arc<finance_api::PairingHub>>()
        .map(|h| h.inner().clone())
}

/// Start the host HTTP server (if mode == "host") + mDNS announcement.
/// Binds one listener per private IPv4 — never 0.0.0.0 — and re-reads the
/// interface list every 30 s so network changes pick up/drop listeners.
pub fn start_host(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.db.mode != "host" {
        return;
    }
    announce_mdns(&state.db.instance_id);
    let db = Arc::new(clone_db(&state.db));
    let hub = manage_hub(app);
    tauri::async_runtime::spawn(async move {
        let mut listeners: std::collections::HashMap<
            std::net::Ipv4Addr,
            tauri::async_runtime::JoinHandle<()>,
        > = std::collections::HashMap::new();
        loop {
            let want = lan_ipv4s(current_ipv4s());
            // Drop listeners whose address vanished.
            listeners.retain(|ip, h| {
                if want.contains(ip) {
                    true
                } else {
                    h.abort();
                    false
                }
            });
            for ip in want {
                let alive = listeners.get(&ip).is_some_and(|h| !h.inner().is_finished());
                if alive {
                    continue;
                }
                let api_state = finance_api::ApiState {
                    db: db.clone(),
                    hub: hub.clone(),
                };
                let handle = tauri::async_runtime::spawn(async move {
                    match tokio::net::TcpListener::bind(std::net::SocketAddr::from((ip, API_PORT)))
                        .await
                    {
                        Ok(l) => {
                            let _ = finance_api::serve(api_state, l).await;
                        }
                        Err(e) => eprintln!("sync: bind {ip}:{API_PORT} failed: {e}"),
                    }
                });
                listeners.insert(ip, handle);
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
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
    state.db.with_read_conn(|c| {
        let get = |k: &str| repos::setting_get(c, k);
        // Keyring first; a pre-keyring settings row is the fallback until the
        // startup migration moves it over.
        let token = read_device_token()
            .or(get("device_token").ok().flatten())
            .ok_or_else(|| Error::Other("no device_token".into()))?;
        Ok((
            get("host_url")?.ok_or_else(|| Error::Other("no host_url".into()))?,
            token,
        ))
    })
}

pub(crate) async fn sync_once(state: &AppState) -> Result<bool, String> {
    match sync_once_inner(state).await {
        Err(e) => {
            // Schema errors already stored their structured payload above.
            if e != "client_outdated" && e != "host_outdated" {
                last_error_set(Some(json!({"code": "error", "message": e})));
            }
            Err(e)
        }
        ok => {
            last_error_set(None);
            ok
        }
    }
}

async fn sync_once_inner(state: &AppState) -> Result<bool, String> {
    let (host, token) = client_cfg(state).map_err(|e| e.to_string())?;
    let http = crate::net::client();

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
            .json(
                &json!({"ops": ops, "schemaVersion": finance_storage::migrations::SCHEMA_VERSION}),
            )
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().as_u16() == 409 {
            // Schema mismatch — parse the code so the UI can explain it.
            let body: Value = resp.json().await.unwrap_or(json!({}));
            let code = body["error"].as_str().unwrap_or("").to_string();
            if code == "client_outdated" || code == "host_outdated" {
                last_error_set(Some(json!({
                    "code": code,
                    "hostSchema": body["hostSchema"],
                    "clientSchema": body["clientSchema"],
                })));
                return Err(code);
            }
            return Err("push 409".to_string());
        }
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
            .write(|c| {
                for op in &ops {
                    let id = op["id"].as_str().unwrap_or("");
                    if !ok_ids.iter().any(|x| x == id) {
                        continue;
                    }
                    let Some(e) = op["entity"]
                        .as_str()
                        .and_then(finance_storage::Entity::parse)
                    else {
                        continue;
                    };
                    c.execute(
                        "DELETE FROM outbox WHERE op_id = ?1",
                        [op["opId"].as_str().unwrap_or("")],
                    )?;
                    c.execute(
                        &format!(
                            "UPDATE {} SET local_dirty=0 WHERE id=?1 AND NOT EXISTS (SELECT 1 FROM outbox WHERE entity_id=?1)",
                            e.table()
                        ),
                        [id],
                    )?;
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
        .write(|c| {
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

/// Candidate URLs clients can reach this host at — the same private
/// addresses the API binds to, shown in Settings so the user doesn't have to
/// run ipconfig/ifconfig.
pub fn host_urls() -> Vec<String> {
    lan_ipv4s(current_ipv4s())
        .into_iter()
        .map(|ip| format!("http://{ip}:{API_PORT}"))
        .collect()
}

// ---------- commands ----------

/// Host: create a 6-digit pairing code for a device (valid 10 min).
/// Generating a fresh code also lifts the failed-attempts lock.
pub fn pair_code<R: tauri::Runtime>(
    app: &AppHandle<R>,
    db: &dbm::Db,
    user_id: &str,
) -> Result<String, String> {
    if let Some(hub) = hub(app) {
        hub.reset_failures();
    }
    let code: String = (0..6)
        .map(|_| (uuid::Uuid::new_v4().as_bytes()[0] % 10).to_string())
        .collect();
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    db.with_conn(|c| {
        c.execute(
            "INSERT OR REPLACE INTO device_pair_codes (code, user_id, expires_at) VALUES (?1,?2,?3)",
            finance_storage::rusqlite::params![code, user_id, exp],
        )?;
        Ok(())
    }).map_err(|e| e.to_string())?;
    Ok(code)
}

/// What a successful pairing hands back to the client.
#[derive(Debug)]
pub struct PairGrant {
    pub token: String,
    pub user_id: String,
    #[allow(dead_code)]
    pub device_id: String,
}

/// Pure network half of pair_client: POST the code, then poll the status
/// endpoint until the host user decides. No db, no keyring — testable
/// against a bare finance_api server.
pub(crate) async fn request_pairing(
    http: &reqwest::Client,
    host: &str,
    code: &str,
    name: &str,
    poll: Duration,
    timeout: Duration,
) -> Result<PairGrant, String> {
    let resp = http
        .post(format!("{host}/api/v1/pair"))
        .json(&json!({
            "code": code,
            "name": name,
            "platform": std::env::consts::OS,
            "schemaVersion": finance_storage::migrations::SCHEMA_VERSION,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));
    if status.as_u16() == 202 {
        let request_id = body["requestId"].as_str().ok_or("pairing failed")?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            tokio::time::sleep(poll).await;
            let st_resp = http
                .get(format!("{host}/api/v1/pair/status"))
                .query(&[("requestId", request_id)])
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if st_resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Err("pair_expired".into());
            }
            let st: Value = st_resp.json().await.unwrap_or(json!({}));
            match st["status"].as_str() {
                Some("pending") => {}
                Some("approved") => {
                    return Ok(PairGrant {
                        token: st["token"].as_str().unwrap_or("").to_string(),
                        user_id: st["userId"].as_str().unwrap_or("").to_string(),
                        device_id: st["deviceId"].as_str().unwrap_or("").to_string(),
                    });
                }
                Some("denied") => return Err("pair_denied".into()),
                Some("expired") | None => return Err("pair_expired".into()),
                Some(_) => return Err("pair_expired".into()),
            }
            if std::time::Instant::now() >= deadline {
                return Err("pair_timeout".into());
            }
        }
    }
    if status.as_u16() == 409 {
        return Err(body["error"]
            .as_str()
            .unwrap_or("schema_mismatch")
            .to_string());
    }
    if status.as_u16() == 429 {
        return Err("pairing_locked".into());
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        if body["error"] == "bad_code" {
            return Err(format!(
                "bad_code:{}",
                body["attemptsLeft"].as_u64().unwrap_or(0)
            ));
        }
        return Err(body["error"].as_str().unwrap_or("forbidden").to_string());
    }
    Err(format!(
        "host returned {}: {}",
        status,
        body["error"].as_str().unwrap_or("")
    ))
}

/// Client: pair with a host using URL + code → store token + switch mode.
/// Returns the host-side user id this device paired as.
pub async fn pair_client(
    state: &AppState,
    host: &str,
    code: &str,
    name: &str,
) -> Result<String, String> {
    let http = crate::net::client();
    let grant = request_pairing(
        http,
        host,
        code,
        name,
        Duration::from_secs(2),
        Duration::from_secs(180),
    )
    .await?;
    // The device token is a secret — keyring only, never a settings row.
    save_device_token(&grant.token)?;
    let user_id = grant.user_id;
    state
        .db
        .write(|c| {
            repos::setting_set(c, "host_url", host)?;
            repos::setting_set(c, "last_seq", "0")?;
            if !user_id.is_empty() {
                repos::setting_set(c, "current_user_id", &user_id)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    state.db.set_mode("client").map_err(|e| e.to_string())?;
    Ok(user_id)
}

/// Onboarding join: pair + drain the host journal + mark onboarding done.
/// The app must relaunch afterwards — `db.mode` is read once at open, so the
/// process keeps running as "host" (server + no client loop) until restart.
pub async fn join_host(state: &AppState, host: &str, code: &str, name: &str) -> Result<(), String> {
    pair_client(state, host, code, name).await?;
    for _ in 0..50 {
        match sync_once(state).await {
            Ok(true) => continue,
            Ok(false) => break,
            Err(e) => return Err(format!("sync failed: {e}")),
        }
    }
    state
        .db
        .with_conn(|c| repos::setting_set(c, "onboarding_done", "1"))
        .map_err(|e| e.to_string())
}

/// Client: unresolved conflicts (id, entity, both versions).
pub fn conflict_list(db: &dbm::Db) -> Result<Vec<Value>, String> {
    db.with_read_conn(|c| {
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
    let resp = crate::net::client()
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

pub fn sync_status(db: &dbm::Db) -> Value {
    db.with_read_conn(|c| {
        let host = repos::setting_get(c, "host_url")?;
        let seq = repos::setting_get(c, "last_seq")?;
        let unresolved: i64 = c.query_row("SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
        let pending: i64 = c.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?;
        let devices: i64 = c.query_row("SELECT COUNT(*) FROM devices WHERE revoked_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
        let last_error = LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok(json!({"mode": db.mode, "hostUrl": host, "lastSeq": seq, "pendingOps": pending, "pairedDevices": devices, "conflicts": unresolved, "lastError": last_error}))
    }).unwrap_or(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> finance_storage::rusqlite::Connection {
        let c = finance_storage::rusqlite::Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn migrate_moves_token_and_deletes_row() {
        let c = conn();
        repos::setting_set(&c, "device_token", "secret-tok").unwrap();
        let saved = std::cell::RefCell::new(String::new());
        let moved = migrate_legacy_token(&c, |t| {
            *saved.borrow_mut() = t.to_string();
            Ok(())
        })
        .unwrap();
        assert!(moved);
        assert_eq!(*saved.borrow(), "secret-tok");
        assert_eq!(repos::setting_get(&c, "device_token").unwrap(), None);
    }

    #[test]
    fn migrate_keeps_row_when_save_fails() {
        let c = conn();
        repos::setting_set(&c, "device_token", "secret-tok").unwrap();
        let moved = migrate_legacy_token(&c, |_| Err("keyring down".into())).unwrap();
        assert!(!moved);
        assert_eq!(
            repos::setting_get(&c, "device_token").unwrap(),
            Some("secret-tok".into())
        );
    }

    #[test]
    fn migrate_noop_without_token() {
        let c = conn();
        let called = std::cell::Cell::new(false);
        let moved = migrate_legacy_token(&c, |_| {
            called.set(true);
            Ok(())
        })
        .unwrap();
        assert!(!moved);
        assert!(!called.get());
        // An empty value migrates nothing either.
        repos::setting_set(&c, "device_token", "").unwrap();
        assert!(!migrate_legacy_token(&c, |_| Ok(())).unwrap());
    }

    /// Regression for the hub-wiring bug: the report a command returns must
    /// reflect a pending request on the hub it was built from. (tauri::test's
    /// MockRuntime crashes the test binary on Windows — STATUS_ENTRYPOINT_NOT_FOUND
    /// — so the report itself is tested here without an AppHandle.)
    #[test]
    fn pairing_report_reflects_pending() {
        let hub = finance_api::PairingHub::new();
        let id = hub.new_request("u1", "dev", "test", "192.168.1.9".into());
        let report = crate::commands::pairing_report(Some(&hub));
        assert_eq!(report["requests"][0]["id"], id);
        assert_eq!(report["requests"][0]["name"], "dev");
        assert_eq!(report["requests"][0]["ip"], "192.168.1.9");
        assert_eq!(report["locked"], false);
        assert_eq!(report["failures"], 0);
        assert_eq!(
            crate::commands::pairing_report(None),
            json!({"requests": [], "locked": false, "failures": 0})
        );
    }

    #[test]
    fn lan_ipv4s_keeps_only_rfc1918() {
        use std::net::Ipv4Addr;
        let v4 = |a, b, c, d| Ipv4Addr::new(a, b, c, d);
        let got = lan_ipv4s(vec![
            v4(192, 168, 1, 5),
            v4(10, 0, 0, 1),
            v4(8, 8, 8, 8),     // public
            v4(127, 0, 0, 1),   // loopback
            v4(169, 254, 1, 1), // link-local
            v4(100, 64, 0, 1),  // CGNAT
            v4(0, 0, 0, 0),     // unspecified
            v4(192, 168, 1, 5), // duplicate
        ]);
        assert_eq!(got, vec![v4(10, 0, 0, 1), v4(192, 168, 1, 5)]);
    }

    /// The browse window is a fixed ~2.5 s scan — long enough that running it
    /// on the IPC thread froze the window; the command runs on spawn_blocking.
    /// This guards the total duration from growing unnoticed.
    #[test]
    fn discover_hosts_completes_within_browse_window() {
        let t = std::time::Instant::now();
        let _ = discover_hosts();
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    /// End-to-end: a real finance_api server on loopback, a code in an
    /// in-memory db, and request_pairing polling until the hub decides.
    #[test]
    fn request_pairing_e2e() {
        tauri::async_runtime::block_on(async {
            finance_storage::secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
            let db = Arc::new(dbm::Db::open_memory(Some(&[7u8; 32])).unwrap());
            let hub = Arc::new(finance_api::PairingHub::new());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let state = finance_api::ApiState {
                db: db.clone(),
                hub: hub.clone(),
            };
            tauri::async_runtime::spawn(async move {
                let _ = finance_api::serve(state, listener).await;
            });

            let exp = {
                let t: chrono::DateTime<chrono::Utc> =
                    (std::time::SystemTime::now() + Duration::from_secs(600)).into();
                t.to_rfc3339()
            };
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO users (id, name, created_at, updated_at) VALUES ('u1','Иван',?1,?1)",
                    [dbm::now_iso()],
                )?;
                c.execute(
                    "INSERT INTO device_pair_codes (code, user_id, expires_at) VALUES ('424242','u1',?1),('555555','u1',?1)",
                    [&exp],
                )?;
                Ok(())
            })
            .unwrap();

            let http = reqwest::Client::new();
            let host = format!("http://{addr}");

            // Approver: grants the request named dev-ok.
            {
                let hub = hub.clone();
                let db = db.clone();
                tauri::async_runtime::spawn(async move {
                    for _ in 0..200 {
                        if let Some(p) = hub.pending().iter().find(|p| p.name == "dev-ok") {
                            hub.decide(&db, &p.id, true).unwrap();
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                });
            }
            let grant = request_pairing(
                &http,
                &host,
                "424242",
                "dev-ok",
                Duration::from_millis(100),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
            assert_eq!(grant.user_id, "u1");
            let hash = {
                use sha2::Digest;
                sha2::Sha256::digest(grant.token.as_bytes())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            };
            let stored: String = db
                .with_conn(|c| {
                    Ok(c.query_row(
                        "SELECT token_hash FROM devices WHERE id=?1",
                        [&grant.device_id],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert_eq!(stored, hash);

            // Denier: rejects the request named dev-no.
            {
                let hub = hub.clone();
                let db = db.clone();
                tauri::async_runtime::spawn(async move {
                    for _ in 0..200 {
                        if let Some(p) = hub.pending().iter().find(|p| p.name == "dev-no") {
                            hub.decide(&db, &p.id, false).unwrap();
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                });
            }
            let err = request_pairing(
                &http,
                &host,
                "555555",
                "dev-no",
                Duration::from_millis(100),
                Duration::from_secs(10),
            )
            .await
            .unwrap_err();
            assert_eq!(err, "pair_denied");

            let err = request_pairing(
                &http,
                &host,
                "000000",
                "dev-x",
                Duration::from_millis(100),
                Duration::from_secs(10),
            )
            .await
            .unwrap_err();
            assert!(err.starts_with("bad_code:"), "{err}");
        });
    }
}
