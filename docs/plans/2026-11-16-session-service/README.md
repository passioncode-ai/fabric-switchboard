# Fabric Sessions: one always-on service that owns coding sessions, apart from every app (2026-11-16)

Spec, draft 2 (after verification round 1: [findings](verification.md#round-1)). Board row:
[SB-397](../../evidence/backlog.md). Evidence: [Switchboard inventory](research-switchboard.md)
(read at `af209503`; wave D merged since as `d2cfd0d2`, so a task re-reads its `file:line` before it
starts), [estate research](research-estate.md) (Fabric Dashboards, Fabric, the lifecycle broker,
the launcher, the contracts), the [session lifecycle audit](../2026-11-16-session-lifecycle/audit.md).
The decomposition is [plan.md](plan.md).

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

The organization's lifecycle contract asks for the same: SL-6 (a session does not depend on the app
that showed it) and SL-7 (everything that watches sessions runs under a supervisor), fabric-workspace
`knowledge/lifecycle.md` ([estate](research-estate.md) 4).

## Where we stand (measured 2026-11-16)

| App | Session part | On quit or update | Evidence |
|---|---|---|---|
| Fabric Dashboards | console sessions (PTY children of Electron), limit-stop resumes, idle (30 min) and stuck close, recovery and hand-over taking | the sessions die (`endAllForQuit`); the watchers stop; an auto-update is held while a console runs | [estate](research-estate.md) 1 |
| Fabric | terminals (node-pty children), the board dispatcher's watchdog and sweep | terminals die on a quit, a broker stop, logout or an update | [estate](research-estate.md) 1 |
| Fabric Switchboard | the duty officer, the session watch, limit-stop registration, claim and finish, hook events to launch records | the duty officer can be cut mid-act (a close takes up to 10.5 s, the drain is 8 s, the hard exit 10 s); registrations and hook events in the gap are lost (fixed for limit stops by W0.4) | [inventory](research-switchboard.md) 0, 2 |
| Fabric Switchboard | the managed proxy | managed requests get 503 during an update drain and fail while no owner runs; the session process lives on | [inventory](research-switchboard.md) 2 |
| Fabric Switchboard | session hosts (`setsid`), Terminal sessions, dispatched runs, monitor files, data files | survive; nothing supervises a host, its end record can be lost, it runs from the app bundle's path | [inventory](research-switchboard.md) 1.1, 2 |

No product installs a shared component "if missing"; the lifecycle broker is private, installed by
hand, and never updates itself ([estate](research-estate.md) 2, 3).

## Decisions

Each names what it rejects. Open ones carry the default this plan proceeds on.

- **D-1 Its own product; its own repository at the end of W2.** Fabric Sessions has its own binary
  (`fabric-sessions`), bundle id `ai.passioncode.fabric-sessions`, release artefacts, feed and
  version from W1. Its code starts as two crates inside the Switchboard workspace
  (`fabric-sessions-core`, `fabric-sessions`), so a fix lands with one gate while the interfaces
  move; once the socket API v1 is frozen (W2 exit) the crates move with their history to
  `passioncode-ai/fabric-sessions` (AGPL-3.0-only OR the commercial licence) and Switchboard depends
  on its tag. Rejected: a new repository on day one (each fix would need a tag and a bump while the
  seams still move: round 1, design 17), and the service inside Switchboard for good (a Fabric-only
  machine would need Switchboard; the operator: «независимо от того, какой софт будет первым
  установлен»).
- **D-2 The core has no Switchboard dependency.** The modules that move are account-free in
  behaviour but today call into Switchboard: `launch` (the Terminal and script helpers), `recovery`
  (its storage), `launches`, `person`, `oplog`, `shell::map`, `agent_settings` and
  `switchboard-core` (`private_fs`, `Provider`) ([inventory](research-switchboard.md) 3; round 1,
  accuracy 4). W1 splits each seam: the storage halves and helpers move into the core, the account
  halves stay, and the core reaches Switchboard only through the `Accounts` trait (D-9). Checked by
  `cargo tree -p fabric-sessions-core` naming no `switchboard-*` crate.
- **D-3 Supervised by the OS.** macOS: a LaunchAgent `ai.passioncode.fabric-sessions` in
  `~/Library/LaunchAgents/` whose `ProgramArguments[0]` is the stable link
  `~/Library/Application Support/ai.passioncode.fabric-sessions/current/fabric-sessions` (LC-05),
  `RunAtLoad`, `KeepAlive`, `ThrottleInterval` 10, `ExitTimeOut` 30, `AssociatedBundleIdentifiers`
  the products that installed it; `launchctl bootstrap gui/<uid>`. Linux: a systemd user unit. A
  Mac Studio space: a LaunchDaemon with `UserName` the space's account, through Observatory's space
  validator, one per account. One copy per user: a second exits 75. Windows: no service; Switchboard
  runs the core in-process as today (SS-1, SS-2, SS-7 and SS-12 are not claimed on Windows; row
  filed). Rejected: the lifecycle broker (private, installs nothing), `SMAppService` (the plist must
  live in one app's bundle).
- **D-4 Hosts outlive the service, and the service is not needed to start one.** A host stays its
  own `setsid` process with its socket and descriptor. A client starts a host directly with the
  installed binary (`fabric-sessions host`); the service adopts it on its next pass, and at its own
  start adopts every live host (Switchboard's `switchboard session-host` ones too, by the socket path
  in each descriptor, not a recomputed digest). The service being down costs watching, never
  starting. Host sockets move from `/tmp` to a per-user runtime folder (`$TMPDIR` on macOS,
  `$XDG_RUNTIME_DIR` on Linux) that the periodic `/tmp` cleanup does not touch.
- **D-5 Accounts and the proxy stay in Switchboard.** The service holds no credential, token, proxy
  or account file and never opens Switchboard's vault (SS-5).
- **D-6 Switchboard is one of three states, and a session never moves account silently.** The
  service records Switchboard as *absent* (not installed), *reachable* (answers `hello`) or
  *unreachable* (installed, quitting, updating or crashed). A session that runs on a Switchboard
  account (a managed group folder or an isolated home) is resumed only while Switchboard is
  reachable; while it is unreachable the resume waits and its row says so. Only with Switchboard
  absent does a session resume on the person's own login (round 1, design 1).
- **D-7 The interface to Switchboard (`Accounts`).** Four questions, each with its answer without
  Switchboard: (1) the launch environment of a resume: Switchboard writes a 0600 script with the
  exports the session needs (a fresh token for an isolated home, the proxy route for a managed one)
  and answers its path; none: the person's own environment. (2) Readiness of a limit stop: which
  accounts may resume it and when to look again; none: at the provider's reset time. (3) A label for
  an account id; none: no label. (4) The config folders to scan for sessions (`homes/*`,
  `runtimes/*`, `shell/*`); none: `~/.claude` and `~/.codex`. Starting a new session on an account
  (launch, dispatch) stays in Switchboard, which prepares the environment and asks the service to
  spawn it, so the service is the one parent and reaper of every process it supervises (round 1,
  design 14).
- **D-8 One protocol, written in the contract.** The socket API, the host protocol (version 2 adds
  `RESTART`: the host keeps its socket and viewers and spawns the resumed child on the same PTY),
  the session row (SB-395) and the restart wave record become decisions of `fabric-agent-contract`,
  with vectors every product's gate runs.
- **D-9 One writer per file per phase.** Table "Data ownership" below. The service imports
  Switchboard's session files when Switchboard hands over (it writes `sessions-handed-over.json`),
  mirrors waits and live stops back into Switchboard's folder while any host started before the
  hand-over lives (those hosts read the old paths), and exports everything back when it is
  uninstalled. Rejected: a one-time copy at first start (hosts and hooks of the old releases go on
  writing the old files; round 1, readiness 8, design 2).
- **D-10 Install if missing or too old; the service updates itself.** A product bundles a signed
  `fabric-sessions` (macOS: `Contents/Helpers/`, not re-signed) and at its start runs the installed
  one if any, else the bundled one: `install --if-needed --min-api N`. It installs only when the
  service is missing or older than the socket API the product needs, under one lock, into
  `releases/<version>-<digest12>/` with a receipt (`schema: 1`, `clients[]`), verifying the
  candidate's signature (team `KJ35UYYL22`, identifier `ai.passioncode.fabric-sessions`) before
  running it. Never a downgrade. Everything else comes from the service's own signed feed (minisign,
  checked every 5 min, `auto-update` switch). A new release becomes `current` only after it answers
  `hello` as itself within 30 s; otherwise the previous one is restored. A release stays while a live
  host uses it. The person's off switch wins: `off` in the data folder, or the job disabled in
  Background Items, makes every install answer `held_off`. The last product's uninstall runs the
  service's uninstall, which refuses while hosts run (SS-11).
- **D-11 Every other app becomes a client.** Fabric Dashboards and Fabric start sessions as hosts and
  attach to them; their idle, stuck and limit-resume logic is the service's (one duty officer, one
  resume path, one idle time: the service's 65 minutes, stated in each product's `AGENTS.md`).
- **D-12 Hooks belong to the service's own plugin.** A Claude Code plugin `fabric-sessions` (hooks and
  the MCP server), installed by the launcher and by the service's install, calls the stable entry
  `~/.local/bin/fabric-sessions` (a link to `current`) for `StopFailure`, `Stop`, `SessionStart`,
  `UserPromptSubmit`, `PreToolUse` (waits) and `SessionEnd`. The hook writes to the socket, or to the
  spool when the service is down, never opens a vault and exits 0 within 1 s. Switchboard's plugin
  keeps only its own `SessionStart` folder step (`project suggest`) and drops the rest in the same
  release, so no event registers twice. A limit stop's account attribution is forwarded to
  Switchboard with the config folder, launch id, folder and time, so it can be attributed later.
- **D-13 Two owners never act.** Switchboard asks `hello` at start and every pass; it stops its duty
  officer and watch while the service's `owns` lists them and takes them back only after three failed
  passes and launchd reporting the job unloaded or disabled. The service holds its own duty officer,
  watch and idle close while a Switchboard older than the switch-over release owns the store
  (`held: switchboard_owner_without_handshake`, shown in rows and health) (round 1, readiness 5, 6).
- **D-14 Restart every running session, watched until each one works again.** Below.
- **Open O-1 (the operator).** The name. Default *Fabric Sessions*; frozen on the day W1 starts.
- **Open O-2 (the operator).** May the default group lend its last account with room (balancing keeps
  two today)? Default: keep two.

### D-14 Restart waves

What it is for: a new skill, plugin, hook, MCP server or Claude Code binary reaches running sessions
only when they restart. W5.0 first measures which of these the pinned Claude Code picks up without a
restart; the wave covers the rest.

- **Two calls, so the caller knows what it does.** `sessions restart plan` (MCP
  `sessions_restart_plan`) answers every session the wave would restart, every session it skips and
  why, the caller's own session, the preflight result of each, and a `digest` (sha256 over the sorted
  `(session, kind, action, skip_reason)` and the caller; turn state and times excluded). It restarts
  nothing. `sessions restart apply --digest <d> --reason <text>` (MCP `sessions_restart_apply`) starts
  the wave for that exact plan (`plan_stale` otherwise). Both tool descriptions and the plan's answer
  say in one sentence: every listed session ends and is resumed as itself, the caller's own last, and
  the caller's turn ends when its turn comes.
- **Who may.** Any agent of the person may restart every session on this machine (the operator's
  words above). Guards: a reason is required and recorded; one live wave at a time
  (`wave_running`); at most one wave per 30 minutes (`too_soon`); a session the person typed into in
  the last two minutes is skipped (`in_use_by_person`); the start, not only a failure, is announced;
  `sessions restart cancel <wave>` leaves every session still `pending` as it is.
- **Never in the middle of work.** A session restarts only between turns: at once if it is idle, else
  at the end of its current turn and after its own background commands and subagents finish. Nothing
  is ended mid-tool. A session busy for a long time stays `pending` and restarts when it finishes; the
  plan names it. Turn state comes from `UserPromptSubmit` and `Stop` hooks and the session file's
  `status`, never from a transcript's content.
- **Preflight before any end.** For each session: Switchboard reachable when the session runs on its
  accounts (D-6), its resume environment prepared and its token alive, admission and the machine cap
  allow a start, the transcript exists, no limit stop is live. A session failing any check is skipped
  with that reason; it is never ended.
- **Same session, same mode.** Resumed with `claude --resume <id>` in its own config folder and
  working folder, with the permission flags its process had (read from its command line), keeping
  `SWITCHBOARD_LAUNCH_ID` so its launch record continues. A hosted session restarts inside its host
  (protocol 2: viewers stay attached); a protocol-1 host is ended and replaced by a new host (viewers
  reattach). A Terminal session comes back in a new Terminal window; the old tab is left with the
  ended process (stated in the plan).
- **What it is told.** A session that was waiting for the person resumes with no prompt (no work it
  was not asked for). A session that held waits resumes with a prompt naming them (a wake-up due at
  T, a monitor, N cron jobs), copied into the wave record before the end because `SessionEnd` deletes
  them, so it sets them up again. A session that was working resumes with a prompt that says it was
  restarted at a turn boundary to load new tools and should continue its task; it calls
  `sessions_restart_status <wave>` first.
- **Paced.** At most three sessions are `ending` or `resuming` at once, each through admission (LC-17)
  and the capacity cap. A wave's acts do not count toward the duty officer's restart budget, and while
  a session is in a wave the duty officer, the duplicate close, the host idle close and recovery
  claims leave it alone. A session that registers a limit stop after the plan becomes
  `skipped: limit` (recovery alone resumes it); a resume that hits a limit is `failed: limit` and a
  stop. A duplicate copy is skipped and reported, not closed by the wave.
- **Watched to the end.** The wave is a durable record (`restarts/<wave>.json`, kept 30 days, at most
  200): per session `pending`, `ending`, `resuming`, `back` (a process started after the end holds the
  session id), `continuing` (a `Stop` turn event, or the transcript written after the resume) or
  `failed` with a reason. The duty officer reads it every pass. A session not `back` 5 minutes after
  its end is retried once, only when no live process holds its id; not `back` 5 minutes later, or not
  `continuing` 10 minutes after `back` when it was given a prompt, it is `failed`, shown under
  *Needs attention*, and the wave's outcome is posted once. The wave survives a restart of the service.
- **The caller goes last** and reads the wave's outcome when it resumes.
- **Scope.** Claude Code sessions of this user on this machine. Skipped with a reason: Codex
  sessions (own spec), dispatched `-p` runs (they end by their deadline), sessions in a config folder
  the service does not know (`unknown_home`), Dashboards and Fabric consoles until W6
  (`console_not_hosted`). Another machine runs its own wave through its own service.
- **Where the outcome goes.** The service's event feed (`fabric-service/0.1`) and its health reading;
  on the operator's machine the communicator subscribes and posts it to him once per wave.

## Data ownership

| File | Phase 0 (today) | W2 to hand-over | After hand-over | Uninstalled |
|---|---|---|---|---|
| `sessions/` host descriptors | SB hosts | SB and FS hosts each their own | FS hosts; SB hosts until they end | exported to SB |
| `monitors/` | SB hook | SB hook | FS hook; mirrored to SB while old hosts live | exported |
| `recovery.json` stops | SB | SB | FS (stops), SB answers readiness | exported |
| `resumable.jsonl` | SB owner and hosts (lock, W0.2) | same | FS; old hosts append to SB's, re-imported by id | exported |
| `duty.jsonl`, `duty.json`, `duty/` | SB | SB | FS | exported |
| `session-watch.json`, `capacity.json` | SB | SB | FS | exported |
| `launches/` | SB | SB | SB writes a launch; FS writes its lifecycle events through SB's API | n/a |
| `dispatches.json` | SB | SB | FS (spawned and supervised by FS) | exported |
| `stop-spool/` (W0.4) | SB hook | SB hook | FS `spool/` (one format, W2.4) | n/a |
| `restarts/` | none | none | FS | exported |
| Dashboards' idle consoles and claims, Fabric's `board-dispatches.json` | FD, FB | FD, FB | imported by FS at W6 | n/a |

## Requirements

| Id | What holds | Checked by |
|---|---|---|
| SS-1 | Quitting, updating or crashing Fabric, Fabric Dashboards or Switchboard ends no session process and loses no lifecycle record. A managed session's requests fail while Switchboard's proxy is down (503 during an update drain); its row says `proxy_down` | per app: start a hosted session, update and quit the app, the process lives and every record is there |
| SS-2 | The service restarts or updates without ending a session; it adopts every live host within 5 s of its start | kill the service, start it: the host list matches the descriptors |
| SS-3 | One copy per user; a second exits 75; one install lock | two starts; two installs at once |
| SS-4 | Whichever product starts first installs it; an older bundled copy never replaces a newer install; the person's off switch wins | install tests over versions a < b both orders, `held_off`, Background Items disabled |
| SS-5 | The service holds no credential and never opens Switchboard's vault | `cargo tree` names no store, vault or keyring crate; a planted-secret scan of its data after a test run |
| SS-6 | Every rule of SB-396 holds in the service: an unread process table never closes, a limit stop or a session's own wait holds it, the floor is 65 minutes, Continue resumes the same session | the moved tests under the same names |
| SS-7 | A limit stop reported while no app and no service runs is kept and resumed | report with everything down, start the service: the stop is registered |
| SS-8 | Fabric Dashboards, Fabric and Switchboard show the same rows and attach to the same hosts | the contract vectors in each product's gate |
| SS-9 | With Switchboard absent the service hosts, watches, closes and resumes sessions on the person's own login; with Switchboard unreachable it resumes none of Switchboard's sessions | a profile without Switchboard; a profile with Switchboard stopped |
| SS-10 | The service reports `agent-health/1`, writes the operations digest (DEC-0153) and has a `fabric-service/0.1` descriptor | the contract validator |
| SS-11 | Uninstall leaves no session unattended (refuses while hosts run unless told to end them) and gives the data back | install, register a stop, uninstall: Switchboard resumes the stop |
| SS-12 | A restart wave resumes each listed session as the same session id, in its own folders and permission mode | hosted and Terminal sessions: ids, folders and flags match; the transcript continues |
| SS-13 | `apply` runs only the plan whose digest it names; the plan names the caller, who restarts last; a session is never ended mid-turn or before its preflight passed | MCP tests: stale digest, order, a busy session stays `pending`, a failed preflight is skipped |
| SS-14 | Each session of a wave is `continuing` or `failed` with a reason within 20 minutes of its end; a failure is retried once only with no live copy, shown under Needs attention and posted once | fake-clock tests; a live wave on the MacBook at W5 |
| SS-15 | A session's own waits and limit stops survive a wave: the waits are named in its resume prompt, a limit-stopped session is skipped | tests over the wave record and the prompt |

## Contract rules this service meets

LC-01: the drain ends within 5 s, hard exit at 10 s, never ends a host (a documented deviation: hosts
are not children). LC-02: hosts are `setsid` with ppid 1 by design; each has a descriptor and the
service is its supervisor of record (deviation, documented). LC-03: a duty act is journaled before it
acts (W0.1). LC-04, LC-21: no secret on argv; an environment script is 0600, read once, removed by
Switchboard after the start. LC-05: D-3, D-10. LC-06 (deviation): it reads `~/.claude/sessions`,
transcript times and the config folders D-7 names. LC-07: Background Items read at each product start.
LC-08: idle CPU under 0.1 %, RSS at most 40 MB, a fake-clock hour test. LC-09: its `AGENTS.md`
lifecycle table; SB, FD and FB update theirs. LC-10: `fabric-sessions mcp` exits on end of input
within 1 s and answers `stale` after an update. LC-11: the socket path is stable across updates.
LC-12: logs (5 × 5 MB), the spool (5,000 files or 20 MB) and waves are bounded. LC-14: D-10 and SS-11.
LC-15: its build prunes. LC-16: D-10. LC-17: hosted starts and resumes go through admission. LC-18:
the idle time, 65 minutes. LC-22: waves and the spool. LC-19, LC-20, LC-23: not applicable (no
user-wide MCP server, no port, no browser). SL-1 the start record, SL-3 turn events, SL-4 end records,
SL-5 D-14's guards, SL-6 D-4, SL-7 D-3 (W0.7 amends SL-7's wording: launchd or systemd directly, or
the broker), SL-8 rows plus `unmanaged`.

## MCP tools

| Today (Switchboard) | From W5 (Fabric Sessions) |
|---|---|
| `switchboard_sessions` rows and tree | `sessions_rows`, `sessions_tree`; launch history stays in Switchboard |
| `switchboard_session_end` | `sessions_end` |
| `switchboard_session_continue` | `sessions_continue` |
| `switchboard_session_close_copy` | `sessions_close_copy` |
| `switchboard_duty`, `switchboard_capacity` | `sessions_duty`, `sessions_capacity` |
| none | `sessions_restart_plan`, `sessions_restart_apply`, `sessions_restart_status`, `sessions_restart_cancel` |
| `switchboard_provenance`, `switchboard_recovery` | stay in Switchboard (accounts), reading the service's records |

Switchboard's moved tools forward through the socket for one minor release, answering
`deprecated: "use sessions_*"`, then go (a Switchboard decision). The service's MCP server is
declared by its plugin (D-12).

## What does not change

Accounts, groups, organizations, the proxy, balancing, spending ceilings and every account act stay
in Switchboard. Fabric's board and dispatcher policy stay in Fabric; Dashboards' views stay in
Dashboards.

## Risks

- **R-1 Two owners during the switch-over.** D-13.
- **R-2 Hosts of an old release.** D-9 mirrors their files; protocol 1 hosts are replaced at their
  next restart; a release is kept while a host uses it.
- **R-3 Install races at login.** One lock, the receipt read under it, idempotent `bootstrap`.
- **R-4 Signing.** A bare Mach-O is notarized as a zip and cannot be stapled (Switchboard's CLI is the
  precedent); a new Apple identifier is a person's step only if Apple asks for one.
- **R-5 An older product that does not know the service** still runs its own watchers: D-13's hold.
- **R-6 A wave's cost.** Each prompted resume sends the session's whole context again (likely a cold
  cache); the plan shows the number of prompted resumes, and idle sessions get no prompt.
