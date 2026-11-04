//! Card-based bot UI: one active inline card per chat. Callbacks edit the card in
//! place; typed input and commands move a fresh card to the bottom of the chat.

use super::report;
use super::text::*;
use super::*;
use serde::Serialize;

/// Bump to resend the persistent reply keyboard to every chat.
const KEYBOARD_VERSION: u32 = 1;
const PAGE: usize = 10;
const STEPS: i64 = 9;
/// Currencies offered at the currency step (the account currency always comes first).
const ENTRY_CURRENCIES: [&str; 4] = ["BYN", "USD", "EUR", "RUB"];

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
enum Step {
    #[default]
    Menu,
    Amount,
    Account,
    Currency,
    ConvAmount,
    Category,
    ToAccount,
    ToAmount,
    Date,
    CustomDate,
    Time,
    Comment,
    Photo,
    Review,
    Edit,
    Cancel,
    Summary,
    Balance,
    Rates,
    Goals,
    Goal,
    GoalInput,
    History,
    HistDate,
    HistCustom,
    TxView,
    TxDelete,
    Help,
    Undo,
    Done,
}

fn is_entry(step: Step) -> bool {
    use Step::*;
    matches!(
        step,
        Amount
            | Account
            | Currency
            | ConvAmount
            | Category
            | ToAccount
            | ToAmount
            | Date
            | CustomDate
            | Time
            | Comment
            | Photo
            | Review
            | Edit
    )
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Draft {
    step: Step,
    previous: Step,
    editing: bool,
    existing: bool,
    id: String,
    revision: u64,
    message_id: i64,
    kind: String,
    /// Amount as entered (in `currency`, or the account currency when None).
    amount: i64,
    account: String,
    /// Entry currency when it differs from the account currency.
    currency: Option<String>,
    /// Amount in the account currency for a foreign-currency entry.
    converted: Option<i64>,
    destination: Option<String>,
    credited: Option<i64>,
    category: Option<String>,
    date: String,
    time: String,
    dated: bool,
    comment: String,
    photo: Option<String>,
    has_receipt: bool,
    page: usize,
    notice: String,
    undo_id: Option<String>,
    period: String,
    goal: Option<String>,
    goal_op: String,
    /// ISO date restricting the history list ("" = latest entries).
    filter_date: String,
    tx: Option<String>,
    pending: String,
    keyboard: u32,
}

fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            step: Step::Menu,
            previous: Step::Menu,
            editing: false,
            existing: false,
            id: finance_storage::db::new_id(),
            revision: 0,
            message_id: 0,
            kind: "expense".into(),
            amount: 0,
            account: String::new(),
            currency: None,
            converted: None,
            destination: None,
            credited: None,
            category: None,
            date: report::today().to_string(),
            time: now_hm(),
            dated: false,
            comment: String::new(),
            photo: None,
            has_receipt: false,
            page: 0,
            notice: String::new(),
            undo_id: None,
            period: String::new(),
            goal: None,
            goal_op: String::new(),
            filter_date: String::new(),
            tx: None,
            pending: String::new(),
            keyboard: 0,
        }
    }
}

impl Draft {
    /// A clean draft that keeps the card identity and keyboard state.
    fn fresh(&self) -> Draft {
        Draft {
            message_id: self.message_id,
            revision: self.revision,
            keyboard: self.keyboard,
            ..Draft::default()
        }
    }
}

fn load(state: &AppState, chat: i64) -> Result<Draft, String> {
    state
        .db
        .with_conn(|c| {
            use finance_storage::rusqlite::OptionalExtension;
            let raw: Option<String> = c
                .query_row(
                    "SELECT state_json FROM tg_sessions WHERE chat_id=?1",
                    [chat],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(raw
                .and_then(|s| serde_json::from_str::<Draft>(&s).ok())
                .unwrap_or_default())
        })
        .map_err(|_| "session read failed".into())
}

fn store(state: &AppState, chat: i64, draft: &Draft) -> Result<(), String> {
    let raw = serde_json::to_string(draft).map_err(|_| "session encoding failed")?;
    state.db.with_conn(|c| {
        c.execute("INSERT OR REPLACE INTO tg_sessions (chat_id,state_json,updated_at) VALUES (?1,?2,?3)", finance_storage::rusqlite::params![chat, raw, now_iso()])?;
        Ok(())
    }).map_err(|_| "session write failed".into())
}

/// Non-deleted rows of any entity (not every table has a `sort` column).
fn rows(state: &AppState, entity: Entity) -> Result<Vec<Value>, String> {
    state
        .db
        .with_conn(|c| repos::list_rows(c, entity, true))
        .map(|v| v.into_iter().filter(|r| r["deletedAt"].is_null()).collect())
        .map_err(|_| "lookup failed".into())
}

fn find(state: &AppState, entity: Entity, id: &str) -> Result<Option<Value>, String> {
    Ok(rows(state, entity)?.into_iter().find(|r| r["id"] == id))
}

fn active(r: &Value) -> bool {
    r["archived"].as_i64().unwrap_or(0) == 0
}

/// Active account used for writes.
fn account(state: &AppState, id: &str) -> Result<Value, String> {
    find(state, Entity::Accounts, id)?
        .filter(active)
        .ok_or_else(|| "account unavailable".into())
}

fn account_label(row: &Value) -> String {
    let last4 = row["last4"]
        .as_str()
        .map(|l| format!(" ·{l}"))
        .unwrap_or_default();
    format!(
        "{} {}{last4} ({})",
        account_emoji(row["kind"].as_str().unwrap_or("")),
        row["name"].as_str().unwrap_or("—"),
        row["currency"].as_str().unwrap_or("")
    )
}

fn category_label(row: &Value) -> String {
    format!(
        "{} {}",
        category_emoji(row["icon"].as_str().unwrap_or("")),
        row["name"].as_str().unwrap_or("—")
    )
}

fn user_name(state: &AppState, id: Option<&str>) -> String {
    id.and_then(|id| find(state, Entity::Users, id).ok().flatten())
        .and_then(|u| u["name"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Category ids used most often for this kind during the last 90 days.
fn favourites(state: &AppState, kind: &str) -> Result<Vec<String>, String> {
    let since = (report::today() - chrono::Duration::days(90)).to_string();
    state
        .db
        .with_conn(|c| {
            let mut st = c.prepare(
                "SELECT category_id FROM transactions WHERE deleted_at IS NULL AND kind=?1
                 AND category_id IS NOT NULL AND occurred_at >= ?2
                 GROUP BY category_id ORDER BY COUNT(*) DESC, MAX(occurred_at) DESC LIMIT 4",
            )?;
            let v = st
                .query_map(finance_storage::rusqlite::params![kind, since], |r| {
                    r.get(0)
                })?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(v)
        })
        .map_err(|_| "favourites failed".into())
}

fn choices(state: &AppState, d: &Draft) -> Result<Vec<Value>, String> {
    let mut values: Vec<Value> = match d.step {
        Step::Category => {
            let mut all: Vec<Value> = rows(state, Entity::Categories)?
                .into_iter()
                .filter(|r| r["kind"] == d.kind && active(r))
                .collect();
            all.sort_by_key(|r| r["sort"].as_i64().unwrap_or(0));
            let favs = favourites(state, &d.kind)?;
            let mut out: Vec<Value> = favs
                .iter()
                .filter_map(|id| all.iter().find(|r| r["id"] == *id).cloned())
                .map(|mut r| {
                    r["fav"] = true.into();
                    r
                })
                .collect();
            out.extend(
                all.into_iter()
                    .filter(|r| !favs.iter().any(|f| r["id"] == *f)),
            );
            return Ok(out);
        }
        Step::Goals => rows(state, Entity::Goals)?
            .into_iter()
            .filter(active)
            .collect(),
        Step::History => {
            let day = (!d.filter_date.is_empty()).then(|| d.filter_date.clone());
            return state
                .db
                .with_conn(|c| {
                    repos::transaction_list(
                        c,
                        &repos::TransactionFilter {
                            from: day.clone(),
                            to: day,
                            limit: Some(50),
                            ..Default::default()
                        },
                    )
                })
                .map_err(|_| "history failed".into());
        }
        _ => rows(state, Entity::Accounts)?
            .into_iter()
            .filter(|r| active(r) && (d.step != Step::ToAccount || r["id"] != d.account))
            .collect(),
    };
    values.sort_by_key(|r| r["sort"].as_i64().unwrap_or(0));
    Ok(values)
}

fn button(d: &Draft, lang: &str, key: &str, action: &str) -> (String, String) {
    (tr(lang, key), format!("{}:{action}", d.revision))
}

fn back_step(d: &Draft) -> Step {
    use Step::*;
    match d.step {
        Account => Amount,
        Currency | ToAccount => Account,
        ConvAmount => Currency,
        Category if d.kind == "transfer" => Account,
        Category if d.currency.is_some() => ConvAmount,
        Category => Currency,
        ToAmount => ToAccount,
        Date if d.kind == "transfer" => {
            if d.credited.is_some() {
                ToAmount
            } else {
                ToAccount
            }
        }
        Date => Category,
        CustomDate | Time => Date,
        Comment => Time,
        Photo => Comment,
        Review if d.existing => Edit,
        Review => Photo,
        Edit if d.existing => TxView,
        Edit => Review,
        Cancel => d.previous,
        Goal => Goals,
        GoalInput => Goal,
        HistDate => History,
        HistCustom => HistDate,
        TxView => History,
        TxDelete => TxView,
        _ => Menu,
    }
}

fn advance(d: &mut Draft, step: Step) {
    d.step = if d.editing {
        d.editing = false;
        Step::Review
    } else {
        step
    };
    d.page = 0;
}

fn step_number(step: Step) -> i64 {
    use Step::*;
    match step {
        Amount => 1,
        Account => 2,
        Currency | ConvAmount => 3,
        Category | ToAccount | ToAmount => 4,
        Date | CustomDate => 5,
        Time => 6,
        Comment => 7,
        Photo => 8,
        _ => 9,
    }
}

fn account_currency(state: &AppState, d: &Draft) -> Result<String, String> {
    if d.account.is_empty() {
        return Ok(String::new());
    }
    Ok(find(state, Entity::Accounts, &d.account)?
        .and_then(|a| a["currency"].as_str().map(str::to_string))
        .unwrap_or_default())
}

/// The entered amount converted to the account currency at the NBRB rate
/// in effect on the operation date.
fn suggested(state: &AppState, d: &Draft) -> Result<Option<i64>, String> {
    let (Some(cur), acc) = (&d.currency, account_currency(state, d)?) else {
        return Ok(None);
    };
    state
        .db
        .with_conn(|c| repos::convert_minor_at(c, d.amount, cur, &acc, &d.date))
        .map(|v| v.filter(|n| *n > 0))
        .map_err(|_| "conversion failed".into())
}

/// On the account-amount step, lazily downloads the NBRB rate in effect on the
/// operation date so the suggestion is computed for that day, not for today.
async fn ensure_conv_rates(state: &AppState, d: &Draft) {
    if d.step != Step::ConvAmount {
        return;
    }
    let Some(cur) = d.currency.clone() else {
        return;
    };
    let acc = account_currency(state, d).unwrap_or_default();
    for c in [cur, acc] {
        let _ = crate::commands::ensure_rate(state, &c, &d.date).await;
    }
}

/// Field lines of the entry card.
fn entry_lines(state: &AppState, d: &Draft, lang: &str) -> Result<String, String> {
    let mut lines = Vec::new();
    let acc = if d.account.is_empty() {
        None
    } else {
        find(state, Entity::Accounts, &d.account)?
    };
    let cur = acc
        .as_ref()
        .and_then(|a| a["currency"].as_str())
        .unwrap_or("")
        .to_string();
    if d.amount > 0 {
        lines.push(format!(
            "{}: <b>{}</b>",
            tr(lang, "amount"),
            signed(&d.kind, d.amount, d.currency.as_deref().unwrap_or(&cur))
        ));
    }
    if let Some(a) = &acc {
        lines.push(format!(
            "{}: {}",
            tr(lang, "account"),
            escape(&account_label(a))
        ));
    }
    if let (Some(_), Some(n)) = (&d.currency, d.converted) {
        lines.push(format!("{}: {}", tr(lang, "on_account"), money(n, &cur)));
    }
    if let Some(id) = &d.destination {
        if let Some(a) = find(state, Entity::Accounts, id)? {
            lines.push(format!(
                "{}: {}",
                tr(lang, "destination"),
                escape(&account_label(&a))
            ));
            if let Some(n) = d.credited {
                lines.push(format!(
                    "{}: {}",
                    tr(lang, "credited"),
                    money(n, a["currency"].as_str().unwrap_or(""))
                ));
            }
        }
    }
    if let Some(id) = &d.category {
        let name = find(state, Entity::Categories, id)?
            .map(|r| category_label(&r))
            .unwrap_or_else(|| "—".into());
        lines.push(format!("{}: {}", tr(lang, "category"), escape(&name)));
    }
    if d.dated {
        lines.push(format!(
            "{}: {}",
            tr(lang, "date"),
            iso_date_label(lang, &d.date)
        ));
        lines.push(format!("{}: {}", tr(lang, "time"), d.time));
    }
    if !d.comment.is_empty() {
        lines.push(format!("{}: {}", tr(lang, "comment"), escape(&d.comment)));
    }
    if d.photo.is_some() || d.has_receipt {
        lines.push(format!("{}: {}", tr(lang, "photo"), tr(lang, "attached")));
    }
    Ok(lines.join("\n"))
}

fn goal_text(g: &Value, lang: &str) -> String {
    let cur = g["currency"].as_str().unwrap_or("BYN");
    let (done, target) = (
        g["currentMinor"].as_i64().unwrap_or(0),
        g["targetMinor"].as_i64().unwrap_or(0),
    );
    let deadline = g["deadline"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" · {} {}", tr(lang, "goal_until"), full_date(s)))
        .unwrap_or_default();
    format!(
        "🎯 <b>{}</b>\n{} {} %\n{} / {}{deadline}{}",
        escape(g["name"].as_str().unwrap_or("—")),
        progress(done, target, 10),
        percent(done, target),
        money(done, cur),
        money(target, cur),
        if target > 0 && done >= target {
            format!("\n{}", tr(lang, "goal_reached"))
        } else {
            String::new()
        }
    )
}

fn tx_button(state: &AppState, r: &Value, lang: &str) -> String {
    let what = if r["kind"] == "transfer" {
        format!(
            "{} → {}",
            r["aName"].as_str().unwrap_or("—"),
            r["a2Name"].as_str().unwrap_or("—")
        )
    } else {
        format!(
            "{} {}",
            category_emoji(r["cIcon"].as_str().unwrap_or("")),
            r["cName"].as_str().unwrap_or("—")
        )
    };
    let who = r["uName"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    let _ = (state, lang);
    format!(
        "{} {} · {what} · {}{who}",
        kind_icon(r["kind"].as_str().unwrap_or("")),
        money(
            r["amountMinor"].as_i64().unwrap_or(0),
            r["aCurrency"].as_str().unwrap_or("")
        ),
        short_date(r["occurredAt"].as_str().unwrap_or(""))
    )
}

fn tx_details(state: &AppState, id: &str, lang: &str) -> Result<Option<String>, String> {
    let Some(row) = state
        .db
        .with_conn(|c| finance_storage::db::row_json(c, "transactions", id))
        .map_err(|_| "lookup failed")?
        .filter(|r| r["deletedAt"].is_null())
    else {
        return Ok(None);
    };
    let receipts: i64 = state
        .db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM attachments WHERE transaction_id=?1 AND deleted_at IS NULL",
                [id],
                |r| r.get(0),
            )?)
        })
        .unwrap_or(0);
    let d = draft_from_row(&row, receipts > 0);
    let mut s = format!(
        "<b>{} · {}</b>\n{}",
        tr(lang, "tx_title"),
        tr(lang, &d.kind),
        entry_lines(state, &d, lang)?
    );
    let author = user_name(state, row["createdByUserId"].as_str());
    if !author.is_empty() {
        s.push_str(&format!("\n{}: {}", tr(lang, "author"), escape(&author)));
    }
    let source = match row["source"].as_str().unwrap_or("") {
        "bot" => "src_bot",
        "desktop" => "src_desktop",
        "reminder" => "src_reminder",
        _ => "src_other",
    };
    s.push_str(&format!("\n{}: {}", tr(lang, "source"), tr(lang, source)));
    let created = row["createdAt"].as_str().unwrap_or("");
    let updated = row["updatedAt"].as_str().unwrap_or("");
    if !created.is_empty() {
        s.push_str(&format!(
            "\n{}: {}",
            tr(lang, "created"),
            local_stamp(created)
        ));
    }
    if !updated.is_empty() && updated.get(..19) != created.get(..19) {
        s.push_str(&format!(
            "\n{}: {}",
            tr(lang, "changed"),
            local_stamp(updated)
        ));
    }
    Ok(Some(s))
}

fn draft_from_row(row: &Value, has_receipt: bool) -> Draft {
    let occurred = row["occurredAt"].as_str().unwrap_or("");
    let amount = row["amountMinor"].as_i64().unwrap_or(0);
    let orig = match (
        row["origCurrency"].as_str(),
        row["origAmountMinor"].as_i64(),
    ) {
        (Some(c), Some(n)) => Some((c.to_string(), n)),
        _ => None,
    };
    Draft {
        id: row["id"].as_str().unwrap_or("").into(),
        existing: true,
        kind: row["kind"].as_str().unwrap_or("expense").into(),
        amount: orig.as_ref().map_or(amount, |o| o.1),
        currency: orig.as_ref().map(|o| o.0.clone()),
        converted: orig.as_ref().map(|_| amount),
        account: row["accountId"].as_str().unwrap_or("").into(),
        destination: row["toAccountId"].as_str().map(str::to_string),
        credited: row["toAmountMinor"].as_i64(),
        category: row["categoryId"].as_str().map(str::to_string),
        date: occurred.get(..10).unwrap_or("").into(),
        time: occurred.get(11..16).unwrap_or("12:00").into(),
        dated: true,
        comment: row["comment"].as_str().unwrap_or("").into(),
        has_receipt,
        ..Draft::default()
    }
}

fn view(state: &AppState, d: &Draft, ctx: &Ctx) -> Result<(String, Value), String> {
    use Step::*;
    let l = ctx.lang.as_str();
    let b = |key: &str, action: &str| button(d, l, key, action);
    let mut keys: Vec<Vec<(String, String)>> = Vec::new();
    let mut text = format!("<b>{}</b>\n", tr(l, "title"));
    match d.step {
        Menu => {
            let name = user_name(state, ctx.db_user.as_deref());
            let greeting = if name.is_empty() {
                tr(l, "welcome")
            } else {
                format!("{}, {}", escape(&name), tr(l, "choose"))
            };
            text.push_str(&format!("{greeting}\n\n{}", report::dashboard(state, l)?));
            keys.extend([
                vec![b("expense", "kind:expense"), b("income", "kind:income")],
                vec![b("transfer", "kind:transfer"), b("balance", "balance")],
                vec![b("summary", "summary"), b("history", "history")],
                vec![b("goals", "goals"), b("rates", "rates")],
                vec![b("undo", "undo"), b("help", "help")],
            ]);
        }
        Done => text.push_str(&format!(
            "\n<b>{}</b>\n{}",
            tr(l, if d.existing { "updated" } else { "saved" }),
            entry_lines(state, d, l)?
        )),
        Summary => {
            text.push_str(&format!("<b>{}</b>\n", tr(l, "summary")));
            if d.period.is_empty() {
                text.push_str(&tr(l, "summary_pick"));
            } else {
                let (from, to) = report::period(&d.period, report::today());
                text.push_str(&report::summary_text(state, l, from, to)?);
            }
            keys.push(vec![b("today", "sum:day"), b("yesterday", "sum:yesterday")]);
            keys.push(vec![b("week", "sum:week"), b("month", "sum:month")]);
            if !d.period.is_empty() {
                keys.push(vec![b("chart", "chart")]);
            }
        }
        Balance => text.push_str(&format!(
            "<b>{}</b>\n\n{}",
            tr(l, "balance"),
            report::balances_text(state, l)?
        )),
        Rates => {
            text.push_str(&format!(
                "<b>{}</b>\n{}",
                tr(l, "rates_title"),
                report::rates_text(state, l)?
            ));
            keys.push(vec![b("refresh", "refresh")]);
        }
        Help => text.push_str(&tr(l, "help_text")),
        Undo => {
            let found = d.undo_id.is_some();
            text.push_str(&tr(l, if found { "undo_question" } else { "no_undo" }));
            if found {
                keys.push(vec![b("undo_confirm", "undo_yes")]);
            }
        }
        Cancel => {
            text.push_str(&tr(l, "cancel_question"));
            keys.push(vec![b("discard", "discard"), b("continue", "continue")]);
        }
        Goals | History => {
            let all = choices(state, d)?;
            text.push_str(&format!(
                "<b>{}{}</b>\n",
                tr(
                    l,
                    if d.step == Goals {
                        "goals"
                    } else {
                        "history_title"
                    }
                ),
                if d.step == History && !d.filter_date.is_empty() {
                    format!(" · {}", iso_date_label(l, &d.filter_date))
                } else {
                    String::new()
                }
            ));
            if all.is_empty() {
                text.push_str(&tr(l, "empty"));
            }
            for r in all.iter().skip(d.page * PAGE).take(PAGE) {
                let id = r["id"].as_str().unwrap_or("");
                if d.step == Goals {
                    text.push_str(&format!("\n{}\n", goal_text(r, l)));
                    keys.push(vec![(
                        format!("🎯 {}", r["name"].as_str().unwrap_or("—")),
                        format!("{}:goal:{id}", d.revision),
                    )]);
                } else {
                    keys.push(vec![(
                        tx_button(state, r, l),
                        format!("{}:tx:{id}", d.revision),
                    )]);
                }
            }
            if d.step == History {
                keys.push(vec![if d.filter_date.is_empty() {
                    b("hist_pick", "hist:pick")
                } else {
                    b("hist_all", "hist:clear")
                }]);
            }
            push_pages(d, all.len(), &mut keys);
        }
        HistDate | HistCustom => {
            text.push_str(&format!("<b>{}</b>\n", tr(l, "history_title")));
            if d.step == HistDate {
                text.push_str(&format!("<b>{}</b>", tr(l, "hist_date_pick")));
                keys.push(vec![
                    b("today", "hdate:today"),
                    b("yesterday", "hdate:yesterday"),
                ]);
                keys.push(vec![
                    b("tomorrow", "hdate:tomorrow"),
                    b("other", "hdate:other"),
                ]);
            } else {
                text.push_str(&format!("<b>{}</b>", tr(l, "date_hint")));
            }
            keys.push(vec![b("back", "back")]);
        }
        Goal | GoalInput => {
            let goal = d
                .goal
                .as_deref()
                .and_then(|id| find(state, Entity::Goals, id).ok().flatten())
                .ok_or("goal unavailable")?;
            text.push_str(&goal_text(&goal, l));
            if d.step == Goal {
                keys.push(vec![b("goal_add", "gop:add"), b("goal_sub", "gop:sub")]);
                keys.push(vec![
                    b("goal_target", "gop:target"),
                    b("goal_rename", "gop:rename"),
                ]);
                keys.push(vec![b("goal_archive", "gop:archive")]);
            } else {
                text.push_str(&format!(
                    "\n\n<b>{}</b>",
                    tr(
                        l,
                        if d.goal_op == "rename" {
                            "goal_name_hint"
                        } else {
                            "goal_amount_hint"
                        }
                    )
                ));
            }
            keys.push(vec![b("back", "back")]);
        }
        TxView | TxDelete => {
            let id = d.tx.as_deref().unwrap_or("");
            match tx_details(state, id, l)? {
                Some(s) => text.push_str(&s),
                None => text.push_str(&tr(l, "tx_missing")),
            }
            if d.step == TxView {
                keys.push(vec![b("edit", "txedit"), b("delete", "txdel")]);
            } else {
                text.push_str(&format!("\n\n<b>{}</b>", tr(l, "delete_question")));
                keys.push(vec![b("delete_yes", "txdel_yes")]);
            }
            keys.push(vec![b("back", "back")]);
        }
        _ => {
            let n = step_number(d.step);
            let lines = entry_lines(state, d, l)?;
            text.push_str(&format!(
                "<b>{}{}</b>\n{} {n}/{STEPS}\n{}{}\n\n",
                if d.existing { "✏️ " } else { "" },
                tr(l, &d.kind),
                progress(n, STEPS, STEPS as usize),
                lines,
                if lines.is_empty() { "" } else { "\n" }
            ));
            let acc_cur = account_currency(state, d)?;
            let prompt = match d.step {
                Amount => "amount_hint",
                Account if d.kind == "income" => "account_to",
                Account => "account_from",
                Currency => "currency_pick",
                ConvAmount if d.kind == "income" => "conv_in",
                ConvAmount => "conv_out",
                Category => "category_pick",
                ToAccount => "destination_pick",
                ToAmount => "credited_hint",
                Date => "date_pick",
                CustomDate => "date_hint",
                Time => "time_hint",
                Comment => "comment_hint",
                Photo => "photo_hint",
                Edit => "edit_pick",
                _ => "review",
            };
            if d.step == ConvAmount {
                text.push_str(&format!("<b>{} {acc_cur}?</b>", tr(l, prompt)));
            } else {
                text.push_str(&format!("<b>{}</b>", tr(l, prompt)));
            }
            match d.step {
                Currency => {
                    keys.push(vec![(
                        format!(
                            "{} {acc_cur} · {}",
                            currency_flag(&acc_cur),
                            tr(l, "account_currency")
                        ),
                        format!("{}:cur:{acc_cur}", d.revision),
                    )]);
                    let others: Vec<(String, String)> = ENTRY_CURRENCIES
                        .iter()
                        .filter(|c| **c != acc_cur)
                        .map(|c| {
                            (
                                format!("{} {c}", currency_flag(c)),
                                format!("{}:cur:{c}", d.revision),
                            )
                        })
                        .collect();
                    keys.extend(others.chunks(3).map(|c| c.to_vec()));
                }
                ConvAmount => {
                    if let Some(n) = suggested(state, d)? {
                        keys.push(vec![(
                            format!("≈ {} · {}", money(n, &acc_cur), tr(l, "by_rate")),
                            format!("{}:conv:rate", d.revision),
                        )]);
                    }
                }
                Account | ToAccount | Category => {
                    let all = choices(state, d)?;
                    if all.is_empty() {
                        text.push_str(&format!("\n{}", tr(l, "empty")));
                    }
                    let page: Vec<&Value> = all.iter().skip(d.page * PAGE).take(PAGE).collect();
                    for chunk in page.chunks(if d.step == Category { 2 } else { 1 }) {
                        keys.push(
                            chunk
                                .iter()
                                .map(|r| {
                                    let label = if d.step == Category {
                                        format!(
                                            "{}{}",
                                            if r["fav"] == true { "⭐" } else { "" },
                                            category_label(r)
                                        )
                                    } else {
                                        account_label(r)
                                    };
                                    (
                                        label,
                                        format!(
                                            "{}:pick:{}",
                                            d.revision,
                                            r["id"].as_str().unwrap_or("")
                                        ),
                                    )
                                })
                                .collect(),
                        );
                    }
                    push_pages(d, all.len(), &mut keys);
                }
                Date => {
                    keys.push(vec![
                        b("today", "date:today"),
                        b("yesterday", "date:yesterday"),
                    ]);
                    keys.push(vec![
                        b("tomorrow", "date:tomorrow"),
                        b("other", "date:other"),
                    ]);
                }
                Time => {
                    keys.push(vec![(
                        format!("{} {}", tr(l, "now"), now_hm()),
                        format!("{}:time:now", d.revision),
                    )]);
                    if !d.editing {
                        keys.push(vec![b("save_now", "save_now")]);
                    }
                }
                Comment | Photo => {
                    keys.push(vec![b("skip", "skip")]);
                    if !d.editing {
                        keys.push(vec![b("save_now", "save_now")]);
                    }
                }
                Review => keys.push(vec![b("save", "save"), b("edit", "edit")]),
                Edit => {
                    let choice = if d.kind == "transfer" {
                        "destination"
                    } else {
                        "category"
                    };
                    keys.push(vec![
                        b("amount", "edit:amount"),
                        b("account", "edit:account"),
                    ]);
                    keys.push(vec![b(choice, "edit:choice"), b("date", "edit:date")]);
                    keys.push(vec![b("time", "edit:time"), b("comment", "edit:comment")]);
                    if d.kind == "transfer" {
                        keys.push(vec![b("photo", "edit:photo")]);
                    } else {
                        keys.push(vec![
                            b("currency", "edit:currency"),
                            b("photo", "edit:photo"),
                        ]);
                    }
                    if d.existing {
                        keys.push(vec![b("save", "save")]);
                    }
                }
                _ => {}
            }
            keys.push(vec![b("back", "back"), b("cancel", "cancel")]);
        }
    }
    if matches!(
        d.step,
        Summary
            | Balance
            | Rates
            | Help
            | Undo
            | Goals
            | Goal
            | History
            | HistDate
            | HistCustom
            | TxView
    ) {
        keys.push(vec![b("menu", "menu")]);
    }
    if !d.notice.is_empty() {
        text.push_str(&format!("\n\n{}", escape(&d.notice)));
    }
    Ok((text, kb(keys)))
}

fn push_pages(d: &Draft, len: usize, keys: &mut Vec<Vec<(String, String)>>) {
    let mut pages = Vec::new();
    if d.page > 0 {
        pages.push(("◀️".into(), format!("{}:page:{}", d.revision, d.page - 1)));
    }
    if (d.page + 1) * PAGE < len {
        pages.push(("▶️".into(), format!("{}:page:{}", d.revision, d.page + 1)));
    }
    if !pages.is_empty() {
        keys.push(pages);
    }
}

/// Edits the active card in place, or (with `fresh`) retires it and sends a new one
/// at the bottom of the chat.
async fn render(
    state: &AppState,
    ctx: &Ctx,
    chat: i64,
    d: &mut Draft,
    fresh: bool,
) -> Result<(), String> {
    d.revision = d.revision.checked_add(1).ok_or("revision overflow")?;
    let (text, keyboard) = view(state, d, ctx)?;
    store(state, chat, d)?;
    if d.message_id != 0 {
        if fresh {
            let _ = ctx
                .tg
                .call::<Value>(
                    "editMessageReplyMarkup",
                    &json!({"chat_id":chat,"message_id":d.message_id,"reply_markup":{"inline_keyboard":[]}}),
                )
                .await;
        } else {
            let result = ctx.tg.call::<Value>("editMessageText", &json!({"chat_id":chat,"message_id":d.message_id,"text":text,"parse_mode":"HTML","reply_markup":keyboard})).await;
            match result {
                Ok(_) => return Ok(()),
                Err(e) if e.contains("message is not modified") => return Ok(()),
                Err(e)
                    if e.contains("message to edit not found")
                        || e.contains("message can't be edited") => {}
                Err(e) => return Err(e),
            }
        }
    }
    let message = ctx
        .tg
        .call::<Value>(
            "sendMessage",
            &json!({"chat_id":chat,"text":text,"parse_mode":"HTML","reply_markup":keyboard}),
        )
        .await?
        .ok_or("missing message")?;
    d.message_id = message["message_id"].as_i64().ok_or("missing message id")?;
    store(state, chat, d)
}

fn reply_keyboard(lang: &str) -> Value {
    json!({
        "keyboard": [
            [{"text": tr(lang, "expense")}, {"text": tr(lang, "income")}],
            [{"text": tr(lang, "balance")}, {"text": tr(lang, "menu")}],
        ],
        "resize_keyboard": true,
        "is_persistent": true,
    })
}

async fn send_keyboard(ctx: &Ctx, chat: i64, d: &mut Draft) -> Result<(), String> {
    ctx.tg
        .call::<Value>(
            "sendMessage",
            &json!({"chat_id":chat,"text":tr(&ctx.lang, "keyboard_on"),"reply_markup":reply_keyboard(&ctx.lang)}),
        )
        .await?;
    d.keyboard = KEYBOARD_VERSION;
    Ok(())
}

/// Maps slash commands and reply-keyboard labels to card actions.
fn command(text: &str) -> Option<&'static str> {
    let text = text.trim();
    let cmd = text.split('@').next().unwrap_or(text);
    let action = match cmd {
        "/start" | "/menu" | "/меню" | "/cancel" => "menu",
        "/expense" => "kind:expense",
        "/income" => "kind:income",
        "/transfer" => "kind:transfer",
        "/balance" => "balance",
        "/summary" => "summary",
        "/today" => "sum:day",
        "/week" => "sum:week",
        "/month" => "sum:month",
        "/history" => "history",
        "/goals" => "goals",
        "/rates" => "rates",
        "/undo" => "undo",
        "/help" => "help",
        _ => {
            return ["ru", "be", "en"].iter().find_map(|l| {
                [
                    ("menu", "menu"),
                    ("expense", "kind:expense"),
                    ("income", "kind:income"),
                    ("balance", "balance"),
                ]
                .into_iter()
                .find(|(key, _)| tr(l, key) == text)
                .map(|(_, a)| a)
            })
        }
    };
    Some(action)
}

fn apply_text(d: &mut Draft, text: &str) -> bool {
    match d.step {
        Step::Amount | Step::ToAmount | Step::ConvAmount => {
            let Some(n) = super::parse_minor(text).filter(|n| *n > 0) else {
                return false;
            };
            if d.step == Step::Amount {
                d.amount = n;
                if d.editing && d.currency.is_some() {
                    // A new entered amount needs a new account-currency amount.
                    d.converted = None;
                    d.step = Step::ConvAmount;
                } else {
                    advance(d, Step::Account);
                }
            } else if d.step == Step::ConvAmount {
                d.converted = Some(n);
                advance(d, Step::Category);
            } else {
                d.credited = Some(n);
                advance(d, Step::Date);
            }
        }
        Step::CustomDate => {
            let Some(date) = date_input(text) else {
                return false;
            };
            d.date = date;
            d.dated = true;
            advance(d, Step::Time);
        }
        Step::HistDate | Step::HistCustom => {
            let Some(date) = date_input(text) else {
                return false;
            };
            d.filter_date = date;
            d.page = 0;
            d.step = Step::History;
        }
        Step::Time => {
            let Some(time) = time_input(text) else {
                return false;
            };
            d.time = time;
            advance(d, Step::Comment);
        }
        Step::Comment => {
            d.comment = if text.trim() == "-" {
                String::new()
            } else {
                text.trim().chars().take(500).collect()
            };
            advance(d, Step::Photo);
        }
        Step::Photo if text.trim() == "-" => {
            d.photo = None;
            advance(d, Step::Review);
        }
        _ => return false,
    }
    true
}

fn accepts(d: &Draft, action: &str) -> bool {
    use Step::*;
    match action {
        "back" => !matches!(d.step, Menu | Done | Cancel),
        "cancel" => is_entry(d.step),
        "menu" => matches!(
            d.step,
            Summary
                | Balance
                | Rates
                | Help
                | Undo
                | Goals
                | Goal
                | History
                | HistDate
                | HistCustom
                | TxView
        ),
        _ => match d.step {
            Menu => {
                matches!(
                    action,
                    "balance" | "summary" | "history" | "goals" | "rates" | "undo" | "help"
                ) || matches!(action, "kind:expense" | "kind:income" | "kind:transfer")
            }
            Account | ToAccount | Category => {
                action.starts_with("pick:") || action.starts_with("page:")
            }
            Currency => action.starts_with("cur:"),
            ConvAmount => action == "conv:rate",
            Date => matches!(
                action,
                "date:today" | "date:yesterday" | "date:tomorrow" | "date:other"
            ),
            Time => action == "time:now" || (action == "save_now" && !d.editing),
            Comment | Photo => action == "skip" || (action == "save_now" && !d.editing),
            Review => matches!(action, "save" | "edit"),
            Edit => {
                (d.existing && action == "save")
                    || matches!(
                        action,
                        "edit:amount"
                            | "edit:account"
                            | "edit:choice"
                            | "edit:date"
                            | "edit:time"
                            | "edit:comment"
                            | "edit:photo"
                    )
                    || (action == "edit:currency" && d.kind != "transfer")
            }
            Summary => {
                matches!(
                    action,
                    "sum:day" | "sum:yesterday" | "sum:week" | "sum:month"
                ) || (action == "chart" && !d.period.is_empty())
            }
            Rates => action == "refresh",
            Undo => action == "undo_yes" && d.undo_id.is_some(),
            Goals => action.starts_with("goal:") || action.starts_with("page:"),
            Goal => matches!(
                action,
                "gop:add" | "gop:sub" | "gop:target" | "gop:rename" | "gop:archive"
            ),
            History => {
                action.starts_with("tx:")
                    || action.starts_with("page:")
                    || matches!(action, "hist:pick" | "hist:clear")
            }
            HistDate => matches!(
                action,
                "hdate:today" | "hdate:yesterday" | "hdate:tomorrow" | "hdate:other"
            ),
            TxView => matches!(action, "txedit" | "txdel"),
            TxDelete => action == "txdel_yes",
            Cancel => matches!(action, "discard" | "continue"),
            _ => false,
        },
    }
}

async fn download(ctx: &Ctx, id: &str) -> Result<Vec<u8>, String> {
    let file = ctx
        .tg
        .call::<Value>("getFile", &json!({"file_id":id}))
        .await?
        .ok_or("missing file")?;
    if file["file_size"].as_u64().unwrap_or(0) > 10 * 1024 * 1024 {
        return Err("file too large".into());
    }
    let path = file["file_path"].as_str().ok_or("missing file path")?;
    let mut response = ctx
        .tg
        .http
        .get(format!(
            "https://api.telegram.org/file/bot{}/{}",
            ctx.tg.token, path
        ))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| "download failed")?
        .error_for_status()
        .map_err(|_| "download failed")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "download failed")? {
        if bytes.len() + chunk.len() > 10 * 1024 * 1024 {
            return Err("file too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn save(state: &AppState, ctx: &Ctx, chat: i64, d: &mut Draft) -> Result<(), String> {
    account(state, &d.account)?;
    if let Some(id) = &d.destination {
        account(state, id)?;
    }
    let bytes = match &d.photo {
        Some(id) => Some(download(ctx, id).await?),
        None => None,
    };
    persist_entry(state, ctx, chat, d, bytes.as_deref())
}

fn bot_actor(state: &AppState, ctx: &Ctx) -> Actor {
    Actor {
        user_id: ctx.db_user.clone(),
        device_id: Some(state.db.device_id.clone()),
        source: Source::Bot,
    }
}

fn persist_entry(
    state: &AppState,
    ctx: &Ctx,
    chat: i64,
    d: &mut Draft,
    bytes: Option<&[u8]>,
) -> Result<(), String> {
    if !(d.step == Step::Review || (d.existing && d.step == Step::Edit)) {
        return Err("confirmation required".into());
    }
    let actor = bot_actor(state, ctx);
    let mut done = d.clone();
    done.step = Step::Done;
    let occurred = format!("{}T{}:00", d.date, d.time);
    // Foreign-currency entry: the account gets `converted`, the entered amount is kept as orig_*.
    let (amount, orig_amount) = match (&d.currency, d.converted) {
        (None, _) => (d.amount, None),
        (Some(_), Some(converted)) if d.kind != "transfer" => (converted, Some(d.amount)),
        (Some(_), _) => return Err("account amount required".into()),
    };
    state.db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let alive: i64 = tx.query_row("SELECT COUNT(*) FROM transactions WHERE id=?1 AND deleted_at IS NULL", [&d.id], |r| r.get(0))?;
        let exists: i64 = tx.query_row("SELECT COUNT(*) FROM transactions WHERE id=?1", [&d.id], |r| r.get(0))?;
        let mut payload = json!({"id":d.id,"kind":d.kind,"occurredAt":occurred,"accountId":d.account,"amountMinor":amount,"categoryId":d.category,"toAccountId":d.destination,"toAmountMinor":d.credited,"comment":d.comment,"origAmountMinor":orig_amount,"origCurrency":orig_amount.and(d.currency.clone())});
        let write = if d.existing {
            if alive == 0 { return Err(finance_storage::Error::NotFound("transaction".into())); }
            true
        } else {
            payload["source"] = "bot".into();
            payload["createdByUserId"] = json!(ctx.db_user);
            exists == 0
        };
        if write {
            repos::transaction_upsert(&tx, &state.db, &actor, payload)?;
            if let Some(bytes) = &bytes { repos::attachment_add(&tx, &state.db, &actor, &d.id, "receipt.jpg", "image/jpeg", bytes)?; }
        }
        tx.execute("UPDATE tg_sessions SET state_json=?1, updated_at=?2 WHERE chat_id=?3", finance_storage::rusqlite::params![serde_json::to_string(&done)?, now_iso(), chat])?;
        tx.commit()?;
        Ok(())
    }).map_err(|e| format!("save failed: {e}"))?;
    *d = done;
    Ok(())
}

fn undo_id(state: &AppState, ctx: &Ctx) -> Result<Option<String>, String> {
    state.db.with_conn(|c| {
        use finance_storage::rusqlite::OptionalExtension;
        Ok(c.query_row("SELECT id FROM transactions WHERE source='bot' AND created_by_user_id=?1 AND deleted_at IS NULL AND julianday(created_at) >= julianday(?2) ORDER BY created_at DESC LIMIT 1", finance_storage::rusqlite::params![ctx.db_user, (chrono::Utc::now()-chrono::Duration::minutes(10)).to_rfc3339()], |r| r.get(0)).optional()?)
    }).map_err(|_| "undo lookup failed".into())
}

fn current_screen(d: &Draft, message_id: i64, revision: u64, action: &str) -> bool {
    d.message_id == message_id && d.revision == revision && accepts(d, action)
}

/// Opens a top-level screen on a fresh draft (menu buttons, commands, pending actions).
fn open_screen(state: &AppState, ctx: &Ctx, d: &mut Draft, action: &str) -> Result<(), String> {
    match action {
        "balance" => d.step = Step::Balance,
        "summary" => d.step = Step::Summary,
        "history" => d.step = Step::History,
        "goals" => d.step = Step::Goals,
        "rates" => d.step = Step::Rates,
        "help" => d.step = Step::Help,
        "undo" => {
            d.undo_id = undo_id(state, ctx)?;
            d.step = Step::Undo;
        }
        _ if action.starts_with("kind:") => {
            d.kind = action[5..].into();
            d.step = Step::Amount;
        }
        _ if action.starts_with("sum:") => {
            d.step = Step::Summary;
            d.period = action[4..].into();
        }
        _ => d.step = Step::Menu,
    }
    Ok(())
}

pub(super) async fn callback(state: &AppState, ctx: &Ctx, cb: &TgCallback) -> Result<(), String> {
    let Some(message) = &cb.message else {
        ctx.tg.answer(&cb.id, "").await;
        return Ok(());
    };
    if cb.data.as_deref().is_some_and(|s| s.starts_with("rem:")) {
        return super::notify::reminder_callback(state, ctx, cb).await;
    }
    let chat = message.chat.id;
    let mut d = load(state, chat)?;
    let parsed = cb
        .data
        .as_deref()
        .and_then(|s| s.split_once(':'))
        .and_then(|(r, a)| r.parse::<u64>().ok().map(|r| (r, a)));
    let Some((revision, action)) = parsed else {
        ctx.tg.answer(&cb.id, &tr(&ctx.lang, "stale")).await;
        return Ok(());
    };
    if ctx.db_user.is_none()
        || cb.from.id != chat
        || !current_screen(&d, message.message_id, revision, action)
    {
        ctx.tg.answer(&cb.id, &tr(&ctx.lang, "stale")).await;
        return Ok(());
    }
    ctx.tg.answer(&cb.id, "").await;
    d.notice.clear();
    if let Err(e) = action_apply(state, ctx, chat, &mut d, action).await {
        super::log(&format!("callback {action}: {e}"));
        d.notice = tr(&ctx.lang, "failed");
    }
    ensure_conv_rates(state, &d).await;
    render(state, ctx, chat, &mut d, false).await?;
    if d.step == Step::Done {
        let mut menu = d.fresh();
        menu.message_id = 0;
        render(state, ctx, chat, &mut menu, true).await?;
    }
    Ok(())
}

fn update_goal(state: &AppState, ctx: &Ctx, id: &str, patch: Value) -> Result<(), String> {
    let mut patch = patch;
    patch["id"] = id.into();
    let actor = bot_actor(state, ctx);
    state
        .db
        .with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            repos::goal_upsert(&tx, &state.db, &actor, patch)?;
            tx.commit()?;
            Ok(())
        })
        .map_err(|e| format!("goal update failed: {e}"))
}

async fn action_apply(
    state: &AppState,
    ctx: &Ctx,
    chat: i64,
    d: &mut Draft,
    action: &str,
) -> Result<(), String> {
    match action {
        "menu" => *d = d.fresh(),
        "discard" => {
            let pending = std::mem::take(&mut d.pending);
            *d = d.fresh();
            open_screen(state, ctx, d, &pending)?;
        }
        "back" => {
            d.step = if d.editing {
                d.editing = false;
                Step::Review
            } else {
                back_step(d)
            };
            if d.step == Step::Menu && d.amount > 0 && !d.existing {
                d.previous = Step::Amount;
                d.step = Step::Cancel;
            }
            d.page = 0;
        }
        "cancel" => {
            d.previous = d.step;
            d.step = Step::Cancel;
        }
        "continue" => {
            d.pending.clear();
            d.step = d.previous;
        }
        "balance" | "summary" | "history" | "goals" | "rates" | "help" | "undo" => {
            *d = d.fresh();
            open_screen(state, ctx, d, action)?;
        }
        _ if action.starts_with("kind:") => {
            *d = d.fresh();
            open_screen(state, ctx, d, action)?;
        }
        _ if action.starts_with("sum:") => {
            d.period = action[4..].into();
        }
        "chart" => {
            let (from, to) = report::period(&d.period, report::today());
            match report::chart(state, &ctx.lang, from, to)? {
                Some((png, caption)) => ctx.tg.send_photo(chat, png, &caption).await?,
                None => d.notice = tr(&ctx.lang, "chart_empty"),
            }
        }
        "refresh" => {
            crate::commands::refresh_rates(state).await?;
        }
        "undo_yes" => {
            if let Some(id) = &d.undo_id {
                let actor = bot_actor(state, ctx);
                let removed = state.db.with_conn(|c| {
                    let tx = c.unchecked_transaction()?;
                    let valid: i64 = tx.query_row("SELECT COUNT(*) FROM transactions WHERE id=?1 AND created_by_user_id=?2 AND source='bot' AND deleted_at IS NULL AND julianday(created_at)>=julianday(?3)", finance_storage::rusqlite::params![id,ctx.db_user,(chrono::Utc::now()-chrono::Duration::minutes(10)).to_rfc3339()], |r|r.get(0))?;
                    if valid == 1 { repos::apply(&state.db,&tx,&actor,Entity::Transactions,finance_storage::Op::Delete,id,None)?; }
                    tx.commit()?; Ok(valid == 1)
                }).map_err(|_|"undo failed")?;
                *d = d.fresh();
                d.notice = tr(&ctx.lang, if removed { "undone" } else { "no_undo" });
            }
        }
        "skip" => {
            if d.step == Step::Comment {
                d.comment.clear();
                advance(d, Step::Photo);
            } else {
                d.photo = None;
                advance(d, Step::Review);
            }
        }
        "save_now" => {
            d.step = Step::Review;
            save(state, ctx, chat, d).await?;
        }
        "save" => save(state, ctx, chat, d).await?,
        "edit" => d.step = Step::Edit,
        "time:now" => {
            d.time = now_hm();
            advance(d, Step::Comment);
        }
        _ if action.starts_with("edit:") => {
            d.editing = true;
            d.step = match &action[5..] {
                "amount" => Step::Amount,
                "account" => Step::Account,
                "choice" if d.kind == "transfer" => Step::ToAccount,
                "choice" => Step::Category,
                "currency" => Step::Currency,
                "date" => Step::Date,
                "time" => Step::Time,
                "comment" => Step::Comment,
                _ => Step::Photo,
            };
        }
        _ if action.starts_with("cur:") => {
            let cur = &action[4..];
            if cur.len() != 3 || !cur.chars().all(|c| c.is_ascii_uppercase()) {
                return Err("bad currency".into());
            }
            if cur == account_currency(state, d)? {
                d.currency = None;
                d.converted = None;
                advance(d, Step::Category);
            } else {
                d.currency = Some(cur.into());
                d.converted = None;
                d.step = Step::ConvAmount;
            }
        }
        "conv:rate" => {
            d.converted = Some(suggested(state, d)?.ok_or("no rate")?);
            advance(d, Step::Category);
        }
        _ if action.starts_with("date:") => {
            if action == "date:other" {
                d.step = Step::CustomDate;
            } else {
                d.date = relative_date(action, report::today()).ok_or("invalid date action")?;
                d.dated = true;
                advance(d, Step::Time);
            }
        }
        _ if action.starts_with("page:") => {
            let page = action[5..].parse::<usize>().map_err(|_| "bad page")?;
            if page.saturating_mul(PAGE) < choices(state, d)?.len() {
                d.page = page;
            }
        }
        _ if action.starts_with("pick:") => {
            let id = &action[5..];
            if !choices(state, d)?.iter().any(|r| r["id"] == id) {
                return Err("unavailable choice".into());
            }
            match d.step {
                Step::Account => {
                    d.account = id.into();
                    if d.kind == "transfer" {
                        d.destination = None;
                        d.credited = None;
                        d.editing = false;
                        d.step = Step::ToAccount;
                    } else {
                        // The currency step follows every account choice (also while editing).
                        d.step = Step::Currency;
                    }
                }
                Step::Category => {
                    d.category = Some(id.into());
                    advance(d, Step::Date);
                }
                Step::ToAccount => {
                    d.destination = Some(id.into());
                    d.credited = None;
                    let different =
                        account(state, &d.account)?["currency"] != account(state, id)?["currency"];
                    if different {
                        d.step = Step::ToAmount;
                    } else {
                        advance(d, Step::Date);
                    }
                }
                _ => {}
            }
            d.page = 0;
        }
        _ if action.starts_with("goal:") => {
            let id = &action[5..];
            find(state, Entity::Goals, id)?
                .filter(active)
                .ok_or("goal unavailable")?;
            d.goal = Some(id.into());
            d.step = Step::Goal;
        }
        "gop:archive" => {
            let id = d.goal.clone().ok_or("goal unavailable")?;
            update_goal(state, ctx, &id, json!({"archived": 1}))?;
            d.goal = None;
            d.step = Step::Goals;
            d.notice = tr(&ctx.lang, "goal_archived");
        }
        _ if action.starts_with("gop:") => {
            d.goal_op = action[4..].into();
            d.step = Step::GoalInput;
        }
        "hist:pick" => d.step = Step::HistDate,
        "hist:clear" => {
            d.filter_date.clear();
            d.page = 0;
        }
        _ if action.starts_with("hdate:") => {
            if action == "hdate:other" {
                d.step = Step::HistCustom;
            } else {
                d.filter_date =
                    relative_date(&action[1..], report::today()).ok_or("invalid date action")?;
                d.page = 0;
                d.step = Step::History;
            }
        }
        _ if action.starts_with("tx:") => {
            d.tx = Some(action[3..].into());
            d.step = Step::TxView;
        }
        "txedit" => {
            let id = d.tx.clone().ok_or("transaction unavailable")?;
            let row = state
                .db
                .with_conn(|c| finance_storage::db::row_json(c, "transactions", &id))
                .map_err(|_| "lookup failed")?
                .filter(|r| r["deletedAt"].is_null())
                .ok_or("transaction unavailable")?;
            let receipts: i64 = state
                .db
                .with_conn(|c| {
                    Ok(c.query_row(
                        "SELECT COUNT(*) FROM attachments WHERE transaction_id=?1 AND deleted_at IS NULL",
                        [&id],
                        |r| r.get(0),
                    )?)
                })
                .unwrap_or(0);
            let mut edit = draft_from_row(&row, receipts > 0);
            edit.message_id = d.message_id;
            edit.revision = d.revision;
            edit.keyboard = d.keyboard;
            edit.tx = Some(id);
            edit.filter_date = d.filter_date.clone();
            edit.step = Step::Edit;
            *d = edit;
        }
        "txdel" => d.step = Step::TxDelete,
        "txdel_yes" => {
            let id = d.tx.clone().ok_or("transaction unavailable")?;
            let actor = bot_actor(state, ctx);
            state
                .db
                .with_conn(|c| {
                    let tx = c.unchecked_transaction()?;
                    repos::apply(
                        &state.db,
                        &tx,
                        &actor,
                        Entity::Transactions,
                        finance_storage::Op::Delete,
                        &id,
                        None,
                    )?;
                    tx.commit()?;
                    Ok(())
                })
                .map_err(|e| format!("delete failed: {e}"))?;
            d.tx = None;
            d.step = Step::History;
            d.notice = tr(&ctx.lang, "deleted");
        }
        _ => {}
    }
    Ok(())
}

fn goal_input(state: &AppState, ctx: &Ctx, d: &mut Draft, text: &str) -> Result<bool, String> {
    let id = d.goal.clone().ok_or("goal unavailable")?;
    let goal = find(state, Entity::Goals, &id)?.ok_or("goal unavailable")?;
    let patch = if d.goal_op == "rename" {
        let name: String = text.trim().chars().take(100).collect();
        if name.is_empty() {
            return Ok(false);
        }
        json!({"name": name})
    } else {
        let Some(n) = super::parse_minor(text).filter(|n| *n > 0) else {
            return Ok(false);
        };
        let current = goal["currentMinor"].as_i64().unwrap_or(0);
        match d.goal_op.as_str() {
            "add" => json!({"currentMinor": current.checked_add(n).ok_or("overflow")?}),
            "sub" => json!({"currentMinor": current.saturating_sub(n).max(0)}),
            _ => json!({"targetMinor": n}),
        }
    };
    update_goal(state, ctx, &id, patch)?;
    d.step = Step::Goal;
    d.notice = tr(&ctx.lang, "goal_saved");
    Ok(true)
}

pub(super) async fn message(state: &AppState, tg: &Tg, m: &TgMessage) -> Result<(), String> {
    let chat = m.chat.id;
    let user = linked_user(state, chat);
    if user.is_none() {
        return Ok(());
    }
    let ctx = Ctx {
        tg: tg.clone(),
        lang: user_lang(state, user.as_ref()),
        db_user: user,
    };
    let mut d = load(state, chat)?;
    let text = m.text.as_deref().unwrap_or("");
    d.notice.clear();
    if text.trim().starts_with("/start") || d.keyboard < KEYBOARD_VERSION {
        send_keyboard(&ctx, chat, &mut d).await?;
    }
    if let Some(action) = command(text) {
        let busy = is_entry(d.step) && (d.amount > 0 || d.existing);
        if busy {
            d.previous = d.step;
            d.step = Step::Cancel;
            d.pending = if action == "menu" {
                String::new()
            } else {
                action.into()
            };
        } else if d.step == Step::Cancel {
            d.pending = if action == "menu" {
                String::new()
            } else {
                action.into()
            };
        } else {
            d = d.fresh();
            open_screen(state, &ctx, &mut d, action)?;
        }
    } else if d.step == Step::Photo && m.photo.is_some() {
        if let Some(photo) = m.photo.as_ref().and_then(|p| p.last()) {
            match download(&ctx, &photo.file_id).await {
                Ok(_) => {
                    d.photo = Some(photo.file_id.clone());
                    advance(&mut d, Step::Review);
                }
                Err(e) => {
                    super::log(&format!("receipt download: {e}"));
                    d.notice = tr(&ctx.lang, "photo_failed");
                }
            }
        }
    } else if d.step == Step::GoalInput {
        if !goal_input(state, &ctx, &mut d, text)? {
            d.notice = tr(
                &ctx.lang,
                if d.goal_op == "rename" {
                    "goal_name_hint"
                } else {
                    "goal_amount_hint"
                },
            );
        }
    } else if !apply_text(&mut d, text) {
        d.notice = tr(
            &ctx.lang,
            match d.step {
                Step::CustomDate | Step::HistDate | Step::HistCustom => "date_hint",
                Step::Time => "time_hint",
                Step::Amount | Step::ToAmount | Step::ConvAmount => "amount_hint",
                _ => "welcome",
            },
        );
    }
    ensure_conv_rates(state, &d).await;
    render(state, &ctx, chat, &mut d, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_time_and_navigation() {
        let mut d = Draft {
            step: Step::Amount,
            ..Draft::default()
        };
        assert!(!apply_text(&mut d, "0"));
        assert!(!apply_text(&mut d, "-2"));
        assert!(apply_text(&mut d, "12,50"));
        assert_eq!(d.amount, 1250);
        assert_eq!(d.step, Step::Account);
        assert_eq!(back_step(&d), Step::Amount);
        d.step = Step::Time;
        assert!(!apply_text(&mut d, "25:00"));
        assert!(apply_text(&mut d, "9:05"));
        assert_eq!((d.time.as_str(), d.step), ("09:05", Step::Comment));
        assert_eq!(back_step(&d), Step::Time);
        assert!(apply_text(&mut d, "-"));
        assert_eq!(d.step, Step::Photo);
        assert!(accepts(&d, "save_now"));
        assert!(apply_text(&mut d, "-"));
        assert_eq!(d.step, Step::Review);
        assert!(accepts(&d, "save"));
        assert!(!accepts(&d, "date:today"));
        assert!(!accepts(&d, "save_now"));
    }

    #[test]
    fn date_buttons_and_custom_dates() {
        let mut d = Draft {
            step: Step::Date,
            ..Draft::default()
        };
        for a in [
            "date:today",
            "date:yesterday",
            "date:tomorrow",
            "date:other",
        ] {
            assert!(accepts(&d, a));
        }
        assert!(!accepts(&d, "date:unknown"));
        for kind in ["expense", "income"] {
            d = Draft {
                kind: kind.into(),
                step: Step::CustomDate,
                ..Draft::default()
            };
            assert!(apply_text(&mut d, "04.11"));
            assert_eq!(d.step, Step::Time);
            assert!(d.dated && d.date.ends_with("-11-04"));
        }
        d.step = Step::Time;
        assert!(accepts(&d, "time:now") && accepts(&d, "save_now"));
        d.editing = true;
        assert!(!accepts(&d, "save_now"));
    }

    #[test]
    fn history_date_filter() {
        let mut d = Draft {
            step: Step::History,
            ..Draft::default()
        };
        assert!(accepts(&d, "hist:pick") && accepts(&d, "tx:x") && accepts(&d, "page:1"));
        assert!(!accepts(&d, "hdate:today"));
        d.step = Step::HistDate;
        assert_eq!(back_step(&d), Step::History);
        for a in [
            "hdate:today",
            "hdate:yesterday",
            "hdate:tomorrow",
            "hdate:other",
        ] {
            assert!(accepts(&d, a));
        }
        assert!(!accepts(&d, "hdate:unknown") && !accepts(&d, "tx:x"));
        assert!(accepts(&d, "menu"));
        assert!(apply_text(&mut d, "05.11"));
        assert_eq!(d.step, Step::History);
        assert!(d.filter_date.ends_with("-11-05"));
        assert!(accepts(&d, "hist:clear") && accepts(&d, "menu"));
        d.step = Step::HistCustom;
        assert_eq!(back_step(&d), Step::HistDate);
        assert!(!apply_text(&mut d, "32.13"));
        assert!(apply_text(&mut d, "01.02.2025"));
        assert_eq!(
            (d.step, d.filter_date.as_str()),
            (Step::History, "2025-02-01")
        );
        d.step = Step::TxView;
        assert_eq!(back_step(&d), Step::History);
    }

    #[test]
    fn obsolete_callbacks_never_change_a_draft() {
        let mut d = Draft {
            step: Step::Date,
            message_id: 17,
            revision: 3,
            ..Draft::default()
        };
        assert!(current_screen(&d, 17, 3, "date:today"));
        assert!(!current_screen(&d, 16, 3, "date:today"));
        assert!(!current_screen(&d, 17, 2, "date:today"));
        assert!(!current_screen(&d, 17, 3, "save"));
        d.step = Step::Review;
        assert!(current_screen(&d, 17, 3, "save"));
        d.step = Step::Done;
        assert!(!current_screen(&d, 17, 3, "save"));
        assert!(!current_screen(&d, 17, 3, "menu"));
    }

    #[test]
    fn commands_and_reply_keyboard() {
        assert_eq!(command("/menu"), Some("menu"));
        assert_eq!(command("/menu@FinanceBot"), Some("menu"));
        assert_eq!(command("/week"), Some("sum:week"));
        assert_eq!(command(&tr("ru", "expense")), Some("kind:expense"));
        assert_eq!(command(&tr("en", "balance")), Some("balance"));
        assert_eq!(command(&tr("be", "menu")), Some("menu"));
        assert_eq!(command("12,50"), None);
        let kb = reply_keyboard("ru");
        assert_eq!(kb["is_persistent"], true);
        assert_eq!(kb["keyboard"][0][0]["text"], tr("ru", "expense"));
    }

    #[test]
    fn screens_navigation() {
        let mut d = Draft {
            step: Step::Goal,
            ..Draft::default()
        };
        assert_eq!(back_step(&d), Step::Goals);
        assert!(accepts(&d, "gop:add") && accepts(&d, "menu") && !accepts(&d, "cancel"));
        d.step = Step::GoalInput;
        assert_eq!(back_step(&d), Step::Goal);
        assert!(!accepts(&d, "menu"));
        d.step = Step::TxDelete;
        assert_eq!(back_step(&d), Step::TxView);
        assert!(accepts(&d, "txdel_yes") && !accepts(&d, "txedit"));
        d.step = Step::Summary;
        assert!(!accepts(&d, "chart"));
        d.period = "week".into();
        assert!(accepts(&d, "chart"));
        d.step = Step::Undo;
        assert!(!accepts(&d, "undo_yes"));
        d.existing = true;
        d.step = Step::Edit;
        assert_eq!(back_step(&d), Step::TxView);
        assert!(accepts(&d, "save") && accepts(&d, "edit:time"));
        let fresh = Draft {
            message_id: 5,
            revision: 9,
            keyboard: 1,
            amount: 100,
            ..Draft::default()
        }
        .fresh();
        assert_eq!(
            (
                fresh.message_id,
                fresh.revision,
                fresh.keyboard,
                fresh.amount
            ),
            (5, 9, 1, 0)
        );
    }

    #[test]
    fn transfer_back_and_edit() {
        let mut d = Draft {
            kind: "transfer".into(),
            step: Step::Date,
            ..Draft::default()
        };
        assert_eq!(back_step(&d), Step::ToAccount);
        d.credited = Some(200);
        assert_eq!(back_step(&d), Step::ToAmount);
        d.editing = true;
        d.step = Step::Amount;
        assert!(apply_text(&mut d, "4"));
        assert_eq!(d.step, Step::Review);
        assert!(!d.editing);
        d.step = Step::Done;
        assert!(!accepts(&d, "save"));
        assert!(!accepts(&d, "kind:income"));
    }

    struct Fixture {
        state: Option<AppState>,
        dir: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("finance-wizard-{}", finance_storage::db::new_id()));
            let db = finance_storage::db::Db::open(&dir.join("test.db"), Some(&[7; 32])).unwrap();
            Self {
                state: Some(AppState {
                    db,
                    db_existed: false,
                    new_recovery_key: None,
                }),
                dir,
            }
        }
        fn state(&self) -> &AppState {
            self.state.as_ref().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.state.take());
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn setup(f: &Fixture) -> (Ctx, Draft) {
        let state = f.state();
        let (user, acc, cat) = state
            .db
            .with_conn(|c| {
                let actor = Actor {
                    user_id: None,
                    device_id: Some(state.db.device_id.clone()),
                    source: Source::Bot,
                };
                let u = repos::user_upsert(c, &state.db, &actor, json!({"name":"<Alice>"}))?;
                let a = repos::account_upsert(
                    c,
                    &state.db,
                    &actor,
                    json!({"name":"Cash","kind":"cash","currency":"BYN"}),
                )?;
                let cat = repos::category_upsert(
                    c,
                    &state.db,
                    &actor,
                    json!({"name":"Food","kind":"expense","icon":"shopping-cart"}),
                )?;
                Ok((
                    u["id"].as_str().unwrap().to_string(),
                    a["id"].as_str().unwrap().to_string(),
                    cat["id"].as_str().unwrap().to_string(),
                ))
            })
            .unwrap();
        let ctx = Ctx {
            tg: Tg {
                http: reqwest::Client::new(),
                token: String::new(),
            },
            db_user: Some(user),
            lang: "en".into(),
        };
        let d = Draft {
            step: Step::Review,
            account: acc,
            category: Some(cat),
            amount: 1250,
            dated: true,
            time: "14:35".into(),
            ..Draft::default()
        };
        (ctx, d)
    }

    fn count(state: &AppState, sql: &str) -> i64 {
        state
            .db
            .with_conn(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?))
            .unwrap()
    }

    #[test]
    fn confirmation_is_atomic_and_idempotent() {
        let f = Fixture::new();
        let state = f.state();
        let (ctx, mut d) = setup(&f);
        store(state, 42, &d).unwrap();
        assert_eq!(count(state, "SELECT COUNT(*) FROM transactions"), 0);
        d.step = Step::Photo;
        assert!(persist_entry(state, &ctx, 42, &mut d, None).is_err());
        assert_eq!(count(state, "SELECT COUNT(*) FROM transactions"), 0);
        d.step = Step::Review;
        let mut retry = d.clone();
        persist_entry(state, &ctx, 42, &mut d, None).unwrap();
        assert_eq!(d.step, Step::Done);
        assert_eq!(load(state, 42).unwrap().step, Step::Done);
        persist_entry(state, &ctx, 42, &mut retry, None).unwrap();
        assert_eq!(count(state, "SELECT COUNT(*) FROM transactions"), 1);
        assert_eq!(
            count(
                state,
                "SELECT COUNT(*) FROM transactions WHERE occurred_at LIKE '%T14:35:00'"
            ),
            1
        );
        assert_eq!(undo_id(state, &ctx).unwrap(), Some(d.id));
    }

    #[test]
    fn editing_an_existing_entry_updates_in_place() {
        let f = Fixture::new();
        let state = f.state();
        let (ctx, mut d) = setup(&f);
        store(state, 42, &d).unwrap();
        persist_entry(state, &ctx, 42, &mut d, None).unwrap();
        let row = state
            .db
            .with_conn(|c| finance_storage::db::row_json(c, "transactions", &d.id))
            .unwrap()
            .unwrap();
        let mut edit = draft_from_row(&row, false);
        assert_eq!((edit.amount, edit.time.as_str()), (1250, "14:35"));
        edit.step = Step::Edit;
        edit.amount = 990;
        edit.comment = "fixed".into();
        persist_entry(state, &ctx, 42, &mut edit, None).unwrap();
        assert_eq!(count(state, "SELECT COUNT(*) FROM transactions"), 1);
        assert_eq!(
            count(
                state,
                "SELECT amount_minor FROM transactions WHERE comment='fixed'"
            ),
            990
        );
        assert!(
            count(
                state,
                "SELECT COUNT(*) FROM audit_log WHERE entity='transactions'"
            ) >= 2
        );
        let details = tx_details(state, &d.id, "en").unwrap().unwrap();
        assert!(details.contains("🤖 bot") && details.contains("&lt;Alice&gt;"));
        state
            .db
            .with_conn(|c| {
                repos::apply(
                    &state.db,
                    c,
                    &bot_actor(state, &ctx),
                    Entity::Transactions,
                    finance_storage::Op::Delete,
                    &d.id,
                    None,
                )?;
                Ok(())
            })
            .unwrap();
        edit.step = Step::Edit;
        assert!(persist_entry(state, &ctx, 42, &mut edit, None).is_err());
        assert!(tx_details(state, &d.id, "en").unwrap().is_none());
    }

    #[test]
    fn foreign_currency_income_on_a_usd_card() {
        let f = Fixture::new();
        let state = f.state();
        let (ctx, mut d) = setup(&f);
        let usd = state
            .db
            .with_conn(|c| {
                let actor = bot_actor(state, &ctx);
                repos::rate_upsert(c, &state.db, "2026-11-04", "USD", 3_250_000, 1)?;
                let a = repos::account_upsert(
                    c,
                    &state.db,
                    &actor,
                    json!({"name":"Visa","kind":"card","currency":"USD","last4":"1234"}),
                )?;
                Ok(a["id"].as_str().unwrap().to_string())
            })
            .unwrap();
        // 65,00 BYN arrive on the USD card.
        d.step = Step::Account;
        d.amount = 6500;
        d.account = usd.clone();
        d.step = Step::Currency;
        assert!(accepts(&d, "cur:BYN") && !accepts(&d, "pick:x"));
        let (_, keys) = view(state, &d, &ctx).unwrap();
        let first = &keys["inline_keyboard"][0][0];
        assert!(first["text"].as_str().unwrap().contains("USD"));
        assert!(keys.to_string().contains("cur:BYN"));
        d.currency = Some("BYN".into());
        d.step = Step::ConvAmount;
        assert_eq!(suggested(state, &d).unwrap(), Some(2000));
        let (text, keys) = view(state, &d, &ctx).unwrap();
        assert!(text.contains("USD?") && keys.to_string().contains("conv:rate"));
        assert_eq!(back_step(&d), Step::Currency);
        assert!(!apply_text(&mut d, "abc"));
        assert!(apply_text(&mut d, "19,90"));
        assert_eq!((d.converted, d.step), (Some(1990), Step::Category));
        assert_eq!(back_step(&d), Step::ConvAmount);
        d.step = Step::Review;
        let (text, _) = view(state, &d, &ctx).unwrap();
        assert!(text.contains("65,00\u{a0}BYN") && text.contains("19,90\u{a0}USD"));
        persist_entry(state, &ctx, 42, &mut d, None).unwrap();
        state
            .db
            .with_conn(|c| {
                let (amount, orig, cur): (i64, i64, String) = c.query_row(
                    "SELECT amount_minor, orig_amount_minor, orig_currency FROM transactions",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                assert_eq!((amount, orig, cur.as_str()), (1990, 6500, "BYN"));
                Ok(())
            })
            .unwrap();
        let row = state
            .db
            .with_conn(|c| finance_storage::db::row_json(c, "transactions", &d.id))
            .unwrap()
            .unwrap();
        let mut edit = draft_from_row(&row, false);
        assert_eq!(
            (edit.amount, edit.currency.as_deref(), edit.converted),
            (6500, Some("BYN"), Some(1990))
        );
        // Editing the amount asks for the account amount again.
        edit.editing = true;
        edit.step = Step::Amount;
        assert!(apply_text(&mut edit, "70"));
        assert_eq!((edit.step, edit.converted), (Step::ConvAmount, None));
        edit.step = Step::Review;
        assert!(persist_entry(state, &ctx, 42, &mut edit, None).is_err());
    }

    #[test]
    fn receipt_uses_exact_id_and_failure_keeps_draft() {
        let f = Fixture::new();
        let state = f.state();
        let (ctx, mut d) = setup(&f);
        store(state, 42, &d).unwrap();
        let mut bad = d.clone();
        bad.category = Some("missing".into());
        assert!(persist_entry(state, &ctx, 42, &mut bad, None).is_err());
        assert_eq!(load(state, 42).unwrap().step, Step::Review);
        persist_entry(state, &ctx, 42, &mut d, Some(b"receipt-test")).unwrap();
        state
            .db
            .with_conn(|c| {
                let id: String =
                    c.query_row("SELECT transaction_id FROM attachments", [], |r| r.get(0))?;
                assert_eq!(id, d.id);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn goals_are_editable() {
        let f = Fixture::new();
        let state = f.state();
        let (ctx, _) = setup(&f);
        let id = state
            .db
            .with_conn(|c| {
                let g = repos::goal_upsert(
                    c,
                    &state.db,
                    &bot_actor(state, &ctx),
                    json!({"name":"Trip","targetMinor":100000}),
                )?;
                Ok(g["id"].as_str().unwrap().to_string())
            })
            .unwrap();
        let mut d = Draft {
            step: Step::GoalInput,
            goal: Some(id.clone()),
            goal_op: "add".into(),
            ..Draft::default()
        };
        assert!(!goal_input(state, &ctx, &mut d, "abc").unwrap());
        assert!(goal_input(state, &ctx, &mut d, "400").unwrap());
        assert_eq!(d.step, Step::Goal);
        d.goal_op = "sub".into();
        assert!(goal_input(state, &ctx, &mut d, "1000").unwrap());
        assert_eq!(count(state, "SELECT current_minor FROM goals"), 0);
        d.goal_op = "rename".into();
        assert!(goal_input(state, &ctx, &mut d, "Sea").unwrap());
        let (text, _) = view(state, &d, &ctx).unwrap();
        assert!(text.contains("Sea") && text.contains("▱▱▱▱▱▱▱▱▱▱"));
    }

    #[test]
    fn rendering_favourites_and_locales() {
        let f = Fixture::new();
        let state = f.state();
        let (mut ctx, mut d) = setup(&f);
        let cat = d.category.clone().unwrap();
        persist_entry(state, &ctx, 42, &mut d.clone(), None).unwrap();
        for lang in ["ru", "be", "en"] {
            ctx.lang = lang.into();
            for step in [
                Step::Menu,
                Step::Balance,
                Step::Rates,
                Step::Summary,
                Step::History,
                Step::Goals,
                Step::Help,
                Step::Review,
                Step::Edit,
                Step::Time,
            ] {
                d.step = step;
                d.period = "month".into();
                let (text, keys) = view(state, &d, &ctx).unwrap();
                if step == Step::Menu {
                    assert!(text.contains("&lt;Alice&gt;"));
                }
                if step == Step::Review {
                    assert!(text.contains("🔴 −12,50") && text.contains("🛒 Food"));
                }
                for row in keys["inline_keyboard"].as_array().unwrap() {
                    for b in row.as_array().unwrap() {
                        assert!(b["callback_data"].as_str().unwrap().len() <= 64);
                    }
                }
            }
        }
        state
            .db
            .with_conn(|c| {
                for n in 0..12 {
                    repos::category_upsert(
                        c,
                        &state.db,
                        &bot_actor(state, &ctx),
                        json!({"name":format!("Category {n}"),"kind":"expense"}),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        d.step = Step::Category;
        let all = choices(state, &d).unwrap();
        assert_eq!(all[0]["id"], cat.as_str());
        assert_eq!(all[0]["fav"], true);
        let (_, first) = view(state, &d, &ctx).unwrap();
        assert!(first.to_string().contains("page:1") && first.to_string().contains("⭐"));
        d.page = 1;
        let (_, last) = view(state, &d, &ctx).unwrap();
        assert!(last.to_string().contains("page:0"));
        d.step = Step::ToAccount;
        assert!(choices(state, &d).unwrap().is_empty());
        assert!(parse_minor("9223372036854775807").is_none());
    }
}
