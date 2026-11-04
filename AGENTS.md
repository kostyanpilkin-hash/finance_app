# Финансы — правила проекта

## Рабочая директория и репозитории

- Проект: `finance_app`. Код: `kostyanpilkin-hash/finance_app` (приватный), ветка `main`, пуш `git push origin main`.
- Релизы: `kostyanpilkin-hash/finance_app_public` (публичный, только установщики и `latest.json`). Публикует `release.yml` по тегу `v*` через секрет `PUBLIC_RELEASES_TOKEN`.
- План разработки (этапы 0–9, архитектура, схема БД, протокол синхронизации): `C:\Users\user\.devin\plans\plan-3209bf19d66ea57d.md`.
- Соседний проект-донор наработок: `tutor-app` (темы, i18n, трей, Telegram-клиент, axum-сервер).

## CHANGELOG

- После каждой завершённой задачи пополнять `CHANGELOG.md` — по-русски, по датам и темам; коммитить вместе с кодом.

## Структура

- Cargo workspace в корне: `crates/core` (чистая логика), `crates/storage` (SQLCipher, миграции, журнал, sync), `crates/api` (axum `/api/v1`), `crates/bot` (Telegram), `src-tauri` (оболочка). Сборка Rust идёт в корневой `target/`.
- Фронт: `src/app` (layout, страницы), `src/pages`, `src/components/ui`, `src/lib`, `src/stores`, `src/i18n/locales/{ru,be,en}.ts` (тип `Locale` из `ru.ts` — be/en обязаны иметь те же ключи).
- Темы: классы `.theme-*` в `src/index.css`, список в `src/lib/themes.ts` (по умолчанию `emerald`). Новая тема = CSS + запись в `THEMES` + перевод в трёх локалях (тест `themes.test.ts` это проверяет).
- Иконка: `npm run icon` (`scripts/generate-icon.py` → `tauri icon`), затем удалить `src-tauri/icons/{android,ios}` до этапа мобильных.

## Сборка и проверки

- Проверки: `npx tsc --noEmit`, `npm run lint`, `npm test`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Установщик: `npm run build:installer` → MSI + NSIS в `installers/<дата>/`.
- macOS собирается только в CI (`release.yml`, universal-apple-darwin).

## Правила кода

- Деньги — `i64` в минимальных единицах + ISO-код валюты (`finance_core::Money`), никаких float.
- Все записи в БД — через `storage::apply_change()` внутри транзакции (журнал + версия + seq); прямые UPDATE в обход журнала запрещены.
- Секреты (ключ БД, токен бота, токены устройств) — только в `keyring`, не в БД/логах.
- Все тексты UI и бота — через i18n на трёх языках.
- Новые зависимости — стабильные версии, опубликованные ≥ 7 дней назад; без `latest`/`*`.

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
- **SQLCipher/OpenSSL:** сборка `rusqlite` с `bundled-sqlcipher-vendored-openssl` требует полноценный Perl — git-bash perl не подходит. Установлен портативный Strawberry Perl: `tools\strawberry-perl`. Перед cargo-командами: `export PATH="tools/strawberry-perl/perl/bin:$PATH"`. На GitHub runners Perl уже есть.
