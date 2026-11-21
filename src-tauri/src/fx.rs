//! BSB bank rates and conversion suggestions for the operation form and the
//! bot. NBRB stays in `rates.rs`; this module falls back to it.

use finance_storage::core::fx::{self, Flow, Mode, Quote, RateKind};
use finance_storage::{bsb, repos};
use serde::{Deserialize, Serialize};

use crate::{blocking::run, state::AppState};

/// What the form / bot asks for: convert the known amount of an entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestReq {
    /// "expense" | "income" | "transfer"
    pub kind: String,
    pub account_id: String,
    #[serde(default)]
    pub to_account_id: Option<String>,
    /// Currency the amount was entered in (expense/income).
    #[serde(default)]
    pub entered_currency: Option<String>,
    /// Entered amount (expense/income) or the debited amount (transfer).
    pub amount_minor: i64,
    /// Local "YYYY-MM-DDTHH:MM[:SS]"; empty = now.
    #[serde(default)]
    pub occurred_at: String,
    /// Manual choice; None = by rule.
    #[serde(default)]
    pub rate_kind: Option<RateKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// In the account currency (expense/income) or the destination currency (transfer).
    pub amount_minor: i64,
    pub currency: String,
    /// Rate actually applied (Nbrb on fallback).
    pub rate_kind: RateKind,
    /// What the rule picked.
    pub default_kind: RateKind,
    pub mode: Mode,
    /// A BSB rate was asked for but NBRB was used.
    pub fallback: bool,
    /// BYN per 1 unit of the foreign currency ×1e6 (the side applied); None for cross pairs.
    pub rate_scaled: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pair {
    pub buy: i64,
    pub sell: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateOverview {
    pub currency: String,
    pub bsb_app: Option<Pair>,
    pub bsb_card: Option<Pair>,
    /// NBRB per 1 unit ×1e6.
    pub nbrb: Option<i64>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Local naive "YYYY-MM-DD[THH:MM[:SS]]" → unix ms, never later than now.
fn local_ms(occurred_at: &str) -> i64 {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone};
    let now = now_ms();
    let s = occurred_at.trim();
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(12, 0, 0))
        });
    ndt.and_then(|n| chrono::Local.from_local_datetime(&n).earliest())
        .map(|t| t.timestamp_millis().min(now))
        .unwrap_or(now)
}

fn local_date(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.date_naive().to_string())
        .unwrap_or_else(|| chrono::Local::now().date_naive().to_string())
}

async fn cached(state: &AppState, kind: RateKind, cur: &str, at_ms: i64) -> Option<Quote> {
    let db = state.db.share();
    let cur = cur.to_string();
    let now = now_ms();
    run(move || {
        db.with_read_conn(|c| repos::bank_rate_find(c, kind, &cur, at_ms, now))
            .map_err(|e| e.to_string())
    })
    .await
    .ok()
    .flatten()
}

async fn fetch(kind: RateKind, at_ms: i64) -> Result<bsb::Snapshot, String> {
    let resp = crate::net::client()
        .post(bsb::URL)
        .header("Accept-Language", "ru")
        .timeout(std::time::Duration::from_secs(10))
        .json(&bsb::request_body(kind, at_ms))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url().to_string())?;
    let text = resp.text().await.map_err(|e| e.without_url().to_string())?;
    bsb::parse(&text).map_err(|e| e.to_string())
}

/// BSB quote for `currency` at `at_ms`: local cache first, then the bank API.
/// Failures are logged and yield None (callers fall back to NBRB).
pub async fn bsb_quote(
    state: &AppState,
    kind: RateKind,
    currency: &str,
    at_ms: i64,
) -> Option<Quote> {
    bsb::api_type(kind)?;
    let cur = currency.to_uppercase();
    if let Some(q) = cached(state, kind, &cur, at_ms).await {
        return Some(q);
    }
    if cfg!(test) {
        return None;
    }
    let snap = match fetch(kind, at_ms).await {
        Ok(s) => s,
        Err(e) => {
            crate::applog::append(
                "rates.log",
                &format!("bsb {} fetch failed: {e}", kind.as_str()),
            );
            return None;
        }
    };
    let db = state.db.share();
    let now = now_ms();
    run(move || {
        db.write(|c| repos::bank_rates_store(c, kind, &snap, now))
            .map_err(|e| e.to_string())
    })
    .await
    .ok()?;
    cached(state, kind, &cur, at_ms).await
}

async fn nbrb_quote(state: &AppState, currency: &str, date: &str) -> Option<Quote> {
    let _ = crate::rates::ensure(state, currency, date).await;
    let db = state.db.share();
    let (cur, date) = (currency.to_string(), date.to_string());
    run(move || {
        db.with_read_conn(|c| repos::nbrb_quote_at(c, &cur, &date))
            .map_err(|e| e.to_string())
    })
    .await
    .ok()
    .flatten()
}

/// Whether an account converts at BSB rates: explicit `rate_source`, else by kind.
fn on_bsb(kind: &str, rate_source: Option<&str>) -> bool {
    match rate_source {
        Some("bsb") => true,
        Some("nbrb") => false,
        _ => matches!(kind, "card" | "savings"),
    }
}

/// (currency, kind, rate_source) of an account.
fn account_info(state: &AppState, id: &str) -> Result<(String, String, Option<String>), String> {
    state
        .db
        .with_read_conn(|c| {
            Ok(c.prepare_cached(
                "SELECT currency, kind, rate_source FROM accounts WHERE id = ?1 AND deleted_at IS NULL",
            )?
            .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)
        })
        .map_err(|_| "account not found".to_string())
}

fn is_foreign(c: &str) -> bool {
    !c.eq_ignore_ascii_case("BYN")
}

async fn quotes_for(
    state: &AppState,
    kind: RateKind,
    from: &str,
    to: &str,
    at: i64,
) -> Option<(Option<Quote>, Option<Quote>)> {
    let mut out = (None, None);
    for (cur, slot) in [(from, &mut out.0), (to, &mut out.1)] {
        if !is_foreign(cur) {
            continue;
        }
        let q = if kind == RateKind::Nbrb {
            nbrb_quote(state, cur, &local_date(at)).await
        } else {
            bsb_quote(state, kind, cur, at).await
        };
        *slot = Some(q?);
    }
    Some(out)
}

/// Suggested converted amount for an entry: picks the rate by rule (or the
/// manual choice), uses BSB quotes at the entry's time and falls back to
/// NBRB when a BSB quote is unavailable. None when there is nothing to convert.
pub async fn suggest(state: &AppState, req: SuggestReq) -> Result<Option<Suggestion>, String> {
    let flow = Flow::parse(&req.kind).ok_or("bad kind")?;
    if req.amount_minor <= 0 {
        return Ok(None);
    }
    let acc = account_info(state, &req.account_id)?;
    let mut all_on_bsb = on_bsb(&acc.1, acc.2.as_deref());
    let entered = req.entered_currency.as_deref().map(str::to_uppercase);
    let (from, to, known_is_from) = match (flow, &entered) {
        (Flow::Transfer, _) => {
            let Some(to_id) = req.to_account_id.as_deref() else {
                return Ok(None);
            };
            let dst = account_info(state, to_id)?;
            all_on_bsb &= on_bsb(&dst.1, dst.2.as_deref());
            (acc.0, dst.0, true)
        }
        (Flow::Expense, Some(e)) => (acc.0, e.clone(), false),
        (Flow::Income, Some(e)) => (e.clone(), acc.0, true),
        _ => return Ok(None),
    };
    if from.eq_ignore_ascii_case(&to) {
        return Ok(None);
    }
    let (default_kind, mode) = fx::default_rule(flow, all_on_bsb, entered.as_deref());
    let wanted = req.rate_kind.unwrap_or(default_kind);
    let at = local_ms(&req.occurred_at);
    let (used, quotes) = match quotes_for(state, wanted, &from, &to, at).await {
        Some(q) => (wanted, q),
        None if wanted != RateKind::Nbrb => {
            match quotes_for(state, RateKind::Nbrb, &from, &to, at).await {
                Some(q) => (RateKind::Nbrb, q),
                None => return Ok(None),
            }
        }
        None => return Ok(None),
    };
    let Some(amount) = fx::convert(
        req.amount_minor,
        &from,
        &to,
        known_is_from,
        quotes.0,
        quotes.1,
    )
    .filter(|n| *n > 0) else {
        return Ok(None);
    };
    let rate_scaled = match (is_foreign(&from), is_foreign(&to)) {
        (true, false) => quotes.0.map(|q| q.buy),
        (false, true) => quotes.1.map(|q| q.sell),
        _ => None,
    };
    Ok(Some(Suggestion {
        amount_minor: amount,
        currency: if known_is_from { to } else { from },
        rate_kind: used,
        default_kind,
        mode,
        fallback: used != wanted,
        rate_scaled,
    }))
}

/// Current BSB (app + card) and NBRB rates for the settings screen and /rates.
pub async fn overview(state: &AppState) -> Result<Vec<RateOverview>, String> {
    let now = now_ms();
    let db = state.db.share();
    let latest = run(move || {
        db.with_read_conn(repos::rates_latest)
            .map_err(|e| e.to_string())
    })
    .await?;
    let pair = |q: Option<Quote>| {
        q.map(|q| Pair {
            buy: q.buy,
            sell: q.sell,
        })
    };
    let mut out = Vec::new();
    for cur in ["USD", "EUR", "RUB", "PLN"] {
        let nbrb = latest
            .iter()
            .find(|r| r["currency"].as_str() == Some(cur))
            .and_then(|r| {
                let rate = r["rateBynScaled"].as_i64()?;
                let scale = r["scale"].as_i64().filter(|s| *s > 0)?;
                Some(rate / scale)
            });
        out.push(RateOverview {
            currency: cur.into(),
            bsb_app: pair(bsb_quote(state, RateKind::BsbApp, cur, now).await),
            bsb_card: pair(bsb_quote(state, RateKind::BsbCard, cur, now).await),
            nbrb,
        });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use finance_storage::{db::Db, Actor, Source};
    use serde_json::json;

    fn state() -> AppState {
        AppState {
            db: Db::open_memory(None).unwrap(),
            db_existed: false,
            new_recovery_key: None,
        }
    }

    fn account(s: &AppState, kind: &str, cur: &str) -> String {
        s.db.with_conn(|c| {
            let a = Actor {
                user_id: None,
                device_id: Some(s.db.device_id.clone()),
                source: Source::Desktop,
            };
            let r = repos::account_upsert(
                c,
                &s.db,
                &a,
                json!({"name":"A","kind":kind,"currency":cur}),
            )?;
            Ok(r["id"].as_str().unwrap().to_string())
        })
        .unwrap()
    }

    /// Caches an open-ended BSB snapshot so tests never hit the network.
    pub(crate) fn cache(s: &AppState, kind: RateKind, quotes: &[(&str, i64, i64)]) {
        let snap = bsb::Snapshot {
            from_ms: 0,
            next_ms: Some(i64::MAX),
            quotes: quotes
                .iter()
                .map(|(c, b, s)| (c.to_string(), Quote { buy: *b, sell: *s }))
                .collect(),
        };
        s.db.write(|c| repos::bank_rates_store(c, kind, &snap, now_ms()))
            .unwrap();
    }

    fn nbrb(s: &AppState, cur: &str, rate: i64) {
        let today = chrono::Local::now().date_naive().to_string();
        s.db.write(|c| repos::rate_upsert(c, &s.db, &today, cur, rate, 1))
            .unwrap();
    }

    fn req(kind: &str, acc: &str, entered: Option<&str>, amount: i64) -> SuggestReq {
        SuggestReq {
            kind: kind.into(),
            account_id: acc.into(),
            to_account_id: None,
            entered_currency: entered.map(Into::into),
            amount_minor: amount,
            occurred_at: String::new(),
            rate_kind: None,
        }
    }

    fn go(s: &AppState, r: SuggestReq) -> Option<Suggestion> {
        tauri::async_runtime::block_on(suggest(s, r)).unwrap()
    }

    #[test]
    fn suggest_expense_byn_on_usd_card_uses_bsb_app_buy() {
        let s = state();
        let usd = account(&s, "card", "USD");
        cache(&s, RateKind::BsbApp, &[("USD", 2_988_000, 3_010_000)]);
        let g = go(&s, req("expense", &usd, Some("BYN"), 5000)).unwrap();
        assert_eq!(
            (
                g.amount_minor,
                g.currency.as_str(),
                g.rate_kind,
                g.mode,
                g.fallback
            ),
            (1673, "USD", RateKind::BsbApp, Mode::Fill, false)
        );
        assert_eq!(g.rate_scaled, Some(2_988_000));
    }

    #[test]
    fn suggest_income_byn_on_usd_card_uses_card_sell() {
        let s = state();
        let usd = account(&s, "card", "USD");
        cache(&s, RateKind::BsbCard, &[("USD", 2_970_000, 3_050_000)]);
        let g = go(&s, req("income", &usd, Some("BYN"), 5000)).unwrap();
        assert_eq!(
            (g.amount_minor, g.rate_kind, g.rate_scaled),
            (1639, RateKind::BsbCard, Some(3_050_000))
        );
    }

    #[test]
    fn suggest_income_rub_is_reference() {
        let s = state();
        let byn = account(&s, "card", "BYN");
        cache(&s, RateKind::BsbApp, &[("RUB", 34_753, 36_440)]);
        let g = go(&s, req("income", &byn, Some("RUB"), 300_000)).unwrap();
        assert_eq!(
            (g.amount_minor, g.mode, g.currency.as_str()),
            (10426, Mode::Reference, "BYN")
        );
    }

    #[test]
    fn suggest_transfer_both_directions() {
        let s = state();
        let usd = account(&s, "card", "USD");
        let byn = account(&s, "card", "BYN");
        cache(&s, RateKind::BsbApp, &[("USD", 2_990_500, 3_010_000)]);
        let mut r = req("transfer", &usd, None, 20723);
        r.to_account_id = Some(byn.clone());
        let g = go(&s, r).unwrap();
        assert_eq!((g.amount_minor, g.currency.as_str()), (61972, "BYN"));
        let mut back = req("transfer", &byn, None, 3010);
        back.to_account_id = Some(usd);
        assert_eq!(go(&s, back).unwrap().amount_minor, 1000);
    }

    #[test]
    fn suggest_cash_account_uses_nbrb() {
        let s = state();
        let cash = account(&s, "cash", "BYN");
        cache(&s, RateKind::BsbCard, &[("USD", 2_970_000, 3_050_000)]);
        nbrb(&s, "USD", 3_000_000);
        let g = go(&s, req("income", &cash, Some("USD"), 10000)).unwrap();
        assert_eq!(
            (g.amount_minor, g.rate_kind, g.default_kind, g.fallback),
            (30000, RateKind::Nbrb, RateKind::Nbrb, false)
        );
    }

    #[test]
    fn suggest_override_nbrb() {
        let s = state();
        let usd = account(&s, "card", "USD");
        cache(&s, RateKind::BsbApp, &[("USD", 2_988_000, 3_010_000)]);
        nbrb(&s, "USD", 3_000_000);
        let mut r = req("expense", &usd, Some("BYN"), 5000);
        r.rate_kind = Some(RateKind::Nbrb);
        let g = go(&s, r).unwrap();
        assert_eq!(
            (g.amount_minor, g.rate_kind, g.default_kind, g.fallback),
            (1667, RateKind::Nbrb, RateKind::BsbApp, false)
        );
    }

    #[test]
    fn suggest_currency_without_bsb_quote_falls_back() {
        let s = state();
        let byn = account(&s, "card", "BYN");
        cache(&s, RateKind::BsbApp, &[("USD", 2_988_000, 3_010_000)]);
        nbrb(&s, "CNY", 420_000);
        let g = go(&s, req("expense", &byn, Some("CNY"), 10000)).unwrap();
        assert_eq!(
            (g.amount_minor, g.rate_kind, g.default_kind, g.fallback),
            (4200, RateKind::Nbrb, RateKind::BsbApp, true)
        );
    }

    #[test]
    fn suggest_same_currency_or_empty_is_none() {
        let s = state();
        let byn = account(&s, "card", "BYN");
        assert!(go(&s, req("expense", &byn, Some("BYN"), 5000)).is_none());
        assert!(go(&s, req("expense", &byn, None, 5000)).is_none());
        assert!(go(&s, req("expense", &byn, Some("USD"), 0)).is_none());
        // No rate anywhere → nothing to suggest, no error.
        assert!(go(&s, req("expense", &byn, Some("USD"), 100)).is_none());
    }

    #[test]
    fn explicit_account_rate_source_wins() {
        assert!(on_bsb("cash", Some("bsb")));
        assert!(!on_bsb("card", Some("nbrb")));
        assert!(on_bsb("savings", None));
        assert!(!on_bsb("other", None));
    }

    #[test]
    fn local_ms_never_in_future() {
        let now = now_ms();
        assert!(local_ms("2999-01-01T10:00") <= now_ms());
        assert!(local_ms("") >= now);
        assert!(local_ms("2026-11-20T20:49:00") < now);
        assert_eq!(
            local_ms("2026-11-20T20:49"),
            local_ms("2026-11-20T20:49:00")
        );
    }
}
