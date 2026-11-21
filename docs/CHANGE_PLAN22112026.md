# План изменений: курсы БСБ Банка в форме операции и в боте

Дата: 2026-11-22. Проект: корень этого репозитория (git-bash, ветка `main`).

## Зачем

Банк списывает и зачисляет деньги по своим курсам, а приложение подставляет
курс Нацбанка. Из-за разницы суммы на картах расходятся с банком и их
приходится постоянно корректировать вручную. Цель — подсказка суммы в другой
валюте должна совпадать с тем, что реально проведёт БСБ Банк (основной банк
семьи), а курс Нацбанка остаётся выбором по желанию.

### Цели

- Курс БСБ по умолчанию в форме операции (`TxForm`) и в мастере бота.
- Тип курса выбирается сам по сценарию операции (таблица ниже).
- Ручной выбор курса в операции: «БСБ приложение / БСБ по карточкам / Нацбанк».
- Курс берётся на момент операции (дата + время).
- Подсказка «фактический курс и отклонение» при ручном вводе — ловит опечатки.
- Источник курса сохраняется в операции.

### Не цели

- Старые операции не пересчитываются и не помечаются (решение пользователя).
- Итоги, капитал, отчёты и диаграммы остаются по Нацбанку (`useDisplayMoney`,
  `repos::convert_minor*`, `bot/report.rs`) — не трогать.
- Другие банки не подключаем (только БСБ + НБРБ), но не мешаем добавить потом.

### Анти-цели (провал, даже если «работает»)

- Сайт БСБ недоступен → форма или бот ломаются / не дают сохранить.
- Приложение само меняет уже введённую сумму без действия пользователя.
- Старое устройство при синхронизации затирает новые поля.

## Источник данных (проверено 2026-11-22)

Внутренний API сайта БСБ, без ключа и без документации:

```
POST https://mobile.bsb.by/api/v1/free-zone-management/exchange-rates/rates
Content-Type: application/json
Accept-Language: ru
{"type":"CARD_ONLINE","bankDepartmentId":7,"period":<unix ms>}
```

- `type`: `CARD_ONLINE` — курс в приложении/интернет-банке (совпадает с
  Myfin «BSB-Bank App»), `CARD` — курс по карточкам, ещё `CASH`,
  `NATIONAL_BANK_BELARUS` (не используем).
- `period` — любой момент; ответ — курс, действовавший тогда. История есть.
- Ответ: `{"fromTime":ms,"nextTime":ms|null,"previousTime":ms,"rates":[...]}`;
  элемент: `buyCurrencyName` (USD), `sellCurrencyName` (BYN),
  `scaledBuyAmount` / `scaledSellAmount` — строки с курсом за
  `buyCurrencyScale` единиц (RUB: `"3.4753"` за 100), `buyAmount`/`sellAmount` — float за 1 (не использовать).
  `nextTime == null` — курс действует сейчас.
- Пары к BYN: USD, EUR, RUB (×100), PLN (×10). Кроссы (USD/RUB, EUR/USD…) игнорируем.
- `CARD` меняется ~раз в рабочий день, `CARD_ONLINE` — много раз в день.

Сверка с реальными операциями подтвердила: переводы между своими картами —
`CARD_ONLINE`; доход BYN на USD-карту (P2P) — `CARD` продажа; траты в BYN с
USD-карты — ближе всего `CARD_ONLINE` покупка.

## Правило выбора курса (утверждено)

Деньги текут из валюты `from` в валюту `to`. Банк покупает валюту, которую
отдаёте вы (курс покупки), и продаёт ту, что получаете (курс продажи):
`BYN = X × buy(X)`, `Y = BYN / sell(Y)`.

| Операция | from → to | Известная сумма | Курс по умолчанию | Режим |
|---|---|---|---|---|
| Перевод между счетами разных валют | счёт → счёт назначения | from | БСБ приложение | заполнить |
| Расход, введён в другой валюте E | валюта счёта → E | to (E) | БСБ приложение | заполнить |
| Доход в другой валюте E ≠ RUB | E → валюта счёта | from (E) | БСБ по карточкам | заполнить |
| Доход в RUB | RUB → валюта счёта | from | БСБ приложение | **только справка** |
| Счёт (любой из участников) не в БСБ | — | — | Нацбанк | заполнить |

- «Справка»: показываем «Для справки: ≈ X BYN · БСБ приложение», кнопки
  подстановки нет, сумму пользователь вводит руками.
- Валюта, которой нет в ответе БСБ (CNY, GBP…), или ошибка сети → Нацбанк,
  флаг `fallback` и пометка «курс БСБ недоступен, по Нацбанку».
- Обе валюты иностранные (EUR с USD-счёта) — через BYN двумя ногами:
  `to = from × buy(from) / sell(to)`.
- Пользователь может переключить курс в операции на любой из трёх.

## Решения по устройству (calls исполнителя помечены *)

1. **Чистая логика — `crates/core/src/fx.rs`** (правило + арифметика, без I/O),
   её используют и команда Tauri, и бот.
2. **Курсы БСБ — локальный кэш `bank_rates`, не синхронизируется.** Это
   публичные данные; каждое устройство качает само. Пишется напрямую, как
   таблица `settings` — осознанное исключение из правила `apply_change`
   (это кэш, не данные пользователя). Записать исключение в `AGENTS.md`.
3. **Счёт → банк:** новая колонка `accounts.rate_source` (`'bsb'|'nbrb'|NULL`).
   `NULL` = авто: `card`/`savings` → БСБ, `cash`/`other` → Нацбанк. Бэкфилла нет.
4. **Операция → источник:** `transactions.rate_source`
   (`'bsb_app'|'bsb_card'|'nbrb'|'manual'|NULL`). Ставится, когда сумма подставлена
   из подсказки; если пользователь потом изменил сумму — `'manual'`; ввод руками
   при режиме «справка» — `'manual'`; без конвертации — `NULL`.
   Фактический курс не храним — считается из двух сумм.
5. **Кэш:** исторический снимок (`nextTime` есть) валиден для
   `valid_from ≤ at < valid_to`; текущий (`nextTime == null`) — 5 минут* с `fetched_at`.
6. **Время операции:** `occurred_at` — локальное наивное время, переводим в
   мс через `chrono::Local`; будущее → сейчас.
7. **Миграция v7** поднимает `SCHEMA_VERSION` → `schema_gate` заставит обновить
   хост и клиенты вместе (отметить в CHANGELOG/«Что нового»).
8. Порог предупреждения об отклонении фактического курса — 1%*.
9. Ошибки сети БСБ — в `<appdata>/logs/rates.log` через `applog::append`* (без тела ответа).

## Global Constraints

- Деньги — `i64` в минимальных единицах; курсы — `i64`, BYN за 1 единицу ×1e6.
  Парсинг курсов БСБ — из строк `scaled*Amount` целочисленно, без `f64`.
  Округление — половина от нуля (как `convert_with` в `repos.rs`).
- Записи пользовательских данных — только через `apply_change()`
  (`repos::*_upsert`); `bank_rates` — единственное исключение (кэш).
- Все тексты UI и бота — на трёх языках: `src/i18n/locales/{ru,be,en}.ts`,
  `src-tauri/src/bot/text.rs`.
- Новых зависимостей не добавлять (reqwest, chrono, serde уже есть).
- Публичный репозиторий: в тестах/фикстурах/доках — никаких реальных имён,
  счетов, сумм пользователя; только курсы БСБ из публичного API.
- Перед cargo-командами в `PATH` должен быть Strawberry Perl
  (`export PATH="<путь к strawberry-perl>/perl/bin:$PATH"`; путь из AGENTS.md
  может не совпадать с машиной — проверить).
- Проверки: `npx tsc --noEmit`, `npm run lint`, `npm test`,
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`. Не пушить — только коммиты.

## Review Focus

1. **БСБ недоступен / сменил формат** → подсказка по Нацбанку с пометкой,
   сохранение не блокируется (тест в задаче 4, тест парсера на мусор — задача 3).
2. **Валюта без курса БСБ** (CNY) → Нацбанк + `fallback` (задача 4).
3. **Редактирование старой операции** (`rate_source = NULL`) → суммы не
   меняются сами; подсказка видна, но не применяется (задача 5, тест формы-логики).
4. **Старый клиент синхронизирует операцию без `rateSource`** → поле на хосте
   не затирается (задача 2).
5. **Операция в будущем или ночью/в выходные** (текущий курс с `nextTime = null`)
   → берётся текущий курс, кэш не живёт дольше 5 минут (задача 3).

---

### Task 1: Правило и арифметика курса — `finance_core::fx`

**Files:**
- Create: `crates/core/src/fx.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod fx;`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum RateKind { BsbApp, BsbCard, Nbrb }          // "bsb_app" | "bsb_card" | "nbrb"
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Mode { Fill, Reference }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Flow { Transfer, Expense, Income }
  /// BYN за 1 единицу валюты ×1e6. Для НБРБ buy == sell.
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Quote { pub buy: i64, pub sell: i64 }
  pub fn default_rule(flow: Flow, all_on_bsb: bool, entered_currency: Option<&str>) -> (RateKind, Mode);
  /// Деньги текут from→to; `known_is_from` — какая сторона известна.
  /// Котировка обязательна для каждой не-BYN валюты; None при нехватке/нулях.
  pub fn convert(amount_minor: i64, from: &str, to: &str, known_is_from: bool,
                 q_from: Option<Quote>, q_to: Option<Quote>) -> Option<i64>;
  /// Для отображения: BYN за 1 единицу иностранной валюты ×1e6 из двух сумм; None если обе BYN или обе иностранные.
  pub fn effective_rate(from: &str, from_minor: i64, to: &str, to_minor: i64) -> Option<i64>;
  ```

- [ ] **Step 1: Write the failing tests** (в `fx.rs`, `#[cfg(test)]`):

```rust
const fn q(buy: i64, sell: i64) -> Option<Quote> { Some(Quote { buy, sell }) }
#[test] fn transfer_usd_to_byn_uses_buy() {
    assert_eq!(convert(20723, "USD", "BYN", true, q(2_990_500, 3_010_000), None), Some(61972));
}
#[test] fn transfer_byn_to_usd_uses_sell() {
    assert_eq!(convert(3010, "BYN", "USD", true, None, q(2_990_500, 3_010_000)), Some(1000));
}
#[test] fn expense_in_byn_from_usd_card_inverse_buy() {
    // 50.00 BYN известны (to), списание USD = 50 / 2.9880
    assert_eq!(convert(5000, "USD", "BYN", false, q(2_988_000, 3_010_000), None), Some(1673));
}
#[test] fn income_byn_to_usd_card_uses_card_sell() {
    assert_eq!(convert(5000, "BYN", "USD", true, None, q(2_970_000, 3_050_000)), Some(1639));
}
#[test] fn rub_per_unit_quote() {
    // 3000 RUB × 0.034753 = 104.26 BYN
    assert_eq!(convert(300000, "RUB", "BYN", true, q(34_753, 36_440), None), Some(10426));
}
#[test] fn cross_via_byn() {
    // 10.00 EUR с USD-счёта (to известна): USD = EUR × sell(EUR)/buy(USD)
    assert_eq!(convert(1000, "USD", "EUR", false, q(2_988_000, 3_010_000), q(3_278_000, 3_309_000)), Some(1107));
}
#[test] fn missing_or_zero_quote_is_none() {
    assert_eq!(convert(100, "USD", "BYN", true, None, None), None);
    assert_eq!(convert(100, "USD", "BYN", true, q(0, 0), None), None);
}
#[test] fn rules() {
    assert_eq!(default_rule(Flow::Transfer, true, None), (RateKind::BsbApp, Mode::Fill));
    assert_eq!(default_rule(Flow::Expense, true, Some("BYN")), (RateKind::BsbApp, Mode::Fill));
    assert_eq!(default_rule(Flow::Income, true, Some("BYN")), (RateKind::BsbCard, Mode::Fill));
    assert_eq!(default_rule(Flow::Income, true, Some("RUB")), (RateKind::BsbApp, Mode::Reference));
    assert_eq!(default_rule(Flow::Income, false, Some("RUB")), (RateKind::Nbrb, Mode::Fill));
    assert_eq!(default_rule(Flow::Expense, false, Some("BYN")), (RateKind::Nbrb, Mode::Fill));
}
#[test] fn effective() {
    assert_eq!(effective_rate("USD", 1000, "BYN", 2921), Some(2_921_000));
    assert_eq!(effective_rate("BYN", 3010, "USD", 1000), Some(3_010_000));
    assert_eq!(effective_rate("USD", 1000, "EUR", 900), None);
}
```

- [ ] **Step 2:** `cargo test -p finance-core fx` → FAIL (модуля нет).
- [ ] **Step 3: Implement** в `fx.rs`. Арифметика на `i128`, округление половина
  от нуля. Прямой ход: `byn = from==BYN ? a : a×buy(from)/1e6`;
  `out = to==BYN ? byn : byn×1e6/sell(to)`. Обратный (известна `to`):
  `byn = to==BYN ? a : a×sell(to)/1e6`; `out = from==BYN ? byn : byn×1e6/buy(from)`.
  Округлять один раз в конце (считать дробью num/den). `from == to` → `Some(amount)`.
- [ ] **Step 4:** `cargo test -p finance-core fx` → PASS.
- [ ] **Step 5: Commit** `feat(core): fx rule and bank buy/sell conversion`.

### Task 2: Миграция v7 — поля источника курса и кэш `bank_rates`

**Files:**
- Modify: `crates/storage/src/migrations.rs` (новый элемент `MIGRATIONS` после v6)
- Modify: `crates/storage/src/repos.rs` (`TransactionInput`: `rate_source`; валидация)
- Modify: `src/lib/types.ts` (`rateSource?: RateSource | null` у операции и счёта)
- Test: `crates/storage/src/lib.rs` (рядом с тестами миграций ~строка 800–900)

**Interfaces:**
- Produces: колонки `accounts.rate_source`, `transactions.rate_source`; таблица `bank_rates`:
  ```sql
  -- v7
  ALTER TABLE accounts ADD COLUMN rate_source TEXT CHECK (rate_source IS NULL OR rate_source IN ('bsb','nbrb'));
  ALTER TABLE transactions ADD COLUMN rate_source TEXT CHECK (rate_source IS NULL OR rate_source IN ('bsb_app','bsb_card','nbrb','manual'));
  -- Local cache of bank quotes (not replicated, written directly like `settings`).
  CREATE TABLE bank_rates (
      kind TEXT NOT NULL CHECK (kind IN ('bsb_app','bsb_card')),
      currency TEXT NOT NULL,
      valid_from INTEGER NOT NULL,   -- unix ms
      valid_to INTEGER,              -- unix ms, NULL = current
      buy_scaled INTEGER NOT NULL,   -- BYN per 1 unit ×1e6
      sell_scaled INTEGER NOT NULL,
      fetched_at INTEGER NOT NULL,   -- unix ms
      PRIMARY KEY (kind, currency, valid_from)
  );
  ```
  `bank_rates` не добавлять в `Entity` — не синхронизируется.
- TS: `export type RateSource = "bsb_app" | "bsb_card" | "nbrb" | "manual";`
  `export type AccountRateSource = "bsb" | "nbrb";`

- [ ] **Step 1: Write the failing tests:**
  - `migration_v7_adds_rate_source_and_bank_rates`: открыть БД на v6
    (по образцу существующего теста миграций), `migrate`, проверить
    `stored_version == 7`, колонки есть, `bank_rates` пуста, у старых строк `rate_source IS NULL`.
  - `old_client_push_keeps_rate_source`: создать операцию с `"rateSource":"bsb_app"`,
    затем `write_row` той же операции без ключа `rateSource` (как старый клиент) →
    `rate_source` остаётся `'bsb_app'`. Явный `null` очищает.
  - `transaction_rejects_bad_rate_source`: `transaction_upsert` с `"rateSource":"foo"` → `Error::Invalid`.
- [ ] **Step 2:** `cargo test -p finance-storage migration_v7 old_client_push transaction_rejects` → FAIL.
- [ ] **Step 3: Implement:** миграция; в `TransactionInput` —
  `#[serde(default)] pub rate_source: Option<String>`, проверка значения из списка
  (ошибка `Invalid("bad rate source")`). Для строк без иностранной суммы и не-кросс-переводов
  `transaction_upsert` записывает `rateSource = null` (как сейчас `origCurrency`).
  Существующий тест `const { assert!(SCHEMA_VERSION >= 6) }` не трогать.
- [ ] **Step 4:** `cargo test -p finance-storage` → PASS.
- [ ] **Step 5: Commit** `feat(storage): v7 — rate source on accounts/transactions, bank_rates cache`.

### Task 3: Разбор ответа БСБ и кэш котировок

**Files:**
- Create: `crates/storage/src/bsb.rs` (по образцу `nbrb.rs`), `pub mod bsb;` в `lib.rs`
- Modify: `crates/storage/src/repos.rs` (раздел `// ---------- exchange rates ----------`)

**Interfaces:**
- Consumes: `finance_core::fx::{Quote, RateKind}`; таблица `bank_rates` (Task 2).
- Produces:
  ```rust
  // bsb.rs
  pub const URL: &str = "https://mobile.bsb.by/api/v1/free-zone-management/exchange-rates/rates";
  pub fn api_type(kind: RateKind) -> Option<&'static str>; // BsbApp→"CARD_ONLINE", BsbCard→"CARD", Nbrb→None
  pub fn request_body(kind: RateKind, at_ms: i64) -> serde_json::Value; // {"type":..,"bankDepartmentId":7,"period":at_ms}
  pub struct Snapshot { pub from_ms: i64, pub next_ms: Option<i64>, pub quotes: Vec<(String, Quote)> }
  pub fn parse(body: &str) -> Result<Snapshot, Error>;   // только пары *→BYN
  pub fn parse_scaled(s: &str, scale: i64) -> Option<i64>; // "3.4753",100 → 34753
  // repos.rs
  pub fn bank_rate_find(conn: &Connection, kind: RateKind, currency: &str, at_ms: i64, now_ms: i64) -> Result<Option<Quote>, Error>;
  pub fn bank_rates_store(conn: &Connection, kind: RateKind, snap: &Snapshot, now_ms: i64) -> Result<(), Error>;
  pub const BANK_RATE_TTL_MS: i64 = 5 * 60 * 1000;
  ```

- [ ] **Step 1: Write the failing tests:**
  - `parse_scaled_cases`: `("2.9905",1)→2_990_500`, `("3.4753",100)→34_753`,
    `("0.8080",10)→80_800`, `("3",1)→3_000_000`, `("abc",1)→None`, `("1.0",0)→None`.
  - `parse_real_response`: фикстура — укороченный реальный ответ (пары USD/BYN 2.982/3.0,
    RUB/BYN scaled 3.4753/3.6440 scale 100, USD/RUB — должна быть отброшена),
    `fromTime`, `nextTime:null` → `quotes` = USD{2_982_000,3_000_000}, RUB{34_753,36_440}; `next_ms None`.
  - `parse_garbage_is_error`: `"<html>"` и `{"rates":[]}` → `Err` (пустой список — ошибка).
  - `cache_current_snapshot_ttl`: store `next_ms=None` при `now`; find(at=now+1с, now+1с) → Some;
    find(now+6мин) → None.
  - `cache_historic_snapshot`: store `from=1000,next=5000`; find(at=4999) → Some; at=5000 → None; at=999 → None.
  - `cache_upsert_closes_previous`: store текущий снимок, потом исторический с тем же `valid_from` и `next_ms=Some` → одна строка, `valid_to` заполнен.
- [ ] **Step 2:** `cargo test -p finance-storage bsb cache_` → FAIL.
- [ ] **Step 3: Implement.** `parse_scaled` — целочисленный разбор десятичной строки
  (до 8 знаков дробной части) в ×1e6, деление на `scale` с округлением половина от нуля.
  `bank_rates_store` — `INSERT ... ON CONFLICT(kind,currency,valid_from) DO UPDATE SET valid_to, buy_scaled, sell_scaled, fetched_at`.
  `bank_rate_find` — строка с `valid_from <= at`, макс. `valid_from`; годна если
  `valid_to > at`, либо `valid_to IS NULL AND now - fetched_at < TTL`.
- [ ] **Step 4:** `cargo test -p finance-storage` → PASS.
- [ ] **Step 5: Commit** `feat(storage): BSB rates parser and local quote cache`.

### Task 4: Подсказка суммы на бэкенде — `rates::suggest` и команды

**Files:**
- Modify: `src-tauri/src/rates.rs`
- Modify: `src-tauri/src/commands.rs` (рядом с `convert_at` ~строка 718)
- Modify: `src-tauri/src/lib.rs` (регистрация команд рядом с `commands::convert_at`)
- Modify: `crates/storage/src/repos.rs` (`pub fn nbrb_quote_at(conn, currency, date) -> Result<Option<Quote>, Error>` поверх приватного `rate_row`, buy=sell=rate/scale)

**Interfaces:**
- Consumes: Task 1 (`default_rule`, `convert`), Task 3 (`bsb::*`, `bank_rate_*`), `rates::ensure` (НБРБ).
- Produces:
  ```rust
  #[derive(Deserialize)] #[serde(rename_all = "camelCase")]
  pub struct SuggestReq {
      pub kind: String,                 // "expense" | "income" | "transfer"
      pub account_id: String,
      pub to_account_id: Option<String>,
      pub entered_currency: Option<String>, // для expense/income
      pub amount_minor: i64,            // известная сумма (введённая / списания при переводе)
      pub occurred_at: String,          // "YYYY-MM-DDTHH:MM[:SS]" локальное; "" = сейчас
      pub rate_kind: Option<RateKind>,  // ручной выбор; None = по правилу
  }
  #[derive(Serialize)] #[serde(rename_all = "camelCase")]
  pub struct Suggestion {
      pub amount_minor: i64,            // в валюте счёта (expense/income) или счёта назначения (transfer)
      pub currency: String,
      pub rate_kind: RateKind,          // фактически применённый (Nbrb при fallback)
      pub default_kind: RateKind,       // что выбрало правило
      pub mode: Mode,
      pub fallback: bool,               // просили БСБ, посчитано по НБРБ
      pub rate_scaled: Option<i64>,     // BYN за 1 ед. иностранной валюты ×1e6 (сторона, что применена)
  }
  pub async fn suggest(state: &AppState, req: SuggestReq) -> Result<Option<Suggestion>, String>;
  pub async fn bsb_quote(state: &AppState, kind: RateKind, currency: &str, at_ms: i64) -> Option<Quote>;
  pub async fn overview(state: &AppState) -> Result<Vec<RateOverview>, String>;
  #[derive(Serialize)] #[serde(rename_all = "camelCase")]
  pub struct RateOverview { pub currency: String, pub bsb_app: Option<Pair>, pub bsb_card: Option<Pair>, pub nbrb: Option<i64> }
  #[derive(Serialize)] pub struct Pair { pub buy: i64, pub sell: i64 }
  // commands.rs
  #[tauri::command] pub async fn fx_suggest(state: State<'_, AppState>, req: SuggestReq) -> Result<Option<Suggestion>, String>;
  #[tauri::command] pub async fn rates_overview(state: State<'_, AppState>) -> Result<Vec<RateOverview>, String>;
  ```
- Логика `suggest`: валюты счетов из БД; `Flow` по `kind`; from/to и `known_is_from` —
  по таблице правила; `all_on_bsb` — у всех участвующих счетов эффективный
  `rate_source` = БСБ (`NULL` → по `kind` счёта: card/savings = БСБ). Если валюты
  совпадают или `amount_minor <= 0` → `Ok(None)`. Для БСБ-вида: котировки каждой
  не-BYN валюты через `bsb_quote`; если какой-то нет → НБРБ (`rates::ensure` + `nbrb_quote_at`), `fallback = true`.
  `at_ms` = `occurred_at` в `chrono::Local`, не позже `now`. `bsb_quote`: кэш →
  `POST bsb::URL` (`crate::net::client()`, timeout 10с, заголовок `Accept-Language: ru`) →
  `bank_rates_store` → снова `bank_rate_find`. Ошибки — `applog::append("rates.log", ...)` без тела ответа.
- `overview`: текущие котировки обоих видов для USD/EUR/RUB/PLN + последний НБРБ (`repos::rates_latest`).

- [ ] **Step 1: Write the failing tests** (в `rates.rs`, `#[cfg(test)]`, БД `open_memory`,
  кэш `bank_rates` заполнен заранее с `valid_to` в будущем — сеть не нужна):
  - `suggest_expense_byn_on_usd_card_uses_bsb_app_buy`: счёт USD kind=card, расход 50.00 BYN →
    `amount_minor 1673`, `rate_kind BsbApp`, `mode Fill`, `fallback false`.
  - `suggest_income_rub_is_reference`: счёт BYN card, доход 3000 RUB → `mode Reference`, `amount_minor 10426`.
  - `suggest_cash_account_uses_nbrb`: счёт BYN kind=cash, доход 100 USD при НБРБ 3.0 → `rate_kind Nbrb`.
  - `suggest_override_nbrb`: `rate_kind: Some(Nbrb)` на карте БСБ → `rate_kind Nbrb`, `default_kind BsbApp`.
  - `suggest_currency_without_bsb_quote_falls_back`: расход в CNY (в кэше нет, НБРБ CNY есть)
    → `rate_kind Nbrb`, `fallback true`. (Сетевой вызов при промахе кэша в тесте
    отключить: `bsb_quote` принимает `offline` через `cfg(test)`-флаг или фетчер-замыкание — на выбор исполнителя.)
  - `suggest_same_currency_is_none`.
- [ ] **Step 2:** `cargo test -p finance-app suggest_` → FAIL.
- [ ] **Step 3: Implement** по Interfaces; команды зарегистрировать в `lib.rs`.
  `convert_at` не удалять (его использует `useDisplayMoney`).
- [ ] **Step 4:** `cargo test --workspace` → PASS; `cargo clippy --workspace --all-targets -- -D warnings` → чисто.
- [ ] **Step 5: Commit** `feat(rates): BSB-aware conversion suggestions (fx_suggest, rates_overview)`.

### Task 5: Форма операции — подсказка, выбор курса, отклонение

**Files:**
- Create: `src/lib/fx.ts`, `src/lib/fx.test.ts`
- Create: `src/components/transactions/FxHint.tsx`
- Modify: `src/lib/api.ts` (рядом с `convertAt`), `src/lib/types.ts`
- Modify: `src/components/transactions/TxForm.tsx` (блоки `suggestion` ~стр. 78–85 и кнопки ~стр. 198 и ~240)
- Modify: `src/i18n/locales/{ru,be,en}.ts`

**Interfaces:**
- Consumes: команда `fx_suggest` (Task 4), `RateSource` (Task 2).
- Produces:
  ```ts
  // api.ts
  fxSuggest: (req: FxSuggestReq) => invoke<FxSuggestion | null>("fx_suggest", { req }),
  ratesOverview: () => invoke<RateOverview[]>("rates_overview"),
  // types.ts
  export type RateKind = "bsb_app" | "bsb_card" | "nbrb";
  export interface FxSuggestReq { kind: TxKind; accountId: string; toAccountId?: string | null;
    enteredCurrency?: string | null; amountMinor: number; occurredAt: string; rateKind?: RateKind | null }
  export interface FxSuggestion { amountMinor: number; currency: string; rateKind: RateKind;
    defaultKind: RateKind; mode: "fill" | "reference"; fallback: boolean; rateScaled: number | null }
  // fx.ts
  export function effectiveRate(from: string, fromMinor: number, to: string, toMinor: number): number | null; // как Rust effective_rate
  export function deviationPct(actualScaled: number, refScaled: number): number; // (a-r)/r*100, 1 знак
  export const DEVIATION_WARN_PCT = 1;
  export function nextRateSource(prev: RateSource | null, filled: number | null, typed: number | null, kind: RateKind): RateSource | null;
  // FxHint.tsx
  export function FxHint(props: { req: FxSuggestReq | null; current: number | null;
    knownCurrency: string; onFill: (amountMinor: number, kind: RateKind) => void;
    rateSource: RateSource | null }): JSX.Element | null;
  ```
- Вид (ru):
  ```
  ≈ 16,73 USD · по курсу [БСБ приложение | БСБ карточки | Нацбанк]   ← кнопка подставляет, сегменты переключают
  Для справки: ≈ 104,26 BYN · БСБ приложение                          ← режим reference, без кнопки
  Курс БСБ недоступен — по Нацбанку                                    ← fallback
  Фактический курс 2,9210 · −2,3% к «БСБ приложение»                   ← когда сумма введена; красным при |Δ| ≥ 1%
  ```
  Переключатель — `Segmented` (`src/components/ui/segmented.tsx`), компактный размер.
  Выбор курса — локальный стейт формы, по умолчанию `null` (= по правилу).
- TxForm: `rateSource` в `editing`; подстановка из подсказки ставит вид курса;
  ручное изменение суммы после подстановки → `'manual'` (`nextRateSource`); в submit
  передать `rateSource` (null, если нет `foreign`/`crossCurrency`). У старой операции
  (`rateSource == null`, суммы уже есть) ничего не подставлять автоматически.
  Запрос — `useQuery({ queryKey: ["fx", req], queryFn, enabled: !!req, staleTime: 60_000 })`.
- i18n-ключи (`ui.fx.*`): `bsbApp`, `bsbCard`, `nbrb`, `byRate` («по курсу»),
  `reference` («Для справки»), `fallback`, `actual` («Фактический курс»), `vs` («к»), `source` («Курс»).
  Старый `ui.byRate` удалить, если больше не используется.

- [ ] **Step 1: Write the failing tests** (`fx.test.ts`):
  ```ts
  expect(effectiveRate("USD", 1000, "BYN", 2921)).toBe(2_921_000);
  expect(effectiveRate("BYN", 3010, "USD", 1000)).toBe(3_010_000);
  expect(effectiveRate("USD", 1000, "EUR", 900)).toBeNull();
  expect(deviationPct(2_921_000, 2_990_500)).toBe(-2.3);
  expect(nextRateSource(null, 1673, 1673, "bsb_app")).toBe("bsb_app");
  expect(nextRateSource("bsb_app", 1673, 1680, "bsb_app")).toBe("manual");
  expect(nextRateSource(null, null, 1680, "bsb_app")).toBe("manual");
  expect(nextRateSource(null, null, null, "bsb_app")).toBeNull();
  ```
- [ ] **Step 2:** `npm test -- fx` → FAIL.
- [ ] **Step 3: Implement** `fx.ts`, `FxHint.tsx`, правки TxForm, api/types, i18n.
- [ ] **Step 4:** `npm test`, `npx tsc --noEmit`, `npm run lint` → PASS. Ручная проверка в `npm run tauri dev`:
  расход BYN с USD-карты, перевод USD→BYN, доход RUB (только справка), отключённая сеть (fallback).
- [ ] **Step 5: Commit** `feat(ui): BSB rate suggestion with source switch and deviation hint`.

### Task 6: Счёт — «курс для пересчёта»; настройки — обзор курсов

**Files:**
- Modify: `src/pages/AccountsPage.tsx` (форма счёта, рядом с полем `ui.bank` ~стр. 138)
- Modify: `src/pages/settings/RatesSection.tsx`
- Modify: `src/i18n/locales/{ru,be,en}.ts`

**Interfaces:**
- Consumes: `accounts.rate_source` (Task 2), `api.ratesOverview` (Task 5/4).
- Счёт: `Segmented` «Авто / БСБ / Нацбанк» → `rateSource: null | "bsb" | "nbrb"`,
  подсказка под ним: «Авто: карты и сбережения — по курсу БСБ, наличные — по Нацбанку».
- Настройки «Курсы»: таблица по USD, EUR, RUB (за 100), PLN (за 10):
  `валюта | БСБ приложение покупка/продажа | БСБ карточки покупка/продажа | Нацбанк`.
  Кнопка «Обновить» обновляет НБРБ (`ratesRefresh`) и перезапрашивает `ratesOverview`.
  Если БСБ недоступен — прочерки и строка `ui.fx.fallback`.
- [ ] **Step 1:** реализовать; `npx tsc --noEmit`, `npm run lint`, `npm test` → PASS.
- [ ] **Step 2:** ручная проверка: смена «курса для пересчёта» у наличных на «БСБ» меняет подсказку в форме.
- [ ] **Step 3: Commit** `feat(ui): per-account rate source and BSB rates overview`.

### Task 7: Бот — подсказки по курсу БСБ, справка для RUB, /rates

**Files:**
- Modify: `src-tauri/src/bot/wizard.rs` (`suggested` ~стр. 410, `ensure_conv_rates` ~425,
  клавиатура `ConvAmount` ~898, шаг `ToAmount`, обработчик `"conv:rate"` ~1606, payload ~1376, тесты ~2239–2300)
- Modify: `src-tauri/src/bot/text.rs` (ключ `by_rate` ~стр. 69 и новые)
- Modify: `src-tauri/src/bot/report.rs` (`rates_text` ~стр. 217)

**Interfaces:**
- Consumes: `rates::suggest`, `rates::bsb_quote` (Task 4).
- `Draft` += `#[serde(default)] fx: Option<FxCache>`, `rate_source: Option<String>`:
  ```rust
  #[derive(Clone, Default, Deserialize, Serialize)]
  struct FxCache { main: Option<(i64, String /*rate kind*/)>, nbrb: Option<i64>, reference: bool, fallback: bool }
  ```
  Заменить `ensure_conv_rates` на `async fn prepare_fx(state, &mut Draft)`: на шагах
  `ConvAmount` и `ToAmount` вызывает `rates::suggest` (по правилу) и, если вид не `Nbrb`,
  ещё раз с `rate_kind = Some(Nbrb)`; кладёт в `d.fx`. Время: `format!("{}T{}:00", d.date, d.time)`.
  Рендер (синхронный) читает только `d.fx` — `suggested()` удалить.
- Клавиатура `ConvAmount` / `ToAmount`:
  ```
  [ ≈ 16,73 USD · БСБ приложение ]   callback {rev}:conv:rate   (ToAmount: {rev}:credit:rate)
  [ По Нацбанку: 16,65 USD ]         callback {rev}:conv:nbrb   (ToAmount: {rev}:credit:nbrb)
  ```
  Режим справки (доход RUB): строка в тексте «Для справки: ≈ 104,26 BYN · БСБ приложение», кнопок нет.
  Fallback: одна кнопка «≈ … · Нацбанк» + строка «курс БСБ недоступен».
- Обработчики: `conv:rate|conv:nbrb` → `d.converted`, `d.rate_source = Some(kind)`;
  `credit:rate|credit:nbrb` → `d.credited`, `d.rate_source`; ввод числа текстом на этих шагах → `rate_source = Some("manual")`.
  Payload (~1376): `"rateSource": d.rate_source` (только если есть `converted` или `credited`, иначе `null`).
  Колбэки добавить в проверку допустимых действий (~1247).
- text.rs (ru/be/en): `rate_bsb_app` «БСБ приложение», `rate_bsb_card` «БСБ по карточкам»,
  `rate_nbrb` «Нацбанк», `by_nbrb` «По Нацбанку», `for_reference` «Для справки»,
  `bsb_unavailable` «курс БСБ недоступен, по Нацбанку», `rates_bsb` «БСБ (приложение / карточки)». `by_rate` удалить.
- `rates_text` (/rates): после строк НБРБ — блок БСБ: `🇺🇸 USD  2.9905 / 3.0100 · 2.9700 / 3.0500`
  (покупка/продажа приложение · карточки) из `rates::overview`; функция становится async — поправить вызов.

- [ ] **Step 1: Write the failing tests** (по образцу теста ~стр. 2239, кэш `bank_rates` заполнен заранее):
  - `conv_step_offers_bsb_and_nbrb`: расход 50 BYN на USD-карте → текст кнопок содержит
    «БСБ приложение» и «Нацбанк», ключи `conv:rate` и `conv:nbrb`; нажатие `conv:rate` →
    `d.converted == Some(1673)`, `d.rate_source == Some("bsb_app")`.
  - `rub_income_shows_reference_only`: доход 3000 RUB на BYN-карте → текст содержит «Для справки», ключа `conv:rate` нет.
  - `transfer_credited_suggestion`: перевод 207,23 USD → BYN, шаг `ToAmount` → кнопка с `credit:rate`, после нажатия `credited == Some(61972)`.
  - `typed_amount_marks_manual`: ввод суммы текстом на `ConvAmount` → `rate_source == Some("manual")`.
  - Обновить существующие ассерты (`contains("conv:rate")`, `converted == Some(1990)`) под новый кэш.
- [ ] **Step 2:** `cargo test -p finance-app bot::` → FAIL.
- [ ] **Step 3: Implement** по Interfaces.
- [ ] **Step 4:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` → PASS.
- [ ] **Step 5: Commit** `feat(bot): BSB rate buttons, RUB reference, BSB block in /rates`.

### Task 8: Документация и финальная проверка

**Files:**
- Modify: `CHANGELOG.md` (новая секция за 2026-11-22, по-русски)
- Modify: `src/lib/whatsnew.ts` (3–5 буллетов на ru/be/en; упомянуть: курс БСБ по
  умолчанию, выбор курса, справка для RUB, нужно обновить приложение на всех устройствах)
- Modify: `AGENTS.md`: раздел «Курсы» — источник БСБ (неофициальный API, URL, типы),
  кэш `bank_rates` — исключение из правила `apply_change`; поправить строку про
  Strawberry Perl на нейтральную формулировку без конкретного локального пути.

- [ ] **Step 1:** внести правки.
- [ ] **Step 2:** полный набор проверок из Global Constraints → всё зелёное.
- [ ] **Step 3:** `git grep` по паттернам личных данных из `AGENTS.md` → совпадений нет.
- [ ] **Step 4: Commit** `docs: BSB rates — changelog, what's new, agents notes`.

## Что оставлено на усмотрение исполнителя

- Как отключать сеть в тестах `rates.rs` (cfg-флаг или внедряемый фетчер).
- Точная вёрстка `FxHint` в рамках существующих компонентов и токенов темы.
- Формулировки i18n на be/en (смысл — как в ru выше).
