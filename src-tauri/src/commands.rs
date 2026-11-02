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

fn db<'a>(state: &'a State<'a, AppState>) -> &'a Db {
    &state.db
}

fn actor(state: &State<AppState>, user_id: Option<String>) -> Actor {
    Actor {
        user_id,
        device_id: Some(state.db.device_id.clone()),
        source: Source::Desktop,
    }
}

fn actor_or_current(state: &State<AppState>) -> Result<Actor, String> {
    db(state)
        .with_conn(|c| {
            let uid = repos::setting_get(c, "current_user_id")?;
            Ok(Actor {
                user_id: uid,
                device_id: Some(state.db.device_id.clone()),
                source: Source::Desktop,
            })
        })
        .map_err(|e| e.to_string())
}

// ---------- onboarding / bootstrap ----------

#[tauri::command]
pub fn onboarding_status(state: State<AppState>) -> Result<Value, String> {
    db(&state)
        .with_conn(|c| {
            let done = repos::setting_get(c, "onboarding_done")? == Some("1".into());
            let users = repos::list_rows(c, Entity::Users, false)?.len();
            Ok(json!({
                "done": done,
                "users": users,
                "dbExisted": state.db_existed,
                "mode": state.db.mode,
            }))
        })
        .map_err(|e| e.to_string())
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
pub fn complete_onboarding(state: State<AppState>, input: OnboardingInput) -> Result<(), String> {
    db(&state)
        .with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let actor = actor_or_stateless(&state);
            seed_categories(&tx, &state.db, &actor)?;
            let mut ids = Vec::new();
            for (i, name) in input.user_names.iter().enumerate() {
                let u = repos::user_upsert(
                    &tx,
                    &state.db,
                    &actor,
                    json!({"name": name, "sort": i as i64}),
                )?;
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
                    &tx,
                    &state.db,
                    &actor,
                    json!({
                        "name": name, "kind": kind, "currency": "BYN", "ownerUserId": owner
                    }),
                )?;
            }
            repos::setting_set(&tx, "onboarding_done", "1")?;
            repos::setting_set(&tx, "current_user_id", owner.as_deref().unwrap_or(""))?;
            state.db.set_mode("host")?;
            tx.commit()?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

fn actor_or_stateless(state: &State<AppState>) -> Actor {
    actor(state, None)
}

#[tauri::command]
pub fn set_current_user(state: State<AppState>, user_id: String) -> Result<(), String> {
    db(&state)
        .with_conn(|c| repos::setting_set(c, "current_user_id", &user_id))
        .map_err(|e| e.to_string())
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Value, String> {
    db(&state)
        .with_conn(repos::settings_all)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    db(&state)
        .with_conn(|c| repos::setting_set(c, &key, &value))
        .map_err(|e| e.to_string())
}

// ---------- generic entity CRUD ----------

#[tauri::command]
pub fn entity_list(
    state: State<AppState>,
    entity: String,
    include_deleted: Option<bool>,
) -> Result<Vec<Value>, String> {
    let e = Entity::parse(&entity).ok_or("unknown entity")?;
    db(&state)
        .with_conn(|c| repos::list_rows(c, e, include_deleted.unwrap_or(false)))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn entity_delete(state: State<AppState>, entity: String, id: String) -> Result<(), String> {
    let e = Entity::parse(&entity).ok_or("unknown entity")?;
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| {
            repos::apply(&state.db, c, &a, e, models::Op::Delete, &id, None)?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn entity_restore(state: State<AppState>, entity: String, id: String) -> Result<(), String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::restore(c, &state.db, &a, &entity, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn user_upsert(state: State<AppState>, input: Value) -> Result<Value, String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::user_upsert(c, &state.db, &a, input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn account_upsert(state: State<AppState>, input: Value) -> Result<Value, String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::account_upsert(c, &state.db, &a, input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn category_upsert(state: State<AppState>, input: Value) -> Result<Value, String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::category_upsert(c, &state.db, &a, input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn category_delete(
    state: State<AppState>,
    id: String,
    move_to: Option<String>,
) -> Result<(), String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::category_delete(c, &state.db, &a, &id, move_to.as_deref()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn transaction_upsert(state: State<AppState>, input: Value) -> Result<Value, String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::transaction_upsert(c, &state.db, &a, input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn transaction_list(state: State<AppState>, filter: Value) -> Result<Vec<Value>, String> {
    let f: repos::TransactionFilter = serde_json::from_value(filter).map_err(|e| e.to_string())?;
    db(&state)
        .with_conn(|c| repos::transaction_list(c, &f))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dashboard(state: State<AppState>, from: String, to: String) -> Result<Value, String> {
    db(&state)
        .with_conn(|c| repos::dashboard(c, &from, &to))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn journal(state: State<AppState>, filter: Value) -> Result<Vec<Value>, String> {
    let f: repos::JournalFilter = serde_json::from_value(filter).map_err(|e| e.to_string())?;
    db(&state)
        .with_conn(|c| repos::journal(c, &f))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn goal_upsert(state: State<AppState>, input: Value) -> Result<Value, String> {
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| repos::goal_upsert(c, &state.db, &a, input))
        .map_err(|e| e.to_string())
}

// ---------- attachments ----------

#[tauri::command]
pub fn attachment_add(
    state: State<AppState>,
    transaction_id: String,
    file_name: String,
    mime: String,
    bytes_base64: String,
) -> Result<Value, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_base64)
        .map_err(|e| e.to_string())?;
    let a = actor_or_current(&state)?;
    db(&state)
        .with_conn(|c| {
            repos::attachment_add(c, &state.db, &a, &transaction_id, &file_name, &mime, &bytes)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn attachment_path(state: State<AppState>, sha256: String) -> Result<String, String> {
    let p = state.db.attachments_dir.join(&sha256);
    if p.exists() {
        Ok(p.to_string_lossy().into_owned())
    } else {
        Err("attachment missing".into())
    }
}

// ---------- exchange rates ----------

#[tauri::command]
pub fn export_xlsx(
    state: State<AppState>,
    app: AppHandle,
    from: String,
    to: String,
) -> Result<String, String> {
    let dir = app
        .path()
        .document_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Финансы");
    db(&state)
        .with_conn(|c| finance_storage::export::export_xlsx(c, &dir, &from, &to))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rates_latest(state: State<AppState>) -> Result<Vec<Value>, String> {
    db(&state)
        .with_conn(repos::rates_latest)
        .map_err(|e| e.to_string())
}

/// Refreshes exchange rates from api.nbrb.by for every currency used by
/// accounts plus USD/EUR. Currencies unknown to NBRB are skipped.
#[tauri::command]
pub async fn rates_refresh(state: State<'_, AppState>) -> Result<Value, String> {
    let currencies: Vec<String> = db(&state)
        .with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT DISTINCT currency FROM accounts WHERE deleted_at IS NULL")?;
            let mut v: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for c in ["USD", "EUR"] {
                if !v.iter().any(|x| x == c) {
                    v.push(c.into());
                }
            }
            Ok(v)
        })
        .map_err(|e| e.to_string())?;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let mut updated = Vec::<String>::new();
    let mut skipped = Vec::<String>::new();
    for cur in currencies {
        let url = format!("https://api.nbrb.by/exrates/rates/{cur}?parammode=2");
        let Ok(resp) = http.get(&url).send().await else {
            skipped.push(cur);
            continue;
        };
        let Ok(rate) = resp.json::<finance_storage::nbrb::NbrbRate>().await else {
            skipped.push(cur);
            continue;
        };
        let scaled = finance_storage::nbrb::per_unit_scaled(&rate);
        let date = finance_storage::nbrb::date_of(&rate).to_string();
        db(&state)
            .with_conn(|c| repos::rate_upsert(c, &state.db, &date, &cur, scaled, 1))
            .map_err(|e| e.to_string())?;
        updated.push(cur);
    }
    Ok(json!({"updated": updated, "skipped": skipped}))
}
