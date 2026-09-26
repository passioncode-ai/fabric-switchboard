<sub>ssheleg skills — task-pipeline · ux-scenarios · sheleg-design · brand-voice · copywriting</sub>

# Четыре подхода к переключению аккаунтов Claude Code и Codex

Исследование исходников · 26 сентября 2026 · статический обзор зафиксированных ревизий.

## Что установлено

Для Fabric Switchboard разумная основа — **изолированные профили плюс отдельный управляемый HTTP-прокси**.
Первый режим выбирает аккаунт для нового запуска, второй — для следующего запроса уже подключённого клиента.
Переключение существующего Codex Desktop требует отдельного механизма завершения и повторного запуска.
Ни один изученный механизм не даёт оснований обещать замену личности посреди уже принятого upstream запроса.
Это проектный вывод из сравнения ниже; [SPEC](../SPEC.md) и [CONTRACTS](../CONTRACTS.md) описывают наше решение, а не результаты его приёмки.

Важные уточнения: у Codex Account Switcher Windows-интерфейс на C#/WPF использует общий Swift core;
claude-swap хранит часть резервных credentials в Base64-файлах `.enc`, без шифрования;
CC Switch уже содержит собственный OAuth-центр, поэтому сводить его к редактору API-ключей неверно.
Доказательства: [общий core][cas-windows], [Base64][cs-base64], [OAuth][cc-oauth].

## Границы исследования

Первые три проекта заданы пользователем; **CC Switch — предварительно выбранный нами четвёртый comparator**,
пока его соответствие неназванному четвёртому проекту пользователя не подтверждено.
Исходники четырёх проектов клонированы отдельно от продукта, только для чтения.
Точные remote и SHA находятся в [sources.json](sources.json); ссылки ниже закреплены за этими SHA.
Читались entry points, модели хранения, пути переключения, прокси, OAuth, usage, платформенные адаптеры и manifests.
Чужой код, установщики, OAuth, тесты и сетевые запросы к провайдерам **не запускались**.
Реальные профили пользователя, токены, Keychain, MCP и работающие процессы не читались и не менялись.

«Есть в коде» означает статически найденную реализацию, а не доказанную работу на текущей версии официального клиента.
README используется как проверяемое заявление автора; доказательство механизма — ссылка на реализацию.
Указанные авторами ограничения API, cache TTL, релизов и подписи не проверены на действующем сервисе.
Это выборочный архитектурный обзор, не полный security audit и не испытание производительности.

## Сравнительная матрица

| Проект | Стек и поверхность | Что реально переключается | Когда действует | Основное хранение |
|---|---|---|---|---|
| Codex Account Switcher | Swift core; SwiftUI/macOS; WPF/Windows через stdio host | Активный `auth.json`, затем Desktop reopen | После перезапуска Desktop; новые CLI | Полные файловые snapshots, права 0700/0600 или Windows ACL |
| Subswapper | Go CLI, monitor, optional HTTP proxy | Account home либо авторизация запроса | Следующий запуск без proxy; следующий запрос через proxy | Native homes, JSON state, отдельные setup-token files |
| claude-swap | Python, Textual TUI, rumps menu bar | Native Claude credentials + account config либо отдельный session home | После перечитывания credentials клиентом; новая session при запуске | macOS Keychain; файловые Base64 backups/fallback; plaintext seed profiles |
| CC Switch | Tauri 2, Rust, React 18/TypeScript, SQLite | Provider config; OAuth binding; при routing — выбранный backend | Config reload/restart либо следующий routed request | SQLite configs; отдельные OAuth JSON |

Стек: [CAS][cas-stack], [CAS Windows][cas-windows], [Go][sub-stack], [Python][cs-stack], [Rust][cc-stack], [React][cc-react].
Механизмы: [CAS handoff][cas-switch], [Subswapper homes][sub-homes], [Subswapper proxy][sub-codex],
[Claude activation][cs-activate], [CC request context][cc-context], [CC live config][cc-live].

| Формулировка автора | Проверенная реализация | Что нельзя из неё заключить |
|---|---|---|
| CAS: completed Desktop handoff | Close → identity check/save → install → verify → commit → reopen | Что текущая генерация продолжится без остановки |
| Subswapper: live swapping | Credentials выбираются при обработке запроса, ответ передаётся по выбранному соединению | Что уже открытый stream меняет аккаунт |
| claude-swap: обычно без restart | Запись native credentials/config; сообщение о cache-dependent эффекте | Что switcher владеет cache или гарантирует точный момент применения |
| CC Switch: hot switch | Различаются direct config и local routing; request context хранит provider | Что всякая смена модели или любой внешний процесс подхватит выбор без restart |

Заявления и реализация: [CAS README][cas-claim] / [switch][cas-switch];
[Subswapper handler][sub-claude]; [claude-swap README][cs-claim] / [follow-up][cs-followup];
[CC direct][cc-claim] / [routing][cc-proxy-claim] / [context][cc-context].

## 1. Codex Account Switcher: аккуратная передача Desktop

### Устройство и добавление аккаунта

Swift Package выделяет `SwitcherCore` и `SwitcherHost`; macOS-приложение подключает core напрямую.
Windows использует WPF и приватный stdio транспорт к bundled Swift host: C# не дублирует account policy.
Это полезный пример разделения общей логики и системного UI, но потребует двух UI-команд при развитии продукта.
[Package][cas-stack], [Windows boundary][cas-windows].

Добавление через браузер реализовано поверх **официального Codex runtime**:
процесс `codex app-server --stdio` получает `CODEX_HOME` профиля, затем RPC `account/login/start` с `type=chatgpt`.
Приложение открывает возвращённый `authUrl`, ждёт `account/login/completed`, затем читает `account/read`.
Ожидание ограничено 600 секундами; cancellation останавливает созданную RPC session.
Это не самостоятельно зарегистрированный OAuth-клиент переключателя. [Process][cas-process], [login][cas-login].

Есть импорт текущего входа: полный `auth.json` копируется в UUID-профиль.
При добавлении проверяется существование credentials и duplicate identity; удаление активного профиля запрещено.
Если удаление папки после изменения registry не удалось, код пытается восстановить registry.
[Import/add][cas-storage], [remove][cas-remove].

### Переключение и его предел

1. Проверяется целевой профиль и известность текущего аккаунта.
2. Desktop закрывается штатно; ошибка останавливает переключение до записи credentials.
3. Текущая identity сверяется с registry, затем сохраняется актуальный `auth.json`.
4. Файл выбранного профиля устанавливается в активный home.
5. Identity перечитывается; только после совпадения фиксируется active ID.
6. Desktop открывается повторно. Ошибка reopen не означает, что credential switch отменился.

Порядок виден в [SwitchService][cas-switch]. При verify/commit failure предусмотрено ограниченное
восстановление предыдущего credential; ошибка восстановления сообщается отдельно. [Recovery][cas-recovery].
Сохранить процесс и текущую генерацию этот путь не пытается. Существующие CLI не закрываются;
следующие CLI должны использовать выбранный active home — это заявленная платформенная граница [Windows adapter][cas-windows].

### Хранение, usage и риски

Snapshots содержат повторно используемый login, включая всё содержимое `auth.json`.
Защита — приватные директории/файлы, проверка путей и атомарная замена; **это не app-level encryption**.
На macOS используются mode 0700/0600, на Windows отдельный native path adapter. [Atomic storage][cas-atomic].
Копии одного refresh lineage всё равно требуют осторожности: сохранение текущего snapshot уменьшает устаревание,
но не доказывает согласованность с любыми внешними CLI, которые самостоятельно обновляют тот же login.
Это вывод о границе владения, а не воспроизведённый дефект.

Usage читается RPC `account/rateLimits/read`, identity — `account/read` с `refreshToken=false`.
Последнее подтверждает согласованность identity через native runtime, а не успешный inference-запрос к провайдеру.
Обновление блокирует повторный refresh task внутри controller, опрашивает профили параллельно,
период по умолчанию 300 секунд. [RPC][cas-usage], [polling][cas-poll].
Сильная идея для нас: видимая стадия handoff и отдельное состояние «аккаунт изменён, открыть Desktop не удалось».
Не переносим в первый релиз: автоматическое закрытие чужого Desktop и запись в глобальный `auth.json`.

## 2. Subswapper: homes, монитор и маршрутизация запросов

### Добавление и запуск

Go-приложение имеет маленький набор runtime dependencies: PTY и системные terminal packages. [Manifest][sub-stack].
CLI различает `home create`, `home login`, `home run`, `capture`, setup-token и миграционные команды.
`home login` описан как вызов native login провайдера, а `capture` — импорт существующего входа;
сам список команд не является доказательством успешного OAuth на текущем runtime. [Command surface][sub-cli].

Account home задаётся через `CLAUDE_CONFIG_DIR` или `CODEX_HOME`.
Создание берёт state lock, проверяет имя/дубликат, делает private directory, затем сохраняет metadata.
У Claude автоматически подготавливаются ссылки на allowlisted user configuration; при конфликте создание прерывается.
[Environment][sub-homes], [creation][sub-create], [share allowlist][sub-share].

Нативный home, общий runtime home и account home — разные сущности.
Shared runtime позволяет сохранить локальную историю между routed аккаунтами, но тем самым соединяет их локальное состояние.
Настройки, plugins, skills и agents в allowlist не являются безвредными credentials-free данными по определению:
содержимое пользовательского settings может включать чувствительные настройки. Это граница проектирования,
а не результат чтения пользовательских файлов. [Runtime selection][sub-homes], [share list][sub-share].

### Как работает live switch

Claude запускается с base URL proxy и локальным placeholder; handler выбирает список маршрутов,
затем заменяет Authorization на credential выбранного аккаунта и передаёт ответ.
Клиент должен быть запущен/настроен для этого proxy заранее. [Handler][sub-claude], [headers and relay][sub-headers].

Codex нуждается также в совместимом `auth.json` placeholder.
`EnsureCodexProxyAuth` отказывается перезаписывать реальный login; отдельная миграция сохраняет его backup.
При отправке заменяются и bearer, и account ID; иначе запрос с токеном B мог бы нести workspace A.
[Placeholder][sub-placeholder], [forwarding][sub-forward].

Маршруты считываются до upstream-запроса. Изменение active account влияет на следующие запросы;
текущий response body relay остаётся на уже выбранном ответе.
Codex handler явно отказывает WebSocket upgrade: это HTTP-режим, не универсальный транспорт.
[Request boundary][sub-codex].

Важное отличие от нашей v0.1: здесь есть replay на другого кандидата при 401/429,
включая throttling без quota headers; обычный сетевой сбой возвращает ошибку вместо перебора.
Sticky selection различает quota rejection и временный throttling.
Тело буферизуется для replay; это требует отдельной оценки повторных эффектов и серверных гарантий,
даже если код не переповторяет успешно начавшийся ответ. [Claude retry][sub-claude], [Codex retry][sub-codex].

### Usage, refresh и автоматический выбор

Claude OAuth usage/profile запрашиваются на Anthropic endpoints; native home mode оставляет refresh официальному клиенту,
чтобы копия refresh token не конкурировала с работающей сессией. Legacy bundle mode содержит иной refresh путь.
[Endpoints][sub-usage], [ownership branch][sub-refresh].
Codex usage использует app-server с account home; proxy дополнительно читает `wham/usage`
и ограничивает фоновые запросы через in-flight bookkeeping и freshness. [RPC path][sub-rpc], [proxy usage][sub-wham].
Setup-token — отдельный JSON envelope с revision, временем и токеном; это не encrypted vault. [Envelope][sub-token].

Monitor содержит threshold 90%, minimum improvement 10% и cooldown 30 минут; warm-up — отдельная ветка.
Эти значения — defaults выбранной ревизии, не наше рекомендуемое поведение.
Warm-up нельзя незаметно переносить в продукт, обещающий отсутствие расхода до явного действия. [Monitor][sub-auto].

Windows имеет native lock через `CreateFile` без sharing, но общий код с chmod не доказывает нужную DACL.
Для shared symlink configuration README требует Developer Mode/привилегию; это отдельный operational prerequisite.
[Windows lock][sub-win], [sharing requirement][sub-win-share].
Берём разделение route/runtime/account home и placeholder; не переносим shared state, warm-up и replay как defaults.

## 3. claude-swap: native storage, борьба с refresh races и TUI

### Поверхности и аккаунты

Python >=3.12, Textual 8, optional rumps menu bar; package помечен Beta. [Manifest][cs-stack].
Базовый add захватывает существующий Claude login; есть отдельные пути token/API-key добавления,
перезаписи слота, enable/disable и session launch. В отличие от CAS, это не собственный браузерный OAuth onboarding
через управляемый app-server. [Capture implementation][cs-add], [account commands][cs-commands].

Обычный switch пишет native credential и splice account-specific `oauthAccount` в конфигурацию;
это не HTTP proxy и не смена environment внутри уже работающего процесса.
Путь берёт свой file lock плюс locks native credential/config перед записью. [Activation][cs-activate], [locks][cs-locks].

README обещает следующий message при file backend и примерно 30 секунд cache на macOS.
Код follow-up отражает зависимость от backend, но сам cache принадлежит Claude Code.
Поэтому задержку следует считать **заявлением совместимости автора**, а не гарантией нашего исследования.
Момент применения на конкретном CLI/extension требует live acceptance. [Claim][cs-claim], [message][cs-followup].

### Секреты: Keychain, fallback и вводящее в заблуждение расширение

На macOS per-account backups используют Keychain service `claude-swap`; Linux/WSL/Windows используют файлы.
При проблеме Keychain предусмотрен file fallback; degraded read может означать устаревшую генерацию,
и код явно запрещает потреблять refresh token из такого чтения. [Backend][cs-file], [degradation][cs-degrade].

`.enc` — Base64, который декодируется без ключа. Запись временного файла/replace и 0600 защищают целостность
и доступ другого Unix пользователя, **но не шифруют содержимое**. [Encoding][cs-base64].
У нас ошибка Keychain должна оставаться ошибкой, без тихого plaintext fallback.

### Почему refresh сложнее копирования файлов

Путь `consume_backup_grant` сериализует re-read → POST → compare-and-swap на fingerprint refresh lineage.
Отдельный consume lock охватывает сетевой обмен, обычный slot lock — только локальное состояние.
Результат, проигравший CAS, сохраняется для последующего adoption, чтобы не потерять уже полученного successor.
Это полезный образец требований к будущему refresh-manager. [Consume gate][cs-refresh].

Usage collector обычно не refresh-ит активный аккаунт: его credentials принадлежат Claude Code.
Session credentials могут стать свежее stored backup; включение того же аккаунта в default login
должно учитывать живую session. [Usage contract][cs-active], [session caveats][cs-mcp].
Наличие таких веток не доказывает отсутствие всех межпроцессных races — нужен fault injection с native client.

### Изоляция и пользовательское состояние

`cswap run` создаёт persistent per-account `CLAUDE_CONFIG_DIR`; macOS Keychain service определяется
первыми восемью hex символами SHA-256 от NFC-normalized **raw path string**, не realpath.
Сначала профиль seeded plaintext credential; комментарий объясняет последующее native Keychain поведение.
[Session mechanism][cs-session], [hash function][cs-hash].

По умолчанию preferences/skills/agents разделяются symlinks на POSIX и синхронизируемыми копиями на Windows.
History sharing opt-in соединяет `projects/` и `history.jsonl`; это локальные transcripts, не перенос
server-owned conversations или прав аккаунта. User-scope MCP definitions зеркалируются,
но MCP OAuth не переносится автоматически. [Sharing][cs-session], [MCP][cs-mcp].

Usage endpoint — `/api/oauth/usage`; общий persisted poll plan и 180-секундный freshness floor
предотвращают отдельный сетевой запрос от каждого repaint TUI/menu bar.
Это берём как принцип: один наблюдатель, много читателей; значения лимитов API не переносим как вечную константу.
[Request][cs-usage], [poll policy][cs-poll].
Windows код отдельно обрабатывает sharing violations при replace с bounded retry. [Implementation][cs-win].
Наличие этой защиты полезно, но Windows vault/ACL/security acceptance ею не заменяется.

## 4. CC Switch: полноценный provider workbench

### Стек и масштаб

Tauri 2/Rust backend, React 18/TypeScript UI, SQLite через rusqlite; есть tray, updater и single-instance plugin.
Это ближайший референс desktop workbench, но его область шире нашего переключателя:
provider configs, auth bindings, MCP, prompts, skills, routing, usage и несколько клиентских форматов.
[Cargo][cc-stack], [frontend][cc-react], [database][cc-db].

Provider сохраняется как запись с `settings_config`, metadata и принадлежностью failover queue.
`settings_config` сериализуется в JSON в SQLite, поэтому app-level encryption из этой схемы не следует.
База открывается обычным `Connection::open`, без видимой SQLCipher-конфигурации в просмотренном пути.
[Provider serialization][cc-provider], [database open][cc-db].

### Direct config и proxy — два разных переключения

Direct mode управляет native live configuration.
README различает Claude hot reload, restart CLI для Codex/Gemini/Grok и restart приложения Claude Desktop.
Routing mode обещает новый provider со следующего запроса; смена модели всё ещё может потребовать restart.
[Live module][cc-live], [direct semantics][cc-claim], [routing semantics][cc-proxy-claim].

RequestContext содержит выбранный Provider и список кандидатов; ProviderRouter читает current provider
и отдельно учитывает automatic failover. Codex Official намеренно исключён из чужого provider retry,
поскольку его входящая native Authorization принадлежит уже выбранному аккаунту.
Это важное доказательство того, что «provider», «account», «workspace» и «request» нельзя объединять в одно поле.
[Context][cc-context], [router boundary][cc-router].

### OAuth-центр и refresh ownership

Выбранная ревизия содержит свой ChatGPT Device Code flow: usercode → browser → polling →
authorization code + verifier → token exchange; client ID и endpoints заданы в адаптере.
Это иной maintenance burden, чем запуск официального CLI; изменения серверного flow нужно обслуживать самому.
Комментарий о происхождении client ID не является разрешением провайдера или гарантией стабильности API.
[Flow/constants][cc-oauth].

Auth manager хранит local account ID отдельно от `chatgpt_account_id`, serializes refresh/id token,
имеет per-account refresh locks, lifecycle lock, storage lock и generation bookkeeping.
Они направлены на гонки между refresh, login, delete и adoption native live-auth.
[Account model][cc-tokens], [locks][cc-locks], [save ordering][cc-save].

OAuth store записывается в plaintext JSON. На Unix temporary file создаётся 0600 и переименовывается.
В Windows-ветке перед rename старый destination удаляется: между удалением и rename есть crash window;
в просмотренной функции также нет DPAPI/Credential Manager или явной настройки DACL.
Это **статически видимая граница конкретного writer**, не утверждение о каждом файле всего приложения.
[Writer][cc-store].

### Usage и Windows

Usage не сводится к одной subscription quota: у provider может быть configurable usage script,
с отдельными API key/base URL/access token и timeout. Такая расширяемость требует самостоятельной threat model,
особенно если конфиг импортируется. Для v0.1 нам достаточно фиксированных provider adapters.
[Usage execution inputs][cc-usage].
Windows присутствует в release workflow x64/ARM и README содержит WSL-specific loopback caveat.
Это свидетельство платформенной реализации и packaging, но выполненный pipeline и native acceptance
на данном SHA этим обзором не подтверждены. [Release matrix][cc-ci], [WSL caveat][cc-wsl].

## Что объединить в Fabric Switchboard

| Берём | Источник идеи | Наш контракт |
|---|---|---|
| Явные стадии и восстановление | CAS Desktop handoff | selected отдельно от observed; failed reopen не скрывать |
| Отдельные account и runtime homes | Subswapper | isolated account home; managed pool home |
| Credential snapshot на request boundary | Subswapper / CC Switch | токен A остаётся у текущего stream, B у следующего request |
| Владение refresh lineage | claude-swap / CC Switch | v0.1 reauth; refresh-manager позже, после race tests |
| Несколько UI читают одно usage observation | claude-swap | timestamp/source/stale/error, без repaint polling |
| Desktop IPC boundary | CAS host / Tauri CC Switch | frontend не получает secret-returning command |
| OS-specific storage semantics | Windows adapters всех проектов | native vault + DACL acceptance, не chmod-эмуляция |

Наш выбор Rust/Tauri позволяет одной core реализации обслуживать macOS и будущий Windows UI.
Это инженерное предпочтение, не измеренное превосходство по RAM, размеру bundle или startup time.
JSON metadata допустим для bounded accounts/events; SQLite нужен при измеренной потребности в индексах/истории.
Keychain хранит секреты отдельно от metadata. Подробнее — [SPEC §§5, 7, 10–12](../SPEC.md).

Предлагаемый onboarding: Add → provider/kind → native isolated login или явный import →
label/pool → syntax-valid account → отдельная authenticated observation.
Добавленный и выбранный аккаунт ещё не доказан как обслуживший запрос.
Для Claude setup-token quota может быть неизвестна до response headers; неизвестность не рисуется как 0%.
Автопереключение, warm-up, shared history/MCP и перенос Desktop оставляем отдельными opt-in возможностями.

## Обязательные проверки до обещания «работает вживую»

1. Два synthetic upstream: stream A остаётся открыт, выбор меняется, request B получает только credential B.
2. Входящий account ID/local token/cookie никогда не уходит upstream вместо рассчитанной identity.
3. 401/429/timeout не переповторяет запрос в v0.1; unsupported WebSocket получает явную ошибку.
4. Expired OAuth ведёт к reauth; denial Keychain не создаёт plaintext fallback.
5. Official login создаёт только owned home; cancel не меняет чужой default login.
6. Native Claude/Codex exact versions: настоящий запрос и наблюдаемая account identity, отдельно от fixture pass.
7. Crash между vault/metadata updates, две копии приложения и symlink/reparse-point входы.
8. Windows: vault isolation, DACL, replace contention, path Unicode, terminal quoting и streaming, на самой Windows.

Эти пункты — план приёмки, не заявление, что они уже пройдены.
Результаты продукта должны жить в [verification](../evidence/verification.md), а следующий шаг — в [handoff](../HANDOFF.md).

## Лицензирование и использование исходников

Во всех четырёх pinned repositories файл LICENSE содержит MIT.
Это результат чтения файлов [CAS][cas-license], [Subswapper][sub-license], [claude-swap][cs-license], [CC Switch][cc-license].
При переносе существенного кода следует сохранять требуемые copyright/license notices;
этот обзор не проверяет лицензии всех transitive dependencies, товарные знаки или договорные условия аккаунтов.
Спецификация Fabric заимствует архитектурные идеи; наличие MIT не доказывает совместимость с provider policies.

## Воспроизводимость

Проверены совпадения `git rev-parse HEAD` с четырьмя SHA из `sources.json`.
Команда `python3 docs/research/check_sources.py /path/to/account-switch-research` проверяет source paths
и диапазоны строк по Git objects. Выполнено: PASS, 4 pinned HEADs, 69 source ranges; HTTP link-check не запускался.
Никакие test results, подписи релизов или работоспособность чужих бинарников этому документу не приписываются.
Следующий исследовательский шаг: live acceptance официальных клиентов в owned temporary homes,
с явно предоставленными аккаунтами, после проверки нашего proxy/security contract.

---

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**

Источники ниже адресуют неизменяемые коммиты; результаты ограничены перечисленными файлами и статическим методом.

[cas-stack]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Package.swift#L49-L94
[cas-windows]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/windows/README.md#L19-L36
[cas-login]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/CodexClient.swift#L509-L557
[cas-process]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/CodexClient.swift#L209-L229
[cas-switch]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/SwitchService.swift#L27-L109
[cas-recovery]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/SwitchService.swift#L111-L138
[cas-storage]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/AccountStore.swift#L144-L170
[cas-remove]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/AccountStore.swift#L191-L220
[cas-atomic]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/AccountStore.swift#L330-L383
[cas-usage]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/CodexClient.swift#L486-L506
[cas-poll]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/Sources/SwitcherCore/AccountController.swift#L85-L159
[cas-claim]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/README.md#L99-L113
[sub-stack]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/go.mod#L1-L10
[sub-cli]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/README.md#L102-L121
[sub-homes]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/homes.go#L36-L72
[sub-create]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/homes.go#L74-L148
[sub-share]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/claude_home_config.go#L12-L26
[sub-token]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/setup_tokens.go#L58-L84
[sub-claude]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/claude_proxy.go#L324-L403
[sub-headers]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/claude_proxy.go#L441-L504
[sub-codex]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/codex_proxy.go#L411-L510
[sub-placeholder]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/codex_proxy.go#L255-L334
[sub-forward]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/codex_proxy.go#L531-L548
[sub-usage]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/claude_usage.go#L16-L25
[sub-refresh]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/claude_usage.go#L70-L104
[sub-rpc]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/codex_usage.go#L76-L100
[sub-wham]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/codex_proxy.go#L662-L765
[sub-auto]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/monitor.go#L22-L58
[sub-win]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/internal/subswapper/lock_windows.go#L15-L46
[sub-win-share]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/README.md#L278-L310
[cs-stack]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/pyproject.toml#L1-L35
[cs-add]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L3482-L3550
[cs-refresh]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L2011-L2084
[cs-activate]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L7173-L7220
[cs-locks]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L6776-L6785
[cs-followup]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L7258-L7279
[cs-claim]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/README.md#L209-L229
[cs-file]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/credentials.py#L1038-L1059
[cs-base64]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/credentials.py#L1113-L1138
[cs-degrade]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/credentials.py#L135-L158
[cs-session]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/session.py#L1-L31
[cs-hash]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/session.py#L232-L243
[cs-mcp]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/README.md#L133-L141
[cs-usage]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/oauth.py#L397-L407
[cs-active]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/oauth.py#L639-L655
[cs-poll]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/poll_policy.py#L55-L79
[cs-win]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/fsutil.py#L65-L101
[cc-stack]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/Cargo.toml#L23-L87
[cc-react]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/package.json#L55-L86
[cc-db]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/database/mod.rs#L96-L125
[cc-provider]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/database/dao/providers.rs#L240-L262
[cc-live]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/services/provider/live.rs#L1-L19
[cc-claim]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/README.md#L288-L295
[cc-proxy-claim]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/README.md#L339-L347
[cc-router]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/provider_router.rs#L15-L80
[cc-context]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/handler_context.rs#L35-L65
[cc-oauth]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L1-L58
[cc-locks]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L389-L410
[cc-tokens]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L280-L303
[cc-store]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L1972-L2024
[cc-save]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L2052-L2065
[cc-usage]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/src-tauri/src/services/provider/usage.rs#L126-L173
[cc-ci]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/.github/workflows/release.yml#L16-L31
[cc-wsl]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/README.md#L471-L475
[cas-license]: https://github.com/liuzhao1225/codex-account-switcher/blob/a371af174defd4e579424286e71f2c08db1f3514/LICENSE#L1-L21
[sub-license]: https://github.com/lawzava/subswapper/blob/cba1f91958cfe1cf49209aead8c2527220da9b9d/LICENSE#L1-L21
[cs-license]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/LICENSE#L1-L21
[cc-license]: https://github.com/farion1231/cc-switch/blob/1ee2fdc3a791f1e73476c631c7ab7ce8fac0638f/LICENSE#L1-L21

[cs-commands]: https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/README.md#L179-L204
