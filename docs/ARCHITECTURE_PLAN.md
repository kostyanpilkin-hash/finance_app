# План рефакторинга архитектуры «Финансы»

Дата аудита: 2026-11-13, версия кода 0.1.10 (коммит `e373bd1`).

Документ состоит из трёх частей:

1. **Находки аудита**: что не так и почему.
2. **Принятые решения**: ответы на развилки. Пересматривать их не нужно.
3. **Пошаговый план**: шаги 1–55. Каждый шаг меняет 1–3 файла (новые файлы
   и строка `mod x;` тоже считаются) и заканчивается успешной сборкой.

---

## 0. Правила выполнения плана (прочитать перед каждым шагом)

1. Шаги выполняются **строго по порядку**. Если шаг зависит от предыдущего,
   это сказано в нём явно.
2. Один шаг = один коммит. Сообщение коммита: `refactor(шаг N): <кратко>`.
3. Меняйте **только** файлы, перечисленные в шаге. Если для сборки нужно
   поменять другой файл, остановитесь и опишите проблему вместо импровизации.
4. Не добавляйте и не удаляйте комментарии в коде, если шаг этого не требует.
5. Shell — git-bash. Перед любой cargo-командой выполните:
   ```bash
   export PATH="tools/strawberry-perl/perl/bin:$PATH"
   ```
6. **Минимальная проверка после каждого шага** (обязательна, даже если в шаге
   указана другая команда):
   ```bash
   cargo check --workspace && npx tsc --noEmit
   ```
7. **Полная проверка в конце каждой фазы** (тот же набор, что в `AGENTS.md`):
   ```bash
   npx tsc --noEmit && npm run lint && npm test
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
8. В конце каждой фазы добавьте запись в `CHANGELOG.md` (по-русски, в раздел
   текущей даты) и закоммитьте вместе с последним шагом фазы.
9. Деньги — только `i64` в минимальных единицах. В TypeScript — только целые
   `number`/`bigint`, никаких делений с дробным результатом при подсчёте сумм.
10. Если тест из шага не проходит, **не ослабляйте тест**. Исправляйте код
    в рамках файлов этого шага.

Краткие обозначения путей:

| Сокращение | Путь |
|---|---|
| `storage/` | `crates/storage/src/` |
| `api/` | `crates/api/src/` |
| `app/` | `src-tauri/src/` |
| `fe/` | `src/` |

---

## 1. Находки аудита

Термины: **модуль** — всё, у чего есть интерфейс и реализация.
**Глубина** — насколько много поведения скрыто за маленьким интерфейсом.
**Шов (seam)** — место, где поведение можно подменить без правки вызывающих.
**Локальность** — изменения, ошибки и знания сосредоточены в одном месте.
**Рычаг (leverage)** — сколько вызывающих выигрывают от одного улучшения.

### 1.1. Целостность данных и синхронизация (критично)

| # | Находка | Где | Последствие |
|---|---|---|---|
| D1 | Большинство записей идёт без транзакции. `apply_change` выполняет 3–4 оператора: строка, `audit_log`, `sync_log`/`outbox`, `UPDATE seq`. При этом команды `transaction_upsert`, `entity_delete`, `category_delete` (N операций!), `account_upsert` и др. вызывают его через `with_conn` **без** `BEGIN`. | `app/commands.rs`, `storage/repos.rs`, `api/lib.rs` (`push`, `resolve`), `app/sync.rs` (pull) | Если процесс упадёт посередине, строка запишется, а запись в `outbox`/`sync_log` — нет: изменение никогда не уедет на другие устройства. Ещё это прямое нарушение правила AGENTS.md («apply_change внутри транзакции»). Плюс autocommit = fsync на каждый оператор, это медленно. |
| D2 | `/pull` возвращает `maxSeq = MAX(seq) FROM sync_log` **глобально**, хотя изменения отдаёт страницей (`LIMIT 2000`). | `api/lib.rs` → `pull` | Клиент записывает `last_seq = глобальный максимум` и **навсегда пропускает** всё после первых 2000 записей. Одна операция даёт 2 seq (audit + строка), так что при первом подключении к хосту, где больше ~1000 операций, клиент получит неполную базу. |
| D3 | Когда `/push` отвечает `status: "conflict"`, операция остаётся в `outbox`. При каждом следующем push (раз в 20 с) хост создаёт **новую** строку `conflicts`. | `app/sync.rs` (ack), `api/lib.rs` (`push`) | Таблица конфликтов бесконечно растёт, пользователь видит дубли. После выбора «серверный вариант» локальная строка клиента остаётся старой навсегда: хост её не меняет, значит, pull её не вернёт. |
| D4 | `/pull` отдаёт **все** нерешённые конфликты всем устройствам. | `api/lib.rs` → `pull` | Устройство B видит и может «решить» конфликт устройства A. |
| D5 | Подтверждение push ставит `local_dirty = 0` для сущности, даже если пока шёл сетевой запрос, пользователь снова её изменил (в `outbox` лежит новая операция). | `app/sync.rs` (ack) | Следующий pull перезапишет свежую локальную правку. |
| D6 | `merge_existing` + `stamp` берут `version` из **payload**, а не из БД. Фронт или клиент, приславший устаревшую строку, откатывает версию назад. | `storage/repos.rs`, `storage/db.rs` | Версия перестаёт расти монотонно, проверка конфликтов `base < cur_version` пропускает реальные конфликты. |
| D7 | Миграции не проверяют, что база **новее** приложения (откат на старую версию). Перед миграцией не делается снимок. | `storage/migrations.rs`, `storage/lib.rs` | Старое приложение молча работает с базой новой схемы. Если миграция упадёт посередине на реальных данных, отката нет. |
| D8 | `Db.mode` читается один раз при открытии. После «Подключиться к хосту» в Настройках процесс до перезапуска остаётся «хостом»: запущен API-сервер, клиентский цикл не стартовал, записи идут в `sync_log`, а не в `outbox`. | `app/sync.rs` (`pair_client`), `fe/pages/SettingsPage.tsx` | Всё, что введено до перезапуска, не синхронизируется. |
| D9 | Ручной и авто-бэкап выполняют `VACUUM INTO` под **мьютексом записи**. Путь `attachments_dir/../backups` и очистка старых копий продублированы. Шифрование бэкапа тестом не проверено. | `app/commands.rs`, `app/scheduler.rs` | На время бэкапа блокируются все записи: UI, бот, синк. Риск незаметно начать писать незашифрованные копии. |

### 1.2. Многопоточность и состояние (Rust)

| # | Находка | Где |
|---|---|---|
| C1 | Синхронные SQLite-вызовы (`with_conn`) внутри `async`: клиентский синк, axum-обработчики, бот, планировщик. Они блокируют потоки tokio, пока ждут мьютекс записи (а он может быть занят бэкапом, см. D9). | `app/sync.rs`, `api/lib.rs`, `app/bot/*.rs`, `app/scheduler.rs` |
| C2 | Второй `reqwest::Client` (таймаут 60 с) в боте рядом с общим `net::client()`. | `app/bot.rs` → `start_polling` |
| C3 | Доступ к keyring размазан по трём файлам, константа `KEY_SERVICE` объявлена трижды. `read_device_token()` (IPC с ОС) вызывается **внутри** `with_read_conn`, то есть под мьютексом чтения. | `app/state.rs`, `app/sync.rs`, `app/bot.rs` |
| C4 | Бизнес-логика курсов (`refresh_rates`, `ensure_rate`) лежит в слое команд. Планировщик вызывает **Tauri-команду** `commands::rates_refresh(state)`, бот вызывает `commands::*`. | `app/commands.rs`, `app/scheduler.rs`, `app/bot/wizard.rs` |
| C5 | В каждой из ~35 команд повторяется шаблон `state.db.share()` → `run(move || db.with_conn(|c| { actor_on(...)?; ... }).map_err(|e| e.to_string()))`. Модуль команд **мелкий**: интерфейс почти равен реализации. | `app/commands.rs` |
| C6 | `mode` — строка (`"host"`/`"client"`), сравнивается литералами в 7 местах. | `storage/db.rs`, `storage/lib.rs`, `app/sync.rs`, `app/commands.rs` |
| C7 | Генерация 6-значных кодов продублирована (бот и синк) и смещена: `byte % 10` даёт неравномерное распределение. | `app/bot.rs`, `app/sync.rs` |
| C8 | Логи разбросаны: `startup.log` пишется inline в `lib.rs`, `bot.log` — в `bot.rs`, синк пишет через `eprintln!`, а в GUI-сборке Windows этот вывод никуда не попадает. | `app/lib.rs`, `app/bot.rs`, `app/sync.rs` |
| C9 | `set_setting` принимает **любой** ключ, в том числе служебные: `last_seq`, `host_url`, `onboarding_done`, `pin_hash`. | `app/commands.rs` |

### 1.3. Фронтенд

| # | Находка | Где |
|---|---|---|
| F1 | **Коллизии ключей кэша.** Ключ `["categories"]` используется и для списка **с удалёнными** (`CategoriesPage`), и **без** (все остальные страницы). То же для `["reminders"]` (RemindersPage и JournalPage) и `["goals"]`. Какая страница загрузилась первой, та и решает, что увидят остальные. | `fe/pages/*.tsx` |
| F2 | **Несогласованные префиксы.** Аналитика и отчёты кэшируют операции под `["tx", …]`, а `TxForm` и `TransactionsPage` инвалидируют `["transactions"]`. После новой операции аналитика остаётся устаревшей. | `fe/pages/AnalyticsPage.tsx`, `fe/pages/ReportsPage.tsx`, `fe/components/transactions/TxForm.tsx` |
| F3 | `Onboarding` вызывает `invalidateQueries({ queryKey: ["onboarding", "settings"] })`. Такого ключа нет, ничего не инвалидируется. | `fe/app/Onboarding.tsx` |
| F4 | Ключи кэша — строковые литералы в 20 файлах, общих хуков нет. Один и тот же `useQuery(["users"], …)` написан 8 раз. | `fe/**` |
| F5 | Операции из бота и изменения, пришедшие синком, не появляются на открытом экране: событий от бэкенда нет. | `fe/app/App.tsx`, `app/*` |
| F6 | **Тяжёлые расчёты на фронте.** Аналитика тянет до 5000 + 5000 + 20000 строк операций через IPC и агрегирует их в JS. Главная тянет 2000. Бот считает те же суммы своим SQL в Rust — две реализации, которые расходятся. | `fe/pages/AnalyticsPage.tsx`, `fe/pages/DashboardPage.tsx`, `app/bot/report.rs` |
| F7 | Пересчёт валют на фронте идёт через **float**: `minor * (r / 1e6) / (target / 1e6)`, а поле `scale` игнорируется. В Rust то же самое сделано целочисленно через i128. | `fe/lib/useDisplayMoney.ts` |
| F8 | Утечка при печати PDF: `iframe` и `blob:`-URL не удаляются после печати. | `fe/pages/ReportsPage.tsx` |
| F9 | `SettingsPage.tsx` — 633 строки, 12 независимых секций в одном файле. | `fe/pages/SettingsPage.tsx` |

### 1.4. Безопасность и конфиг Tauri (в целом хорошо)

- CSP строгая, `connect-src` только IPC; плагинов fs/shell нет; asset-scope
  ограничен `attachments/**` и закреплён тестом. Изменений не требуется.
- `capabilities/default.json` содержит только `core/opener/autostart/updater/process:default`.
  `process:default` нужен для `relaunch()`. Изменений не требуется.
- Секреты (ключ БД, токен бота, токен устройства) лежат в keyring, токены
  устройств на хосте хранятся как SHA-256. Это корректно.
  Нужно только собрать код keyring в один модуль (C3).
- PIN хранится как быстрый SHA-256 без лимита попыток. **Решение: не трогать**
  (см. раздел 2): база и так зашифрована SQLCipher, PIN защищает только экран.

### 1.5. Кандидаты на углубление (сводка)

| Кандидат | Сила рекомендации | Шаги |
|---|---|---|
| **Транзакционный шов записи** `Db::write` + `AppState::write`: одна точка, где открывается транзакция, вычисляется actor и запускается blocking-пул | Strong | 1–5, 15, 44–46 |
| **Протокол синхронизации**: пагинация pull, жизненный цикл конфликта, версии | Strong | 6–9, 16–17 |
| **Модуль бэкапа** в `storage::backup` (create / prune / pre-migration) | Strong | 11, 13–14 |
| **Модуль аналитики** `storage::analytics`: один источник сумм для UI и бота | Strong | 31–36 |
| **Ключи кэша + события «данные изменились»** | Strong | 20–30 |
| Модуль `keystore` (все секреты ОС) | Worth exploring | 37–39 |
| `Mode` enum вместо строки | Worth exploring | 40–43 |
| Модуль `rates` вне слоя команд | Worth exploring | 19 |
| Единый `applog` | Worth exploring | 49–50 |
| Разрезать `SettingsPage` | Worth exploring | 51–55 |
| Разрезать `bot/wizard.rs` (2432 строки), типизированные ошибки команд, колонка `conflicts.device_id` (схема v7) | Speculative | раздел 4 |

**Главная рекомендация — начать с фазы 1 (шаги 1–14).** Это реальные ошибки,
которые теряют данные пользователя, а не косметика. Шов `Db::write` даёт
максимальный рычаг: одно изменение делает атомарными все пути записи
(UI, бот, синк, API).

---

## 2. Принятые решения

| Развилка | Решение | Следствие для плана |
|---|---|---|
| Смена режима host → client после привязки в Настройках | **А. Автоматический перезапуск** сразу после успешной привязки | Шаг 12. Переключать режим «на лету» не нужно, `Db.mode` остаётся неизменяемым до перезапуска. |
| Где считать аналитику | **А. Всё в Rust/SQL**: одна команда для экрана и бота | Фаза 4 (шаги 31–36). |
| Обновление UI при изменениях из бота или синка | **А. События от бэкенда** | Шаги 28–29. Реализация — наблюдатель за `MAX(rowid)` в `audit_log` (каждая доменная запись, включая записи из pull, добавляет строку в `audit_log`). Так один модуль ловит все источники изменений, и в каждый путь записи не нужно вставлять `emit`. |
| PIN-код | **В. Не трогать** | В плане нет шагов про хеш PIN и лимит попыток. |

---

## 3. Пошаговый план

### Фаза 1. Целостность данных

#### Шаг 1. Транзакционный метод `Db::write`

**Файлы:** `crates/storage/src/lib.rs`

1. В `impl db::Db { ... }` сразу после метода `with_read_conn` добавьте:
   ```rust
   /// Serialized write inside one IMMEDIATE transaction: commits on Ok,
   /// rolls back on Err. Never open another transaction inside `f`.
   pub fn write<T>(
       &self,
       f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, Error>,
   ) -> Result<T, Error> {
       let mut conn = self.conn.lock().map_err(|e| Error::Other(e.to_string()))?;
       let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
       let out = f(&tx)?;
       tx.commit()?;
       Ok(out)
   }
   ```
2. В `mod tests` того же файла добавьте тест:
   ```rust
   #[test]
   fn write_rolls_back_on_error() {
       let d = db();
       let r: Result<(), Error> = d.write(|c| {
           repos::setting_set(c, "probe", "1")?;
           Err(Error::Invalid("boom".into()))
       });
       assert!(r.is_err());
       let v = d.with_read_conn(|c| repos::setting_get(c, "probe")).unwrap();
       assert_eq!(v, None);
       d.write(|c| repos::setting_set(c, "probe", "2")).unwrap();
       let v = d.with_read_conn(|c| repos::setting_get(c, "probe")).unwrap();
       assert_eq!(v.as_deref(), Some("2"));
   }
   ```

`&Transaction` автоматически приводится к `&Connection`, поэтому все функции
`repos::*(conn: &Connection, …)` принимают его без изменений.

**Проверка:** `cargo test -p finance-storage write_rolls_back_on_error`

---

#### Шаг 2. Команды UI пишут через `Db::write`

**Файлы:** `src-tauri/src/commands.rs`

1. В командах ниже замените `db.with_conn(|c| { ... })` на `db.write(|c| { ... })`:
   `complete_onboarding`, `set_current_user`, `set_setting`, `entity_delete`,
   `entity_restore`, `user_upsert`, `account_upsert`, `category_upsert`,
   `category_delete`, `transaction_upsert`, `goal_upsert`, `reminder_upsert`,
   `reminder_resolve`, `pin_set`, `pin_clear`, `attachment_add`, а также
   записывающую часть `refresh_rates` (цикл `rate_upsert`) и последнюю
   запись в `ensure_rate`.
2. **Обязательно:** в `complete_onboarding` и `reminder_resolve` удалите
   внутреннюю транзакцию — строки `let tx = c.unchecked_transaction()?;` и
   `tx.commit()?;`, а все `&tx`/`tx.` замените на `c`. Вложенная транзакция
   внутри `write` упадёт с ошибкой SQLite
   `cannot start a transaction within a transaction`.
3. `backup_create` **не трогать**: `VACUUM INTO` нельзя выполнять внутри
   транзакции (его заменит шаг 14).

**Проверка:** `cargo check -p finance-app && cargo test -p finance-app`.
Затем `grep -n "unchecked_transaction" src-tauri/src/commands.rs` должен
ничего не найти.

---

#### Шаг 3. Бот пишет через `Db::write`

**Файлы:** `src-tauri/src/bot/wizard.rs`, `src-tauri/src/bot/notify.rs`, `src-tauri/src/bot.rs`

1. В `wizard.rs` и `notify.rs` найдите каждое место вида
   ```rust
   state.db.with_conn(|c| {
       let tx = c.unchecked_transaction()?;
       ...
       tx.commit()?;
       Ok(...)
   })
   ```
   и замените его на
   ```rust
   state.db.write(|tx| {
       ...
       Ok(...)
   })
   ```
   То есть удалите строки `let tx = c.unchecked_transaction()?;` и
   `tx.commit()?;`, а параметр замыкания назовите `tx`, чтобы тело не менялось.
   Таких мест 5: в `wizard.rs` около строк 1373, 1484, 1552, 1746, в
   `notify.rs` около строки 296. Если в конце замыкания стоит
   `tx.commit()?; Ok(x)`, оставьте только `Ok(x)`.
2. В `bot.rs` замените `with_conn` на `write` в `tg_connect` (внутри
   `spawn_blocking`), `tg_disconnect` и `tg_pair_code`.

**Проверка:** `cargo check -p finance-app`, затем
`grep -rn "unchecked_transaction" src-tauri/src` не должен ничего найти.

---

#### Шаг 4. Клиентский синк: атомарные подтверждения и pull, защита от гонки

**Файлы:** `src-tauri/src/sync.rs`

1. В `sync_once_inner` блок подтверждения push (цикл `for (i, op) in ops.iter().enumerate()`)
   замените на:
   ```rust
   state
       .db
       .write(|c| {
           for op in &ops {
               let id = op["id"].as_str().unwrap_or("");
               if !ok_ids.iter().any(|x| x == id) {
                   continue;
               }
               let Some(e) = op["entity"].as_str().and_then(finance_storage::Entity::parse) else {
                   continue;
               };
               c.execute("DELETE FROM outbox WHERE op_id = ?1", [op["opId"].as_str().unwrap_or("")])?;
               c.execute(
                   &format!(
                       "UPDATE {} SET local_dirty=0 WHERE id=?1 AND NOT EXISTS (SELECT 1 FROM outbox WHERE entity_id=?1)",
                       e.table()
                   ),
                   [id],
               )?;
           }
           Ok(())
       })
       .map_err(|e| e.to_string())?;
   ```
   Что изменилось: (а) одна транзакция; (б) имя таблицы берётся из
   `Entity::parse`, а не из сырого JSON; (в) `local_dirty` сбрасывается,
   только если для этой сущности в outbox не осталось новых операций (D5).
2. Блок применения pull (`state.db.with_conn(|c| { for ch in body["changes"]...`)
   переведите на `state.db.write(|c| { ... })` с тем же телом.
3. В `pair_client` блок из трёх `setting_set` переведите на `state.db.write`.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 5. API хоста: атомарный push/resolve/pair

**Файлы:** `crates/api/src/lib.rs`

1. В обработчиках `push`, `resolve` и `pair` замените `s.db.with_conn(|c| {` на
   `s.db.write(|c| {`. Тела замыканий не меняются.
2. В `PairingHub::decide` оставьте `db.with_conn`: там один `INSERT`.

**Проверка:** `cargo test -p finance-api`

---

#### Шаг 6. `/pull`: правильный `maxSeq`, снимок чтения, `hasMore` (D2)

**Файлы:** `crates/api/src/lib.rs`, `crates/api/tests/push.rs`

1. В `pull` внутри `s.db.with_read_conn(|c| { ... })`:
   - первой строкой после `auth(c, &token)?;` добавьте
     `let snap = c.unchecked_transaction()?;` — это снимок чтения: все
     запросы видят одно состояние. Read-соединение никогда не открывает
     других транзакций, поэтому здесь это безопасно.
   - запросы выполняйте через `snap` (`snap.prepare(...)`,
     `dbm::row_json(&snap, ...)`);
   - удалите запрос `SELECT COALESCE(MAX(seq),0) FROM sync_log`;
   - вычисляйте:
     ```rust
     let max_seq = changes
         .last()
         .and_then(|c| c["seq"].as_i64())
         .unwrap_or(since);
     let has_more = changes.len() as i64 >= limit;
     ```
   - ответ: `json!({"changes": changes, "conflicts": conflicts, "maxSeq": max_seq, "hasMore": has_more})`;
   - `commit` и явный `drop(snap)` **не** вызывайте: подготовленные запросы
     заимствуют `snap`, и явный `drop` не скомпилируется. Транзакция чтения
     сама откатится в конце замыкания.
2. В `crates/api/tests/push.rs` добавьте тест:
   ```rust
   #[tokio::test]
   async fn pull_pages_do_not_skip_changes() {
       let (app, db) = app();
       seed_device(&db);
       db.with_conn(|c| {
           let a = Actor { user_id: None, device_id: Some(db.device_id.clone()), source: Source::Desktop };
           for n in ["A", "B", "C"] {
               repos::account_upsert(c, &db, &a, json!({"name": n, "kind": "cash", "currency": "BYN"}))?;
           }
           Ok(())
       })
       .unwrap();
       let req = Request::get("/api/v1/pull?since=0&limit=2")
           .header("authorization", "Bearer tok")
           .body(Body::empty())
           .unwrap();
       let resp = app.clone().oneshot(lan_req(req)).await.unwrap();
       assert_eq!(resp.status(), StatusCode::OK);
       let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
       let body: Value = serde_json::from_slice(&bytes).unwrap();
       let changes = body["changes"].as_array().unwrap();
       assert_eq!(changes.len(), 2);
       assert_eq!(body["maxSeq"], changes[1]["seq"]);
       assert_eq!(body["hasMore"], true);
   }
   ```
   Если `Actor`, `Source`, `repos`, `json`, `Value`, `to_bytes` уже
   импортированы в начале файла (так и есть), новые `use` не нужны.

Клиент менять не нужно: он уже пишет `maxSeq` в `last_seq` и повторяет
`sync_once`, пока тот возвращает `Ok(true)`.

**Проверка:** `cargo test -p finance-api --test push`

---

#### Шаг 7. Конфликты на хосте: без дублей и только своего устройства (D3, D4)

**Файлы:** `crates/api/src/lib.rs`, `crates/api/tests/push.rs`

1. В `push`, в ветке конфликта:
   - в JSON `local` добавьте поле устройства:
     `json!({"entity": op.entity, "id": op.id, "op": op.op, "payload": op.payload, "deviceId": device_id})`;
   - **перед** `INSERT INTO conflicts` удалите прежний нерешённый конфликт
     этой сущности от этого устройства:
     ```rust
     c.execute(
         "DELETE FROM conflicts WHERE entity_id = ?1 AND resolved_at IS NULL
            AND json_extract(local_payload, '$.deviceId') = ?2",
         finance_storage::rusqlite::params![op.id, device_id],
     )?;
     ```
2. В `pull` замените `auth(c, &token)?;` на `let (device_id, _) = auth(c, &token)?;`,
   а запрос конфликтов — на:
   ```sql
   SELECT id, entity, entity_id, local_payload, server_payload FROM conflicts
   WHERE resolved_at IS NULL
     AND (json_extract(local_payload, '$.deviceId') IS NULL
          OR json_extract(local_payload, '$.deviceId') = ?1)
   ```
   с параметром `[&device_id]`. Условие `IS NULL` нужно для старых конфликтов,
   записанных до этого шага.
3. Тест в `push.rs`: дважды отправьте push с одной и той же устаревшей
   операцией (`baseVersion: 0` для строки, у которой на хосте версия ≥ 2) и
   проверьте, что `SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL`
   равно 1. За образцом формирования push-запроса смотрите существующие тесты
   в этом файле.

Схема БД не меняется (`SCHEMA_VERSION` прежний), так что совместимость с
клиентами v6 сохраняется.

**Проверка:** `cargo test -p finance-api --test push`

---

#### Шаг 8. Конфликты на клиенте: убрать из outbox, правильно решать (D3)

**Файлы:** `src-tauri/src/sync.rs`

1. В `sync_once_inner` после вычисления `ok_ids` вычислите
   `conflict_ids` тем же способом, но с фильтром `x["status"] == "conflict"`.
   В транзакции подтверждения (шаг 4) для операций из `conflict_ids` выполните
   **только** `DELETE FROM outbox WHERE op_id = ?1`. `local_dirty` **не**
   сбрасывайте: пока пользователь не выберет вариант, pull не должен
   перезаписывать его версию.
2. В `conflict_resolve` после успешного ответа хоста замените блок
   `with_conn` на:
   ```rust
   state
       .db
       .write(|c| {
           let (entity, entity_id, server): (String, String, String) = c.query_row(
               "SELECT entity, entity_id, server_payload FROM conflicts WHERE id=?1",
               [conflict_id],
               |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
           )?;
           if let Some(e) = finance_storage::Entity::parse(&entity) {
               if winner == "server" {
                   let row: Value = serde_json::from_str(&server)?;
                   if row.is_object() && row.get("id").is_some() {
                       dbm::write_row(c, e.table(), &row)?;
                   }
               }
               c.execute(
                   &format!("UPDATE {} SET local_dirty=0 WHERE id=?1", e.table()),
                   [&entity_id],
               )?;
           }
           c.execute(
               "UPDATE conflicts SET resolved_at=?1 WHERE id=?2",
               finance_storage::rusqlite::params![dbm::now_iso(), conflict_id],
           )?;
           Ok(())
       })
       .map_err(|e| e.to_string())
   ```
   Если победил «local», хост применит локальный payload и выдаст новый
   `seq`. Строка уже не dirty, поэтому следующий pull её примет.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 9. Версия и служебные поля берутся из БД, а не из payload (D6)

**Файлы:** `crates/storage/src/db.rs`, `crates/storage/src/lib.rs`

1. В `db.rs`, в функции `apply_change`, в замыкании `prep` сразу после
   `map.insert("id".into(), json!(id));` добавьте:
   ```rust
   for k in ["version", "seq", "updatedByDevice"] {
       match before.as_ref().and_then(|b| b.get(k)).filter(|v| !v.is_null()) {
           Some(v) => {
               map.insert(k.into(), v.clone());
           }
           None => {
               map.remove(k);
           }
       }
   }
   if let Some(v) = before.as_ref().and_then(|b| b.get("createdAt")).filter(|v| !v.is_null()) {
       map.insert("createdAt".into(), v.clone());
   }
   ```
   `createdAt` у новой строки сохраняется из payload: клиент создал запись
   раньше, чем она дошла до хоста.
2. В `lib.rs`, в `mod tests`, добавьте тест:
   ```rust
   #[test]
   fn stale_payload_version_cannot_rewind() {
       let d = db();
       d.write(|c| {
           let a = actor(&d);
           let acc = repos::account_upsert(c, &d, &a, json!({"name":"A","kind":"cash","currency":"BYN"}))?;
           let id = acc["id"].as_str().unwrap().to_string();
           for _ in 0..3 {
               repos::account_upsert(c, &d, &a, json!({"id": id, "name": "B"}))?;
           }
           let v_before = db::row_json(c, "accounts", &id)?.unwrap()["version"].as_i64().unwrap();
           repos::account_upsert(c, &d, &a, json!({"id": id, "name": "C", "version": 1}))?;
           let v_after = db::row_json(c, "accounts", &id)?.unwrap()["version"].as_i64().unwrap();
           assert_eq!(v_after, v_before + 1);
           Ok(())
       })
       .unwrap();
   }
   ```

**Проверка:** `cargo test --workspace`. Если упадёт существующий тест,
который ожидает версию из payload, **остановитесь и опишите**, какой тест
и почему.

---

#### Шаг 10. Отказ открывать базу новее приложения (D7, часть 1)

**Файлы:** `crates/storage/src/migrations.rs`, `crates/storage/src/lib.rs`

1. В `migrations.rs`, в `migrate`, сразу после вычисления `current` добавьте:
   ```rust
   if current > SCHEMA_VERSION {
       return Err(Error::Invalid(format!(
           "db_newer_than_app: db schema v{current}, app supports v{SCHEMA_VERSION}"
       )));
   }
   ```
2. Добавьте публичную функцию (она понадобится в шаге 11):
   ```rust
   /// Schema version stored in `meta`, 0 for a fresh or unreadable db.
   pub fn stored_version(conn: &Connection) -> i64 {
       conn.query_row(
           "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
           [],
           |r| r.get(0),
       )
       .unwrap_or(0)
   }
   ```
   и используйте её внутри `migrate` вместо inline-запроса `current`.
3. Тест в `lib.rs` → `mod tests`:
   ```rust
   #[test]
   fn newer_schema_is_refused() {
       let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
       let p = dir.join("f.db");
       let d = db::Db::open(&p, Some(&[3u8; 32])).unwrap();
       d.write(|c| {
           c.execute(
               "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
               [(migrations::SCHEMA_VERSION + 1).to_string()],
           )?;
           Ok(())
       })
       .unwrap();
       drop(d);
       let err = db::Db::open(&p, Some(&[3u8; 32])).err().unwrap().to_string();
       assert!(err.contains("db_newer_than_app"), "{err}");
       std::fs::remove_dir_all(&dir).ok();
   }
   ```

Экран ошибки запуска (`DbError`) уже показывает текст ошибки и предлагает
восстановление. Отдельный UI не нужен.

**Проверка:** `cargo test -p finance-storage newer_schema_is_refused`

---

#### Шаг 11. Снимок базы перед миграцией (D7, часть 2)

**Файлы:** `crates/storage/src/backup.rs`, `crates/storage/src/lib.rs`

1. В `backup.rs` добавьте:
   ```rust
   /// Encrypted copy of the live db in `<data_dir>/backups` before a schema
   /// upgrade: `pre-migrate-v{from}-{timestamp}.db`.
   pub fn pre_migration_snapshot(conn: &Connection, data_dir: &Path, from: i64) -> Result<PathBuf, Error> {
       let dir = data_dir.join(BACKUP_DIR);
       std::fs::create_dir_all(&dir)?;
       let dest = dir.join(format!(
           "pre-migrate-v{from}-{}.db",
           chrono::Utc::now().format("%Y%m%d-%H%M%S")
       ));
       conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
       Ok(dest)
   }
   ```
2. В `lib.rs`, в `Db::open`, после `apply_key(&conn, key)?;` (для `conn`)
   добавьте:
   ```rust
   let stored = migrations::stored_version(&conn);
   if stored > 0 && stored < migrations::SCHEMA_VERSION {
       backup::pre_migration_snapshot(
           &conn,
           path.parent().unwrap_or(Path::new(".")),
           stored,
       )?;
   }
   ```
3. Тест в `lib.rs` → `mod tests` (заодно закрепляет, что `VACUUM INTO`
   шифрует копию):
   ```rust
   #[test]
   fn pre_migration_snapshot_is_encrypted() {
       let dir = std::env::temp_dir().join(format!("fin-{}", new_id()));
       let key = [6u8; 32];
       let d = db::Db::open(&dir.join("finance.db"), Some(&key)).unwrap();
       let snap = d
           .with_conn(|c| backup::pre_migration_snapshot(c, &dir, 5))
           .unwrap();
       drop(d);
       let head = std::fs::read(&snap).unwrap();
       assert_ne!(&head[..16], b"SQLite format 3\0");
       assert!(db::Db::open(&snap, Some(&key)).is_ok());
       assert!(backup::list(&dir).iter().any(|n| n.starts_with("pre-migrate-v5-")));
       std::fs::remove_dir_all(&dir).ok();
   }
   ```

**Проверка:** `cargo test -p finance-storage pre_migration_snapshot_is_encrypted`.
Если тест покажет, что копия **не** зашифрована, остановитесь и сообщите:
это уязвимость текущих бэкапов, и её нужно решать отдельно.

---

#### Шаг 12. Перезапуск после привязки к хосту из Настроек (D8)

**Файлы:** `src/pages/SettingsPage.tsx`

1. В `SyncSection` у мутации `pair` замените `onSuccess` на:
   ```ts
   onSuccess: async () => {
     await new Promise((r) => setTimeout(r, 1200));
     await relaunch();
   },
   ```
   `relaunch` уже импортирован в этом файле.
2. Строку `{pair.isSuccess && <p ...>{t("ui.restoreRestart")}</p>}` оставьте:
   текст покажется на 1,2 секунды до перезапуска.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 13. `backup::create` и `backup::prune` в storage (D9)

**Файлы:** `crates/storage/src/backup.rs`, `crates/storage/src/lib.rs`

1. В `backup.rs` добавьте:
   ```rust
   /// Encrypted `backups/backup-<ts>.db` via VACUUM INTO on the READ
   /// connection — writers (UI, bot, sync) are not blocked meanwhile.
   pub fn create(db: &crate::db::Db, data_dir: &Path) -> Result<PathBuf, Error> {
       let dir = data_dir.join(BACKUP_DIR);
       std::fs::create_dir_all(&dir)?;
       let dest = dir.join(format!(
           "backup-{}.db",
           chrono::Local::now().format("%Y%m%d-%H%M%S")
       ));
       db.with_read_conn(|c| {
           c.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
           Ok(())
       })?;
       Ok(dest)
   }

   /// Keeps the newest `keep` of our own `backup-*.db` files; never touches
   /// pre-restore / pre-migrate copies or sidecars.
   pub fn prune(data_dir: &Path, keep: usize) {
       let dir = data_dir.join(BACKUP_DIR);
       let Ok(rd) = std::fs::read_dir(&dir) else { return };
       let mut files: Vec<_> = rd
           .flatten()
           .filter(|f| {
               let n = f.file_name();
               let n = n.to_string_lossy();
               n.starts_with("backup-") && n.ends_with(".db")
           })
           .collect();
       files.sort_by_key(|f| f.file_name());
       if files.len() > keep {
           for f in files.drain(..files.len() - keep) {
               let _ = std::fs::remove_file(f.path());
           }
       }
   }
   ```
2. В `lib.rs`, в `impl db::Db`, добавьте вспомогательный метод:
   ```rust
   /// Directory holding finance.db, attachments/ and backups/.
   pub fn data_dir(&self) -> PathBuf {
       self.attachments_dir
           .parent()
           .map(Path::to_path_buf)
           .unwrap_or_else(|| PathBuf::from("."))
   }
   ```
3. Тест в `lib.rs` → `mod tests`: откройте файловую БД во временной папке,
   вызовите `backup::create(&d, &dir)` 3 раза с `std::thread::sleep(1100 ms)`
   между вызовами (имя содержит секунды), затем `backup::prune(&dir, 2)`.
   Проверьте, что осталось 2 файла `backup-*.db` и каждый открывается тем же
   ключом.

**Проверка:** `cargo test -p finance-storage`

---

#### Шаг 14. Команда и планировщик используют `backup::create/prune`

**Файлы:** `src-tauri/src/commands.rs`, `src-tauri/src/scheduler.rs`

1. `commands.rs` → `backup_create`: тело внутри `run(move || { ... })` замените на
   ```rust
   finance_storage::backup::create(&db, &db.data_dir())
       .map(|p| p.to_string_lossy().into_owned())
       .map_err(|e| e.to_string())
   ```
2. `scheduler.rs` → `auto_backup`: удалите ручное построение `dir`/`name`,
   `VACUUM INTO` и блок очистки. Новая логика:
   ```rust
   let data_dir = state.db.data_dir();
   if finance_storage::backup::create(&state.db, &data_dir).is_ok() {
       let _ = state
           .db
           .write(|c| finance_storage::repos::setting_set(c, "auto_backup_month", &month));
       finance_storage::backup::prune(&data_dir, 12);
   }
   ```

**Проверка:** полная проверка фазы (п. 7 раздела 0) + запись в `CHANGELOG.md`
(«Сохранность данных: транзакции, пагинация синка, конфликты, снимки перед
миграцией, бэкап без блокировки записи»).

---

### Фаза 2. Потоки и блокировки

#### Шаг 15. Общий helper `blocking::run`

**Файлы:** `src-tauri/src/blocking.rs` (новый), `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

1. Создайте `src-tauri/src/blocking.rs`:
   ```rust
   //! SQLite/disk work off the async runtime: the future resolves on tokio
   //! while a blocking-pool thread does the work.

   pub async fn run<T, F>(f: F) -> Result<T, String>
   where
       T: Send + 'static,
       F: FnOnce() -> Result<T, String> + Send + 'static,
   {
       tauri::async_runtime::spawn_blocking(f)
           .await
           .map_err(|e| e.to_string())?
   }
   ```
2. В `lib.rs` добавьте `mod blocking;` (по алфавиту, перед `mod bot;`).
3. В `commands.rs` удалите локальную `async fn run` вместе с её doc-комментарием
   и добавьте `use crate::blocking::run;` в импорты.

**Проверка:** `cargo check -p finance-app`

---

#### Шаг 16. Клиентский синк не блокирует tokio

**Файлы:** `src-tauri/src/sync.rs`

Каждый вызов `state.db.with_conn(...)`, `state.db.write(...)`,
`state.db.with_read_conn(...)` внутри `async fn` (`sync_once_inner`,
`pair_client`, `join_host`, `conflict_resolve`) оберните по образцу:

```rust
let db = state.db.share();
let ops: Vec<Value> = crate::blocking::run(move || {
    db.with_read_conn(|c| { /* прежнее тело */ }).map_err(|e| e.to_string())
})
.await?;
```

- Всё, что замыкание использует из внешнего кода (`ops`, `ok_ids`,
  `conflict_ids`, `body`, `host`, `user_id`, `conflict_id`, `winner`), перед
  замыканием клонируйте в owned-значения (`.clone()`, `.to_string()`):
  замыкание должно быть `'static`.
- Чтение `outbox` в начале `sync_once_inner` сейчас идёт через `with_conn`.
  Переведите его на `with_read_conn`: это чтение.
- `client_cfg` (синхронная функция) пока не трогайте, её исправит шаг 38.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 17. Axum-обработчики не блокируют tokio

**Файлы:** `crates/api/src/lib.rs`

1. Добавьте helper:
   ```rust
   async fn blocking<T: Send + 'static>(
       f: impl FnOnce() -> T + Send + 'static,
   ) -> Result<T, StatusCode> {
       tokio::task::spawn_blocking(f)
           .await
           .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
   }
   ```
2. В `push`, `pull`, `resolve`, `pair` перенесите вызов `s.db.write(...)`/
   `s.db.with_read_conn(...)` в `blocking(move || ...)`:
   - перед замыканием: `let db = s.db.clone();` (это `Arc<Db>`), в `pair`
     также `let hub = s.hub.clone();`; внутри замыкания используйте `db`/`hub`
     вместо `s.db`/`s.hub`;
   - в `push` замыкание возвращает `Result<Vec<Value>, Error>`: создайте
     `let mut results = Vec::new();` **внутри** замыкания и верните
     `Ok(results)`;
   - результат: `blocking(move || ...).await.map_err(bad)?` для `push`/`pair`
     (тип ошибки `(StatusCode, Json<Value>)`), `.await?` для `pull`/`resolve`
     (тип ошибки `StatusCode`), а затем прежний `.map_err(...)` по внутренней
     ошибке.

**Проверка:** `cargo test -p finance-api && cargo test -p finance-app request_pairing_e2e`

---

#### Шаг 18. Бот использует общий HTTP-клиент

**Файлы:** `src-tauri/src/bot.rs`

1. В `start_polling` удалите `reqwest::Client::builder().timeout(60s)...`
   и используйте `let http = crate::net::client().clone();`.
2. В `Tg::call` добавьте таймаут на запрос перед `.send()`:
   ```rust
   .timeout(Duration::from_secs(if method == "getUpdates" { 40 } else { 20 }))
   ```
   Long-poll `getUpdates` ждёт 25 с, поэтому запас должен быть больше.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 19. Модуль `rates` вне слоя команд (C4)

**Файлы:** `src-tauri/src/rates.rs` (новый), `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

1. Перенесите из `commands.rs` в новый `rates.rs` функции `refresh_rates` и
   `ensure_rate` **целиком, без изменений тела**. Переименуйте их в
   `pub async fn refresh(state: &AppState)` и
   `pub async fn ensure(state: &AppState, currency: &str, date: &str)`.
   Импорты в `rates.rs`:
   `use finance_storage::repos; use serde_json::{json, Value}; use crate::{blocking::run, state::AppState};`
2. В `commands.rs`: `rates_refresh` вызывает `crate::rates::refresh(&state).await`,
   `convert_at` вызывает `crate::rates::ensure(&state, cur, &date).await`.
3. В `lib.rs` добавьте `mod rates;`.
4. Сразу после этого шага (в том же коммите, это 2 строки) замените вызовы
   в других файлах:
   - `src-tauri/src/bot/wizard.rs:434` → `crate::rates::ensure(state, &c, &d.date)`;
   - `src-tauri/src/bot/wizard.rs:1547` → `crate::rates::refresh(state)`;
   - `src-tauri/src/scheduler.rs:66` → `let _ = crate::rates::refresh(&state).await;`.

   Это исключение из правила «1–3 файла»: без него проект не соберётся.

**Проверка:** `cargo check -p finance-app && cargo test -p finance-app`.
Затем полная проверка фазы и `CHANGELOG.md` («Фоновые задачи не блокируют
потоки, общий HTTP-клиент, модуль курсов»).

---

### Фаза 3. Кэш фронтенда и автообновление

#### Шаг 20. Фабрика ключей и общие хуки

**Файлы:** `src/lib/queryKeys.ts` (новый), `src/lib/queries.ts` (новый)

1. `src/lib/queryKeys.ts`:
   ```ts
   import type { QueryClient } from "@tanstack/react-query";
   import type { EntityName } from "./api";

   export const qk = {
     entity: (e: EntityName, includeDeleted = false) => ["entity", e, includeDeleted] as const,
     entityAll: (e: EntityName) => ["entity", e] as const,
     transactions: ["transactions"] as const,
     txList: (scope: string, ...parts: unknown[]) => ["transactions", scope, ...parts] as const,
     txDays: (month: string) => ["transactions", "days", month] as const,
     dashboard: (...parts: unknown[]) => ["dashboard", ...parts] as const,
     analytics: (...parts: unknown[]) => ["analytics", ...parts] as const,
     journal: (...parts: unknown[]) => ["journal", ...parts] as const,
     settings: ["settings"] as const,
     rates: ["rates"] as const,
     remindersDue: ["reminders-due"] as const,
     onboarding: ["onboarding"] as const,
     pinEnabled: ["pin-enabled"] as const,
     appInfo: ["app-info"] as const,
     backups: ["backups"] as const,
     syncStatus: ["sync-status"] as const,
     pairingRequests: ["pairing-requests"] as const,
     conflicts: ["conflicts"] as const,
     tgStatus: ["tg-status"] as const,
   };

   /** Keys that must NOT refetch on "data-changed" (slow or unrelated to the db). */
   export const NON_DATA_ROOTS = new Set([
     "db-status", "app-info", "autostart", "discover-hosts", "recovery-key", "host-addresses", "startup-error",
   ]);

   /** Everything a money movement can affect. */
   export function invalidateMoney(qc: QueryClient) {
     for (const key of [
       qk.transactions, qk.dashboard(), qk.analytics(), qk.journal(),
       qk.entityAll("accounts"), qk.entityAll("attachments"), qk.entityAll("goals"),
       ["tx"], ["tx-days"],
     ]) {
       void qc.invalidateQueries({ queryKey: key });
     }
   }
   ```
   Ключи `["tx"]` и `["tx-days"]` — временные (legacy), их удалит шаг 27.
2. `src/lib/queries.ts`:
   ```ts
   import { useQuery } from "@tanstack/react-query";
   import { api } from "./api";
   import { qk } from "./queryKeys";
   import type { AccountRow, CategoryRow, UserRow } from "./types";

   export const useUsers = () =>
     useQuery({ queryKey: qk.entity("users"), queryFn: () => api.entityList<UserRow>("users") });
   export const useAccounts = (includeDeleted = false) =>
     useQuery({ queryKey: qk.entity("accounts", includeDeleted), queryFn: () => api.entityList<AccountRow>("accounts", includeDeleted) });
   export const useCategories = (includeDeleted = false) =>
     useQuery({ queryKey: qk.entity("categories", includeDeleted), queryFn: () => api.entityList<CategoryRow>("categories", includeDeleted) });
   export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: api.getSettings });
   ```

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 21. Операции: TxForm, TransactionsPage, DayCalendar

**Файлы:** `src/components/transactions/TxForm.tsx`, `src/pages/TransactionsPage.tsx`, `src/components/transactions/DayCalendar.tsx`

Правила замены (одинаковы для шагов 21–26):

| Было | Стало |
|---|---|
| `useQuery({ queryKey: ["users"], queryFn: () => api.entityList<…>("users") })` | `useUsers()` |
| `useQuery({ queryKey: ["accounts"], … entityList("accounts") })` | `useAccounts()` |
| `useQuery({ queryKey: ["categories"], … entityList("categories") })` | `useCategories()` |
| `… entityList("categories", true)` | `useCategories(true)` |
| `useQuery({ queryKey: ["settings"], queryFn: api.getSettings })` | `useSettings()` |
| `queryKey: ["transactions", "page", …rest]` | `queryKey: qk.txList("page", …rest)` |
| `queryKey: ["tx-days", month]` | `queryKey: qk.txDays(month)` |
| `queryKey: ["attachments"]` + `entityList("attachments")` | `queryKey: qk.entity("attachments")` |
| группа `invalidateQueries` для transactions/tx-days/dashboard/attachments | `invalidateMoney(qc)` |

Импорты: `import { qk, invalidateMoney } from "@/lib/queryKeys";` и
`import { useUsers, useAccounts, useCategories, useSettings } from "@/lib/queries";`
(только то, что используется — иначе ругнётся линтер). Неиспользуемые
импорты `useQuery`/`api` удалите.

**Проверка:** `npx tsc --noEmit && npm run lint && npm test`

---

#### Шаг 22. Главная, счета, просмотр категории

**Файлы:** `src/pages/DashboardPage.tsx`, `src/pages/AccountsPage.tsx`, `src/components/transactions/CategoryTxDialog.tsx`

Те же правила, что в шаге 21, плюс:
- `["dashboard", from, to]` → `qk.dashboard(from, to)`;
  `["dashboard", "accounts"]` → `qk.dashboard("accounts")`;
- `["transactions", "dash", from, to]` → `qk.txList("dash", from, to)`;
- `["transactions", "cat-peek", …]` → `qk.txList("cat-peek", …)`;
- в `AccountsPage` все пары `invalidateQueries(["accounts"])` +
  `invalidateQueries(["dashboard"])` замените на `invalidateMoney(qc)`.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 23. Аналитика и отчёты (ключи) + утечка печати (F2, F8)

**Файлы:** `src/pages/AnalyticsPage.tsx`, `src/pages/ReportsPage.tsx`

1. Ключи: `["tx", "analytics", a, b]` → `qk.txList("analytics", a, b)`,
   `["tx", "analytics-m6", x]` → `qk.txList("analytics-m6", x)`,
   `["tx", "report", from, to]` → `qk.txList("report", from, to)`; плюс правила
   из шага 21 для users/categories/accounts.
2. `ReportsPage.printPdf`: замените последние 4 строки функции на
   ```ts
   const url = URL.createObjectURL(new Blob([html], { type: "text/html" }));
   const iframe = document.createElement("iframe");
   iframe.style.display = "none";
   const cleanup = () => {
     URL.revokeObjectURL(url);
     iframe.remove();
   };
   iframe.onload = () => {
     const w = iframe.contentWindow;
     if (!w) return cleanup();
     w.onafterprint = cleanup;
     w.print();
     window.setTimeout(cleanup, 60_000);
   };
   iframe.src = url;
   document.body.appendChild(iframe);
   ```
   Повторный вызов `cleanup` безопасен: `remove()` и `revokeObjectURL`
   идемпотентны.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 24. Категории, цели, напоминания (коллизии F1)

**Файлы:** `src/pages/CategoriesPage.tsx`, `src/pages/GoalsPage.tsx`, `src/pages/RemindersPage.tsx`

- `CategoriesPage`: `useCategories(true)`; `invalidate` →
  `qc.invalidateQueries({ queryKey: qk.entityAll("categories") })`
  (сбрасывает обе версии: с удалёнными и без).
- `GoalsPage`: `queryKey: qk.entity("goals", true)`; `invalidate` →
  `qk.entityAll("goals")` + `qk.entityAll("accounts")`.
- `RemindersPage`: список → `qk.entity("reminders", true)`; due →
  `qk.remindersDue`; `invalidate` → `qk.entityAll("reminders")` +
  `qk.remindersDue`. После `resolve` (может создать операцию) дополнительно
  вызовите `invalidateMoney(qc)`.
- Остальное — по правилам шага 21.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 25. Журнал, запрос привязки, «Что нового»

**Файлы:** `src/pages/JournalPage.tsx`, `src/components/PairingPrompt.tsx`, `src/components/WhatsNewDialog.tsx`

- `JournalPage`: `["journal", search, entity]` → `qk.journal(search, entity)`;
  `["reminders"]` → `qk.entity("reminders")`; правила шага 21;
  `restore.onSuccess: () => qc.invalidateQueries()` оставить как есть
  (восстановление может затронуть любую сущность).
- `PairingPrompt`: `qk.pairingRequests`, `qk.syncStatus`, `useUsers()`.
- `WhatsNewDialog`: `qk.appInfo`, `useSettings()`, инвалидация `qk.settings`.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 26. Настройки, курсы, онбординг (F3)

**Файлы:** `src/pages/SettingsPage.tsx`, `src/lib/useDisplayMoney.ts`, `src/app/Onboarding.tsx`

- `SettingsPage`: все литеральные ключи → поля `qk` (`tgStatus`, `syncStatus`,
  `pairingRequests`, `conflicts`, `pinEnabled`, `backups`, `appInfo`,
  `settings`, `rates`). Ключи `["host-addresses"]` и `["autostart"]`
  оставьте литералами: они в `NON_DATA_ROOTS`. Пользователей получайте через `useUsers()`.
- `useDisplayMoney.ts`: `["settings"]` → `useSettings()`, `["rates"]` → `qk.rates`.
- `Onboarding.tsx`, **исправление бага F3**: замените
  `queryClient.invalidateQueries({ queryKey: ["onboarding", "settings"] });` на
  ```ts
  void queryClient.invalidateQueries({ queryKey: qk.onboarding });
  void queryClient.invalidateQueries({ queryKey: qk.settings });
  ```

**Проверка:** `npx tsc --noEmit && npm run lint && npm test`

---

#### Шаг 27. Убрать legacy-ключи

**Файлы:** `src/lib/queryKeys.ts`

1. Удалите `["tx"], ["tx-days"],` из `invalidateMoney`.
2. Убедитесь, что литеральных ключей не осталось:
   ```bash
   grep -rn 'queryKey: \["' src --include=*.tsx --include=*.ts | grep -v "host-addresses\|autostart\|db-status\|discover-hosts\|recovery-key\|startup-error"
   ```
   Вывод должен быть пустым (кроме `src/lib/queryKeys.ts`, если grep его зацепит).
   Если что-то нашлось, переведите это на `qk` в этом же шаге.

**Проверка:** `npx tsc --noEmit && npm run lint && npm test`

---

#### Шаг 28. Backend: событие `data-changed`

**Файлы:** `src-tauri/src/watch.rs` (новый), `src-tauri/src/lib.rs`

1. `src-tauri/src/watch.rs`:
   ```rust
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
                       Ok(c.query_row("SELECT COALESCE(MAX(rowid),0) FROM audit_log", [], |r| r.get::<_, i64>(0))?)
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
   ```
2. В `lib.rs`: `mod watch;` и в `setup` после `sync::start_client(...)`
   строка `watch::start(app.handle().clone());`.

Разрешение `core:default` уже включает `event:default` (подписку `listen`),
capabilities менять не нужно.

**Проверка:** `cargo check -p finance-app && cargo clippy -p finance-app -- -D warnings`

---

#### Шаг 29. Frontend: подписка на `data-changed`

**Файлы:** `src/lib/useDataChanged.ts` (новый), `src/app/App.tsx`

1. `src/lib/useDataChanged.ts`:
   ```ts
   import { useEffect } from "react";
   import { listen } from "@tauri-apps/api/event";
   import { useQueryClient } from "@tanstack/react-query";
   import { NON_DATA_ROOTS } from "./queryKeys";

   /** Refetches active db-backed queries when the backend reports a change. */
   export function useDataChanged() {
     const qc = useQueryClient();
     useEffect(() => {
       let timer: number | undefined;
       const unlisten = listen<number>("data-changed", () => {
         window.clearTimeout(timer);
         timer = window.setTimeout(() => {
           void qc.invalidateQueries({ predicate: (q) => !NON_DATA_ROOTS.has(String(q.queryKey[0])) });
         }, 300);
       });
       return () => {
         window.clearTimeout(timer);
         void unlisten.then((f) => f());
       };
     }, [qc]);
   }
   ```
2. `App.tsx`: импортируйте хук и вызовите `useDataChanged();` в начале
   `App()`, сразу после `const init = ...` и **до** любых `return`
   (правило хуков).

**Проверка:** `npx tsc --noEmit && npm run lint`. Ручная проверка: `npm run tauri dev`,
добавьте операцию через бота — Главная обновится в течение ~3 с без
переключения страниц.

---

#### Шаг 30. Целочисленный пересчёт валют на фронте (F7)

**Файлы:** `src/lib/money.ts`, `src/lib/money.test.ts`, `src/lib/useDisplayMoney.ts`

1. В `money.ts` добавьте:
   ```ts
   /** Rate pair = (rateBynScaled, scale), same formula as Rust convert_with:
    *  amount * fRate/fScale * tScale/tRate, rounded half away from zero. */
   export function convertMinor(
     minor: number,
     from: { rate: number; scale: number },
     to: { rate: number; scale: number },
   ): number | null {
     if (from.rate <= 0 || to.rate <= 0 || from.scale <= 0 || to.scale <= 0) return null;
     const num = BigInt(minor) * BigInt(from.rate) * BigInt(to.scale);
     const den = BigInt(from.scale) * BigInt(to.rate);
     const half = den / 2n;
     const v = num >= 0n ? (num + half) / den : (num - half) / den;
     return Number(v);
   }
   ```
2. В `money.test.ts` добавьте тесты: BYN→BYN без изменений; 100 USD
   (`rate=3_250_000`) → BYN = 325; округление 0,5 от нуля для отрицательных;
   нулевой курс → `null`; большое значение `1e12` не теряет точность.
3. В `useDisplayMoney.ts` в `toDisplay`:
   ```ts
   const pair = (c: string) =>
     c === "BYN" ? { rate: 1_000_000, scale: 1 } : (() => {
       const r = rates.data?.find((x) => x.currency === c);
       return r ? { rate: r.rateBynScaled, scale: r.scale || 1 } : null;
     })();
   const f = pair(cur);
   const tgt = pair(displayCurrency);
   if (!f || !tgt) return null;
   return convertMinor(minor, f, tgt);
   ```

**Проверка:** `npm test && npx tsc --noEmit && npm run lint`. Затем полная
проверка фазы и `CHANGELOG.md` («Кэш интерфейса: без коллизий и устаревших
данных, автообновление при изменениях из бота и синка, точный пересчёт валют»).

---

### Фаза 4. Аналитика в Rust (одна реализация для UI и бота)

#### Шаг 31. Модуль `storage::analytics`

**Файлы:** `crates/storage/src/analytics.rs` (новый), `crates/storage/src/lib.rs`

1. `analytics.rs`:
   ```rust
   //! Period sums for the UI and the bot, converted to one currency with the
   //! same integer math as repos::convert_minor. Group sums are converted
   //! per (kind, account currency) group, then added.

   use std::collections::{BTreeMap, BTreeSet};

   use rusqlite::Connection;
   use serde::Serialize;

   use crate::error::Error;
   use crate::repos;

   #[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
   #[serde(rename_all = "camelCase")]
   pub struct Bucket {
       pub key: Option<String>,
       pub income: i64,
       pub expense: i64,
   }

   #[derive(Debug, Clone, Default, Serialize)]
   #[serde(rename_all = "camelCase")]
   pub struct Summary {
       pub currency: String,
       pub income: i64,
       pub expense: i64,
       pub missing: Vec<String>,
       pub by_day: Vec<Bucket>,
       pub by_category: Vec<Bucket>,
       pub by_owner: Vec<Bucket>,
       pub by_account: Vec<Bucket>,
   }

   #[derive(Clone, Copy)]
   enum Dim { Total, Day, Month, Category, Owner, Account }

   impl Dim {
       fn sql(self) -> &'static str {
           match self {
               Dim::Total => "''",
               Dim::Day => "substr(t.occurred_at,1,10)",
               Dim::Month => "substr(t.occurred_at,1,7)",
               Dim::Category => "t.category_id",
               Dim::Owner => "COALESCE(t.spent_by_user_id, t.created_by_user_id)",
               Dim::Account => "t.account_id",
           }
       }
   }

   fn grouped(
       conn: &Connection,
       from: &str,
       to: &str,
       cur: &str,
       dim: Dim,
       missing: &mut BTreeSet<String>,
   ) -> Result<Vec<Bucket>, Error> {
       let sql = format!(
           "SELECT t.kind, a.currency, {} AS k, SUM(t.amount_minor)
            FROM transactions t JOIN accounts a ON a.id = t.account_id
            WHERE t.deleted_at IS NULL AND t.kind != 'transfer'
              AND t.occurred_at >= ?1 || 'T00:00:00' AND t.occurred_at <= ?2 || 'T23:59:59.999'
            GROUP BY t.kind, a.currency, k",
           dim.sql()
       );
       let mut st = conn.prepare_cached(&sql)?;
       let rows: Vec<(String, String, Option<String>, i64)> = st
           .query_map(rusqlite::params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
           .collect::<Result<_, _>>()?;
       let mut acc: BTreeMap<Option<String>, Bucket> = BTreeMap::new();
       for (kind, currency, key, sum) in rows {
           let Some(v) = repos::convert_minor(conn, sum, &currency, cur)? else {
               missing.insert(currency);
               continue;
           };
           let b = acc.entry(key.clone()).or_insert_with(|| Bucket { key, ..Default::default() });
           if kind == "income" {
               b.income = b.income.saturating_add(v);
           } else {
               b.expense = b.expense.saturating_add(v);
           }
       }
       Ok(acc.into_values().collect())
   }

   fn by_size(mut v: Vec<Bucket>) -> Vec<Bucket> {
       v.sort_by_key(|b| std::cmp::Reverse(b.income.saturating_add(b.expense)));
       v
   }

   pub fn summary(conn: &Connection, from: &str, to: &str, currency: &str) -> Result<Summary, Error> {
       let mut missing = BTreeSet::new();
       let total = grouped(conn, from, to, currency, Dim::Total, &mut missing)?;
       let t = total.into_iter().next().unwrap_or_default();
       Ok(Summary {
           currency: currency.to_string(),
           income: t.income,
           expense: t.expense,
           by_day: grouped(conn, from, to, currency, Dim::Day, &mut missing)?,
           by_category: by_size(grouped(conn, from, to, currency, Dim::Category, &mut missing)?),
           by_owner: by_size(grouped(conn, from, to, currency, Dim::Owner, &mut missing)?),
           by_account: by_size(grouped(conn, from, to, currency, Dim::Account, &mut missing)?),
           missing: missing.into_iter().collect(),
       })
   }

   pub fn monthly(conn: &Connection, from: &str, to: &str, currency: &str) -> Result<Vec<Bucket>, Error> {
       let mut missing = BTreeSet::new();
       grouped(conn, from, to, currency, Dim::Month, &mut missing)
   }
   ```
   `dim.sql()` возвращает только `&'static str` из фиксированного enum:
   SQL-инъекция невозможна.
2. В `lib.rs`: `pub mod analytics;` (по алфавиту, перед `pub mod backup;`).
3. Тест в `lib.rs` → `mod tests`: засейдите категории, два счёта BYN и USD,
   курс USD (`repos::rate_upsert(c, &d, "2026-11-01", "USD", 3_000_000, 1)`),
   расходы 10,00 BYN и 1,00 USD на одну категорию, доход 5,00 BYN. Проверьте:
   `summary(c, "2026-11-01", "2026-11-30", "BYN")` даёт `expense == 1000 + 300`,
   `income == 500`, `by_category[0].expense == 1300`, `missing` пуст; для
   валюты без курса (`"PLN"`) — `missing == ["BYN", "USD"]` или содержит
   `"BYN"` (BYN нельзя перевести в PLN без курса PLN).

**Проверка:** `cargo test -p finance-storage`

---

#### Шаг 32. Команды `analytics_summary` и `analytics_monthly`

**Файлы:** `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

1. `commands.rs`:
   ```rust
   fn display_currency_on(c: &finance_storage::rusqlite::Connection) -> Result<String, finance_storage::Error> {
       Ok(repos::setting_get(c, "display_currency")?
           .filter(|s| s.len() == 3)
           .unwrap_or_else(|| "BYN".into()))
   }

   #[tauri::command]
   pub async fn analytics_summary(
       state: State<'_, AppState>,
       from: String,
       to: String,
       currency: Option<String>,
   ) -> Result<finance_storage::analytics::Summary, String> {
       let db = state.db.share();
       run(move || {
           db.with_read_conn(|c| {
               let cur = match currency { Some(c) => c, None => display_currency_on(c)? };
               finance_storage::analytics::summary(c, &from, &to, &cur)
           })
           .map_err(|e| e.to_string())
       })
       .await
   }
   ```
   И `analytics_monthly` по тому же образцу, возвращает
   `Vec<finance_storage::analytics::Bucket>`.
2. `lib.rs`: добавьте обе команды в `generate_handler![...]` после
   `commands::dashboard,`.

**Проверка:** `cargo check -p finance-app`

---

#### Шаг 33. Типы и вызовы на фронте

**Файлы:** `src/lib/types.ts`, `src/lib/api.ts`

1. `types.ts`:
   ```ts
   export interface Bucket { key: string | null; income: number; expense: number }
   export interface AnalyticsSummary {
     currency: string;
     income: number;
     expense: number;
     missing: string[];
     byDay: Bucket[];
     byCategory: Bucket[];
     byOwner: Bucket[];
     byAccount: Bucket[];
   }
   ```
2. `api.ts` (импортируйте типы из `./types`):
   ```ts
   analyticsSummary: (from: string, to: string, currency?: string) =>
     invoke<AnalyticsSummary>("analytics_summary", { from, to, currency }),
   analyticsMonthly: (from: string, to: string, currency?: string) =>
     invoke<Bucket[]>("analytics_monthly", { from, to, currency }),
   ```

**Проверка:** `npx tsc --noEmit`

---

#### Шаг 34. AnalyticsPage на серверных суммах

**Файлы:** `src/pages/AnalyticsPage.tsx`

1. Замените запросы `cur`, `prev`, `m6`:
   ```ts
   const cur = useQuery({ queryKey: qk.analytics("summary", r.from, r.to, displayCurrency), queryFn: () => api.analyticsSummary(r.from, r.to, displayCurrency) });
   const prev = useQuery({ queryKey: qk.analytics("summary", pr.from, pr.to, displayCurrency), queryFn: () => api.analyticsSummary(pr.from, pr.to, displayCurrency) });
   const m6 = useQuery({ queryKey: qk.analytics("monthly", m6from, displayCurrency), queryFn: () => api.analyticsMonthly(m6from, todayIso(), displayCurrency) });
   ```
   (`displayCurrency` берётся из `useDisplayMoney()`. Перенесите эту строку
   выше запросов.)
2. Удалите `conv` и `sum`. Замените:
   - `e = cur.data?.expense ?? 0`, `i = cur.data?.income ?? 0`, `pe`/`pi` — из `prev.data`;
   - `byDay`: `(cur.data?.byDay ?? []).map((b) => ({ label: (b.key ?? "").slice(5), expense: b.expense, income: b.income }))`;
   - `byMonth`: из `m6.data`, `label` считается так же, как раньше, от `b.key`
     (`YYYY-MM`); деления `/ 100` для графика оставьте как были — это
     отображение, не расчёт сумм;
   - `byCategory`: из `cur.data.byCategory`, где `expense > 0`; `id = b.key`,
     `value = b.expense`; имя, цвет и иконка ищутся в `categories.data` так же, как раньше;
   - `byPeople`: из `cur.data.byOwner`; `value = b.expense` или `b.income`
     для своего вида, отбросить нулевые; имя и цвет — из `users.data`;
     при `key == null` используется `t("ui.noAuthor")`;
   - `byAccount`: из `cur.data.byAccount` (`value = b.expense`, фильтр `> 0`).
3. Предупреждение о курсах: если `cur.data?.missing.length`, покажите тот же
   текст, что и раньше при `null` от `toDisplay`. Если такого текста не было,
   ничего не добавляйте.
4. Удалите ставшие ненужными импорты (`TransactionRow`, `ownerOf`, …).

Визуальная разметка (JSX), `DonutChart`, логика «Прочее» **не меняются**.

**Проверка:** `npx tsc --noEmit && npm run lint && npm test`. Ручная проверка:
цифры на «Аналитике» совпадают с ответом бота `/summary` за тот же месяц.

---

#### Шаг 35. DashboardPage на серверных суммах

**Файлы:** `src/pages/DashboardPage.tsx`

1. Удалите запрос `tx` (`qk.txList("dash", …)`) и `converted`.
2. Добавьте
   `const sum = useQuery({ queryKey: qk.analytics("summary", from, to, displayCurrency), queryFn: () => api.analyticsSummary(from, to, displayCurrency) });`
3. `catSlices(kind)`: источник — `sum.data?.byCategory`, значение
   `kind === "expense" ? b.expense : b.income`, отбросить нули, `id = b.key`.
   `peopleSlices(kind)`: источник — `sum.data?.byOwner`.
4. `totalByn` (баланс счетов) оставьте на `toDisplay`: после шага 30 он
   целочисленный.

**Проверка:** `npx tsc --noEmit && npm run lint`

---

#### Шаг 36. Бот использует `analytics::summary`

**Файлы:** `src-tauri/src/bot/report.rs`

1. Тело `totals(state, from, to, cur)` замените вызовом
   `finance_storage::analytics::summary(c, &from.to_string(), &to.to_string(), cur)`
   внутри `with_read_conn` и преобразуйте результат в существующий `Totals`:
   - `income`/`expense` — из `Summary`;
   - `missing` — `BTreeSet` из `summary.missing`;
   - `slices` — из `by_category`, где `expense > 0`. Имя и иконку берите
     запросом `SELECT name, icon FROM categories WHERE id = ?1` (нет строки →
     `"—"` и `""`). Порядок уже по убыванию.
2. Удалите тип `SumRow` и ставший ненужным SQL.

**Проверка:** `cargo test -p finance-app`, затем полная проверка фазы и
`CHANGELOG.md` («Аналитика считается в одном месте для приложения и бота»).

---

### Фаза 5. Углубление модулей и гигиена

#### Шаг 37. Модуль `keystore` + `state.rs`

**Файлы:** `src-tauri/src/keystore.rs` (новый), `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`

1. `keystore.rs`:
   ```rust
   //! The only place that touches the OS keychain.

   const SERVICE: &str = "by.kostyan.finance";
   pub const DB_KEY: &str = "db-primary";
   pub const DEVICE_TOKEN: &str = "device-token";
   pub const BOT_TOKEN: &str = "bot-token";

   fn entry(account: &str) -> Result<keyring::Entry, String> {
       keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
   }

   pub fn read(account: &str) -> Result<Option<Vec<u8>>, String> {
       match entry(account)?.get_secret() {
           Ok(b) => Ok(Some(b)),
           Err(keyring::Error::NoEntry) => Ok(None),
           Err(e) => Err(e.to_string()),
       }
   }

   pub fn read_string(account: &str) -> Option<String> {
       read(account).ok().flatten().map(|b| String::from_utf8_lossy(&b).into_owned())
   }

   pub fn write(account: &str, secret: &[u8]) -> Result<(), String> {
       entry(account)?.set_secret(secret).map_err(|e| e.to_string())
   }

   pub fn delete(account: &str) {
       if let Ok(e) = entry(account) {
           let _ = e.delete_credential();
       }
   }
   ```
2. `state.rs`: удалите `KEY_SERVICE`, `keyring_read`, `keyring_write`.
   В `install_key_store` используйте
   `|_| crate::keystore::read(crate::keystore::DB_KEY)` и
   `|_, k| crate::keystore::write(crate::keystore::DB_KEY, k)`;
   в `unlock_with_recovery` — `crate::keystore::write(crate::keystore::DB_KEY, &key)`.
   Имя записи `"db-primary"` совпадает с прежним `format!("db-{instance}")`
   при `instance = "primary"`, так что существующие ключи пользователей читаются.
3. `lib.rs`: `mod keystore;`.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 38. `sync.rs` на `keystore`, keyring вне блокировки

**Файлы:** `src-tauri/src/sync.rs`

1. Удалите `KEY_SERVICE`, `DEVICE_TOKEN_ACCOUNT`, `save_device_token`,
   `read_device_token`. Вызовы замените на
   `crate::keystore::write(crate::keystore::DEVICE_TOKEN, token.as_bytes())` и
   `crate::keystore::read_string(crate::keystore::DEVICE_TOKEN)`.
   В `migrate_device_token` передавайте замыкание
   `|t| crate::keystore::write(crate::keystore::DEVICE_TOKEN, t.as_bytes())`.
2. `client_cfg`: сначала, **вне** `with_read_conn`, вызовите
   `let kr = crate::keystore::read_string(crate::keystore::DEVICE_TOKEN);`.
   Внутри замыкания используйте `kr.or(get("device_token").ok().flatten())`.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 39. `bot.rs` на `keystore`

**Файлы:** `src-tauri/src/bot.rs`

Удалите `KEY_SERVICE`, `TG_ACCOUNT`. Тела `save_token`, `read_token`,
`delete_token` замените однострочными вызовами `crate::keystore::{write,
read_string, delete}(crate::keystore::BOT_TOKEN, …)`. Сигнатуры функций не меняйте.

**Проверка:** `cargo test -p finance-app && grep -rn "keyring::" src-tauri/src`
(совпадения должны быть только в `keystore.rs`).

---

#### Шаг 40. Тип `Mode` (часть 1: методы-обёртки)

**Файлы:** `crates/storage/src/db.rs`

1. Над `pub struct Db` добавьте:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Mode {
       Host,
       Client,
   }

   impl Mode {
       pub fn parse(s: &str) -> Self {
           if s == "client" { Mode::Client } else { Mode::Host }
       }
       pub fn as_str(self) -> &'static str {
           match self {
               Mode::Host => "host",
               Mode::Client => "client",
           }
       }
   }
   ```
2. В `impl Db` (рядом с `share`) добавьте:
   ```rust
   pub fn mode(&self) -> Mode {
       Mode::parse(&self.mode)
   }
   pub fn is_host(&self) -> bool {
       self.mode() == Mode::Host
   }
   pub fn is_client(&self) -> bool {
       self.mode() == Mode::Client
   }
   ```
3. В этом же файле замените 4 сравнения: `ctx.db.mode == "client"` →
   `ctx.db.is_client()`, `ctx.db.mode == "host"` → `ctx.db.is_host()`.

**Проверка:** `cargo test -p finance-storage`

---

#### Шаг 41. Тип `Mode` (часть 2: вызывающие в приложении)

**Файлы:** `src-tauri/src/sync.rs`, `src-tauri/src/commands.rs`

- `sync.rs`: `state.db.mode != "host"` → `!state.db.is_host()`;
  `state.db.mode != "client"` → `!state.db.is_client()`;
  `"mode": db.mode` → `"mode": db.mode().as_str()`.
- `commands.rs`: `"mode": db.mode` → `"mode": db.mode().as_str()`.

**Проверка:** `cargo check --workspace`

---

#### Шаг 42. Тип `Mode` (часть 3: поле становится enum)

**Файлы:** `crates/storage/src/db.rs`, `crates/storage/src/lib.rs`

1. `db.rs`: поле `pub mode: String` → `pub mode: Mode`. В `share()`:
   `mode: self.mode,`. Метод `mode()` теперь `self.mode`, удалите
   `Mode::parse(&self.mode)`.
2. `lib.rs` → `init_conn`: `let mode = db::Mode::parse(&meta("mode").unwrap_or_else(|| "host".into()));`.
3. `set_mode(&self, mode: &str)` → `set_mode(&self, mode: Mode)`, в SQL
   передавайте `mode.as_str()`. В `src-tauri/src/sync.rs` вызов
   `set_mode("client")` → `set_mode(finance_storage::db::Mode::Client)`.
   Это третий файл шага.

**Проверка:** `cargo test --workspace`

---

#### Шаг 43. `actor_on` переезжает в `state.rs`

**Файлы:** `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`

1. Перенесите функцию `actor_on` из `commands.rs` в `state.rs` как
   `pub(crate) fn actor_on(...)` (тело без изменений, импорты `repos`,
   `Source`, `Actor` уже есть или добавьте их).
2. В `commands.rs` добавьте `use crate::state::actor_on;`. Удалите
   `#[allow(dead_code)] pub fn actor(...)` из `impl AppState`, если он
   нигде не используется (`grep -rn "\.actor(" src-tauri/src`).

**Проверка:** `cargo check -p finance-app`

---

#### Шаг 44. `AppState::read` и `AppState::write`

**Файлы:** `src-tauri/src/state.rs`

```rust
use finance_storage::rusqlite::Connection;

impl AppState {
    /// Read-only query on the blocking pool.
    pub async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, finance_storage::Error> + Send + 'static,
    ) -> Result<T, String> {
        let db = self.db.share();
        crate::blocking::run(move || db.with_read_conn(f).map_err(|e| e.to_string())).await
    }

    /// One transaction on the blocking pool, with the desktop actor resolved inside it.
    pub async fn write<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection, &Db, &Actor) -> Result<T, finance_storage::Error> + Send + 'static,
    ) -> Result<T, String> {
        let db = self.db.share();
        crate::blocking::run(move || {
            db.write(|tx| {
                let c: &Connection = tx;
                let a = actor_on(c, &db)?;
                f(c, &db, &a)
            })
            .map_err(|e| e.to_string())
        })
        .await
    }
}
```

**Проверка:** `cargo check -p finance-app` (предупреждение о неиспользуемых
методах исчезнет в шаге 45; при запуске clippy с `-D warnings` временно
допустимо `#[allow(dead_code)]` над двумя методами — **удалите его в шаге 45**).

---

#### Шаг 45. Команды CRUD на `state.read/write` (C5)

**Файлы:** `src-tauri/src/commands.rs`

Переведите команды разделов `generic entity CRUD`, `onboarding / bootstrap`,
`settings`. Образец:

```rust
#[tauri::command]
pub async fn account_upsert(state: State<'_, AppState>, input: Value) -> Result<Value, String> {
    state.write(move |c, db, a| repos::account_upsert(c, db, a, input)).await
}

#[tauri::command]
pub async fn transaction_list(state: State<'_, AppState>, filter: Value) -> Result<Vec<Value>, String> {
    let f: repos::TransactionFilter = serde_json::from_value(filter).map_err(|e| e.to_string())?;
    state.read(move |c| repos::transaction_list(c, &f)).await
}
```

Сигнатуры команд (имена, параметры, возвращаемые типы) **не меняются**:
от них зависит фронт. Удалите `#[allow(dead_code)]` из шага 44.

**Проверка:** `cargo clippy -p finance-app -- -D warnings && cargo test -p finance-app`

---

#### Шаг 46. Остальные команды на `state.read/write`

**Файлы:** `src-tauri/src/commands.rs`

То же для разделов `goals/reminders`, `security & backups` (кроме
`backup_create`/`backup_restore`/`backup_list`), `analytics_*`, `rates_latest`,
`export_xlsx`, `attachment_add` (байты клонируйте до вызова), `journal`,
`dashboard`, `transaction_days`.

**Проверка:** `cargo clippy -p finance-app -- -D warnings && cargo test -p finance-app`

---

#### Шаг 47. Защищённые ключи настроек (C9)

**Файлы:** `src-tauri/src/commands.rs`

1. Добавьте:
   ```rust
   /// Settings only the backend may write (sync state, onboarding, PIN, bot link).
   const PROTECTED_SETTINGS: &[&str] = &[
       "pin_hash", "pin_salt", "current_user_id", "onboarding_done", "last_seq",
       "host_url", "device_token", "auto_backup_month", "tg_bot_username", "tg_enabled",
   ];

   pub(crate) fn setting_writable(key: &str) -> bool {
       !key.is_empty() && key.len() <= 64 && !PROTECTED_SETTINGS.contains(&key)
   }
   ```
2. В `set_setting` первой строкой:
   `if !setting_writable(&key) { return Err("setting is read-only".into()); }`.
3. Тест в `mod tests`: `theme`, `language`, `display_currency`,
   `bot_daily_enabled` — разрешены; все ключи из `PROTECTED_SETTINGS` и `""`
   — запрещены.

Перед шагом проверьте, что фронт не пишет защищённые ключи через `setSetting`:
`grep -rn "setSetting(" src`. Сейчас он пишет только `theme`, `language`,
`whatsnew`-ключ и ключи из `useLocalSettings`. Если среди ключей
`useLocalSettings` в `SettingsPage.tsx` есть защищённый, остановитесь и сообщите.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 48. Равномерные числовые коды (C7)

**Файлы:** `crates/storage/src/secrets.rs`, `src-tauri/src/bot.rs`, `src-tauri/src/sync.rs`

1. `secrets.rs`:
   ```rust
   /// Uniform decimal code from the OS CSPRNG (rejection sampling: bytes
   /// >= 250 are dropped so every digit has the same probability).
   pub fn numeric_code(len: usize) -> String {
       let mut out = String::with_capacity(len);
       while out.len() < len {
           for b in uuid::Uuid::new_v4().into_bytes() {
               if b < 250 && out.len() < len {
                   out.push(char::from(b'0' + b % 10));
               }
           }
       }
       out
   }
   ```
   плюс тест: длина 6, только цифры.
2. В `bot.rs::tg_pair_code` и `sync.rs::pair_code` замените генерацию
   `(0..6).map(|_| ...)...collect()` на `finance_storage::secrets::numeric_code(6)`.

**Проверка:** `cargo test --workspace`

---

#### Шаг 49. Единый журнал `applog`

**Файлы:** `src-tauri/src/applog.rs` (новый), `src-tauri/src/lib.rs`, `src-tauri/src/bot.rs`

1. `applog.rs`:
   ```rust
   //! Append-only text logs in <appdata>/logs, rotated at 1 MB. Never pass secrets.

   use std::io::Write;
   use std::path::PathBuf;
   use std::sync::OnceLock;

   static DIR: OnceLock<PathBuf> = OnceLock::new();

   pub fn init(app: &tauri::AppHandle) {
       use tauri::Manager;
       if let Ok(dir) = app.path().app_data_dir() {
           let dir = dir.join("logs");
           let _ = std::fs::create_dir_all(&dir);
           let _ = DIR.set(dir);
       }
   }

   pub fn path(file: &str) -> Option<PathBuf> {
       DIR.get().map(|d| d.join(file))
   }

   pub fn append(file: &str, message: &str) {
       let Some(path) = path(file) else { return };
       if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
           let _ = std::fs::rename(&path, path.with_extension("log.1"));
       }
       if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
           let _ = writeln!(f, "{} {message}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
       }
   }
   ```
2. `lib.rs`: `mod applog;`. Первой строкой `setup` вызовите
   `applog::init(app.handle());`. В ветке `Err(e) =>` замените ручную запись
   `startup.log` на `applog::append("startup.log", &e);`.
3. `bot.rs`: удалите `LOG_DIR` и `log_path()`. В `log()` замените файловую
   часть на `crate::applog::append("bot.log", message);` (обновление
   `STATUS` оставьте). В `start_polling` удалите блок `LOG_DIR.set`. В
   `tg_status` замените `log_path()` на `crate::applog::path("bot.log")`.

**Проверка:** `cargo test -p finance-app`

---

#### Шаг 50. `sync.rs` пишет в `sync.log` вместо `eprintln!`

**Файлы:** `src-tauri/src/sync.rs`

Три `eprintln!(…)` (строки около 67, 69, 157) замените на
`crate::applog::append("sync.log", &format!(…))` с тем же текстом.

**Проверка:** `cargo clippy -p finance-app -- -D warnings`

---

#### Шаги 51–55. Разрезать `SettingsPage.tsx` (F9)

Общие правила для 51–55:
- Создаётся папка `src/pages/settings/`.
- Функции **переносятся без изменений**. Меняются только `export` и импорты.
- В `SettingsPage.tsx` перенесённые функции удаляются и импортируются
  из нового файла.
- После каждого шага: `npx tsc --noEmit && npm run lint`.

| Шаг | Новый файл | Что перенести (функции из `SettingsPage.tsx`) |
|---|---|---|
| 51 | `src/pages/settings/Section.tsx` + `src/pages/settings/useLocalSettings.ts` | `Section` (export), `useLocalSettings` (export) |
| 52 | `src/pages/settings/RatesSection.tsx` | `RatesSection`, `DisplayCurrency`, `DashboardCharts` |
| 53 | `src/pages/settings/BotSection.tsx` | `TgSection`, `BotNotifySection` |
| 54 | `src/pages/settings/SyncSection.tsx` | `SyncSection`, `ConflictsBlock` |
| 55 | `src/pages/settings/SecuritySection.tsx` | `SecuritySection`, `BackupSection`, `UpdateCheckButton` |

После шага 55 в `SettingsPage.tsx` остаётся только `export function SettingsPage()`.

**Проверка в конце фазы:** полная проверка + `CHANGELOG.md` («Внутренняя
реструктуризация: единые модули секретов, логов, режима работы; настройки
разбиты на секции; служебные ключи защищены»).

---

## 4. Отложенное (Speculative): делать только после отдельного обсуждения

1. **Разрезать `src-tauri/src/bot/wizard.rs` (2432 строки)** на `wizard/draft.rs`
   (тип `Draft` и шаги), `wizard/render.rs` (тексты и клавиатуры),
   `wizard/persist.rs` (все обращения к БД). Сначала нужно покрыть тестами
   `persist`, иначе механический перенос рискован. Заодно `rows()`/`find()`
   перестанут читать всю таблицу ради одной строки.
2. **Типизированные ошибки команд**: `#[derive(Serialize)] struct CmdError { code, message }`
   вместо `String`. Это позволит удалить разбор строк в `src/lib/syncError.ts`.
   Затрагивает все команды и весь фронт, поэтому только после шагов 45–46.
3. **Колонка `conflicts.device_id` (схема v7)** вместо `json_extract` из шага 7.
   Меняет `SCHEMA_VERSION`: клиенты v6 перестанут синхронизироваться
   (`schema_gate` требует точного совпадения). Объединять с другим
   изменением схемы.
4. **Переключение host ↔ client без перезапуска**: отклонено (раздел 2), при
   необходимости — отдельный ADR.
5. **Усиление PIN** (Argon2 + лимит попыток): отклонено (раздел 2).
6. Типизированный `api.entityList` (перегрузки `EntityName → Row`) вместо
   ручного `<T>` у каждого вызова.

---

## 5. Сводная таблица шагов

| Шаг | Фаза | Файлы | Проверка |
|---|---|---|---|
| 1 | 1 | storage/lib.rs | `cargo test -p finance-storage write_rolls_back_on_error` |
| 2 | 1 | app/commands.rs | `cargo test -p finance-app` |
| 3 | 1 | app/bot/wizard.rs, app/bot/notify.rs, app/bot.rs | `cargo check -p finance-app` |
| 4 | 1 | app/sync.rs | `cargo test -p finance-app` |
| 5 | 1 | api/lib.rs | `cargo test -p finance-api` |
| 6 | 1 | api/lib.rs, api/tests/push.rs | `cargo test -p finance-api --test push` |
| 7 | 1 | api/lib.rs, api/tests/push.rs | `cargo test -p finance-api --test push` |
| 8 | 1 | app/sync.rs | `cargo test -p finance-app` |
| 9 | 1 | storage/db.rs, storage/lib.rs | `cargo test --workspace` |
| 10 | 1 | storage/migrations.rs, storage/lib.rs | `cargo test -p finance-storage newer_schema_is_refused` |
| 11 | 1 | storage/backup.rs, storage/lib.rs | `cargo test -p finance-storage pre_migration_snapshot_is_encrypted` |
| 12 | 1 | fe/pages/SettingsPage.tsx | `npx tsc --noEmit && npm run lint` |
| 13 | 1 | storage/backup.rs, storage/lib.rs | `cargo test -p finance-storage` |
| 14 | 1 | app/commands.rs, app/scheduler.rs | полная проверка фазы |
| 15 | 2 | app/blocking.rs, app/commands.rs, app/lib.rs | `cargo check -p finance-app` |
| 16 | 2 | app/sync.rs | `cargo test -p finance-app` |
| 17 | 2 | api/lib.rs | `cargo test -p finance-api` |
| 18 | 2 | app/bot.rs | `cargo test -p finance-app` |
| 19 | 2 | app/rates.rs, app/commands.rs, app/lib.rs (+2 строки в wizard.rs, scheduler.rs) | полная проверка фазы |
| 20 | 3 | fe/lib/queryKeys.ts, fe/lib/queries.ts | `npx tsc --noEmit` |
| 21–27 | 3 | страницы и компоненты (см. шаги) | `npx tsc --noEmit && npm run lint` |
| 28 | 3 | app/watch.rs, app/lib.rs | `cargo clippy -p finance-app` |
| 29 | 3 | fe/lib/useDataChanged.ts, fe/app/App.tsx | `npx tsc --noEmit` |
| 30 | 3 | fe/lib/money.ts, money.test.ts, useDisplayMoney.ts | полная проверка фазы |
| 31 | 4 | storage/analytics.rs, storage/lib.rs | `cargo test -p finance-storage` |
| 32 | 4 | app/commands.rs, app/lib.rs | `cargo check -p finance-app` |
| 33 | 4 | fe/lib/types.ts, fe/lib/api.ts | `npx tsc --noEmit` |
| 34 | 4 | fe/pages/AnalyticsPage.tsx | `npm test` |
| 35 | 4 | fe/pages/DashboardPage.tsx | `npx tsc --noEmit` |
| 36 | 4 | app/bot/report.rs | полная проверка фазы |
| 37–39 | 5 | app/keystore.rs, state.rs, sync.rs, bot.rs | `cargo test -p finance-app` |
| 40–42 | 5 | storage/db.rs, storage/lib.rs, app/sync.rs, app/commands.rs | `cargo test --workspace` |
| 43–46 | 5 | app/state.rs, app/commands.rs | `cargo clippy -p finance-app -- -D warnings` |
| 47 | 5 | app/commands.rs | `cargo test -p finance-app` |
| 48 | 5 | storage/secrets.rs, app/bot.rs, app/sync.rs | `cargo test --workspace` |
| 49–50 | 5 | app/applog.rs, app/lib.rs, app/bot.rs, app/sync.rs | `cargo clippy` |
| 51–55 | 5 | fe/pages/settings/*.tsx, SettingsPage.tsx | полная проверка фазы |
