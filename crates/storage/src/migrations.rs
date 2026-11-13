//! Versioned schema. Each migration is a list of statements applied in one transaction.

use rusqlite::Connection;

use crate::error::Error;

const SYNC_FIELDS: &str = "
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by_device TEXT,
    deleted_at TEXT,
    seq INTEGER NOT NULL DEFAULT 0,
    local_dirty INTEGER NOT NULL DEFAULT 0";

/// Highest schema version this binary can create/upgrade to.
/// Sent in sync handshakes so a client pushing rows written under an older
/// schema is rejected instead of silently losing fields.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

const MIGRATIONS: &[&[&str]] = &[
    // ---- v1 ----
    &[
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        "CREATE TABLE users (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            color TEXT NOT NULL DEFAULT '#059669',
            sort INTEGER NOT NULL DEFAULT 0,
            SYNC_FIELDS
        )",
        // Devices known to the host (client pairing). Stage 5.
        "CREATE TABLE devices (
            id TEXT PRIMARY KEY,
            user_id TEXT REFERENCES users(id),
            name TEXT NOT NULL DEFAULT '',
            platform TEXT NOT NULL DEFAULT '',
            token_hash TEXT NOT NULL DEFAULT '',
            cert_fp TEXT NOT NULL DEFAULT '',
            paired_at TEXT,
            last_seen_at TEXT,
            revoked_at TEXT
        )",
        "CREATE TABLE accounts (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'other' CHECK (kind IN ('card','cash','savings','other')),
            last4 TEXT CHECK (last4 IS NULL OR (length(last4) = 4 AND last4 GLOB '[0-9][0-9][0-9][0-9]')),
            currency TEXT NOT NULL CHECK (length(currency) = 3),
            color TEXT NOT NULL DEFAULT '#059669',
            owner_user_id TEXT,
            include_in_total INTEGER NOT NULL DEFAULT 1,
            archived INTEGER NOT NULL DEFAULT 0,
            sort INTEGER NOT NULL DEFAULT 0,
            SYNC_FIELDS
        )",
        "CREATE TABLE categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            key TEXT,
            kind TEXT NOT NULL CHECK (kind IN ('expense','income')),
            icon TEXT NOT NULL DEFAULT 'tag',
            color TEXT NOT NULL DEFAULT '#6b7280',
            archived INTEGER NOT NULL DEFAULT 0,
            is_system INTEGER NOT NULL DEFAULT 0,
            sort INTEGER NOT NULL DEFAULT 0,
            SYNC_FIELDS
        )",
        "CREATE TABLE transactions (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL CHECK (kind IN ('expense','income','transfer')),
            occurred_at TEXT NOT NULL,
            account_id TEXT NOT NULL,
            amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
            category_id TEXT,
            to_account_id TEXT,
            to_amount_minor INTEGER CHECK (to_amount_minor IS NULL OR to_amount_minor > 0),
            comment TEXT NOT NULL DEFAULT '',
            created_by_user_id TEXT,
            source TEXT NOT NULL DEFAULT 'desktop',
            device_id TEXT,
            tg_message_id INTEGER,
            SYNC_FIELDS
        )",
        "CREATE INDEX idx_transactions_occurred ON transactions(occurred_at)",
        "CREATE INDEX idx_transactions_account ON transactions(account_id)",
        "CREATE INDEX idx_transactions_to_account ON transactions(to_account_id)",
        "CREATE INDEX idx_transactions_category ON transactions(category_id)",
        "CREATE INDEX idx_transactions_deleted ON transactions(deleted_at)",
        "CREATE INDEX idx_transactions_seq ON transactions(seq)",
        "CREATE TABLE attachments (
            id TEXT PRIMARY KEY,
            transaction_id TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            file_name TEXT NOT NULL DEFAULT '',
            mime TEXT NOT NULL DEFAULT '',
            size INTEGER NOT NULL DEFAULT 0,
            SYNC_FIELDS
        )",
        "CREATE INDEX idx_attachments_tx ON attachments(transaction_id)",
        "CREATE TABLE goals (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            target_minor INTEGER NOT NULL,
            current_minor INTEGER NOT NULL DEFAULT 0,
            currency TEXT NOT NULL DEFAULT 'BYN',
            deadline TEXT,
            color TEXT NOT NULL DEFAULT '#059669',
            icon TEXT NOT NULL DEFAULT 'target',
            archived INTEGER NOT NULL DEFAULT 0,
            SYNC_FIELDS
        )",
        "CREATE TABLE reminders (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            amount_minor INTEGER,
            currency TEXT NOT NULL DEFAULT 'BYN',
            account_id TEXT,
            category_id TEXT,
            kind TEXT NOT NULL DEFAULT 'expense' CHECK (kind IN ('expense','income')),
            rule_type TEXT NOT NULL CHECK (rule_type IN ('monthly','weekly','yearly')),
            rule_day INTEGER NOT NULL DEFAULT 1,
            rule_month INTEGER,
            next_due TEXT,
            active INTEGER NOT NULL DEFAULT 1,
            SYNC_FIELDS
        )",
        "CREATE TABLE reminder_events (
            id TEXT PRIMARY KEY,
            reminder_id TEXT NOT NULL,
            due_date TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','done','skipped','snoozed')),
            transaction_id TEXT,
            SYNC_FIELDS
        )",
        // `id` = "<date>|<CURRENCY>" — lets apply_change() treat it like any row.
        "CREATE TABLE exchange_rates (
            id TEXT PRIMARY KEY,
            date TEXT NOT NULL,
            currency TEXT NOT NULL,
            rate_byn_scaled INTEGER NOT NULL,
            scale INTEGER NOT NULL DEFAULT 1,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by_device TEXT,
            seq INTEGER NOT NULL DEFAULT 0,
            local_dirty INTEGER NOT NULL DEFAULT 0,
            UNIQUE (date, currency)
        )",
        // Append-only audit. `changes_json` — [{field,before,after}]; before/after hold full snapshots.
        "CREATE TABLE audit_log (
            id TEXT PRIMARY KEY,
            ts TEXT NOT NULL,
            user_id TEXT,
            device_id TEXT,
            source TEXT NOT NULL,
            entity TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            action TEXT NOT NULL CHECK (action IN ('create','update','delete','restore','conflict_resolved','merge_category')),
            before_json TEXT,
            after_json TEXT,
            changes_json TEXT,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by_device TEXT,
            seq INTEGER NOT NULL DEFAULT 0,
            local_dirty INTEGER NOT NULL DEFAULT 0
        )",
        "CREATE INDEX idx_audit_entity ON audit_log(entity, entity_id)",
        "CREATE INDEX idx_audit_ts ON audit_log(ts)",
        "CREATE INDEX idx_audit_seq ON audit_log(seq)",
        // Host: append-only feed of every change for pull. Stage 5.
        "CREATE TABLE sync_log (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            entity TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            op TEXT NOT NULL,
            ts TEXT NOT NULL
        )",
        "CREATE INDEX idx_sync_log_entity ON sync_log(entity, entity_id)",
        // Client: pending outbound ops.
        "CREATE TABLE outbox (
            op_id TEXT PRIMARY KEY,
            entity TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            op TEXT NOT NULL,
            base_version INTEGER,
            payload_json TEXT,
            created_at TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0
        )",
        // Client: version conflicts waiting for user resolution.
        "CREATE TABLE conflicts (
            id TEXT PRIMARY KEY,
            entity TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            local_payload TEXT NOT NULL,
            server_payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            resolved_at TEXT
        )",
        // Telegram integration (host). Stage 4.
        "CREATE TABLE telegram_links (
            user_id TEXT NOT NULL,
            chat_id INTEGER NOT NULL UNIQUE,
            linked_at TEXT NOT NULL
        )",
        "CREATE TABLE tg_sessions (
            chat_id INTEGER PRIMARY KEY,
            state_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE tg_pair_codes (
            code TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            expires_at TEXT NOT NULL
        )",
        "CREATE TABLE device_pair_codes (
            code TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            expires_at TEXT NOT NULL
        )",
        // Per-device preferences (not replicated).
        "CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        // Shared preferences (replicated).
        "CREATE TABLE shared_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by_device TEXT,
            seq INTEGER NOT NULL DEFAULT 0,
            local_dirty INTEGER NOT NULL DEFAULT 0
        )",
        // seq indexes for replicated tables.
        "CREATE INDEX idx_users_seq ON users(seq)",
        "CREATE INDEX idx_accounts_seq ON accounts(seq)",
        "CREATE INDEX idx_categories_seq ON categories(seq)",
        "CREATE INDEX idx_attachments_seq ON attachments(seq)",
        "CREATE INDEX idx_goals_seq ON goals(seq)",
        "CREATE INDEX idx_reminders_seq ON reminders(seq)",
        "CREATE INDEX idx_reminder_events_seq ON reminder_events(seq)",
        "CREATE INDEX idx_rates_seq ON exchange_rates(seq)",
        "CREATE INDEX idx_shared_seq ON shared_settings(seq)",
    ],
    // ---- v2: income/expense entered in a currency other than the account's ----
    // amount_minor stays in the account currency (balances); orig_* keep what was entered.
    &[
        "ALTER TABLE transactions ADD COLUMN orig_amount_minor INTEGER CHECK (orig_amount_minor IS NULL OR orig_amount_minor > 0)",
        "ALTER TABLE transactions ADD COLUMN orig_currency TEXT CHECK (orig_currency IS NULL OR length(orig_currency) = 3)",
    ],
    // ---- v3: card details for the bank-card look ----
    &[
        "ALTER TABLE accounts ADD COLUMN bank TEXT",
        "ALTER TABLE accounts ADD COLUMN payment_system TEXT CHECK (payment_system IS NULL OR payment_system IN ('visa','mastercard','belkart','mir','unionpay'))",
    ],
    // ---- v4: a goal can be linked to an account; progress = account balance ----
    &["ALTER TABLE goals ADD COLUMN account_id TEXT"],
    // ---- v5: who the expense belongs to (≠ author); backfill from the author ----
    &[
        "ALTER TABLE transactions ADD COLUMN spent_by_user_id TEXT",
        "UPDATE transactions SET spent_by_user_id = created_by_user_id WHERE spent_by_user_id IS NULL",
    ],
    // ---- v6: the field now applies to every kind — backfill the income/transfer
    // rows v5-era code left NULL; bumps SCHEMA_VERSION so old clients must update ----
    &["UPDATE transactions SET spent_by_user_id = created_by_user_id WHERE spent_by_user_id IS NULL"],
];

/// Schema version stored in `meta`, 0 for a fresh or unreadable db.
pub fn stored_version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Applies pending migrations inside a transaction.
pub fn migrate(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;
    let current = stored_version(conn);
    if current > SCHEMA_VERSION {
        return Err(Error::Invalid(format!(
            "db_newer_than_app: db schema v{current}, app supports v{SCHEMA_VERSION}"
        )));
    }
    for (i, stmts) in MIGRATIONS.iter().enumerate() {
        let v = (i + 1) as i64;
        if v <= current {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        for s in stmts.iter() {
            let sql = s.replace("SYNC_FIELDS", SYNC_FIELDS);
            tx.execute_batch(&sql)
                .map_err(|e| Error::Other(format!("migration v{v} failed on `{s}`: {e}")))?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [v.to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}
