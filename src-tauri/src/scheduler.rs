//! Periodic background tasks: exchange-rate refresh (daily), monthly backups and
//! minute-level bot notifications.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tauri::Manager;

use crate::state::AppState;

pub fn start(app: AppHandle) {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    tauri::async_runtime::spawn(async move {
        // Small delay so the app finishes starting before hitting the network.
        tokio::time::sleep(Duration::from_secs(10)).await;
        while flag.load(Ordering::Relaxed) {
            refresh_once(&app).await;
            let app2 = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || auto_backup(&app2)).await;
            // Check hourly; refresh at most once per calendar day.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

/// Bot notifications (summaries, reminders) are checked every minute.
pub fn start_notifications(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            if let Some(state) = app.try_state::<AppState>() {
                crate::bot::notify::tick(&state).await;
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn refresh_once(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let already = state
        .db
        .with_read_conn(|c| {
            let today = chrono::Local::now().date_naive().to_string();
            let n: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM exchange_rates WHERE date >= ?1",
                    [today],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(n > 0)
        })
        .unwrap_or(true);
    if already {
        return;
    }
    let app = app.clone();
    // Reuse the same code path as the manual command.
    let state = app.state::<AppState>();
    let _ = crate::rates::refresh(&state).await;
}

/// Monthly encrypted backup to <appdata>/backups, keeps the last 12.
fn auto_backup(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let month = chrono::Local::now().format("%Y-%m").to_string();
    let last = state
        .db
        .with_conn(|c| finance_storage::repos::setting_get(c, "auto_backup_month"))
        .ok()
        .flatten();
    if last.as_deref() == Some(&month) {
        return;
    }
    let data_dir = state.db.data_dir();
    if finance_storage::backup::create(&state.db, &data_dir).is_ok() {
        let _ = state
            .db
            .write(|c| finance_storage::repos::setting_set(c, "auto_backup_month", &month));
        finance_storage::backup::prune(&data_dir, 12);
    }
}
