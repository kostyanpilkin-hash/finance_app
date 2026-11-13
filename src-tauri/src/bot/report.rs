//! Read-only reports shared by the wizard screens and scheduled notifications.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};
use finance_storage::repos;
use finance_storage::rusqlite::{Connection, OptionalExtension};

use super::text::*;
use crate::state::AppState;

pub(super) fn display_currency(state: &AppState) -> String {
    state
        .db
        .with_read_conn(|c| repos::setting_get(c, "display_currency"))
        .ok()
        .flatten()
        .filter(|c| c.len() == 3)
        .unwrap_or_else(|| "BYN".into())
}

fn convert(c: &Connection, amount: i64, from: &str, to: &str) -> Option<i64> {
    repos::convert_minor(c, amount, from, to).ok().flatten()
}

pub(super) fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// (from, to) for a summary period key relative to `today`.
pub(super) fn period(key: &str, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match key {
        "yesterday" => {
            let y = today.pred_opt().unwrap_or(today);
            (y, y)
        }
        "week" => (
            today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64),
            today,
        ),
        "month" => (today.with_day(1).unwrap_or(today), today),
        _ => (today, today),
    }
}

pub(super) struct Slice {
    pub name: String,
    pub icon: String,
    pub amount: i64,
}

#[derive(Default)]
pub(super) struct Totals {
    pub income: i64,
    pub expense: i64,
    pub slices: Vec<Slice>,
    pub missing: BTreeSet<String>,
}

/// Income/expense for a period converted to `cur`, plus expense by category.
pub(super) fn totals(
    state: &AppState,
    from: NaiveDate,
    to: NaiveDate,
    cur: &str,
) -> Result<Totals, String> {
    state
        .db
        .with_read_conn(|c| {
            let s =
                finance_storage::analytics::summary(c, &from.to_string(), &to.to_string(), cur)?;
            let mut t = Totals {
                income: s.income,
                expense: s.expense,
                missing: s.missing.into_iter().collect(),
                ..Default::default()
            };
            for b in s.by_category {
                if b.expense <= 0 {
                    continue;
                }
                let meta: Option<(Option<String>, Option<String>)> = match &b.key {
                    Some(id) => c
                        .prepare_cached("SELECT name, icon FROM categories WHERE id = ?1")?
                        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()?,
                    None => None,
                };
                let (name, icon) = meta.unwrap_or_default();
                t.slices.push(Slice {
                    name: name.unwrap_or_else(|| "—".into()),
                    icon: icon.unwrap_or_default(),
                    amount: b.expense,
                });
            }
            Ok(t)
        })
        .map_err(|_| "totals failed".into())
}

fn missing_line(lang: &str, missing: &BTreeSet<String>) -> String {
    if missing.is_empty() {
        String::new()
    } else {
        format!(
            "\n{} {}",
            tr(lang, "no_rate"),
            missing.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    }
}

/// Balances per account (account currency), total in `cur`, accounts without a rate.
fn balances(
    state: &AppState,
    cur: &str,
) -> Result<(Vec<serde_json::Value>, i64, BTreeSet<String>), String> {
    state
        .db
        .with_read_conn(|c| {
            let d = today().to_string();
            let dash = repos::dashboard(c, &d, &d)?;
            let accounts: Vec<serde_json::Value> = dash["accounts"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a["archived"].as_i64().unwrap_or(0) == 0)
                .collect();
            let mut total = 0i64;
            let mut missing = BTreeSet::new();
            let mut out = Vec::new();
            for mut a in accounts {
                let balance = a["balance"].as_i64().unwrap_or(0);
                let currency = a["currency"].as_str().unwrap_or("").to_string();
                let converted = convert(c, balance, &currency, cur);
                if a["includeInTotal"].as_i64().unwrap_or(1) == 1 {
                    match converted {
                        Some(v) => total = total.saturating_add(v),
                        None => {
                            missing.insert(currency.clone());
                        }
                    }
                }
                a["converted"] = converted.map_or(serde_json::Value::Null, |v| v.into());
                out.push(a);
            }
            Ok((out, total, missing))
        })
        .map_err(|_| "balances failed".into())
}

/// Dashboard lines above the main menu.
pub(super) fn dashboard(state: &AppState, lang: &str) -> Result<String, String> {
    let cur = display_currency(state);
    let (_, total, mut missing) = balances(state, &cur)?;
    let now = today();
    let day = totals(state, now, now, &cur)?;
    let (mf, mt) = period("month", now);
    let month = totals(state, mf, mt, &cur)?;
    missing.extend(day.missing);
    missing.extend(month.missing.iter().cloned());
    Ok(format!(
        "{}: <b>{}</b>\n{}: 🔴 {} · 🟢 {}\n{}: 🔴 {} · 🟢 {}{}",
        tr(lang, "total"),
        money(total, &cur),
        tr(lang, "today_line"),
        money(day.expense, &cur),
        money(day.income, &cur),
        tr(lang, "month_line"),
        money(month.expense, &cur),
        money(month.income, &cur),
        missing_line(lang, &missing)
    ))
}

pub(super) fn balances_text(state: &AppState, lang: &str) -> Result<String, String> {
    let cur = display_currency(state);
    let (accounts, total, missing) = balances(state, &cur)?;
    if accounts.is_empty() {
        return Ok(tr(lang, "empty"));
    }
    let mut lines = Vec::new();
    for a in &accounts {
        let balance = a["balance"].as_i64().unwrap_or(0);
        let currency = a["currency"].as_str().unwrap_or("");
        let last4 = a["last4"]
            .as_str()
            .map(|l| format!(" ·{l}"))
            .unwrap_or_default();
        let owner = a["ownerName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| format!(" · {}", escape(s)))
            .unwrap_or_default();
        let approx = match a["converted"].as_i64() {
            Some(v) if currency != cur => format!(" (≈ {})", money(v, &cur)),
            _ => String::new(),
        };
        lines.push(format!(
            "{}{} <b>{}</b>{last4}{owner}\n      {}{approx}",
            account_emoji(a["kind"].as_str().unwrap_or("")),
            if balance < 0 { "🔴" } else { "" },
            escape(a["name"].as_str().unwrap_or("—")),
            money(balance, currency),
        ));
    }
    Ok(format!(
        "{}\n━━━━━━━━━━━━\n{}: <b>{}</b>{}",
        lines.join("\n"),
        tr(lang, "total"),
        money(total, &cur),
        missing_line(lang, &missing)
    ))
}

pub(super) fn rates_text(state: &AppState, lang: &str) -> Result<String, String> {
    let rates = state
        .db
        .with_read_conn(repos::rates_latest)
        .map_err(|_| "rates failed")?;
    if rates.is_empty() {
        return Ok(tr(lang, "rates_empty"));
    }
    let mut date = String::new();
    let mut lines = Vec::new();
    let mut rates = rates;
    rates.sort_by_key(|r| {
        let c = r["currency"].as_str().unwrap_or("").to_string();
        (!matches!(c.as_str(), "USD" | "EUR" | "RUB"), c)
    });
    for r in rates {
        let code = r["currency"].as_str().unwrap_or("");
        let scaled = r["rateBynScaled"].as_i64().unwrap_or(0);
        let units = if code == "RUB" { 100 } else { 1 };
        date = date.max(r["date"].as_str().unwrap_or("").to_string());
        lines.push(format!(
            "{} {units} {code} = <b>{}</b> BYN",
            currency_flag(code),
            rate4(scaled.saturating_mul(units))
        ));
    }
    Ok(format!(
        "{} {}\n\n{}",
        tr(lang, "rates_on"),
        full_date(&date),
        lines.join("\n")
    ))
}

/// HTML summary for a period, with comparison to the previous period of equal length.
pub(super) fn summary_text(
    state: &AppState,
    lang: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<String, String> {
    let cur = display_currency(state);
    let t = totals(state, from, to, &cur)?;
    let range = if from == to {
        date_label(lang, from, today())
    } else {
        format!("{} — {}", from.format("%d.%m.%Y"), to.format("%d.%m.%Y"))
    };
    let mut s = format!("📅 {range}\n\n");
    if t.income == 0 && t.expense == 0 && t.missing.is_empty() {
        s.push_str(&tr(lang, "no_ops"));
        return Ok(s);
    }
    let net = t.income.saturating_sub(t.expense);
    s.push_str(&format!(
        "{}: <b>{}</b>\n{}: <b>{}</b>\n{}: <b>{}{}</b>",
        tr(lang, "income_word"),
        money(t.income, &cur),
        tr(lang, "expense_word"),
        money(t.expense, &cur),
        tr(lang, "net"),
        if net > 0 { "+" } else { "" },
        money(net, &cur)
    ));
    let days = (to - from).num_days() + 1;
    let prev_to = from.pred_opt().unwrap_or(from);
    let prev_from = prev_to - chrono::Duration::days(days - 1);
    let prev = totals(state, prev_from, prev_to, &cur)?;
    if prev.expense > 0 {
        let change = percent(t.expense - prev.expense, prev.expense);
        s.push_str(&format!(
            "\n{} {}: {}{change} %",
            if change > 0 { "📈" } else { "📉" },
            tr(lang, "vs_prev"),
            if change > 0 { "+" } else { "" }
        ));
    }
    if !t.slices.is_empty() {
        s.push_str(&format!("\n\n<b>{}</b>", tr(lang, "top")));
        for x in t.slices.iter().take(5) {
            s.push_str(&format!(
                "\n{} {} — {} · {} %\n{}",
                category_emoji(&x.icon),
                escape(&x.name),
                money(x.amount, &cur),
                percent(x.amount, t.expense),
                progress(x.amount, t.expense, 10)
            ));
        }
    }
    s.push_str(&missing_line(lang, &t.missing));
    Ok(s)
}

/// Donut PNG + legend caption for expenses of a period; None when there are no expenses.
pub(super) fn chart(
    state: &AppState,
    lang: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Option<(Vec<u8>, String)>, String> {
    let cur = display_currency(state);
    let t = totals(state, from, to, &cur)?;
    if t.expense <= 0 {
        return Ok(None);
    }
    let mut slices: Vec<(String, i64)> = t
        .slices
        .iter()
        .take(7)
        .map(|s| {
            (
                format!("{} {}", category_emoji(&s.icon), escape(&s.name)),
                s.amount,
            )
        })
        .collect();
    let rest: i64 = t.slices.iter().skip(7).map(|s| s.amount).sum();
    if rest > 0 {
        slices.push((tr(lang, "others"), rest));
    }
    let png = super::chart::donut_png(&slices.iter().map(|s| s.1).collect::<Vec<_>>())?;
    let range = if from == to {
        date_label(lang, from, today())
    } else {
        format!("{} — {}", from.format("%d.%m"), to.format("%d.%m.%Y"))
    };
    let mut caption = format!(
        "<b>{}</b>\n📅 {range} · {}: <b>{}</b>\n",
        tr(lang, "chart_caption"),
        tr(lang, "expense_word"),
        money(t.expense, &cur)
    );
    for (i, (name, amount)) in slices.iter().enumerate() {
        caption.push_str(&format!(
            "\n{} {name} — {} · {} %",
            super::chart::LEGEND[i],
            money(*amount, &cur),
            percent(*amount, t.expense)
        ));
    }
    caption.push_str(&missing_line(lang, &t.missing));
    Ok(Some((png, caption.chars().take(1024).collect())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods() {
        let wed = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        assert_eq!(period("day", wed), (wed, wed));
        let y = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        assert_eq!(period("yesterday", wed), (y, y));
        assert_eq!(
            period("week", wed).0,
            NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
        );
        assert_eq!(
            period("month", wed).0,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
        );
    }
}
