//! Emits "data-changed" when any writer (UI, bot, LAN sync, host API)
//! commits a domain change: every apply_change and every pulled change
//! appends to audit_log, so its max rowid is a cheap change counter.

use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<i64> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let db = state.db.share();
            let cur = crate::blocking::run(move || {
                db.with_read_conn(|c| {
                    Ok(
                        c.query_row("SELECT COALESCE(MAX(rowid),0) FROM audit_log", [], |r| {
                            r.get::<_, i64>(0)
                        })?,
                    )
                })
                .map_err(|e| e.to_string())
            })
            .await
            .ok();
            if let Some(cur) = cur {
                if last.is_some_and(|l| l != cur) {
                    let _ = app.emit("data-changed", cur);
                }
                last = Some(cur);
            }
        }
    });
}
