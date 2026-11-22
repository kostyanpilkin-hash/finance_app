# Финансы — правила проекта

## Рабочая директория и репозитории

- Проект: корень этого репозитория. Код: `kostyanpilkin-hash/finance_app` (публичный), ветка `main`, пуш `git push origin main`.
- Релизы: `kostyanpilkin-hash/finance_app_public` (публичный, только установщики и `latest.json`). Публикует `release.yml` по тегу `v*` через секрет `PUBLIC_RELEASES_TOKEN`.
- План разработки (этапы 0–9, архитектура, схема БД, протокол синхронизации): `%USERPROFILE%\.devin\plans\plan-3209bf19d66ea57d.md`.
- Соседний проект-донор наработок: `..\tutor-app` (темы, i18n, трей, Telegram-клиент, axum-сервер).

## CHANGELOG

- После каждой завершённой задачи пополнять `CHANGELOG.md` — по-русски, по датам и темам; коммитить вместе с кодом.

## Структура

- Cargo workspace в корне: `crates/core` (чистая логика), `crates/storage` (SQLCipher, миграции, журнал, sync), `crates/api` (axum `/api/v1`), `crates/bot` (Telegram), `src-tauri` (оболочка). Сборка Rust идёт в корневой `target/`.
- Фронт: `src/app` (layout, страницы), `src/pages`, `src/components/ui`, `src/lib`, `src/stores`, `src/i18n/locales/{ru,be,en}.ts` (тип `Locale` из `ru.ts` — be/en обязаны иметь те же ключи).
- Темы: классы `.theme-*` в `src/index.css`, список в `src/lib/themes.ts` (по умолчанию `emerald`). Новая тема = CSS + запись в `THEMES` + перевод в трёх локалях (тест `themes.test.ts` это проверяет).
- Иконка: `npm run icon` (`scripts/generate-icon.py` → `tauri icon`), затем удалить `src-tauri/icons/{android,ios}` до этапа мобильных.
- Переиспользуемый оверлей — `Dialog` в `src/components/ui/dialog.tsx` (Esc, клик по фону, `aria-modal`); новые всплывающие окна строить на нём, а не на своих `fixed` обёртках.
- Тексты окна «Что нового» при каждом релизе заполнять в `src/lib/whatsnew.ts` (массив `WHATS_NEW`, 3–7 буллетов на трёх языках) вместе с `CHANGELOG.md`.

## Сборка и проверки

- Проверки: `npx tsc --noEmit`, `npm run lint`, `npm test`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Установщик: `npm run build:installer` → MSI + NSIS в `installers/<дата>/`.
- macOS собирается только в CI (`release.yml`, universal-apple-darwin).

## Граф кода (graphify)

- `graphify-out/graph.json` — knowledge graph кодовой базы (tree-sitter AST, детерминированно, без LLM). Автообновляется git-хуками на commit/checkout/merge/rewrite; ручное обновление `graphify update .`, полный пересчёт `graphify extract . --code-only --cargo`. Разово отключить хук: `GRAPHIFY_SKIP_HOOK=1`.
- При разведке кодовой базы использовать ПЕРЕД глобальным grep: `graphify explain "X"` (соседи узла с file:line), `graphify path "A" "B"`, `graphify affected "X"` (что затронет изменение), `graphify god-nodes`, `graphify query "..."` (BFS-контекст), `graphify section "X"` (секции markdown).
- Ограничение: мост `invoke() ↔ #[tauri::command]` в графе не моделируется — фронт и бэк не связаны; межъязыковые связи искать grep'ом по имени команды.
- Сообщества без осмысленных имён («Community N») — нет LLM-бэкенда. `graphify-out/` gitignored; отчёт `graphify-out/GRAPH_REPORT.md`, визуализация `graphify-out/graph.html`.

## Правила кода

- Деньги — `i64` в минимальных единицах + ISO-код валюты (`finance_core::Money`), никаких float.
- Все записи в БД — через `storage::apply_change()` внутри транзакции (журнал + версия + seq); прямые UPDATE в обход журнала запрещены.
- Секреты (ключ БД, токен бота, токены устройств) — только в `keyring`, не в БД/логах.
- Все тексты UI и бота — через i18n на трёх языках.
- Новые зависимости — стабильные версии, опубликованные ≥ 7 дней назад; без `latest`/`*`.
- **Репозиторий публичный — ноль личных данных.** В файлы, сообщения коммитов и историю не должны попадать: настоящие имена, почты, телефоны, адреса, локальные пути (`C:\Users\<имя>`, `D:\...`), токены, ключи, IP/имена устройств. В тестах, примерах и доках — нейтральные имена (Иван, Мария) и generic-пути. Коммитим под GitHub-noreply identity (настроена локально в `.git/config` — не менять на настоящую почту). Перед пушем проверять `git grep` по именам членов семьи, почте и паттернам локальных путей (`Users\`, `<диск>:\Work`) — совпадений быть не должно (кроме этого пункта).

## Курсы валют

- НБРБ — `api.nbrb.by` (`src-tauri/src/rates.rs`, таблица `exchange_rates`, синхронизируется): итоги, капитал, отчёты.
- БСБ Банк — неофициальный эндпоинт сайта `POST https://mobile.bsb.by/api/v1/free-zone-management/exchange-rates/rates` `{"type":"CARD_ONLINE"|"CARD","bankDepartmentId":7,"period":<ms>}` (без ключа и SLA). `CARD_ONLINE` = курс приложения, `CARD` = по карточкам. Разбор — `crates/storage/src/bsb.rs` (из строк `scaled*Amount`, без float).
- Котировки БСБ — локальный кэш `bank_rates` (не синхронизируется, пишется напрямую как `settings` — единственное исключение из правила `apply_change`). Текущий снимок живёт 5 минут.
- Правило выбора курса и арифметика покупки/продажи — `finance_core::fx`; подсказка для формы и бота — `src-tauri/src/fx.rs` (`suggest`, команда `fx_suggest`). Ошибки БСБ → откат на НБРБ, лог `logs/rates.log`.
- `accounts.rate_source` (`bsb|nbrb|NULL` = по типу счёта), `transactions.rate_source` (`bsb_app|bsb_card|nbrb|manual`).

## Telegram-бот

- Модули `src-tauri/src/bot/`: `wizard.rs` (карточки и шаги, ревизия кнопок), `text.rs` (тексты ru/be/en, форматирование), `report.rs` (балансы, сводки), `chart.rs` (PNG-диаграмма), `notify.rs` (расписание, напоминания).
- Callback-кнопки: колбэк редактирует активную карточку на месте; текст и команды гасят старую карточку и присылают новую внизу.
- Ошибки бота: `<appdata>/logs/bot.log` (ротация 1 МБ). Ошибки reqwest — только через `without_url()`, чтобы токен не попал в лог.
- Настройки уведомлений — локальная таблица `settings`: `bot_{daily,weekly,monthly,reminders}_{enabled,time}`, `bot_weekly_day`, `bot_monthly_day`, `bot_quiet_{enabled,from,to}`, `display_currency`.
- Каталог иконок категорий — `src/lib/category-icons.json` (id lucide + эмодзи для бота), общий для фронта и Rust (`include_str!`).

## Подводные камни

- `nul` — зарезервированное имя Windows: `> nul` в git-bash создаёт файл, ломающий git. Использовать `2>/dev/null`.
- MSYS path conversion: аргументы вида `/S` превращаются в пути — запускать `MSYS_NO_PATHCONV=1 ...`.
- Длинные логи (сборка, npm) — в файл или `| tail -N`.
- Shell — git-bash: команды в bash-форме.
- **Синхронизация:** push идемпотентен — payload, идентичный по смыслу хранимой строке (`db::same_content`, без метаполей), отвечает `ok` без конфликта; `exchange_rates` никогда не конфликтуют (last-writer-wins). Хост — источник истины по открытым конфликтам: клиент зеркалит их в `mirror_conflicts`, resolve закрывает и дубликаты. `db::prune_noop_conflicts` при старте закрывает конфликты без реальных изменений на каждом устройстве. Одновременный фоновый и ручной синк исключён `tokio::sync::Mutex` в `sync::sync_lock`. `last_sync_at` пишется после каждого удачного обмена (и пустого). Высота каркаса — `.app-frame`/`.app-frame-min` с `--app-height` из `window.innerHeight` (обход устаревшего `100vh` в WKWebView на macOS).
- **SQLCipher/OpenSSL:** сборка `rusqlite` с `bundled-sqlcipher-vendored-openssl` требует полноценный Perl — git-bash perl не подходит. Нужен портативный Strawberry Perl (без установки, достаточно распаковки). Перед cargo-командами добавить `<strawberry-perl>/perl/bin` в начало `PATH` (`export PATH="<путь>/perl/bin:$PATH"`; путь зависит от машины). На GitHub runners Perl уже есть.
