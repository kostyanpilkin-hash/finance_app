//! Periodic background tasks: exchange-rate refresh (daily) and, later, reminders/backups.

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
            // Check hourly; refresh at most once per calendar day.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

async fn refresh_once(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let already = state
        .db
        .with_conn(|c| {
            let today = finance_storage::db::now_iso()[..10].to_string();
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
    let _ = crate::commands::rates_refresh(state).await;
}
