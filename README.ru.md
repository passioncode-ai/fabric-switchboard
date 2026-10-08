# Fabric Switchboard

[English](README.md) · **Русский**

Локальный менеджер аккаунтов и квот **Claude Code, Codex CLI и Kimi Code**.
Показывает текущий аккаунт и его квоту, помогает выбрать другой и открыть сессию в нужном
проекте. Приложение для macOS и Windows, команда `switchboard` и MCP-сервер от
[PassionCode.ai](https://passioncode.ai/).

**[Скачать для macOS](https://passioncode.ai/switchboard/download/macos)** ·
**[Скачать для Windows](https://passioncode.ai/switchboard/download/windows)** ·
[Установка](docs/INSTALL.md) · [Что нового](CHANGELOG.md) ·
[Сайт](https://passioncode.ai/switchboard/)

![Fabric Switchboard 0.6.13: текущие аккаунты, квоты и отдельные пулы](docs/evidence/screenshots/accounts-0.6.13.png)

*Текущий интерфейс на английском с демонстрационными аккаунтами. В приложении также есть русский язык. [Данные о снимке](docs/runs/2026-10-08-release-0612/README.md#screenshots).*

## Что можно делать

| Задача | В Switchboard |
|---|---|
| Разделять рабочие и личные аккаунты | Пулы, правила проектов и отдельные профили сессий |
| Видеть, когда понадобится другой аккаунт | Квоты провайдера, время сброса и явные отметки неизвестной квоты или истёкшего входа |
| Выбирать следующий аккаунт | Переключение обычного Claude Code или маршрутизация управляемых запросов; текущий ответ сохраняет свой аккаунт |
| Работать с подписками Kimi Code | Официальный вход, тариф и квоты, запуск выбранного аккаунта в проекте |
| Настраивать другие агенты | Модель и провайдер Hermes, запуск поддерживаемых агентов через OpenRouter |
| Работать из терминала или агента | CLI, MCP-инструменты и запуск в текущей консоли через `launch --in-place` |

Секреты защищены macOS Keychain или Windows DPAPI. Автопереключение включается по вашему
выбору. Каждый аккаунт Kimi остаётся в отдельной папке входа: Switchboard не копирует и не
продлевает его авторизацию. Подробности: [аккаунты и переключение](docs/ACCOUNTS-AND-ROTATION.md),
[поддержка агентов](docs/AGENT-SUPPORT.md).

## Текущая версия

**Текущий релиз: [v0.6.14](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.14)** · 8 октября 2026.
Аккаунты Kimi Code, настройка модели и провайдера Hermes, запуск через OpenRouter,
исправления входа Claude и ошибки после обновления. [Проверка скачиваемых файлов](docs/runs/2026-10-08-release-0612/README.md#published-release).
Версии 0.6.11–0.6.13 заменены до публикации.

macOS universal подписывается Developer ID и проходит нотариальную проверку Apple.
Windows x64 собирается нативно; подписи Authenticode пока нет. Начиная с 0.6.1 приложение
обновляется автоматически. Сборку и подписи выполняет защищённый
[release workflow](docs/DISTRIBUTION.md#how-a-release-happens).
Проверки с реальными аккаунтами провайдеров отмечаются отдельно на
[доске](docs/evidence/backlog.md) (SB-01, SB-02, SB-15).

Открытый исходный код под **GNU AGPL-3.0**; доступна коммерческая лицензия.
[Условия](#лицензия).

## Документация

- [CLI и MCP](docs/CLI.md): команды, JSON, вход и запуск агентов.
- [Карта продукта](docs/PRODUCT.ru.md): возможности и ограничения.
- [Спецификация](docs/SPEC.md): поведение, безопасность и критерии приёмки.
- [Эксплуатация](docs/OPERATIONS.md): профили, восстановление и удаление.
- [Передача работы](docs/HANDOFF.md): проверки и следующая задача.

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

1. **Add account** — официальный вход в отдельном профиле, API key, Claude setup token или явно импортированный OAuth JSON. Switchboard читает текущий вход Claude Code и Codex, чтобы показать, какой сохранённый аккаунт сейчас используется, но без вас его не сохраняет и не переключает.
2. Задайте label и pool, например `work` либо `personal`. Пул определяет границу маршрутизации; аккаунты разных провайдеров не взаимозаменяемы.
3. **Select** — выбрать аккаунт для следующих управляемых запросов.
4. **Launch managed** — выбрать каталог проекта и открыть CLI через локальный прокси. Приложение или `switchboard serve` должно оставаться открытым. На один provider/pool допускается один запущенный управляемый home.
5. **Launch isolated** — отдельный home выбранного аккаунта и прямое соединение CLI с провайдером. Последующие Select на него не действуют.
6. **Check usage** — квота проверяется в фоне (каждые 3 минуты для аккаунтов в работе или в пуле с автопереключением, каждые 10 минут для остальных); **Check usage** проверяет сразу. API key не выдаёт достоверную квоту подписки; неизвестная квота не превращается в ноль.
7. **Язык** — интерфейс на русском или английском, по языку системы; О программе → *Язык* переопределяет выбор ([SCN-041](docs/ux/scenarios.md#scn-041--use-switchboard-in-russian)).

Пользуйтесь Switchboard в соответствии с правилами Anthropic и OpenAI. Нарушение правил провайдера может привести к блокировке аккаунта. Ответственность за использование аккаунтов несёте вы.

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

Полная hosted CI (macOS и нативные Windows-фикстуры) запускается ночью; push/PR не запускают полный suite. Windows-сборка релиза делается в release workflow. Наличие workflow не означает успешное выполнение. Чужие исходники исследованы, но не включены как зависимости.

## Лицензия

Открытый исходный код под [GNU AGPL-3.0](LICENSE). Для использования, которое не выполняет условия AGPL, доступна [коммерческая лицензия](COMMERCIAL-LICENSE.md) — [passioncode.ai/business](https://passioncode.ai/business/). v0.4.1-beta.1 — первый релиз под AGPL. Версии до v0.4.0-beta.1 включительно выпущены под PolyForm Noncommercial или Internal Use (v0.4.0-beta.1) и под MIT (v0.3.1-beta.1 и раньше, коммиты до `7c36f4a` включительно) и сохраняют свою лицензию. Лицензии сторонних компонентов в собранных программах — [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Вклад принимается по [CLA](CLA.md), см. [CONTRIBUTING.md](CONTRIBUTING.md).
