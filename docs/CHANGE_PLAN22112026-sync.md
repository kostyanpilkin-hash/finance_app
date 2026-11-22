# План изменений: синхронизация, кнопки «Обновить», курсы на дашборде, журнал, Mac

Дата: 2026-11-22. Проект: корень этого репозитория (git-bash, ветка `main`).
Релиз: **0.3.1**. Схема БД **не меняется** (`SCHEMA_VERSION` остаётся 7), поэтому
0.3.1 синхронизируется с 0.3.0 и устройства можно обновлять по одному.

## Зачем

1. В «Настройки → Устройства» и на хосте, и на Mac написано «Это устройство —
   хост». Не видно, кто хост, а кто клиент, и когда была последняя синхронизация.
2. Синхронизацию нельзя запустить вручную.
3. Ошибки синхронизации непонятные (сырой текст reqwest), нигде не логируются.
4. На хосте накопилось 10 972 фиктивных конфликта по курсам (`exchange_rates`):
   данные в обеих версиях одинаковые. Они уезжают на Mac при каждом pull и
   показываются стеной карточек.
5. После ошибки синхронизации клиент опрашивает хост раз в 2 минуты, пока не
   придут новые данные. Поэтому операции с другого устройства появляются с
   задержкой, и кажется, что «не обновляется без переключения вкладок».
6. Нужно окно курсов на дашборде (с выключателем в настройках).
7. Нужна кнопка «Обновить» на страницах.
8. Журнал показывает поля, а не понятную фразу. Время в UTC (сдвиг на 3 часа).
9. На Mac в полноэкранном режиме (зелёная кнопка) на внешнем мониторе с другим
   масштабом низ окна остаётся чёрным до перехода на другую страницу.

### Решения пользователя (зафиксировано)

| # | Решение |
|---|---|
| Q1 | Настройки → Устройства: разный вид для хоста и клиента (см. задачу 3) |
| Q2 | Кнопка «Синхронизировать»: на клиенте — полный обмен с хостом; на хосте — только перечитать данные и список устройств |
| Q3 | Заготовка под будущий внешний/облачный сервер — только в коде, без UI |
| Q4 | Понятные тексты ошибок + `logs/sync.log` |
| Q5 | Чинить фиктивные конфликты: не создавать новые, вычистить накопленные, сгруппировать в UI |
| Q6 | Окно курсов: USD, EUR, RUB; БСБ приложение (покупка/продажа), БСБ карточки (покупка/продажа), Нацбанк; время обновления; «100 RUB» |
| Q7 | Окно курсов — четвёртая карточка в верхнем ряду дашборда |
| Q8 | Флажок «Курсы валют» в блоке «Диаграммы на дашборде», по умолчанию включён, на каждом устройстве свой |
| Q9 | «Обновить» перезапрашивает данные без перезагрузки окна; на клиенте ещё и синхронизирует |
| Q10 | Починить причину задержки обновления (задача 2), кнопку всё равно сделать |
| Q11 | Mac: `h-dvh` + перерисовка при изменении размера/масштаба |
| Q12 | Журнал: понятная фраза для каждого события, у изменений — поля «было → стало» |
| Q13 | Журнал: местное время, пользователь, источник (это устройство / имя устройства / хост / бот / напоминание / система) |
| Q14 | Служебные записи журнала скрыты, переключатель «Показать служебные» |
| Q15 | Фильтры журнала по пользователю и по источнику |
| Q16 | «Показать ещё» в журнале |
| Q17 | «Обновить» — иконка справа в строке заголовка страницы, крутится во время обновления |
| Q18 | Порядок — задачи ниже, по коммиту на задачу; перед релизом показать результат и спросить |

### Не цели

- Сервер в интернете не делаем — только тип-заготовка (задача 3).
- Отзыв (revoke) и переименование устройств не делаем.
- Схему БД не меняем, миграций нет.
- Старые операции и данные не трогаем (кроме пометки фиктивных конфликтов
  как решённых — это служебная таблица `conflicts`, не данные пользователя).

### Анти-цели (провал, даже если «работает»)

- Настоящий конфликт (разные данные в счёте/операции) молча теряется.
- Ручная и фоновая синхронизация идут одновременно и отправляют одну очередь дважды
  (второй раз хост увидит устаревшую версию и создаст конфликт).
- В лог или UI попадают токены устройств.
- Mac 0.3.0 перестаёт синхронизироваться с хостом 0.3.1.

## Как устроено сейчас (прочитать перед работой)

- **Режим устройства** хранится в `meta.mode` (`host`/`client`), читается один раз
  при открытии БД: `finance_storage::db::Db::mode()`, `is_host()`, `is_client()`
  (`crates/storage/src/db.rs`, `crates/storage/src/lib.rs`).
- **Хост** поднимает HTTP API (`crates/api/src/lib.rs`): `/api/v1/push`, `/pull`,
  `/resolve`, вложения. Запуск — `sync::start_host` (`src-tauri/src/sync.rs`).
- **Клиент** крутит цикл `sync::start_client` → `sync_once` → `sync_once_inner`:
  1) читает `outbox`, шлёт `POST /push`; 2) удаляет из `outbox` подтверждённые
  (`status: ok`) и конфликтные операции; 3) `GET /pull?since=<last_seq>`, пишет
  строки через `dbm::write_row`, пропускает строки с `local_dirty = 1`;
  4) сохраняет `last_seq`; 5) `INSERT OR REPLACE` всех конфликтов из ответа в
  локальную таблицу `conflicts`; 6) `sync_attachments`.
- **Последняя ошибка** — `static LAST_ERROR` в `sync.rs`, отдаётся в
  `sync_status()` вместе с `mode`, `hostUrl`, `lastSeq`, `pendingOps`,
  `pairedDevices`, `conflicts`. Команда Tauri `sync_status` в `src-tauri/src/commands.rs`.
- **Конфликт на хосте** (`push` в `crates/api/src/lib.rs`): если `op` не `delete`,
  `base_version < version строки` и строка не удалена → запись в `conflicts`,
  ответ `status: conflict`. Дедуп по `(entity_id, deviceId)` есть только для
  записей, где в `local_payload` есть `deviceId` (с 13.11). Старые записи без
  `deviceId` не дедуплицируются → 10 972 штуки.
- **Автообновление UI:** `src-tauri/src/watch.rs` каждые 2 с смотрит
  `MAX(rowid)` в `audit_log` и шлёт событие `data-changed`;
  `src/lib/useDataChanged.ts` по нему делает `invalidateQueries` для всех ключей,
  кроме `NON_DATA_ROOTS` (`src/lib/queryKeys.ts`).
- **Журнал:** `repos::journal(conn, &JournalFilter)` (`crates/storage/src/repos.rs`,
  ~строка 1173), команда `journal` в `commands.rs`, экран `src/pages/JournalPage.tsx`.
  `audit_log.ts` — UTC ISO (`dbm::now_iso()`), `source` ∈ `desktop|bot|sync|system|reminder`,
  `device_id` — устройство, где произошло изменение. Записи `audit_log` тоже
  синхронизируются (сущность `AuditLog`).
- **Настройки устройства** (локальные, не синхронизируются): таблица `settings`,
  на фронте `useLocalSettings()` (`src/pages/settings/useLocalSettings.ts`) и
  `useSettings()` (`src/lib/queries.ts`). Флажки диаграмм — `dash_chart_*`,
  список `DASH_CHARTS` в `src/pages/settings/RatesSection.tsx`.
- **Курсы:** команда `rates_overview` → `RateOverview[]` (`src/lib/types.ts`:
  `currency`, `bsbApp {buy,sell}|null`, `bsbCard {buy,sell}|null`, `nbrb|null`,
  все ×1e6 за 1 единицу). Форматирование — `formatRate`, `rateUnits` из `src/lib/fx.ts`.
  Таблица курсов уже нарисована в `RatesSection` (настройки).
- **Каркас окна:** `src/app/App.tsx` — `<div className="flex h-screen ...">`.

## Global Constraints

- Деньги — `i64`, никаких float в расчётах.
- Все тексты UI — через i18n в трёх локалях `src/i18n/locales/{ru,be,en}.ts`.
  Тип `Locale` берётся из `ru.ts` — ключи в be/en обязаны совпадать, иначе `tsc` упадёт.
- Записи пользовательских данных — только через `apply_change()`/`repos::apply`.
  Таблицы `conflicts`, `settings`, `outbox` — служебные, пишутся напрямую (как сейчас).
- Новых зависимостей не добавлять. Разрешено только включить фичу `"sync"` у уже
  подключённого `tokio` в `src-tauri/Cargo.toml` (задача 1).
- Ошибки reqwest в лог/UI — через `e.without_url()` или без URL; токены не логировать.
- Тяжёлая работа в командах Tauri — через `crate::blocking::run` / `state.read` /
  `state.write` (как соседние команды), не на IPC-потоке.
- Публичный репозиторий: в тестах только вымышленные имена (Иван, Мария), generic-пути.
- Перед cargo-командами: `export PATH="<путь к strawberry-perl>/perl/bin:$PATH"`
  (путь из AGENTS.md может не совпадать с машиной — проверить `ls`).
- Проверки после каждой задачи:
  `npx tsc --noEmit`, `npm run lint`, `npm test`,
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- Коммит после каждой задачи (сообщение указано в задаче), CHANGELOG — в задаче 8.
  Не пушить.

## Review Focus

1. Настоящий конфликт по счёту/операции по-прежнему создаётся (тест задачи 1).
2. Одинаковые данные с устаревшей версией → `ok`, без новой записи в журнале и
   без нового `seq` (тест задачи 1).
3. Ручная синхронизация во время фоновой не отправляет очередь дважды (лок, задача 1).
4. Mac 0.3.0 + хост 0.3.1: push/pull работают (поля ответа только добавляются).
5. Журнал на клиенте не показывает «эхо» своих изменений, пришедших обратно с хоста.
6. Время в журнале — местное, группировка дней — по местной дате.

---

### Task 1: Синхронизация — бэкенд (конфликты, лок, отчёт, ошибки, паузы, лог)

**Файлы:** `crates/storage/src/db.rs`, `crates/api/src/lib.rs`,
`crates/api/tests/push.rs`, `src-tauri/src/sync.rs`, `src-tauri/src/commands.rs`,
`src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`.

#### 1.1 Сравнение содержимого строки — `crates/storage/src/db.rs`

Рядом с `diff_json` добавить общий список служебных полей и функцию сравнения:

```rust
/// Sync/stamp columns that never carry user meaning.
pub const SYNC_META_KEYS: &[&str] =
    &["version", "updatedAt", "updatedByDevice", "seq", "localDirty", "createdAt"];

/// True when every non-meta field the payload carries equals the row's value.
/// Fields absent from the payload are ignored (old clients send partial rows).
pub fn same_content(payload: &Value, row: &Value) -> bool {
    let (Some(p), Some(_)) = (payload.as_object(), row.as_object()) else { return false };
    p.iter()
        .filter(|(k, _)| !SYNC_META_KEYS.contains(&k.as_str()))
        .all(|(k, v)| &row[k.as_str()] == v)
}
```

В `diff_json` заменить `matches!(k.as_str(), "version" | ...)` на
`SYNC_META_KEYS.contains(&k.as_str())` (поведение то же).

Тесты в `db.rs` (`#[cfg(test)]`, если модуля тестов нет — создать):
- `same_content_ignores_meta`: payload `{"id":"a","rate":1,"version":1,"seq":5}`,
  row `{"id":"a","rate":1,"version":3,"seq":9}` → `true`.
- `same_content_detects_change`: `rate` 1 против 2 → `false`.
- `same_content_partial_payload`: payload без поля `extra`, row с `extra` → `true`.
- `same_content_non_object`: `Value::Null` → `false`.

#### 1.2 Хост перестаёт создавать фиктивные конфликты — `push` в `crates/api/src/lib.rs`

В цикле `for op in ops` сразу после вычисления `before`, `cur_version`, `base`,
`deleted_remotely`, `is_delete` и **до** ветки конфликта вставить:

```rust
// Idempotent replay: identical data needs neither a conflict nor a new version.
if !is_delete && !deleted_remotely {
    if let (Some(b), Some(p)) = (before.as_ref(), op.payload.as_ref()) {
        if dbm::same_content(p, b) {
            results.push(json!({"id": op.id, "status": "ok"}));
            continue;
        }
    }
}
// Exchange rates are public reference data: last writer wins, never a conflict.
let never_conflicts = entity == models::Entity::ExchangeRates;
```

и условие конфликта сделать
`if !is_delete && !never_conflicts && base < cur_version && !deleted_remotely {`.
(`Entity` уже `derive(PartialEq, Eq)` — сравнение `==` работает.)

#### 1.3 Хост: разрешение конфликта закрывает дубли — `resolve` в `crates/api/src/lib.rs`

После `UPDATE conflicts SET resolved_at=?1 WHERE id=?2` добавить: пометить решёнными
все остальные нерешённые конфликты с тем же `entity_id`
(`UPDATE conflicts SET resolved_at=?1 WHERE entity_id=?2 AND resolved_at IS NULL`).
Победитель уже применён, дубли бессмысленны.

#### 1.4 Хост: разовая чистка накопленного мусора

В `crates/storage/src/db.rs` (или `repos.rs`, рядом с прочими sync-хелперами)
добавить:

```rust
/// Marks resolved every open conflict whose local payload carries no real
/// change (same_content) or that targets exchange_rates. Returns the count.
pub fn prune_noop_conflicts(conn: &Connection) -> Result<usize, Error>
```

Алгоритм: `SELECT id, entity, local_payload, server_payload FROM conflicts WHERE resolved_at IS NULL`;
для каждой строки: распарсить `local_payload` (поле `payload` внутри) и `server_payload`;
если `entity == "exchange_rates"` **или** `same_content(&local["payload"], &server)` →
собрать `id`. Затем одним проходом `UPDATE conflicts SET resolved_at=?now WHERE id=?`
(в цикле по id внутри одной транзакции). Битый JSON → пропустить строку.
**Не удалять** строки — только `resolved_at`.

Вызов на хосте — в `src-tauri/src/lib.rs` в `setup`, сразу после
`sync::migrate_device_token(...)`: новая функция `sync::prune_conflicts(&state)`
по образцу `migrate_device_token` — `state.db.write(|c| prune_noop_conflicts(c))`,
результат > 0 записать в `sync.log` («pruned N no-op conflicts»), ошибку — тоже в
лог, не падать. Вызывать **и на хосте, и на клиенте** (функция идемпотентна).

Тест в storage: создать `open_memory`, вставить 3 конфликта: (а) курс, одинаковый;
(б) счёт, одинаковый `name`; (в) счёт, разный `name` → после вызова `(а)` и `(б)`
решены, `(в)` нет, функция вернула 2.

#### 1.5 Клиент: зеркало конфликтов хоста — `sync_once_inner` в `src-tauri/src/sync.rs`

Сейчас клиент только добавляет конфликты из pull и никогда не убирает. Вынести
блок «Persist unresolved conflicts» в отдельную функцию:

```rust
/// The host's unresolved list is authoritative: upsert what it sends, mark
/// resolved what it no longer reports, and release rows those conflicts kept
/// dirty (unless the outbox still holds an op for them).
fn mirror_conflicts(c: &Connection, host: &[Value]) -> Result<(), Error>
```

Шаги внутри:
1. Как сейчас — `INSERT OR REPLACE` каждого конфликта из `host`.
2. Собрать JSON-массив id из `host`. Выбрать локальные
   `SELECT id, entity, entity_id FROM conflicts WHERE resolved_at IS NULL AND id NOT IN (SELECT value FROM json_each(?1))`.
3. Для каждой: `UPDATE conflicts SET resolved_at=now WHERE id=?`; если
   `Entity::parse(entity)` даёт сущность — `UPDATE <table> SET local_dirty=0 WHERE id=?1 AND NOT EXISTS (SELECT 1 FROM outbox WHERE entity_id=?1)`.

Вызывать вместо старого цикла. Тест в `sync.rs` (`mod tests`): in-memory
соединение с таблицами `conflicts`, `outbox`, `exchange_rates`(id, local_dirty) —
создать вручную минимальными `CREATE TABLE` как в существующем тесте `conn()`;
локально 2 конфликта (A, B), хост прислал только A → B решён, строка курса B
`local_dirty = 0`, A остался открытым.

#### 1.6 Один лок на синхронизацию

`src-tauri/Cargo.toml`: у `tokio` добавить фичу `"sync"`
(`features = ["time", "net", "sync"]`).

В `sync.rs`:

```rust
fn sync_lock() -> &'static tokio::sync::Mutex<()> {
    static L: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}
```

В начале `sync_once` — `let _guard = sync_lock().lock().await;`. Так фоновый цикл и
ручная кнопка никогда не идут параллельно.

#### 1.7 Отчёт о синхронизации и время последнего успеха

Добавить тип:

```rust
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport { pub pushed: usize, pub pulled: usize, pub has_more: bool }
```

`sync_once_inner` возвращает `Result<SyncReport, SyncError>` (см. 1.8):
`pushed = ops.len()`, `pulled = changes.len()`, `has_more = body["hasMore"].as_bool().unwrap_or(false)`.

`sync_once` на успехе: `last_error_set(None)` и пишет в `settings`
`last_sync_at = dbm::now_iso()` (через `db.write(|c| repos::setting_set(...))` в
`blocking::run`). Возвращает `Result<SyncReport, String>` наружу (строка — код
ошибки, см. 1.8). Места, где раньше ждали `bool` (`start_client`, `join_host`),
переписать на `report.pushed > 0 || report.pulled > 0` / `report.has_more`.

#### 1.8 Понятные ошибки + лог

Тип ошибки в `sync.rs`:

```rust
enum SyncError {
    Unreachable(String),   // connect / timeout / DNS
    Unauthorized,          // 401 — устройство отвязано на хосте
    Schema { code: String, host: Value, client: Value }, // 409 client_outdated / host_outdated
    Http(u16),             // прочие не-2xx
    Other(String),         // БД, разбор JSON и т.п.
}
impl SyncError {
    fn code(&self) -> &'static str  // "host_unreachable" | "unauthorized" | <schema code> | "host_error" | "error"
    fn to_json(&self) -> Value      // {"code", "message"?, "hostSchema"?, "clientSchema"?, "at": now_iso}
}
fn net_err(e: reqwest::Error) -> SyncError {
    if e.is_connect() || e.is_timeout() { SyncError::Unreachable(e.without_url().to_string()) }
    else { SyncError::Other(e.without_url().to_string()) }
}
```

Во всех местах `sync_once_inner`, где сейчас `.map_err(|e| e.to_string())?` на
`send()` — использовать `net_err`. Статусы: 401 → `Unauthorized`; 409 с кодом схемы →
`Schema`; прочие не-2xx → `Http(status)`. Ошибки БД/`blocking::run` → `Other`.
`Schema.code` — строка из ответа, а `code()` возвращает её (сделать `code()` →
`String`, если так проще).

`sync_once` на ошибке: `last_error_set(Some(err.to_json()))`; в `sync.log` писать
`"sync failed: <code>: <message>"`, **но только если код или сообщение отличаются
от предыдущей ошибки** (хранить последнюю строку в `static LAST_LOGGED: Mutex<Option<String>>`),
чтобы лог не рос каждые 20–120 с. При успехе после ошибки — одна строка
`"sync recovered"` и сброс `LAST_LOGGED`. Наружу вернуть `Err(err.code())`.

Удалить старую ветку «Schema errors already stored their structured payload above»
— теперь всё идёт через `to_json`.

Тест: `SyncError::Unreachable("x".into()).to_json()["code"] == "host_unreachable"`,
`Unauthorized` → `"unauthorized"`, `Http(500)` → `"host_error"`, у `Schema` есть
`hostSchema`/`clientSchema`.

#### 1.9 Паузы клиентского цикла (причина «не обновляется», Q10)

Вынести расчёт паузы в чистую функцию и покрыть тестом:

```rust
const IDLE: Duration = Duration::from_secs(20);
const MAX_BACKOFF: Duration = Duration::from_secs(120);
/// (sleep before next attempt, backoff to remember).
fn next_delay(r: &Result<SyncReport, String>, backoff: Duration) -> (Duration, Duration) {
    match r {
        Ok(rep) if rep.has_more || rep.pulled > 0 || rep.pushed > 0 => (Duration::ZERO, IDLE),
        Ok(_) => (IDLE, IDLE),
        Err(_) => (backoff, (backoff * 2).min(MAX_BACKOFF)),
    }
}
```

`start_client`: `let mut backoff = Duration::from_secs(5);` цикл:
`let r = sync_once(&state).await; let (sleep, nb) = next_delay(&r, backoff); backoff = nb; if !sleep.is_zero() { tokio::time::sleep(sleep).await; }`.
Главное: **успех без данных возвращает паузу к 20 с** (сейчас остаётся 120 с).

Тесты: ошибка 5 с → (5, 10); ошибка 80 → (80, 120); успех без данных при
backoff 120 → (20, 20); успех с `pulled = 3` → (0, 20).

#### 1.10 Команды `sync_now` и `devices_list`, расширенный `sync_status`

Заготовка под будущий сервер (Q3) — в `sync.rs`:

```rust
/// Where this device syncs to. Today only the LAN host; a remote/cloud server
/// would become another variant with its own transport.
enum SyncTarget { LanHost }

fn sync_target(db: &dbm::Db) -> Option<SyncTarget> {
    db.is_client().then_some(SyncTarget::LanHost)
}

/// Manual "Sync now". Host: nothing to exchange (clients pull from us) —
/// returns {"mode":"host"} so the UI just refetches. Client: drains until
/// the host reports no more pages (max 20 rounds).
pub async fn sync_now(state: &AppState) -> Result<Value, String> {
    match sync_target(&state.db) {
        None => Ok(json!({"mode": "host"})),
        Some(SyncTarget::LanHost) => {
            let mut total = SyncReport::default();
            for _ in 0..20 {
                let r = sync_once(state).await?;
                total.pushed += r.pushed;
                total.pulled += r.pulled;
                if !r.has_more { break; }
            }
            Ok(json!({"mode": "client", "pushed": total.pushed, "pulled": total.pulled, "at": dbm::now_iso()}))
        }
    }
}
```

`devices_list(db)` в `sync.rs`: `SELECT id, name, platform, paired_at, last_seen_at FROM devices WHERE revoked_at IS NULL ORDER BY paired_at`
→ `Vec<Value>` с ключами `id, name, platform, pairedAt, lastSeenAt`.

`sync_status`: добавить поля `deviceId` (`db.device_id`), `lastSyncAt`
(`settings.last_sync_at`, может быть `null`). Остальные поля не трогать.

`commands.rs` — две команды по образцу соседних:

```rust
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<Value, String> {
    crate::sync::sync_now(&state).await
}
#[tauri::command]
pub async fn devices_list(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let db = state.db.share();
    run(move || crate::sync::devices_list(&db)).await
}
```

Зарегистрировать обе в `tauri::generate_handler![...]` в `src-tauri/src/lib.rs`
(рядом с `commands::sync_status`).

#### Шаги задачи 1

- [ ] **Step 1:** написать тесты 1.1, 1.4, 1.5, 1.8, 1.9 и в
  `crates/api/tests/push.rs` три API-теста (использовать хелперы файла `app()`,
  `seed_device()`, `lan_req()`):
  - `push_identical_stale_row_is_ok_without_conflict`: создать счёт через
    `repos::apply` (версия 1), обновить ещё раз (версия 2), отправить push с
    `baseVersion: 1` и payload **равным текущей строке** → `status: "ok"`,
    `SELECT COUNT(*) FROM conflicts` = 0, `MAX(seq)` в `sync_log` не изменился.
  - `push_stale_exchange_rate_applies`: строка курса версии 2, push с
    `baseVersion: 1` и **другим** `rateBynScaled` → `ok`, курс в БД новый, конфликтов 0.
  - `push_real_conflict_still_detected`: счёт версии 2, push с `baseVersion: 1` и
    другим `name` → `status: "conflict"`, одна строка в `conflicts`.
- [ ] **Step 2:** `cargo test --workspace` → новые тесты FAIL.
- [ ] **Step 3:** реализовать 1.1–1.10.
- [ ] **Step 4:** все проверки из Global Constraints → PASS.
- [ ] **Step 5: Commit** `fix(sync): no-op conflicts, sync lock, readable errors, idle backoff, sync_now`.

---

### Task 2: Синхронизация — фронтенд (режим, кнопка, ошибки, конфликты)

**Файлы:** `src/lib/api.ts`, `src/lib/queryKeys.ts`, `src/lib/syncError.ts`,
`src/lib/syncError.test.ts` (новый), `src/pages/settings/SyncSection.tsx`,
`src/i18n/locales/{ru,be,en}.ts`.

#### 2.1 API

В `api.ts`:
- `syncStatus` — расширить тип ответа: `deviceId: string; lastSyncAt?: string | null;`
  `lastError?: { code: string; message?: string; hostSchema?: number; clientSchema?: number; at?: string } | null`.
- `syncNow: () => invoke<{ mode: "host" } | { mode: "client"; pushed: number; pulled: number; at: string }>("sync_now")`.
- `devicesList: () => invoke<{ id: string; name: string; platform: string; pairedAt: string; lastSeenAt?: string | null }[]>("devices_list")`.

В `queryKeys.ts`: `devices: ["devices"] as const`.

#### 2.2 Общий хук синхронизации — `src/lib/useSyncNow.ts` (новый)

```ts
/** Manual sync: client exchanges with the host, host only refetches.
 *  Afterwards every db-backed query is refetched. */
export function useSyncNow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.syncNow,
    onSettled: () => {
      void qc.invalidateQueries({ predicate: (q) => !NON_DATA_ROOTS.has(String(q.queryKey[0])) });
    },
  });
}
```

(Его же использует кнопка «Обновить» в задаче 3.)

#### 2.3 Тексты ошибок — `src/lib/syncError.ts`

В `syncErrorText` перед общей веткой добавить:
- `code === "host_unreachable"` → `t("ui.sync.errUnreachable")`
- `code === "unauthorized"` → `t("ui.sync.errUnauthorized")`
- `code === "host_error"` → `t("ui.sync.errHost")`
Остальное — как сейчас (`ui.syncError` с сообщением).

Тест `src/lib/syncError.test.ts` (vitest, по образцу `money.test.ts`): `t` — заглушка
`(k: string) => k`; проверить, что каждый код даёт свой ключ, а `{code:"error",message:"x"}` → `ui.syncError`.

#### 2.4 Экран «Устройства» — `SyncSection.tsx`

Разделить по `s?.mode`:

**Хост (`mode === "host"`):**
- Заголовок `t("ui.sync.thisIsHost")` — «Это устройство — хост» и подпись
  `t("ui.sync.hostHint")` — «База данных хранится на этом компьютере. Другие устройства подключаются к нему».
- Адреса хоста — как сейчас.
- Список подключённых устройств из `useQuery({ queryKey: qk.devices, queryFn: api.devicesList, refetchInterval: 15000 })`:
  строка «MacBook · macos · последняя связь: 22.11 09:41» (`lastSeenAt` → местное время
  `new Date(x).toLocaleString(i18n.language, { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })`;
  если `null` — `t("ui.sync.never")`). Пусто → `t("ui.sync.noDevices")`.
- Блок кодов привязки по пользователям — как сейчас.
- Кнопка `t("ui.sync.now")` («Синхронизировать») через `useSyncNow()`; на хосте после
  успеха показать `t("ui.sync.hostRefreshed")` — «Данные перечитаны. Устройства
  синхронизируются сами каждые 20 секунд».
- Блок «Подключить это устройство к хосту» на хосте оставить (через него хост
  может стать клиентом), но свернуть под кнопку-ссылку `t("ui.joinHost")`
  (локальный `useState(false)`), чтобы не путал.

**Клиент (`mode === "client"`):**
- Заголовок `t("ui.sync.thisIsClient")` — «Это устройство подключено к хосту».
- `t("ui.sync.host")`: `s.hostUrl`.
- `t("ui.sync.lastSync")`: местное время `s.lastSyncAt` или `t("ui.sync.never")`.
- Очередь: `s.pendingOps` + `t("ui.pendingOps")`.
- Ошибка — `syncErrorText(t, s.lastError)` красным, если есть.
- Кнопка `t("ui.sync.now")` (`loading` пока `isPending`). После успеха:
  `t("ui.sync.done", { pushed, pulled })` — «Готово: отправлено {{pushed}}, получено {{pulled}}».
  После ошибки — `syncErrorText(t, String(error))`.
- Блоки кодов привязки, адресов хоста и «Подключить к хосту» **не показывать**.
- После `syncNow` дополнительно `invalidateQueries({ queryKey: qk.syncStatus })`.

Убрать старую строку `{s?.hostUrl && <p>… seq {s.lastSeq}</p>}` (её заменил блок клиента).

#### 2.5 Конфликты — группировка (`ConflictsBlock` в том же файле)

- Сгруппировать `list.data` по `entityId` (`Map<string, Conflict[]>`), в каждой
  группе взять последний (максимальный `id` — id UUIDv7, сортируются по времени).
- Карточка на группу; если в группе > 1 — бейдж `×N`.
- Кнопки «Оставить A/B» вызывают `conflictResolve` для выбранного (последнего)
  конфликта группы. Хост сам закроет дубли (задача 1.3), клиент получит это при
  следующем pull (зеркало 1.5).
- Заголовок блока: `t("ui.conflicts")`: число **групп**.
- Показывать не больше 20 групп, дальше строка `t("ui.sync.moreConflicts", { n })`.

#### 2.6 i18n (все три файла, внутри `ui:` новый объект `sync`)

| ключ | ru | be | en |
|---|---|---|---|
| `thisIsHost` | Это устройство — хост | Гэта прылада — хост | This device is the host |
| `hostHint` | База данных хранится на этом компьютере. Другие устройства подключаются к нему. | База даных захоўваецца на гэтым камп'ютары. Іншыя прылады падключаюцца да яго. | The database lives on this computer. Other devices connect to it. |
| `thisIsClient` | Это устройство подключено к хосту | Гэта прылада падключана да хоста | This device is connected to a host |
| `host` | Хост | Хост | Host |
| `lastSync` | Последняя синхронизация | Апошняя сінхранізацыя | Last sync |
| `never` | ещё не было | яшчэ не было | never |
| `now` | Синхронизировать | Сінхранізаваць | Sync now |
| `done` | Готово: отправлено {{pushed}}, получено {{pulled}} | Гатова: адпраўлена {{pushed}}, атрымана {{pulled}} | Done: sent {{pushed}}, received {{pulled}} |
| `hostRefreshed` | Данные перечитаны. Устройства синхронизируются сами каждые 20 секунд. | Даныя перачытаны. Прылады сінхранізуюцца самі кожныя 20 секунд. | Data reloaded. Devices sync on their own every 20 seconds. |
| `devices` | Подключённые устройства | Падключаныя прылады | Connected devices |
| `noDevices` | Пока ни одного | Пакуль ніводнай | None yet |
| `lastSeen` | последняя связь | апошняя сувязь | last seen |
| `errUnreachable` | Хост недоступен. Проверьте, что устройства в одной сети, приложение на хосте запущено и VPN не блокирует локальную сеть. | Хост недаступны. Праверце, што прылады ў адной сетцы, праграма на хасце запушчана і VPN не блакуе лакальную сетку. | Host unreachable. Make sure both devices are on the same network, the app is running on the host and no VPN blocks the local network. |
| `errUnauthorized` | Хост не узнаёт это устройство. Подключите его заново. | Хост не пазнае гэтую прыладу. Падключыце яе нанова. | The host doesn't recognise this device. Pair it again. |
| `errHost` | Ошибка на стороне хоста. Подробности — в logs/sync.log на хосте. | Памылка на баку хоста. Падрабязнасці — у logs/sync.log на хасце. | Host-side error. See logs/sync.log on the host. |
| `moreConflicts` | и ещё {{n}} | і яшчэ {{n}} | and {{n}} more |

Старые ключи `ui.hostMode` и `ui.syncStatus` удалить из всех трёх локалей, если после правок они нигде не используются (`grep -rn "hostMode\|ui.syncStatus" src`).

- [ ] **Step 1:** тест `syncError.test.ts` → FAIL.
- [ ] **Step 2:** реализовать 2.1–2.6.
- [ ] **Step 3:** проверки → PASS.
- [ ] **Step 4: Commit** `feat(sync-ui): host/client view, sync now, readable errors, grouped conflicts`.

---

### Task 3: Кнопка «Обновить» на страницах

**Файлы:** `src/components/RefreshButton.tsx` (новый), страницы
`src/pages/{Dashboard,Transactions,Accounts,Analytics,Goals,Journal}Page.tsx`, локали.

Компонент:

```tsx
/** Refetches every db-backed query; on a client device also syncs with the host first. */
export function RefreshButton() {
  const { t } = useTranslation();
  const sync = useSyncNow();                 // из задачи 2
  const fetching = useIsFetching() > 0;      // @tanstack/react-query
  const busy = sync.isPending || fetching;
  return (
    <Button size="icon-sm" variant="ghost" title={t("ui.refresh")} aria-label={t("ui.refresh")}
      disabled={busy} onClick={() => sync.mutate()}>
      <RefreshCw className={cn("h-4 w-4", busy && "animate-spin")} />
    </Button>
  );
}
```

Размер `icon-sm` (h-8 w-8) уже есть в `src/components/ui/button.tsx`. Ошибку синхронизации кнопка не показывает
(она видна в настройках), но данные всё равно перечитываются (`onSettled`).

Размещение — справа в строке заголовка:
- **Dashboard:** заголовок уже в flex-строке с `Segmented` — поставить кнопку
  перед `Segmented`, обернув их в `<div className="flex items-center gap-2">`.
- **Transactions, Accounts, Goals:** заголовок уже в `flex justify-between` — поставить
  кнопку в правую группу (если справа одна кнопка — обернуть правую часть в
  `<div className="flex items-center gap-2">` и положить `RefreshButton` первым).
- **Analytics, Journal:** `<h2>` стоит отдельно — обернуть в
  `<div className="flex items-center justify-between gap-3">` с `<RefreshButton />` справа.

i18n: `ui.refresh` — «Обновить» / «Абнавіць» / «Refresh».

- [ ] **Step 1:** реализовать.
- [ ] **Step 2:** проверки → PASS. Вручную (`npm run tauri dev`): кнопка крутится, данные перечитываются.
- [ ] **Step 3: Commit** `feat(ui): refresh button on main pages`.

---

### Task 4: Окно курсов на дашборде

**Файлы:** `src/components/rates/RatesTable.tsx` (новый),
`src/pages/settings/RatesSection.tsx`, `src/pages/DashboardPage.tsx`,
`src/lib/queryKeys.ts`, локали.

#### 4.1 Вынести таблицу

Создать `RatesTable({ compact }: { compact?: boolean })`:
- Внутри `useQuery({ queryKey: qk.ratesOverview, queryFn: api.ratesOverview, staleTime: 60_000 })`
  и `useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<RateRow>() })` —
  перенести из `RatesSection` вместе с расчётом `rows`, `nbrbDate`, `bsbMissing`,
  `fmt`, `pair`.
- `compact === false` — ровно текущая разметка таблицы из `RatesSection` (все валюты).
- `compact === true` — только `["USD","EUR","RUB"]` (в этом порядке, отсутствующие
  пропустить), шрифт `text-xs`, заголовки колонок короткие:
  `t("ui.fx.app")` / `t("ui.fx.card")` / `t("ui.fx.nbrbShort")`, каждая ячейка БСБ —
  «покупка / продажа», под таблицей строка
  `t("ui.fx.updatedAt", { time })` — время из `overview.dataUpdatedAt`
  (`new Date(ms).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" })`)
  и, если `bsbMissing`, — `t("ui.fx.fallback")`.
- RUB подписан «100 RUB» (через существующий `rateUnits`).

В `queryKeys.ts` добавить `ratesOverview: ["ratesOverview"] as const` и заменить
строку `["ratesOverview"]` во всех местах (`grep -rn '"ratesOverview"' src`).

`RatesSection` теперь: кнопка «Обновить курсы» + `<RatesTable />` + строка
«Обновлено …» (`refresh.data`) — как было.

#### 4.2 Карточка на дашборде

В `DashboardPage.tsx`:
- `const ratesOn = chartOn("dash_rates");`
- Верхний ряд: если `ratesOn` —
  `className="grid gap-3 sm:grid-cols-3 xl:grid-cols-[1fr_1fr_1fr_1.6fr]"`, иначе как сейчас.
- Четвёртый элемент ряда (после «Доходы за период»), только если `ratesOn`:
  `<div className={cn(card, "sm:col-span-3 xl:col-span-1")}>` с заголовком
  `t("ui.fx.title")` (`text-xs text-muted-foreground`, как у соседних карточек) и
  `<RatesTable compact />`. На узком окне карточка уходит на отдельную строку во всю ширину.
- В скелетоне загрузки ряд не менять.

#### 4.3 Флажок в настройках

В `DASH_CHARTS` (`RatesSection.tsx`) добавить **первым** элементом
`{ key: "dash_rates", label: "settings.dashRates" }`. По умолчанию `"1"` (как у остальных — `get(key, "1")`).

#### 4.4 i18n

| ключ | ru | be | en |
|---|---|---|---|
| `ui.fx.title` | Курсы валют | Курсы валют | Exchange rates |
| `ui.fx.app` | БСБ прил. | БСБ дад. | BSB app |
| `ui.fx.card` | БСБ карты | БСБ карты | BSB card |
| `ui.fx.nbrbShort` | НБРБ | НБРБ | NBRB |
| `ui.fx.updatedAt` | обновлено {{time}} | абноўлена {{time}} | updated {{time}} |
| `settings.dashRates` | Курсы валют | Курсы валют | Exchange rates |

(Если какой-то из `ui.fx.*` уже существует с другим смыслом — выбрать другое имя.)

- [ ] **Step 1:** реализовать.
- [ ] **Step 2:** проверки → PASS. Вручную: карточка видна, флажок её скрывает,
  при ширине окна < 1280 карточка во всю ширину отдельной строкой.
- [ ] **Step 3: Commit** `feat(dashboard): exchange rates card with settings toggle`.

---

### Task 5: Журнал — бэкенд (фильтры, служебные, «это устройство»)

**Файлы:** `crates/storage/src/repos.rs`, `src-tauri/src/commands.rs`.

В `JournalFilter` добавить поля (на структуре уже стоит
`#[serde(rename_all = "camelCase", default)]`, так что из UI они приходят как
`hideSystem`, `origin`; отсутствующие = `None`):

```rust
/// Hide reference/service records (rates, settings, reminder events, system writes).
pub hide_system: Option<bool>,
/// "self" | "other" | "bot" | "reminder" | "system".
pub origin: Option<String>,
/// Filled by the command, never by the UI.
#[serde(skip_deserializing)]
pub self_device_id: Option<String>,
```

`repos::journal`:
1. В SELECT добавить флаг своего устройства:
   `..., CASE WHEN l.device_id = ?S THEN 1 ELSE 0 END AS is_self`, где `?S` —
   `self_device_id` (если `None` — пустая строка). Параметр для `?S` положить в
   `params` первым (до остальных фильтров), т.е. сформировать SQL так, чтобы
   нумерация `?N` совпадала — проще всего вызвать `push(...)` для него сразу после
   создания `push` и подставить полученное имя в SELECT через `format!`.
2. **Всегда** исключать эхо своих изменений, вернувшихся с хоста:
   `AND NOT (l.source = 'sync' AND l.device_id = ?S)`.
3. `hide_system == Some(true)` →
   `AND l.entity NOT IN ('exchange_rates','shared_settings','reminder_events','audit_log') AND l.source <> 'system'`.
4. `origin`:
   - `"self"` → `AND l.device_id = ?S AND l.source = 'desktop'`
   - `"other"` → `AND (l.device_id IS NULL OR l.device_id <> ?S) AND l.source IN ('desktop','sync')`
   - `"bot"` / `"reminder"` / `"system"` → `AND l.source = '<то же>'`
   - прочее — игнорировать.
5. `user_id`, `offset`, `limit` уже поддерживаются — не трогать.

Команда `journal` в `commands.rs`: после разбора фильтра
`f.self_device_id = Some(state.db.device_id.clone());` (сделать `let mut f`).

Тесты в `repos.rs` (по образцу существующих тестов журнала; `open_memory`, писать через `repos::apply` с разными `Actor`):
- `journal_hides_own_sync_echo`: запись с `source: Sync, device_id: self` не попадает,
  с `source: Sync, device_id: "other"` — попадает.
- `journal_hide_system`: запись `exchange_rates` скрыта при `hide_system: true`, видна при `None`.
- `journal_origin_filters`: `origin: "bot"` возвращает только bot; `"self"` — только
  `desktop` своего устройства; в строках есть `isSelf` (1/0).

- [ ] **Step 1:** тесты → FAIL. **Step 2:** реализовать. **Step 3:** проверки → PASS.
- [ ] **Step 4: Commit** `feat(journal): origin and service filters, own-device flag, hide sync echo`.

---

### Task 6: Журнал — фронтенд (сводка, кто/откуда, местное время, фильтры, «ещё»)

**Файлы:** `src/lib/journalSummary.ts` (новый), `src/lib/journalSummary.test.ts`
(новый), `src/pages/JournalPage.tsx`, `src/lib/types.ts` (`AuditRow`), `src/lib/api.ts`, локали.

#### 6.1 Типы

`AuditRow` (`src/lib/types.ts`, сейчас: id, ts, userId, source, entity, entityId, action,
beforeJson, afterJson, changesJson, uName) дополнить: `dName?: string | null; isSelf?: number; deviceId?: string | null;`.
Тип `JournalFilter` в `api.ts` найти по `grep -n "JournalFilter" src/lib`. В `api.journal` фильтр расширить полями
`hideSystem?: boolean; origin?: string; userId?: string; offset?: number`.

#### 6.2 Чистая функция сводки — `src/lib/journalSummary.ts`

```ts
export interface SummaryCtx {
  t: TFunction;
  account: (id: string) => { name: string; currency: string } | undefined;
  category: (id: string) => string | undefined;   // уже локализованное имя
  user: (id: string) => string | undefined;
}
/** One human sentence for an audit row, built from after ?? before. */
export function summarize(entity: string, action: string, row: Record<string, unknown> | null, ctx: SummaryCtx): string
/** Currency in which a money field of this row is expressed. */
export function moneyCurrency(entity: string, field: string, row: Record<string, unknown> | null, ctx: SummaryCtx): string
```

Правила `summarize` (`row` — `after ?? before`; `null` → `t("ui.entity." + entity)`):
- **transactions**, `acc = ctx.account(accountId)`, `cur = acc?.currency ?? "BYN"`:
  - `expense`: `t("journal.sum.expense", { amount: formatMinor(amountMinor, cur), category, account })`
    → «Расход 25,00 BYN · Продукты · Visa USD».
  - `income`: `t("journal.sum.income", …)` → «Доход 100,00 BYN · Репетиторство · Белкарт».
  - если есть `origAmountMinor` и `origCurrency` — дописать ` (formatMinor(orig, origCurrency))`.
  - `transfer`: `to = ctx.account(toAccountId)`;
    `t("journal.sum.transfer", { amount, from: acc.name, toAmount: formatMinor(toAmountMinor ?? amountMinor, to?.currency ?? cur), to: to.name })`
    → «Перевод 100,00 USD Visa USD → 299,05 BYN Белкарт».
  - непустой `comment` → дописать ` · «comment»` (обрезать до 60 символов + «…»).
  - нет имени счёта/категории → `"—"`.
- **accounts:** `t("journal.sum.account", { name, currency })` → «Счёт «Visa USD» · USD».
- **categories:** `t("journal.sum.category", { name })` (имя через `ctx.category(id)`, иначе `name`).
- **goals:** `t("journal.sum.goal", { name, current: formatMinor(currentMinor, currency), target: formatMinor(targetMinor, currency) })` → «Цель «Отпуск» · 150,00 из 1 000,00 USD».
- **reminders:** `t("journal.sum.reminder", { name })` + сумма, если `amountMinor`.
- **users:** `t("journal.sum.user", { name })`.
- остальное: `` `${t("ui.entity." + entity)}` `` + ` «name»`, если есть `name`.

Глагол действия показывается отдельно в строке метаданных (`ui.action.*`, уже есть).

`moneyCurrency`: для `transactions` — `toAmountMinor` → валюта `toAccountId`,
`origAmountMinor` → `origCurrency`, прочие суммы → валюта `accountId`;
для остальных сущностей → `row.currency ?? "BYN"`.

Тесты `journalSummary.test.ts` (`t` — заглушка, возвращающая `key + JSON.stringify(opts)`,
или настоящий i18n с `ru`, как удобнее; `formatMinor` настоящий): расход, доход с
исходной валютой, перевод между валютами, комментарий длиннее 60, удалённый счёт
(account → undefined → «—»), цель, `moneyCurrency` для `toAmountMinor`.

#### 6.3 Экран `JournalPage.tsx`

**Загрузка:** заменить `useQuery` на `useInfiniteQuery`:
`queryKey: qk.journal(search, entity, userId, origin, showSystem)`,
`queryFn: ({ pageParam }) => api.journal<AuditRow>({ search, entity: entity || undefined, userId: userId || undefined, origin: origin || undefined, hideSystem: !showSystem, limit: 100, offset: pageParam })`,
`initialPageParam: 0`, `getNextPageParam: (last, all) => last.length < 100 ? undefined : all.length * 100`.
Строки — `data.pages.flat()`. Внизу кнопка `t("journal.more")` («Показать ещё»),
если `hasNextPage`; `loading={isFetchingNextPage}`.

**Местное время:** день группы — `isoOf(new Date(e.ts))` (из `@/lib/money`), время —
`new Date(e.ts).toLocaleTimeString(i18n.language, { hour: "2-digit", minute: "2-digit" })`.
Убрать `e.ts.slice(0, 10)` и `e.ts.slice(11, 16)`.

**Строка записи:**
1. Иконка действия — как сейчас.
2. Первая строка (жирная, `truncate`): `summarize(e.entity, e.action, after ?? before, ctx)`.
3. Вторая строка (`text-xs text-muted-foreground`):
   `время · {t("ui.action." + action)} · {пользователь} · {источник}`, где источник:
   - `source === "bot"` → иконка `Send` + `t("journal.origin.bot")`
   - `source === "reminder"` → `Bell` + `t("journal.origin.reminder")`
   - `source === "system"` → `Cog` + `t("journal.origin.system")`
   - `isSelf === 1` → `Monitor` + `t("journal.origin.self")` («это устройство»)
   - `source === "sync"` → `MonitorSmartphone` + (`dName ?? t("journal.origin.otherDevice")`)
   - иначе (`desktop` с чужого устройства — на клиенте это хост) → `Server` + `t("journal.origin.host")`.
   Пользователь — `e.uName`, если есть. Эмодзи 🤖 убрать.
4. Для `update`/`restore`/`conflict_resolved` — поля «было → стало» как сейчас, но:
   - расширить `SKIP_FIELDS`: `id, deviceId, tgMessageId, source, createdByUserId, sort,
     deletedAt, version, seq, localDirty, updatedAt, updatedByDevice, createdAt, changesJson, beforeJson, afterJson`;
   - валюта денежных полей — `moneyCurrency(e.entity, c.field, after ?? before, ctx)`
     (сейчас всегда `after.currency ?? "BYN"` — для операций по USD-счёту это ошибка);
   - для `create` поля **не показывать** (хватает сводки);
   - для `delete` — сводка + кнопка «Восстановить» (как сейчас).
5. Убрать вывод `rowName` и `t("ui.noChanges")` (их заменила сводка).

**Фильтры** (под поиском, рядом с текущими чипами сущностей — второй ряд чипов,
тем же стилем `rounded-full border ...`):
- Пользователь: чип «Все» + чип на каждого из `useUsers()`.
- Источник: «Все», `journal.origin.self`, `journal.origin.otherDevices`, `journal.origin.bot`,
  `journal.origin.reminder` → `origin` = `"" | "self" | "other" | "bot" | "reminder"`.
- Переключатель `Checkbox` `t("journal.showSystem")` («Показать служебные»), по умолчанию выкл.;
  при включении в фильтр источников добавляется чип `journal.origin.system`.

`ctx` для `summarize` собрать в `useMemo` из `useAccounts()`, `useCategories()` (имя через
существующий `catLabel`), `useUsers()`.

#### 6.4 i18n (новый объект `journal` верхнего уровня во всех трёх локалях)

| ключ | ru | be | en |
|---|---|---|---|
| `sum.expense` | Расход {{amount}} · {{category}} · {{account}} | Выдатак {{amount}} · {{category}} · {{account}} | Expense {{amount}} · {{category}} · {{account}} |
| `sum.income` | Доход {{amount}} · {{category}} · {{account}} | Даход {{amount}} · {{category}} · {{account}} | Income {{amount}} · {{category}} · {{account}} |
| `sum.transfer` | Перевод {{amount}} {{from}} → {{toAmount}} {{to}} | Перавод {{amount}} {{from}} → {{toAmount}} {{to}} | Transfer {{amount}} {{from}} → {{toAmount}} {{to}} |
| `sum.account` | Счёт «{{name}}» · {{currency}} | Рахунак «{{name}}» · {{currency}} | Account “{{name}}” · {{currency}} |
| `sum.category` | Категория «{{name}}» | Катэгорыя «{{name}}» | Category “{{name}}” |
| `sum.goal` | Цель «{{name}}» · {{current}} из {{target}} | Мэта «{{name}}» · {{current}} з {{target}} | Goal “{{name}}” · {{current}} of {{target}} |
| `sum.reminder` | Напоминание «{{name}}» | Напамін «{{name}}» | Reminder “{{name}}” |
| `sum.user` | Пользователь «{{name}}» | Карыстальнік «{{name}}» | User “{{name}}” |
| `origin.self` | это устройство | гэтая прылада | this device |
| `origin.host` | хост | хост | host |
| `origin.otherDevice` | другое устройство | іншая прылада | another device |
| `origin.otherDevices` | другие устройства | іншыя прылады | other devices |
| `origin.bot` | Telegram-бот | Telegram-бот | Telegram bot |
| `origin.reminder` | напоминание | напамін | reminder |
| `origin.system` | система | сістэма | system |
| `showSystem` | Показать служебные | Паказаць службовыя | Show service records |
| `more` | Показать ещё | Паказаць яшчэ | Show more |
| `user` | Пользователь | Карыстальнік | User |
| `origin.title` | Источник | Крыніца | Source |

Корневого ключа `journal` в локалях нет (есть `nav.journal`, `pages.journal` — это строки
внутри других объектов), конфликта нет. Добавить объект после `settings`.

- [ ] **Step 1:** тесты `journalSummary.test.ts` → FAIL.
- [ ] **Step 2:** реализовать 6.1–6.4.
- [ ] **Step 3:** проверки → PASS. Вручную: новая операция видна фразой, время совпадает
  с часами компьютера, фильтры работают, «Показать ещё» догружает.
- [ ] **Step 4: Commit** `feat(journal): readable summaries, origin, local time, filters, load more`.

---

### Task 7: Mac — чёрная полоса внизу в полноэкранном режиме

**Файлы:** `src/app/App.tsx`, `src/app/useViewportHeight.ts` (новый),
`src/index.css`, `src/app/Onboarding.tsx`, `src/components/{DbError,DbUnlock,ErrorBoundary}.tsx`.

Причина: WKWebView при входе в полноэкранный режим на мониторе с другим масштабом
не пересчитывает `100vh`, каркас `h-screen` остаётся старой высоты.

1. **Высота каркаса из реального окна.** Хук:

```ts
/** Keeps --app-h equal to the real viewport height. WKWebView can keep a stale
 *  100vh after a fullscreen/scale change on macOS; re-measuring on every
 *  resize/scale event (and once more after the animation) fixes the black strip. */
export function useViewportHeight() {
  useEffect(() => {
    const set = () => document.documentElement.style.setProperty("--app-h", `${window.innerHeight}px`);
    const kick = () => { set(); requestAnimationFrame(set); window.setTimeout(set, 250); window.setTimeout(set, 800); };
    set();
    window.addEventListener("resize", kick);
    const win = getCurrentWindow();                  // @tauri-apps/api/window
    const un = [win.onResized(kick), win.onScaleChanged(kick)];
    return () => {
      window.removeEventListener("resize", kick);
      for (const p of un) void p.then((f) => f());
    };
  }, []);
}
```

Вызвать в корне `App` (рядом с `useDataChanged()`), до любых ранних `return`
(правило хуков).

2. **Классы высоты.** В `App.tsx` оба `h-screen` → `h-[var(--app-h,100dvh)]`.
   В `Onboarding.tsx`, `DbError.tsx`, `DbUnlock.tsx`, `ErrorBoundary.tsx`
   `min-h-screen` → `min-h-dvh`.
3. **`src/index.css`:** у `body` и `#root` после `min-height: 100vh;` добавить строку
   `min-height: 100dvh;` (старая остаётся запасным вариантом).
4. Проверить, что события окна разрешены: `core:default` в
   `src-tauri/capabilities/*.json` включает `core:window:default` и `core:event:default`.
   Если `onResized` падает с ошибкой прав — добавить `"core:window:allow-scale-factor"`
   не нужно; нужно только событие, оно идёт через `core:event:default`. Если всё же
   ошибка — обернуть вызовы в `try/catch` и оставить `window.resize`.

На Windows проверить, что ничего не сломалось (развернуть/свернуть окно, сменить размер).
На Mac проверит пользователь на сборке.

- [ ] **Step 1:** реализовать. **Step 2:** проверки → PASS.
- [ ] **Step 3: Commit** `fix(macos): recompute app height after fullscreen/scale changes`.

---

### Task 8: Документация, версия, проверка, релиз

1. `CHANGELOG.md` — раздел `## 0.3.1 — <дата>` (по-русски), темы:
   «Синхронизация», «Дашборд», «Журнал», «macOS». Отметить, что схема не менялась и
   устройства можно обновлять по одному; фиктивные конфликты по курсам вычищаются сами
   при запуске.
2. `src/lib/whatsnew.ts` — запись `version: "0.3.1"` первой в `WHATS_NEW`, 5–7 буллетов
   на трёх языках (по образцу записи 0.3.0): видно, хост или клиент; кнопка
   «Синхронизировать» и понятные ошибки; кнопка «Обновить»; курсы на дашборде;
   понятный журнал; исправлен низ окна на Mac.
3. `AGENTS.md` — в раздел про синхронизацию (создать «## Синхронизация», если нет):
   `sync_now` + лок `sync_lock()`; одинаковые данные → `ok` без конфликта; курсы
   никогда не конфликтуют; клиент зеркалит список конфликтов хоста; ошибки в `logs/sync.log`
   без повторов.
4. Версия **0.3.1** в `package.json`, `src-tauri/tauri.conf.json`, `Cargo.toml`
   (`[workspace.package]`), затем `npm install --package-lock-only` и `cargo check --workspace`.
5. Полный прогон всех проверок из Global Constraints.
6. Независимое ревью ветки (code-review skill) по пунктам Review Focus.
7. **Остановиться и показать результат пользователю.** Релиз (тег `v0.3.1`, пуш,
   сборка на GitHub, публикация черновика) — только после его подтверждения, по
   skill `release`.

- [ ] **Commit** `docs: 0.3.1 — changelog, what's new, agents notes` (+ версия отдельным
  коммитом `release: v0.3.1` при релизе).

---

## Ручная проверка перед релизом (чек-лист для пользователя)

1. Хост: Настройки → Устройства — «Это устройство — хост», в списке MacBook с временем связи.
2. Mac: «Это устройство подключено к хосту», адрес, время последней синхронизации,
   кнопка «Синхронизировать» → «Готово: отправлено N, получено M».
3. Включить VPN на хосте → на Mac через ~20 с текст «Хост недоступен…»; выключить → ошибка
   пропала после следующей синхронизации (до 2 мин или сразу по кнопке).
4. Блок конфликтов на Mac пуст (или несколько групп), нет тысяч карточек.
5. Операция через бота на хосте появляется на дашборде хоста за ~3 с, на Mac — за ~20 с,
   без переключения вкладок. Кнопка «Обновить» подтягивает сразу.
6. Дашборд: карточка курсов; Настройки → Общие → «Курсы валют» скрывает её.
7. Журнал: фразы вместо полей, местное время, источник («это устройство», «MacBook»,
   «Telegram-бот»), фильтры, «Показать ещё», служебные скрыты.
8. Mac: внешний монитор, зелёная кнопка → низ окна не чёрный.
