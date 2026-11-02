mod commands;
mod scheduler;
mod state;
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
            commands::attachment_add,
            commands::attachment_path,
            commands::export_xlsx,
            commands::rates_latest,
            commands::rates_refresh,
        ])
        .setup(|app| {
            let state = state::init_db(app).expect("failed to open the database");
            app.manage(state);
            tray::create(app.handle())?;
            scheduler::start(app.handle().clone());
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
