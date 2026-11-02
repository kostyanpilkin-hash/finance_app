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
            fire_reminders(&app).await;
            auto_backup(&app);
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

/// Notifies linked Telegram chats about due reminders (once per day).
async fn fire_reminders(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let today = finance_storage::db::now_iso()[..10].to_string();
    // Only fire once per day.
    let last = state
        .db
        .with_conn(|c| finance_storage::repos::setting_get(c, "reminders_fired"))
        .ok()
        .flatten();
    if last.as_deref() == Some(&today) {
        return;
    }
    let due = match state
        .db
        .with_conn(|c| finance_storage::repos::reminders_due(c, &today))
    {
        Ok(v) => v,
        Err(_) => return,
    };
    if due.is_empty() {
        return;
    }
    let _ = state
        .db
        .with_conn(|c| finance_storage::repos::setting_set(c, "reminders_fired", &today));
    let names: Vec<String> = due
        .iter()
        .map(|r| {
            let amt = r["amountMinor"]
                .as_i64()
                .map(|a| {
                    format!(
                        " {:.2} {}",
                        a as f64 / 100.0,
                        r["currency"].as_str().unwrap_or("")
                    )
                })
                .unwrap_or_default();
            format!(
                "• {}{} ({})",
                r["name"].as_str().unwrap_or("?"),
                amt,
                r["nextDue"].as_str().unwrap_or("")
            )
        })
        .collect();
    let _ =
        crate::bot::notify_linked(&state, &format!("⏰ Напоминания:\n{}", names.join("\n"))).await;
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
    let dir = std::path::Path::new(&state.db.attachments_dir)
        .join("..")
        .join("backups");
    let _ = std::fs::create_dir_all(&dir);
    let name = format!("backup-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let ok = state
        .db
        .with_conn(|c| {
            c.execute(
                "VACUUM INTO ?1",
                [dir.join(&name).to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .is_ok();
    if ok {
        let _ = state
            .db
            .with_conn(|c| finance_storage::repos::setting_set(c, "auto_backup_month", &month));
        if let Ok(mut files) = std::fs::read_dir(&dir).map(|rd| rd.flatten().collect::<Vec<_>>()) {
            files.sort_by_key(|f| f.file_name());
            if files.len() > 12 {
                for f in files.drain(..files.len() - 12) {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
    }
}
