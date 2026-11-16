# Agent health across PassionCode: signals, incidents and repair agents (proposal, 2026-11-16)

Board row: [SB-393](../evidence/backlog.md). A proposal for the whole estate, written where the
operator asked for it (this session) and sent to every project on the board; the estate's rules
live in fabric-workspace `knowledge/`, which is asked to take the decisions below.

## What was asked

> …не забывай о том, что еще типа у нас есть баги, их нужно исправлять. И следить за тем, чтобы
> главная задача, чтобы и агенты, главное, запускались, можно было их переносить между машинами
> безболезненно, не кикались, не сбивались, life cycle. вообще мониторинга типа ошибок которые у
> нас вообще по всем проектам passion кода есть во всех организациях надо тоже подумать как
> выстроить ими системы мониторинга агентов чтобы мы реагировали на уровне passion код организации
> когда у наших клиентов случаются проблемы с агентами или что-то типа критичное на уровне свитч
> борда дашборда фабрика на уровне всех передай это всем агентам … выкатим решение которое нам
> поможет быстро выпускать агентов для решения проблем. Также агентов, которые будут это все
> чинить и выкатывать, и проверка обновлений софта, проверка окружения, безопасность…

The agents database's architect was consulted first (`audit.py consult`, 2026-11-16): the closest
templates are `web-quality`, `installer` and `agent-host`, the pattern to reuse is
`alert-episodes` (1.1.0, used by ten templates), with `resumable-jobs` and `sandbox-queue` for the
repair runs, under R33 O8 (every action an agent capability) and R33 O9 (who gets which message).

## What exists today, and why nobody reacts

Measured on this MacBook on 2026-11-16 (Observatory `observatory_overview`, 12:04 UTC):

- 21 critical findings sit unread: the disk at 1.3 % free (3.5 GB at 12:40), a virtual machine
  writing to that disk, and 19 keys recorded as leaked 32 to 65 days ago with no rotation.
- Each product already reads its own health, in its own shape:
  - Switchboard: Needs attention (`switchboard attention`), crashes found at the next start and
    error counts (`switchboard bugs`), the one-minute session watch (stuck, duplicate, gone), the
    duty officer's journal, hourly and daily operations digests (`ops.digest.*@1`, the contract's
    DEC-0153).
  - Fabric Dashboards: `audit` over every installed agent (findings by severity), `list_services`
    (state, `update_available`), `setup_check` (the launcher's doctor: skills, Fabric, Dashboards,
    Switchboard, Inbox, Observatory, MCP).
  - Observatory: findings (disk, memory, leaked or expiring keys, dark sites, clones nowhere else),
    `observatory_dependencies` (OSV advisories of every lockfile), `observatory_ssh_keys`,
    `ops_digest_record` (the daily digest of every product, kept 400 days).
  - Fabric: the board (requests by capability, `com.submit`), claims and heartbeats of sessions.
  - Every agent built from the base kit: its own `health` door and, in ten templates, its own copy
    of the episode mechanic (`alert-episodes`).
- What is missing is the loop: nothing turns those readings into one incident with an owner, tells
  the right person once, starts a repair, and closes the incident when the reading is good again.
  An organization's machines (a client's) report to nobody at PassionCode at all.

## The loop (AH-1 to AH-8)

| Step | What holds | Owner |
|---|---|---|
| AH-1 One health report | Every product and agent answers `agent-health/1`: `{schema, product, version, at, state, checks: [{id, area, state, count, fix}]}`, `state` one of `ok`, `warn`, `fail`, `unknown` (unknown is never ok), `area` one of `lifecycle`, `sessions`, `accounts`, `crash`, `errors`, `update`, `environment`, `security`, `connection`, `fix.tool` the MCP tool (or `fix.capability`, the board capability) that repairs it. Counts, kinds and codes only (DEC-0157): no name, path, account, text. Served as MCP `<product>_health` and CLI `health --json`. | each product; schema in fabric-agent-contract |
| AH-2 One watcher per machine | The machine's watcher reads every enrolled service's health every 5 minutes, plus the machine's own (Observatory findings, the launcher's doctor once a day), and keeps episodes with the `alert-episodes` mechanic: open after 2 bad readings, close after 2 good, one message per episode, unknown never closes one, a blind watcher opens its own episode. | Fabric Dashboards (it already enumerates every service and runs `audit`) |
| AH-3 Who it belongs to | An episode carries the machine, the organization of the service (Dashboards' `organization`, Switchboard's group context) and the project that owns the code (Observatory `project.resolve`). | Dashboards, Observatory |
| AH-4 Who hears | R33 O9: an episode of the operator's own estate goes to the operator through `communicator_notify` (one stable `eventId` per episode); an organization's episode goes to that organization's administrator through its own channel; PassionCode receives every organization's episode as counts (product, version, area, check id, state, duration), never content, so the estate sees a client's trouble without seeing the client's data. Quiet hours defer, never drop; urgent (`fail` of lifecycle, crash or security) bypasses. | Dashboards (local), the organization's administrator agent, PassionCode's cloud (Vesta) for the cross-organization count |
| AH-5 Repair agents | An episode whose check names `fix.capability` becomes one board request (`com.submit`, `kind: request`, the owning project, that capability, the episode id as idempotency key, the evidence as counts). The project's dispatch (Fabric's dispatcher, Switchboard `dispatch` with its time limit) starts a repair session on the project's group account; the session works through `task-pipeline` in the owning repository, ships under the project's release rules and answers the request with the release. An episode with only `fix.tool` is acted on by the watcher's agent itself when the tool is not a person's gate (closing a duplicate's quiet copy, a cache prune); otherwise it names the gate once. | Fabric (dispatch), Switchboard (runs, limits, accounts), each project (its capability) |
| AH-6 Proof of repair | An episode closes only after 2 good readings; its close message names the release or action that fixed it. A repair whose readings stay bad reopens the request once, then escalates to the person. | Dashboards, Fabric |
| AH-7 Daily estate checks | Once a day per machine: updates (every product's `update_available`, `sshlg-skills update`, the launcher's version), environment (`passioncode doctor`, disk, memory, swap, Colima and VM disks), security (leaked and unrotated keys, keys expiring in 14 days, OSV advisories marked known-exploited, ssh keys without a passphrase or readable by others, a firewall off). Each is a check of the machine's `agent-health/1` and goes through the same episodes. | Observatory (the reads), Dashboards (the episodes) |
| AH-8 Agents survive moves | Lifecycle checks every product reports: an agent session ended before it finished, a session stuck, a session duplicated, an account moved without its sign-in, a launch whose end nobody recorded. They are the operator's first rule («агенты … запускались, можно было их переносить между машинами безболезненно, не кикались, не сбивались»): each is a `fail` of `lifecycle` or `sessions` and starts AH-5 at once. | Switchboard, Dashboards, Fabric |

## What each project does

| Project | First change | Board |
|---|---|---|
| fabric-agent-contract | `agent-health/1` as a schema and decision (the shape above, its privacy rule under DEC-0157, its relation to DEC-0153 digests: a digest says what happened in an hour, health says what is wrong now) | request |
| fabric-switchboard | `switchboard health` and MCP `switchboard_health` (done here, 0.7.102); `fix.capability` for its checks once the contract names capabilities; dispatch of repair runs exists (`switchboard dispatch`) | this repository, SB-393 |
| fabric-dashboards | the watcher: read every service's health, keep episodes (`alert-episodes`), notify (communicator, the organization's channel), open board requests for `fix.capability` | request |
| fabric | the board side: an `incident.fix` request kind or capability convention, dispatch of the owning project's repair session, the close receipt | request |
| project-observatory | the machine's own checks as `agent-health/1` (disk, memory, VM disks, leaked and expiring keys, OSV, ssh keys, doctor once a day) and the history of episodes beside `ops_digest_record` | request |
| passioncode-vesta | the cross-organization count of episodes (AH-4), per organization and product, counts only, for PassionCode's support | request |
| passioncode (launcher) | `doctor --json` carries `agent-health/1` so a person's machine reports the same way | request |
| fabric-agents-database | the base kit's agents answer `agent-health/1` from their existing `health` door; `alert-episodes` names the watcher variant | request |
| fabric-workspace | the decisions below into `knowledge/` (rules, lifecycle), the roadmap track | request |
| every agent project | answer `agent-health/1` (a few lines over what its `health` already reads) and declare its `fix` capability | announcement |

## Decisions proposed

- **AH-D1.** One health shape for every product and agent, `agent-health/1`; unknown is never ok.
- **AH-D2.** One watcher per machine (Fabric Dashboards) keeps the episodes; products report, they do
  not alert on their own (a product's own Telegram alerts move behind the watcher).
- **AH-D3.** Messages follow R33 O9: the operator through the communicator, an organization's
  administrator through its channel, PassionCode only counts across organizations.
- **AH-D4.** A repair is a board request to the owning project, run as a dispatched session under
  `task-pipeline`, closed by good readings, never by the repairer's word.

## Switchboard's part, shipped with this proposal

`switchboard health` / MCP `switchboard_health` (`crates/switchboard-runtime/src/health.rs`):
checks `owner`, `sign_in`, `accounts`, `sessions_stuck`, `sessions_duplicate`, `sessions_gone`,
`crashes`, `errors_today`; tests `health::tests::a_quiet_machine_is_ok_and_unknown_errors_are_never_ok`
and `needs_attention_crashes_and_an_error_storm_set_the_worst_state` (a planted label, folder and
account id never leave). The session duty officer's wake-up floor (SB-392 S-1) closes one way an
agent was ended before it finished.
