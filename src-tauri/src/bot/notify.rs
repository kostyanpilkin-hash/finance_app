//! Scheduled bot notifications: daily/weekly/monthly summaries and payment
//! reminders with buttons, optionally held back during quiet hours. Settings are
//! stored in the host's local `settings` table and edited in the app.

use chrono::{Datelike, Months, NaiveDate, NaiveDateTime, NaiveTime};

use super::report;
use super::text::*;
use super::*;

/// Notifications older than this (e.g. after the host was off) are not sent.
const CATCH_UP_HOURS: i64 = 12;

fn setting(state: &AppState, key: &str, default: &str) -> String {
    state
        .db
        .with_read_conn(|c| repos::setting_get(c, key))
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}

fn hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// True when `now` is inside the quiet window [from, to), which may wrap midnight.
pub(super) fn in_quiet(now: NaiveTime, from: NaiveTime, to: NaiveTime) -> bool {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => now >= from && now < to,
        std::cmp::Ordering::Greater => now >= from || now < to,
    }
}

/// Most recent date whose scheduled moment (`at`) has passed and which matches `day`.
pub(super) fn due_date(
    now: NaiveDateTime,
    at: NaiveTime,
    day: impl Fn(NaiveDate) -> bool,
) -> Option<NaiveDate> {
    let mut d = now.date();
    if now.time() < at {
        d = d.pred_opt()?;
    }
    for _ in 0..62 {
        if day(d) {
            return Some(d);
        }
        d = d.pred_opt()?;
    }
    None
}

fn last_day(d: NaiveDate) -> u32 {
    let next = d
        .with_day(1)
        .and_then(|f| f.checked_add_months(Months::new(1)))
        .unwrap_or(d);
    next.pred_opt().map(|x| x.day()).unwrap_or(28)
}

pub(super) fn monthly_matches(d: NaiveDate, day: u32) -> bool {
    d.day() == day.clamp(1, 31).min(last_day(d))
}

struct Job {
    name: &'static str,
    enabled: &'static str,
    time: &'static str,
}

/// Called every minute by the scheduler.
pub async fn tick(state: &AppState) {
    let Some(token) = read_token() else {
        return;
    };
    let now = chrono::Local::now().naive_local();
    if setting(state, "bot_quiet_enabled", "0") == "1" {
        if let (Some(from), Some(to)) = (
            hm(&setting(state, "bot_quiet_from", "23:30")),
            hm(&setting(state, "bot_quiet_to", "08:00")),
        ) {
            if in_quiet(now.time(), from, to) {
                return;
            }
        }
    }
    let tg = Tg {
        http: crate::net::client().clone(),
        token,
    };
    let jobs = [
        Job {
            name: "daily",
            enabled: "1",
            time: "23:00",
        },
        Job {
            name: "weekly",
            enabled: "0",
            time: "20:00",
        },
        Job {
            name: "monthly",
            enabled: "0",
            time: "20:00",
        },
        Job {
            name: "reminders",
            enabled: "1",
            time: "10:00",
        },
    ];
    for job in jobs {
        if setting(state, &format!("bot_{}_enabled", job.name), job.enabled) != "1" {
            continue;
        }
        let Some(at) = hm(&setting(state, &format!("bot_{}_time", job.name), job.time)) else {
            continue;
        };
        let weekday = setting(state, "bot_weekly_day", "7")
            .parse::<u32>()
            .unwrap_or(7);
        let month_day = setting(state, "bot_monthly_day", "1")
            .parse::<u32>()
            .unwrap_or(1);
        let due = match job.name {
            "weekly" => due_date(now, at, |d| d.weekday().number_from_monday() == weekday),
            "monthly" => due_date(now, at, |d| monthly_matches(d, month_day)),
            _ => due_date(now, at, |_| true),
        };
        let Some(due) = due else { continue };
        if (now - due.and_time(at)).num_hours() >= CATCH_UP_HOURS {
            continue;
        }
        let sent_key = format!("bot_{}_sent", job.name);
        if setting(state, &sent_key, "") == due.to_string() {
            continue;
        }
        let _ = state
            .db
            .with_conn(|c| repos::setting_set(c, &sent_key, &due.to_string()));
        let result = if job.name == "reminders" {
            send_reminders(state, &tg, due).await
        } else {
            let from = match job.name {
                "weekly" => due - chrono::Duration::days(6),
                "monthly" => due
                    .checked_sub_months(Months::new(1))
                    .and_then(|d| d.succ_opt())
                    .unwrap_or(due),
                _ => due,
            };
            send_summary(state, &tg, job.name, from, due).await
        };
        if let Err(e) = result {
            super::log(&format!("notify {}: {e}", job.name));
        }
    }
}

fn linked_chats(state: &AppState) -> Vec<(String, i64)> {
    state
        .db
        .with_read_conn(|c| {
            let mut st = c.prepare("SELECT user_id, chat_id FROM telegram_links")?;
            let v = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

async fn send_summary(
    state: &AppState,
    tg: &Tg,
    job: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<(), String> {
    let lang = user_lang(state, None);
    let text = format!(
        "<b>{}</b>\n{}",
        tr(&lang, &format!("{job}_title")),
        report::summary_text(state, &lang, from, to)?
    );
    for (_, chat) in linked_chats(state) {
        tg.call::<Value>(
            "sendMessage",
            &json!({"chat_id": chat, "text": text, "parse_mode": "HTML"}),
        )
        .await?;
    }
    Ok(())
}

fn reminder_text(state: &AppState, lang: &str, r: &Value) -> String {
    let mut s = format!(
        "<b>{}</b>\n🔁 <b>{}</b>",
        tr(lang, "reminder_title"),
        escape(r["name"].as_str().unwrap_or("—"))
    );
    if let Some(a) = r["amountMinor"].as_i64() {
        s.push_str(&format!(
            "\n{}: {}",
            tr(lang, "amount"),
            signed(
                r["kind"].as_str().unwrap_or("expense"),
                a,
                r["currency"].as_str().unwrap_or("")
            )
        ));
    }
    s.push_str(&format!(
        "\n{}: {}",
        tr(lang, "due"),
        iso_date_label(lang, r["nextDue"].as_str().unwrap_or(""))
    ));
    let rows = |e: Entity| {
        state
            .db
            .with_read_conn(|c| repos::list_rows(c, e, false))
            .unwrap_or_default()
    };
    if let Some(a) = r["accountId"]
        .as_str()
        .and_then(|id| rows(Entity::Accounts).into_iter().find(|x| x["id"] == id))
    {
        s.push_str(&format!(
            "\n{}: {}",
            tr(lang, "account"),
            escape(a["name"].as_str().unwrap_or("—"))
        ));
    }
    if let Some(c) = r["categoryId"]
        .as_str()
        .and_then(|id| rows(Entity::Categories).into_iter().find(|x| x["id"] == id))
    {
        s.push_str(&format!(
            "\n{}: {} {}",
            tr(lang, "category"),
            category_emoji(c["icon"].as_str().unwrap_or("")),
            escape(c["name"].as_str().unwrap_or("—"))
        ));
    }
    s
}

async fn send_reminders(state: &AppState, tg: &Tg, due: NaiveDate) -> Result<(), String> {
    let lang = user_lang(state, None);
    let list = state
        .db
        .with_read_conn(|c| repos::reminders_due(c, &due.to_string()))
        .map_err(|e| e.to_string())?;
    for r in list {
        let id = r["id"].as_str().unwrap_or("");
        let next = r["nextDue"].as_str().unwrap_or("");
        let keyboard = kb(vec![
            vec![(tr(&lang, "paid"), format!("rem:p:{id}:{next}"))],
            vec![
                (tr(&lang, "rem_skip"), format!("rem:s:{id}:{next}")),
                (tr(&lang, "rem_later"), format!("rem:l:{id}:{next}")),
            ],
        ]);
        let text = reminder_text(state, &lang, &r);
        for (_, chat) in linked_chats(state) {
            tg.call::<Value>(
                "sendMessage",
                &json!({"chat_id": chat, "text": text, "parse_mode": "HTML", "reply_markup": keyboard}),
            )
            .await?;
        }
    }
    Ok(())
}

/// Creates the operation for a paid reminder (when it has amount, account and
/// category) and advances the reminder, atomically.
fn resolve_reminder(
    state: &AppState,
    ctx: &Ctx,
    id: &str,
    due: &str,
    status: &str,
) -> Result<Option<&'static str>, String> {
    let actor = Actor {
        user_id: ctx.db_user.clone(),
        device_id: Some(state.db.device_id.clone()),
        source: Source::Bot,
    };
    state
        .db
        .with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let Some(r) = finance_storage::db::row_json(&tx, "reminders", id)? else {
                return Ok(None);
            };
            if !r["deletedAt"].is_null() || r["nextDue"].as_str() != Some(due) {
                return Ok(None);
            }
            let mut created = None;
            if status == "done" {
                if let (Some(amount), Some(account), Some(category)) = (
                    r["amountMinor"].as_i64().filter(|a| *a > 0),
                    r["accountId"].as_str(),
                    r["categoryId"].as_str(),
                ) {
                    let row = repos::transaction_upsert(
                        &tx,
                        &state.db,
                        &actor,
                        json!({
                            "kind": r["kind"].as_str().unwrap_or("expense"),
                            "occurredAt": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                            "accountId": account, "amountMinor": amount, "categoryId": category,
                            "comment": r["name"], "source": "reminder", "createdByUserId": ctx.db_user,
                        }),
                    )?;
                    created = row["id"].as_str().map(str::to_string);
                }
            }
            repos::reminder_resolve(&tx, &state.db, &actor, id, status, created.as_deref())?;
            tx.commit()?;
            Ok(Some(match (status, created) {
                ("done", Some(_)) => "rem_paid_tx",
                ("done", None) => "rem_paid",
                _ => "rem_skipped",
            }))
        })
        .map_err(|e| format!("reminder: {e}"))
}

pub(super) async fn reminder_callback(
    state: &AppState,
    ctx: &Ctx,
    cb: &TgCallback,
) -> Result<(), String> {
    let data = cb.data.as_deref().unwrap_or("");
    let parts: Vec<&str> = data.splitn(4, ':').collect();
    let (Some(message), [_, act, id, due]) = (&cb.message, parts.as_slice()) else {
        ctx.tg.answer(&cb.id, &tr(&ctx.lang, "stale")).await;
        return Ok(());
    };
    if ctx.db_user.is_none() || cb.from.id != message.chat.id {
        ctx.tg.answer(&cb.id, &tr(&ctx.lang, "stale")).await;
        return Ok(());
    }
    let outcome = match *act {
        "l" => Some("rem_tomorrow"),
        "p" => resolve_reminder(state, ctx, id, due, "done")?,
        "s" => resolve_reminder(state, ctx, id, due, "skipped")?,
        _ => None,
    };
    let key = outcome.unwrap_or("rem_handled");
    ctx.tg.answer(&cb.id, &tr(&ctx.lang, key)).await;
    let text = format!(
        "{}\n\n{}",
        message.text.as_deref().unwrap_or(""),
        tr(&ctx.lang, key)
    );
    ctx.tg
        .call::<Value>(
            "editMessageText",
            &json!({"chat_id": message.chat.id, "message_id": message.message_id, "text": text}),
        )
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: &str, t: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("{d} {t}"), "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        let t = |s| hm(s).unwrap();
        assert!(in_quiet(t("23:45"), t("23:30"), t("08:00")));
        assert!(in_quiet(t("07:59"), t("23:30"), t("08:00")));
        assert!(!in_quiet(t("08:00"), t("23:30"), t("08:00")));
        assert!(in_quiet(t("13:00"), t("12:00"), t("14:00")));
        assert!(!in_quiet(t("15:00"), t("12:00"), t("14:00")));
        assert!(!in_quiet(t("15:00"), t("12:00"), t("12:00")));
    }

    #[test]
    fn schedules() {
        let t = hm("23:00").unwrap();
        let day = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(
            due_date(at("2026-11-04", "23:01"), t, |_| true),
            Some(day("2026-11-04"))
        );
        assert_eq!(
            due_date(at("2026-11-04", "22:59"), t, |_| true),
            Some(day("2026-11-03"))
        );
        // 2026-11-08 is a Sunday.
        let sunday = |d: NaiveDate| d.weekday().number_from_monday() == 7;
        assert_eq!(
            due_date(at("2026-11-10", "12:00"), t, sunday),
            Some(day("2026-11-08"))
        );
        assert!(monthly_matches(day("2026-02-28"), 31));
        assert!(monthly_matches(day("2024-02-29"), 30));
        assert!(!monthly_matches(day("2024-02-28"), 30));
        assert!(monthly_matches(day("2026-11-15"), 15));
        assert_eq!(
            due_date(at("2026-12-01", "08:00"), t, |d| monthly_matches(d, 30)),
            Some(day("2026-11-30"))
        );
    }
}
