# Fabric Switchboard — PassionCode.ai

[English](README.md) · **Русский**

Fabric Switchboard — локальный менеджер аккаунтов Claude Code и Codex CLI с desktop-интерфейсом и командой `switchboard`: инструмент Fabric, который работает и сам по себе. Постоянные секреты защищены macOS Keychain или Windows DPAPI; рабочие и личные аккаунты разделены пулами. В управляемой сессии выбранный аккаунт меняется **со следующего запроса**: текущий поток ответа продолжает использовать прежний.

**Статус: beta.** Опубликован [v0.5.3-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.3-beta.1) (prerelease): macOS universal подписан Developer ID, **нотаризован Apple** и со stapled-тикетом; Windows x64 не подписан и собран кросс-компиляцией. Со следующего релиза сборки собирает и подписывает только [release workflow](docs/DISTRIBUTION.md#how-a-release-happens) в GitHub Actions. 0.4.1 убирает повторяющиеся запросы связки ключей macOS: сохранённые аккаунты один раз переносятся приложением в `ai.passioncode.fabric-switchboard.shared` со списком доступа, которому доверяют приложение и встроенный CLI, а CLI и MCP-сервер никогда не показывают диалог Keychain; при переносе macOS может спросить не больше одного раза на аккаунт ([KEYCHAIN.md](docs/KEYCHAIN.md), [протокол релиза 0.4.1](docs/evidence/release-0.4.1.md)). Это первый релиз под AGPL. В 0.4 агенты через MCP (`switchboard mcp`, 8 инструментов) видят остаток лимитов и переключают аккаунт своей управляемой сессии; обычный логин Claude Code меняется только с явным `global: true`. Добавлены необязательные правила проектов, экраны Projects и Agents и плагин `switchboard` для Claude Code. Сохранены захват текущей авторизации CLI, импорт Claude Swap, окна лимитов и автопереключение. [Протокол релиза 0.4.0](docs/evidence/release-0.4.md), [план 0.4](docs/PLAN-0.4.md), [контракт 0.3](docs/ACCOUNTS-AND-ROTATION.md). Вход и запросы через реальные аккаунты провайдеров ещё не проверены. Подключить агента: положите `switchboard` из архива в `PATH` и выполните `claude mcp add --scope user switchboard -- switchboard mcp`.

- [Сайт](https://passioncode.ai/switchboard/) · скачать для [macOS](https://passioncode.ai/switchboard/download/macos) / [Windows](https://passioncode.ai/switchboard/download/windows) · [релизы](https://github.com/passioncode-ai/fabric-switchboard/releases) · [установка готовых архивов](docs/INSTALL.md).
- [Исследование четырёх решений](docs/research/README.md): исходники, архитектура, хранение и механика переключения; 69 ссылок на фиксированные коммиты.
- [Спецификация](docs/SPEC.md): функции, оси совместимости, состояния, безопасность, протоколы, Windows и критерии приёмки.
- [Карта решения на русском](docs/PRODUCT.ru.md): что построено, что заимствовано как идея, ограничения и очередность развития.
- [Точка входа для следующего агента](docs/HANDOFF.md): проверки, решения и точная следующая задача.
- [CLI](docs/CLI.md): команды, JSON, stdin и общий владелец сессий.
- [Дистрибутивы](docs/DISTRIBUTION.md): macOS universal app/CLI, подпись и Windows installer/CLI.

## Запуск из исходников

Для macOS нужны macOS 14+, Xcode Command Line Tools, Rust и Node.js. Для Windows — Rust MSVC, Visual Studio C++ Build Tools, Node.js и WebView2. Официальный `claude` или `codex` нужен только для входа/запуска сессии, но не для работы менеджера. Статус проверки каждой платформы указан в актуальном протоколе сборки.

```sh
npm ci
npm run app:dev
```

Создание локального приложения:

```sh
npm run app:build
open 'target/release/bundle/macos/Fabric Switchboard.app'
```

Эта команда сама по себе не подтверждает Developer ID или notarization, а сборка, подписанная на ноутбуке, — отладочная. Релизы собирает, подписывает и нотаризует только release workflow в GitHub Actions после одобрения из `release-approvers`; см. [инструкцию дистрибуции](docs/DISTRIBUTION.md). Бинарные файлы не хранятся в Git. Зависимости зафиксированы Cargo.lock и package-lock.json; побайтовая воспроизводимость не заявляется.

CLI из исходников:

```sh
cargo build --release --locked -p switchboard-cli
./target/release/switchboard --help
./target/release/switchboard accounts list
./target/release/switchboard serve
```

`serve` или открытое desktop-приложение владеет прокси и сессиями. Другие команды CLI автоматически обращаются к этому владельцу. При отсутствии владельца операции с метаданными выполняются под эксклюзивной блокировкой; login и launch требуют работающий runtime.

Для просмотра интерфейса на синтетических данных: `npm run dev`, затем `http://127.0.0.1:1420/?demo=1`. Обычная веб-страница не подключается к хранилищу. В браузерном demo нет настоящих аккаунтов, запуска CLI или сетевых вызовов к провайдерам.

## Как пользоваться

1. **Add account** — официальный вход в отдельном профиле, API key, Claude setup token или явно импортированный OAuth JSON. Приложение не читает текущую глобальную авторизацию автоматически.
2. Задайте label и pool, например `work` либо `personal`. Пул определяет границу маршрутизации; аккаунты разных провайдеров не взаимозаменяемы.
3. **Select** — выбрать аккаунт для следующих управляемых запросов.
4. **Launch managed** — выбрать каталог проекта и открыть CLI через локальный прокси. Приложение или `switchboard serve` должно оставаться открытым. На один provider/pool допускается один запущенный управляемый home.
5. **Launch isolated** — отдельный home выбранного аккаунта и прямое соединение CLI с провайдером. Последующие Select на него не действуют.
6. **Check usage** — явная проверка квоты OAuth. API key не выдаёт достоверную квоту подписки; неизвестная квота не превращается в ноль.

Постоянный credential находится в OS vault, но isolated-режим создаёт необходимую официальному CLI рабочую копию access token в приватном файле (0600 на macOS, user-only DACL на Windows). Refresh token туда не передаётся; по истечении срока нужен повторный вход. Подробности, восстановление после сбоя и удаление: [операционная инструкция](docs/OPERATIONS.md).

## Проверки и структура

```sh
./scripts/check.sh
# Явно запрошенный тест создаёт и удаляет только случайный синтетический Keychain item:
cargo test -p switchboard-core native_vault_roundtrip_uses_only_random_app_owned_item -- --ignored --exact
```

| Путь | Ответственность |
|---|---|
| `crates/switchboard-core` | аккаунты, Keychain/DPAPI, атомарные метаданные, route snapshot |
| `crates/switchboard-proxy` | loopback-аутентификация, запросы, потоки, usage |
| `crates/switchboard-runtime` | общий владелец, control API, официальный login и запуск CLI |
| `crates/switchboard-cli` | команда `switchboard`, stdin и JSON |
| `src-tauri` | нативное окно и IPC к общему runtime |
| `src` | TypeScript-интерфейс и явно обозначенный demo |
| `docs` | research, спецификация, сценарии, договорённости и evidence |

Полная hosted CI настроена на ночной запуск. Отдельная Windows-сборка запускается вручную для точного commit SHA; push/PR не запускают полный suite. Наличие workflow не означает успешное выполнение. Чужие исходники исследованы, но не включены как зависимости.

## Лицензия

Открытый исходный код под [GNU AGPL-3.0](LICENSE). Для использования, которое не выполняет условия AGPL, доступна [коммерческая лицензия](COMMERCIAL-LICENSE.md) — [passioncode.ai/business](https://passioncode.ai/business/). v0.4.1-beta.1 — первый релиз под AGPL. Версии до v0.4.0-beta.1 включительно выпущены под PolyForm Noncommercial или Internal Use (v0.4.0-beta.1) и под MIT (v0.3.1-beta.1 и раньше, коммиты до `7c36f4a` включительно) и сохраняют свою лицензию. Лицензии сторонних компонентов в собранных программах — [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Вклад принимается по [CLA](CLA.md), см. [CONTRIBUTING.md](CONTRIBUTING.md).
