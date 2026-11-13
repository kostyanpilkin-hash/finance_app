//! NBRB rate fetching and storage — shared by commands, the bot and the
//! scheduler. Lives outside the command layer so non-UI callers don't reach
//! into `commands.rs`.

use finance_storage::repos;
use serde_json::{json, Value};

use crate::{blocking::run, state::AppState};

/// Fetches NBRB rates for account currencies plus USD/EUR (used by the UI,
/// scheduler and bot). Currencies are fetched in parallel on the shared client;
/// unknown ones are skipped.
pub async fn refresh(state: &AppState) -> Result<Value, String> {
    let db = state.db.share();
    let currencies: Vec<String> = run(move || {
        db.with_read_conn(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT DISTINCT currency FROM accounts WHERE deleted_at IS NULL",
            )?;
            let mut v: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for c in ["USD", "EUR", "RUB"] {
                if !v.iter().any(|x| x == c) {
                    v.push(c.into());
                }
            }
            Ok(v)
        })
        .map_err(|e| e.to_string())
    })
    .await?;

    let http = crate::net::client();
    let mut fetches = Vec::new();
    for cur in &currencies {
        let cur = cur.clone();
        let http = http.clone();
        fetches.push(tauri::async_runtime::spawn(async move {
            let url = format!("https://api.nbrb.by/exrates/rates/{cur}?parammode=2");
            let resp = http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let rate = resp
                .json::<finance_storage::nbrb::NbrbRate>()
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>((
                cur,
                finance_storage::nbrb::per_unit_scaled(&rate),
                finance_storage::nbrb::date_of(&rate).to_string(),
            ))
        }));
    }

    let mut updated = Vec::<String>::new();
    let mut fetched = Vec::<(String, i64, String)>::new();
    for f in fetches {
        if let Ok(Ok(v)) = f.await {
            fetched.push(v);
        }
    }
    // Writes still serialize on the single write conn.
    let db = state.db.share();
    let fetched2 = fetched.clone();
    run(move || {
        db.write(|c| {
            for (cur, scaled, date) in &fetched2 {
                repos::rate_upsert(c, &db, date, cur, *scaled, 1)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
    .await?;
    updated.extend(fetched.iter().map(|f| f.0.clone()));
    let fetched_set: std::collections::HashSet<&String> = fetched.iter().map(|f| &f.0).collect();
    let skipped: Vec<String> = currencies
        .iter()
        .filter(|c| !fetched_set.contains(*c))
        .cloned()
        .collect();
    Ok(json!({"updated": updated, "skipped": skipped}))
}

/// Fetches and stores the NBRB rate for `currency` on `date` when it is not in
/// the table yet. Past dates only — future dates and BYN are a no-op; fetch
/// errors are silently ignored so the caller can fall back to the latest rate.
pub async fn ensure(state: &AppState, currency: &str, date: &str) -> Result<(), String> {
    if currency.eq_ignore_ascii_case("BYN") || date.is_empty() {
        return Ok(());
    }
    let cur = currency.to_uppercase();
    let today = chrono::Local::now().date_naive().to_string();
    if date > today.as_str() {
        return Ok(());
    }
    let db = state.db.share();
    let cc = cur.clone();
    let dd = date.to_string();
    let have = run(move || {
        db.with_read_conn(|c| repos::has_rate_on(c, &cc, &dd))
            .map_err(|e| e.to_string())
    })
    .await?;
    if have {
        return Ok(());
    }
    let url = format!("https://api.nbrb.by/exrates/rates/{cur}?parammode=2&ondate={date}");
    let Ok(resp) = crate::net::client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    else {
        return Ok(());
    };
    let Ok(rate) = resp.json::<finance_storage::nbrb::NbrbRate>().await else {
        return Ok(());
    };
    let scaled = finance_storage::nbrb::per_unit_scaled(&rate);
    if scaled <= 0 {
        return Ok(());
    }
    let stored_date = finance_storage::nbrb::date_of(&rate).to_string();
    let db = state.db.share();
    run(move || {
        db.write(|c| repos::rate_upsert(c, &db, &stored_date, &cur, scaled, 1))
            .map_err(|e| e.to_string())
    })
    .await
}
