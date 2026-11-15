# StepMaster

Личная программа-набор инструментов для Windows 10/11. Один файл `StepMaster.exe`, тёмный монохромный интерфейс, всё на русском (английский — в настройках). Запускается от администратора (UAC).

## Решения Степы (не менять без спроса)
- Файл всегда называется `StepMaster.exe`, без версии в имени. Один файл, без установщика.
- Только Windows 10 и 11. Язык по умолчанию русский, английский — в настройках.
- Дизайн: тёмный монохром как в `assets/reference.webp`, логотип — `assets/logo-source.webp`.
- «Жидкое стекло» везде, где есть переключатель: боковое меню (`src/components/Sidebar.tsx`) и все вкладки (`Tabs` в `src/components/ui.tsx`). Зажать и вести — линза едет за курсором, отпустить — выбор. Новые переключатели делать только через `Tabs`, тогда эффект будет и у них.
- Везде, где показаны программы, — их настоящие иконки, а не буквы (`src/components/AppIcon.tsx`, `src-tauri/src/icons.rs`).
- Окно обновления пишет только «Нужно обновить программу» (без номера версии), кнопки «Обновить» и «Позже». «Проверить обновления» в настройках всегда показывает результат строкой.
- Антивирус не выключаем сами: кнопка ведёт в «Безопасность Windows».
- Новые функции добавляются в свой раздел (модуль), программа будет расти версиями.

## Стек
- Интерфейс: React 19 + TypeScript + Vite, анимации `motion`, иконки `lucide-react`, тосты `sonner`, состояние `zustand`.
- Оболочка и система: Tauri 2 + Rust (`src-tauri/`). PowerShell 5.1 для WMI/Defender/сети, реестр через `winreg`, WinAPI через `windows-sys`.
- Дизайн-токены: `src/styles/tokens.css` (цвета, радиусы, отступы, кривые анимаций). Компоненты: `src/styles/global.css`, `src/components/ui.tsx`.

## Как устроено
- Каждый раздел бокового меню — модуль: UI в `src/modules/<id>/`, бэкенд в `src-tauri/src/<module>.rs`.
- Реестр разделов: `src/modules/registry.tsx`. Команды бэкенда регистрируются в `src-tauri/src/lib.rs` (`generate_handler!`).
- Типы обмена UI ↔ Rust: `src/lib/types.ts` (serde `camelCase` на стороне Rust). Вызовы: `src/lib/api.ts`.
- В обычном браузере (`npm run dev`) UI работает на тестовых данных `src/lib/mock.ts`. Режимы: `localStorage.setItem('sm-mock', 'normal'|'worst'|'empty'|'error')`.
- Тексты: `src/i18n/ru.ts` (источник), `src/i18n/en.ts` (TypeScript не даст забыть перевод).

## Как добавить функцию
1. Новая кнопка/инструмент в существующем разделе: команда в нужном `src-tauri/src/<module>.rs` → регистрация в `lib.rs` → метод в `api.ts` + тип в `types.ts` + мок в `mock.ts` → UI в `src/modules/<id>/` → строки в `ru.ts` и `en.ts`.
2. Новый раздел: папка `src/modules/<id>/`, id в `ModuleId` (`src/lib/store.ts`), `nav.<id>` в i18n, строка в `registry.tsx`, при желании пункт в палитре `src/components/Palette.tsx`.

## Правила безопасности (не нарушать)
- Удаление пользовательских файлов — только в Корзину (`disk::recycle_paths`), с подтверждением и списком путей.
- Очистка мусора — только фиксированные папки кэша (`cleanup.rs`), обход без перехода по junction/symlink (`fsops.rs`).
- Любые строки в PowerShell — через `util::psq()`. Пути проверять `util::is_protected_path()`. Системные утилиты — только через `util::sys_tool()`.
- Изменения системы должны быть обратимыми (игровой режим хранит прежние значения в `game-mode.json`).

## Команды
- `npm run dev` — UI в браузере на моках (порт 1420).
- `npx tauri dev` — настоящее окно (без прав админа, покажет баннер).
- `npm test` — тесты UI; `cargo test --lib` в `src-tauri` — тесты Rust; `cargo test --lib smoke -- --ignored --nocapture` — проверка на реальной системе (только чтение).
- `npm run release` — сборка + подпись → `StepMaster.exe` в корне проекта.
- Выпуск обновления: `powershell -File scripts/release.ps1 -Version 1.0.1 -Publish` (репозиторий `c1naii/StepMaster` уже прописан в `REPO`).

## Обновления
- Программа проверяет GitHub Releases (`updater.rs`, константа `REPO`). В релизе должны быть `StepMaster.exe` и `StepMaster.exe.sig`.
- Ключ подписи: `%USERPROFILE%\.stepmaster-keys\updater.key` (не коммитить, не терять — без него обновления не пройдут проверку).
- Не редактировать файлы через PowerShell `Get-Content`/`Set-Content` без `-Encoding utf8` — ломается кириллица.
