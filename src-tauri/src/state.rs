use finance_storage::{db::Db, secrets, Actor, Source};
use std::sync::OnceLock;
use tauri::Manager;

const KEY_SERVICE: &str = "by.kostyan.finance";

fn keyring_read(instance: &str) -> Result<Option<Vec<u8>>, String> {
    let entry =
        keyring::Entry::new(KEY_SERVICE, &format!("db-{instance}")).map_err(|e| e.to_string())?;
    match entry.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn keyring_write(instance: &str, key: &[u8]) -> Result<(), String> {
    let entry =
        keyring::Entry::new(KEY_SERVICE, &format!("db-{instance}")).map_err(|e| e.to_string())?;
    entry.set_secret(key).map_err(|e| e.to_string())
}

/// Where to read the db key until we know instance_id (stored inside the db).
/// Chicken-and-egg: the instance id lives in `meta`, which is encrypted. So the
/// key is stored under a fixed account name per data dir instead.
fn install_key_store() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        secrets::install_key_store(
            |_instance| keyring_read("primary"),
            |_instance, k| keyring_write("primary", k),
        );
    });
}

pub struct AppState {
    pub db: Db,
    /// True when the db file already existed (used by onboarding wording).
    pub db_existed: bool,
    /// The recovery-key text, kept only in memory for the wizard to show once.
    pub new_recovery_key: Option<String>,
}

impl AppState {
    #[allow(dead_code)]
    pub fn actor(&self, user_id: Option<String>) -> Actor {
        Actor {
            user_id,
            device_id: Some(self.db.device_id.clone()),
            source: Source::Desktop,
        }
    }
}

pub fn init_db(app: &tauri::App) -> Result<AppState, String> {
    install_key_store();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
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

/// Second-chance open after the user typed their recovery key.
#[allow(dead_code)]
pub fn open_with_recovery(dir: &std::path::Path, recovery: &str) -> Result<AppState, String> {
    install_key_store();
    let key = Db::adopt_recovery_key("primary", recovery).map_err(|e| e.to_string())?;
    let db = Db::open(&dir.join("finance.db"), Some(&key)).map_err(|e| e.to_string())?;
    Ok(AppState {
        db,
        db_existed: true,
        new_recovery_key: None,
    })
}
