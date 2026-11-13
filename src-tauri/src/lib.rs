mod blocking;
mod bot;
mod commands;
mod net;
mod rates;
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
            commands::transaction_days,
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
            commands::pairing_requests,
            commands::pairing_decide,
            commands::pair_to_host,
            commands::sync_status,
            commands::discover_hosts,
            commands::host_addresses,
            commands::conflict_list,
            commands::conflict_resolve,
            commands::join_host,
            commands::db_status,
            commands::startup_error,
            commands::unlock_db,
        ])
        .setup(|app| {
            let state = match state::init_db(app) {
                Ok(s) => s,
                Err(e) if e == "db_locked" => {
                    // Db exists but the keyring lost its key — the window shows
                    // the recovery-key screen; no state is managed. The tray is
                    // still needed: the close button only hides the window.
                    tray::create(app.handle())?;
                    show_main_window(app.handle());
                    return Ok(());
                }
                Err(e) => {
                    // Corrupt db / failed migration: keep the app alive so the
                    // UI can show the error screen (backup restore etc.) and
                    // log the cause for support. Never log secrets.
                    if let Ok(dir) = app.path().app_data_dir() {
                        let logs = dir.join("logs");
                        let _ = std::fs::create_dir_all(&logs);
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(logs.join("startup.log"))
                        {
                            use std::io::Write;
                            let _ = writeln!(
                                f,
                                "{} {e}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                            );
                        }
                    }
                    app.manage(state::StartupError(e));
                    tray::create(app.handle())?;
                    show_main_window(app.handle());
                    return Ok(());
                }
            };
            app.manage(state);
            sync::migrate_device_token(app.state::<state::AppState>().inner());
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
