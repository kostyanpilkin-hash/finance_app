//! Telegram bot (host-side): pairing by code, card-based wizard, summaries,
//! scheduled notifications, status reporting and an error log.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use finance_storage::db::now_iso;
use finance_storage::{models::Source, repos, Actor, Entity};

use crate::state::AppState;

mod chart;
pub mod notify;
mod report;
mod text;
mod wizard;

// ---------- token storage ----------

pub fn save_token(token: &str) -> Result<(), String> {
    crate::keystore::write(crate::keystore::BOT_TOKEN, token.as_bytes())
}

fn read_token() -> Option<String> {
    crate::keystore::read_string(crate::keystore::BOT_TOKEN)
}

pub fn delete_token() {
    crate::keystore::delete(crate::keystore::BOT_TOKEN);
}

// ---------- error log & status ----------

/// Appends a line to `<appdata>/logs/bot.log` (rotated at 1 MB). Never pass secrets.
fn log(message: &str) {
    crate::applog::append("bot.log", message);
    let mut s = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    s.last_error = Some(message.chars().take(300).collect());
    s.last_error_at = Some(now_iso());
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BotStatus {
    running: bool,
    last_poll: Option<String>,
    last_message: Option<String>,
    last_error: Option<String>,
    last_error_at: Option<String>,
}

static STATUS: Mutex<BotStatus> = Mutex::new(BotStatus {
    running: false,
    last_poll: None,
    last_message: None,
    last_error: None,
    last_error_at: None,
});

fn status_update(f: impl FnOnce(&mut BotStatus)) {
    f(&mut STATUS.lock().unwrap_or_else(|e| e.into_inner()));
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
    from: Option<TgUser>,
    photo: Option<Vec<TgPhoto>>,
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

    /// Calls a Bot API method. Errors never contain the token (URLs are stripped).
    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<Option<T>, String> {
        let r: TgResp<T> = self
            .http
            .post(format!("{}/{}", self.base(), method))
            .json(body)
            .timeout(Duration::from_secs(if method == "getUpdates" {
                40
            } else {
                20
            }))
            .send()
            .await
            .map_err(|e| format!("{method}: {}", e.without_url()))?
            .json()
            .await
            .map_err(|e| format!("{method}: {}", e.without_url()))?;
        if !r.ok {
            return Err(format!("{method}: {}", r.description));
        }
        Ok(r.result)
    }

    async fn answer(&self, id: &str, text: &str) {
        let _ = self
            .call::<Value>(
                "answerCallbackQuery",
                &json!({"callback_query_id": id, "text": text}),
            )
            .await;
    }

    async fn send(&self, chat: i64, text: &str) -> Result<(), String> {
        self.call::<Value>("sendMessage", &json!({"chat_id": chat, "text": text}))
            .await
            .map(|_| ())
    }

    /// sendPhoto with a hand-built multipart body (PNG bytes + HTML caption).
    async fn send_photo(&self, chat: i64, png: Vec<u8>, caption: &str) -> Result<(), String> {
        let boundary = format!("finance-{}", uuid::Uuid::new_v4().simple());
        let mut body = Vec::with_capacity(png.len() + 1024);
        for (name, value) in [
            ("chat_id", chat.to_string()),
            ("caption", caption.to_string()),
            ("parse_mode", "HTML".to_string()),
        ] {
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                    .as_bytes(),
            );
        }
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"chart.png\"\r\nContent-Type: image/png\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(&png);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let r: TgResp<Value> = self
            .http
            .post(format!("{}/sendPhoto", self.base()))
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| format!("sendPhoto: {}", e.without_url()))?
            .json()
            .await
            .map_err(|e| format!("sendPhoto: {}", e.without_url()))?;
        if r.ok {
            Ok(())
        } else {
            Err(format!("sendPhoto: {}", r.description))
        }
    }

    async fn updates(&self, offset: i64) -> Result<Vec<TgUpdate>, String> {
        let r: Option<Vec<TgUpdate>> = self
            .call("getUpdates", &json!({"offset": offset, "timeout": 25, "allowed_updates": ["message", "callback_query"]}))
            .await?;
        Ok(r.unwrap_or_default())
    }
}

// ---------- shared helpers ----------

fn kb(rows: Vec<Vec<(String, String)>>) -> Value {
    json!({
        "inline_keyboard": rows.into_iter().map(|r| {
            r.into_iter().map(|(t, d)| json!({"text": t, "callback_data": d})).collect::<Vec<_>>()
        }).collect::<Vec<_>>()
    })
}

struct Ctx {
    tg: Tg,
    db_user: Option<String>, // linked users.id
    lang: String,
}

fn linked_user(state: &AppState, chat: i64) -> Option<String> {
    state
        .db
        .with_read_conn(|c| {
            c.query_row(
                "SELECT user_id FROM telegram_links WHERE chat_id=?1",
                [chat],
                |r| r.get(0),
            )
            .map_err(|e| e.into())
        })
        .ok()
}

/// The bot speaks the app's language (shared by both users).
fn user_lang(state: &AppState, _user_id: Option<&String>) -> String {
    state
        .db
        .with_read_conn(|c| repos::setting_get(c, "language"))
        .ok()
        .flatten()
        .filter(|l| matches!(l.as_str(), "ru" | "be" | "en"))
        .unwrap_or_else(|| "ru".into())
}

/// "12,5" | "12.50" | "1 200" → minor units; None on invalid or overflow.
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
    i.parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(format!("{:0<2}", f).parse::<i64>().ok()?)
}

// ---------- pairing (unlinked chats) ----------

async fn handle_unlinked(state: &AppState, tg: &Tg, m: &TgMessage) -> Result<(), String> {
    let chat = m.chat.id;
    // Private chats only: chat id equals the sender id.
    if chat <= 0 || m.from.as_ref().map(|u| u.id) != Some(chat) {
        return Ok(());
    }
    let lang = user_lang(state, None);
    let text = m.text.clone().unwrap_or_default();
    let code = text.trim();
    if !(code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())) {
        return tg.send(chat, &text::tr(&lang, "pair_help")).await;
    }
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
    let Some(uid) = uid else {
        return tg.send(chat, &text::tr(&lang, "badcode")).await;
    };
    state
        .db
        .write(|c| {
            c.execute(
                "INSERT OR REPLACE INTO telegram_links (user_id, chat_id, linked_at) VALUES (?1,?2,?3)",
                finance_storage::rusqlite::params![uid, chat, now_iso()],
            )?;
            c.execute("DELETE FROM tg_pair_codes WHERE code=?1", [code])?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    tg.send(chat, &text::tr(&lang, "linked")).await?;
    wizard::message(state, tg, m).await
}

// ---------- polling ----------

static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn set_commands(tg: &Tg, lang: &str) {
    let commands: Vec<Value> = [
        ("menu", "cmd_menu"),
        ("expense", "cmd_expense"),
        ("income", "cmd_income"),
        ("balance", "cmd_balance"),
        ("transfer", "cmd_transfer"),
        ("summary", "cmd_summary"),
        ("history", "cmd_history"),
        ("goals", "cmd_goals"),
        ("rates", "cmd_rates"),
        ("undo", "cmd_undo"),
        ("help", "cmd_help"),
    ]
    .into_iter()
    .map(|(c, k)| json!({"command": c, "description": text::tr(lang, k)}))
    .collect();
    if let Err(e) = tg
        .call::<Value>("setMyCommands", &json!({"commands": commands}))
        .await
    {
        log(&e);
    }
}

async fn handle_update(app_state: &AppState, tg: &Tg, u: &TgUpdate) -> Result<(), String> {
    status_update(|s| s.last_message = Some(now_iso()));
    if let Some(m) = &u.message {
        if linked_user(app_state, m.chat.id).is_some() {
            wizard::message(app_state, tg, m).await
        } else {
            handle_unlinked(app_state, tg, m).await
        }
    } else if let Some(cb) = &u.callback_query {
        let chat = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
        let user = linked_user(app_state, chat);
        let ctx = Ctx {
            tg: tg.clone(),
            lang: user_lang(app_state, user.as_ref()),
            db_user: user,
        };
        wizard::callback(app_state, &ctx, cb).await
    } else {
        Ok(())
    }
}

pub fn start_polling(app: AppHandle) {
    if RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let http = crate::net::client().clone();
        let mut offset = 0i64;
        let mut commands_for = String::new();
        status_update(|s| s.running = true);
        while let Some(token) = read_token() {
            let tg = Tg {
                http: http.clone(),
                token,
            };
            let app_state = app.state::<AppState>();
            let app_state = &*app_state;
            let lang = user_lang(app_state, None);
            if commands_for != lang {
                set_commands(&tg, &lang).await;
                commands_for = lang;
            }
            match tg.updates(offset).await {
                Ok(ups) => {
                    status_update(|s| s.last_poll = Some(now_iso()));
                    for u in ups {
                        offset = offset.max(u.update_id + 1);
                        if let Err(e) = handle_update(app_state, &tg, &u).await {
                            log(&format!("update {}: {e}", u.update_id));
                        }
                    }
                }
                Err(e) => {
                    log(&e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
        status_update(|s| s.running = false);
        RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

// ---------- commands used by the UI ----------

pub async fn tg_connect(state: &AppState, token: String) -> Result<(String, String), String> {
    let tg = Tg {
        http: crate::net::client().clone(),
        token: token.trim().into(),
    };
    let me: Option<TgUser> = tg.call("getMe", &json!({})).await?;
    let me = me.ok_or("invalid token")?;
    save_token(token.trim())?;
    let username = me.username.unwrap_or_default();
    let db = state.db.share();
    let uname = username.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.write(|c| {
            repos::setting_set(c, "tg_bot_username", &uname)?;
            repos::setting_set(c, "tg_enabled", "1")
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok((username, me.id.to_string()))
}

pub fn tg_pair_code(db: &finance_storage::db::Db, user_id: &str) -> Result<String, String> {
    let code = finance_storage::secrets::numeric_code(6);
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    db.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO tg_pair_codes (code, user_id, expires_at) VALUES (?1,?2,?3)",
            finance_storage::rusqlite::params![code, user_id, exp],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(code)
}

pub fn tg_status(db: &finance_storage::db::Db) -> Value {
    let status = STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let fresh = |t: &Option<String>| {
        t.as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() < 90)
            .unwrap_or(false)
    };
    let health = if !status.running {
        "stopped"
    } else if fresh(&status.last_poll) {
        "ok"
    } else {
        "offline"
    };
    let mut v = db
        .with_read_conn(|c| {
            let username = repos::setting_get(c, "tg_bot_username")?.filter(|u| !u.is_empty());
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
        .unwrap_or(json!({"enabled": false}));
    v["health"] = health.into();
    v["status"] = serde_json::to_value(status).unwrap_or_default();
    v["logPath"] = crate::applog::path("bot.log")
        .map(|p| p.to_string_lossy().into_owned())
        .into();
    v
}

pub fn tg_disconnect(db: &finance_storage::db::Db) -> Result<(), String> {
    delete_token();
    db.write(|c| {
        c.execute("DELETE FROM telegram_links", [])?;
        repos::setting_set(c, "tg_bot_username", "")?;
        repos::setting_set(c, "tg_enabled", "0")
    })
    .map_err(|e| e.to_string())
}
