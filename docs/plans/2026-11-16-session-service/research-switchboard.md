# Fabric Switchboard: session lifecycle inventory for a separate always-on session service

Read-only research, 2026-11-16. Worktree `.claude/worktrees/agent-a3dff6e86b720f53e`, branch
`lifecycle-d`, HEAD `af209503` (origin/main `145e338e` plus the wave D commits `8b33ff73`,
`64e0d593`, `3af1bb22`, `af209503`). Paths are relative to the repository root. Nothing was built
or run; every claim below is a `file:line` read.

Related plan: `docs/plans/2026-11-16-session-lifecycle/README.md:86` already says the rest of wave
D "moves into the standalone session service (SB-397)". SB-397 has no row in
`docs/evidence/backlog.md` yet (grep finds it only in that README).

## 0. The owner, its drain and its update restarts (the reference for every "on quit" column)

- The owner is the desktop app (Tauri) or `switchboard serve`. It holds the store lock
  `instance.lock`, the control listener (`control.json`), the managed proxy, and the background
  monitor (one pass every 30 s). `AGENTS.md:80-94`.
- Deadlines: `DRAIN_DEADLINE` 8 s, `HARD_EXIT_AFTER` 10 s
  (`crates/switchboard-runtime/src/lib.rs:3341-3342`); the hard exit is a plain thread calling
  `std::process::exit` (`lib.rs:3350-3359`).
- `Owner::shutdown` (`lib.rs:3536-3590`), in order: refresh grants off; `monitor.stop` (awaits the
  tokio pass task, aborts it at the deadline, `monitor.rs:104-116`); `control.close`; take the
  mutation lock; wait for grants; `proxy.stop(left)` (streams get what is left of the 8 s,
  `lib.rs:3563-3565`, `crates/switchboard-proxy/src/lib.rs:432`); `codex_route_sync(false)` takes
  Switchboard's lines out of Codex's `config.toml` (`lib.rs:3567`); statistics, digest, store flush.
  Detached `std::thread`s (the session watch and the duty officer, the dispatch reapers) are NOT
  joined: they die with `process::exit`.
- App quit path: `RunEvent::Exit` arms the hard exit (code 0) and blocks on `owner.shutdown`
  (`src-tauri/src/main.rs:2077-2092`); a signal does the same via `handle.exit(0)`
  (`src-tauri/src/main.rs:1882-1892`). `serve`: signal or update -> `arm_hard_exit` ->
  `owner.shutdown(DRAIN_DEADLINE)` -> exit 75 on an update restart
  (`crates/switchboard-cli/src/main.rs:2224-2244`, `crates/switchboard-runtime/src/serve_update.rs:24`).
- Unattended update restart of the app: at most once in 6 h unless the feed says urgent
  (`src-tauri/src/updates.rs:55`), only at an idle moment; after 30 min of waiting only for
  requests in flight it drains: `proxy.set_draining(true)` (new requests 503 + `Retry-After`),
  in-flight requests get up to 90 s, logs `update_restart drained`, then `restart_as` quits
  through the same `Owner::shutdown` (`src-tauri/src/updates.rs:47-59`, `:1000-1030`). On macOS
  and Linux a detached `/bin/sh` relaunch helper (own process group) waits up to 60 s for the old
  pid to end and execs the new copy (`src-tauri/src/updates.rs:315-338`).
- `serve` update: waits up to 30 min for two quiet looks, then drains and exits 75 so launchd
  (`KeepAlive {SuccessfulExit: false}`) starts it again (`serve_update.rs:20-60`).
- Gap behaviour for CLI callers: `call` tries the control socket, else runs offline by opening
  the store AND the vault (`lib.rs:4455-4465`); while the lock is held (an owner draining or
  starting) it retries every 200 ms for `STORE_WAIT` 8 s (`lib.rs:4468`, `:4492-4520`). Owner-only
  operations offline return "Start the desktop app or 'switchboard serve' before login or launch."
  (`needs_owner`, `lib.rs:4991-4995`).

## 1. Component inventory

Legend for "Owner?": yes = refused or impossible without a running owner; no = works without it;
partial = degrades.

### 1.1 Session host (`crates/switchboard-runtime/src/session_host.rs`, `crates/switchboard-cli/src/hosted.rs`)

- What: one process per hosted session holding its PTY, 1 MiB scrollback, a Unix socket for up to
  8 viewers; frame protocol HELLO/OUTPUT/INPUT/RESIZE/EXIT/ERROR/DETACH/HISTORY/END
  (`session_host.rs:1-49`).
- Process: hidden `switchboard session-host` (`crates/switchboard-cli/src/main.rs:373`,
  `:2922-2940`), spawned by the `switchboard launch --hosted` CLI (or `session-open` in a Terminal
  window Switchboard opened), NOT by the owner: `hosted::spawn` runs `current_exe()` with
  `setsid` in `pre_exec`, stdio null (`hosted.rs:166-197`); the session's own child also gets
  `setsid` + `TIOCSCTTY` (`session_host.rs:400-416`).
- Reads/writes: socket `/tmp/switchboard-<uid>/<16 hex>.sock` (folder 0700, `session_host.rs:107-121`);
  `<data>/sessions/<launch_id>.json` descriptor (`:136`, `:176`); `<data>/sessions/<launch_id>.output`
  kept a day, pruned at start of each host (`:140-171`, `:445`). Idle close thread: with no viewer
  and no activity for `DEFAULT_IDLE` (= duty `WAKE_UP_FLOOR`, 65 min, `:55`) it reads
  `recovery.json` (`recovery::session_has_live_stop`) and `<data>/monitors/<session>.json`
  (`monitors::holds`) without the owner, then ends the session and appends to `resumable.jsonl`
  directly (`session_host.rs:446-487`).
- End: after the child exits, `call(Operation::SessionEnded)` (`main.rs:2929-2939`): via the owner,
  else offline under the store lock; recorded by `launches::end_once` (`lib.rs:5931-5946`).
- Interfaces: CLI `attach`, `launch --hosted`, `sessions end`; MCP `switchboard_session_end`
  (`crates/switchboard-cli/src/mcp.rs:144`); Tauri `session_attach`, `session_end`
  (`src-tauri/src/main.rs:962`, `:984`); `Operation::SessionAttach/SessionEnd` (`lib.rs:6370-6440`).
- Owner?: no for the running session. Starting one needs the owner (`launch` is owner-only;
  `capacity::admit` checked in the CLI, `main.rs:3490`).
- On quit / update: SURVIVES. Own session, not a child of the owner (`AGENTS.md:85`; module doc
  `session_host.rs:6-7`). Two weak points: (a) its `SessionEnded` call during the gap waits up to
  8 s for the lock and fails if the gap is longer, leaving the launch "gone" until the watch
  notices; (b) a managed session inside it still talks to the owner's proxy (section 3).

### 1.2 Duty officer (`crates/switchboard-runtime/src/duty.rs`)

- What: every 5 min (`CHECK_SECONDS` 300, `duty.rs:38`) looks at each live interactive Claude Code
  session; closes finished (idle past max(idle_minutes, 65 min) with no own commands), closes and
  resumes stalled, closes newer duplicate copies; at most 3 actions per check, 3 restarts per 6 h
  (`duty.rs:1-28`, `:42-47`).
- Process: the owner, on the session watch's detached thread: `session_watch::watch` calls
  `duty::tick` first (`session_watch.rs:387-389`); `duty::tick` gate (`duty.rs:589-598`);
  `check` (`:601-662`). Not on Windows (`act_on` returns "unsupported", `:702-704`).
- Reads: process table via `machine_sessions::processes` (in-process) and `/bin/ps` once
  (`:221-274`, `:332-350`); every `sessions/<pid>.json` in `~/.claude`, `<data>/homes/*`,
  `<data>/runtimes/*`, `<data>/shell/*` (`machine_sessions.rs:181-321`); transcript stamps;
  `recovery.json` (`live_stop_sessions`, `:608`), `monitors/` (`:171-180`, `:615`).
- Writes: `duty.json` settings (`:35`, `:106-112`), `duty.jsonl` journal (`:36`, `:528`),
  `resumable.jsonl` on a close (`:641-648`), `<data>/duty/<session>.command` restart script opened
  with `launch::start_terminal` (`:680-699`). Signals: SIGTERM, then SIGKILL after 10 s, polling
  every 500 ms (`stop`, `:845-859`). CPU samples for "stuck" live in memory only (`CPU_SEEN`,
  `:381`).
- Restart script: `resume_script_for` copies the home's `launch.command` exports (except launch id
  and `CLAUDE_CONFIG_DIR`) and runs `claude --resume <id>` in its config folder (`:462-518`).
- Interfaces: CLI `switchboard duty`, MCP `switchboard_duty` (`mcp.rs:137`), Tauri `duty`
  (`src-tauri/src/main.rs:1020`), `Operation::Duty` (`lib.rs:6499-6530`); manual close of a copy:
  `Operation::SessionCloseCopy` / MCP `switchboard_session_close_copy` (`duty::close_copy`,
  `duty.rs:789`). `Duty` and `SessionCloseCopy` bypass the owner transaction (`lib.rs:1504-1508`),
  and `Duty` also runs offline.
- Owner?: the automatic check yes (it lives only in the owner's background pass); `duty --run`
  works offline.
- On quit / update: does NOT survive and is NOT drained. "the drain ends the watch thread with the
  owner" (`AGENTS.md:91`). A close in progress can be cut between SIGTERM and SIGKILL (the stop
  takes up to 10.5 s, longer than the 8 s drain), and a restart cut after the close but before
  `start_terminal` or before `duty.jsonl`/`resumable.jsonl` are written (`duty.rs:664-699`,
  `:640-648`). After the restart the watch's `LAST_TICK` and duty's `LAST_CHECK` are fresh statics
  (`session_watch.rs:349`, `duty.rs:586`), so the first watch of the new owner (within about 60 s)
  checks at once, and stall detection needs two checks again because `CPU_SEEN` was lost.

### 1.3 Session watch (`crates/switchboard-runtime/src/session_watch.rs`)

- What: the tree (every live coding session joined to its launch record, the day's launches) and
  findings (duplicate, stuck board run 30 min, gone launch 10 min) (`session_watch.rs:1-11`,
  `:17-27`).
- Process: owner background pass, at most once a minute, on a spawned `std::thread` guarded by
  `RUNNING` (`:349-384`), called from `monitor.rs:362-365`, and only inside
  `if let Ok(snapshot) = runtime.store.snapshot()` (`monitor.rs:362`): it does not run while the
  account store cannot be read.
- Writes: `<data>/session-watch.json` only when findings change (`:17`, `:408-411`). Reads launch
  records, process table, session files.
- Interfaces: `Operation::SessionTree` (`lib.rs:6328`), MCP `switchboard_sessions {tree}`
  (`mcp.rs:145`), Tauri `session_tree` (`src-tauri/src/main.rs:947`), Attention
  (`lib.rs:4957`). Tree reads run offline too (`lib.rs:1510`).
- On quit / update: stops with the process; findings file stays; first new tick within a minute.

### 1.4 Monitors (`crates/switchboard-runtime/src/monitors.rs`)

- What: a session's own waits (ScheduleWakeup, Monitor `until`; CronCreate/Delete count) so
  nobody closes it before its wait end plus `GRACE` 600 s, or 6 h after the last turn for a cron
  holder (`monitors.rs:1-19`, `:23-31`, `:112`).
- Process: the plugin hook process `switchboard monitor hook` (`plugins/switchboard/hooks/hooks.json`
  PreToolUse matcher `ScheduleWakeup|Monitor|CronCreate|CronDelete`, and SessionEnd, timeout 3 s);
  handler reads stdin and calls `monitors::hook` directly, no owner, no store
  (`crates/switchboard-cli/src/main.rs:4207-4220`, `monitors.rs:90`).
- Writes: `<data>/monitors/<session>.json` (0600, kinds, times, count). Read by the duty officer,
  the session host's idle close and the Sessions rows.
- On quit / update: SURVIVES fully (no owner involved).

### 1.5 Resumable ledger (`crates/switchboard-runtime/src/resumable.rs`)

- What: `resumable.jsonl` (newest 1000 lines, ids and paths) of every end Switchboard caused
  (duty close, host idle close); `continue_session` resumes the same session with
  `claude --resume <id>` in its own config folder in a Terminal window, refused while live or
  without transcript (`resumable.rs:1-11`, `:14-29`, `:115-148`).
- Writers: the owner's duty thread (`duty.rs:643`) and each session host process
  (`session_host.rs:478`). `record` is an unlocked read-modify-write (`resumable.rs:33-47`): two
  processes can lose a line. Reader: Sessions rows, Continue.
- Interfaces: `Operation::SessionContinue` (`lib.rs:6403-6406`, runs offline too), MCP
  `switchboard_session_continue` (`mcp.rs:143`), Tauri `session_continue`
  (`src-tauri/src/main.rs:975`).
- On quit / update: file survives; a Continue during the gap works offline (it opens a Terminal,
  no owner needed). Coupling: the resume script reads the home's `launch.command`/`settings.json`
  (section 3).

### 1.6 Recovery stops (`crates/switchboard-runtime/src/recovery.rs`) and the claim/finish/hook protocol

- What: durable limit stops in `<data>/recovery.json` (7 days, 512 rows; `recovery-pruned.json`
  ids), states waiting/claimed/resumed/cancelled, readiness = fresh capacity or a probe on a
  2/5/10/15 min backoff, LC-9 no probe before a known reset (`recovery.rs:16-39`, `:547-620`,
  `:622-640`). Hosts (Fabric Dashboards, `resumeAfterLimit`) resume; Switchboard never runs the
  session (module doc `:1-7`, `docs/CONTRACTS.md:1682-1700`).
- Protocol and who runs it:
  - `StopFailure` hook (matcher `rate_limit|billing_error`, timeout 10 s): `switchboard recovery
    hook` -> `Operation::RecoveryHook` (`main.rs:4229-4303`). Handler is OWNER-ONLY
    (`lib.rs:5318-5324` calls `needs_owner()`): the stopped account comes from the owner's
    in-memory `limits.bound(session)`, else the folder's group (`snapshot.project_for`), else the
    ordinary CLI's current account (`lib.rs:5324-5347`); `register_stop` resolves the pool from the
    snapshot.
  - `Stop` hook after every turn (timeout 5 s): reads `recovery.json` without lock or owner
    (`session_has_live_stop`, `recovery.rs:731`); only then `Operation::RecoveryContinued` within
    1.5 s; this one also runs offline under the store lock (`lib.rs:5390-5401`,
    `main.rs:4254-4275`).
  - Host commands `recovery register | claim | finish` (`main.rs:1376-1410`): owner-only
    (`lib.rs:5272-5317`). `claim` reads in-memory sign-in verdicts, `store.credential(account)`,
    the snapshot, `limits.recovery_blocked`, `capped_ids`, and refuses a live session (D7,
    `recovery.rs:644-674`). Status `Operation::Recovery` is owner-only too (`lib.rs:5252-5270`).
  - Lock: the file's `WRITES` is an in-process mutex (`recovery.rs:219-223`); cross-process safety
    relies on one writer process (the owner, or an offline CLI holding the store lock).
- Interfaces: CLI `recovery`, MCP `switchboard_recovery` (`mcp.rs:185`), Sessions rows.
- On quit / update: file survives. During the gap: a `StopFailure` registration either waits for
  the new owner (8 s `STORE_WAIT` inside the hook's 10 s) or is refused (`needs_owner`) and the
  stop is lost; a `Stop` hook's settle fits in 1.5 s only when the lock is free; host
  claim/finish/status are refused until the new owner runs. The in-memory limits and session
  bindings (`limits.rs:364`) are rebuilt from transcripts and `limit-evidence.json`
  (`limits.rs:98`).

### 1.7 Launch records (`crates/switchboard-runtime/src/launches.rs`)

- What: `<data>/launches/<launch_id>.json` per launch (launched_by, home, provider session,
  transcript, turns, ended, continued_as), 90 days; logs in `launches/logs/`
  (`launches.rs:1-8`, `:24-35`).
- Writers: the owner at launch (`launch_core`, `launch.rs:3075-3110` region writes `record`);
  hook processes via `Operation::LaunchEvent` (SessionStart via `project suggest --from-hook`,
  Stop = turn, StopFailure = limit) with a 1.5 s deadline (`main.rs:4115-4156`,
  `lib.rs:7976-7990`); session host end `SessionEnded`; dispatch reaper. Lock: in-process
  `WRITES` mutex (`launches.rs:280`). Pruned by the owner pass at most hourly (`monitor.rs:354`,
  `launches.rs:1327`).
- Interfaces: `sessions --history`, MCP `switchboard_sessions`, `switchboard_provenance`
  (`mcp.rs:145-146`); reads add account labels from the store (`lib.rs:7991-8020`).
- On quit / update: files survive; hook events during the gap are dropped once 1.5 s pass.

### 1.8 Dispatch (`crates/switchboard-runtime/src/dispatch.rs`) and remote dispatch (`remote_dispatch.rs`)

- What: Fabric's `switchboard dispatch` starts `claude -p` / `codex exec` for a board request;
  one live run per workflow; time limit (`dispatch.rs:1-17`, `:55-56`).
- Process: `Operation::Dispatch` executed by the owner (`lib.rs:5859-5926`, `dispatch_start`
  `:4614-4830`): account choice (`fallback::account_for_default`), spend gate, `refresh::ensure_fresh`
  of the account's credential, isolated home, capacity gate (`lib.rs:4726`), then
  `launch::spawn_headless` from the owner's own process with `process_group(0)` (not setsid),
  stdin null, output to `launches/logs/<id>.log` (`launch.rs:3314-3365`). `dispatch::watch`
  reaps it on a thread (`dispatch.rs:389-414`); `dispatch::sweep` in the owner pass enforces the
  deadline and records an end nobody watched (`dispatch.rs:534-565`, `monitor.rs:358`). Live list
  `<data>/dispatches.json` (`dispatch.rs:61-64`), in-process `live_lock` (`:312`).
- Remote (SB-317): over ssh, the run on the target is started with `setsid sh run.sh` (or perl
  `setpgrp`) in the background (`remote_dispatch.rs:277-278`); `dispatch status` reads its result
  file over ssh (`:296`).
- Interfaces: CLI `dispatch`, `dispatch stop|status` (`DispatchStop`, `DispatchStatus` bypass the
  transaction, `lib.rs:1614-1619`).
- On quit / update: the running process SURVIVES (own process group, no terminal, reparented to
  launchd; isolated home, so no proxy). Its reaper thread dies; the next owner's sweep records
  `exited` from the record or enforces the deadline (`AGENTS.md:86`). New dispatches are refused
  during the gap.

### 1.9 Machine sessions (`machine_sessions.rs`) and capacity (`capacity.rs`)

- Machine sessions: every Claude Code and Codex process of this user, in-process
  (`proc_listpids` on macOS, `/proc` on Linux, `machine_sessions.rs:445-577`), joined to launches
  by `.session-pid` in `homes/` and `runtimes/` (`:322-349`); quiet after 30 min (`:16`). Read
  only, no owner, no account data (`machine_sessions.rs:1-10`).
- Capacity: `<data>/capacity.json` cap; `dispatch` and `launch --hosted` refuse at the cap
  (`capacity.rs:1-10`, `lib.rs:4726`, `main.rs:3490`). MCP `switchboard_capacity` (`mcp.rs:136`),
  Tauri `capacity` (`src-tauri/src/main.rs:998`). Runs offline (`lib.rs:1504`).
- On quit / update: nothing running; both work offline.

### 1.10 Session rows (`session_rows.rs`)

- What: one list in four groups (needs you, running, waiting, ended) with acts close_copy, end,
  attach, continue (`session_rows.rs:1-13`, `:20`).
- Sources (`:22-35`): the tree, `recovery` status, account labels, attachable hosts,
  `resumable::recent`, monitors. `Operation::SessionRows` (`lib.rs:6445-6489`) reads the snapshot
  for labels and runs `Operation::Recovery` (owner-only, so offline the stops are `Null` and the
  rows say degraded). `SessionRowsEverywhere` reads other machines over ssh.
- Interfaces: Tauri `session_rows` (`src-tauri/src/main.rs:953`), CLI `sessions --rows`, MCP
  `switchboard_sessions {rows}`.

### 1.11 Launch paths and resume paths (`launch.rs`)

- `launch_core` (`launch.rs:2861`): managed mode writes `runtimes/<provider>-<pool>` and puts the
  proxy into the environment (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` capability,
  `launch.rs:2993-3005`); isolated mode writes `homes/<account>` with the account's access token
  in `settings.json` env `CLAUDE_CODE_OAUTH_TOKEN` (or Codex `auth.json`), access only, no refresh
  token (`launch.rs:3010-3045`). Owner-only.
- Terminal paths: `start_terminal` (`launch.rs:765`, `/usr/bin/open -a Terminal` on macOS, own
  process group on Linux `:824-860`) is used by launch, the duty restart, Continue, hand-overs
  (`monitor.rs:338-352` `handovers::sweep`) and the automatic fallback (`fallback::pass`,
  `monitor.rs:333`). A Terminal window is not a child of the owner and outlives it.
- Headless: `spawn_headless` (`launch.rs:3320`), see dispatch.

### 1.12 Plugin hooks (`plugins/switchboard/hooks/hooks.json`)

| Event | Command | Timeout | Owner needed | In the restart gap |
|---|---|---|---|---|
| StopFailure (`rate_limit\|billing_error`) | `switchboard recovery hook` | 10 s | yes (RecoveryHook) | waits up to 8 s, else refused: stop lost |
| Stop | `switchboard recovery hook` | 5 s | no (offline under store lock) | settle and turn count dropped after 1.5 s if lock held |
| SessionStart | `switchboard project suggest --from-hook` | 3 s | no (read-only answer; add needs the transaction) | launch Start event and folder add dropped after 1.5 s |
| PreToolUse (ScheduleWakeup, Monitor, CronCreate, CronDelete) | `switchboard monitor hook` | 3 s | no, no store | unaffected |
| SessionEnd | `switchboard monitor hook` | 3 s | no, no store | unaffected |

Every offline hook call opens the store and the vault (`lib.rs:4455-4465`); only `monitor hook`
avoids them.

## 2. Survival summary on app quit or update

| Component | Survives quit/update | Why / evidence |
|---|---|---|
| Session host and its session | yes | setsid, separate process (`hosted.rs:166-197`, `AGENTS.md:85`) |
| Terminal-launched sessions (launch, duty restart, Continue, hand-over) | yes as processes | Terminal is not a child (`launch.rs:765`, `AGENTS.md:92`) |
| Dispatched runs (local) | yes as processes; reaper lost, next owner sweeps | process_group(0) (`launch.rs:3349`), `dispatch.rs:534` |
| Remote dispatched runs | yes | setsid on the target (`remote_dispatch.rs:277`) |
| Monitor hook files | yes | no owner (`main.rs:4207`) |
| recovery.json, resumable.jsonl, launches/, duty.jsonl, session-watch.json | yes as files | private_write by rename |
| Duty officer check | no; cut mid-action possible | detached thread, `process::exit`; stop takes up to 10.5 s (`duty.rs:845`) |
| Session watch | no; resumes within a minute of the new owner | `session_watch.rs:355-384` |
| Recovery hook registration, host claim/finish/status | no; refused or delayed in the gap | `lib.rs:4991`, `:5252-5324` |
| Hook turn/start/limit events to launch records | dropped past 1.5 s | `main.rs:4146`, `:4063` |
| Managed sessions' requests (runtimes homes, group folders' `.claude/settings.local.json`) | process survives, requests in the gap fail or get 503 | proxy is in the owner (`lib.rs:3563`, `updates.rs:1000-1030`) |
| Codex sessions in group folders | route removed at stop, rewritten at start | `lib.rs:3567`, `AGENTS.md:93` |
| Headless sign-ins (`login begin --headless`) | child of the owner; read as ended by the next owner | `AGENTS.md:84` |

## 3. Coupling to account switching

Tied to accounts, credentials, `accounts.json` or the proxy (stays in Switchboard, or behind a
Switchboard API):
- The managed proxy and its capability tokens: every managed session and every group folder's
  bindings; resume of a managed home copies `launch.command` exports that point at the proxy
  (`duty.rs:483-495`, `launch.rs:2993-3005`).
- Isolated homes: `settings.json` holds an access token with an expiry and no refresh
  (`launch.rs:3010-3036`); a later resume in that config folder silently depends on Switchboard
  having refreshed it.
- Recovery readiness, claim and the StopFailure registration: snapshot accounts, usage readings
  and windows (`full_until`, `recovery.rs:622`), group membership (`may_continue`, `bound`),
  in-memory limits/holds/capped/sign-in verdicts (`lib.rs:5252-5310`), credential validation on
  claim, session to account binding (`limits.rs:364`).
- Dispatch start: account choice, spend gate and ceilings, credential refresh, isolated home
  (`lib.rs:4730-4805`).
- The fallback chain and hand-overs (`monitor.rs:333-352`), launch itself, provenance, the account
  labels in sessions rows, launch histories and Attention.
- `project suggest --from-hook` (folder joins a group: writes `accounts.json`).
- Session watch is gated on `store.snapshot()` succeeding (`monitor.rs:362`), an incidental
  coupling.

Not tied to accounts (could move into a daemon with no account data):
- Session hosts (PTY, socket, viewers, idle close), their descriptors and outputs.
- Process table and session-file scanning (`machine_sessions`), capacity cap and gate.
- The session watch and its findings; the duty officer's observation, decisions, signals,
  journal and restart scheduling (the restart script itself needs the home's env, see above).
- Monitor waits and their hooks.
- The resumable ledger and `continue_session` mechanics.
- Launch-record lifecycle events (turns, ends, gone detection), pruning; dispatch run
  supervision (reap, deadline, stop, `dispatches.json`) once the process is started.
- Recovery file storage, the `Stop`-hook settle, the horizon, and "never claim a live session"
  (D7) as a liveness answer.

Interface such a daemon would need from Switchboard:
1. Folder to group and account: "which group and account does folder F (or session S) run on,
   and in which mode" (today `snapshot.project_for`, `serving_pool`, `limits.bound`).
2. A resume environment: "prepare config folder C / launch L for a resume" (fresh token in an
   isolated home, or the proxy URL and capability for a managed one), instead of reading
   `launch.command` and `settings.json` itself.
3. Recovery readiness and claim: usage readings, holds, sign-in state, group membership, credential
   check; or the daemon keeps the stops and asks Switchboard "which accounts may resume stop E now
   and when to probe", and Switchboard asks the daemon "is session S live".
4. Start a session on an account: launch and dispatch keep choosing accounts and writing homes,
   then hand the started process (pid, launch id, deadline) to the daemon to supervise.
5. Labels for display (account id to label) or the daemon shows ids only.
6. A write path for the hooks that does not open the vault: today every offline hook opens the
   store and vault; the daemon's own socket would take turn/limit/stop events with no account data
   and forward the account-bound part (StopFailure attribution) when Switchboard is up.
7. One writer per file or a cross-process lock: `recovery.json`, `launches/` and
   `resumable.jsonl` are protected by in-process mutexes only (`recovery.rs:219`,
   `launches.rs:280`) or not at all (`resumable.rs:33`).

## 4. Precedent: how Switchboard installs launchd items and updates itself

- Login item: `tauri_plugin_autostart` with `MacosLauncher::LaunchAgent`, label
  `ai.passioncode.fabric-switchboard`, argument `--background` (`src-tauri/src/residency.rs:17-81`).
  After the plugin writes `~/Library/LaunchAgents/ai.passioncode.fabric-switchboard.plist`,
  Switchboard rewrites it by rename to add `KeepAlive {SuccessfulExit: false}` and
  `ThrottleInterval 30` (`residency.rs:84-118`); launchd reads it at the next login (no
  `launchctl bootstrap`). A quit ends with 0 so it is never restarted (SB-254, `residency.rs:81-83`,
  `lib.rs:3347-3359`). Windows `HKCU\...\Run` value `Fabric Switchboard`, Linux XDG autostart;
  uninstall removes them (`crates/switchboard-runtime/src/uninstall.rs:131-137`). The choice is
  `<data>/login-item` (`residency.rs:17`). Only packaged builds register (`residency.rs:36-56`).
- Headless owner: on Mac Studio spaces `serve` runs as a LaunchDaemon installed by Project
  Observatory's validator (`observatory-space-daemons`, OBS-361), definition in
  `docs/CLI.md:213`; `serve` exits 75 to be restarted on a new binary (`serve_update.rs:20-24`).
- Updates: the app checks `latest.json` every 5 min (DEC-0004), minisign-verified, installs into
  the bundle on macOS right away, restarts at an idle moment at most every 6 h, drains for up to
  90 s when requests never stop, and a relaunch helper starts the new copy
  (`src-tauri/src/updates.rs:1-60`, `:315-338`, `:1000-1030`). `switchboard update` does the same
  for CLI-only installs (`crates/switchboard-runtime/src/self_update.rs:1-12`).
- The machine also has the local-lifecycle broker (always-on, launchd at login) that enrols
  `switchboard.desktop` (`AGENTS.md:80`, `:115-121`); a session daemon could be enrolled there or
  installed like the login item.

## 5. Findings worth a board row (not fixed here)

1. Duty officer actions can be cut by the drain (SIGTERM without SIGKILL, close without restart or
   journal line): `duty.rs:664-699`, `:845-859` vs `lib.rs:3341-3342`.
2. `StopFailure` registration is owner-only, so a limit stop during an app restart can be lost:
   `lib.rs:5318-5324`.
3. `resumable::record` has no lock and two processes write it (owner thread, every session host):
   `resumable.rs:33-47`, `session_host.rs:478`, `duty.rs:643`.
4. Every offline hook opens the vault: `lib.rs:4455-4465`.
5. A session-watch tick is skipped whenever the account snapshot fails: `monitor.rs:362`.
6. SB-397 is named in the plan but has no backlog row: `docs/plans/2026-11-16-session-lifecycle/README.md:86`.
