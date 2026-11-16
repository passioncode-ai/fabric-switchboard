# Fabric Sessions: the plan in waves (2026-11-16)

The spec is [README.md](README.md); this file decomposes it. Each task has an id (`W<wave>.<n>`), the
repository that owns it, what it changes and the check that closes it. A wave ends when its exit
checks are green on the artefact the operator uses; the next wave starts then. Tasks owned by
another repository go there as a board request on the day the wave opens; their owners build them.

Repositories: **SB** fabric-switchboard, **FS** passioncode-ai/fabric-sessions (new), **CT**
fabric-agent-contract, **FW** fabric-workspace, **FD** fabric-dashboards, **FB** fabric (the app),
**LN** passioncode (the launcher), **OB** project-observatory, **AD** fabric-agents-database, **OI**
org-index.

## W0. Decisions, contract and the holes that hurt today

Nothing moves yet. The current code gets the fixes that protect sessions until the switch-over; the
contract and the organization learn the plan.

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W0.1 | SB | Duty officer: a close or restart in flight finishes or is journaled across the drain (finding 1: `duty.rs:664-699`, `:845-859`); the drain waits for the act, never cuts it between the end and the resume | a test that stops the owner mid-act: the journal holds the act, the next owner completes the resume |
| W0.2 | SB | `resumable::record` under a cross-process lock (finding 3: `resumable.rs:33-47`, writers `session_host.rs:478`, `duty.rs:643`) | a test with two writers in parallel: no line lost |
| W0.3 | SB | The session watch runs when the account snapshot fails (finding 5: `monitor.rs:362`) | test: a failed snapshot still ticks the watch |
| W0.4 | SB | A `StopFailure` registration made while no owner runs is spooled and taken by the next owner (finding 2: `lib.rs:5318-5324`) | test: register offline, start the owner, the stop is in `recovery.json` |
| W0.5 | SB | Backlog row SB-397 (finding 6) and rows for each W0 fix; this plan linked from HANDOFF | the rows exist under lease |
| W0.6 | CT | Decisions: the session-host protocol (`protocol: 1`), the session row (SB-395), the restart wave record (D-13), the `fabric-sessions` service id and its health | a CT decision with vectors; FS and SB gates run them |
| W0.7 | FW | `knowledge/lifecycle.md` SL-6, SL-7 name Fabric Sessions as the owner; `products.md` gains the product; roadmap RM-27 and the unified plan's 2.8 point here | the pages merged |
| W0.8 | OI, AD | The repository row in `repositories.json`; the architect's template for the service (agent-host, variant from host-operations' lifecycle-broker: immutable releases, receipt, exit 75, leases) recorded | the rows merged |
| W0.9 | SB | Ask the operator O-1 (name) and O-2 (default group lending) once, through `fabric_question_ask` and the communicator; proceed on the defaults | the questions filed |

Exit: W0.1 to W0.4 shipped in a Switchboard release; W0.6 decided; the repository exists.

## W1. The repository and the shared core

The account-free code moves into FS as crates; Switchboard uses them from a tagged release and
behaves exactly as before.

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W1.1 | FS | Repository from the agent-host template: AGPL-3.0-only OR commercial, CLA, `scripts/check.sh`, nightly CI at 23:00 Europe/Warsaw, release workflow with minisign and notarization | the gate runs green on an empty crate |
| W1.2 | FS | Crate `fabric-sessions-core`: `session_host` and the attach protocol, `machine_sessions`, `capacity`, `monitors`, `resumable`, the account-free part of `launches` (lifecycle events, pruning), `session_rows`, the duty officer's observation and decisions (`duty` without the resume environment), `session_watch` | the moved tests pass in FS (same names) |
| W1.3 | FS | A trait `Accounts` for D-9 (launch environment, readiness, labels) with a no-Switchboard implementation (the person's own login, reset-time readiness, no labels) | unit tests of the default answers |
| W1.4 | SB | Switchboard depends on `fabric-sessions-core` at a tag; its own copies are deleted and the modules re-exported; `Accounts` implemented over the store | `./scripts/check.sh` green; no behaviour change (the same CLI and MCP answers on the fixtures) |
| W1.5 | SB, FS | Data layout stays where it is for now (`<switchboard data>/…`); the core takes the root as a parameter | grep: no path constant left in the core |

Exit: Switchboard released on the shared core with no visible change; FS gate green.

## W2. The service runs on its own

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W2.1 | FS | Binary `fabric-sessions` with `serve`: one copy per user (lock, a second exits 75), drain on SIGTERM that never ends a session, `agent-health/1` report, `fabric-service/0.1` descriptor | SS-3, SS-10 tests |
| W2.2 | FS | Its data folder (`~/Library/Application Support/ai.passioncode.fabric-sessions/`, Linux XDG), the one-time import of Switchboard's session files (D-10) with a receipt | test: import once, idempotent, Switchboard's files untouched |
| W2.3 | FS | Adopt live hosts at start from their descriptors; supervise them (a host that died without its end record gets one) | SS-2 test |
| W2.4 | FS | Local socket API (owner-only, 0600) for: rows, attach, end, continue, hook events, host start; a spool folder written when the service is down and read at start | tests over the socket and the spool |
| W2.5 | FS | `fabric-sessions hook` for every plugin hook event: writes to the socket or the spool, never opens a vault, always exits 0 within 1 s | SS-7; a timing test |
| W2.6 | FS | The duty officer, the watch and the monitors run in the service; Continue and the idle close too (SS-6) | the SB-396 tests in FS; a fake-clock hour inside the idle budget |
| W2.7 | SB | Switchboard sees the service (socket answers, capability flag) and stops its own duty officer and watch; its session MCP tools and CLI forward to the service; without the service it acts as today | SS-1 for Switchboard; R-1 test: never two duty officers |
| W2.8 | SB | The plugin's hooks call `fabric-sessions hook` when it is installed, `switchboard` otherwise | hooks test with and without the binary |

Exit: on the MacBook the service runs under a LaunchAgent installed by hand from a build; quitting
and updating Switchboard ends no session and loses no record (SS-1 for Switchboard).

## W3. Accounts through one interface

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W3.1 | SB | `switchboard sessions env --folder F | --session S` answers the launch environment for a resume: the group, the account, the mode and a script path Switchboard prepared (a fresh token for an isolated home, the proxy route for a managed one); the service runs that script and never reads `launch.command` or a home's `settings.json` itself (research 3, interface 2) | test: an isolated home resumed by the service gets a fresh token |
| W3.2 | SB | Readiness: `switchboard recovery readiness --stop E` answers which accounts may resume a stop and when to look again; the service keeps the stops and asks | the recovery tests split across the interface |
| W3.3 | FS, SB | Recovery stops move into the service (`recovery.json` owner); `StopFailure` attribution is forwarded to Switchboard when it runs | SS-7 with Switchboard up and down |
| W3.4 | SB, FS | Launch and dispatch keep choosing the account and starting the process in Switchboard, then hand pid, launch id and deadline to the service, which supervises, reaps and stops it | dispatch tests: stop and deadline enforced by the service |
| W3.5 | FS | Labels from Switchboard for rows; ids only without it | row test |

Exit: SS-9 on a profile without Switchboard; every recovery and dispatch test green across the two
repositories.

## W4. Install, update and uninstall

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W4.1 | FS | `fabric-sessions install --if-newer <bundled binary>`: one lock, `releases/<version>-<digest>/`, a receipt, never a downgrade, `launchctl bootstrap` or `kickstart` of `ai.passioncode.fabric-sessions` (`RunAtLoad`, `KeepAlive`, `ThrottleInterval` 10); Linux a systemd user unit | SS-3, SS-4 tests (a < b both orders, two products at once) |
| W4.2 | FS | Self-update from its signed feed every 5 min (DEC-0004 cadence), exit 75, a release kept while a host uses it (D-7) | test: update with a live host, the host lives |
| W4.3 | FS | `fabric-sessions uninstall`: refuses while hosts run unless `--end-sessions`; removes the agent, keeps data unless `--purge` | SS-11 |
| W4.4 | SB | The app bundles the signed binary and runs `install --if-newer` at start (and `switchboard uninstall` leaves the service alone unless asked) | app smoke on macOS: a fresh profile gets the service |
| W4.5 | FD, FB | The same bundling and start step in Fabric Dashboards and Fabric | each product's smoke |
| W4.6 | LN | The launcher installs it and its doctor checks it (`fabric-sessions` present, version, health) | doctor output |
| W4.7 | OB | Mac Studio spaces: the service as a LaunchDaemon per space through Observatory's space validator, like `serve` | the space's health reads `ok` |

Exit: on a clean macOS user, installing any one product leaves the service running; on the MacBook
and the Studio the installed service updates itself with sessions running.

## W5. Restart every running session (D-13)

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W5.1 | FS | The wave planner: every live session with its kind (hosted, Terminal, console, dispatched), turn state, waits, limit stop, duplicate; the caller's own session (from `CLAUDE_CODE_SESSION_ID` or the calling process's session); a digest | unit tests over fixtures |
| W5.2 | FS | The wave runner: end at turn end (or `--now`), resume as the same id with the restart prompt naming the session's waits; hosted sessions restart inside the host; the caller last | SS-12, SS-13, SS-15 |
| W5.3 | FS | The wave record and its watcher in the duty officer: `back` and `continuing` from the session file and the transcript's new turn; retry once at 5 min, then `failed`, Needs attention, one `communicator_notify` per wave | SS-14 on a fake clock |
| W5.4 | FS | CLI `sessions restart plan|apply|status` and MCP `sessions_restart_plan`, `sessions_restart_apply`, `sessions_restart_status`; the tool descriptions say every session, the caller's included, ends and resumes | MCP tests; the tools reference page |
| W5.5 | SB | Sessions page: *Restart all sessions* with the plan shown before the confirm, then the wave's progress per row | design pass over the plan and progress states; Copylot ids |

Exit: a live wave on the MacBook restarts every running session (this one included) and each one
comes back as itself and continues.

## W6. Every app is a client

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W6.1 | FD | Consoles start as hosted sessions of the service and attach; their own idle, stuck and limit-resume code is removed in favour of the service's; quit and update no longer end sessions or wait for consoles | SS-1 for Dashboards |
| W6.2 | FB | Terminals as hosted sessions of the service; the dispatcher's watchdog reads the service | SS-1 for Fabric |
| W6.3 | SB | The Sessions page reads the service's rows; Switchboard's forwarding session tools are kept one minor version, then removed (LC-14) | SS-8 vectors; the parity tables |
| W6.4 | CT | The session row and host protocol vectors run in FD, FB and SB gates | SS-8 |

Exit: SS-1 for all three apps.

## W7. Acceptance and release

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W7.1 | all | Live check on the MacBook and the Studio: quit and update each app with sessions running; kill and update the service; a restart wave; no session lost, every row honest | the evidence in `docs/evidence/` with commands and outputs |
| W7.2 | FS, SB | Releases; the HANDOFF entries; the board rows closed with receipts | release receipts |

## Order and parallel work

W0 first (the SB fixes and the contract can run side by side). W1 then W2 in order. W3 and W4 can run
in parallel after W2. W5 needs W2 (and W4 for the live wave). W6 needs W2 and W4, and runs in the
other products' repositories in parallel with W5.
