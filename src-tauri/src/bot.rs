//! Telegram bot (host-side): pairing by code, button-driven expense/income/
//! transfer entry, summaries, 10-minute undo and a one-line fallback format.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri::Manager;

use finance_storage::db::now_iso;
use finance_storage::{models::Source, repos, Actor, Entity};

use crate::state::AppState;

const KEY_SERVICE: &str = "by.kostyan.finance";
const TG_ACCOUNT: &str = "bot-token";

// ---------- token storage ----------

pub fn save_token(token: &str) -> Result<(), String> {
    keyring::Entry::new(KEY_SERVICE, TG_ACCOUNT)
        .map_err(|e| e.to_string())?
        .set_secret(token.as_bytes())
        .map_err(|e| e.to_string())
}

fn read_token() -> Option<String> {
    keyring::Entry::new(KEY_SERVICE, TG_ACCOUNT)
        .ok()?
        .get_secret()
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn delete_token() {
    if let Ok(e) = keyring::Entry::new(KEY_SERVICE, TG_ACCOUNT) {
        let _ = e.delete_credential();
    }
}

// ---------- telegram api ----------

#[derive(Deserialize)]
struct TgResp<T> {
    ok: bool,
    result: Option<T>,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct TgUser {
    id: i64,
    username: Option<String>,
}

#[derive(Deserialize)]
struct TgMessage {
    message_id: i64,
    text: Option<String>,
    chat: TgChat,
    #[allow(dead_code)]
    from: Option<TgUser>,
    photo: Option<Vec<TgPhoto>>,
    caption: Option<String>,
}

#[derive(Deserialize)]
struct TgPhoto {
    file_id: String,
}

#[derive(Deserialize)]
struct TgChat {
    id: i64,
}

#[derive(Deserialize)]
struct TgCallback {
    id: String,
    data: Option<String>,
    message: Option<TgMessage>,
    #[allow(dead_code)]
    from: TgUser,
}

#[derive(Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
    callback_query: Option<TgCallback>,
}

#[derive(Clone)]
struct Tg {
    http: reqwest::Client,
    token: String,
}

impl Tg {
    fn base(&self) -> String {
        format!("https://api.telegram.org/bot{}", self.token)
    }
    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<Option<T>, String> {
        let r: TgResp<T> = self
            .http
            .post(format!("{}/{}", self.base(), method))
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if !r.ok {
            return Err(format!("{method}: {}", r.description));
        }
        Ok(r.result)
    }
    async fn send(&self, chat: i64, text: &str, kb: Option<Value>) -> Result<(), String> {
        let mut b = json!({"chat_id": chat, "text": text});
        if let Some(kb) = kb {
            b["reply_markup"] = kb;
        }
        self.call::<Value>("sendMessage", &b).await.map(|_| ())
    }
    #[allow(dead_code)]
    async fn edit(&self, chat: i64, msg: i64, text: &str, kb: Option<Value>) -> Result<(), String> {
        let mut b = json!({"chat_id": chat, "message_id": msg, "text": text});
        if let Some(ref kb) = kb {
            b["reply_markup"] = kb.clone();
        }
        let _ = self.call::<Value>("editMessageText", &b).await;
        self.send(chat, text, kb).await.ok();
        Ok(())
    }
    async fn answer(&self, id: &str, text: &str) {
        let _ = self
            .call::<Value>(
                "answerCallbackQuery",
                &json!({"callback_query_id": id, "text": text}),
            )
            .await;
    }
    async fn updates(&self, offset: i64) -> Result<Vec<TgUpdate>, String> {
        let r: Option<Vec<TgUpdate>> = self
            .call("getUpdates", &json!({"offset": offset, "timeout": 25, "allowed_updates": ["message", "callback_query"]}))
            .await?;
        Ok(r.unwrap_or_default())
    }
}

// ---------- i18n (compact bot strings) ----------

fn bt(lang: &str, key: &str) -> String {
    let (ru, be, en) = match key {
        "menu" => (
            "Меню: расход, доход, перевод, баланс, сводка, отмена.",
            "Меню: выдатак, даход, перавод, баланс, зводка, адмена.",
            "Menu: expense, income, transfer, balance, summary, undo.",
        ),
        "kind_exp" => ("− Расход", "− Выдатак", "− Expense"),
        "kind_inc" => ("+ Доход", "+ Даход", "+ Income"),
        "day" => ("Сегодня", "Сёння", "Today"),
        "week" => ("Неделя", "Тыдзень", "Week"),
        "month" => ("Месяц", "Месяц", "Month"),
        "linked" => (
            "Привязано! Пишите сумму или используйте /menu.",
            "Звязана! Пішыце суму або выкарыстоўвайце /menu.",
            "Linked! Type an amount or use /menu.",
        ),
        "badcode" => (
            "Неверный или просроченный код.",
            "Няправільны або пратэрмінаваны код.",
            "Invalid or expired code.",
        ),
        "amount" => (
            "Сумма? (напр. 12.50)",
            "Сума? (напр. 12.50)",
            "Amount? (e.g. 12.50)",
        ),
        "account" => ("Счёт:", "Рахунак:", "Account:"),
        "category" => ("Категория:", "Катэгорыя:", "Category:"),
        "to" => ("Куда перевести:", "Куды перавесці:", "Transfer to:"),
        "comment" => (
            "Комментарий (или «-»):",
            "Каментар (або «-»):",
            "Comment (or «-»):",
        ),
        "photo" => (
            "Фото чека или «-»:",
            "Фота чэка або «-»:",
            "Receipt photo or «-»:",
        ),
        "done" => ("Записано ✅", "Запісана ✅", "Saved ✅"),
        "undone" => (
            "Последняя запись удалена.",
            "Апошні запіс выдалены.",
            "Last record deleted.",
        ),
        "noundo" => (
            "Нечего отменять (старая запись или не найдено).",
            "Няма чаго адмяняць.",
            "Nothing to undo.",
        ),
        "badline" => (
            "Формат: «-500 продукты [комментарий]», «+1000 зп», или /menu.",
            "Фармат: «-500 прадукты [каментар]», «+1000 зп» або /menu.",
            "Format: «-500 groceries [note]», «+1000 salary» or /menu.",
        ),
        "choose" => ("Выберите:", "Абярыце:", "Choose:"),
        "date" => ("Дата:", "Дата:", "Date:"),
        "today" => ("Сегодня", "Сёння", "Today"),
        "yesterday" => ("Вчера", "Учора", "Yesterday"),
        "other" => ("Другая дата", "Іншая дата", "Other date"),
        "datefmt" => (
            "Дата в формате ДД.ММ.ГГГГ:",
            "Дата ў фармаце ДД.ММ.ГГГГ:",
            "Date as DD.MM.YYYY:",
        ),
        "tamount" => ("Зачислено:", "Залічана:", "Credited:"),
        _ => ("?", "?", "?"),
    };
    match lang {
        "be" => be.to_string(),
        "en" => en.to_string(),
        _ => ru.to_string(),
    }
}

fn kb(rows: Vec<Vec<(String, String)>>) -> Value {
    json!({
        "inline_keyboard": rows.into_iter().map(|r| {
            r.into_iter().map(|(t, d)| json!({"text": t, "callback_data": d})).collect::<Vec<_>>()
        }).collect::<Vec<_>>()
    })
}

// ---------- state machine ----------

struct Ctx {
    tg: Tg,
    db_user: Option<String>, // linked users.id
    lang: String,
}

struct Sess {
    state: String, // "idle"|"amount"|"account"|"category"|"to_account"|"to_amount"|"date"|"date_text"|"comment"|"photo"
    kind: String,
    amount_minor: i64,
    account_id: String,
    to_account_id: Option<String>,
    to_amount_minor: Option<i64>,
    category_id: Option<String>,
    comment: String,
    occurred_at: Option<String>,
    photo_file_id: Option<String>,
    msg_id: i64,
}

impl Sess {
    fn idle() -> Self {
        Self {
            state: "idle".into(),
            kind: "expense".into(),
            amount_minor: 0,
            account_id: String::new(),
            to_account_id: None,
            to_amount_minor: None,
            category_id: None,
            comment: String::new(),
            occurred_at: None,
            photo_file_id: None,
            msg_id: 0,
        }
    }
    fn to_json(&self) -> String {
        json!({"state": self.state, "kind": self.kind, "amount": self.amount_minor, "acc": self.account_id, "toAcc": self.to_account_id, "toAmt": self.to_amount_minor, "cat": self.category_id, "comment": self.comment, "occ": self.occurred_at, "photo": self.photo_file_id, "msg": self.msg_id}).to_string()
    }
    fn from_json(s: &str) -> Self {
        let v: Value = serde_json::from_str(s).unwrap_or(json!({}));
        Self {
            state: v["state"].as_str().unwrap_or("idle").into(),
            kind: v["kind"].as_str().unwrap_or("expense").into(),
            amount_minor: v["amount"].as_i64().unwrap_or(0),
            account_id: v["acc"].as_str().unwrap_or("").into(),
            to_account_id: v["toAcc"].as_str().map(|x| x.to_string()),
            to_amount_minor: v["toAmt"].as_i64(),
            category_id: v["cat"].as_str().map(|x| x.to_string()),
            comment: v["comment"].as_str().unwrap_or("").into(),
            occurred_at: v["occ"].as_str().map(|x| x.to_string()),
            photo_file_id: v["photo"].as_str().map(|x| x.to_string()),
            msg_id: v["msg"].as_i64().unwrap_or(0),
        }
    }
}

fn load_sess(state: &AppState, chat: i64) -> Sess {
    state
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT state_json FROM tg_sessions WHERE chat_id=?1",
                [chat],
                |r| r.get::<_, String>(0),
            )
            .map(|s| Sess::from_json(&s))
            .map_err(|e| e.into())
        })
        .unwrap_or_else(|_| Sess::idle())
}

fn save_sess(state: &AppState, chat: i64, s: &Sess) {
    let _ = state.db.with_conn(|c| {
        c.execute("INSERT OR REPLACE INTO tg_sessions (chat_id, state_json, updated_at) VALUES (?1,?2,?3)",
            finance_storage::rusqlite::params![chat, s.to_json(), now_iso()])?;
        Ok(())
    });
}

fn linked_user(state: &AppState, chat: i64) -> Option<String> {
    state
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT user_id FROM telegram_links WHERE chat_id=?1",
                [chat],
                |r| r.get(0),
            )
            .map_err(|e| e.into())
        })
        .ok()
}

fn user_lang(state: &AppState, _user_id: Option<&String>) -> String {
    state
        .db
        .with_conn(|c| repos::setting_get(c, "language"))
        .ok()
        .flatten()
        .unwrap_or_else(|| "ru".into())
        .replace(|_| false, "")
}

fn accounts_kb(state: &AppState, prefix: &str) -> Value {
    let accs = state
        .db
        .with_conn(|c| repos::list_rows(c, Entity::Accounts, false))
        .unwrap_or_default();
    let rows: Vec<Vec<(String, String)>> = accs
        .iter()
        .map(|a| {
            let mut name = a["name"].as_str().unwrap_or("?").to_string();
            if let Some(l4) = a["last4"].as_str() {
                name.push_str(&format!(" ·{l4}"));
            }
            name.push_str(&format!(" ({})", a["currency"].as_str().unwrap_or("")));
            vec![(name, format!("{prefix}{}", a["id"].as_str().unwrap()))]
        })
        .collect();
    kb(rows)
}

fn categories_kb(state: &AppState, kind: &str, prefix: &str) -> Value {
    let cats = state
        .db
        .with_conn(|c| repos::list_rows(c, Entity::Categories, false))
        .unwrap_or_default();
    let lang = "ru";
    let _ = lang;
    let filtered: Vec<_> = cats
        .iter()
        .filter(|c| c["kind"].as_str() == Some(kind))
        .collect();
    let mut rows: Vec<Vec<(String, String)>> = Vec::new();
    let mut row: Vec<(String, String)> = Vec::new();
    for c in filtered.iter() {
        let name = c["name"].as_str().unwrap_or("?").to_string();
        row.push((name, format!("{prefix}{}", c["id"].as_str().unwrap())));
        if row.len() == 2 {
            rows.push(std::mem::take(&mut row));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    kb(rows)
}

fn parse_minor(s: &str) -> Option<i64> {
    let t = s.trim().replace([' ', '\u{a0}'], "").replace(',', ".");
    let (i, f) = t.split_once('.').unwrap_or((&t, ""));
    if i.is_empty()
        || !i.chars().all(|c| c.is_ascii_digit())
        || f.len() > 2
        || !f.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(i.parse::<i64>().ok()? * 100 + format!("{:0<2}", f).parse::<i64>().ok()?)
}

async fn send_summary(
    state: &AppState,
    ctx: &Ctx,
    chat: i64,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let rows = state
        .db
        .with_conn(|c| {
            repos::transaction_list(
                c,
                &repos::TransactionFilter {
                    from: Some(from.into()),
                    to: Some(to.into()),
                    limit: Some(5000),
                    ..Default::default()
                },
            )
        })
        .map_err(|e| e.to_string())?;
    let mut e = 0i64;
    let mut i = 0i64;
    for x in &rows {
        match x["kind"].as_str() {
            Some("expense") => e += x["amountMinor"].as_i64().unwrap_or(0),
            Some("income") => i += x["amountMinor"].as_i64().unwrap_or(0),
            _ => {}
        }
    }
    let msg = format!(
        "{} — {}\n− {:.2}\n+ {:.2}\n= {:.2}",
        from,
        to,
        e as f64 / 100.0,
        i as f64 / 100.0,
        (i - e) as f64 / 100.0
    );
    ctx.tg.send(chat, &msg, None).await
}

async fn commit_tx(
    state: &AppState,
    ctx: &Ctx,
    s: &Sess,
    _chat: i64,
    tg_msg: Option<i64>,
) -> Result<(), String> {
    let actor = Actor {
        user_id: ctx.db_user.clone(),
        device_id: Some(state.db.device_id.clone()),
        source: Source::Bot,
    };
    state
        .db
        .with_conn(|c| {
            repos::transaction_upsert(
                c,
                &state.db,
                &actor,
                json!({
                    "kind": s.kind, "occurredAt": s.occurred_at.clone().unwrap_or_else(now_iso),
                    "accountId": s.account_id, "amountMinor": s.amount_minor,
                    "categoryId": s.category_id, "toAccountId": s.to_account_id,
                    "toAmountMinor": s.to_amount_minor, "comment": s.comment,
                    "createdByUserId": ctx.db_user, "source": "bot", "tgMessageId": tg_msg
                }),
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

async fn handle_text(
    state: &AppState,
    ctx: &Ctx,
    chat: i64,
    text: &str,
    msg_id: i64,
) -> Result<(), String> {
    let mut s = load_sess(state, chat);
    match s.state.as_str() {
        "amount" => {
            if let Some(v) = parse_minor(text) {
                s.amount_minor = v;
                s.state = "account".into();
                save_sess(state, chat, &s);
                ctx.tg
                    .send(
                        chat,
                        &bt(&ctx.lang, "account"),
                        Some(accounts_kb(state, "acc:")),
                    )
                    .await
            } else {
                ctx.tg.send(chat, &bt(&ctx.lang, "amount"), None).await
            }
        }
        "to_amount" => {
            if let Some(v) = parse_minor(text) {
                s.to_amount_minor = Some(v);
                s.state = "date".into();
                save_sess(state, chat, &s);
                let k = vec![vec![
                    (bt(&ctx.lang, "today"), "date:today".to_string()),
                    (bt(&ctx.lang, "yesterday"), "date:yesterday".to_string()),
                    (bt(&ctx.lang, "other"), "date:other".to_string()),
                ]];
                ctx.tg.send(chat, &bt(&ctx.lang, "date"), Some(kb(k))).await
            } else {
                ctx.tg.send(chat, &bt(&ctx.lang, "amount"), None).await
            }
        }
        "comment" => {
            if text != "-" {
                s.comment = text.into();
            }
            s.state = "photo".into();
            save_sess(state, chat, &s);
            ctx.tg.send(chat, &bt(&ctx.lang, "photo"), None).await
        }
        "photo" => {
            if text == "-" {
                commit_tx(state, ctx, &s, chat, Some(msg_id)).await?;
                save_sess(state, chat, &Sess::idle());
                ctx.tg.send(chat, &bt(&ctx.lang, "done"), None).await
            } else {
                ctx.tg.send(chat, &bt(&ctx.lang, "photo"), None).await
            }
        }
        "date_text" => {
            let p: Vec<&str> = text.trim().split(['.', '/', '-']).collect();
            if p.len() == 3 {
                let (d, m, y) = (p[0], p[1], p[2]);
                if let (Ok(dd), Ok(mm), Ok(yy)) =
                    (d.parse::<u32>(), m.parse::<u32>(), y.parse::<i32>())
                {
                    if chrono::NaiveDate::from_ymd_opt(yy, mm, dd).is_some() {
                        s.occurred_at = Some(format!("{:04}-{:02}-{:02}T12:00:00", yy, mm, dd));
                        s.state = "comment".into();
                        save_sess(state, chat, &s);
                        return ctx.tg.send(chat, &bt(&ctx.lang, "comment"), None).await;
                    }
                }
            }
            ctx.tg.send(chat, &bt(&ctx.lang, "datefmt"), None).await
        }
        _ => {
            // Fallback one-line format: "-500 продукты молоко" | "+1000 зп" | "500"
            let (kind, rest) = if let Some(r) = text.strip_prefix('-') {
                ("expense", r)
            } else if let Some(r) = text.strip_prefix('+') {
                ("income", r)
            } else if parse_minor(text).is_some() {
                ("expense", text)
            } else {
                return ctx.tg.send(chat, &bt(&ctx.lang, "badline"), None).await;
            };
            let mut parts = rest.trim().splitn(2, ' ');
            let amount = parts.next().and_then(parse_minor);
            let Some(amount) = amount else {
                return ctx.tg.send(chat, &bt(&ctx.lang, "badline"), None).await;
            };
            let comment = parts.next().unwrap_or("").to_string();
            s.state = "amount_done".into();
            s.kind = kind.into();
            s.amount_minor = amount;
            s.comment = comment;
            s.state = "account".into();
            save_sess(state, chat, &s);
            ctx.tg
                .send(
                    chat,
                    &bt(&ctx.lang, "account"),
                    Some(accounts_kb(state, "acc:")),
                )
                .await
        }
    }
}

async fn handle_callback(state: &AppState, ctx: &Ctx, cb: &TgCallback) -> Result<(), String> {
    let chat = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
    let data = cb.data.clone().unwrap_or_default();
    let mut s = load_sess(state, chat);
    ctx.tg.answer(&cb.id, "").await;
    let (p, v) = data.split_once(':').unwrap_or((data.as_str(), ""));
    match p {
        "acc" => {
            s.account_id = v.into();
            if s.kind == "transfer" {
                s.state = "to_account".into();
                save_sess(state, chat, &s);
                ctx.tg
                    .send(
                        chat,
                        &bt(&ctx.lang, "to"),
                        Some(accounts_kb(state, "tacc:")),
                    )
                    .await
            } else {
                s.state = "category".into();
                save_sess(state, chat, &s);
                ctx.tg
                    .send(
                        chat,
                        &bt(&ctx.lang, "category"),
                        Some(categories_kb(state, &s.kind, "cat:")),
                    )
                    .await
            }
        }
        "tacc" => {
            s.to_account_id = Some(v.into());
            // Check if currencies differ → ask to_amount.
            let (c1, c2) = state
                .db
                .with_conn(|c| {
                    let c1: String = c.query_row(
                        "SELECT currency FROM accounts WHERE id=?1",
                        [s.account_id.as_str()],
                        |r| r.get(0),
                    )?;
                    let c2: String =
                        c.query_row("SELECT currency FROM accounts WHERE id=?1", [v], |r| {
                            r.get(0)
                        })?;
                    Ok((c1, c2))
                })
                .unwrap_or_default();
            if c1 != c2 {
                s.state = "to_amount".into();
                save_sess(state, chat, &s);
                ctx.tg
                    .send(
                        chat,
                        &format!("{} → {}: {}", c1, c2, bt(&ctx.lang, "amount")),
                        None,
                    )
                    .await
            } else {
                s.state = "date".into();
                save_sess(state, chat, &s);
                let k = vec![vec![
                    (bt(&ctx.lang, "today"), "date:today".to_string()),
                    (bt(&ctx.lang, "yesterday"), "date:yesterday".to_string()),
                    (bt(&ctx.lang, "other"), "date:other".to_string()),
                ]];
                ctx.tg.send(chat, &bt(&ctx.lang, "date"), Some(kb(k))).await
            }
        }
        "cat" => {
            s.category_id = Some(v.into());
            s.state = "date".into();
            save_sess(state, chat, &s);
            let k = vec![vec![
                (bt(&ctx.lang, "today"), "date:today".to_string()),
                (bt(&ctx.lang, "yesterday"), "date:yesterday".to_string()),
                (bt(&ctx.lang, "other"), "date:other".to_string()),
            ]];
            ctx.tg.send(chat, &bt(&ctx.lang, "date"), Some(kb(k))).await
        }
        "date" => {
            match v {
                "today" => s.occurred_at = None,
                "yesterday" => {
                    s.occurred_at = Some(format!(
                        "{}T12:00:00",
                        chrono::Local::now().date_naive() - chrono::Duration::days(1)
                    ));
                }
                _ => {
                    s.state = "date_text".into();
                    save_sess(state, chat, &s);
                    return ctx.tg.send(chat, &bt(&ctx.lang, "datefmt"), None).await;
                }
            }
            s.state = "comment".into();
            save_sess(state, chat, &s);
            ctx.tg.send(chat, &bt(&ctx.lang, "comment"), None).await
        }
        "kind" => {
            s.kind = v.into();
            s.state = "amount".into();
            save_sess(state, chat, &s);
            ctx.tg.send(chat, &bt(&ctx.lang, "amount"), None).await
        }
        _ => Ok(()),
    }
}

async fn menu(_state: &AppState, ctx: &Ctx, chat: i64) -> Result<(), String> {
    let k = vec![
        vec![
            (bt(&ctx.lang, "kind_exp"), "kind:expense".to_string()),
            (bt(&ctx.lang, "kind_inc"), "kind:income".to_string()),
            ("⇄ Перевод".to_string(), "kind:transfer".to_string()),
        ],
        vec![
            (bt(&ctx.lang, "day"), "sum:day".to_string()),
            (bt(&ctx.lang, "week"), "sum:week".to_string()),
            (bt(&ctx.lang, "month"), "sum:month".to_string()),
        ],
    ];
    ctx.tg
        .send(chat, &bt(&ctx.lang, "choose"), Some(kb(k)))
        .await
}

async fn undo(state: &AppState, ctx: &Ctx, chat: i64) -> Result<(), String> {
    let actor = Actor {
        user_id: ctx.db_user.clone(),
        device_id: Some(state.db.device_id.clone()),
        source: Source::Bot,
    };
    let id: Option<String> = state
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT id FROM transactions WHERE source='bot' AND deleted_at IS NULL
             AND created_by_user_id = ?1 AND updated_at > ?2 ORDER BY updated_at DESC LIMIT 1",
                finance_storage::rusqlite::params![
                    ctx.db_user.clone().unwrap_or_default(),
                    (chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339()
                ],
                |r| r.get(0),
            )
            .map_err(|e| e.into())
        })
        .ok();
    if let Some(id) = id {
        state
            .db
            .with_conn(|c| {
                repos::apply(
                    &state.db,
                    c,
                    &actor,
                    Entity::Transactions,
                    finance_storage::Op::Delete,
                    &id,
                    None,
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        ctx.tg.send(chat, &bt(&ctx.lang, "undone"), None).await
    } else {
        ctx.tg.send(chat, &bt(&ctx.lang, "noundo"), None).await
    }
}

async fn handle_message(state: &AppState, tg: &Tg, m: &TgMessage) -> Result<(), String> {
    let chat = m.chat.id;
    let user = linked_user(state, chat);
    let lang = user_lang(state, user.as_ref());
    let ctx = Ctx {
        tg: tg.clone(),
        db_user: user.clone(),
        lang,
    };
    let text = m.text.clone().or(m.caption.clone()).unwrap_or_default();
    let photo = m
        .photo
        .as_ref()
        .and_then(|p| p.last().map(|x| x.file_id.clone()));

    if user.is_none() {
        // Pairing flow: a 6-digit code links this chat.
        let code = text.trim();
        if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
            let uid = state
                .db
                .with_conn(|c| {
                    c.query_row(
                        "SELECT user_id FROM tg_pair_codes WHERE code=?1 AND expires_at > ?2",
                        finance_storage::rusqlite::params![code, now_iso()],
                        |r| r.get::<_, String>(0),
                    )
                    .map_err(|e| e.into())
                })
                .ok();
            if let Some(uid) = uid {
                state.db.with_conn(|c| {
                    c.execute("INSERT OR REPLACE INTO telegram_links (user_id, chat_id, linked_at) VALUES (?1,?2,?3)",
                        finance_storage::rusqlite::params![uid, chat, now_iso()])?;
                    c.execute("DELETE FROM tg_pair_codes WHERE code=?1", [code])?;
                    Ok(())
                }).ok();
                return ctx.tg.send(chat, &bt(&ctx.lang, "linked"), None).await;
            }
            return ctx.tg.send(chat, &bt(&ctx.lang, "badcode"), None).await;
        }
        return ctx.tg.send(chat, &bt(&ctx.lang, "badcode"), None).await;
    }

    match text.as_str() {
        "/start" | "/menu" | "/меню" => menu(state, &ctx, chat).await,
        "/today" => send_summary(state, &ctx, chat, &today(), &today()).await,
        "/week" => {
            let (f, t) = week_range();
            send_summary(state, &ctx, chat, &f, &t).await
        }
        "/month" => {
            let (f, t) = month_range();
            send_summary(state, &ctx, chat, &f, &t).await
        }
        "/balance" => send_balance(state, &ctx, chat).await,
        "/undo" => undo(state, &ctx, chat).await,
        _ => {
            if let Some(file_id) = photo {
                // During the "photo" step the picture completes the entry.
                let s = load_sess(state, chat);
                if s.state == "photo" {
                    commit_tx(state, &ctx, &s, chat, Some(m.message_id)).await?;
                    // Download and attach the photo to the just-created transaction.
                    if let Ok(Some(f)) = ctx
                        .tg
                        .call::<Value>("getFile", &json!({"file_id": file_id}))
                        .await
                    {
                        if let Some(fp) = f["file_path"].as_str() {
                            if let Ok(bytes) = ctx
                                .tg
                                .http
                                .get(format!(
                                    "https://api.telegram.org/file/bot{}/{}",
                                    ctx.tg.token, fp
                                ))
                                .send()
                                .await
                            {
                                if let Ok(bytes) = bytes.bytes().await {
                                    let ext = fp.rsplit('.').next().unwrap_or("jpg");
                                    let last_tx: Option<String> = state.db.with_conn(|c| {
                                        c.query_row("SELECT id FROM transactions WHERE created_by_user_id=?1 ORDER BY created_at DESC LIMIT 1",
                                            [ctx.db_user.clone().unwrap_or_default()], |r| r.get(0)).map_err(|e| e.into())
                                    }).ok();
                                    if let Some(tid) = last_tx {
                                        let actor = Actor {
                                            user_id: ctx.db_user.clone(),
                                            device_id: Some(state.db.device_id.clone()),
                                            source: Source::Bot,
                                        };
                                        let _ = state.db.with_conn(|c| {
                                            repos::attachment_add(
                                                c,
                                                &state.db,
                                                &actor,
                                                &tid,
                                                &format!("receipt.{ext}"),
                                                "image/jpeg",
                                                &bytes,
                                            )
                                        });
                                    }
                                }
                            }
                        }
                    }
                    save_sess(state, chat, &Sess::idle());
                    ctx.tg.send(chat, &bt(&ctx.lang, "done"), None).await
                } else {
                    ctx.tg.send(chat, &bt(&ctx.lang, "menu"), None).await
                }
            } else {
                handle_text(state, &ctx, chat, &text, m.message_id).await
            }
        }
    }
}

async fn send_balance(state: &AppState, ctx: &Ctx, chat: i64) -> Result<(), String> {
    let accs = state.db.with_conn(|c| {
        let mut st = c.prepare(
            "SELECT a.name, a.currency,
                    COALESCE((SELECT SUM(CASE WHEN t.kind='income' AND t.account_id=a.id THEN t.amount_minor
                                              WHEN t.kind='transfer' AND t.to_account_id=a.id THEN COALESCE(t.to_amount_minor,t.amount_minor) ELSE 0 END)
                              FROM transactions t WHERE t.deleted_at IS NULL AND (t.account_id=a.id OR t.to_account_id=a.id)),0)
                  - COALESCE((SELECT SUM(t.amount_minor) FROM transactions t
                              WHERE t.deleted_at IS NULL AND t.account_id=a.id AND t.kind IN ('expense','transfer')),0) AS bal
             FROM accounts a WHERE a.deleted_at IS NULL AND a.archived=0 ORDER BY a.sort")?;
        let v = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }).map_err(|e| e.to_string())?;
    let mut s = String::from("💼\n");
    for (n, c, b) in accs {
        s.push_str(&format!("{n}: {:.2} {c}\n", b as f64 / 100.0));
    }
    ctx.tg.send(chat, &s, None).await
}

fn today() -> String {
    now_iso()[..10].into()
}
fn week_range() -> (String, String) {
    let d = chrono::Local::now().date_naive();
    let wd = d.weekday().num_days_from_monday();
    let mon = d - chrono::Duration::days(wd as i64);
    (
        mon.to_string(),
        (mon + chrono::Duration::days(6)).to_string(),
    )
}
fn month_range() -> (String, String) {
    let d = chrono::Local::now().date_naive();
    let first = d.with_day(1).unwrap();
    let last = if d.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap() - chrono::Duration::days(1)
    } else {
        d.with_day(1).unwrap().with_month(d.month() + 1).unwrap() - chrono::Duration::days(1)
    };
    (first.to_string(), last.to_string())
}
use chrono::Datelike;

// ---------- polling ----------

static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn start_polling(app: AppHandle) {
    if RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut offset = 0i64;
        while let Some(token) = read_token() {
            let tg = Tg {
                http: reqwest::Client::new(),
                token,
            };
            match tg.updates(offset).await {
                Ok(ups) => {
                    for u in ups {
                        offset = offset.max(u.update_id + 1);
                        let app_state = app.state::<AppState>();
                        let app_state = &*app_state;
                        if let Some(m) = &u.message {
                            let _ = handle_message(app_state, &tg, m).await;
                        } else if let Some(cb) = &u.callback_query {
                            let chat = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                            let lang = user_lang(app_state, linked_user(app_state, chat).as_ref());
                            let ctx = Ctx {
                                tg: tg.clone(),
                                db_user: linked_user(app_state, chat),
                                lang,
                            };
                            if let Some(v) = cb.data.as_deref() {
                                if let Some(period) = v.strip_prefix("sum:") {
                                    let (f, t) = match period {
                                        "day" => (today(), today()),
                                        "week" => week_range(),
                                        _ => month_range(),
                                    };
                                    let _ = send_summary(app_state, &ctx, chat, &f, &t).await;
                                } else if v.starts_with("kind:")
                                    || v.starts_with("acc:")
                                    || v.starts_with("cat:")
                                    || v.starts_with("tacc:")
                                {
                                    let _ = handle_callback(app_state, &ctx, cb).await;
                                }
                            }
                        }
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_secs(5)).await,
            }
        }
        RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

// ---------- commands used by the UI ----------

pub async fn tg_connect(state: &AppState, token: String) -> Result<(String, String), String> {
    let tg = Tg {
        http: reqwest::Client::new(),
        token: token.trim().into(),
    };
    let me: Option<TgUser> = tg.call("getMe", &json!({})).await?;
    let me = me.ok_or("invalid token")?;
    save_token(token.trim())?;
    let username = me.username.unwrap_or_default();
    state
        .db
        .with_conn(|c| {
            repos::setting_set(c, "tg_bot_username", &username)?;
            repos::setting_set(c, "tg_enabled", "1")
        })
        .map_err(|e| e.to_string())?;
    Ok((username, me.id.to_string()))
}

pub fn tg_pair_code(state: &AppState, user_id: &str) -> Result<String, String> {
    let code: String = (0..6)
        .map(|_| (uuid::Uuid::new_v4().as_bytes()[0] % 10).to_string())
        .collect();
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    state.db.with_conn(|c| {
        c.execute("INSERT OR REPLACE INTO tg_pair_codes (code, user_id, expires_at) VALUES (?1,?2,?3)",
            finance_storage::rusqlite::params![code, user_id, exp])?;
        Ok(())
    }).map_err(|e| e.to_string())?;
    Ok(code)
}

pub fn tg_status(state: &AppState) -> Value {
    state
        .db
        .with_conn(|c| {
            let username = repos::setting_get(c, "tg_bot_username")?;
            let mut st = c.prepare("SELECT user_id, chat_id FROM telegram_links")?;
            let links: Vec<(String, i64)> = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            Ok(json!({
                "enabled": username.is_some(),
                "username": username,
                "linked": links,
            }))
        })
        .unwrap_or(json!({"enabled": false}))
}

/// Sends a message to every linked chat (reminders, notifications).
pub async fn notify_linked(state: &AppState, text: &str) -> Result<(), String> {
    let Some(token) = read_token() else {
        return Ok(());
    };
    let tg = Tg {
        http: reqwest::Client::new(),
        token,
    };
    let chats: Vec<i64> = state
        .db
        .with_conn(|c| {
            let mut st = c.prepare("SELECT chat_id FROM telegram_links")?;
            let v: Vec<i64> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    for chat in chats {
        let _ = tg.send(chat, text, None).await;
    }
    Ok(())
}

pub fn tg_disconnect(state: &AppState) -> Result<(), String> {
    delete_token();
    state
        .db
        .with_conn(|c| {
            c.execute("DELETE FROM telegram_links", [])?;
            repos::setting_set(c, "tg_bot_username", "")?;
            repos::setting_set(c, "tg_enabled", "0")
        })
        .map_err(|e| e.to_string())
}
