//! Storage layer for Finance: encrypted SQLite, migrations, repositories,
//! the audit log and (later) the host/client sync engine.

pub mod analytics;
pub mod backup;
pub mod bsb;
pub mod db;
mod error;
pub mod export;
pub mod migrations;
pub mod models;
pub mod nbrb;
pub mod repos;
pub mod secrets;

pub use error::Error;
pub use finance_core as core;
pub use models::{Actor, Entity, Op, Source};
pub use rusqlite;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use db::{new_id, now_iso};

fn apply_key(conn: &Connection, key: Option<&[u8]>) -> Result<(), Error> {
    if let Some(k) = key {
        conn.pragma_update(None, "key", hex(k))?;
    }
    Ok(())
}

impl db::Db {
    /// Opens (or creates) `path` as a SQLCipher database.
    /// `key` — the 32-byte DB key; `None` opens an unencrypted db (tests).
    /// Two connections share the file: `conn` for writes, `read_conn` (WAL) for
    /// reads so queries never queue behind a write transaction.
    pub fn open(path: &Path, key: Option<&[u8]>) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        apply_key(&conn, key)?;
        let stored = migrations::stored_version(&conn);
        if stored > 0 && stored < migrations::SCHEMA_VERSION {
            backup::pre_migration_snapshot(&conn, path.parent().unwrap_or(Path::new(".")), stored)?;
        }
        let read_conn = Connection::open(path)?;
        apply_key(&read_conn, key)?;
        Self::init_conn(
            conn,
            read_conn,
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        )
    }

    /// In-memory db for tests. A named shared-cache URI gives conn+read_conn
    /// a common database that is still private to this `Db` — `file::memory:`
    /// alone would leak state between `Db`s in the same test process.
    pub fn open_memory(key: Option<&[u8]>) -> Result<Self, Error> {
        let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
            | rusqlite::OpenFlags::SQLITE_OPEN_URI;
        let name = format!("file:memdb-{}?mode=memory&cache=shared", new_id());
        let conn = Connection::open_with_flags(&name, flags)?;
        apply_key(&conn, key)?;
        let read_conn = Connection::open_with_flags(&name, flags)?;
        apply_key(&read_conn, key)?;
        Self::init_conn(
            conn,
            read_conn,
            std::env::temp_dir().join("finance-test-attachments"),
        )
    }

    fn init_conn(
        conn: Connection,
        read_conn: Connection,
        attachments_dir: PathBuf,
    ) -> Result<Self, Error> {
        // Throws when the key is wrong or the file isn't a SQLite db.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        migrations::migrate(&conn)?;
        // Per-connection pragmas the migration batch can't set for us.
        read_conn.pragma_update(None, "busy_timeout", 5000)?;

        let meta = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
                .ok()
        };
        let set_meta = |key: &str, value: &str| {
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                [key, value],
            )
            .ok();
        };
        if meta("instance_id").is_none() {
            set_meta("instance_id", &new_id());
        }
        if meta("device_id").is_none() {
            set_meta("device_id", &new_id());
        }
        if meta("mode").is_none() {
            set_meta("mode", "host");
        }
        if meta("created_at").is_none() {
            set_meta("created_at", &now_iso());
        }
        let mode = db::Mode::parse(&meta("mode").unwrap_or_else(|| "host".into()));
        let device_id = meta("device_id").unwrap();
        let instance_id = meta("instance_id").unwrap();
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            read_conn: Arc::new(Mutex::new(read_conn)),
            mode,
            device_id,
            instance_id,
            attachments_dir: attachments_dir.join("attachments"),
        })
    }

    /// Serialized write access (mutations, journal, outbox).
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let conn = self.conn.lock().map_err(|e| Error::Other(e.to_string()))?;
        f(&conn)
    }

    /// Directory holding finance.db, attachments/ and backups/.
    pub fn data_dir(&self) -> PathBuf {
        self.attachments_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Read-only queries on the dedicated read connection. In WAL mode they
    /// run concurrently with the writer; never call this inside `with_conn`.
    pub fn with_read_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let conn = self
            .read_conn
            .lock()
            .map_err(|e| Error::Other(e.to_string()))?;
        f(&conn)
    }

    /// Serialized write inside one IMMEDIATE transaction: commits on Ok,
    /// rolls back on Err. Never open another transaction inside `f`.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut conn = self.conn.lock().map_err(|e| Error::Other(e.to_string()))?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn set_mode(&self, mode: db::Mode) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('mode', ?1)",
                [mode.as_str()],
            )?;
            Ok(())
        })
    }

    /// Provision a DB key for `instance_id`: read from the OS key store or
    /// generate + store a new one. Returns the key bytes and whether it was new.
    /// `db_exists` — the file was already there (then a missing stored key means
    /// "ask the user for the recovery key" instead of silently re-encrypting).
    pub fn provision_key(instance_id: &str, db_exists: bool) -> Result<KeyProvision, Error> {
        if let Some(k) = secrets::read_key(instance_id)? {
            return Ok(KeyProvision {
                key: k,
                generated: false,
                need_recovery_input: false,
            });
        }
        if db_exists {
            return Ok(KeyProvision {
                key: Vec::new(),
                generated: false,
                need_recovery_input: true,
            });
        }
        let k = secrets::generate_key();
        secrets::write_key(instance_id, &k)?;
        Ok(KeyProvision {
            key: k,
            generated: true,
            need_recovery_input: false,
        })
    }

    /// Called when the user typed a recovery key; stores it and returns bytes.
    pub fn adopt_recovery_key(instance_id: &str, text: &str) -> Result<Vec<u8>, Error> {
        let k = secrets::decode_recovery(text)?;
        if k.len() != 32 {
            return Err(Error::Invalid(
                "recovery key must decode to 32 bytes".into(),
            ));
        }
        secrets::write_key(instance_id, &k)?;
        Ok(k)
    }
}

pub struct KeyProvision {
    pub key: Vec<u8>,
    pub generated: bool,
    pub need_recovery_input: bool,
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------- seed data ----------

/// Seeds default categories (with i18n keys) on first run.
pub fn seed_categories(conn: &Connection, db: &db::Db, actor: &Actor) -> Result<(), Error> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    // (key, name_ru, kind, icon, color)
    let seeds: &[(&str, &str, &str, &str, &str)] = &[
        (
            "groceries",
            "Продукты",
            "expense",
            "shopping-cart",
            "#16a34a",
        ),
        (
            "eating_out",
            "Кафе и рестораны",
            "expense",
            "utensils",
            "#ea580c",
        ),
        ("transport", "Транспорт", "expense", "bus", "#2563eb"),
        ("auto", "Авто", "expense", "car", "#4b5563"),
        ("housing", "Жильё и ЖКХ", "expense", "house", "#7c3aed"),
        ("telecom", "Связь и интернет", "expense", "wifi", "#0891b2"),
        ("health", "Здоровье", "expense", "heart-pulse", "#dc2626"),
        ("clothes", "Одежда и обувь", "expense", "shirt", "#db2777"),
        ("home", "Дом и быт", "expense", "lamp", "#a16207"),
        ("kids", "Дети", "expense", "baby", "#f59e0b"),
        (
            "entertainment",
            "Развлечения",
            "expense",
            "popcorn",
            "#9333ea",
        ),
        ("gifts", "Подарки", "expense", "gift", "#e11d48"),
        ("subscriptions", "Подписки", "expense", "repeat", "#0d9488"),
        ("beauty", "Красота и уход", "expense", "sparkles", "#c026d3"),
        ("misc", "Прочее", "expense", "circle-help", "#6b7280"),
        ("salary", "Зарплата", "income", "banknote", "#16a34a"),
        ("side_job", "Подработка", "income", "briefcase", "#059669"),
        (
            "cashback",
            "Кэшбэк и проценты",
            "income",
            "percent",
            "#0891b2",
        ),
        ("gifts_in", "Подарки", "income", "gift", "#e11d48"),
        ("misc_in", "Прочее", "income", "circle-help", "#6b7280"),
    ];
    for (i, (key, name, kind, icon, color)) in seeds.iter().enumerate() {
        let is_system = if *key == "misc" || *key == "misc_in" {
            1
        } else {
            0
        };
        repos::category_upsert(
            conn,
            db,
            actor,
            serde_json::json!({
                "name": name, "key": key, "kind": kind, "icon": icon,
                "color": color, "isSystem": is_system, "sort": (i + 1) as i64
            }),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    // Key store and tests share the process — install once.
    static INIT: Mutex<()> = Mutex::new(());

    fn db() -> db::Db {
        let _g = INIT.lock().unwrap();
        secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
        db::Db::open_memory(Some(&[7u8; 32])).unwrap()
    }

    fn actor(db: &db::Db) -> Actor {
        Actor {
            user_id: None,
            device_id: Some(db.device_id.clone()),
            source: Source::Desktop,
        }
    }

    #[test]
    fn encrypted_file_requires_key() {
        let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
        let p = dir.join("f.db");
        let d = db::Db::open(&p, Some(&[1u8; 32])).unwrap();
        d.with_conn(|c| {
            c.execute("CREATE TABLE t (x)", []).map_err(Error::from)?;
            Ok(())
        })
        .unwrap();
        drop(d);
        // Wrong key fails to read.
        assert!(db::Db::open(&p, Some(&[2u8; 32])).is_err());
        // Right key works.
        assert!(db::Db::open(&p, Some(&[1u8; 32])).is_ok());
        // Even the header of the file is not a plain sqlite signature.
        let head = std::fs::read(&p).unwrap();
        assert_ne!(&head[..16], b"SQLite format 3\0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn audit_captures_field_diffs() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Visa","kind":"card","last4":"1234","currency":"BYN"}),
            )?;
            repos::account_upsert(c, &d, &a, json!({"id": acc["id"], "name":"Visa Platinum"}))?;
            let log = repos::journal(
                c,
                &repos::JournalFilter {
                    entity: Some("accounts".into()),
                    entity_id: Some(acc["id"].as_str().unwrap().into()),
                    ..Default::default()
                },
                "self-dev",
                None,
            )?;
            assert_eq!(log.len(), 2);
            let changes = log[0]["changesJson"].as_str().unwrap();
            assert!(changes.contains("Visa Platinum"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn balances_and_transfers() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let card = repos::account_upsert(c, &d, &a, json!({"name":"Карта","kind":"card","last4":"4321","currency":"BYN"}))?;
            let cash = repos::account_upsert(c, &d, &a, json!({"name":"Наличные","kind":"cash","currency":"BYN"}))?;
            let usd = repos::account_upsert(c, &d, &a, json!({"name":"Вклад","kind":"savings","currency":"USD"}))?;
            let cat: serde_json::Value = c
                .query_row("SELECT id FROM categories WHERE kind='income' AND key='salary'", [], |r| r.get::<_, String>(0))
                .map(|id| json!(id))?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"income","occurredAt":"2026-11-02T10:00:00","accountId":card["id"],"amountMinor":100000,"categoryId":cat}),
            )?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-02T11:00:00","accountId":card["id"],"amountMinor":3050,"categoryId":"".to_string()}),
            )
            .unwrap_err(); // category required
            let exp_cat: String = c.query_row("SELECT id FROM categories WHERE kind='expense' AND key='groceries'", [], |r| r.get(0))?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-02T11:00:00","accountId":card["id"],"amountMinor":3050,"categoryId":exp_cat}),
            )?;
            // transfer 300 BYN -> cash, and 330 BYN -> 100 USD savings (manual rate)
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"transfer","occurredAt":"2026-11-02T12:00:00","accountId":card["id"],"toAccountId":cash["id"],"amountMinor":30000}),
            )?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"transfer","occurredAt":"2026-11-02T12:00:00","accountId":card["id"],"toAccountId":usd["id"],"amountMinor":33000,"toAmountMinor":10000}),
            )?;
            // 50 BYN received on the USD account, credited as 15.00 USD by the bank.
            let foreign = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"income","occurredAt":"2026-11-02T13:00:00","accountId":usd["id"],"amountMinor":1500,"categoryId":cat,"origAmountMinor":5000,"origCurrency":"byn"}),
            )?;
            assert_eq!((foreign["origCurrency"].as_str(), foreign["origAmountMinor"].as_i64()), (Some("BYN"), Some(5000)));
            // Same currency as the account → no original amount is stored.
            let same = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-02T13:00:00","accountId":card["id"],"amountMinor":100,"categoryId":exp_cat,"origAmountMinor":100,"origCurrency":"BYN"}),
            )?;
            assert!(same["origCurrency"].is_null() && same["origAmountMinor"].is_null());
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-02T13:00:00","accountId":card["id"],"amountMinor":100,"categoryId":exp_cat,"origCurrency":"USD"}),
            )
            .unwrap_err();
            repos::apply(&d, c, &a, Entity::Transactions, models::Op::Delete, same["id"].as_str().unwrap(), None)?;
            let dash = repos::dashboard(c, "2026-11-01", "2026-11-30")?;
            let accs = dash["accounts"].as_array().unwrap();
            let bal = |name: &str| accs.iter().find(|a| a["name"] == name).unwrap()["balance"].as_i64().unwrap();
            // 1000.00 - 30.50 - 300 - 330 = 339.50
            assert_eq!(bal("Карта"), 33950);
            assert_eq!(bal("Наличные"), 30000);
            assert_eq!(bal("Вклад"), 11500);
            // conversion: 100 USD at 3.30 BYN/USD => 33000 BYN-minor? (rate 3.30 per unit)
            repos::rate_upsert(c, &d, "2026-11-02", "USD", 3_300_000, 1)?;
            assert_eq!(repos::convert_minor(c, 10000, "USD", "BYN")?, Some(33000));
            assert_eq!(repos::convert_minor(c, 33000, "BYN", "USD")?, Some(10000));
            // dated conversion picks the rate in effect on the operation day
            repos::rate_upsert(c, &d, "2026-10-01", "USD", 3_000_000, 1)?;
            repos::rate_upsert(c, &d, "2026-10-10", "RUB", 3_600_000, 100)?; // 3.60 BYN per 100 RUB
            assert_eq!(repos::convert_minor_at(c, 10000, "USD", "BYN", "2026-10-15")?, Some(30000));
            assert_eq!(repos::convert_minor_at(c, 10000, "USD", "BYN", "2026-11-03")?, Some(33000));
            // exact day, nearest earlier day, identity, RUB scale, no rate at all
            assert_eq!(repos::convert_minor_at(c, 10000, "USD", "BYN", "2026-11-02")?, Some(33000));
            assert_eq!(repos::convert_minor_at(c, 10000, "USD", "BYN", "2026-10-02")?, Some(30000));
            assert_eq!(repos::convert_minor_at(c, 1234, "USD", "USD", "2026-10-15")?, Some(1234));
            assert_eq!(repos::convert_minor_at(c, 10000, "RUB", "BYN", "2026-10-15")?, Some(360));
            assert_eq!(repos::convert_minor_at(c, 10000, "USD", "BYN", "2026-09-01")?, None);
            assert!(repos::has_rate_on(c, "USD", "2026-10-01")?);
            assert!(!repos::has_rate_on(c, "USD", "2026-10-02")?);
            assert!(repos::has_rate_on(c, "BYN", "2020-01-01")?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn category_merge_on_delete() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let acc = repos::account_upsert(c, &d, &a, json!({"name":"A","kind":"cash","currency":"BYN"}))?;
            let mine = repos::category_upsert(c, &d, &a, json!({"name":"Моя","kind":"expense"}))?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-02T10:00:00","accountId":acc["id"],"amountMinor":100,"categoryId":mine["id"]}),
            )?;
            repos::category_delete(c, &d, &a, mine["id"].as_str().unwrap(), None)?;
            let moved: String = c.query_row(
                "SELECT c.key FROM transactions t JOIN categories c ON c.id=t.category_id",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(moved, "misc");
            Ok(())
        })
        .unwrap();
    }

    /// An old-schema client pushes a partial row via /push (repos::apply with
    /// only the columns it knows). The upsert must keep columns it didn't send.
    #[test]
    fn old_client_push_keeps_new_columns() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Приорбанк","kind":"card","last4":"1234","bank":"Приорбанк","paymentSystem":"visa","currency":"BYN"}),
            )?;
            let goal = repos::goal_upsert(
                c,
                &d,
                &a,
                json!({"name":"Отпуск","targetMinor":100000,"accountId":acc["id"]}),
            )?;
            let v0 = acc["version"].as_i64().unwrap();

            // Old client: row minus the v3/v4 columns, with a renamed account.
            let mut old_acc = db::row_json(c, "accounts", acc["id"].as_str().unwrap())?.unwrap();
            let m = old_acc.as_object_mut().unwrap();
            m.remove("bank");
            m.remove("paymentSystem");
            m.insert("name".into(), json!("Приорбанк (старый клиент)"));
            repos::apply(&d, c, &a, Entity::Accounts, models::Op::Upsert, acc["id"].as_str().unwrap(), Some(old_acc))?;
            let row = db::row_json(c, "accounts", acc["id"].as_str().unwrap())?.unwrap();
            assert_eq!(row["name"], "Приорбанк (старый клиент)");
            assert_eq!(row["bank"], "Приорбанк");
            assert_eq!(row["paymentSystem"], "visa");
            assert!(row["version"].as_i64().unwrap() > v0);

            let mut old_goal = db::row_json(c, "goals", goal["id"].as_str().unwrap())?.unwrap();
            let m = old_goal.as_object_mut().unwrap();
            m.remove("accountId");
            m.insert("name".into(), json!("Отпуск в горах"));
            repos::apply(&d, c, &a, Entity::Goals, models::Op::Upsert, goal["id"].as_str().unwrap(), Some(old_goal))?;
            let row = db::row_json(c, "goals", goal["id"].as_str().unwrap())?.unwrap();
            assert_eq!(row["name"], "Отпуск в горах");
            assert_eq!(row["accountId"], acc["id"].as_str().unwrap());

            // An explicit null still clears a column.
            let mut clear = db::row_json(c, "accounts", acc["id"].as_str().unwrap())?.unwrap();
            clear.as_object_mut().unwrap().insert("bank".into(), serde_json::Value::Null);
            repos::apply(&d, c, &a, Entity::Accounts, models::Op::Upsert, acc["id"].as_str().unwrap(), Some(clear))?;
            let row = db::row_json(c, "accounts", acc["id"].as_str().unwrap())?.unwrap();
            assert!(row["bank"].is_null());
            Ok(())
        })
        .unwrap();
    }

    /// Backup → stage → apply roundtrip on an encrypted db in a temp dir.
    #[test]
    fn staged_restore_roundtrip() {
        let dir = std::env::temp_dir().join(format!("finbak-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join(backup::DB_FILE);
        let key = [3u8; 32];

        let d = db::Db::open(&db_path, Some(&key)).unwrap();
        d.with_conn(|c| {
            let a = actor(&d);
            repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"А","kind":"cash","currency":"BYN"}),
            )?;
            std::fs::create_dir_all(dir.join(backup::BACKUP_DIR))?;
            c.execute(
                "VACUUM INTO ?1",
                [dir.join(backup::BACKUP_DIR)
                    .join("backup-1.db")
                    .to_string_lossy()
                    .as_ref()],
            )?;
            Ok(())
        })
        .unwrap();

        // The backup is encrypted too — no plain SQLite header, opens with the key.
        let head = std::fs::read(dir.join(backup::BACKUP_DIR).join("backup-1.db")).unwrap();
        assert_ne!(&head[..16], b"SQLite format 3\0");
        assert!(db::Db::open(
            &dir.join(backup::BACKUP_DIR).join("backup-1.db"),
            Some(&key)
        )
        .is_ok());

        // Data B lands after the backup; then the live db is closed.
        d.with_conn(|c| {
            let a = actor(&d);
            repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Б","kind":"cash","currency":"BYN"}),
            )
        })
        .unwrap();
        drop(d);
        std::fs::write(dir.join("finance.db-wal"), b"garbage").unwrap();
        std::fs::write(dir.join("finance.db-shm"), b"garbage").unwrap();
        let live_before = std::fs::read(&db_path).unwrap();

        backup::stage_restore(&dir, "backup-1.db", &key).unwrap();
        assert!(dir.join(backup::RESTORE_FILE).exists());
        assert_eq!(std::fs::read(&db_path).unwrap(), live_before); // live file untouched

        let preserved = backup::apply_pending_restore(&dir).unwrap().unwrap();
        // The stale -shm was deleted; opening the restored db recreates fresh ones.
        assert!(!dir.join("finance.db-shm").exists());
        assert!(dir
            .join(backup::BACKUP_DIR)
            .join(format!(
                "{}-wal",
                preserved.file_name().unwrap().to_string_lossy()
            ))
            .exists());
        let d2 = db::Db::open(&db_path, Some(&key)).unwrap();
        let names: Vec<String> = d2
            .with_conn(|c| {
                let mut st = c.prepare("SELECT name FROM accounts ORDER BY name")?;
                let v = st
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .unwrap();
        assert_eq!(names, vec!["А".to_string()]);
        drop(d2);
        // The preserved copy opens with the key and still has B.
        let d3 = db::Db::open(&preserved, Some(&key)).unwrap();
        let has_b: bool = d3
            .with_conn(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM accounts WHERE name='Б'", [], |r| {
                        r.get::<_, i64>(0)
                    })? > 0,
                )
            })
            .unwrap();
        assert!(has_b);
        drop(d3);
        let listed = backup::list(&dir);
        assert!(listed.contains(&"backup-1.db".to_string()));
        assert!(listed.contains(
            &preserved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        ));
        assert!(listed
            .iter()
            .all(|n| n.ends_with(".db") && !n.contains("-wal") && !n.contains("-shm")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn staged_restore_rejects_bad_input() {
        let dir = std::env::temp_dir().join(format!("finbak-{}", new_id()));
        std::fs::create_dir_all(dir.join(backup::BACKUP_DIR)).unwrap();
        let key = [4u8; 32];
        for bad in [
            "",
            "../x.db",
            "a/b.db",
            "a\\b.db",
            "..",
            "noext",
            "missing.db",
        ] {
            assert!(backup::stage_restore(&dir, bad, &key).is_err(), "{bad}");
        }
        assert!(!dir.join(backup::RESTORE_FILE).exists());
        // Garbage file fails the probe/integrity check.
        std::fs::write(dir.join(backup::BACKUP_DIR).join("garbage.db"), b"not a db").unwrap();
        assert!(backup::stage_restore(&dir, "garbage.db", &key).is_err());
        assert!(!dir.join(backup::RESTORE_FILE).exists());
        // A valid db under a different key fails too.
        let d = db::Db::open(
            &dir.join(backup::BACKUP_DIR).join("other.db"),
            Some(&[9u8; 32]),
        )
        .unwrap();
        drop(d);
        assert!(backup::stage_restore(&dir, "other.db", &key).is_err());
        assert!(!dir.join(backup::RESTORE_FILE).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pending_restore_noop() {
        let dir = std::env::temp_dir().join(format!("finbak-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep.txt"), b"x").unwrap();
        assert_eq!(backup::apply_pending_restore(&dir).unwrap(), None);
        assert!(dir.join("keep.txt").exists());
        assert!(!dir.join(backup::DB_FILE).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A crashed previous run may leave a staged restore next to a leftover
    /// finance.db-wal with no finance.db. The wal must still be parked in
    /// backups/ — never replayed into the restored file.
    #[test]
    fn pending_restore_collects_leftover_wal() {
        let dir = std::env::temp_dir().join(format!("finbak-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = [5u8; 32];

        // A valid encrypted backup with data A.
        std::fs::create_dir_all(dir.join(backup::BACKUP_DIR)).unwrap();
        let d = db::Db::open(
            &dir.join(backup::BACKUP_DIR).join("backup-1.db"),
            Some(&key),
        )
        .unwrap();
        d.with_conn(|c| {
            let a = actor(&d);
            repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"А","kind":"cash","currency":"BYN"}),
            )?;
            c.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
            Ok(())
        })
        .unwrap();
        drop(d);

        backup::stage_restore(&dir, "backup-1.db", &key).unwrap();
        // Simulate the crash window: staged file present, db gone, wal left.
        std::fs::write(dir.join(format!("{}-wal", backup::DB_FILE)), b"stale wal").unwrap();

        assert_eq!(backup::apply_pending_restore(&dir).unwrap(), None);
        assert!(!dir.join(format!("{}-wal", backup::DB_FILE)).exists());
        let moved: Vec<_> = std::fs::read_dir(dir.join(backup::BACKUP_DIR))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with("-wal"))
            .collect();
        assert_eq!(moved.len(), 1, "expected one parked wal, got {moved:?}");

        let d2 = db::Db::open(&dir.join(backup::DB_FILE), Some(&key)).unwrap();
        let has_a: bool = d2
            .with_conn(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM accounts WHERE name='А'", [], |r| {
                        r.get::<_, i64>(0)
                    })? > 0,
                )
            })
            .unwrap();
        assert!(has_a);
        drop(d2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_create_and_prune() {
        let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
        let key = [5u8; 32];
        let d = db::Db::open(&dir.join("finance.db"), Some(&key)).unwrap();
        for _ in 0..3 {
            backup::create(&d, &dir).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        backup::prune(&dir, 2);
        let names = backup::list(&dir)
            .into_iter()
            .filter(|n| n.starts_with("backup-"))
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 2, "{names:?}");
        for n in &names {
            let p = dir.join(backup::BACKUP_DIR).join(n);
            assert!(db::Db::open(&p, Some(&key)).is_ok(), "{n}");
        }
        drop(d);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pre_migration_snapshot_is_encrypted() {
        let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
        let key = [6u8; 32];
        let d = db::Db::open(&dir.join("finance.db"), Some(&key)).unwrap();
        let snap = d
            .with_conn(|c| backup::pre_migration_snapshot(c, &dir, 5))
            .unwrap();
        drop(d);
        let head = std::fs::read(&snap).unwrap();
        assert_ne!(&head[..16], b"SQLite format 3\0");
        assert!(db::Db::open(&snap, Some(&key)).is_ok());
        assert!(backup::list(&dir)
            .iter()
            .any(|n| n.starts_with("pre-migrate-v5-")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
        let p = dir.join("f.db");
        let d = db::Db::open(&p, Some(&[3u8; 32])).unwrap();
        d.write(|c| {
            c.execute(
                "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
                [(migrations::SCHEMA_VERSION + 1).to_string()],
            )?;
            Ok(())
        })
        .unwrap();
        drop(d);
        let err = db::Db::open(&p, Some(&[3u8; 32]))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("db_newer_than_app"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_payload_version_cannot_rewind() {
        let d = db();
        d.write(|c| {
            let a = actor(&d);
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"A","kind":"cash","currency":"BYN"}),
            )?;
            let id = acc["id"].as_str().unwrap().to_string();
            for _ in 0..3 {
                repos::account_upsert(c, &d, &a, json!({"id": id, "name": "B"}))?;
            }
            let v_before = db::row_json(c, "accounts", &id)?.unwrap()["version"]
                .as_i64()
                .unwrap();
            repos::account_upsert(c, &d, &a, json!({"id": id, "name": "C", "version": 1}))?;
            let v_after = db::row_json(c, "accounts", &id)?.unwrap()["version"]
                .as_i64()
                .unwrap();
            assert_eq!(v_after, v_before + 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn write_rolls_back_on_error() {
        let d = db();
        let r: Result<(), Error> = d.write(|c| {
            repos::setting_set(c, "probe", "1")?;
            Err(Error::Invalid("boom".into()))
        });
        assert!(r.is_err());
        let v = d
            .with_read_conn(|c| repos::setting_get(c, "probe"))
            .unwrap();
        assert_eq!(v, None);
        d.write(|c| repos::setting_set(c, "probe", "2")).unwrap();
        let v = d
            .with_read_conn(|c| repos::setting_get(c, "probe"))
            .unwrap();
        assert_eq!(v.as_deref(), Some("2"));
    }

    #[test]
    fn recovery_roundtrip() {
        let k = secrets::generate_key();
        let enc = secrets::encode_recovery(&k);
        assert_eq!(secrets::decode_recovery(&enc).unwrap(), k);
        assert_eq!(
            secrets::decode_recovery(&enc.to_lowercase().replace('-', " ")).unwrap(),
            k
        );
    }

    /// v5 added transactions.spent_by_user_id; v6 extended it to every kind and
    /// backfills the income/transfer NULLs left by v5-era upserts.
    #[test]
    fn migration_spender_column_and_backfill() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            // Column exists at the migrated schema.
            let cols: Vec<String> = c
                .prepare("SELECT name FROM pragma_table_info('transactions')")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            assert!(cols.iter().any(|n| n == "spent_by_user_id"));
            const { assert!(migrations::SCHEMA_VERSION >= 6) }

            // A pre-v5 row is simulated by nulling the column; replaying the
            // migration's backfill statement must copy the author id over.
            let u = repos::user_upsert(c, &d, &a, json!({"name":"Иван"}))?;
            let acc =
                repos::account_upsert(c, &d, &a, json!({"name":"A","kind":"cash","currency":"BYN"}))?;
            let cat = repos::category_upsert(c, &d, &a, json!({"name":"K","kind":"expense"}))?;
            let uid = u["id"].as_str().unwrap();
            let tx = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-10T10:00:00","accountId":acc["id"],
                       "amountMinor":500,"categoryId":cat["id"],"createdByUserId":u["id"]}),
            )?;
            c.execute("UPDATE transactions SET spent_by_user_id = NULL", [])?;
            c.execute(
                "UPDATE transactions SET spent_by_user_id = created_by_user_id WHERE spent_by_user_id IS NULL",
                [],
            )?;
            let row = db::row_json(c, "transactions", tx["id"].as_str().unwrap())?.unwrap();
            assert_eq!(row["spentByUserId"].as_str(), Some(uid));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn account_rate_source_validated() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Cash","kind":"cash","currency":"BYN","rateSource":"bsb"}),
            )?;
            assert_eq!(acc["rateSource"].as_str(), Some("bsb"));
            let id = acc["id"].as_str().unwrap();
            let cleared = repos::account_upsert(c, &d, &a, json!({"id": id, "rateSource": ""}))?;
            assert!(cleared["rateSource"].is_null());
            let bad = repos::account_upsert(c, &d, &a, json!({"id": id, "rateSource": "alfa"}));
            assert!(matches!(bad, Err(Error::Invalid(_))));
            Ok(())
        })
        .unwrap();
    }

    fn snap(from: i64, next: Option<i64>, buy: i64) -> bsb::Snapshot {
        bsb::Snapshot {
            from_ms: from,
            next_ms: next,
            quotes: vec![(
                "USD".into(),
                finance_core::fx::Quote {
                    buy,
                    sell: buy + 20_000,
                },
            )],
        }
    }

    #[test]
    fn cache_current_snapshot_ttl() {
        use finance_core::fx::RateKind::BsbApp;
        let d = db();
        let now = 1_000_000_000;
        d.write(|c| repos::bank_rates_store(c, BsbApp, &snap(now - 60_000, None, 2_982_000), now))
            .unwrap();
        d.with_read_conn(|c| {
            let hit = repos::bank_rate_find(c, BsbApp, "usd", now + 1_000, now + 1_000)?;
            assert_eq!(hit.map(|q| q.buy), Some(2_982_000));
            let stale = repos::bank_rate_find(c, BsbApp, "USD", now + 360_000, now + 360_000)?;
            assert_eq!(stale, None);
            assert_eq!(
                repos::bank_rate_find(c, finance_core::fx::RateKind::BsbCard, "USD", now, now)?,
                None
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cache_historic_snapshot() {
        use finance_core::fx::RateKind::BsbCard;
        let d = db();
        d.write(|c| {
            repos::bank_rates_store(c, BsbCard, &snap(1000, Some(5000), 2_970_000), 9_999_999)
        })
        .unwrap();
        d.with_read_conn(|c| {
            let now = 99_999_999;
            assert!(repos::bank_rate_find(c, BsbCard, "USD", 4999, now)?.is_some());
            assert!(repos::bank_rate_find(c, BsbCard, "USD", 5000, now)?.is_none());
            assert!(repos::bank_rate_find(c, BsbCard, "USD", 999, now)?.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cache_upsert_closes_previous() {
        use finance_core::fx::RateKind::BsbApp;
        let d = db();
        d.write(|c| {
            repos::bank_rates_store(c, BsbApp, &snap(1000, None, 2_982_000), 2000)?;
            repos::bank_rates_store(c, BsbApp, &snap(1000, Some(8000), 2_982_000), 9000)
        })
        .unwrap();
        d.with_read_conn(|c| {
            let (n, to): (i64, Option<i64>) =
                c.query_row("SELECT COUNT(*), MAX(valid_to) FROM bank_rates", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!((n, to), (1, Some(8000)));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn nbrb_quote_per_unit() {
        let d = db();
        d.write(|c| repos::rate_upsert(c, &d, "2026-11-01", "RUB", 3_600_000, 100))
            .unwrap();
        d.with_read_conn(|c| {
            let q = repos::nbrb_quote_at(c, "RUB", "2026-11-05")?.unwrap();
            assert_eq!((q.buy, q.sell), (36_000, 36_000));
            assert!(repos::nbrb_quote_at(c, "RUB", "2026-10-01")?.is_none());
            Ok(())
        })
        .unwrap();
    }

    /// rate_source: kept on a converted entry, survives a partial row from an
    /// old client (no `rateSource` key), cleared by explicit null, validated,
    /// and dropped when the entry has no conversion.
    #[test]
    fn transaction_rate_source() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            let usd = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"U","kind":"card","currency":"USD"}),
            )?;
            let cat = repos::category_upsert(c, &d, &a, json!({"name":"K","kind":"expense"}))?;
            let tx = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-10T10:00:00","accountId":usd["id"],
                       "amountMinor":1673,"categoryId":cat["id"],"origAmountMinor":5000,
                       "origCurrency":"BYN","rateSource":"bsb_app"}),
            )?;
            let id = tx["id"].as_str().unwrap().to_string();
            assert_eq!(tx["rateSource"].as_str(), Some("bsb_app"));

            // Old client: full row without the new key → value kept on write_row.
            let mut old = db::row_json(c, "transactions", &id)?.unwrap();
            old.as_object_mut().unwrap().remove("rateSource");
            old["comment"] = json!("edited");
            db::write_row(c, "transactions", &old)?;
            let row = db::row_json(c, "transactions", &id)?.unwrap();
            assert_eq!(row["rateSource"].as_str(), Some("bsb_app"));
            assert_eq!(row["comment"].as_str(), Some("edited"));

            // Partial upsert without the key keeps it too.
            let up = repos::transaction_upsert(c, &d, &a, json!({"id": id, "comment": "x"}))?;
            assert_eq!(up["rateSource"].as_str(), Some("bsb_app"));

            // Explicit null clears.
            let mut cleared = db::row_json(c, "transactions", &id)?.unwrap();
            cleared["rateSource"] = serde_json::Value::Null;
            db::write_row(c, "transactions", &cleared)?;
            assert!(db::row_json(c, "transactions", &id)?.unwrap()["rateSource"].is_null());

            let bad = repos::transaction_upsert(c, &d, &a, json!({"id": id, "rateSource": "foo"}));
            assert!(matches!(bad, Err(Error::Invalid(_))));

            // Same-currency entry: no conversion → no rate source.
            let plain = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"expense","occurredAt":"2026-11-10T10:00:00","accountId":usd["id"],
                       "amountMinor":100,"categoryId":cat["id"],"rateSource":"nbrb"}),
            )?;
            assert!(plain["rateSource"].is_null());
            Ok(())
        })
        .unwrap();
    }

    /// Every kind carries "whose entry": default = author, explicit value is
    /// validated and kept for expenses, income and transfers alike.
    #[test]
    fn transaction_spender_defaults_and_validation() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let u1 = repos::user_upsert(c, &d, &a, json!({"name":"Иван"}))?;
            let u2 = repos::user_upsert(c, &d, &a, json!({"name":"Мария"}))?;
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"A","kind":"cash","currency":"BYN"}),
            )?;
            let cash = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"B","kind":"cash","currency":"BYN"}),
            )?;
            let cat = repos::category_upsert(c, &d, &a, json!({"name":"K","kind":"expense"}))?;
            let icat = repos::category_upsert(c, &d, &a, json!({"name":"I","kind":"income"}))?;
            let mk = |extra: serde_json::Value| {
                let mut p = json!({"kind":"expense","occurredAt":"2026-11-10T10:00:00",
                                   "accountId":acc["id"],"amountMinor":100,"categoryId":cat["id"]});
                p.as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
                repos::transaction_upsert(c, &d, &a, p)
            };

            // Not given → author.
            let x = mk(json!({"createdByUserId":u1["id"]}))?;
            assert_eq!(x["spentByUserId"], u1["id"]);
            // Explicit spender kept and exposed via the join.
            let x = mk(json!({"createdByUserId":u1["id"],"spentByUserId":u2["id"]}))?;
            assert_eq!(x["spentByUserId"], u2["id"]);
            let listed = repos::transaction_list(
                c,
                &repos::TransactionFilter {
                    from: Some("2026-11-10".into()),
                    to: Some("2026-11-10".into()),
                    ..Default::default()
                },
            )?;
            let row = listed.iter().find(|r| r["id"] == x["id"]).unwrap();
            assert_eq!(row["suName"], "Мария");
            assert_eq!(row["uName"], "Иван");
            // Unknown user rejected.
            assert!(mk(json!({"spentByUserId":"ghost"})).is_err());
            // Income and transfers keep the owner too.
            let inc = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"income","occurredAt":"2026-11-10T10:00:00","accountId":acc["id"],
                       "amountMinor":100,"categoryId":icat["id"],"spentByUserId":u2["id"]}),
            )?;
            assert_eq!(inc["spentByUserId"], u2["id"]);
            let tr = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"transfer","occurredAt":"2026-11-10T10:00:00","accountId":acc["id"],
                       "toAccountId":cash["id"],"amountMinor":100,"spentByUserId":u2["id"]}),
            )?;
            assert_eq!(tr["spentByUserId"], u2["id"]);
            // Not given → author for every kind.
            let inc2 = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"kind":"income","occurredAt":"2026-11-10T10:00:00","accountId":acc["id"],
                       "amountMinor":100,"categoryId":icat["id"],"createdByUserId":u1["id"]}),
            )?;
            assert_eq!(inc2["spentByUserId"], u1["id"]);
            // Editing keeps the saved spender when the field is not resent.
            let kept = repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({"id":x["id"],"kind":"expense","occurredAt":"2026-11-10T10:00:00",
                       "accountId":acc["id"],"amountMinor":200,"categoryId":cat["id"],
                       "comment":"upd"}),
            )?;
            assert_eq!(kept["spentByUserId"], x["spentByUserId"]);
            Ok(())
        })
        .unwrap();
    }

    /// category_ids filters by several categories; "" matches NULL rows.
    #[test]
    fn transaction_filter_category_ids() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let acc = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"A","kind":"cash","currency":"BYN"}),
            )?;
            let c1 = repos::category_upsert(c, &d, &a, json!({"name":"К1","kind":"expense"}))?;
            let c2 = repos::category_upsert(c, &d, &a, json!({"name":"К2","kind":"expense"}))?;
            let mk = |cat: &serde_json::Value| {
                repos::transaction_upsert(
                    c,
                    &d,
                    &a,
                    json!({"kind":"expense","occurredAt":"2026-11-10T10:00:00",
                           "accountId":acc["id"],"amountMinor":100,"categoryId":cat["id"]}),
                )
            };
            let t1 = mk(&c1)?;
            let t2 = mk(&c2)?;
            let t3 = mk(&c1)?;
            // Simulate a category-less row (e.g. after the category was dropped).
            c.execute(
                "UPDATE transactions SET category_id = NULL WHERE id = ?1",
                [t3["id"].as_str().unwrap()],
            )?;
            let list = |ids: &[&str]| -> Result<Vec<String>, Error> {
                Ok(repos::transaction_list(
                    c,
                    &repos::TransactionFilter {
                        category_ids: Some(ids.iter().map(|s| s.to_string()).collect()),
                        ..Default::default()
                    },
                )?
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_string())
                .collect())
            };
            let (i1, i2, i3) = (
                t1["id"].as_str().unwrap(),
                t2["id"].as_str().unwrap(),
                t3["id"].as_str().unwrap(),
            );
            let (c1id, c2id) = (c1["id"].as_str().unwrap(), c2["id"].as_str().unwrap());
            assert_eq!(list(&[c1id])?, vec![i1]);
            assert_eq!(list(&[c2id])?, vec![i2]);
            assert_eq!(list(&[""])?, vec![i3]);
            let mut multi = list(&[c1id, "", c2id])?;
            multi.sort();
            let mut all = vec![i1, i2, i3];
            all.sort();
            assert_eq!(multi, all);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn analytics_summary_converts_groups() {
        let d = db();
        d.with_conn(|c| {
            let a = actor(&d);
            seed_categories(c, &d, &a)?;
            let byn = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Карта","kind":"card","currency":"BYN"}),
            )?;
            let usd = repos::account_upsert(
                c,
                &d,
                &a,
                json!({"name":"Вклад","kind":"savings","currency":"USD"}),
            )?;
            repos::rate_upsert(c, &d, "2026-11-01", "USD", 3_000_000, 1)?;
            let exp_cat: String = c.query_row(
                "SELECT id FROM categories WHERE kind='expense' AND key='groceries'",
                [],
                |r| r.get(0),
            )?;
            let inc_cat: String = c.query_row(
                "SELECT id FROM categories WHERE kind='income' AND key='salary'",
                [],
                |r| r.get(0),
            )?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({
                "kind":"expense","occurredAt":"2026-11-05T10:00:00","accountId":byn["id"],
                "amountMinor":1000,"categoryId":exp_cat}),
            )?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({
                "kind":"expense","occurredAt":"2026-11-06T10:00:00","accountId":usd["id"],
                "amountMinor":100,"categoryId":exp_cat}),
            )?;
            repos::transaction_upsert(
                c,
                &d,
                &a,
                json!({
                "kind":"income","occurredAt":"2026-11-07T10:00:00","accountId":byn["id"],
                "amountMinor":500,"categoryId":inc_cat}),
            )?;

            let s = analytics::summary(c, "2026-11-01", "2026-11-30", "BYN")?;
            assert_eq!(s.expense, 1300); // 10.00 BYN + 1.00 USD at 3.00
            assert_eq!(s.income, 500);
            assert_eq!(s.by_category[0].expense, 1300);
            assert_eq!(s.by_day.len(), 3);
            assert!(s.missing.is_empty());

            // No PLN rate anywhere — nothing can be converted into it.
            let s = analytics::summary(c, "2026-11-01", "2026-11-30", "PLN")?;
            assert_eq!((s.income, s.expense), (0, 0));
            assert!(s.missing.contains(&"BYN".to_string()));

            let m = analytics::monthly(c, "2026-01-01", "2026-12-31", "BYN")?;
            assert_eq!(m.len(), 1);
            assert_eq!(m[0].key.as_deref(), Some("2026-11"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_rows_orders_every_entity() {
        // Regression: `ORDER BY sort, name` used to break attachments/reminders —
        // those tables have no such columns, so the query returned an error and
        // receipt photos never reached the UI.
        let d = db();
        d.with_conn(|c| {
            for e in Entity::all() {
                for &flag in &[false, true] {
                    repos::list_rows(c, *e, flag)
                        .unwrap_or_else(|err| panic!("list_rows({}, {flag}): {err}", e.table()));
                }
            }
            Ok(())
        })
        .unwrap();
    }

    /// Journal filtering: service rows (rates/system) hidden by default,
    /// host echoes of a client's own writes hidden via echo_of, is_self flag.
    #[test]
    fn journal_hides_service_and_echoes() -> Result<(), Error> {
        let d = db();
        d.with_conn(|c| {
            let ins = |id: &str, dev: &str, src: &str, entity: &str, seq: i64| {
                c.execute(
                    "INSERT INTO audit_log (id, ts, user_id, device_id, source, entity, entity_id, action, created_at, updated_at, seq, local_dirty)
                     VALUES (?1,'2026-01-01T00:00:00Z',NULL,?2,?3,?4,'e1','create','t','t',?5,0)",
                    rusqlite::params![id, dev, src, entity, seq],
                )
                .unwrap();
            };
            ins("own", "self-dev", "desktop", "accounts", 0); // my write, unpushed
            ins("own-echo", "self-dev", "desktop", "accounts", 10); // host echo of it
            ins("other", "other-dev", "desktop", "accounts", 10); // remote write
            ins("rate", "self-dev", "system", "exchange_rates", 0); // service
            Ok(())
        })?;
        let all = |echo: Option<&str>| {
            d.with_conn(|c| repos::journal(c, &repos::JournalFilter::default(), "self-dev", echo))
                .unwrap()
        };
        let ids: Vec<String> = all(Some("self-dev"))
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["other".to_string(), "own".to_string()]);
        // is_self marks the device that wrote the row.
        let own = all(None).iter().find(|r| r["id"] == "own").unwrap().clone();
        assert_eq!(own["isSelf"], 1);
        let other = all(None)
            .iter()
            .find(|r| r["id"] == "other")
            .unwrap()
            .clone();
        assert_eq!(other["isSelf"], 0);
        // Without echo filter the echo row appears; service rows stay hidden.
        let ids: Vec<String> = all(None)
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert!(!ids.contains(&"rate".to_string()));
        // hideSystem=false brings service rows back.
        let v = d
            .with_conn(|c| {
                repos::journal(
                    c,
                    &repos::JournalFilter {
                        hide_system: Some(false),
                        ..Default::default()
                    },
                    "self-dev",
                    None,
                )
            })
            .unwrap();
        assert!(v.iter().any(|r| r["id"] == "rate"));
        // origin matches device_id or source.
        let v = d
            .with_conn(|c| {
                repos::journal(
                    c,
                    &repos::JournalFilter {
                        hide_system: Some(false),
                        origin: Some("system".into()),
                        ..Default::default()
                    },
                    "self-dev",
                    None,
                )
            })
            .unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["id"], "rate");
        Ok(())
    }
}
