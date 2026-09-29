# Switchboard — PassionCode.ai

[English](README.md) · **Русский**

Локальный менеджер аккаунтов Claude Code и Codex CLI с desktop-интерфейсом и командой `switchboard`. Постоянные секреты защищены macOS Keychain или Windows DPAPI; рабочие и личные аккаунты разделены пулами. В управляемой сессии выбранный аккаунт меняется **со следующего запроса**: текущий поток ответа продолжает использовать прежний.

**Статус: beta.** Общая тёмная дизайн-система PassionCode и жёлтая S-иконка. Опубликован [v0.3.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.3.1-beta.1) (prerelease, macOS + Windows); `main` содержит исправления 0.3.2, ещё не выпущенные. Добавлены сохранение текущей авторизации CLI, импорт Claude Swap, активный CLI-профиль, окна лимитов и фоновое автопереключение. Сохранены интерфейс, CLI, хранилище, HTTP/SSE-прокси и адаптеры macOS/Windows. [Контракт 0.3](docs/ACCOUNTS-AND-ROTATION.md) и [проверки/сборки 0.3.1](docs/evidence/release-0.3.1.md). Вход и inference через реальные аккаунты провайдеров ещё не проверены. [Протокол 0.2](docs/evidence/release-0.2.md) отдельно фиксирует тесты, полученные сборки, подпись и notarization; [историческая проверка 0.1](docs/evidence/verification.md) сохранена.

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

Эта команда сама по себе не подтверждает Developer ID или notarization. Процедура подписи и отдельный Windows workflow описаны в инструкции дистрибуции. Бинарные файлы не хранятся в Git. Зависимости зафиксированы Cargo.lock и package-lock.json; побайтовая воспроизводимость не заявляется.

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

Source-available: PolyForm Noncommercial 1.0.0 или PolyForm Internal Use 1.0.0 на выбор; коммерческая лицензия по запросу (contact@passioncode.ai). Текст — [LICENSE](LICENSE). Релизы до v0.3.1-beta.1 включительно и коммиты до `7c36f4a` включительно были выпущены под MIT и остаются доступны под MIT. Лицензии сторонних компонентов в собранных программах — [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Вклад принимается по [CLA](CLA.md), см. [CONTRIBUTING.md](CONTRIBUTING.md).
