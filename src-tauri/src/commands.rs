use finance_storage::{db::Db, models, repos, seed_categories, Actor, Entity, Source};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

#[tauri::command]
pub fn app_info(app: AppHandle) -> serde_json::Value {
    json!({
        "version": app.package_info().version.to_string(),
        "apiVersion": finance_api::API_VERSION,
        "platform": std::env::consts::OS,
    })
}

#[tauri::command]
pub fn set_tray_labels(
    app: AppHandle,
    open: String,
    quit: String,
    tooltip: String,
) -> Result<(), String> {
    let tray = app
        .tray_by_id(crate::tray::TRAY_ID)
        .ok_or("tray not found")?;
    let menu = crate::tray::build_menu(&app, &open, &quit).map_err(|e| e.to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())
}

/// Runs SQLite/disk work on the blocking pool: the command future resolves on
/// the async runtime while a worker thread does the blocking part, so a slow
/// query can never freeze the window.
async fn run<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Current user id read on the write conn — used inside `with_conn` closures
/// where the write lock is already held.
fn actor_on(
    c: &finance_storage::rusqlite::Connection,
    db: &Db,
) -> Result<Actor, finance_storage::Error> {
    Ok(Actor {
        user_id: repos::setting_get(c, "current_user_id")?,
        device_id: Some(db.device_id.clone()),
        source: Source::Desktop,
    })
}

// ---------- onboarding / bootstrap ----------

#[tauri::command]
pub async fn onboarding_status(state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.db.share();
    let db_existed = state.db_existed;
    run(move || {
        db.with_read_conn(|c| {
            let done = repos::setting_get(c, "onboarding_done")? == Some("1".into());
            let users = repos::list_rows(c, Entity::Users, false)?.len();
            Ok(json!({
                "done": done,
                "users": users,
                "dbExisted": db_existed,
                "mode": db.mode,
            }))
        })
        .map_err(|e| e.to_string())
    })
    .await
}

/// Shows the freshly generated recovery key once (fresh installs only).
#[tauri::command]
pub fn recovery_key(state: State<AppState>) -> Option<String> {
    state.new_recovery_key.clone()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingInput {
    pub user_names: Vec<String>,
    pub current_user: usize,
    pub starter_accounts: Vec<String>, // kinds: cash|savings
}

#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    input: OnboardingInput,
) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let actor = actor_on(c, &db)?;
            seed_categories(c, &db, &actor)?;
            let mut ids = Vec::new();
            for (i, name) in input.user_names.iter().enumerate() {
                let u =
                    repos::user_upsert(c, &db, &actor, json!({"name": name, "sort": i as i64}))?;
                ids.push(u["id"].as_str().unwrap().to_string());
            }
            let owner = ids
                .get(input.current_user)
                .cloned()
                .or_else(|| ids.first().cloned());
            for kind in &input.starter_accounts {
                let name = if kind == "cash" {
                    "Наличные"
                } else {
                    "Сбережения"
                };
                repos::account_upsert(
                    c,
                    &db,
                    &actor,
                    json!({
                        "name": name, "kind": kind, "currency": "BYN", "ownerUserId": owner
                    }),
                )?;
            }
            repos::setting_set(c, "onboarding_done", "1")?;
            repos::setting_set(c, "current_user_id", owner.as_deref().unwrap_or(""))?;
            c.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('mode', 'host')",
                [],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_current_user(state: State<'_, AppState>, user_id: String) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| repos::setting_set(c, "current_user_id", &user_id))
            .map_err(|e| e.to_string())
    })
    .await
}

// ---------- settings ----------

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.with_read_conn(repos::settings_all)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| repos::setting_set(c, &key, &value))
            .map_err(|e| e.to_string())
    })
    .await
}

// ---------- generic entity CRUD ----------

#[tauri::command]
pub async fn entity_list(
    state: State<'_, AppState>,
    entity: String,
    include_deleted: Option<bool>,
) -> Result<Vec<Value>, String> {
    let e = Entity::parse(&entity).ok_or("unknown entity")?;
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::list_rows(c, e, include_deleted.unwrap_or(false)))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn entity_delete(
    state: State<'_, AppState>,
    entity: String,
    id: String,
) -> Result<(), String> {
    let e = Entity::parse(&entity).ok_or("unknown entity")?;
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::apply(&db, c, &a, e, models::Op::Delete, &id, None)?;
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn entity_restore(
    state: State<'_, AppState>,
    entity: String,
    id: String,
) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::restore(c, &db, &a, &entity, &id)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn user_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::user_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn account_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::account_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn category_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::category_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn category_delete(
    state: State<'_, AppState>,
    id: String,
    move_to: Option<String>,
) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::category_delete(c, &db, &a, &id, move_to.as_deref())
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn transaction_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::transaction_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn transaction_list(
    state: State<'_, AppState>,
    filter: Value,
) -> Result<Vec<Value>, String> {
    let f: repos::TransactionFilter = serde_json::from_value(filter).map_err(|e| e.to_string())?;
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::transaction_list(c, &f))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn transaction_days(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<Vec<String>, String> {
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::transaction_days(c, &from, &to))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn dashboard(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::dashboard(c, &from, &to))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn journal(state: State<'_, AppState>, filter: Value) -> Result<Vec<Value>, String> {
    let f: repos::JournalFilter = serde_json::from_value(filter).map_err(|e| e.to_string())?;
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::journal(c, &f))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn goal_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::goal_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn reminder_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            repos::reminder_upsert(c, &db, &a, input)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn reminder_resolve(
    state: State<'_, AppState>,
    id: String,
    status: String,
    create_tx: Option<Value>,
) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let a = actor_on(c, &db)?;
            let mut tx_id = None;
            if let Some(v) = create_tx {
                let row = repos::transaction_upsert(c, &db, &a, v)?;
                tx_id = row["id"].as_str().map(|s| s.to_string());
            }
            repos::reminder_resolve(c, &db, &a, &id, &status, tx_id.as_deref())?;
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
    .await
}

// ---------- security & backups ----------

#[tauri::command]
pub async fn pin_set(state: State<'_, AppState>, pin: String) -> Result<(), String> {
    use sha2::Digest;
    let salt = uuid::Uuid::new_v4().to_string();
    let hash = sha2::Sha256::digest(format!("{salt}{pin}").as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            repos::setting_set(c, "pin_salt", &salt)?;
            repos::setting_set(c, "pin_hash", &hex)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn pin_verify(state: State<'_, AppState>, pin: String) -> Result<bool, String> {
    use sha2::Digest;
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| {
            let salt = repos::setting_get(c, "pin_salt")?.unwrap_or_default();
            let expect = repos::setting_get(c, "pin_hash")?;
            match expect {
                None => Ok(true),
                Some(h) => {
                    let got = sha2::Sha256::digest(format!("{salt}{pin}").as_bytes());
                    Ok(got.iter().map(|b| format!("{b:02x}")).collect::<String>() == h)
                }
            }
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn pin_clear(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            repos::setting_set(c, "pin_salt", "")?;
            repos::setting_set(c, "pin_hash", "")
        })
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn pin_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.share();
    run(move || {
        Ok(db
            .with_read_conn(|c| repos::setting_get(c, "pin_hash"))
            .ok()
            .flatten()
            .map(|h| !h.is_empty())
            .unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn backup_create(state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.share();
    run(move || {
        finance_storage::backup::create(&db, &db.data_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn backup_list(app: AppHandle) -> Vec<String> {
    app.path()
        .app_data_dir()
        .map(|dir| finance_storage::backup::list(&dir))
        .unwrap_or_default()
}

/// Validates the backup and stages it as finance.db.restore; the swap happens
/// at the next startup, so the live db is never written over. Works on the
/// startup-error screen too (no AppState needed).
#[tauri::command]
pub async fn backup_restore(app: AppHandle, name: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let key = finance_storage::secrets::read_key("primary")
        .map_err(|e| e.to_string())?
        .ok_or("database key unavailable")?;
    tauri::async_runtime::spawn_blocking(move || {
        finance_storage::backup::stage_restore(&dir, &name, &key).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reminders due by today (local time — the user plans payments by their clock).
#[tauri::command]
pub async fn reminders_due(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let today = chrono::Local::now().date_naive().to_string();
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::reminders_due(c, &today))
            .map_err(|e| e.to_string())
    })
    .await
}

// ---------- attachments ----------

/// Receipts are capped: they go over IPC raw, get hashed and written to disk.
const MAX_ATTACHMENT_BYTES: usize = 15 * 1024 * 1024;

/// Decodes `encodeURIComponent` output — header values are ASCII-only, so the
/// file name travels percent-encoded.
fn unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The photo goes as a raw `Uint8Array` body — metadata rides in headers.
#[tauri::command]
pub async fn attachment_add(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<Value, String> {
    let headers = request.headers();
    let header = |k: &str| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    let transaction_id = header("x-transaction-id").ok_or("missing transaction id")?;
    let file_name = header("x-file-name")
        .and_then(|n| unescape(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "receipt".into());
    let mime = header("x-mime")
        .filter(|m| !m.is_empty() && m.len() <= 100)
        .unwrap_or_else(|| "image/jpeg".into());
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw photo bytes".into());
    };
    if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err("attachment size out of range".into());
    }
    let bytes = bytes.clone();
    let db = state.db.share();
    run(move || {
        db.write(|c| {
            let actor = actor_on(c, &db)?;
            repos::attachment_add(c, &db, &actor, &transaction_id, &file_name, &mime, &bytes)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

/// Attachment file names are sha256 hex digests — reject anything else so a
/// crafted name can't wander the filesystem under the asset protocol.
pub(crate) fn valid_attachment_hash(sha256: &str) -> bool {
    sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit())
}

#[tauri::command]
pub async fn attachment_path(state: State<'_, AppState>, sha256: String) -> Result<String, String> {
    if !valid_attachment_hash(&sha256) {
        return Err("invalid attachment id".into());
    }
    let p = state.db.attachments_dir.join(&sha256);
    if p.exists() {
        Ok(p.to_string_lossy().into_owned())
    } else {
        Err("attachment missing".into())
    }
}

// ---------- exchange rates ----------

#[tauri::command]
pub async fn export_xlsx(
    state: State<'_, AppState>,
    app: AppHandle,
    from: String,
    to: String,
) -> Result<String, String> {
    let dir = app
        .path()
        .document_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Финансы");
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| finance_storage::export::export_xlsx(c, &dir, &from, &to))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn rates_latest(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let db = state.db.share();
    run(move || {
        db.with_read_conn(repos::rates_latest)
            .map_err(|e| e.to_string())
    })
    .await
}

// ---------- telegram bot ----------

#[tauri::command]
pub async fn tg_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<Value, String> {
    let (username, bot_id) = crate::bot::tg_connect(&state, token).await?;
    crate::bot::start_polling(app);
    Ok(json!({"username": username, "botId": bot_id}))
}

#[tauri::command]
pub async fn tg_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.share();
    run(move || crate::bot::tg_disconnect(&db)).await
}

#[tauri::command]
pub async fn tg_status(state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.db.share();
    run(move || Ok(crate::bot::tg_status(&db))).await
}

#[tauri::command]
pub async fn tg_pair_code(state: State<'_, AppState>, user_id: String) -> Result<String, String> {
    let db = state.db.share();
    run(move || crate::bot::tg_pair_code(&db, &user_id)).await
}

// ---------- LAN sync ----------

#[tauri::command]
pub async fn device_pair_code<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    user_id: String,
) -> Result<String, String> {
    let db = state.db.share();
    run(move || crate::sync::pair_code(&app, &db, &user_id)).await
}

/// The pairing_requests payload for a given hub (None = client mode,
/// locked/error screens). Split out so it can be tested without an AppHandle.
pub(crate) fn pairing_report(hub: Option<&finance_api::PairingHub>) -> Value {
    match hub {
        Some(hub) => {
            let pending = hub.pending();
            json!({
                "requests": pending,
                "locked": hub.locked(),
                "failures": hub.failures(),
            })
        }
        None => json!({"requests": [], "locked": false, "failures": 0}),
    }
}

/// Pending pairing requests for the host UI.
#[tauri::command]
pub fn pairing_requests<R: tauri::Runtime>(app: AppHandle<R>) -> Value {
    pairing_report(crate::sync::hub(&app).as_deref())
}

#[tauri::command]
pub async fn pairing_decide<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request_id: String,
    allow: bool,
) -> Result<(), String> {
    let hub = crate::sync::hub(&app).ok_or("pairing unavailable")?;
    let db = state.db.share();
    run(move || {
        hub.decide(&db, &request_id, allow)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn pair_to_host(
    state: State<'_, AppState>,
    host_url: String,
    code: String,
    name: String,
) -> Result<(), String> {
    crate::sync::pair_client(&state, &host_url, &code, &name)
        .await
        .map(|_| ())
}

/// Onboarding "join existing host": pair, pull the whole journal, mark done.
/// The frontend relaunches the app afterwards to restart cleanly in client mode.
#[tauri::command]
pub async fn join_host(
    state: State<'_, AppState>,
    host_url: String,
    code: String,
    name: String,
) -> Result<(), String> {
    crate::sync::join_host(&state, &host_url, &code, &name).await
}

/// Works without AppState — reports whether the db opened or is waiting for a
/// recovery key (init failed with db_locked → nothing is managed).
#[tauri::command]
pub fn db_status(app: AppHandle) -> &'static str {
    if app.try_state::<AppState>().is_some() {
        "ready"
    } else if app.try_state::<crate::state::StartupError>().is_some() {
        "error"
    } else {
        "locked"
    }
}

/// Details for the startup-error screen; works without AppState.
#[tauri::command]
pub fn startup_error(app: AppHandle) -> Option<Value> {
    let e = app.try_state::<crate::state::StartupError>()?;
    let dir = app.path().app_data_dir().ok()?;
    Some(json!({"message": e.0, "dataDir": dir.to_string_lossy()}))
}

/// Accepts a recovery key from the lock screen, verifies it opens the db and
/// stores it in the OS keyring. The frontend relaunches on success.
#[tauri::command]
pub async fn unlock_db(app: AppHandle, recovery: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    run(move || crate::state::unlock_with_recovery(&dir, &recovery)).await
}

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.db.share();
    run(move || Ok(crate::sync::sync_status(&db))).await
}

/// ~2.5 s mDNS browse — must not run on the IPC thread.
#[tauri::command]
pub async fn discover_hosts() -> Result<Vec<Value>, String> {
    run(|| Ok(crate::sync::discover_hosts())).await
}

/// LAN URLs this device can be reached at when it acts as a host.
#[tauri::command]
pub fn host_addresses() -> Vec<String> {
    crate::sync::host_urls()
}

#[tauri::command]
pub async fn conflict_list(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let db = state.db.share();
    run(move || crate::sync::conflict_list(&db)).await
}

#[tauri::command]
pub async fn conflict_resolve(
    state: State<'_, AppState>,
    conflict_id: String,
    winner: String,
) -> Result<(), String> {
    crate::sync::conflict_resolve(&state, &conflict_id, &winner).await
}

#[tauri::command]
pub async fn rates_refresh(state: State<'_, AppState>) -> Result<Value, String> {
    refresh_rates(&state).await
}

/// Fetches NBRB rates for account currencies plus USD/EUR (used by the UI,
/// scheduler and bot). Currencies are fetched in parallel on the shared client;
/// unknown ones are skipped.
pub async fn refresh_rates(state: &AppState) -> Result<Value, String> {
    let db = state.db.share();
    let currencies: Vec<String> = run(move || {
        db.with_read_conn(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT DISTINCT currency FROM accounts WHERE deleted_at IS NULL",
            )?;
            let mut v: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for c in ["USD", "EUR", "RUB"] {
                if !v.iter().any(|x| x == c) {
                    v.push(c.into());
                }
            }
            Ok(v)
        })
        .map_err(|e| e.to_string())
    })
    .await?;

    let http = crate::net::client();
    let mut fetches = Vec::new();
    for cur in &currencies {
        let cur = cur.clone();
        let http = http.clone();
        fetches.push(tauri::async_runtime::spawn(async move {
            let url = format!("https://api.nbrb.by/exrates/rates/{cur}?parammode=2");
            let resp = http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let rate = resp
                .json::<finance_storage::nbrb::NbrbRate>()
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>((
                cur,
                finance_storage::nbrb::per_unit_scaled(&rate),
                finance_storage::nbrb::date_of(&rate).to_string(),
            ))
        }));
    }

    let mut updated = Vec::<String>::new();
    let mut fetched = Vec::<(String, i64, String)>::new();
    for f in fetches {
        if let Ok(Ok(v)) = f.await {
            fetched.push(v);
        }
    }
    // Writes still serialize on the single write conn.
    let db = state.db.share();
    let fetched2 = fetched.clone();
    run(move || {
        db.write(|c| {
            for (cur, scaled, date) in &fetched2 {
                repos::rate_upsert(c, &db, date, cur, *scaled, 1)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
    .await?;
    updated.extend(fetched.iter().map(|f| f.0.clone()));
    let fetched_set: std::collections::HashSet<&String> = fetched.iter().map(|f| &f.0).collect();
    let skipped: Vec<String> = currencies
        .iter()
        .filter(|c| !fetched_set.contains(*c))
        .cloned()
        .collect();
    Ok(json!({"updated": updated, "skipped": skipped}))
}

/// Fetches and stores the NBRB rate for `currency` on `date` when it is not in
/// the table yet. Past dates only — future dates and BYN are a no-op; fetch
/// errors are silently ignored so the caller can fall back to the latest rate.
pub async fn ensure_rate(state: &AppState, currency: &str, date: &str) -> Result<(), String> {
    if currency.eq_ignore_ascii_case("BYN") || date.is_empty() {
        return Ok(());
    }
    let cur = currency.to_uppercase();
    let today = chrono::Local::now().date_naive().to_string();
    if date > today.as_str() {
        return Ok(());
    }
    let db = state.db.share();
    let cc = cur.clone();
    let dd = date.to_string();
    let have = run(move || {
        db.with_read_conn(|c| repos::has_rate_on(c, &cc, &dd))
            .map_err(|e| e.to_string())
    })
    .await?;
    if have {
        return Ok(());
    }
    let url = format!("https://api.nbrb.by/exrates/rates/{cur}?parammode=2&ondate={date}");
    let Ok(resp) = crate::net::client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    else {
        return Ok(());
    };
    let Ok(rate) = resp.json::<finance_storage::nbrb::NbrbRate>().await else {
        return Ok(());
    };
    let scaled = finance_storage::nbrb::per_unit_scaled(&rate);
    if scaled <= 0 {
        return Ok(());
    }
    let stored_date = finance_storage::nbrb::date_of(&rate).to_string();
    let db = state.db.share();
    run(move || {
        db.write(|c| repos::rate_upsert(c, &db, &stored_date, &cur, scaled, 1))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Converts `amount_minor` between currencies using the rate in effect on `date`
/// (fetching it from NBRB when missing). Used by the operation form.
#[tauri::command]
pub async fn convert_at(
    state: State<'_, AppState>,
    amount_minor: i64,
    from: String,
    to: String,
    date: String,
) -> Result<Option<i64>, String> {
    for cur in [&from, &to] {
        let _ = ensure_rate(&state, cur, &date).await;
    }
    let db = state.db.share();
    run(move || {
        db.with_read_conn(|c| repos::convert_minor_at(c, amount_minor, &from, &to, &date))
            .map_err(|e| e.to_string())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_decodes_percent_encoded_utf8() {
        assert_eq!(unescape("receipt.jpg"), Some("receipt.jpg".to_string()));
        assert_eq!(
            unescape("%D1%87%D0%B5%D0%BA%2012%20%D0%BC%D0%B0%D1%8F.png"),
            Some("чек 12 мая.png".to_string())
        );
        assert_eq!(unescape("100%25.jpg"), Some("100%.jpg".to_string()));
        assert_eq!(unescape("truncated%"), None);
        assert_eq!(unescape("%zz"), None);
        assert_eq!(unescape("%ff%ff"), None); // invalid UTF-8
    }

    #[test]
    fn attachment_hash_validation() {
        let good = "a".repeat(64);
        assert!(valid_attachment_hash(&good));
        assert!(valid_attachment_hash(&("Ab0123".repeat(10) + "abcd")));
        for bad in [
            "",
            &"a".repeat(63),
            &"a".repeat(65),
            "../etc/passwd",
            "not-hex!",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            assert!(!valid_attachment_hash(bad), "{bad}");
        }
    }

    #[test]
    fn asset_scope_is_limited_to_attachments() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let scope = &conf["app"]["security"]["assetProtocol"]["scope"];
        assert_eq!(scope["allow"], json!(["$APPDATA/attachments/**"]));
        let deny: Vec<&str> = scope["deny"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for expected in [
            "$APPDATA/*.db",
            "$APPDATA/*.db-*",
            "$APPDATA/*.restore*",
            "$APPDATA/backups/**",
            "$APPDATA/logs/**",
        ] {
            assert!(deny.contains(&expected), "missing deny {expected}");
        }
    }
}
