mod bot;
mod commands;
mod scheduler;
mod state;
mod sync;
mod tray;

use tauri::{Manager, WindowEvent};

pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::set_tray_labels,
            commands::onboarding_status,
            commands::recovery_key,
            commands::complete_onboarding,
            commands::set_current_user,
            commands::get_settings,
            commands::set_setting,
            commands::entity_list,
            commands::entity_delete,
            commands::entity_restore,
            commands::user_upsert,
            commands::account_upsert,
            commands::category_upsert,
            commands::category_delete,
            commands::transaction_upsert,
            commands::transaction_list,
            commands::dashboard,
            commands::journal,
            commands::goal_upsert,
            commands::reminder_upsert,
            commands::reminder_resolve,
            commands::reminders_due,
            commands::pin_set,
            commands::pin_verify,
            commands::pin_clear,
            commands::pin_enabled,
            commands::backup_create,
            commands::backup_list,
            commands::backup_restore,
            commands::attachment_add,
            commands::attachment_path,
            commands::export_xlsx,
            commands::rates_latest,
            commands::convert_at,
            commands::rates_refresh,
            commands::tg_connect,
            commands::tg_disconnect,
            commands::tg_status,
            commands::tg_pair_code,
            commands::device_pair_code,
            commands::pair_to_host,
            commands::sync_status,
            commands::discover_hosts,
            commands::host_addresses,
            commands::conflict_list,
            commands::conflict_resolve,
            commands::join_host,
            commands::db_status,
            commands::unlock_db,
        ])
        .setup(|app| {
            let state = match state::init_db(app) {
                Ok(s) => s,
                Err(e) if e == "db_locked" => {
                    // Db exists but the keyring lost its key — the window shows
                    // the recovery-key screen; no state is managed.
                    show_main_window(app.handle());
                    return Ok(());
                }
                Err(e) => panic!("failed to open the database: {e}"),
            };
            app.manage(state);
            tray::create(app.handle())?;
            scheduler::start(app.handle().clone());
            scheduler::start_notifications(app.handle().clone());
            bot::start_polling(app.handle().clone());
            sync::start_host(app.handle());
            sync::start_client(app.handle().clone());
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
