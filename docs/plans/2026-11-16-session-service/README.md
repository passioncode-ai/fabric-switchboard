# Fabric Sessions: one always-on service that owns coding sessions, apart from every app (2026-11-16)

Spec for the operator's request of 2026-11-16 evening. Board row: [SB-397](../../evidence/backlog.md).
Evidence: [Switchboard inventory](research-switchboard.md), [estate research](research-estate.md)
(Fabric Dashboards, Fabric, the lifecycle broker, the launcher, the contracts), the
[session lifecycle audit](../2026-11-16-session-lifecycle/audit.md). The decomposition and the waves
are in [plan.md](plan.md). Every claim about today's code below cites those files, which cite
`file:line`.

## What was asked (the operator's words)

> если мы перезапускаемся, перезапускаем обновление, там проект перезапускается, то мы не должны
> ранить лайфсайкл сессий. Нужно, чтобы сервис, который отличается за лайфсайкл сессий, он отдельно
> жил от софта после запуска. чтобы он ставился отдельно. Наверное, есть смысл, может быть, даже
> вынести его в отдельный репозиторий, чтобы он запускался независимо от того, какой софт будет
> первым установлен. И там, допустим, может пользователь только поставить Fabric и он тоже поставит
> LCCL. Сессии будет через него.

> еще очу чтобы в этом модуле можно было перезапусти ть все работающие сессии, типо например для
> перезагрузки скилов, и чтобы жто можно было сделать через MCP агенту но он должен понимать что все
> сессии перезагрузятся и мониторинг долен следить чтобы все поднялись назад ип родолдили работу

> рефакторим исследуем составляем план пишем спеку декомпозируем еще раз там проверяем спеку и
> планом 2-3 раза на точность а потом мы волнами сюда доводим до конца до идеального результата

The organization's lifecycle contract already asks for it: SL-6 (a session does not depend on the
app that showed it) and SL-7 (everything that watches sessions runs under a supervisor) in
fabric-workspace `knowledge/lifecycle.md` ([estate research](research-estate.md) section 4), and
Switchboard's own [plan of 2026-11-13](../2026-11-13-session-lifecycle.md).

## Where we stand (measured 2026-11-16)

What dies or stops when an app quits or updates today:

| App | Session part | On quit or update | Evidence |
|---|---|---|---|
| Fabric Dashboards | ordinary console sessions (PTY children of Electron), limit-stop resumes, idle and stuck close, recovery and hand-over taking | the sessions die (`endAllForQuit`); the watchers stop; an auto-update is held while any console runs | [estate](research-estate.md) 1 |
| Fabric | terminals (node-pty children), the board dispatcher's watchdog and sweep | terminals die on a real quit, a broker stop, logout or an update; runs it handed to Switchboard live on | [estate](research-estate.md) 1 |
| Fabric Switchboard | the duty officer, the session watch, limit-stop registration (`StopFailure` hook), claim and finish, hook events to launch records, the managed proxy | the duty officer is cut mid-act by the 8 s drain; registrations and hook events in the gap are lost; managed requests get 503 | [inventory](research-switchboard.md) |
| Fabric Switchboard | session hosts (`switchboard session-host`, `setsid`), Terminal sessions, dispatched runs, monitor hook files, data files | survive; but nothing supervises a host, its end record can be lost, and it runs from the app bundle's path | [inventory](research-switchboard.md) |

No product has an "install if missing, keep the newest" mechanism for a shared component; the
lifecycle broker is private, installed by hand and never updates itself ([estate](research-estate.md)
2, 3).

## Decisions

Each decision names the alternative it rejects. Open ones are marked and carry the default this plan
proceeds on.

- **D-1 One service, its own repository.** `passioncode-ai/fabric-sessions`, a product of the
  organization (AGPL-3.0-only OR the commercial licence, as Switchboard). Rejected: keeping it inside
  Switchboard (a Fabric-only machine would need Switchboard; the operator: «независимо от того,
  какой софт будет первым установлен»); putting it in Observatory's node (Observatory is optional on a
  customer's machine).
- **D-2 Rust, from Switchboard's code.** The session host, attach, monitors, the resumable ledger,
  the duty officer's observation and decisions, the session watch, the process scan, capacity and
  the session rows already exist in Rust and are account-free ([inventory](research-switchboard.md),
  "not tied to accounts"). They move into crates of the new repository; Switchboard depends on a
  tagged release of them. Rejected: a rewrite in TypeScript or Python (no signed single binary,
  LC-05).
- **D-3 Supervised by the OS, never by an app.** macOS: a LaunchAgent
  `ai.passioncode.fabric-sessions` (`RunAtLoad`, `KeepAlive`, `ThrottleInterval` 10). Linux: a systemd
  user unit. Windows: later (the session host is Unix-only today). One copy per user: a second copy
  exits 75 (contract "One copy", [estate](research-estate.md) 4). Rejected: enrolling it in the
  lifecycle broker (private; the broker supervises apps, it installs nothing).
- **D-4 Session hosts outlive the service.** A host stays its own `setsid` process with its socket
  and descriptor; the service adopts every live host at start (from the descriptors) and is their
  supervisor of record. Restarting or updating the service never ends a session.
- **D-5 Accounts stay in Switchboard.** The service holds no credential, token, proxy or account
  file. What it needs from accounts it asks of Switchboard through one interface (D-9), and works
  without it: on a machine with no Switchboard a session runs on the person's own login.
- **D-6 Install if missing, keep the newest.** Every product (Fabric, Fabric Dashboards, Fabric
  Switchboard, the launcher) bundles a signed `fabric-sessions` and runs
  `fabric-sessions install --if-newer` at its start: under one lock, into an immutable release folder
  (`releases/<version>-<digest>/`, the broker's layout), with a receipt, never a downgrade, then
  `launchctl bootstrap` or `kickstart`. The launcher's doctor checks it. Rejected: each product running
  its own copy (two duty officers, two ledgers).
- **D-7 Self-update without harm.** The service reads its own signed feed (minisign, as Switchboard's
  updater) and replaces its release; it exits 75 and launchd starts the new one. Hosts live on (D-4).
  A host keeps the release it started from until its session ends; the release folder stays until
  no host uses it (pruned after).
- **D-8 One protocol, written in the contract.** The session-host socket protocol (`protocol: 1`,
  Switchboard CONTRACTS "A session host") and the session row (SB-395, Switchboard CONTRACTS "Sessions
  page") become decisions of `fabric-agent-contract`, so Dashboards and Fabric attach to the same
  hosts and show the same rows. Asked of the contract on the board (wave W0).
- **D-9 Switchboard's interface for accounts.** The service asks Switchboard, when installed,
  three things: the launch environment of a folder (group, account, mode: Switchboard writes the
  script, as today), the readiness of a limit stop (which accounts may resume it and when), and a
  label for an account id. Without Switchboard the answers are: the person's own login, "resume
  when the provider's reset passes", no label.
- **D-10 Data moves once, read-only first.** At its first start the service imports Switchboard's
  `recovery.json`, `resumable.jsonl`, `monitors/`, `duty.jsonl`, `session-watch.json` and the launch
  records it needs (copies; Switchboard's files stay). From the switch-over release on Switchboard
  reads and writes them only through the service.
- **D-11 Every other app becomes a client.** Fabric Dashboards and Fabric start sessions as hosted
  sessions of the service and attach to them; their own idle, stuck and limit-resume logic moves to
  the service (one duty officer, one resume path). Their quit and update then end no session.
- **D-12 Hooks never need an app or a vault.** The plugin's hooks (`StopFailure`, `Stop`,
  `SessionStart`, `PreToolUse` monitors, `SessionEnd`) write to the service (a socket, or its spool
  folder when it is down); none opens Switchboard's store or vault.
- **D-13 Restart every running session, watched until each one works again.** The service restarts
  the sessions it knows (to load new skills, plugins, hooks or a new Claude Code) as one *restart
  wave*, from the CLI, MCP and the Sessions page:
  - **Two steps, so the caller knows what it does.** `sessions restart plan` (MCP
    `sessions_restart_plan`) answers every session the wave would restart, the ones it skips and
    why, which one is the caller's own session, and a `digest`; it restarts nothing.
    `sessions restart apply --digest <d>` (MCP `sessions_restart_apply`) starts the wave only for
    that exact plan (a plan that changed since answers `plan_stale`). The MCP tool's description and
    its answer say plainly that every listed session, the caller's own included, will end and be
    resumed, and that the caller's own turn ends when its turn comes.
  - **Same session, never a new one.** Each session ends at the end of its current turn (a session
    in the middle of a turn is waited for, at most `--turn-timeout`, default 15 min, then restarted
    anyway only with `--now`), and is resumed as itself (`claude --resume <id>`, D-2's Continue path)
    in its own config folder, with a prompt that says it was restarted to reload its tools and should
    continue where it left off. A hosted session restarts inside its host: the viewers stay attached.
  - **Waits survive.** A session holding a wake-up, a monitor or cron jobs (SB-396 wave C) gets them
    named in its resume prompt so it sets them up again; a session stopped on a limit is not
    restarted (its resume already starts the new tools); a duplicate copy is closed, not restarted.
  - **The caller goes last.** The session that asked is restarted after every other session of the
    wave came back, so it can read the wave's outcome when it resumes.
  - **Watched to the end.** The wave is a durable record (`restarts/<wave>.json`): per session
    `pending`, `ending`, `resuming`, `back` (its process holds the session again and a new turn
    started), `continuing` (that turn finished a tool call or wrote a reply) or `failed` with the
    reason. The duty officer reads it every pass; a session not `back` within 5 minutes is retried
    once, then marked `failed`, shown under *Needs attention* and sent to the operator
    (`communicator_notify`, one event per wave). `sessions restart status <wave>` (MCP
    `sessions_restart_status`) answers it at any time; the resumed caller reads it first. The wave
    survives a restart of the service itself (D-4).
- **Open O-1 (the operator).** The name. Default: *Fabric Sessions*, binary `fabric-sessions`. The
  operator wrote «LCCL»; the plan proceeds on the default and renames in one step if he chooses.
- **Open O-2 (the operator).** May the default group lend its last account with room to a group that
  has none (balancing keeps two today, LC-8 of the lifecycle spec)? Default: keep two; asked once.

## Requirements

| Id | What holds | Checked by |
|---|---|---|
| SS-1 | Quitting, updating or crashing Fabric, Fabric Dashboards or Fabric Switchboard ends no session and loses no lifecycle record | an end-to-end test: start a hosted session through each app, update and quit the app, the session keeps writing and its row reads `running` |
| SS-2 | The service restarts or updates without ending a session; it adopts every live host within 5 s of its start | test: kill the service, start it, the host list matches the descriptors |
| SS-3 | One copy per user; a second exits 75; one lock for install | test with two starts |
| SS-4 | Whichever product starts first installs it; a newer bundled copy replaces an older install; never a downgrade; a product older than the installed service changes nothing | install tests over versions a < b, both orders |
| SS-5 | The service holds no credential and never opens Switchboard's vault | grep test on the crate graph, a planted-secret test on its files |
| SS-6 | Every session lifecycle rule of SB-396 (waves A to D) holds in the service: unknown commands never close, a limit stop or a session's own wait holds it, the floor is 65 minutes, Continue resumes the same session | the moved tests, run in the new repository |
| SS-7 | A limit stop registered while no app runs is kept and resumed (the hook writes to the service or its spool) | test: register with Switchboard down |
| SS-8 | Fabric Dashboards and Fabric show the same rows (the contract's session row) and attach to the same hosts | the contract's vectors in each product's gate |
| SS-9 | Without Switchboard the service still hosts, watches, closes and resumes sessions on the person's own login | a test machine profile with no Switchboard |
| SS-10 | The service reports `agent-health/1` (DEC-0179) and has a `fabric-service/0.1` descriptor | the contract validator |
| SS-11 | Uninstall leaves no session running unattended: it refuses while hosts run unless told to end them | test |
| SS-12 | A restart wave resumes every listed session as the same session id with its context, in its own config folder; none is replaced by a fresh one | test over hosted, Terminal and console sessions: the ids before and after match, the transcript continues |
| SS-13 | `restart apply` runs only the plan whose digest it names; the plan names the caller's own session; the caller is restarted last | MCP tests: stale digest refused, order recorded |
| SS-14 | Every session of a wave ends `continuing` or `failed` with a reason within 20 minutes; a failure is retried once, shown under Needs attention and notified once | a fake-clock test of the wave watcher; a live wave on the MacBook at W6 |
| SS-15 | A session's own waits (wake-up, monitor, cron) and limit stops are kept across a restart wave | test: the resume prompt names them; a limit-stopped session is skipped |

## What does not change

Accounts, groups, organizations, the proxy, balancing, spending ceilings and every account act stay
in Switchboard. Fabric's board, tasks and dispatcher policy stay in Fabric; Dashboards' views stay in
Dashboards. Agents keep calling Switchboard's MCP tools for sessions until W5; from then the session
tools are the service's, and Switchboard's answer through it for one minor version (LC-14).

## Risks

- **R-1 Two owners during the switch-over.** Mitigation: Switchboard's duty officer and watch stop
  when it sees the service answer (a capability flag), never both act (W3 exit check).
- **R-2 A host started from an old release.** Mitigation: D-7 keeps a release until no host uses it;
  the host protocol stays backwards compatible (`protocol` field).
- **R-3 Install races between products starting at login.** Mitigation: one lock file, the receipt
  read under it, idempotent `bootstrap`.
- **R-4 Signing and notarization.** The release pipeline reuses Switchboard's (minisign feed, Apple
  notarization); a new repository needs its own secrets in CI, a person's step only if the Apple
  account needs a new identifier (named when met, not assumed).
