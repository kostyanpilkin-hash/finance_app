//! Append-only text logs in <appdata>/logs, rotated at 1 MB. Never pass secrets.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_data_dir() {
        let dir = dir.join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let _ = DIR.set(dir);
    }
}

pub fn path(file: &str) -> Option<PathBuf> {
    DIR.get().map(|d| d.join(file))
}

pub fn append(file: &str, message: &str) {
    let Some(path) = path(file) else { return };
    if std::fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            f,
            "{} {message}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
    }
}
