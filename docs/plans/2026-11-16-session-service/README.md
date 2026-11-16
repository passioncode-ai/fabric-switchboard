# Fabric Sessions: one always-on service that owns coding sessions, apart from every app (2026-11-16)

Spec, draft 3 (after verification rounds 1 and 2: [findings](verification.md)). Board row:
[SB-397](../../evidence/backlog.md). Evidence: [Switchboard inventory](research-switchboard.md)
(read at `af209503`; every task re-reads its `file:line` before it starts), [estate
research](research-estate.md), the [session lifecycle audit](../2026-11-16-session-lifecycle/audit.md).
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

## Two outcomes, in this order

1. **R1, restart waves, in the Switchboard that runs today.** An agent restarts every running Claude
   Code session through MCP, each comes back as itself, and Switchboard watches until every one is
   back. Everything a wave needs already lives in Switchboard: the duty officer stops and resumes a
   session as itself, the session watch runs every minute, the monitors and limit stops say which
   sessions must be left alone, and session hosts keep a session's terminal. R1 ships first (round 2,
   simplicity 1).
2. **The service.** The session lifecycle moves into Fabric Sessions, installed by whichever product
   starts first, so no app's quit or update ends a session or loses a record. R1's code moves with it.

## Where we stand (measured 2026-11-16)

| App | Session part | On quit or update | Evidence |
|---|---|---|---|
| Fabric Dashboards | plain console sessions (PTY children of Electron), limit-stop resumes, idle (30 min) and stuck close | the sessions die (`endAllForQuit`); the watchers stop; an auto-update is held while a console runs | [estate](research-estate.md) 1 |
| Fabric Dashboards | hosted consoles (`switchboard session-host`, FD-211) | survive | [estate](research-estate.md) 1 |
| Fabric | terminals (node-pty children), the board dispatcher's watchdog and sweep | terminals die on a quit, a broker stop, logout or an update | [estate](research-estate.md) 1 |
| Fabric Switchboard | the duty officer, the session watch, limit-stop registration, claim and finish, hook events to launch records | a duty act could be cut mid-way (a close takes up to 10.5 s, the drain 8 s, the hard exit 10 s) and a limit stop reported in the gap was refused: both fixed by W0.1 and W0.4; hook events to launch records in the gap are still lost | [inventory](research-switchboard.md) 0, 2 |
| Fabric Switchboard | the managed proxy | managed requests get 503 during an update drain and fail while no owner runs; the process lives on | [inventory](research-switchboard.md) 2 |
| Fabric Switchboard | session hosts (`setsid`), Terminal sessions, dispatched runs, data files | survive; nothing supervises a host; it runs from the app bundle's path | [inventory](research-switchboard.md) 1.1, 2 |

No product installs a shared component "if missing"; the lifecycle broker is private, installed by
hand, and never updates itself ([estate](research-estate.md) 2, 3).

## R1: restart waves in Switchboard

- **Two calls, so the caller knows what it does.** `switchboard sessions restart plan` (MCP
  `switchboard_sessions_restart_plan`) answers every running Claude Code session the wave would
  restart, every one it skips with the reason, the caller's own session, and a `digest` (sha256 over
  the sorted `(session, action, skip_reason)` and the caller; turn state and times excluded). It
  restarts nothing. `sessions restart apply --digest <d> --reason <text>` (MCP
  `switchboard_sessions_restart_apply`) starts the wave for that exact plan (`plan_stale`
  otherwise). Both tool descriptions, and the plan's answer, say in one sentence: every listed
  session, the caller's own last, ends and is resumed as itself at the end of its current turn; the
  caller should end its turn after `apply` and will be resumed with the wave's outcome. `status`
  and `cancel` complete the set.
- **Who may.** Any agent of the person may restart every session on this machine (the operator's
  words above). Guards: a reason is required and recorded; one live wave at a time
  (`wave_running`); `cancel` leaves every session still `pending` untouched; the start is
  announced, not only the outcome.
- **Never in the middle of work.** A session restarts only between turns: when its session file
  says `idle` and no command or subagent of its own runs (`duty::observe`). `busy`, `shell` and
  `waiting` (a question or permission for the person, mid-turn) stay `pending`; a session that stays
  busy restarts when its turn ends, however late. Nothing is read from a transcript's content.
- **Skipped in R1, each with its reason in the plan:** a session with a live limit stop (`limit`:
  recovery alone resumes it, SL-5); one holding its own wait, a wake-up, a monitor or a cron job
  (`has_waits`: re-arming them through a prompt comes later; skipping loses nothing); a second copy
  of a session (`duplicate`, reported, not closed); a Dashboards plain console (`console_not_hosted`:
  it would come back outside Dashboards); a session whose host predates restarts (`host_too_old`);
  a non-interactive run (`not_interactive`); one whose folder or transcript is gone, or with no
  `claude` to resume it (`cannot_resume`); and the preflight below.
- **Preflight before any end.** The folder and transcript exist, `claude` is found (no capacity
  check: a restart replaces its own session), and for a session in a managed or isolated home its home's
  `launch.command` is readable. A session failing any check is skipped and never ended.
- **Same session.** Resumed with `claude --resume <id>` in its own config folder and working folder
  (`duty::resume_script_for`), with Switchboard's permission choice as the duty officer's restart
  and Continue use today (the flags the session started with are not read in R1: a person's own
  `--permission-mode` is not carried; stated in the plan's answer). R1.0 measures whether the id is
  kept; if Claude Code gives the resumed process a new id, `back` reads the session's launch or
  host instead.
- **In place for a hosted session.** A host started by this release offers `restart` (`can` in its
  `HELLO` answer; protocol 1 is unchanged, so every viewer keeps working). Switchboard writes the
  resume script, keeping `SWITCHBOARD_LAUNCH_ID`, under `<data>/restarts/` and sends `RESTART`; the
  host hangs up the session's process group, starts the script on the same terminal once it exited,
  and every viewer (a Dashboards hosted console, a Terminal window) stays attached and sees the new
  process. A session not in a host is stopped as the duty officer stops it and resumed in a new
  Terminal window (`start_terminal`), through a host when it has a launch; its old tab shows the
  ended process.
- **What it is told.** A session that was idle (waiting for the person's next message) resumes with
  no prompt: it is back, waiting as before, and does no work nobody asked for. Only the caller
  resumes with a prompt: the wave id, which sessions are back, failed or still pending, and to call
  `switchboard_sessions_restart_status` before it continues its task.
- **Paced.** At most three sessions are ending or resuming at once. While a session is in a wave the
  duty officer leaves it alone. A wave's acts do not count toward the duty officer's restart budget.
- **Watched to the end.** The wave is a durable record `restarts/<wave>.json` (0600; kept 30 days,
  at most 200), advanced every 60 s on the session watch's tick: per session `pending`, `ending`,
  `resuming`, `back` (a live process started after the end holds the session), or `failed` with the
  reason (not `back` 5 minutes after its end). For the caller, `continuing` follows `back` once its
  transcript is written after the resume. No retry in R1 (a slow first resume and a retry would make
  two copies). A failure shows under *Needs attention* and in the outcome. The caller restarts last:
  once every other listed session is `back`, `failed` or still `pending` behind a long turn, or 15
  minutes after `apply`, whichever comes first; its prompt names what is still pending. A wave
  survives an update or a restart of Switchboard: the next owner reads the record and goes on.
- **Where the outcome goes.** `status`, the Sessions page's *Needs attention*, the log event
  `restart_wave`, and on the operator's machine one `communicator_notify` per wave from the agent
  that ran it.

## The service: decisions

Each names what it rejects. The open one carries the default this plan proceeds on.

- **D-1 Its own product; its own repository once its API is frozen.** Fabric Sessions has its own
  binary (`fabric-sessions`), bundle id `ai.passioncode.fabric-sessions`, release artefact and
  version from W1. Its code starts as two crates inside the Switchboard workspace
  (`fabric-sessions-core`, `fabric-sessions`), so a fix lands with one gate while the seams move; when
  the socket API v1 is frozen (W2's exit) the crates move with their history to
  `passioncode-ai/fabric-sessions` and Switchboard depends on its tag. Rejected: a new repository on
  day one (round 1, design 17); the service inside Switchboard for good (a Fabric-only machine would
  need Switchboard).
- **D-2 The core has no Switchboard dependency.** W1 moves into the core every helper the moving
  modules use (round 2, accuracy 1): `person`, `launches::{valid_id, new_id, session_of,
  recent_records, NO_SUCH_LAUNCH}`, `monitor::now` (as `clock::now`), `account_send::{data, quoted}`,
  `switchboard_core::{private_fs, windows, Provider}`, the Terminal and script helpers of `launch`,
  and `recovery`'s storage half. The permission choice (`agent_settings::bypass`) becomes a parameter
  answered by `Accounts` (none: false); the shell homes folder is `Accounts`' answer to "which config
  folders". Checked by `cargo tree -p fabric-sessions-core` naming no `switchboard-*` crate.
- **D-3 Supervised by launchd on macOS.** A LaunchAgent `ai.passioncode.fabric-sessions` whose
  `ProgramArguments[0]` is the stable link
  `~/Library/Application Support/ai.passioncode.fabric-sessions/current/fabric-sessions` (LC-05),
  `RunAtLoad`, `KeepAlive`, `ThrottleInterval` 10, `ExitTimeOut` 30, `AssociatedBundleIdentifiers` the
  products that installed it; `launchctl bootstrap gui/<uid>`. One copy per user: a second exits 75.
  Linux, Windows and Mac Studio spaces run the core in-process inside Switchboard as today, each with
  its row (round 2, simplicity 2). Rejected: the lifecycle broker (private, installs nothing),
  `SMAppService` (the plist would live in one app's bundle).
- **D-4 Hosts outlive the service, and the service is not needed to start one.** A client asks the
  service to start a host (`host.start`); when the socket does not answer within a second it runs
  `fabric-sessions host` itself and the service adopts the host on its next pass. At its own start
  the service adopts every live host, Switchboard's too, by the socket path in each descriptor.
  Dispatched runs stay Switchboard's to start and supervise (they already survive a quit, and the
  next owner sweeps them; round 2, simplicity 2).
- **D-5 Accounts and the proxy stay in Switchboard.** The service holds no credential, token, proxy
  or account file and never opens Switchboard's vault (SS-5).
- **D-6 Switchboard is one of three states, and a session never moves account silently.** *Absent*
  (not installed), *reachable* (answers `hello`) or *unreachable* (installed, quitting, updating or
  crashed). A session on a Switchboard account (a managed group folder or an isolated home) is
  resumed only while Switchboard is reachable; while it is unreachable the resume waits and its row
  says so. Only with Switchboard absent does a session resume on the person's own login.
- **D-7 The `Accounts` interface.** Five questions, each with its answer without Switchboard: the
  launch environment of a resume (Switchboard writes a 0600 script; none: the person's own
  environment), the readiness of a limit stop (none: the provider's reset time), a label for an
  account id (none: no label), the config folders to scan (none: `~/.claude`, `~/.codex`), and the
  permission choice for a resume (none: none).
- **D-8 One protocol, written in the contract at W2's exit.** The socket API, the host's `can` list,
  the session row (SB-395) and the restart wave record become decisions of `fabric-agent-contract`
  with vectors, when the API is frozen (round 2, simplicity 2: not before the seams settle).
- **D-9 One writer per file, handed over once.** Switchboard hands its session files over (it writes
  `sessions-handed-over.json` after stopping its own duty officer and watch) only when no host it
  started before the switch-over is alive; until then it stays the owner, and one restart wave
  replaces those hosts at once. No mirroring. Uninstalling the service leaves its data folder;
  Switchboard taking the work back reads it through the same import (round 2, simplicity 2).
- **D-10 Every product installs it if missing or older.** A product bundles a signed
  `fabric-sessions` (macOS `Contents/Helpers/`, copied after signing, not re-signed) and at its start
  runs `install --if-newer`: under one lock, verifying the candidate's signature (team `KJ35UYYL22`,
  identifier `ai.passioncode.fabric-sessions`) before running it, into
  `releases/<version>-<digest12>/` with a receipt (`schema: 1`, `clients[]`); never a downgrade; the
  new release becomes `current` only after it answers `hello` as itself within 30 s, else the old one
  is restored. Products update every five minutes, so their bundled copies keep the service current;
  the service has no feed of its own (a documented LC-16 deviation). The person's off switch wins
  (`off` in the data folder, or the job disabled in Background Items: `held_off`). The last product's
  uninstall runs the service's, which refuses while hosts run (SS-11).
- **D-11 The other apps become clients.** Fabric terminals become hosts at W4. Dashboards' limit-resume
  consoles start hosted now (an FD row, round 2, simplicity 2); its plain consoles become hosts later.
  Each app keeps its own idle time; the service closes a session it owns at its own floor of at least
  65 minutes.
- **D-12 Hooks belong to the service's own plugin.** A Claude Code plugin `fabric-sessions` (hooks and
  the MCP server), installed with the service, calls the stable entry `~/.local/bin/fabric-sessions`.
  The hook reads stdin up to 8 MiB and keeps only `hook_event_name`, `session_id`, `cwd`,
  `transcript_path`, `source`, `error`, `tool_name` and the waits' `tool_input`; it drops
  `last_assistant_message` and `prompt`; the stored event is at most 64 KiB (round 2, accuracy 17).
  It writes to the socket, or the spool when the service is down, never opens a vault, exits 0 within
  a second. Switchboard's plugin keeps only `project suggest` on `SessionStart`, and stops sending the
  launch `Start` event in the same release (round 2, accuracy 23).
- **D-13 Two owners never act.** Switchboard asks `hello` at start and every pass; it stops its duty
  officer and watch while the service's `owns` lists them and takes them back only after three failed
  passes with launchd reporting the job unloaded or disabled. The service holds its own while a
  Switchboard older than the switch-over release owns the store (`held:
  switchboard_owner_without_handshake`).
- **Open O-1 (the operator).** The name. Default *Fabric Sessions*; frozen the day W1 starts.

## Data ownership

| File | Until hand-over | After hand-over |
|---|---|---|
| `sessions/` host descriptors | Switchboard's hosts | the service's; Switchboard's last hosts end before the hand-over (D-9) |
| `monitors/`, `recovery.json`, `resumable.jsonl`, `duty.*`, `session-watch.json`, `capacity.json`, `restarts/`, `stop-spool/` | Switchboard | the service; `recovery claim|finish|register` forward to it (round 2, accuracy 10) |
| `launches/` | Switchboard | Switchboard writes a launch; the core writes its lifecycle events through the `Events` trait, spooled while Switchboard is unreachable and dropped while it is absent |
| `dispatches.json` | Switchboard | Switchboard |

## Requirements

| Id | What holds | Checked by |
|---|---|---|
| SS-1 | Quitting, updating or crashing Fabric, Fabric Dashboards or Switchboard ends no session process the service hosts and loses no lifecycle record. A managed session's requests fail while Switchboard's proxy is down (503 during an update drain); its row says `proxy_down` | per app: a hosted session, the app updated and quit, the process lives, every record is there |
| SS-2 | The service restarts or updates without ending a session; it adopts every live host within 5 s of its start | kill the service, start it: the host list matches the descriptors |
| SS-3 | One copy per user; a second exits 75; one install lock | two starts; two installs at once |
| SS-4 | Whichever product starts first installs it; an older bundled copy never replaces a newer install; a failed new release is rolled back; the person's off switch wins | install tests a < b both orders, a release that never answers `hello`, `held_off`, Background Items disabled |
| SS-5 | The service holds no credential and never opens Switchboard's vault | `cargo tree`; a planted-secret scan of its data after a test run |
| SS-6 | Every rule of SB-396 holds in the service | the moved tests under the same names |
| SS-7 | A limit stop reported while no app and no service runs is kept and resumed | report with everything down, start: the stop is registered |
| SS-8 | Every product shows the same rows and attaches to the same hosts | the contract's vectors in each gate (from W2's exit) |
| SS-9 | With Switchboard absent the service hosts, watches, closes and resumes sessions on the person's own login; with Switchboard unreachable it resumes none of Switchboard's sessions | a profile without Switchboard; one with Switchboard stopped |
| SS-10 | The service reports `agent-health/1`, writes the operations digest (DEC-0153) and has a `fabric-service/0.1` descriptor | the contract validator |
| SS-11 | Uninstall leaves no session unattended (refuses while hosts run unless told to end them) and leaves the data where Switchboard can take it back | install, register a stop, uninstall: Switchboard resumes the stop |
| SS-12 | A restart wave resumes each listed session as itself, in its own config and working folders | hosted and Terminal sessions: the same session (R1.0's rule), the folders match, the transcript continues |
| SS-13 | `apply` runs only the plan whose digest it names; the plan names the caller, who restarts last; no session is ended mid-turn or before its preflight passed | MCP tests: stale digest, order, a busy session stays `pending`, a failed preflight is skipped |
| SS-14 | Each restarted session is `back` or `failed` with a reason within 6 minutes of its end (5 minutes plus the 60 s tick) | fake-clock tests; a live wave on the MacBook (R1.7) |
| SS-15 | A session with a live limit stop or its own wait is never ended by a wave | planner tests |

## Contract rules this service meets

LC-01: the drain ends within 5 s, hard exit 10 s, never ends a host (deviation: hosts are not
children). LC-02: hosts are `setsid` with ppid 1 by design, each with a descriptor, the service their
supervisor of record (deviation). LC-03: a duty act is journaled before it acts (W0.1). LC-04, LC-21:
no secret on argv; an environment script is 0600, read once and removed by Switchboard after the
start. LC-05: D-3, D-10. LC-06 (deviation): it reads `~/.claude/sessions`, transcript times and the
config folders D-7 names. LC-07: Background Items read at each product start. LC-08: idle CPU under
0.1 %, RSS at most 40 MB, a fake-clock hour test. LC-09: its `AGENTS.md` lifecycle table; Switchboard,
Dashboards and Fabric update theirs. LC-10: `fabric-sessions mcp` exits on end of input within 1 s and
answers `stale` after an update. LC-11: the socket path is stable across updates. LC-12: logs (5 × 5
MB), the spool (5,000 files or 20 MB) and waves are bounded. LC-13: a release it no longer uses is
pruned after no live host uses it (pid checked). LC-14: D-10, SS-11. LC-15: its build prunes. LC-16:
D-10's deviation. LC-17: hosted starts and resumes go through admission. LC-18: at least 65 minutes.
LC-22: waves and the spool. LC-19, LC-20, LC-23: not applicable (no user-wide MCP server, no port, no
browser). SL-1 the start record; SL-2 the session's account comes from `Accounts`; SL-3 turn events;
SL-4 end records; SL-5 a wave never resumes a limit-stopped session; SL-6 D-4; SL-7 D-3 (W0.7 amends
SL-7's wording: launchd or systemd directly, or the broker); SL-8 rows plus `unmanaged`.

## MCP tools

| Switchboard (today and R1) | Fabric Sessions (from W2) |
|---|---|
| `switchboard_sessions` rows, tree and machine | `sessions_rows`, `sessions_tree`; launch history stays in Switchboard |
| `switchboard_session_end`, `_continue`, `_close_copy` | `sessions_end`, `_continue`, `_close_copy` |
| `switchboard_duty`, `switchboard_capacity` | `sessions_duty`, `sessions_capacity` |
| `switchboard_sessions_restart_plan`, `_apply`, `_status`, `_cancel` (R1) | `sessions_restart_plan`, `_apply`, `_status`, `_cancel` |
| `switchboard_provenance`, `switchboard_recovery` | stay in Switchboard (accounts), reading the service's records |

Switchboard's names keep working and forward to the service once it owns the sessions (round 2,
simplicity 4: no deprecation period). The service's MCP server is declared by its plugin (D-12).

## What does not change

Accounts, groups, organizations, the proxy, balancing, spending ceilings, dispatch and every account
act stay in Switchboard. Fabric's board and dispatcher policy stay in Fabric; Dashboards' views stay in
Dashboards.

## Risks

- **R-1 Two owners during the switch-over.** D-13.
- **R-2 Hosts of an older release.** D-9 waits for them or replaces them with one wave.
- **R-3 Install races at login.** One lock, the receipt read under it, idempotent `bootstrap`.
- **R-4 Signing.** A bare Mach-O is notarized as a zip and cannot be stapled (Switchboard's CLI is the
  precedent).
- **R-5 An older product that does not know the service** runs its own watchers: D-13's hold.
- **R-6 `--resume` and session ids.** The evidence is mixed (`machine_sessions.rs` holds the same id;
  CONTRACTS records a `SessionStart` with `source: resume` under a new one): R1.0 measures it first.
