//! Typed repositories on top of apply_change(). All writes go through `apply()`.

use finance_core::Currency;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Digest;

use crate::db::{apply_change, new_id, row_json, ApplyCtx};
use crate::error::Error;
use crate::models::{Entity, Op};

pub fn apply(
    db: &crate::db::Db,
    conn: &Connection,
    actor: &crate::models::Actor,
    entity: Entity,
    op: Op,
    id: &str,
    payload: Option<Value>,
) -> Result<crate::db::ApplyResult, Error> {
    apply_change(&ApplyCtx { conn, db, actor }, entity, op, id, payload)
}

pub fn id() -> String {
    new_id()
}

// ---------- generic helpers ----------

pub fn list_rows(
    conn: &Connection,
    entity: Entity,
    include_deleted: bool,
) -> Result<Vec<Value>, Error> {
    let table = entity.table();
    let order = entity.order_by();
    let sql = if include_deleted || !entity.has_deleted_at() {
        format!("SELECT * FROM {table} ORDER BY {order}")
    } else {
        format!("SELECT * FROM {table} WHERE deleted_at IS NULL ORDER BY {order}")
    };
    let mut stmt = conn.prepare_cached(&sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |r| Ok(crate::db::row_to_json(r, &cols)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn get_row(conn: &Connection, entity: Entity, id: &str) -> Result<Value, Error> {
    row_json(conn, entity.table(), id)?
        .ok_or_else(|| Error::NotFound(format!("{} {id}", entity.table())))
}

fn required<'a>(map: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a Value, Error> {
    map.get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| Error::Invalid(format!("missing field {key}")))
}

fn opt_str(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn req_str(map: &serde_json::Map<String, Value>, key: &str) -> Result<String, Error> {
    required(map, key)?
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Invalid(format!("field {key} must be a string")))
}

fn req_i64(map: &serde_json::Map<String, Value>, key: &str) -> Result<i64, Error> {
    required(map, key)?
        .as_i64()
        .ok_or_else(|| Error::Invalid(format!("field {key} must be an integer")))
}

fn set_default(map: &mut serde_json::Map<String, Value>, key: &str, v: Value) {
    map.entry(key.to_string()).or_insert(v);
}

// ---------- users ----------

pub fn user_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    mut input: Value,
) -> Result<Value, Error> {
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("user payload".into()))?;
    let name = req_str(map, "name")?;
    if name.trim().is_empty() {
        return Err(Error::Invalid("name is empty".into()));
    }
    let id = merge_existing(conn, Entity::Users, &mut input)?;
    let map = input.as_object_mut().unwrap();
    set_default(map, "color", json!("#059669"));
    set_default(map, "sort", json!(next_sort(conn, "users")?));
    apply(db, conn, actor, Entity::Users, Op::Upsert, &id, Some(input))?;
    get_row(conn, Entity::Users, &id)
}

fn next_sort(conn: &Connection, table: &str) -> Result<i64, Error> {
    Ok(conn
        .prepare_cached(&format!("SELECT COALESCE(MAX(sort),0)+1 FROM {table}"))?
        .query_row([], |r| r.get(0))
        .unwrap_or(0))
}

// ---------- accounts ----------

/// For updates, missing fields are taken from the existing row.
fn merge_existing(conn: &Connection, entity: Entity, input: &mut Value) -> Result<String, Error> {
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("payload".into()))?;
    let id = opt_str(map, "id").unwrap_or_else(new_id);
    if let Some(mut existing) = row_json(conn, entity.table(), &id)? {
        // input wins; existing fills the gaps.
        for (k, v) in map.iter() {
            existing
                .as_object_mut()
                .unwrap()
                .insert(k.clone(), v.clone());
        }
        *map = existing.as_object().unwrap().clone();
    }
    Ok(id)
}

pub fn account_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    mut input: Value,
) -> Result<Value, Error> {
    let id = merge_existing(conn, Entity::Accounts, &mut input)?;
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("account payload".into()))?;
    if req_str(map, "name")?.trim().is_empty() {
        return Err(Error::Invalid("name is empty".into()));
    }
    let currency = req_str(map, "currency")?;
    Currency::new(&currency).map_err(|e| Error::Invalid(e.to_string()))?;
    map.insert("currency".into(), json!(currency.to_uppercase()));
    let kind = opt_str(map, "kind").unwrap_or_else(|| "other".into());
    if !["card", "cash", "savings", "other"].contains(&kind.as_str()) {
        return Err(Error::Invalid(format!("bad account kind {kind}")));
    }
    if let Some(l4) = opt_str(map, "last4") {
        if !l4.is_empty() && (l4.len() != 4 || !l4.chars().all(|c| c.is_ascii_digit())) {
            return Err(Error::Invalid("last4 must be 4 digits".into()));
        }
        if l4.is_empty() {
            map.insert("last4".into(), Value::Null);
        }
    }
    if let Some(ps) = opt_str(map, "paymentSystem") {
        if !ps.is_empty()
            && !["visa", "mastercard", "belkart", "mir", "unionpay"].contains(&ps.as_str())
        {
            return Err(Error::Invalid(format!("bad payment system {ps}")));
        }
        if ps.is_empty() {
            map.insert("paymentSystem".into(), Value::Null);
        }
    }
    if let Some(rs) = opt_str(map, "rateSource") {
        if !rs.is_empty() && !["bsb", "nbrb"].contains(&rs.as_str()) {
            return Err(Error::Invalid(format!("bad rate source {rs}")));
        }
        if rs.is_empty() {
            map.insert("rateSource".into(), Value::Null);
        }
    }
    if let Some(b) = opt_str(map, "bank") {
        let b = b.trim();
        map.insert(
            "bank".into(),
            if b.is_empty() { Value::Null } else { json!(b) },
        );
    }
    if kind != "card" {
        map.insert("last4".into(), Value::Null);
        map.insert("bank".into(), Value::Null);
        map.insert("paymentSystem".into(), Value::Null);
    }
    set_default(map, "kind", json!(kind));
    set_default(map, "color", json!("#059669"));
    set_default(map, "includeInTotal", json!(1));
    set_default(map, "archived", json!(0));
    set_default(map, "sort", json!(next_sort(conn, "accounts")?));
    apply(
        db,
        conn,
        actor,
        Entity::Accounts,
        Op::Upsert,
        &id,
        Some(input),
    )?;
    get_row(conn, Entity::Accounts, &id)
}

// ---------- categories ----------

pub fn category_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    mut input: Value,
) -> Result<Value, Error> {
    let id = merge_existing(conn, Entity::Categories, &mut input)?;
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("category payload".into()))?;
    if req_str(map, "name")?.trim().is_empty() {
        return Err(Error::Invalid("name is empty".into()));
    }
    let kind = opt_str(map, "kind").unwrap_or_else(|| "expense".into());
    if !["expense", "income"].contains(&kind.as_str()) {
        return Err(Error::Invalid(format!("bad category kind {kind}")));
    }
    set_default(map, "kind", json!(kind));
    set_default(map, "icon", json!("tag"));
    set_default(map, "color", json!("#6b7280"));
    set_default(map, "archived", json!(0));
    set_default(map, "isSystem", json!(0));
    set_default(map, "sort", json!(next_sort(conn, "categories")?));
    apply(
        db,
        conn,
        actor,
        Entity::Categories,
        Op::Upsert,
        &id,
        Some(input),
    )?;
    get_row(conn, Entity::Categories, &id)
}

/// Delete a category moving its transactions to `move_to` ("misc" of the same kind if absent).
pub fn category_delete(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    id: &str,
    move_to: Option<&str>,
) -> Result<(), Error> {
    let cat = get_row(conn, Entity::Categories, id)?;
    if cat["isSystem"].as_i64() == Some(1) {
        return Err(Error::Invalid("system category cannot be deleted".into()));
    }
    let kind = cat["kind"].as_str().unwrap_or("expense").to_string();
    let target = match move_to {
        Some(t) => t.to_string(),
        None => misc_category_id(conn, db, actor, &kind)?,
    };
    if target == id {
        return Err(Error::Invalid("cannot move to itself".into()));
    }
    let count: i64 = conn
        .prepare_cached(
            "SELECT COUNT(*) FROM transactions WHERE category_id = ?1 AND deleted_at IS NULL",
        )?
        .query_row([id], |r| r.get(0))?;
    if count > 0 {
        // Re-apply each moved transaction so version/seq/outbox stay consistent,
        // plus one merge_category audit entry describing the bulk move.
        let mut st = conn.prepare_cached(
            "SELECT id FROM transactions WHERE category_id = ?1 AND deleted_at IS NULL",
        )?;
        let ids: Vec<String> = st
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(st);
        for tx_id in &ids {
            let mut row = row_json(conn, Entity::Transactions.table(), tx_id)?.unwrap_or_default();
            row.as_object_mut()
                .unwrap()
                .insert("categoryId".into(), json!(target));
            apply(
                db,
                conn,
                actor,
                Entity::Transactions,
                Op::Upsert,
                tx_id,
                Some(row),
            )?;
        }
        crate::db::record_audit(
            &ApplyCtx { conn, db, actor },
            Entity::Categories,
            id,
            "merge_category",
            &Some(cat.clone()),
            &None,
            Some(json!([{"field":"categoryId","movedCount":count,"before":id,"after":target}])),
        )?;
    }
    apply(db, conn, actor, Entity::Categories, Op::Delete, id, None)?;
    Ok(())
}

fn misc_category_id(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    kind: &str,
) -> Result<String, Error> {
    if let Ok(id) = conn.prepare_cached(
        "SELECT id FROM categories WHERE kind = ?1 AND key = 'misc' AND deleted_at IS NULL LIMIT 1",
    )?.query_row(
        [kind],
        |r| r.get::<_, String>(0),
    ) {
        return Ok(id);
    }
    let v = category_upsert(
        conn,
        db,
        actor,
        json!({"name": "Прочее", "key": "misc", "kind": kind, "icon": "circle-help", "color": "#6b7280", "isSystem": 1}),
    )?;
    Ok(v["id"].as_str().unwrap().to_string())
}

// ---------- transactions ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub id: Option<String>,
    pub kind: String,
    pub occurred_at: String,
    pub account_id: String,
    pub amount_minor: i64,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub to_account_id: Option<String>,
    #[serde(default)]
    pub to_amount_minor: Option<i64>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub created_by_user_id: Option<String>,
    /// Whose entry (every kind; falls back to the author).
    #[serde(default)]
    pub spent_by_user_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tg_message_id: Option<i64>,
    /// Amount as entered, when its currency differs from the account currency.
    #[serde(default)]
    pub orig_amount_minor: Option<i64>,
    #[serde(default)]
    pub orig_currency: Option<String>,
    /// Which rate filled the converted amount (`bsb_app|bsb_card|nbrb|manual`).
    #[serde(default)]
    pub rate_source: Option<String>,
}

pub const RATE_SOURCES: &[&str] = &["bsb_app", "bsb_card", "nbrb", "manual"];

pub fn transaction_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    input: Value,
) -> Result<Value, Error> {
    let mut row = input;
    let id = merge_existing(conn, Entity::Transactions, &mut row)?;
    let t: TransactionInput = serde_json::from_value(row.clone())
        .map_err(|e| Error::Invalid(format!("transaction: {e}")))?;
    if t.amount_minor <= 0 {
        return Err(Error::Invalid("amount must be > 0".into()));
    }
    if !["expense", "income", "transfer"].contains(&t.kind.as_str()) {
        return Err(Error::Invalid(format!("bad kind {}", t.kind)));
    }
    ensure_exists(conn, Entity::Accounts, &t.account_id, "account")?;
    match t.kind.as_str() {
        "expense" | "income" => {
            let cat = t
                .category_id
                .clone()
                .ok_or_else(|| Error::Invalid("category required".into()))?;
            ensure_exists(conn, Entity::Categories, &cat, "category")?;
            let ck: String = conn
                .prepare_cached("SELECT kind FROM categories WHERE id=?1")?
                .query_row([&cat], |r| r.get(0))?;
            if ck != t.kind {
                return Err(Error::Invalid(format!("category kind {ck} != {}", t.kind)));
            }
        }
        "transfer" => {
            let to = t
                .to_account_id
                .clone()
                .ok_or_else(|| Error::Invalid("toAccountId required".into()))?;
            if to == t.account_id {
                return Err(Error::Invalid("transfer to the same account".into()));
            }
            ensure_exists(conn, Entity::Accounts, &to, "to account")?;
        }
        _ => unreachable!(),
    }
    let account_currency: String = conn
        .prepare_cached("SELECT currency FROM accounts WHERE id=?1")?
        .query_row([&t.account_id], |r| r.get(0))?;
    let orig = match (t.orig_currency.as_deref(), t.orig_amount_minor) {
        (Some(cur), _) if t.kind == "transfer" || cur.eq_ignore_ascii_case(&account_currency) => {
            None
        }
        (None, _) => None,
        (Some(cur), Some(amount)) if amount > 0 => Some((
            Currency::new(cur).map_err(|e| Error::Invalid(e.to_string()))?,
            amount,
        )),
        (Some(_), _) => return Err(Error::Invalid("original amount must be > 0".into())),
    };
    let converted = orig.is_some() || (t.kind == "transfer" && t.to_amount_minor.is_some());
    let map = row.as_object_mut().unwrap();
    match orig {
        Some((cur, amount)) => {
            map.insert("origCurrency".into(), json!(cur.code()));
            map.insert("origAmountMinor".into(), json!(amount));
        }
        None => {
            map.insert("origCurrency".into(), Value::Null);
            map.insert("origAmountMinor".into(), Value::Null);
        }
    }
    // Only a converted amount has a rate source; an absent key is left alone
    // so a partial update keeps the stored value.
    if let Some(src) = t.rate_source.as_deref() {
        if !RATE_SOURCES.contains(&src) {
            return Err(Error::Invalid(format!("bad rate source {src}")));
        }
    }
    if !converted {
        map.insert("rateSource".into(), Value::Null);
    }
    set_default(map, "comment", json!(""));
    set_default(map, "source", json!("desktop"));
    set_default(
        map,
        "createdByUserId",
        actor.user_id.clone().map_or(Value::Null, |u| json!(u)),
    );
    // "Whose entry" — every kind; defaults to the author.
    let author = map.get("createdByUserId").cloned().unwrap_or(Value::Null);
    match t.spent_by_user_id.as_deref().filter(|s| !s.is_empty()) {
        Some(uid) => ensure_exists(conn, Entity::Users, uid, "user")?,
        None => {
            map.insert("spentByUserId".into(), author);
        }
    }
    if let Some(m) = map
        .get("source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if !["desktop", "bot", "reminder", "sync", "system"].contains(&m.as_str()) {
            map.insert("source".into(), json!("desktop"));
        }
    }
    apply(
        db,
        conn,
        actor,
        Entity::Transactions,
        Op::Upsert,
        &id,
        Some(row),
    )?;
    get_row(conn, Entity::Transactions, &id)
}

fn ensure_exists(conn: &Connection, entity: Entity, id: &str, what: &str) -> Result<(), Error> {
    let n: i64 = conn
        .prepare_cached(&format!(
            "SELECT COUNT(*) FROM {} WHERE id = ?1 AND deleted_at IS NULL",
            entity.table()
        ))?
        .query_row([id], |r| r.get(0))?;
    if n == 0 {
        return Err(Error::NotFound(format!("{what} {id}")));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TransactionFilter {
    pub from: Option<String>, // YYYY-MM-DD
    pub to: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    /// Multi-category filter (chart tail etc.); "" in the list matches NULL.
    pub category_ids: Option<Vec<String>>,
    pub kind: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub include_deleted: Option<bool>,
}

/// Transactions joined with names for the UI list.
pub fn transaction_list(conn: &Connection, f: &TransactionFilter) -> Result<Vec<Value>, Error> {
    let mut sql = String::from(
        "SELECT t.*, a.name AS a_name, a.currency AS a_currency, a.color AS a_color,
                c.name AS c_name, c.icon AS c_icon, c.color AS c_color, c.key AS c_key,
                u.name AS u_name, su.name AS su_name,
                a2.name AS a2_name, a2.currency AS a2_currency,
                (SELECT COUNT(*) FROM attachments at WHERE at.transaction_id = t.id AND at.deleted_at IS NULL) AS att_count
         FROM transactions t
         LEFT JOIN accounts a ON a.id = t.account_id
         LEFT JOIN accounts a2 ON a2.id = t.to_account_id
         LEFT JOIN categories c ON c.id = t.category_id
         LEFT JOIN users u ON u.id = t.created_by_user_id
         LEFT JOIN users su ON su.id = t.spent_by_user_id
         WHERE 1=1",
    );
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if f.include_deleted != Some(true) {
        sql.push_str(" AND t.deleted_at IS NULL");
    }
    let mut push = |v: rusqlite::types::Value| {
        params.push(v);
        format!("?{}", params.len())
    };
    if let Some(d) = &f.from {
        sql.push_str(&format!(
            " AND t.occurred_at >= {} || 'T00:00:00'",
            push(d.clone().into())
        ));
    }
    if let Some(d) = &f.to {
        sql.push_str(&format!(
            " AND t.occurred_at <= {} || 'T23:59:59.999'",
            push(d.clone().into())
        ));
    }
    if let Some(a) = &f.account_id {
        sql.push_str(&format!(
            " AND (t.account_id = {} OR t.to_account_id = {})",
            push(a.clone().into()),
            push(a.clone().into())
        ));
    }
    if let Some(c) = &f.category_id {
        sql.push_str(&format!(" AND t.category_id = {}", push(c.clone().into())));
    }
    if let Some(ids) = &f.category_ids {
        if !ids.is_empty() {
            let marks = ids
                .iter()
                .filter(|i| !i.is_empty())
                .map(|i| push(i.clone().into()))
                .collect::<Vec<_>>()
                .join(",");
            let mut cond = String::new();
            if !marks.is_empty() {
                cond.push_str(&format!("t.category_id IN ({marks})"));
            }
            if ids.iter().any(|i| i.is_empty()) {
                if !cond.is_empty() {
                    cond.push_str(" OR ");
                }
                cond.push_str("t.category_id IS NULL");
            }
            sql.push_str(&format!(" AND ({cond})"));
        }
    }
    if let Some(k) = &f.kind {
        sql.push_str(&format!(" AND t.kind = {}", push(k.clone().into())));
    }
    if let Some(s) = &f.search {
        if !s.trim().is_empty() {
            sql.push_str(&format!(
                " AND LOWER(t.comment) LIKE '%' || LOWER({}) || '%'",
                push(s.clone().into())
            ));
        }
    }
    sql.push_str(" ORDER BY t.occurred_at DESC, t.created_at DESC");
    if let Some(l) = f.limit {
        sql.push_str(&format!(" LIMIT {}", push(l.into())));
        if let Some(o) = f.offset {
            sql.push_str(&format!(" OFFSET {}", push(o.into())));
        }
    }
    let mut stmt = conn.prepare_cached(&sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok(crate::db::row_to_json(r, &cols))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Distinct days (YYYY-MM-DD) that have at least one non-deleted transaction in the range.
pub fn transaction_days(conn: &Connection, from: &str, to: &str) -> Result<Vec<String>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT substr(occurred_at, 1, 10) AS d
         FROM transactions
         WHERE deleted_at IS NULL
           AND occurred_at >= ?1 || 'T00:00:00'
           AND occurred_at <= ?2 || 'T23:59:59.999'
         ORDER BY d",
    )?;
    let days = stmt
        .query_map(rusqlite::params![from, to], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(days)
}

// ---------- attachments ----------

pub fn attachment_add(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    transaction_id: &str,
    file_name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<Value, Error> {
    ensure_exists(conn, Entity::Transactions, transaction_id, "transaction")?;
    let sha = hex_lower(&sha2::Sha256::digest(bytes));
    std::fs::create_dir_all(&db.attachments_dir)?;
    std::fs::write(db.attachments_dir.join(&sha), bytes)?;
    let id = new_id();
    let row = json!({
        "id": id,
        "transactionId": transaction_id,
        "sha256": sha,
        "fileName": file_name,
        "mime": mime,
        "size": bytes.len() as i64,
    });
    apply(
        db,
        conn,
        actor,
        Entity::Attachments,
        Op::Upsert,
        &id,
        Some(row),
    )?;
    get_row(conn, Entity::Attachments, &id)
}

fn hex_lower(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------- settings ----------

pub fn setting_get(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    Ok(conn
        .prepare_cached("SELECT value FROM settings WHERE key = ?1")?
        .query_row([key], |r| r.get(0))
        .ok())
}

pub fn setting_set(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.prepare_cached("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)")?
        .execute([key, value])?;
    Ok(())
}

pub fn settings_all(conn: &Connection) -> Result<Value, Error> {
    let mut stmt = conn.prepare_cached("SELECT key, value FROM settings")?;
    let map: serde_json::Map<String, Value> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    Ok(Value::Object(map))
}

// ---------- dashboard ----------

/// Balances per account (in account currency) + totals for a period, summed per currency.
pub fn dashboard(conn: &Connection, from: &str, to: &str) -> Result<Value, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT a.*, u.name AS owner_name,
                COALESCE((SELECT SUM(CASE WHEN t.kind='income' AND t.account_id = a.id THEN t.amount_minor
                                          WHEN t.kind='transfer' AND t.to_account_id = a.id THEN COALESCE(t.to_amount_minor, t.amount_minor)
                                          ELSE 0 END)
                          FROM transactions t WHERE t.deleted_at IS NULL
                            AND (t.account_id = a.id OR t.to_account_id = a.id)),0) -
                COALESCE((SELECT SUM(t.amount_minor)
                          FROM transactions t WHERE t.deleted_at IS NULL
                            AND t.account_id = a.id AND t.kind IN ('expense','transfer')),0) AS balance
         FROM accounts a
         LEFT JOIN users u ON u.id = a.owner_user_id
         WHERE a.deleted_at IS NULL
         ORDER BY a.sort, a.name",
    )?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let accounts = stmt
        .query_map([], |r| Ok(crate::db::row_to_json(r, &cols)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare_cached(
        "SELECT t.kind, a.currency, SUM(t.amount_minor) AS amount
         FROM transactions t JOIN accounts a ON a.id = t.account_id
         WHERE t.deleted_at IS NULL AND t.kind != 'transfer'
           AND t.occurred_at >= ?1 || 'T00:00:00' AND t.occurred_at <= ?2 || 'T23:59:59.999'
         GROUP BY t.kind, a.currency",
    )?;
    let cols2: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let totals = stmt
        .query_map(rusqlite::params![from, to], |r| {
            Ok(crate::db::row_to_json(r, &cols2))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json!({ "accounts": accounts, "totals": totals }))
}

// ---------- exchange rates ----------

pub fn rate_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    date: &str,
    currency: &str,
    rate_byn_scaled: i64,
    scale: i64,
) -> Result<(), Error> {
    let actor = crate::models::Actor {
        user_id: None,
        device_id: Some(db.device_id.clone()),
        source: crate::models::Source::System,
    };
    let row = json!({
        "date": date,
        "currency": currency.to_uppercase(),
        "rateBynScaled": rate_byn_scaled,
        "scale": scale,
    });
    let id = format!("{date}|{}", currency.to_uppercase());
    let mut row = row;
    row.as_object_mut()
        .unwrap()
        .insert("id".into(), json!(id.clone()));
    apply_change(
        &ApplyCtx {
            conn,
            db,
            actor: &actor,
        },
        Entity::ExchangeRates,
        Op::Upsert,
        &id,
        Some(row),
    )?;
    Ok(())
}

/// Latest rate per currency.
pub fn rates_latest(conn: &Connection) -> Result<Vec<Value>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.* FROM exchange_rates r
         JOIN (SELECT currency, MAX(date) AS d FROM exchange_rates GROUP BY currency) m
           ON m.currency = r.currency AND m.d = r.date",
    )?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let out = stmt
        .query_map([], |r| Ok(crate::db::row_to_json(r, &cols)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// (rate_byn_scaled, scale) for `currency`: the latest stored rate on or before
/// `date`, or the latest overall when `date` is None.
fn rate_row(
    conn: &Connection,
    currency: &str,
    date: Option<&str>,
) -> Result<Option<(i64, i64)>, Error> {
    if currency.eq_ignore_ascii_case("BYN") {
        return Ok(Some((1_000_000, 1)));
    }
    let row = match date {
        Some(d) => conn
            .prepare_cached(
                "SELECT rate_byn_scaled, scale FROM exchange_rates
                 WHERE currency = ?1 AND date <= ?2 ORDER BY date DESC LIMIT 1",
            )?
            .query_row(
                rusqlite::params![currency.to_uppercase(), d],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .ok(),
        None => conn
            .prepare_cached(
                "SELECT rate_byn_scaled, scale FROM exchange_rates WHERE currency = ?1 ORDER BY date DESC LIMIT 1",
            )?
            .query_row(
                [currency.to_uppercase()],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .ok(),
    };
    Ok(row)
}

fn convert_with(
    amount_minor: i64,
    from: Option<(i64, i64)>,
    to: Option<(i64, i64)>,
) -> Option<i64> {
    match (from, to) {
        (Some((f_rate, f_scale)), Some((t_rate, t_scale))) if f_rate > 0 && t_rate > 0 => {
            // amount * (f_rate/f_scale) * (t_scale/t_rate), rounded half away
            // from zero — kopecks, not truncated fractions.
            let num = (amount_minor as i128) * (f_rate as i128) * (t_scale as i128);
            let den = (f_scale as i128) * (t_rate as i128);
            let v = if num >= 0 {
                (num + den / 2) / den
            } else {
                (num - den / 2) / den
            };
            Some(v as i64)
        }
        _ => None,
    }
}

/// Convert `amount_minor` from `from` to `to` using latest rates (both per BYN, scaled 1e6).
/// Returns None when a rate is missing.
pub fn convert_minor(
    conn: &Connection,
    amount_minor: i64,
    from: &str,
    to: &str,
) -> Result<Option<i64>, Error> {
    if from.eq_ignore_ascii_case(to) {
        return Ok(Some(amount_minor));
    }
    let fr = rate_row(conn, from, None)?;
    let tr = rate_row(conn, to, None)?;
    Ok(convert_with(amount_minor, fr, tr))
}

/// Same as [`convert_minor`], but uses the rate in effect on `date` (YYYY-MM-DD):
/// the latest stored rate on or before that day.
pub fn convert_minor_at(
    conn: &Connection,
    amount_minor: i64,
    from: &str,
    to: &str,
    date: &str,
) -> Result<Option<i64>, Error> {
    if from.eq_ignore_ascii_case(to) {
        return Ok(Some(amount_minor));
    }
    let fr = rate_row(conn, from, Some(date))?;
    let tr = rate_row(conn, to, Some(date))?;
    Ok(convert_with(amount_minor, fr, tr))
}

/// True when a stored rate row exists for `currency` on `date`.
pub fn has_rate_on(conn: &Connection, currency: &str, date: &str) -> Result<bool, Error> {
    if currency.eq_ignore_ascii_case("BYN") {
        return Ok(true);
    }
    let n: i64 = conn
        .prepare_cached("SELECT COUNT(*) FROM exchange_rates WHERE currency = ?1 AND date = ?2")?
        .query_row(rusqlite::params![currency.to_uppercase(), date], |r| {
            r.get(0)
        })?;
    Ok(n > 0)
}

/// NBRB rate on `date` (latest stored on or before it) as a quote with buy == sell.
pub fn nbrb_quote_at(
    conn: &Connection,
    currency: &str,
    date: &str,
) -> Result<Option<finance_core::fx::Quote>, Error> {
    Ok(rate_row(conn, currency, Some(date))?
        .filter(|(r, s)| *r > 0 && *s > 0)
        .map(|(r, s)| {
            let v = r / s;
            finance_core::fx::Quote { buy: v, sell: v }
        }))
}

// ---------- bank quote cache (local, not replicated) ----------

/// How long a snapshot that is still current (`valid_to IS NULL`) is trusted.
pub const BANK_RATE_TTL_MS: i64 = 5 * 60 * 1000;

/// Cached quote for `at_ms`: a closed snapshot covering it, or the current
/// one fetched less than [`BANK_RATE_TTL_MS`] ago.
pub fn bank_rate_find(
    conn: &Connection,
    kind: finance_core::fx::RateKind,
    currency: &str,
    at_ms: i64,
    now_ms: i64,
) -> Result<Option<finance_core::fx::Quote>, Error> {
    let row = conn
        .prepare_cached(
            "SELECT valid_to, buy_scaled, sell_scaled, fetched_at FROM bank_rates
             WHERE kind = ?1 AND currency = ?2 AND valid_from <= ?3
             ORDER BY valid_from DESC LIMIT 1",
        )?
        .query_row(
            rusqlite::params![kind.as_str(), currency.to_uppercase(), at_ms],
            |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .ok();
    Ok(row.and_then(|(to, buy, sell, fetched)| {
        let fresh = match to {
            Some(to) => at_ms < to,
            None => now_ms - fetched < BANK_RATE_TTL_MS,
        };
        fresh.then_some(finance_core::fx::Quote { buy, sell })
    }))
}

/// Stores a fetched snapshot. Re-fetching the same snapshot later (e.g. once
/// it got a `next` time) updates it in place.
pub fn bank_rates_store(
    conn: &Connection,
    kind: finance_core::fx::RateKind,
    snap: &crate::bsb::Snapshot,
    now_ms: i64,
) -> Result<(), Error> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO bank_rates (kind, currency, valid_from, valid_to, buy_scaled, sell_scaled, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(kind, currency, valid_from) DO UPDATE SET
           valid_to = excluded.valid_to, buy_scaled = excluded.buy_scaled,
           sell_scaled = excluded.sell_scaled, fetched_at = excluded.fetched_at",
    )?;
    for (cur, q) in &snap.quotes {
        stmt.execute(rusqlite::params![
            kind.as_str(),
            cur.to_uppercase(),
            snap.from_ms,
            snap.next_ms,
            q.buy,
            q.sell,
            now_ms
        ])?;
    }
    Ok(())
}

// ---------- goals / reminders (basic CRUD for now) ----------

pub fn goal_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    mut input: Value,
) -> Result<Value, Error> {
    let id = merge_existing(conn, Entity::Goals, &mut input)?;
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("goal payload".into()))?;
    if req_str(map, "name")?.trim().is_empty() {
        return Err(Error::Invalid("name is empty".into()));
    }
    if req_i64(map, "targetMinor")? <= 0 {
        return Err(Error::Invalid("target must be > 0".into()));
    }
    set_default(map, "currency", json!("BYN"));
    set_default(map, "currentMinor", json!(0));
    set_default(map, "color", json!("#059669"));
    set_default(map, "icon", json!("target"));
    set_default(map, "archived", json!(0));
    apply(db, conn, actor, Entity::Goals, Op::Upsert, &id, Some(input))?;
    get_row(conn, Entity::Goals, &id)
}

pub fn reminder_upsert(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    mut input: Value,
) -> Result<Value, Error> {
    let id = merge_existing(conn, Entity::Reminders, &mut input)?;
    let map = input
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("reminder payload".into()))?;
    if req_str(map, "name")?.trim().is_empty() {
        return Err(Error::Invalid("name is empty".into()));
    }
    let rule = opt_str(map, "ruleType").unwrap_or_else(|| "monthly".into());
    if !["monthly", "weekly", "yearly"].contains(&rule.as_str()) {
        return Err(Error::Invalid(format!("bad rule {rule}")));
    }
    let kind = opt_str(map, "kind").unwrap_or_else(|| "expense".into());
    if !["expense", "income"].contains(&kind.as_str()) {
        return Err(Error::Invalid(format!("bad kind {kind}")));
    }
    if let Some(d) = opt_str(map, "ruleDay") {
        let d: i64 = d.parse().map_err(|_| Error::Invalid("ruleDay".into()))?;
        let max = match rule.as_str() {
            "monthly" => 31,
            "weekly" => 7,
            _ => 31,
        };
        if d < 1 || d > max {
            return Err(Error::Invalid("ruleDay out of range".into()));
        }
    }
    if let Some(a) = map.get("amountMinor").and_then(|v| v.as_i64()) {
        if a <= 0 {
            return Err(Error::Invalid("amount must be > 0".into()));
        }
    }
    set_default(map, "currency", json!("BYN"));
    set_default(map, "kind", json!(kind));
    set_default(map, "ruleType", json!(rule));
    set_default(map, "ruleDay", json!(1));
    set_default(map, "active", json!(1));
    if opt_str(map, "nextDue").is_none() {
        map.insert(
            "nextDue".into(),
            json!(next_due(
                &rule,
                req_i64(map, "ruleDay").unwrap_or(1),
                map.get("ruleMonth").and_then(|v| v.as_i64())
            )),
        );
    }
    apply(
        db,
        conn,
        actor,
        Entity::Reminders,
        Op::Upsert,
        &id,
        Some(input),
    )?;
    get_row(conn, Entity::Reminders, &id)
}

/// Compute next due date (local) for a rule.
pub fn next_due(rule_type: &str, rule_day: i64, rule_month: Option<i64>) -> String {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    match rule_type {
        "weekly" => {
            let target = (rule_day - 1).clamp(0, 6);
            let cur = today.weekday().num_days_from_monday() as i64;
            let delta = (target - cur).rem_euclid(7);
            (today + chrono::Duration::days(delta)).to_string()
        }
        "yearly" => {
            let m = rule_month.unwrap_or(1).clamp(1, 12) as u32;
            let d = rule_day.clamp(1, 28) as u32;
            let mut date = chrono::NaiveDate::from_ymd_opt(today.year(), m, d).unwrap();
            if date < today {
                date = chrono::NaiveDate::from_ymd_opt(today.year() + 1, m, d).unwrap();
            }
            date.to_string()
        }
        _ => {
            // monthly: rule_day-th of this or next month.
            let d = rule_day.clamp(1, 28) as u32;
            let mut date = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), d).unwrap();
            if date < today {
                let (y, m) = if today.month() == 12 {
                    (today.year() + 1, 1)
                } else {
                    (today.year(), today.month() + 1)
                };
                date = chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap();
            }
            date.to_string()
        }
    }
}

/// Reminders due by `today` (inclusive). Returns reminder rows.
pub fn reminders_due(conn: &Connection, today: &str) -> Result<Vec<Value>, Error> {
    let mut st = conn.prepare_cached(
        "SELECT * FROM reminders WHERE deleted_at IS NULL AND active = 1 AND next_due <= ?1",
    )?;
    let cols: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let v = st
        .query_map([today], |r| Ok(crate::db::row_to_json(r, &cols)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Marks the reminder done (optional tx id) or skipped; advances next_due.
pub fn reminder_resolve(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    id: &str,
    status: &str,
    transaction_id: Option<&str>,
) -> Result<(), Error> {
    let mut r = get_row(conn, Entity::Reminders, id)?;
    let rule = r["ruleType"].as_str().unwrap_or("monthly").to_string();
    let day = r["ruleDay"].as_i64().unwrap_or(1);
    let month = r["ruleMonth"].as_i64();
    let due = r["nextDue"].as_str().unwrap_or("").to_string();
    let next = next_due_after(&rule, day, month, &due);
    let map = r.as_object_mut().unwrap();
    map.insert("nextDue".into(), json!(next));
    apply(db, conn, actor, Entity::Reminders, Op::Upsert, id, Some(r))?;
    // Event record.
    let ev_id = new_id();
    apply(
        db,
        conn,
        actor,
        Entity::ReminderEvents,
        Op::Upsert,
        &ev_id,
        Some(json!({
            "id": ev_id, "reminderId": id, "dueDate": due, "status": status, "transactionId": transaction_id
        })),
    )?;
    Ok(())
}

fn next_due_after(rule_type: &str, _rule_day: i64, _rule_month: Option<i64>, due: &str) -> String {
    use chrono::Datelike;
    let d = chrono::NaiveDate::parse_from_str(&due[..due.len().min(10)], "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::Local::now().date_naive());
    match rule_type {
        "weekly" => (d + chrono::Duration::days(7)).to_string(),
        "yearly" => chrono::NaiveDate::from_ymd_opt(d.year() + 1, d.month(), d.day())
            .map(|x| x.to_string())
            .unwrap_or_else(|| d.to_string()),
        _ => {
            let (y, m) = if d.month() == 12 {
                (d.year() + 1, 1)
            } else {
                (d.year(), d.month() + 1)
            };
            chrono::NaiveDate::from_ymd_opt(y, m, d.day())
                .unwrap_or(d)
                .to_string()
        }
    }
}

// ---------- journal ----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalFilter {
    pub entity: Option<String>,
    pub entity_id: Option<String>,
    pub user_id: Option<String>,
    pub source: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Service rows (exchange rates, shared settings, reminder/system
    /// bookkeeping) are hidden unless this is `Some(false)`.
    pub hide_system: Option<bool>,
    /// Combined device/source filter: matches `l.device_id` or `l.source`.
    pub origin: Option<String>,
}

/// `self_id` flags rows made by this device (`is_self`). `echo_of` hides
/// pulled host copies of a client's own writes — the local original keeps
/// `seq = 0` while the host echo carries `seq > 0` for the same device, so
/// `device_id = self AND seq > 0` only ever matches echoes on a client.
pub fn journal(
    conn: &Connection,
    f: &JournalFilter,
    self_id: &str,
    echo_of: Option<&str>,
) -> Result<Vec<Value>, Error> {
    let mut sql = String::from(
        "SELECT l.*, u.name AS u_name, d.name AS d_name, (l.device_id = ?SELFDEV) AS is_self
         FROM audit_log l
         LEFT JOIN users u ON u.id = l.user_id
         LEFT JOIN devices d ON d.id = l.device_id
         WHERE 1=1",
    );
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    let mut push = |v: rusqlite::types::Value| {
        params.push(v);
        format!("?{}", params.len())
    };
    if let Some(e) = &f.entity {
        sql.push_str(&format!(" AND l.entity = {}", push(e.clone().into())));
    }
    if let Some(e) = &f.entity_id {
        sql.push_str(&format!(" AND l.entity_id = {}", push(e.clone().into())));
    }
    if let Some(u) = &f.user_id {
        sql.push_str(&format!(" AND l.user_id = {}", push(u.clone().into())));
    }
    if let Some(s) = &f.source {
        sql.push_str(&format!(" AND l.source = {}", push(s.clone().into())));
    }
    if let Some(a) = &f.action {
        sql.push_str(&format!(" AND l.action = {}", push(a.clone().into())));
    }
    if let Some(d) = &f.from {
        sql.push_str(&format!(" AND l.ts >= {}", push(d.clone().into())));
    }
    if let Some(d) = &f.to {
        sql.push_str(&format!(" AND l.ts <= {}", push(d.clone().into())));
    }
    if let Some(s) = &f.search {
        if !s.trim().is_empty() {
            sql.push_str(&format!(" AND (LOWER(l.before_json) LIKE '%'||LOWER({})||'%' OR LOWER(l.after_json) LIKE '%'||LOWER({})||'%' OR LOWER(l.changes_json) LIKE '%'||LOWER({})||'%')",
                push(s.clone().into()), push(s.clone().into()), push(s.clone().into())));
        }
    }
    if f.hide_system != Some(false) {
        sql.push_str(
            " AND l.entity NOT IN ('exchange_rates','shared_settings','reminder_events','audit_log','devices')
             AND l.source <> 'system'",
        );
    }
    if let Some(o) = &f.origin {
        sql.push_str(&format!(" AND (l.device_id = {0} OR l.source = {0})", push(o.clone().into())));
    }
    if let Some(e) = echo_of {
        sql.push_str(&format!(" AND NOT (l.device_id = {} AND l.seq > 0)", push(e.to_string().into())));
    }
    sql.push_str(" ORDER BY l.rowid DESC");
    sql.push_str(&format!(" LIMIT {}", push(f.limit.unwrap_or(200).into())));
    if let Some(o) = f.offset {
        sql.push_str(&format!(" OFFSET {}", push(o.into())));
    }
    sql = sql.replace("?SELFDEV", &push(self_id.to_string().into()));
    let mut stmt = conn.prepare_cached(&sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let out = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok(crate::db::row_to_json(r, &cols))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Restore a soft-deleted entity row to the snapshot stored in the audit entry
/// (or just clear deleted_at when no snapshot is found).
pub fn restore(
    conn: &Connection,
    db: &crate::db::Db,
    actor: &crate::models::Actor,
    entity_s: &str,
    id: &str,
) -> Result<(), Error> {
    let entity =
        Entity::parse(entity_s).ok_or_else(|| Error::Invalid(format!("entity {entity_s}")))?;
    if !entity.has_deleted_at() {
        return Err(Error::Invalid("entity is not restorable".into()));
    }
    apply(db, conn, actor, entity, Op::Restore, id, None)?;
    Ok(())
}
