---
report: 2026-11-19-lifecycle-remote-access-audit
module: M1 session lifecycle on one machine
read_at: origin/main c997483b (0.7.123)
method: read-only (code and docs from a temporary copy); one read-only `ps` for M1-01; nothing built
---

# M1: session lifecycle on one machine

45 defects no open backlog row fully covers: 6 P1, 18 P2, 21 P3; twelve more are already filed (end of file).

## How a session moves through its life today

**Start.** `launch` (Terminal, `--in-place`, `--headless`, `--hosted`) prepares a home (`homes/<account>` isolated, `runtimes/<provider>-<pool>` managed) and writes `launch.command`, `.session.json` and, for isolated Claude, `settings.json` with the access token. `.launch-pending` is the reservation; the script writes `.session-pid` as it starts; the record is `launches/<id>.json`; the plugin hooks add start, turns and limit stops. A hosted session runs under `session-host` (socket in `/tmp/switchboard-<uid>/`, descriptor `sessions/<id>.json`); a Terminal launch carrying a launch id is wrapped through `session-open`. Board runs: `dispatch` starts `claude -p` or `codex exec` (`dispatches.json`, `launches/logs/<id>.log`); admission through `capacity.json` and `board-pending.json`; refusals in `dispatch-refusals.json`.

**Run and wait.** The owner's one-minute watch (`session_watch::tick`) runs in order: the duty officer (every 300 s), restart waves, a scan, `dispatch_refusals::observe`, the live-session ledger (`live-sessions.json`), boot resume (once per owner; `boot-live.json`, `boot-resume.jsonl`), the findings (duplicate, stuck, gone), Continue offers (`continue-offers.json`) and `session-watch.json`. Waits come from `monitors/<session>.json` and limit stops in `recovery.json` (with `stop-spool/` when no owner runs).

**Stop.** The duty officer closes or restarts and journals each act in `duty.jsonl`; closes write `resumable.jsonl`. The host's idle close uses the same rule; a person's End sends SIGHUP then SIGTERM. The dispatch pass enforces deadlines and the 10-minute extension. A restart wave ends sessions at the end of their turn and resumes them (`restarts/<wave>.json`).

**Continue.** `resumable::continue_as` serves Continue, boot resume and `sessions continue`; the duty officer's `duty::resume`, recovery claims and the automatic fallback continue by their own paths. All build the script with `duty::resume_script_for`, which takes `resume_flags` and the export lines of the home's current `launch.command`.

**Outside the owner.** The watchdog job (`watchdog --once` every 5 minutes) writes `watchdog.json` and acts on nothing.

## Findings

### P1

| id | title | evidence | failure | fix |
|---|---|---|---|---|
| M1-01 | Commands of sessions in Switchboard's own homes are invisible | `duty.rs:498` counts a command only when its line has `/.claude/shell-snapshots/`; live `ps` shows `.../fabric-switchboard/homes/<id>/shell-snapshots/...`; same count in `duty.rs:240` (host keep), `:1115` (`working`), `restarts.rs:699-700` | Every isolated, managed and hosted session reads "no command running": the duty officer idle-closes one with a background job running or restarts one mid-command; host keep, Close the quiet copy and restart waves misjudge it too | Match `<config>/shell-snapshots/` for the session's config folder; test with a home path |
| M1-02 | Claude Code board runs are never extended at their deadline | `dispatch.rs:788-795` extends only on a recent log write (`session_watch.rs:395`); Claude runs as plain `-p` (`launch.rs:3864-3872`), printing only at exit | Every Claude run is stopped `timed_out` at 60 minutes while it works, the failure SB-509 was for; only Codex benefits | Also use the transcript time, or `--output-format stream-json --verbose` (gap in SB-509) |
| M1-03 | The duty officer treats hosted sessions, Dashboards consoles included, as Terminal sessions | `duty.rs:155-169`, `:287` (left alone only with "Fabric Dashboards" ancestry, lost once the spawning CLI exits; boot-resumed hosts never had it); `duty.rs:916-971` stops and resumes in Terminal without a host (`launch.rs:891-903`, `duty.rs:609` strips the launch id) | A stalled hosted or console session is killed, viewers get EXIT, it returns as a plain Terminal window with no host or record; Dashboards may start a second copy | Skip sessions in a live host (`live_hosts`) or restart them in place as waves do |
| M1-04 | A resumed session writes no `.session-pid`, so its home reads idle | only `launch.rs:370` writes it; `duty.rs:587-645` writes none; `ensure_idle` (`launch.rs:1492-1501`) reads only the marker; `clean_account` (`launch.rs:1751-1765`) | During a duty restart, Continue, wave or boot resume, a new launch rewrites that home's files; remove or a lease end deletes the home, transcripts included, under the live session | The resume script writes `.session-pid`, or `ensure_idle` checks the live-session table |
| M1-05 | Resuming an isolated session reuses the home's old access token and skips the account checks | only `launch_core` writes `CLAUDE_CODE_OAUTH_TOKEN` (`launch.rs:3410-3438`); `resume_script_for` uses it as is | A boot resume after a night, a Continue days later or a duty restart fails authentication at once, or runs on an account disabled, elsewhere, given to an agent or at its limit | Refresh the credential through the store and check the account before any resume script; refuse or use a managed route |
| M1-06 | Resuming widens a session's permissions | `session_accounts.rs:87-96` gives the global setting, used at `resumable.rs:317`, `duty.rs:940`, `restarts.rs:913`; a launch's `permissions: ask` or named mode (`launch.rs:3588-3603`) is never read back | A hand-over started with `--ask`, or a `--permission-mode plan` launch, returns from a restart or Continue with `--dangerously-skip-permissions` | Store the effective permission mode per launch and read it on every resume path (SB-541 close) |

### P2

| id | title | evidence | fix | covered |
|---|---|---|---|---|
| M1-07 | A resume picks up another launch's environment (`OBSERVATORY_WORKFLOW_ID`, `OBSERVATORY_HANDOFF_ID` not in `CONFLICTS`) | `duty.rs:604-616`; `launch.rs:3364-3369`, `:54-87` | Keep each launch's environment in its record; add `OBSERVATORY_*` to `CONFLICTS` | none |
| M1-08 | `ended: "limit"` is final: real end, exit code, boot resume and gone lost | `launches.rs:1336-1339`, `end_once` `:475-492`; `boot_resume.rs:376`; `dispatch.rs:516-518` | Limit stop in a separate field, cleared on the next turn | none |
| M1-09 | Watch and duty officer paced by the wall clock: a backward step stops them for its length | `session_watch.rs:517-523`, `duty.rs:803-807` | Pace by `Instant` or reset when `now < last` | none |
| M1-10 | Dispatch deadlines on the wall clock: a sleeping Mac stops runs on wake; a backward step over-admits | deadline in `dispatch_start`, `dispatch.rs:788`, `extension`; `fabric-sessions-core/src/capacity.rs:491` | A clock that stops during sleep; a future-dated reservation is fresh | none |
| M1-11 | A workflow can be dispatched twice while its run is on another machine | `launches.rs:1351-1361` (`home_of` rejects `remote-dispatch/`), `live_dispatch` `:541-565` | Count a record with `dispatch.target` and no `ended` as live | none |
| M1-12 | A remote stop can signal an unrelated process group | `remote_dispatch.rs:379-389` | Answer `none` when `exit` exists; check the start time | none |
| M1-13 | A dispatch holds the owner transaction for its whole start | `lib.rs:1865-1870` | Gates before the transaction | SB-417 (pool lock only) |
| M1-14 | A run starts even when its record or watch entry cannot be written | `launches.rs:954-960`; `dispatch.rs:335-344` | Kill and reap the child, return an error | none |
| M1-15 | A restart wave reads an unread process table as "every session gone" | `machine_sessions.rs:206-208`; `restarts.rs:220-240` | Skip a pass whose table was not read | none |
| M1-16 | `session-open` runs a second copy after a slow host start | `crates/switchboard-cli/src/main.rs:3717-3722`; `session_host.rs:557-593` | Kill and reap the host before falling back; attach on "already open" | SB-444 (one path) |
| M1-17 | A host whose descriptor cannot be written runs invisible | `session_host.rs:198-213` | A failed first write ends `serve` before spawning | none |
| M1-18 | A slow viewer is dropped but its connection stays open | `session_host.rs:327`, `:1040-1088` | Shut the socket both ways | none |
| M1-19 | Restarts without a display stop the session and cannot start it again | `fabric-sessions-core/src/terminal.rs:207-236`; `restarts.rs:505`; `duty.rs:965` | Resume through `session_host::start_detached` headless | none |
| M1-20 | A limit stop is booked to the wrong account for isolated sessions | `lib.rs` ~6050, 4326-4343; `crates/switchboard-cli/src/main.rs:5290` | Resolve from the launch record or `CLAUDE_CONFIG_DIR` | none |
| M1-21 | A recovery claim can start a second copy | `recovery.rs:433-460` | Refuse while `being_continued` or `continued` | SB-463 (part) |
| M1-22 | The automatic fallback ignores stops and continuations in progress | `fallback.rs:315-349`, `:771-1000` | Skip such workflows | SB-463, SB-73 |
| M1-23 | A spooled limit stop comes back after the session continued | `monitor.rs:380-394`; `lib.rs:6090` | `Stop` hook drops the spool; drain skips stale spools | none |
| M1-24 | An unreadable state is read as "nothing there" | `fabric-sessions-core/src/stops.rs:299-308`; `duty.rs:930` | `Option`, skip the act when unknown | none |

### P3

| id | defect | evidence | fix | covered |
|---|---|---|---|---|
| M1-25 | Sessions page offers Continue on every ended or gone Claude launch, board runs included | `session_rows.rs:235-239` | Same `continuable` rule; none on dispatches | none |
| M1-26 | Boot-resume `over_limit`, `at_capacity`, `failed` hidden from ledger rows; tree window by `launched_at` vs records by file time | `lib.rs:7369`; `session_watch.rs:197-203` vs `launches.rs:612-628` | Exclude only `continued`; one window rule | none |
| M1-27 | The duty officer acts up to ~20 s after its read without re-check; CPU sample keyed by pid; cut act settled by `kill(pid,0)` | `duty.rs:507-531`, `:846-888`, `:1049`, `:1244` | Re-read status and pid start before `stop` | none |
| M1-28 | A late host after a boot-resume start runs under a record ended `not_started`; `launched_at` is the pass's start | `boot_resume.rs:662-673`; `resumable.rs:340` | Kill the host on give-up; stamp actual time | SB-444 (similar) |
| M1-29 | `SessionEnd` deletes the whole waits file, erasing a Fabric hold | `fabric-sessions-core/src/monitors.rs:65` | Keep `hold_until` across `SessionEnd` | none |
| M1-30 | Cron waits hold forever in a host; one-shot crons never decrement | `session_host.rs:730-734`; `monitors.rs:73` | Real idle start; expire cron counts | none |
| M1-31 | Session folders hard-coded to `~/.claude/sessions` | `machine_sessions.rs:445-456` | `person::claude_dir()` | none |
| M1-32 | The ledger doc promises continuation after a crash; only `ended: boot` is offered | `live_ledger.rs:1-2` vs `:317-321` | Fix the doc or offer it | none |
| M1-33 | Host: no SIGTERM/SIGHUP handler, no retry of its end; `Session::end` never SIGKILLs yet writes `host_idle`; `SessionEnded` maps `ended` and `signal` to `exited` | `session_host.rs:360-387`, `:778-784`; `main.rs:3703-3714`; `lib.rs:6681-6689` | Handlers, SIGKILL, keep the reason | none |
| M1-34 | `attachable()` lacks the boot check; outputs and dead descriptors pruned only when another host starts (AGENTS.md says on exit) | `session_host.rs:256` vs `:221`; `:173-191` | Add the check; prune on exit | SB-402, SB-554(1) |
| M1-35 | Socket name hashes the data folder as typed; two hosts can bind; failed `waves.lock` ignored; wave digest omits how and pid | `session_host.rs:147-160`, `:649-652`; `restarts.rs:296`, `:189-203` | Canonical path; bind lock; refuse without the lock; digest how and pid | none |
| M1-36 | Dispatch doc says deadlines are rebuilt after a restart; nothing does | `dispatch.rs:12-13` vs `:763-800` | Rebuild at owner start or fix the doc | none |
| M1-37 | A failed spawn leaves a dispatch record that never ends | `launch.rs:3504-3513` | End it `not_started` | none |
| M1-38 | A failing deadline stop is retried every 30 s with two journal lines each; reaper failure leaves a zombie | `dispatch.rs:490-560`, `:435-462` | Back off, one attention item, reap on the caller's thread | SB-545 (part) |
| M1-39 | Survivors in a run's process group are never killed | `launch.rs:4091-4105` | Always SIGKILL the group (a Job Object on Windows) | none |
| M1-40 | Windows: dispatch refused `handoff_refused`; `capacity set` gates nothing; fallback never hands over; `continue_as` wrong refusal text | `continuation.rs:109`; `capacity.rs:301`; `lib.rs:2298`; `resumable.rs:371` | An "unsupported here" code and text | SB-543 (watchdog only) |
| M1-41 | A queued hand-over can run a newer launch's `launch.command` | `launch.rs:521-527`; `lib.rs:5007-5045` | Own script file per queued hand-over | none |
| M1-42 | Docs: reservation 10 vs 5 minutes; SB-376 text vs `SESSION_LIVE` refusal | `launch.rs:534`; `recovery.rs:443` | Fix the docs | none |
| M1-43 | A clock set back makes `being_continued` refuse Continue for the skew | `resumable.rs:258-287` | Treat a future stamp as stale beyond a bound | none |
| M1-44 | The watch sees only records written in 24 h, so a long `claude -p` run drops out of stuck and gone; a remote run reads gone in the tree | `session_watch.rs:338`, `:205-212` | Widen for un-ended dispatches; skip remote runs | none |
| M1-45 | Continue on a session with a waiting limit stop ignores and does not settle it, resuming on an exhausted account | `session_watch.rs:709-760`; `resumable.rs:296-366` | Exclude it or settle the stop | SB-463 |

### Already covered by open rows

SB-552(5) and SB-554(11) boot time on the wall clock; SB-552(2) Continue in place without a host; SB-529 duty check without a spanning lock; SB-545 extension lines in the duty journal's cap; SB-546 and SB-554(6) cross-process locks; SB-402 host descriptor matched by pid only; SB-544 a rotated proxy token restarts every session; SB-503 duplicate copies share one transcript; SB-504 double reads in one minute; SB-554(4) Dashboards' resume agent may race Switchboard's; SB-543 no watchdog job on Windows; SB-553 and SB-557 watchdog loose ends.

**Root shared by M1-01, M1-03, M1-04, M1-05, M1-06:** resume scripts built from the home's current state, and sessions identified by ancestry or command-line path. One continuation module (SB-463) that builds every resume from the launch record (account, credential, permissions, environment, host) and writes the home marker fixes them together.
