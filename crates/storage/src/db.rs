//! Low-level helpers: generic row JSON, apply_change (audit + version + seq + outbox),
//! pull/push application primitives shared by the host API and the client sync.

use chrono::{SecondsFormat, Utc};
use rusqlite::{types::ValueRef, Connection};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use uuid::Uuid;

use crate::error::Error;
use crate::models::{Actor, Entity, Op};

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn new_id() -> String {
    Uuid::now_v7().to_string()
}

/// Reads a whole row as a camelCase JSON map.
pub fn row_json(conn: &Connection, table: &str, id: &str) -> Result<Option<Value>, Error> {
    let mut stmt = conn.prepare_cached(&format!("SELECT * FROM {table} WHERE id = ?1"))?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query([id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(row_to_json(row, &cols)))
}

type CamelCache = Mutex<std::collections::HashMap<Vec<String>, std::sync::Arc<Vec<String>>>>;

/// snake_case → camelCase for a whole column list, memoized. Result rows share
/// the same handful of column shapes, so one lookup replaces the per-row work.
fn camel_names(cols: &[String]) -> std::sync::Arc<Vec<String>> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<CamelCache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(cols) {
        return v.clone();
    }
    let v = std::sync::Arc::new(cols.iter().map(|c| snake_to_camel(c)).collect::<Vec<_>>());
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(cols.to_vec(), v.clone());
    v
}

pub fn row_to_json(row: &rusqlite::Row, cols: &[String]) -> Value {
    let names = camel_names(cols);
    let mut map = Map::with_capacity(cols.len());
    for (i, name) in names.iter().enumerate() {
        let v = match row.get_ref(i) {
            Ok(ValueRef::Null) => Value::Null,
            Ok(ValueRef::Integer(n)) => json!(n),
            Ok(ValueRef::Real(f)) => json!(f),
            Ok(ValueRef::Text(t)) => json!(String::from_utf8_lossy(t)),
            Ok(ValueRef::Blob(b)) => {
                json!(data_encoding::BASE64.encode(b))
            }
            Err(_) => Value::Null,
        };
        map.insert(name.clone(), v);
    }
    Value::Object(map)
}

pub fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Field-level diff between two JSON snapshots.
pub fn diff_json(before: &Value, after: &Value) -> Value {
    let mut changes = Vec::new();
    let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(o) = before.as_object() {
        keys.extend(o.keys().cloned());
    }
    if let Some(o) = after.as_object() {
        keys.extend(o.keys().cloned());
    }
    for k in keys {
        // Sync internals are noise for the user-facing journal.
        if matches!(
            k.as_str(),
            "version" | "updatedAt" | "updatedByDevice" | "seq" | "localDirty" | "createdAt"
        ) {
            continue;
        }
        let b = &before[k.as_str()];
        let a = &after[k.as_str()];
        if b != a {
            changes.push(json!({"field": k, "before": b, "after": a}));
        }
    }
    Value::Array(changes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Host,
    Client,
}

impl Mode {
    pub fn parse(s: &str) -> Self {
        if s == "client" { Mode::Client } else { Mode::Host }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Host => "host",
            Mode::Client => "client",
        }
    }
}

pub struct Db {
    /// Write connection: every mutation (and its journal/outbox side effects)
    /// serializes on this mutex.
    pub conn: std::sync::Arc<Mutex<Connection>>,
    /// Dedicated read connection (WAL): UI queries don't wait on a busy writer
    /// (sync pushes, bot, backups). Never used for writes.
    pub read_conn: std::sync::Arc<Mutex<Connection>>,
    /// host | client — set by the wizard; default host.
    pub mode: Mode,
    /// Stable id of this device (this machine's database).
    pub device_id: String,
    /// Generated once for this installation.
    pub instance_id: String,
    /// Files (receipt photos) live next to the db.
    pub attachments_dir: std::path::PathBuf,
}

impl Db {
    /// Cheap handle sharing the same connections (for the API server task).
    pub fn share(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            read_conn: self.read_conn.clone(),
            mode: self.mode,
            device_id: self.device_id.clone(),
            instance_id: self.instance_id.clone(),
            attachments_dir: self.attachments_dir.clone(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn is_host(&self) -> bool {
        self.mode() == Mode::Host
    }
    pub fn is_client(&self) -> bool {
        self.mode() == Mode::Client
    }
}

pub struct ApplyCtx<'a> {
    pub conn: &'a Connection,
    pub db: &'a Db,
    pub actor: &'a Actor,
}

pub struct ApplyResult {
    /// True when the change was recorded (for new rows there is no "conflict" path locally).
    pub applied: bool,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Bump version/updated_at on a JSON row before writing it back.
fn stamp(row: &mut Value, device_id: &str) {
    let map = row.as_object_mut().unwrap();
    let version = map.get("version").and_then(Value::as_i64).unwrap_or(0);
    let now = now_iso();
    map.insert("version".into(), json!(version + 1));
    map.insert("updatedAt".into(), json!(now));
    if map.get("createdAt").is_none() || map["createdAt"].is_null() {
        map.insert("createdAt".into(), json!(now));
    }
    map.insert("updatedByDevice".into(), json!(device_id));
}

/// UPSERT of a camelCase JSON row into `table`. Columns are taken from the
/// table itself so unknown keys are ignored. Columns absent from the row keep
/// their stored values (old clients pushing a partial row can't wipe newer
/// columns); an explicit `null` clears a column.
pub fn write_row(conn: &Connection, table: &str, row: &Value) -> Result<(), Error> {
    let stmt = conn.prepare_cached(&format!("SELECT * FROM {table} LIMIT 0"))?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let map = row
        .as_object()
        .ok_or_else(|| Error::Invalid("row must be object".into()))?;
    if !map.contains_key("id") {
        return Err(Error::Invalid("row without id".into()));
    }
    let mut names = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    for col in &cols {
        let key = snake_to_camel(col);
        if let Some(v) = map.get(&key) {
            names.push(col.clone());
            params.push(json_to_sql(v));
        }
    }
    let placeholders = vec!["?"; names.len()].join(",");
    let updates = names
        .iter()
        .filter(|c| c.as_str() != "id")
        .map(|c| format!("{c}=excluded.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    let conflict = if updates.is_empty() {
        "DO NOTHING".to_string()
    } else {
        format!("DO UPDATE SET {updates}")
    };
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({placeholders}) ON CONFLICT(id) {conflict}",
        names.join(",")
    );
    conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(())
}

fn json_to_sql(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else {
                rusqlite::types::Value::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

/// Single write-path: stamps the row, writes it, logs audit, appends to
/// sync_log (host) or outbox (client). Must run inside a caller's transaction.
pub fn apply_change(
    ctx: &ApplyCtx,
    entity: Entity,
    op: Op,
    id: &str,
    mut payload: Option<Value>,
) -> Result<ApplyResult, Error> {
    let table = entity.table();
    let before = row_json(ctx.conn, table, id)?;
    let dirty = ctx.db.is_client();
    let prep = |mut row: Value, id: &str, extra: &[(&str, Value)]| -> Value {
        let map = row.as_object_mut().unwrap();
        map.insert("id".into(), json!(id));
        // Server-owned fields come from the stored row, never from the
        // payload — a stale client can't rewind version/seq/timestamps.
        for k in ["version", "seq", "updatedByDevice"] {
            match before
                .as_ref()
                .and_then(|b| b.get(k))
                .filter(|v| !v.is_null())
            {
                Some(v) => {
                    map.insert(k.into(), v.clone());
                }
                None => {
                    map.remove(k);
                }
            }
        }
        if let Some(v) = before
            .as_ref()
            .and_then(|b| b.get("createdAt"))
            .filter(|v| !v.is_null())
        {
            map.insert("createdAt".into(), v.clone());
        }
        for (k, v) in extra {
            map.insert((*k).to_string(), v.clone());
        }
        if dirty {
            map.insert("localDirty".into(), json!(1));
        }
        row
    };
    let action = match op {
        Op::Upsert => {
            let row = prep(
                payload
                    .take()
                    .unwrap_or_else(|| before.clone().unwrap_or(json!({}))),
                id,
                &[],
            );
            let mut row = row;
            stamp(&mut row, &ctx.db.device_id);
            write_row(ctx.conn, table, &row)?;
            if before.is_none() {
                "create"
            } else {
                "update"
            }
        }
        Op::Delete => {
            if entity.has_deleted_at() {
                let row = prep(
                    before.clone().unwrap_or(json!({})),
                    id,
                    &[("deletedAt", json!(now_iso()))],
                );
                let mut row = row;
                stamp(&mut row, &ctx.db.device_id);
                write_row(ctx.conn, table, &row)?;
            }
            "delete"
        }
        Op::Restore => {
            let row = prep(
                payload
                    .take()
                    .unwrap_or_else(|| before.clone().unwrap_or(json!({}))),
                id,
                &[("deletedAt", Value::Null)],
            );
            let mut row = row;
            stamp(&mut row, &ctx.db.device_id);
            write_row(ctx.conn, table, &row)?;
            "restore"
        }
    };

    let after = row_json(ctx.conn, table, id)?;
    // A delete on a non-existent row is a no-op; don't pollute the journal.
    if before.is_none() && after.is_none() {
        return Ok(ApplyResult {
            applied: false,
            before,
            after,
        });
    }
    if matches!(op, Op::Delete) && before.is_none() {
        return Ok(ApplyResult {
            applied: false,
            before,
            after,
        });
    }

    record_audit(ctx, entity, id, action, &before, &after, None)?;

    // Version that just got written.
    let version = after
        .as_ref()
        .and_then(|r| r["version"].as_i64())
        .unwrap_or(1);
    record_sync_side(ctx, entity, id, version, op)?;
    Ok(ApplyResult {
        applied: true,
        before,
        after,
    })
}

/// Writes an audit entry and, on the host, assigns it a seq for replication.
/// `extra_changes` overrides the auto-computed diff (e.g. merge_category).
pub fn record_audit(
    ctx: &ApplyCtx,
    entity: Entity,
    id: &str,
    action: &str,
    before: &Option<Value>,
    after: &Option<Value>,
    extra_changes: Option<Value>,
) -> Result<(), Error> {
    let changes = match extra_changes {
        Some(v) => Some(v.to_string()),
        None => match (before, after) {
            (Some(b), Some(a)) => Some(diff_json(b, a).to_string()),
            _ => None,
        },
    };
    let now = now_iso();
    let entry_id = new_id();
    // Audit rows are replicated too, so on the host they get their own seq
    // (separate from the entity change seq recorded in record_sync_side).
    let seq = if ctx.db.is_host() {
        ctx.conn
            .prepare_cached(
                "INSERT INTO sync_log (entity, entity_id, version, op, ts) VALUES (?1,?2,1,'audit',?3) RETURNING seq",
            )?
            .query_row(
                rusqlite::params![Entity::AuditLog.table(), entry_id, now],
                |r| r.get(0),
            )
            .unwrap_or(0)
    } else {
        0
    };
    ctx.conn.prepare_cached(
        "INSERT INTO audit_log (id, ts, user_id, device_id, source, entity, entity_id, action, before_json, after_json, changes_json, version, created_at, updated_at, updated_by_device, seq, local_dirty)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12,?12,?13,?14,?15)",
    )?
    .execute(
        rusqlite::params![
            entry_id,
            now,
            ctx.actor.user_id,
            ctx.actor.device_id.clone().or(Some(ctx.db.device_id.clone())),
            ctx.actor.source.as_str(),
            entity.table(),
            id,
            action,
            before.as_ref().map(|v| v.to_string()),
            after.as_ref().map(|v| v.to_string()),
            changes,
            now,
            ctx.db.device_id,
            seq,
            if ctx.db.is_client() { 1i64 } else { 0i64 },
        ],
    )?;
    Ok(())
}

fn record_sync_side(
    ctx: &ApplyCtx,
    entity: Entity,
    id: &str,
    version: i64,
    op: Op,
) -> Result<(), Error> {
    if ctx.db.is_host() {
        let op_s = match op {
            Op::Upsert => "upsert",
            Op::Delete => "delete",
            Op::Restore => "restore",
        };
        let seq: i64 = ctx
            .conn
            .prepare_cached(
                "INSERT INTO sync_log (entity, entity_id, version, op, ts) VALUES (?1,?2,?3,?4,?5) RETURNING seq",
            )?
            .query_row(
                rusqlite::params![entity.table(), id, version, op_s, now_iso()],
                |r| r.get(0),
            )?;
        ctx.conn
            .prepare_cached(&format!(
                "UPDATE {} SET seq = ?1 WHERE id = ?2",
                entity.table()
            ))?
            .execute(rusqlite::params![seq, id])?;
    } else {
        // Queue for the host. `payload` read back from the row for fidelity.
        let row = row_json(ctx.conn, entity.table(), id)?;
        let op_id = Sha256::digest(format!("{}:{}:{}", entity.table(), id, now_iso())).to_vec();
        let op_id = data_encoding::HEXLOWER.encode(&op_id[..12]);
        let op_s = match op {
            Op::Upsert => "upsert",
            Op::Delete => "delete",
            Op::Restore => "restore",
        };
        let base_version = row
            .as_ref()
            .and_then(|r| r["version"].as_i64())
            .map(|v| v - 1);
        ctx.conn.prepare_cached(
            "INSERT OR REPLACE INTO outbox (op_id, entity, entity_id, op, base_version, payload_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
        )?
        .execute(
            rusqlite::params![
                op_id,
                entity.table(),
                id,
                op_s,
                base_version,
                row.map(|r| r.to_string()),
                now_iso()
            ],
        )?;
    }
    Ok(())
}
