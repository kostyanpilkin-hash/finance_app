//! Storage layer for Finance: encrypted SQLite, migrations, repositories,
//! the audit log and (later) the host/client sync engine.

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

impl db::Db {
    /// Opens (or creates) `path` as a SQLCipher database.
    /// `key` — the 32-byte DB key; `None` opens an unencrypted db (tests).
    pub fn open(path: &Path, key: Option<&[u8]>) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_conn(
            conn,
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            key,
        )
    }

    /// In-memory db for tests.
    pub fn open_memory(key: Option<&[u8]>) -> Result<Self, Error> {
        let conn = Connection::open_in_memory()?;
        Self::init_conn(
            conn,
            std::env::temp_dir().join("finance-test-attachments"),
            key,
        )
    }

    fn init_conn(
        conn: Connection,
        attachments_dir: PathBuf,
        key: Option<&[u8]>,
    ) -> Result<Self, Error> {
        if let Some(k) = key {
            conn.pragma_update(None, "key", hex(k))?;
        }
        // Throws when the key is wrong or the file isn't a SQLite db.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        migrations::migrate(&conn)?;

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
        let mode = meta("mode").unwrap_or_else(|| "host".into());
        let device_id = meta("device_id").unwrap();
        let instance_id = meta("instance_id").unwrap();
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            mode,
            device_id,
            instance_id,
            attachments_dir: attachments_dir.join("attachments"),
        })
    }

    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let conn = self.conn.lock().map_err(|e| Error::Other(e.to_string()))?;
        f(&conn)
    }

    pub fn set_mode(&self, mode: &str) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('mode', ?1)",
                [mode],
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

fn hex(b: &[u8]) -> String {
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
            let dash = repos::dashboard(c, "2026-11-01", "2026-11-30")?;
            let accs = dash["accounts"].as_array().unwrap();
            let bal = |name: &str| accs.iter().find(|a| a["name"] == name).unwrap()["balance"].as_i64().unwrap();
            // 1000.00 - 30.50 - 300 - 330 = 339.50
            assert_eq!(bal("Карта"), 33950);
            assert_eq!(bal("Наличные"), 30000);
            assert_eq!(bal("Вклад"), 10000);
            // conversion: 100 USD at 3.30 BYN/USD => 33000 BYN-minor? (rate 3.30 per unit)
            repos::rate_upsert(c, &d, "2026-11-02", "USD", 3_300_000, 1)?;
            assert_eq!(repos::convert_minor(c, 10000, "USD", "BYN")?, Some(33000));
            assert_eq!(repos::convert_minor(c, 33000, "BYN", "USD")?, Some(10000));
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
}
