# Финансы

Десктоп-приложение для учёта семейного бюджета: карты и счета, категории, аналитика и отчёты, цели, напоминания, Telegram-бот, синхронизация нескольких компьютеров в локальной сети.

Платформы: Windows x64, macOS (Apple Silicon и Intel). Стек: Tauri 2 (Rust) + React + TypeScript.

## Установка

Установщики публикуются в публичном репозитории
[kostyanpilkin-hash/finance_app_public → Releases](https://github.com/kostyanpilkin-hash/finance_app_public/releases).

Приложение не подписано сертификатами Microsoft/Apple, поэтому при первой установке система предупредит:

- **Windows** (`*-setup.exe` или `*.msi`): «Windows защитила ваш компьютер» → **Подробнее** → **Выполнить в любом случае**.
- **macOS** (`*.dmg`): перетащите «Финансы» в «Программы». При первом запуске, если macOS пишет «приложение повреждено» или «неустановленный разработчик»:
  - Системные настройки → Конфиденциальность и безопасность → **Всё равно открыть**, или
  - в Терминале: `xattr -cr /Applications/Финансы.app`

## Разработка

Требования: Node.js 24+, Rust stable, Python 3 + Pillow (только для генерации иконки).

```bash
npm install
npm run tauri dev          # запуск в режиме разработки
npm run build:installer    # MSI + NSIS в installers/<дата>/
```

Проверки:

```bash
npx tsc --noEmit
npm run lint
npm test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Структура:

```
crates/core      доменная логика: деньги, валюты, балансы, периоды
crates/storage   SQLite/SQLCipher, миграции, журнал, синхронизация
crates/api       HTTP-API /api/v1 (LAN-хост, позже сервер)
crates/bot       Telegram-бот
src-tauri        десктоп-оболочка (трей, автозапуск, команды)
src              интерфейс на React
```

## Релизы (GitHub Actions)

- `ci.yml` — проверки на каждый push в `main` (Windows).
- `release.yml` — по тегу `v*` собирает Windows (MSI, NSIS) и macOS (universal DMG) и создаёт **черновик** релиза в `finance_app_public`.

Однократная настройка секретов в `finance_app` → Settings → Secrets and variables → Actions:

1. **`PUBLIC_RELEASES_TOKEN`** — GitHub → Settings → Developer settings → Fine-grained tokens → Generate:
   - Repository access: *Only select repositories* → `finance_app_public`;
   - Permissions → Repository → **Contents: Read and write**.
2. **`TAURI_SIGNING_PRIVATE_KEY`** и **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`** — понадобятся с включением автообновлений (этап 7).

Выпуск версии:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Через ~20 минут черновик релиза появится в `finance_app_public` → Releases; проверьте и нажмите **Publish**.
