use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    api_version: u32,
    platform: &'static str,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        api_version: finance_api::API_VERSION,
        platform: std::env::consts::OS,
    }
}

/// Localizes the tray menu when the UI language changes.
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
