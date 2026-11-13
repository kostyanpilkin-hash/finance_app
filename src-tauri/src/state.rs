use finance_storage::{db::Db, repos, secrets, Actor, Source};
use std::sync::OnceLock;
use tauri::Manager;

/// Where to read the db key until we know instance_id (stored inside the db).
/// Chicken-and-egg: the instance id lives in `meta`, which is encrypted. So the
/// key is stored under a fixed account name per data dir instead.
fn install_key_store() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        secrets::install_key_store(
            |_instance| crate::keystore::read(crate::keystore::DB_KEY),
            |_instance, k| crate::keystore::write(crate::keystore::DB_KEY, k),
        );
    });
}

/// Managed when the db can't be opened at all — the UI shows the error screen
/// with backup-restore and data-dir actions instead of the app.
pub struct StartupError(pub String);

pub struct AppState {
    pub db: Db,
    /// True when the db file already existed (used by onboarding wording).
    pub db_existed: bool,
    /// The recovery-key text, kept only in memory for the wizard to show once.
    pub new_recovery_key: Option<String>,
}

impl AppState {
    /// Read-only query on the blocking pool.
    pub async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&finance_storage::rusqlite::Connection) -> Result<T, finance_storage::Error>
            + Send
            + 'static,
    ) -> Result<T, String> {
        let db = self.db.share();
        crate::blocking::run(move || db.with_read_conn(f).map_err(|e| e.to_string())).await
    }

    /// One transaction on the blocking pool, with the desktop actor resolved inside it.
    pub async fn write<T: Send + 'static>(
        &self,
        f: impl FnOnce(
                &finance_storage::rusqlite::Connection,
                &Db,
                &Actor,
            ) -> Result<T, finance_storage::Error>
            + Send
            + 'static,
    ) -> Result<T, String> {
        let db = self.db.share();
        crate::blocking::run(move || {
            db.write(|tx| {
                let c: &finance_storage::rusqlite::Connection = tx;
                let a = actor_on(c, &db)?;
                f(c, &db, &a)
            })
            .map_err(|e| e.to_string())
        })
        .await
    }
}

/// Current user id read on the write conn — used inside `with_conn`/`write`
/// closures where the write lock is already held.
pub(crate) fn actor_on(
    c: &finance_storage::rusqlite::Connection,
    db: &Db,
) -> Result<Actor, finance_storage::Error> {
    Ok(Actor {
        user_id: repos::setting_get(c, "current_user_id")?,
        device_id: Some(db.device_id.clone()),
        source: Source::Desktop,
    })
}

pub fn init_db(app: &tauri::App) -> Result<AppState, String> {
    install_key_store();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // A restore staged by backup_restore is swapped in before any connection.
    finance_storage::backup::apply_pending_restore(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("finance.db");
    let existed = path.exists();

    // Read meta without a key only when unencrypted (fresh) — provision first.
    let prov = Db::provision_key("primary", existed).map_err(|e| e.to_string())?;
    if prov.need_recovery_input {
        return Err("db_locked".into()); // UI will offer the recovery-key prompt later
    }
    let mut new_key = None;
    if prov.generated {
        new_key = Some(secrets::encode_recovery(&prov.key));
    }
    let db = Db::open(&path, Some(&prov.key)).map_err(|e| e.to_string())?;
    Ok(AppState {
        db,
        db_existed: existed,
        new_recovery_key: new_key,
    })
}

/// Verify a typed recovery key against the existing db and commit it to the
/// keyring on success. Called from the db_locked screen; the app relaunches after.
/// The key is stored only AFTER it proves it can open the db — otherwise a
/// wrong key would poison the keyring and lock the user out for good.
pub fn unlock_with_recovery(dir: &std::path::Path, recovery: &str) -> Result<(), String> {
    install_key_store();
    let key = secrets::decode_recovery(recovery).map_err(|e| e.to_string())?;
    if key.len() != 32 {
        return Err("recovery key must decode to 32 bytes".into());
    }
    Db::open(&dir.join("finance.db"), Some(&key)).map_err(|_| "wrong recovery key".to_string())?;
    crate::keystore::write(crate::keystore::DB_KEY, &key)
}
