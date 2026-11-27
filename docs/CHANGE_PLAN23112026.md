# План 23.11.2026 — запрет будущих операций + внутренние элементы дашборда

Две независимые фичи в одном плане. Решения пользователя (раунд гриллинга):

1. **Операции на будущее запрещены по календарному дню** (`occurred_at` не позже сегодняшней локальной даты; время внутри сегодня — любое).
2. Запрет действует **везде**: форма десктопа, storage (`transaction_upsert`), бот.
3. **Строго**: уже существующая операция с будущей датой не сохранится, пока дату не исправят.
4. Внутренние элементы дашборда **разравниваются в самостоятельные виджеты** на общей сетке.
5. Охват: и три плашки сводки, и **каждый счёт** — отдельный виджет. Сетка 4 → **12 колонок**.

## Контекст и факты (уже проверено)

- `TxForm.tsx:184-191` — `type="date"` без `max`, дата не валидируется; `missing` покрывает только amount/account/category/to*.
- `repos::transaction_upsert` (`crates/storage/src/repos.rs:372`) — единая точка записи: команда `transaction_upsert`, `reminder_resolve` (`commands.rs:344`), бот (`wizard.rs:1478`), автонапоминание (`notify.rs:310`, всегда `now()` — безопасно).
- **Sync не задевается**: push применяет чужие операции через `repos::apply` напрямую (`crates/api/src/lib.rs:554`), минуя `transaction_upsert`. Операции с будущей датой со старых версий/других устройств доезжают как раньше.
- Бот: шаг `Date` рисует кнопки `today/yesterday` + `tomorrow/other` (`wizard.rs:1042-1050`); `accepts` разрешает `date:tomorrow` (`wizard.rs:1335-1338`); ручной ввод — `Step::CustomDate` → `date_input` (`wizard.rs:1263-1270`), любая валидная дата. Фильтр истории `hdate:*` — отдельный namespace (`wizard.rs:1375-1378`), там «завтра» остаётся (безобидно).
- `relative_date` (`bot/text.rs:436`) разделяется по префиксу — `date:` и `hdate:` независимы.
- Фронт дашборда: `WidgetId` — закрытое перечисление, `WIDGET_DEFS` (minW/minH/defW/defH), `GRID_COLS=4`, `ROW_HEIGHT=24`, `GRID_MARGIN=16`; `parseLayout` читает v2 `{v:2,items:[{i,x,y,w,h}]}` и v1 `[{id,span}]`; auto-fit высот — только для `kind:"flow"` (несохранённой) раскладки через измерительный проход в `DashGrid`.
- `stats` = три отдельные `card`-плашки в `auto-fill` grid; `accounts` = grid `AccountCard`. Визуально уже отдельные карточки — разравнивание не меняет вида.

## Задача 1 — запрет будущих дат (storage)

Файл: `crates/storage/src/repos.rs`, тесты `crates/storage/src/lib.rs`.

- В `transaction_upsert` после проверки kind: `t.occurred_at.get(..10)` > `dbm::now_iso().get(..10)` → `Err(Error::Invalid("future date"))`. Сравнение строк ISO-дат — лексикографическое.
- Проверка безусловная (и create, и update) — строгое правило из решения №3.
- Тест: upsert с `occurredAt = tomorrow` → `Err(Invalid)`; с сегодня/вчера → `Ok`; update существующей записи с будущей датой → `Err`.

## Задача 2 — форма десктопа

Файл: `src/components/transactions/TxForm.tsx`, локали.

- `<Input type="date" max={todayIso()}>` — блокирует выбор в календаре.
- `futureDate = opDate > todayIso()`: при `tried` помечает поле `invalid`, блокирует `submit`, показывает `ui.err.futureDate` (добавить в `firstMissing`-цепочку отдельным случаем).
- `backendErr`: ветка `/future/i` → `ui.err.futureDate` (придёт из storage, если UI обойдён).
- Локали ru/be/en: `ui.err.futureDate` = «Дата не может быть в будущем» / «Дата не можа быць у будучыні» / «Date cannot be in the future».

## Задача 3 — бот

Файлы: `src-tauri/src/bot/wizard.rs`, `src-tauri/src/bot/text.rs`.

- Шаг `Date`: убрать кнопку `tomorrow`; ряд кнопок — `[today, yesterday]`, `[other]`.
- `accepts` для `Step::Date`: удалить `date:tomorrow` (отсекает и скрафченные колбэки со старых карточек).
- `Step::CustomDate`: после `date_input` — если дата > `report::today()` → `d.notice = tr(l, "date_future")`, остаёмся на шаге (return true).
- `text.rs`: новый ключ `date_future` на трёх языках (ru «Нельзя выбрать дату в будущем», be, en). `hdate:*` не трогаем — фильтр истории.
- Тесты wizard: `accepts(date:tomorrow)` в Date → false; custom «завтра» → остаётся на шаге с notice.

## Задача 4 — модель раскладки 12 колонок + динамические id

Файл: `src/lib/dashLayout.ts`, тесты `dashLayout.test.ts`.

- `GRID_COLS = 12`. `WidgetId` → `string` (открытый тип); статические id: `stat_balance|stat_expense|stat_income|rates|chart_*` + динамические `account_<id>`.
- `WIDGET_DEFS` → `widgetDef(id)`:
  | id | minW | minH | defW | defH |
  |---|---|---|---|---|
  | stat_* | 3 | 3 | 4 | 3 |
  | rates | 8 | 4 | 12 | 6 |
  | chart_* | 6 | 5 | 6 | 9 |
  | account_* | 3 | 3 | 4 | 4 |
- `DEFAULT_ORDER(accounts)`: `stat_balance, stat_expense, stat_income, rates, chart_*, account_*` в порядке списка счетов.
- `parseLayout`: принимает `visible` (уже включает `account_<id>` и `stat_*`).
  - **Миграция v2/v1**: координаты ×3 (x·3, w·3); элемент `stats` → три `stat_*` на его y: x = x·3, x·3+4, x·3+8 (последний клампится), w=4, h — старая h (auto-fit доберёт для flow); элемент `accounts` → по `account_<id>` из `visible`, укладка слева направо по w=4 с переносом строки внутри области старого виджета.
  - Неизвестные/невидимые id — отбрасываются (старое поведение сохраняется).
- `serializeGrid`, `readingOrder`, `rowsFor`, `flowToGrid` — без изменений (работают по GRID_COLS).
- Тесты: миграция `stats`→3 плашки и `accounts`→N счетов, ×3-конверсия координат, отбрасывание удалённых счетов, добавление новых, дефолт.

## Задача 5 — DashboardPage + DashGrid

- `visible` строится как `[stat_*, chart_*(on), rates(on), account_<id>...]`.
- `renderWidget`: `stat_*` → одиночная `card`-плашка (вынести три тела из stats-grid); `account_<id>` → `<AccountCard compact>`; остальные без изменений.
- `widgetTitle`: `stat_*` → «Общий баланс»/«Расходы за период»/«Доходы за период» (существующие ключи `ui.totalBalance/spentMonth/incomeMonth`); `account_<id>` → имя + валюта счёта.
- **Мобильный (<lg) путь без изменений по виду**: три stat-плашки в том же `auto-fill` grid, счета в `auto-fill` grid — группировка только визуальная.
- `DashGrid` почти не трогаем: `WIDGET_DEFS[id]` → `widgetDef(id)`; кнопки и ручки наследуются сами. Высота кнопочной панели помещается в плашку h=3 (72px content + ручка в углу — проверить визуально).
- Заголовок-обёртка: статические id по-прежнему резолвятся через `widgetTitle`.

## Задача 6 — проверки, CHANGELOG, установщик

- `npx tsc --noEmit`, `npm run lint`, `npm test`, `npm run build`; `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Impeccable detector по изменённым UI-файлам → `[]`.
- Playwright-стенд: миграция раскладки со скриншота пользователя (stats→3 плашки, счета по одному), drag/resize новых виджетов, кнопки, сброс.
- `CHANGELOG.md` (рус., по дате); запуск `npm run build:installer`, ручная установка → тест пользователем → только потом push.
- Проверить вживую: операция на завтра блокируется в форме (max + ошибка), бот отвечает отказом на «завтра»/ручную будущую дату, reminder по-прежнему создаёт операцию.

## Review focus

- `occurred_at` формат везде `YYYY-MM-DDT...` — срез `..10` безопасен; часовой пояс — локальный (`now_iso` уже локальный).
- Strict-правило и бот: старые карточки-визарды с кнопкой «Завтра» (до обновления) — колбэк `date:tomorrow` теперь отклоняется `accepts` до обработчика.
- Миграция: у пользователя сейчас сохранена кастомная 4-кол раскладка (rates сверху) — после апдейта она должна встать ×3 без перекрытий и сюрпризов.
- Плашки stat в h=3: не должно быть внутреннего скролла при стандартном контенте; auto-fit для flow-раскладки обязан покрыть и их.
- `account_<id>` в сохранённой раскладке для удалённого/заархивированного счёта — отбрасывается `visible`.
- Не задеть `hdate:tomorrow` (фильтр истории) и `GoalsPage` (у целей дата законно в будущем).
