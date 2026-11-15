# План изменений: просмотр фото чеков, синхронизация файлов, кэш связки ключей macOS

Дата: 2026-11-13. Проект: `finance_app` (git-bash, ветка `main`).

## Контекст (прочитать перед началом)

Три независимые задачи:

1. **Фото чека не видно в карточке операции.** Форма `TxForm` умеет только прикрепить
   новый файл; уже сохранённые вложения не показывает. Миниатюра 28px есть только
   в списке (`Thumb` в `src/pages/TransactionsPage.tsx`).
2. **Файлы фото не синхронизируются.** По сети ходит только строка таблицы
   `attachments`; файл живёт в `<datadir>/attachments/<sha256>` на том устройстве,
   где его добавили. API файлов не отдаёт и не принимает.
3. **macOS постоянно спрашивает пароль связки ключей.** Секреты читаются из
   keychain на каждой итерации фоновых циклов (sync каждые 5–20с в
   `client_cfg` в `src-tauri/src/sync.rs`; бот каждые ~40с в
   `src-tauri/src/bot.rs`; планировщик раз в минуту в
   `src-tauri/src/bot/notify.rs`), а «Always Allow» для ad-hoc подписи не
   запоминается.

### Ключевые правила проекта (соблюдать обязательно)

- Деньги — `i64` minor units; здесь не пригодится.
- Все записи в БД — через `storage::apply_change()` (в коде: `repos::apply` /
  `repos::*_upsert`). Прямые `UPDATE` только в миграциях и тестах.
- Секреты — только в keyring (`src-tauri/src/keystore.rs`), не в БД/логах.
- Все новые тексты UI — i18n на трёх языках: `src/i18n/locales/{ru,be,en}.ts`.
- Оверлеи — через `src/components/ui/dialog.tsx` (Esc, клик по фону, `aria-modal`).
- Проверки: `npx tsc --noEmit`, `npm run lint`, `npm test`,
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- Перед любой cargo-командой: `export PATH="tools/strawberry-perl/perl/bin:$PATH"`.
- НЕ пушить в git — только коммит.
- В конце работы: дописать `CHANGELOG.md` (по-русски, секция без версии под
  заголовком `# Журнал изменений`).

### Существующая архитектура, на которую опираемся

- `attachments` — обычная синхронизируемая таблица (`Entity::Attachments`).
  Строка: `{id, transactionId, sha256, fileName, mime, size}`.
  Файл называется по sha256 содержимого (`repos::attachment_add`,
  `crates/storage/src/repos.rs` ~604).
- API хоста — axum 0.8, `crates/api/src/lib.rs`: роуты
  `/api/v1/{hello,pair,pair/status,push,pull,resolve}`. Авторизация — Bearer
  токен, проверяется `fn auth()` (SELECT по `devices.token_hash`). Все роуты
  под middleware `lan_only` (только локальные IP).
- Клиентский цикл — `sync_once_inner()` в `src-tauri/src/sync.rs` (~264):
  сначала `POST /push`, потом `GET /pull?since=`, строки применяются через
  `dbm::write_row`. HTTP — `crate::net::client()` (reqwest).
- Команда `attachment_path` (`src-tauri/src/commands.rs` ~509): проверяет
  `valid_attachment_hash` (64 hex), возвращает путь файла или ошибку
  `"attachment missing"`. Ассет-протокол разрешает `$APPDATA/attachments/**`.
- `keyring` v3, фичи `windows-native, apple-native, sync-secret-service`.
  Обёртка `src-tauri/src/keystore.rs`: `read/read_string/write/delete`
  над `keyring::Entry::new("by.kostyan.finance", account)`. Аккаунты:
  `db-primary`, `device-token`, `bot-token`.

## Результат

- Карточка операции показывает все фото (миниатюры → полный размер в Dialog),
  кнопка удалить с тостом-отменой; список операций использует тот же компонент.
- Файлы фото ездят по LAN в обе стороны (клиент↔хост) в рамках обычного синка.
- Keychain читается один раз за запуск; три записи объединяются в одну —
  максимум одно окно пароля за запуск.

---

## Шаг 1. `crates/api`: endpoint'ы файлов

**Файл:** `crates/api/src/lib.rs` (единственный файл крейта).

Добавить в `use axum::{...}`: `extract::Path` (к `extract::{connect_info...}`),
`routing::put`, `body::Bytes`, `http::header`. В `router()` добавить роуты и
лимит тела (по умолчанию axum режет тело на 2 МБ — обязательно поднять):

```rust
const MAX_ATTACHMENT_BYTES: usize = 15 * 1024 * 1024;

// в router(), до .layer(middleware::from_fn(lan_only)):
        .route("/api/v1/attachments", get(attachment_index))
        .route("/api/v1/attachments/{sha}", get(attachment_get).put(attachment_put))
// после .with_state(state) — НЕТ, лимит вешается на роут:
```

Правильно: `DefaultBodyLimit` — на роут или на весь роутер до `.with_state`:

```rust
use axum::extract::DefaultBodyLimit;
// в router():
    Router::new()
        // ...существующие роуты...
        .route("/api/v1/attachments", get(attachment_index))
        .route("/api/v1/attachments/{sha}", get(attachment_get).put(attachment_put))
        .layer(middleware::from_fn(lan_only))
        .layer(DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES))
        .with_state(state)
```

Хелпер авторизации у каждого хендлера — скопировать паттерн `pull` (~строка 566:
`headers.get("authorization")` → `strip_prefix("Bearer ")` → `auth(c, &token)`).
Сигнатуры хендлеров (аналог `pull`: `async fn x(State(s): State<ApiState>, headers: HeaderMap) -> ApiResult`).

- `attachment_index` → `Json({"shas": [...]})`: читает директорию
  `s.db.attachments_dir`, собирает имена файлов, проходящих
  `is_sha` (`len==64 && все ascii hexdigit` — скопировать логику
  `valid_attachment_hash` из `src-tauri/src/commands.rs` в приватную fn `is_sha`).
  Авторизация обязательна (как в `pull`).
- `attachment_get(Path(sha))` → если `!is_sha` → 400 `{"error":"bad_sha"}`;
  файл отсутствует → 404 `{"error":"missing"}`; иначе `200` +
  `[(header::CONTENT_TYPE, "application/octet-stream")]` + `Vec<u8>`.
- `attachment_put(Path(sha), body: Bytes)` → `!is_sha` → 400; `body.len() == 0
  || > MAX_ATTACHMENT_BYTES` → 413 `{"error":"size"}`; sha256(body) != sha →
  400 `{"error":"hash_mismatch"}`; иначе атомарно: записать
  `attachments_dir/sha.tmp`, затем `std::fs::rename` в `sha` (если файл уже
  есть — просто 200, записывать не нужно). Ответ 200 `{"ok":true}`.
  ВАЖНО: файл без строки таблицы — строка приходит обычной синхронизацией,
  endpoint работает только с файлами.
- Ошибки БД/auth — как в `pull` (`unauthorized` → 401, прочее → 500).
  Файловые операции блокирующие — обернуть в
  `state.db.with_conn(|c| ...)` только для auth, а сам `fs::read/write` делать
  вне блокировки БД (директория не требует лока БД).

**Проверка:** `cargo check -p finance-api` → успех.

## Шаг 2. Тесты endpoint'ов

**Файл:** `crates/api/tests/attachments.rs` (новый). За образец брать
`crates/api/tests/push.rs` целиком — тот же каркас (`lan_req`, `app()`,
`seed_device`, Bearer `tok`).

Тесты:
1. `attachments_roundtrip`: PUT валидный файл → 200; GET → байты совпадают;
   GET `/attachments` → sha в списке.
2. `attachment_rejects_bad_name`: GET `/attachments/zzz` → 400.
3. `attachment_put_hash_mismatch`: PUT файл, чей sha не равен имени → 400.
4. `attachment_requires_auth`: GET без Bearer → 401.
5. `attachment_missing`: GET несуществующего sha → 404.
6. `attachment_put_dedup`: два PUT одного sha → оба 200 (второй не перезаписывает —
   проверить только статус; дедуп проверяется по наличию файла).

**Проверка:** `cargo test -p finance-api` → все тесты зелёные, включая старые.

## Шаг 3. `src-tauri/src/sync.rs`: докачка/выгрузка файлов

**Файл:** `src-tauri/src/sync.rs`.

Новая async-функция, вызывается в конце `sync_once_inner` (после применения
pull, перед `Ok(...)` ~строка 447). Ошибки файлов НЕ должны ронять синк —
каждый файл оборачивается в `let _ = ...`/лог через
`crate::applog::append("sync.log", ...)`.

```rust
async fn sync_attachments(state: &AppState, http: &reqwest::Client, host: &str, token: &str) {
    // 1. Что есть на хосте
    let resp = http.get(format!("{host}/api/v1/attachments")).bearer_auth(token)
        .send().await;
    let Ok(resp) = resp else { return };
    if !resp.status().is_success() { return; }
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let remote: std::collections::HashSet<String> = body["shas"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    // 2. Локальные не удалённые вложения + наличие локального файла
    let db = state.db.share();
    let rows: Vec<(String, String)> = crate::blocking::run(move || {
        db.with_read_conn(|c| {
            let mut st = c.prepare("SELECT id, sha256 FROM attachments WHERE deleted_at IS NULL")?;
            let v = st.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?)))?
                .collect::<Result<Vec<_>,_>>()?;
            Ok(v)
        }).map_err(|e| e.to_string())
    }).await.unwrap_or_default();

    let dir = state.db.attachments_dir.clone();
    for (_id, sha) in rows {
        let local = dir.join(&sha);
        let remote_has = remote.contains(&sha);
        if !remote_has && local.exists() {
            // 3a. Выгрузка на хост
            let bytes = match std::fs::read(&local) { Ok(b) => b, Err(_) => continue };
            let r = http.put(format!("{host}/api/v1/attachments/{sha}"))
                .bearer_auth(token).body(bytes).send().await;
            if let Err(e) = r { crate::applog::append("sync.log", &format!("attachment upload {sha}: {e}")); }
        } else if remote_has && !local.exists() {
            // 3b. Докачка с хоста (атомарно: tmp → rename)
            let r = http.get(format!("{host}/api/v1/attachments/{sha}"))
                .bearer_auth(token).send().await;
            match r {
                Ok(r) if r.status().is_success() => {
                    if let Ok(bytes) = r.bytes().await {
                        let tmp = dir.join(format!("{sha}.tmp"));
                        if std::fs::write(&tmp, &bytes).is_ok() { let _ = std::fs::rename(&tmp, &local); }
                    }
                }
                _ => {}
            }
        }
    }
}
```

Вызов в `sync_once_inner` сразу после блока применения pull (`await?` на
`crate::blocking::run` у `// 2. Pull changes.`): добавить строку
`sync_attachments(state, &http, &host, &token).await;` — по значению `Ok(...)`
возвращать `true` не требуется (синк данных уже посчитан; файлы подтянутся
следующими циклами если надо).

Внимание: `use` уже есть (`Value` и т.д.); проверить, что `reqwest::Client`
доступен — в файле уже `crate::net::client()` возвращает reqwest::Client,
в сигнатуре использовать `&reqwest::Client` (reqwest в зависимостях src-tauri).

**Проверка:** `cargo check -p finance-app` (имя пакета src-tauri — сверить в
`src-tauri/Cargo.toml` `[package] name`; если другое — `cargo check --workspace`).

## Шаг 4. `src-tauri/src/keystore.rs`: кэш + объединённая запись

**Файл:** `src-tauri/src/keystore.rs` — переписать целиком (37 строк → ~120).

Дизайн: одна keychain-запись `SERVICE/"secrets"` — JSON
`{"<account>": "<hex-секрета>", ...}`. In-memory `RwLock<BTreeMap<String,
Option<Vec<u8>>>>`; каждый аккаунт читается из ОС один раз за запуск.
Перенос со старых записей: при первом обращении, если `secrets` нет —
прочитать все три legacy-аккаунта напрямую из keyring, собрать JSON, записать
`secrets`; только после успешной записи удалить старые. При ошибке — fallback
на старые записи (ничего не удалять).

```rust
//! Единственное место обращения к keychain ОС.
use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

const SERVICE: &str = "by.kostyan.finance";
const MERGED: &str = "secrets";
pub const DB_KEY: &str = "db-primary";
pub const DEVICE_TOKEN: &str = "device-token";
pub const BOT_TOKEN: &str = "bot-token";
const LEGACY: &[&str] = &[DB_KEY, DEVICE_TOKEN, BOT_TOKEN];

// сериализация — чистые функции, покрываем тестами
fn pack(map: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let obj: serde_json::Map<String, serde_json::Value> = map.iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(v.iter().map(|b| format!("{b:02x}")).collect::<String>())))
        .collect();
    serde_json::to_vec(&serde_json::Value::Object(obj)).unwrap_or_default()
}
fn unpack(b: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) else { return BTreeMap::new() };
    let mut m = BTreeMap::new();
    if let Some(o) = v.as_object() {
        for (k, hexs) in o {
            if let Some(h) = hexs.as_str() {
                if let Some(bytes) = decode_hex(h) { m.insert(k.clone(), bytes); }
            }
        }
    }
    m
}
fn decode_hex(h: &str) -> Option<Vec<u8>> {
    if h.len() % 2 != 0 { return None; }
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).ok()).collect()
}

fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
}
fn os_read(account: &str) -> Result<Option<Vec<u8>>, String> {
    match entry(account)?.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// Карта в памяти: account → Option<секрет>. None = прочитано, записи нет.
static CACHE: OnceLock<RwLock<BTreeMap<String, Option<Vec<u8>>>>> = OnceLock::new();
fn cache() -> &'static RwLock<BTreeMap<String, Option<Vec<u8>>>> {
    CACHE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

// Одноразовая загрузка/миграция: merged → либо legacy + перенос.
static LOADED: OnceLock<()> = OnceLock::new();
fn ensure_loaded() {
    LOADED.get_or_init(|| {
        let mut map = match os_read(MERGED) {
            Ok(Some(b)) => unpack(&b).into_iter().map(|(k, v)| (k, Some(v))).collect::<BTreeMap<_,_>>(),
            _ => BTreeMap::new(),
        };
        // Миграция legacy — только для аккаунтов, которых нет в merged.
        let mut migrated = false;
        for acc in LEGACY {
            if map.contains_key(*acc) { continue; }
            match os_read(acc) {
                Ok(Some(b)) => { map.insert((*acc).into(), Some(b)); migrated = true; }
                Ok(None) => {}
                Err(_) => {} // keyring недоступен — попробуем при следующем запуске
            }
        }
        if migrated {
            let packed: BTreeMap<String, Vec<u8>> = map.iter()
                .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v))).collect();
            if entry(MERGED).and_then(|e| e.set_secret(&packed).map_err(|e| e.to_string())).is_ok() {
                for acc in LEGACY { if let Ok(e) = entry(acc) { let _ = e.delete_credential(); } }
            }
        }
        *cache().write().unwrap_or_else(|e| e.into_inner()) = map;
    });
}

fn persist() {
    let snapshot: BTreeMap<String, Vec<u8>> = cache().read().unwrap_or_else(|e| e.into_inner())
        .iter().filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v))).collect();
    let packed = pack(&snapshot);
    if let Ok(e) = entry(MERGED) { let _ = e.set_secret(&packed); }
}

pub fn read(account: &str) -> Result<Option<Vec<u8>>, String> {
    ensure_loaded();
    Ok(cache().read().unwrap_or_else(|e| e.into_inner()).get(account).cloned().flatten())
}
pub fn read_string(account: &str) -> Option<String> {
    read(account).ok().flatten().map(|b| String::from_utf8_lossy(&b).into_owned())
}
pub fn write(account: &str, secret: &[u8]) -> Result<(), String> {
    ensure_loaded();
    cache().write().unwrap_or_else(|e| e.into_inner()).insert(account.into(), Some(secret.to_vec()));
    persist();
    Ok(())
}
pub fn delete(account: &str) {
    ensure_loaded();
    cache().write().unwrap_or_else(|e| e.into_inner()).insert(account.into(), None);
    persist();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn pack_unpack_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), vec![0xde, 0xad, 0xbe, 0xef]);
        m.insert("b".to_string(), b"tok".to_vec());
        let u = unpack(&pack(&m));
        assert_eq!(u, m);
    }
    #[test] fn unpack_garbage_is_empty() {
        assert!(unpack(b"not json").is_empty());
        assert!(unpack(b"{\"a\": \"zz\"}").is_empty());
    }
}
```

Замечание по краевому случаю: `read` неизвестного аккаунта (не в карте) вернёт
`None` — это корректно («записи нет»). `os_read` ошибка при `ensure_loaded`
трактуется как пустая карта — следующий запуск повторит.

Внешний API (`read/read_string/write/delete`) не меняется → вызывающие
(`state.rs`, `bot.rs`, `sync.rs`, `commands.rs`) править НЕ нужно. Это и даёт
снижение обращений к keychain до одного за запуск на аккаунт + одного на
`secrets`.

**Проверка:** `cargo check -p finance-app` + `cargo test -p finance-app keystore`
(или `cargo test --workspace keystore`).

## Шаг 5. Фронт: компонент `TxAttachments`

**Файл (новый):** `src/components/attachments/TxAttachments.tsx` (+ рядом ничего).

Компонент полоски вложений одной операции:

```tsx
interface AttachmentRow { id: string; transactionId: string; sha256: string; fileName: string; mime?: string }
```

Пропсы: `{ txId: string; editable?: boolean }`.

- `useQuery({ queryKey: qk.entity("attachments"), queryFn: () => api.entityList<AttachmentRow>("attachments") })` — список всех, фильтр по `transactionId === txId`.
- Для каждой строки — `useQuery`-путь через `api.attachmentPath(sha256)` —
  аккуратно: hooks в цикле запрещены → вынести в дочерний `AttachmentThumb({ att })` (в этом же файле), который делает `useQuery({ queryKey: qk.attachmentPath(att.sha256), queryFn: () => api.attachmentPath(att.sha256), retry: false })`.
- `path.data` → `<img src={convertFileSrc(path.data)} className="h-14 w-14 cursor-pointer rounded-lg object-cover ring-1 ring-border" />`; клик → `setView(i)` (локальный state индекса).
- `path.isError` → квадрат-заглушка `h-14 w-14` с текстом `t("ui.photoPending")`.
- Удаление (только `editable`): маленькая `Button size="icon-sm" variant="ghost"` поверх миниатюры (absolute внутри relative-обёртки) с `Trash2` → `useMutation api.entityDelete("attachments", att.id)` → `invalidateMoney(qc)` → `toast(t("ui.photoDeleted"), { actionLabel: t("ui.undo"), onAction: () => api.entityRestore("attachments", att.id).then(() => invalidateMoney(qc)) })`. Паттерн — `TransactionsPage` строка ~80.
- Просмотр: `Dialog` (`open={view !== null}`, `title={att.fileName}`, `closeLabel={t("ui.close")}`) с `<img className="max-h-[65vh] w-auto rounded-lg object-contain" />`; если фото >1 — стрелки ←/→ (`Button ghost` с `ChevronLeft/ChevronRight`) листают `view`.
- Экспорты: `export function TxAttachments({ txId, editable })` и `export function TxThumb({ txId })` (первая миниатюра `h-7 w-7` + `+N`, без удаления/просмотра — просто картинка для списка; внутри тот же `AttachmentThumb`).

Импорты: `useQuery/useMutation/useQueryClient`, `convertFileSrc` из
`@tauri-apps/api/core`, `api`/`qk`/`invalidateMoney`, `Dialog`, `Button`,
`toast`, `useTranslation`, иконки `Trash2, ChevronLeft, ChevronRight, ImageOff`.

**Проверка:** `npx tsc --noEmit`.

## Шаг 6. Подключить `TxAttachments` в `TxForm`

**Файл:** `src/components/transactions/TxForm.tsx`.

- Импорт `TxAttachments`.
- Рядом с кнопкой «📎 Чек» (~строка 230): если `editing?.id` существует
  (редактирование, не создание) — `<TxAttachments txId={editing.id} editable />`.
- Кнопка «📎 Чек» остаётся как есть (добавление новых фото).

**Проверка:** `npx tsc --noEmit` + `npm run lint`.

## Шаг 7. Список операций: `Thumb` → `TxThumb`

**Файл:** `src/pages/TransactionsPage.tsx`.

- Удалить локальные `interface AttachmentRow` и `function Thumb` (~строки 23–35).
- Заменить `<Thumb txId={x.id} />` на `<TxThumb txId={x.id} />`; добавить импорт
  `import { TxThumb } from "@/components/attachments/TxAttachments";`.
- Убрать ставшие неиспользуемыми импорты (`convertFileSrc`, возможно
  `api.entityList`/`useQuery` если больше не нужны — lint покажет).

**Проверка:** `npx tsc --noEmit` + `npm run lint` (неиспользуемые импорты — ошибка lint).

## Шаг 8. i18n ключи

**Файлы:** `src/i18n/locales/ru.ts`, `be.ts`, `en.ts` — в объект `ui` добавить
одинаковые ключи (порядок и место — рядом с `photo`):

| Ключ | ru | be | en |
|---|---|---|---|
| `photoPending` | «загружается…» | «загрузка…» | «loading…» |
| `photoDeleted` | «Фото удалено» | «Фота выдалена» | «Photo deleted» |

(`ui.undo`, `ui.close` уже есть — проверить grep'ом.)

**Проверка:** `npx tsc --noEmit` — `be`/`en` типизированы по `Locale` из `ru.ts`,
отсутствие ключа — ошибка компиляции. Плюс `npm test` (vitest проверяет паритет
локалей, если есть такой тест — см. `src/lib/themes.test.ts`/`i18n` тесты).

## Шаг 9. CHANGELOG + полная проверка

**Файл:** `CHANGELOG.md` — под `# Журнал изменений` добавить датированную
секцию `## 2026-11-13` (без номера версии — версию проставит релизный скилл):

- Фото чеков в карточке операции (миниатюры, полный размер, удаление с отменой).
- Синхронизация файлов фото между устройствами (GET/PUT/список в API, двусторонняя докачка в клиентском цикле).
- macOS: секреты кэшируются в памяти и хранятся одной записью связки ключей — одно окно пароля за запуск.

**Финальная проверка (все должны быть зелёными):**
```
npx tsc --noEmit && npm run lint && npm test
export PATH="tools/strawberry-perl/perl/bin:$PATH"
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

## Шаг 10. Коммит (БЕЗ пуша)

```
git add -A ':!AGENTS.md' ':!docs/CHANGE_PLAN13112026.md'  # план — отдельно или вместе, по желанию
git commit -m "фото: просмотр в карточке, синк файлов, кэш keychain macOS"
```

План-файл можно коммитить вместе с кодом (он в `docs/`).

---

## Не входит в задачу (не делать)

- Удаление самих файлов с диска (нужно для «Отменить» и общих sha) — отдельная задача по файлам-сиротам.
- Apple Developer ID.
- Миграция схемы БД — не требуется (`SCHEMA_VERSION` не трогать).
- Фото в боте кроме уже существующего шага — бот не трогаем.
