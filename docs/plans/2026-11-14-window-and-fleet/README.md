# The window and the fleet: one spec in modules (2026-11-14)

The operator's review of the window on 2026-11-14 (chat, about 16:40 UTC), in twelve modules
that are each worked out, built and released on their own, in waves through `task-pipeline`.
Board rows: one per module, [SB-363 to SB-374](../../evidence/backlog.md). This file is the entry
point; each module file holds its own operator words, current state with `file:line`, sub-modules
and tasks, interface and MCP notes, decisions, open questions with the assumed answer, checks and
dependencies.

## What was asked (the operator's words, abridged in order)

> аккаунты, группировка... сначала у нас были аккаунты на этой машине, потом уже показывались на
> других... изначально группировка по машинам. Вот эти фильтры... дизайн поправить, automatic
> switching... пусть оно будет в настройках... Сессия, кодекса поднимем выше. И вот InUseNow кодекс
> почему-то не такими... given to agents... больше информации. ...подпискам, которые not checking,
> while disabled... чекинг подписок, который на другом устройстве, должен все равно сюда
> синхронизироваться... продумать все это сетевое взаимодействие... автоматические смены...
> чтобы ничего не вылетало... Подписки... с ошибками, нужно залогиниться, их сразу поднимаем вверх,
> в отдельную категорию, типа "Attention"... после залогинивания они возвращаются туда же...
> API ключи... более компактное... по группам разделить, по блокам... группы внутри... сделать
> читаемым... как подконнектиться к организации, как дисконнектиться... связь с Вестой, с облаком...
> проверена... настройки агентов... глобальные... switchboard... централизованное место...
> темплейты... скиллы... общие плагины... Activity... длинные таблицы... сворачиваемые...
> все должно быть изначально в MCP... не должно быть никакой функции, которая работает в интерфейсе
> и не работает в MCP... несколько свитчбордов... на удаленной машине... как админ организации...
> видеть свои организации... перейти внутрь... в зависимости от уровня доступа... связь свитчбордов
> на разных машинах, аккаунты, быстро... перекинуть аккаунты между машинами... автоматический
> роутинг между машинами... группу backup аккаунтов... когда... не заканчивают аккаунты тогда они
> туда переключаются и поддерживают эту группу пока там не появится свой аккаунт потом возвращается
> обратно в бэкап

Then: «Все это давай разбери, изучи... составь общую спеку, раздели ее на модули... на задачи, на
подмодули... смотреть вопросы по реализации, интерфейсу... Потом по task pipeline все это закрываем,
волнами... тестим и все выкатываем.»

## The request as rows (REQ spine)

| REQ | What is true when it is done | Module |
|---|---|---|
| R-01 | Accounts are shown by machine: this computer first, then each other machine | [WF-2](WF-02-accounts.md) |
| R-02 | The filters read as one clear control | WF-2 |
| R-03 | Automatic switching is configured in Settings, not on the Accounts page | [WF-1](WF-01-foundations.md) |
| R-04 | Codex sits right after Claude; Claude and Codex *In use now* look alike | WF-2 |
| R-05 | *Given to agents* says which agent, its organization, its machine and where it works | WF-2 |
| R-06 | An account used on another machine shows that machine's usage, not "Not checked while disabled" | [WF-4](WF-04-remote-usage.md) |
| R-07 | Network work with other machines never hangs or breaks the window | WF-4 |
| R-08 | Automatic switching never drops a session | [WF-5](WF-05-switching.md) |
| R-09 | Accounts that need a sign-in stand at the top under *Attention* and stay visible, disabled, in their group; signed in again they are back where they were | [WF-3](WF-03-attention.md) |
| R-10 | The API keys page is compact and in blocks per group | [WF-7](WF-07-api-keys.md) |
| R-11 | A group's page is readable | [WF-8](WF-08-groups.md) |
| R-12 | Connecting to and leaving an organization, and the link to PassionCode.ai (Vesta) and the cloud, are checked end to end | [WF-9](WF-09-organizations.md) |
| R-13 | Switchboard is the central place for agent settings: global permissions, fallbacks, templates, shared skills and plugins | [WF-10](WF-10-agents.md) |
| R-14 | Activity and Statistics never ask for long scrolling: sections fold, tables page | [WF-11](WF-11-activity-statistics.md) |
| R-15 | Every window action has an MCP tool, checked by a test; MCP may do more, never less | WF-1 |
| R-16 | An organization admin sees the organization's machines, their Switchboards and accounts, moves accounts between machines quickly, by access level | [WF-12](WF-12-fleet.md) |
| R-17 | Routing between machines is designed and, opted in, works | WF-12 |
| R-18 | A backup group holds spare accounts that support a group out of room and come back | [WF-6](WF-06-backup-group.md) |

## Modules

| Module | Board | Sub-modules | Wave |
|---|---|---|---|
| [WF-1 Foundations: Settings page, folding sections, window to MCP parity](WF-01-foundations.md) | SB-363 | 1.1 Settings page · 1.2 Section component · 1.3 Parity test and missing tools | 1 |
| [WF-2 Accounts by machine](WF-02-accounts.md) | SB-364 | 2.1 Machine sections · 2.2 Filters · 2.3 In use now · 2.4 Given to agents | 1 |
| [WF-3 Attention](WF-03-attention.md) | SB-365 | 3.1 What needs attention · 3.2 The section · 3.3 Back in place | 1 |
| [WF-4 Usage of accounts on other machines](WF-04-remote-usage.md) | SB-366 | 4.1 Background read · 4.2 Network budget and failure · 4.3 Rows | 3 |
| [WF-5 Switching that never drops a session](WF-05-switching.md) | SB-367 | 5.1 Failure inventory · 5.2 Fixes · 5.3 Measure | 3 |
| [WF-6 Backup group](WF-06-backup-group.md) | SB-368 | 6.1 Decision · 6.2 Core · 6.3 Window and MCP | 3 |
| [WF-7 API keys page](WF-07-api-keys.md) | SB-369 | 7.1 Blocks per group · 7.2 Compact rows | 2 |
| [WF-8 Groups page](WF-08-groups.md) | SB-370 | 8.1 Group card · 8.2 Page sections | 2 |
| [WF-9 Organizations and PassionCode.ai](WF-09-organizations.md) | SB-371 | 9.1 Connect and leave, verified · 9.2 States and copy · 9.3 Cloud link | 4 |
| [WF-10 Agents: the central place](WF-10-agents.md) | SB-372 | 10.1 Global settings in the window · 10.2 Templates · 10.3 Shared skills and plugins | 2 and 4 |
| [WF-11 Activity and Statistics](WF-11-activity-statistics.md) | SB-373 | 11.1 Activity by day with filters · 11.2 Statistics folding and paging | 2 |
| [WF-12 Fleet: machines, Switchboards, organizations](WF-12-fleet.md) | SB-374 | 12.1 Own machines · 12.2 Organization admin · 12.3 Routing between machines | 4 and 5 |

## Waves

Each wave ends with the full gate on the Mac Studio, Linux clippy in a container, the release, the
nightly on every platform, the update on this Mac and a check of what the wave claims on the
released app. A wave does not start until the previous one is released.

| Wave | Content | Why first |
|---|---|---|
| 1 | WF-1, WF-2, WF-3 | the main screen the operator reads every day, and the parity test every later wave relies on |
| 2 | WF-7, WF-8, WF-11, WF-10.1 | the other pages, on the components wave 1 built |
| 3 | WF-4, WF-5, WF-6 | behaviour across machines and groups; each changes a decision (DEC-0012, DEC-0013) |
| 4 | WF-9, WF-10.2 and 10.3, WF-12.1 | organizations and agent templates, on wave 3's machine reads |
| 5 | WF-12.2, WF-12.3 | wait for the estate's hub and access levels (SB-177, SB-291) |

## Proposed decisions (each lands in [DECISIONS.md](../../DECISIONS.md) with its module)

- **DEC-0014, Settings owns standing configuration** (WF-1): automatic switching, balancing and the
  backup group, the Codex route, the terminal hook, updates, language, appearance and backups live on
  one Settings page; other pages show state and link there.
- **DEC-0015, Attention is a view, not a group** (WF-3): an account needing a person stays in its
  group, disabled, and is listed again at the top; it never moves.
- **DEC-0016, a backup group lends only** (WF-6): a group marked backup holds no folders, serves no
  session of its own, and lends to the chosen groups under DEC-0012's rules until the group has room
  of its own.
- **DEC-0017, the usage of an account on another machine is read in the background** (WF-4): amends
  DEC-0013's "on request" for reads only; no routing.
- **DEC-0018, Switchboard is the machine's place for agent settings** (WF-10): permissions, the
  default agent, fallbacks, templates, shared skills and plugins; other products link to it.
- **DEC-0020, routing between the person's own machines, opted in** (WF-12.3): supersedes DEC-0013's
  "never routes" for moving a sign-in to a machine out of room; requests are never relayed.
- **DEC-0019, every window action has an MCP tool** (WF-1): a test fails on a Tauri command with no
  tool and no written reason; adopts the contract's DEC-0121 here ahead of SB-200.

## Questions, with the answer assumed until the operator says otherwise

Asked once, together, on the Fabric board and in the hand-back; no build waits for them.

1. «Сессия, кодекса поднимем выше»: assumed the Codex accounts come right after Claude's in this
   computer's section, above other machines and *Given to agents* (WF-2.3).
2. Settings is a new sidebar page, not a part of About (WF-1.1).
3. Routing between machines is opt-in, only between the person's own machines Switchboard already
   moved accounts to, and moves a sign-in (send and recall) rather than relaying requests (WF-12.3).
4. A backup group can lend across groups of one organization context only, as balancing does (WF-6).

## Routes for every module

- Interface behaviour: `super-ux` scenarios in [docs/ux/scenarios.md](../../ux/scenarios.md) in the
  same change (DOCMAP), `docs/ux/lint.py` green.
- Look and motion: `sheleg-design`, the existing tokens (`src/tokens.css`), no hand-copied values.
- Words: `copywriting`, then `copylot_check` in English and Russian; registry
  [docs/brand/strings.md](../../brand/strings.md); `src/locales/ru.ts`.
- Proof: `evidence-docs`; contracts in [CONTRACTS.md](../../CONTRACTS.md); lifecycle in `AGENTS.md`.
- Delivery: `task-pipeline`, waves; the architect's class template `account-service` 1.1.2
  (fabric-agents-database `templates/account-service/`); gaps this plan found in it (window to MCP
  parity, an organization's fleet view, a backup pool) go to the base as a proposal.
