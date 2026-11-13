//! LAN host API: /api/v1/hello, /pair, /push, /pull.
//! Auth: Bearer token issued by /pair (device token). Stored hashed in `devices`.
//! Pairing is two-phase: a code only opens a request, the host user confirms
//! it in the UI; a token is never minted before that confirmation.

pub const API_VERSION: u32 = 1;
/// Failed /pair attempts before the hub locks and burns all active codes.
pub const PAIR_MAX_FAILURES: u32 = 5;
/// Pending and approved-but-unclaimed pairing requests live this long.
const PAIR_REQUEST_TTL: std::time::Duration = std::time::Duration::from_secs(180);

use axum::{
    body::Bytes,
    extract::{connect_info::ConnectInfo, DefaultBodyLimit, Path, Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use finance_storage::{db as dbm, migrations::SCHEMA_VERSION, models, repos, Error, Op, Source};

/// True for addresses that may talk to the host API: loopback, RFC1918,
/// link-local and IPv6 ULA/link-local. Public IPs and CGNAT are refused.
pub fn is_lan_ip(ip: IpAddr) -> bool {
    let v4 = match ip {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                v4
            } else {
                return v6.is_loopback()
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
                    || (v6.segments()[0] & 0xfe00) == 0xfc00;
            }
        }
    };
    v4.is_loopback()
        || v4.is_private() // 10/8, 172.16/12, 192.168/16
        || v4.is_link_local() // 169.254/16
}

/// Rejects non-LAN peers before any route runs. Requires ConnectInfo —
/// see `router()`'s docs.
async fn lan_only(req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .or_else(|| {
            req.extensions()
                .get::<axum::extract::connect_info::MockConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip())
        });
    let ok = peer.map(is_lan_ip).unwrap_or(false);
    if !ok {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "not_lan"}))).into_response();
    }
    next.run(req).await
}

#[derive(Clone)]
pub struct ApiState {
    pub db: Arc<dbm::Db>,
    pub hub: Arc<PairingHub>,
}

/// The router relies on `ConnectInfo<SocketAddr>` being present on every
/// request (peer filter). Serve it with
/// `into_make_service_with_connect_info::<SocketAddr>()` or inject
/// `MockConnectInfo` in tests — requests without it get 403.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/hello", get(hello))
        .route("/api/v1/pair", post(pair))
        .route("/api/v1/pair/status", get(pair_status))
        .route("/api/v1/push", post(push))
        .route("/api/v1/pull", get(pull))
        .route("/api/v1/resolve", post(resolve))
        .route("/api/v1/attachments", get(attachment_index))
        .route(
            "/api/v1/attachments/{sha}",
            get(attachment_get).put(attachment_put),
        )
        .layer(middleware::from_fn(lan_only))
        .layer(DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES))
        .with_state(state)
}

pub async fn serve(state: ApiState, listener: tokio::net::TcpListener) -> Result<(), String> {
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| e.to_string())
}

// ---------- pairing hub ----------

enum PairOutcome {
    Pending,
    Approved { token: String, device_id: String },
    Denied,
}

struct PairRequest {
    user_id: String,
    name: String,
    platform: String,
    ip: String,
    created_at: Instant,
    created_iso: String,
    outcome: PairOutcome,
}

/// Pending view handed to the host UI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub platform: String,
    pub ip: String,
    pub created_at: String,
}

struct HubInner {
    failures: u32,
    requests: HashMap<String, PairRequest>,
}

/// In-memory pairing state: failed-code counter and pending requests.
/// Requests expire lazily on every read/write; the hub keeps no timers.
pub struct PairingHub {
    inner: Mutex<HubInner>,
}

impl Default for PairingHub {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HubInner {
                failures: 0,
                requests: HashMap::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HubInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A freshly generated pair code unlocks pairing again.
    pub fn reset_failures(&self) {
        self.lock().failures = 0;
    }

    pub fn failures(&self) -> u32 {
        self.lock().failures
    }

    pub fn locked(&self) -> bool {
        self.failures() >= PAIR_MAX_FAILURES
    }

    /// Drops requests whose TTL ran out (pending or unclaimed approval).
    fn gc(inner: &mut HubInner) {
        let ttl = PAIR_REQUEST_TTL;
        inner.requests.retain(|_, r| r.created_at.elapsed() < ttl);
    }

    /// Adds a failed-code attempt; returns whether the hub is now locked.
    fn note_failure(&self) -> bool {
        let mut g = self.lock();
        g.failures += 1;
        g.failures >= PAIR_MAX_FAILURES
    }

    /// Registers a pending request; returns its unguessable id.
    /// `#[doc(hidden)]`: public only so integration tests (and the app crate's
    /// hub regression test) can seed requests without HTTP.
    #[doc(hidden)]
    pub fn new_request(&self, user_id: &str, name: &str, platform: &str, ip: String) -> String {
        // Two uuids ≈ 256 bits — not guessable and not enumerable via the
        // status endpoint.
        let id = uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string();
        let mut g = self.lock();
        Self::gc(&mut g);
        g.requests.insert(
            id.clone(),
            PairRequest {
                user_id: user_id.to_string(),
                name: name.to_string(),
                platform: platform.to_string(),
                ip,
                created_at: Instant::now(),
                created_iso: dbm::now_iso(),
                outcome: PairOutcome::Pending,
            },
        );
        id
    }

    /// Host UI list of live pending requests.
    pub fn pending(&self) -> Vec<PendingView> {
        let mut g = self.lock();
        Self::gc(&mut g);
        g.requests
            .iter()
            .filter(|(_, r)| matches!(r.outcome, PairOutcome::Pending))
            .map(|(id, r)| PendingView {
                id: id.clone(),
                user_id: r.user_id.clone(),
                name: r.name.clone(),
                platform: r.platform.clone(),
                ip: r.ip.clone(),
                created_at: r.created_iso.clone(),
            })
            .collect()
    }

    /// Host decision: mint the device token only on approval.
    pub fn decide(&self, db: &dbm::Db, id: &str, allow: bool) -> Result<(), Error> {
        {
            let mut g = self.lock();
            Self::gc(&mut g);
            let req = g
                .requests
                .get_mut(id)
                .ok_or_else(|| Error::Invalid("unknown pairing request".into()))?;
            if !matches!(req.outcome, PairOutcome::Pending) {
                return Err(Error::Invalid("pairing request already decided".into()));
            }
            if !allow {
                req.outcome = PairOutcome::Denied;
                return Ok(());
            }
        }
        // Approved: mint the token exactly like the old one-shot /pair did.
        let token = uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string();
        let hash = hex(&sha2::Sha256::digest(token.as_bytes()));
        let device_id = dbm::new_id();
        db.with_conn(|c| {
            let mut g = self.lock();
            let req = g
                .requests
                .get_mut(id)
                .ok_or_else(|| Error::Invalid("unknown pairing request".into()))?;
            // A concurrent decide could have claimed the request between the
            // outer check and this insert — never mint a second device row.
            if !matches!(req.outcome, PairOutcome::Pending) {
                return Err(Error::Invalid("pairing request already decided".into()));
            }
            c.execute(
                "INSERT INTO devices (id, user_id, name, platform, token_hash, paired_at) VALUES (?1,?2,?3,?4,?5,?6)",
                finance_storage::rusqlite::params![device_id, req.user_id, req.name, req.platform, hash, dbm::now_iso()],
            )?;
            req.outcome = PairOutcome::Approved { token, device_id };
            Ok(())
        })
    }

    /// Client poll. Approved/denied results are delivered exactly once —
    /// the entry is removed when handed out.
    fn poll(&self, id: &str) -> Value {
        let mut g = self.lock();
        Self::gc(&mut g);
        let Some(req) = g.requests.get(id) else {
            return json!({"status": "expired"});
        };
        match &req.outcome {
            PairOutcome::Pending => json!({"status": "pending"}),
            PairOutcome::Denied => {
                g.requests.remove(id);
                json!({"status": "denied"})
            }
            PairOutcome::Approved { token, device_id } => {
                let out = json!({
                    "status": "approved",
                    "token": token,
                    "deviceId": device_id,
                    "userId": req.user_id,
                });
                g.requests.remove(id);
                out
            }
        }
    }
}

/// Schema gate shared by /pair and /push: a client built for a different
/// schema must not proceed. Absent version = pre-0.1.8 client, allowed.
fn schema_gate(client_schema: Option<i64>) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(client_schema) = client_schema else {
        return Ok(());
    };
    if client_schema == SCHEMA_VERSION {
        return Ok(());
    }
    let err = if client_schema < SCHEMA_VERSION {
        "client_outdated"
    } else {
        "host_outdated"
    };
    Err((
        StatusCode::CONFLICT,
        Json(json!({
            "error": err,
            "hostSchema": SCHEMA_VERSION,
            "clientSchema": client_schema,
        })),
    ))
}

async fn hello(State(s): State<ApiState>) -> Json<Value> {
    Json(
        json!({"apiVersion": 1, "deviceId": s.db.device_id, "instanceId": s.db.instance_id, "schemaVersion": SCHEMA_VERSION}),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairReq {
    code: String,
    name: String,
    platform: String,
    #[serde(default)]
    schema_version: Option<i64>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, StatusCode> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn err(code: StatusCode, body: Value) -> (StatusCode, Json<Value>) {
    (code, Json(body))
}

async fn pair(
    State(s): State<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<PairReq>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    schema_gate(req.schema_version)?;
    // One DB critical section per attempt: the locked check, the lookup, the
    // failure counter and the burn-at-5 all happen while the DB mutex is
    // held, so concurrent guesses can't slip past the attempt limit.
    // Lock order: DB → hub (same as decide()). The verdict is computed
    // inside the section too — after spawn_blocking the world may have moved
    // on, so a re-check outside would mislabel a counted failure as a lock.
    enum Verdict {
        Locked,
        BadCode { left: u32 },
        Accepted(String),
    }
    let db = s.db.clone();
    let hub = s.hub.clone();
    let code = req.code.clone();
    let verdict = blocking(move || {
        db.write(|c| {
            if hub.locked() {
                return Ok(Verdict::Locked);
            }
            let uid: Option<String> = match c.query_row(
                "SELECT user_id FROM device_pair_codes WHERE code=?1 AND expires_at > ?2",
                finance_storage::rusqlite::params![&code, dbm::now_iso()],
                |r| r.get(0),
            ) {
                Ok(u) => Some(u),
                Err(finance_storage::rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            };
            if let Some(u) = uid {
                // A valid code is consumed (single use) and unlocks pairing.
                c.execute("DELETE FROM device_pair_codes WHERE code=?1", [&code])?;
                hub.reset_failures();
                return Ok(Verdict::Accepted(u));
            }
            if hub.note_failure() {
                // Locked: burn every active code — they may be compromised.
                c.execute("DELETE FROM device_pair_codes", [])?;
                return Ok(Verdict::Locked);
            }
            Ok(Verdict::BadCode {
                left: PAIR_MAX_FAILURES - hub.failures(),
            })
        })
    })
    .await
    .map_err(bad)?
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error": e.to_string()}),
        )
    })?;
    match verdict {
        Verdict::Locked => Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": "pairing_locked"}),
        )),
        Verdict::BadCode { left } => Err(err(
            StatusCode::FORBIDDEN,
            json!({"error": "bad_code", "attemptsLeft": left}),
        )),
        Verdict::Accepted(user_id) => {
            let id = s
                .hub
                .new_request(&user_id, &req.name, &req.platform, peer.ip().to_string());
            Ok((
                StatusCode::ACCEPTED,
                Json(json!({"status": "pending", "requestId": id})),
            )
                .into_response())
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairStatusQ {
    request_id: String,
}

async fn pair_status(State(s): State<ApiState>, Query(q): Query<PairStatusQ>) -> ApiResult {
    let out = s.hub.poll(&q.request_id);
    if out["status"] == "expired" {
        return Err(err(StatusCode::NOT_FOUND, out));
    }
    Ok(Json(out))
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Missing version = pre-0.1.8 client; accept those (the upsert writer
    // keeps columns they don't send).
    schema_gate(body["schemaVersion"].as_i64())?;
    let ops: Vec<PushOp> =
        serde_json::from_value(body["ops"].clone()).map_err(|_| bad(StatusCode::BAD_REQUEST))?;
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();

    let db = s.db.clone();
    let results: Vec<Value> = blocking(move || {
        db.write(|c| {
        let (device_id, user_id) = auth(c, &token)?;
        let actor = models::Actor { user_id, device_id: Some(device_id.clone()), source: Source::Sync };
        let mut results = Vec::new();
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
                // Conflict: stash for client-side resolution, at most one
                // unresolved row per (entity_id, device).
                let local = json!({"entity": op.entity, "id": op.id, "op": op.op, "payload": op.payload, "deviceId": device_id});
                let server = before.clone().unwrap_or(json!({}));
                c.execute(
                    "DELETE FROM conflicts WHERE entity_id = ?1 AND resolved_at IS NULL
                     AND json_extract(local_payload, '$.deviceId') = ?2",
                    finance_storage::rusqlite::params![op.id, device_id],
                )?;
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
            repos::apply(&db, c, &actor, entity, mop, &op.id, op.payload)?;
            results.push(json!({"id": op.id, "status": "ok"}));
        }
        c.execute("UPDATE devices SET last_seen_at=?1 WHERE id=?2", finance_storage::rusqlite::params![dbm::now_iso(), device_id])?;
        Ok(results)
        })
    })
    .await
    .map_err(bad)?
    .map_err(|e| if e.to_string() == "unauthorized" { bad(StatusCode::UNAUTHORIZED) } else { bad(StatusCode::INTERNAL_SERVER_ERROR) })?;
    Ok(Json(json!({"results": results})))
}

fn bad(code: StatusCode) -> (StatusCode, Json<Value>) {
    (code, Json(json!({})))
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
    let db = s.db.clone();
    blocking(move || {
        db.write(|c| {
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
            repos::apply(&db, c, &actor, entity, op, &entity_id, if p.is_null() { None } else { Some(p) })?;
        }
        c.execute(
            "UPDATE conflicts SET resolved_at=?1 WHERE id=?2",
            finance_storage::rusqlite::params![dbm::now_iso(), req.conflict_id],
        )?;
        let _ = server;
        Ok(Json(json!({"ok": true})))
        })
    })
    .await?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
    let db = s.db.clone();
    blocking(move || {
        db.with_read_conn(|c| {
        // Read snapshot: the whole page reflects one db state.
        let snap = c.unchecked_transaction()?;
        let (device_id, _) = auth(&snap, &token)?;
        let mut st = snap.prepare(
            "SELECT s.seq, s.entity, s.entity_id, s.op FROM sync_log s WHERE s.seq > ?1 ORDER BY s.seq LIMIT ?2",
        )?;
        let mut rows = st.query_map(finance_storage::rusqlite::params![since, limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        let mut changes = Vec::new();
        while let Some(r) = rows.next().transpose()? {
            let (seq, entity, id, op) = r;
            let payload = dbm::row_json(&snap, &entity, &id)?;
            changes.push(json!({"seq": seq, "entity": entity, "id": id, "op": op, "row": payload}));
        }
        // Conflicts are scoped to the requesting device; rows without a
        // deviceId (pre-filter hosts) stay visible to everyone.
        let mut st2 = snap.prepare(
            "SELECT id, entity, entity_id, local_payload, server_payload FROM conflicts
             WHERE resolved_at IS NULL
               AND (json_extract(local_payload, '$.deviceId') IS NULL
                    OR json_extract(local_payload, '$.deviceId') = ?1)",
        )?;
        let conflicts: Vec<Value> = st2
            .query_map([&device_id], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "entity": r.get::<_, String>(1)?, "entityId": r.get::<_, String>(2)?,
                    "local": r.get::<_, String>(3)?, "server": r.get::<_, String>(4)?
                }))
            })?
            .collect::<Result<_, _>>()?;
        // Last seq actually delivered — not the global max, or a paged
        // response would skip rows the client never saw.
        let max_seq = changes
            .last()
            .and_then(|c| c["seq"].as_i64())
            .unwrap_or(since);
        let has_more = changes.len() as i64 >= limit;
        Ok(Json(json!({"changes": changes, "conflicts": conflicts, "maxSeq": max_seq, "hasMore": has_more})))
        })
    })
    .await?
    .map_err(|e| if e.to_string() == "unauthorized" { StatusCode::UNAUTHORIZED } else { StatusCode::INTERNAL_SERVER_ERROR })
}

// ---------- attachment files ----------

/// Receipt photos travel separately from their table rows: the `attachments`
/// row syncs like any other entity, the bytes ride these routes on demand.
const MAX_ATTACHMENT_BYTES: usize = 15 * 1024 * 1024;

/// Attachment file names are sha256 hex digests — anything else is rejected
/// so a crafted name can't wander the filesystem.
fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Bearer-token check shared by the attachment routes (same rule as /pull).
/// Runs on the blocking pool; the db lock is released before any file work.
async fn require_device(
    s: &ApiState,
    headers: &axum::http::HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();
    let db = s.db.clone();
    blocking(move || db.with_read_conn(|c| auth(c, &token).map(|_| ())))
        .await
        .map_err(bad)?
        .map_err(|e| {
            if e.to_string() == "unauthorized" {
                bad(StatusCode::UNAUTHORIZED)
            } else {
                bad(StatusCode::INTERNAL_SERVER_ERROR)
            }
        })
}

/// sha256 names of every file the host has — clients diff this against the
/// attachment rows they pulled to decide what to upload or download.
async fn attachment_index(State(s): State<ApiState>, headers: axum::http::HeaderMap) -> ApiResult {
    require_device(&s, &headers).await?;
    let dir = s.db.attachments_dir.clone();
    let shas = blocking(move || {
        let mut v: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if is_sha(name) {
                        v.push(name.to_string());
                    }
                }
            }
        }
        v.sort();
        v
    })
    .await
    .map_err(bad)?;
    Ok(Json(json!({"shas": shas})))
}

async fn attachment_get(
    State(s): State<ApiState>,
    Path(sha): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    if !is_sha(&sha) {
        return Err(err(StatusCode::BAD_REQUEST, json!({"error": "bad_sha"})));
    }
    require_device(&s, &headers).await?;
    let path = s.db.attachments_dir.join(&sha);
    let bytes = blocking(move || std::fs::read(path).ok())
        .await
        .map_err(bad)?;
    let Some(bytes) = bytes else {
        return Err(err(StatusCode::NOT_FOUND, json!({"error": "missing"})));
    };
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

async fn attachment_put(
    State(s): State<ApiState>,
    Path(sha): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> ApiResult {
    if !is_sha(&sha) {
        return Err(err(StatusCode::BAD_REQUEST, json!({"error": "bad_sha"})));
    }
    if body.is_empty() || body.len() > MAX_ATTACHMENT_BYTES {
        return Err(err(StatusCode::PAYLOAD_TOO_LARGE, json!({"error": "size"})));
    }
    require_device(&s, &headers).await?;
    if hex(&sha2::Sha256::digest(&body)) != sha {
        return Err(err(
            StatusCode::BAD_REQUEST,
            json!({"error": "hash_mismatch"}),
        ));
    }
    let dir = s.db.attachments_dir.clone();
    blocking(move || -> Result<(), String> {
        let target = dir.join(&sha);
        if target.exists() {
            return Ok(()); // content-addressed dedup
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{sha}.tmp"));
        std::fs::write(&tmp, &body).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&tmp, &target) {
            // A concurrent PUT of the same sha already landed — identical
            // bytes, so the lost race is harmless.
            let _ = std::fs::remove_file(&tmp);
            if !target.exists() {
                return Err(e.to_string());
            }
        }
        Ok(())
    })
    .await
    .map_err(bad)?
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": e})))?;
    Ok(Json(json!({"ok": true})))
}
