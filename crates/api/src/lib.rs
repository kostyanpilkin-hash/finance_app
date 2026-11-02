//! LAN host API: /api/v1/hello, /pair, /push, /pull.
//! Auth: Bearer token issued by /pair (device token). Stored hashed in `devices`.

pub const API_VERSION: u32 = 1;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Digest;
use std::net::SocketAddr;
use std::sync::Arc;

use finance_storage::{db as dbm, models, repos, Error, Op, Source};

#[derive(Clone)]
pub struct ApiState {
    pub db: Arc<dbm::Db>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/hello", get(hello))
        .route("/api/v1/pair", post(pair))
        .route("/api/v1/push", post(push))
        .route("/api/v1/pull", get(pull))
        .route("/api/v1/resolve", post(resolve))
        .with_state(state)
}

pub async fn serve(state: ApiState, addr: SocketAddr) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;
    axum::serve(listener, router(state))
        .await
        .map_err(|e| e.to_string())
}

async fn hello(State(s): State<ApiState>) -> Json<Value> {
    Json(json!({"apiVersion": 1, "deviceId": s.db.device_id, "instanceId": s.db.instance_id}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairReq {
    code: String,
    name: String,
    platform: String,
}

async fn pair(
    State(s): State<ApiState>,
    Json(req): Json<PairReq>,
) -> Result<Json<Value>, StatusCode> {
    let res = s.db.with_conn(|c| {
        let uid: String = c
            .query_row(
                "SELECT user_id FROM device_pair_codes WHERE code=?1 AND expires_at > ?2",
                finance_storage::rusqlite::params![req.code, dbm::now_iso()],
                |r| r.get(0),
            )
            .map_err(Error::Db)?;
        let token = uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string();
        let hash = hex(&sha2::Sha256::digest(token.as_bytes()));
        let device_id = dbm::new_id();
        c.execute(
            "INSERT INTO devices (id, user_id, name, platform, token_hash, paired_at) VALUES (?1,?2,?3,?4,?5,?6)",
            finance_storage::rusqlite::params![device_id, uid, req.name, req.platform, hash, dbm::now_iso()],
        )?;
        c.execute("DELETE FROM device_pair_codes WHERE code=?1", [req.code.clone()])?;
        Ok(json!({"deviceId": device_id, "userId": uid, "token": token}))
    }).map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(res))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Authenticates a device by its bearer token.
fn auth(
    c: &finance_storage::rusqlite::Connection,
    token: &str,
) -> Result<(String, Option<String>), Error> {
    let hash = hex(&sha2::Sha256::digest(token.as_bytes()));
    c.query_row(
        "SELECT id, user_id FROM devices WHERE token_hash=?1 AND revoked_at IS NULL",
        [hash],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )
    .map_err(|_| Error::Other("unauthorized".into()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushOp {
    entity: String,
    id: String,
    op: String, // upsert|delete|restore
    #[serde(default)]
    base_version: Option<i64>,
    #[serde(default)]
    payload: Option<Value>,
}

async fn push(
    State(s): State<ApiState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let ops: Vec<PushOp> =
        serde_json::from_value(body["ops"].clone()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();

    let mut results = Vec::new();
    s.db.with_conn(|c| {
        let (device_id, user_id) = auth(c, &token)?;
        let actor = models::Actor { user_id, device_id: Some(device_id.clone()), source: Source::Sync };
        for op in ops {
            let entity = match models::Entity::parse(&op.entity) {
                Some(e) => e,
                None => continue,
            };
            let before = dbm::row_json(c, entity.table(), &op.id)?;
            let cur_version = before.as_ref().and_then(|r| r["version"].as_i64()).unwrap_or(0);
            let base = op.base_version.unwrap_or(cur_version);
            // Delete wins over edit; otherwise a stale base_version is a conflict.
            let deleted_remotely = before.as_ref().and_then(|r| r["deletedAt"].as_str()).is_some();
            let is_delete = op.op == "delete";
            if !is_delete && base < cur_version && !deleted_remotely {
                // Conflict: stash for client-side resolution.
                let local = json!({"entity": op.entity, "id": op.id, "op": op.op, "payload": op.payload});
                let server = before.clone().unwrap_or(json!({}));
                c.execute(
                    "INSERT INTO conflicts (id, entity, entity_id, local_payload, server_payload, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    finance_storage::rusqlite::params![
                        dbm::new_id(), op.entity, op.id, local.to_string(), server.to_string(), dbm::now_iso()
                    ],
                )?;
                results.push(json!({"id": op.id, "status": "conflict"}));
                continue;
            }
            let mop = match op.op.as_str() {
                "delete" => Op::Delete,
                "restore" => Op::Restore,
                _ => Op::Upsert,
            };
            repos::apply(&s.db, c, &actor, entity, mop, &op.id, op.payload)?;
            results.push(json!({"id": op.id, "status": "ok"}));
        }
        c.execute("UPDATE devices SET last_seen_at=?1 WHERE id=?2", finance_storage::rusqlite::params![dbm::now_iso(), device_id])?;
        Ok(())
    }).map_err(|e| if e.to_string() == "unauthorized" { StatusCode::UNAUTHORIZED } else { StatusCode::INTERNAL_SERVER_ERROR })?;
    Ok(Json(json!({"results": results})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveReq {
    conflict_id: String,
    /// Which side wins: "local" (client's version) or "server".
    winner: String,
}

async fn resolve(
    State(s): State<ApiState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ResolveReq>,
) -> Result<Json<Value>, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();
    s.db.with_conn(|c| {
        let (device_id, user_id) = auth(c, &token)?;
        let (entity_s, entity_id, local, server): (String, String, String, String) = c.query_row(
            "SELECT entity, entity_id, local_payload, server_payload FROM conflicts WHERE id=?1 AND resolved_at IS NULL",
            [&req.conflict_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let entity = models::Entity::parse(&entity_s).ok_or_else(|| Error::Invalid(entity_s.clone()))?;
        let actor = models::Actor { user_id, device_id: Some(device_id), source: Source::Sync };
        // "local" applies the client payload (delete wins if its op was delete);
        // "server" just drops the conflict — host state already stands.
        if req.winner == "local" {
            let payload: Value = serde_json::from_str(&local)?;
            let op = if payload["op"].as_str() == Some("delete") { Op::Delete } else { Op::Upsert };
            let p = payload["payload"].clone();
            repos::apply(&s.db, c, &actor, entity, op, &entity_id, if p.is_null() { None } else { Some(p) })?;
        }
        c.execute(
            "UPDATE conflicts SET resolved_at=?1 WHERE id=?2",
            finance_storage::rusqlite::params![dbm::now_iso(), req.conflict_id],
        )?;
        let _ = server;
        Ok(Json(json!({"ok": true})))
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct PullQ {
    since: Option<i64>,
    limit: Option<i64>,
}

async fn pull(
    State(s): State<ApiState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<PullQ>,
) -> Result<Json<Value>, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();
    let since = q.since.unwrap_or(0);
    let limit = q.limit.unwrap_or(2000);
    s.db.with_conn(|c| {
        auth(c, &token)?;
        let mut st = c.prepare(
            "SELECT s.seq, s.entity, s.entity_id, s.op FROM sync_log s WHERE s.seq > ?1 ORDER BY s.seq LIMIT ?2",
        )?;
        let mut rows = st.query_map(finance_storage::rusqlite::params![since, limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        let mut changes = Vec::new();
        while let Some(r) = rows.next().transpose()? {
            let (seq, entity, id, op) = r;
            let payload = dbm::row_json(c, &entity, &id)?;
            changes.push(json!({"seq": seq, "entity": entity, "id": id, "op": op, "row": payload}));
        }
        let mut st2 = c.prepare("SELECT id, entity, entity_id, local_payload, server_payload FROM conflicts WHERE resolved_at IS NULL")?;
        let conflicts: Vec<Value> = st2
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "entity": r.get::<_, String>(1)?, "entityId": r.get::<_, String>(2)?,
                    "local": r.get::<_, String>(3)?, "server": r.get::<_, String>(4)?
                }))
            })?
            .collect::<Result<_, _>>()?;
        let max_seq: i64 = c.query_row("SELECT COALESCE(MAX(seq),0) FROM sync_log", [], |r| r.get(0))?;
        Ok(Json(json!({"changes": changes, "conflicts": conflicts, "maxSeq": max_seq})))
    }).map_err(|e| if e.to_string() == "unauthorized" { StatusCode::UNAUTHORIZED } else { StatusCode::INTERNAL_SERVER_ERROR })
}
