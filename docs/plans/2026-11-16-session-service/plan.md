# Fabric Sessions: the plan in waves (2026-11-16, draft 3)

The spec is [README.md](README.md). Each task has an id, the repository that owns it, what it changes
and the check that closes it. A wave ends when its exit checks are green on the artefact the operator
uses. Tasks of another repository go there as a board request the day the wave opens. Every task
re-reads the `file:line` it cites before it starts.

Repositories: **SB** fabric-switchboard (the service's crates live here until W2's exit), **FS**
passioncode-ai/fabric-sessions (from W2's exit), **CT** fabric-agent-contract, **FW**
fabric-workspace, **FD** fabric-dashboards, **FB** fabric, **LN** passioncode (the launcher), **OI**
org-index, **AD** fabric-agents-database.

## W0. The holes that hurt today

| Id | Repo | Task | State |
|---|---|---|---|
| W0.1 | SB | The duty officer writes an `intent` line (with the config folder and the prompt choice) before it acts and the outcome after; the next owner finishes an act a quit cut within an hour (a close whose process is gone is recorded and resumable; a restart whose process is gone is resumed; a live process is recorded `interrupted`); `duty.jsonl` appended under a lock on `duty.lock` | **done** on `session-w0`: `an_intent_without_its_outcome_is_an_act_cut_by_a_quit`, `a_cut_close_is_finished_by_the_next_owner_and_a_live_process_is_left_alone` |
| W0.2 | SB | `resumable::record` under a lock on `resumable.lock` | **done** on `session-w0`: `two_writers_at_once_both_keep_every_line` (without the lock 31 of 200 lines survive) |
| W0.3 | SB | The session watch runs when the account snapshot fails | **done** on `session-w0` |
| W0.4 | SB | A `StopFailure` report with no owner, or while a draining owner holds the store, is kept in `stop-spool/`; the next owner's pass registers it and removes the file only after it answered | **done** on `session-w0`: `a_stop_reported_with_no_owner_is_kept_and_registered_by_the_next_owner`, `stop_spool::tests`, `recovery_hook_never_fails_the_session` |
| W0.5 | SB | Backlog SB-397 (done) and rows: Linux, Windows and Studio spaces run the core in-process; hook events to launch records lost while no owner runs; argv flags not carried on a resume | rows under lease |
| W0.6 | SB | Release W0 | merged #775; in 0.7.108 (`56db77bc`) |

## R1. Restart waves in Switchboard (README "R1")

State 2026-11-16: R1.0 to R1.6 built on `restart-waves` (PR #780), gate EXIT 0; R1.7 (release and the live wave) next.

| Id | Repo | Task | Closed by |
|---|---|---|---|
| R1.0 | SB | Measure on the installed Claude Code: what a running session picks up without a restart (a new skill, a changed skill, a plugin, a hook, an MCP server); whether `claude --resume <id>` keeps the id; whether `Stop` fires on an interrupt | the measurement in `docs/evidence/` with the version |
| R1.1 | SB | The host's `restart` capability: `can: ["restart"]` in the `HELLO` answer, frame `RESTART {program}` (a file of this user under `<data>/restarts/`, not writable by others), the session's process group hung up and the program started on the same terminal once it exited, viewers kept; `session_host::restart` for a controller | host tests: a viewer attached across a restart sees the new process's output; a program outside `restarts/` is refused; an old host's `HELLO` without `can` is answered `false` |
| R1.2 | SB | The planner `restarts::plan`: `duty::observe`, live limit stops, monitor waits, duplicates, Dashboards plain consoles, hosts and their `can`, preflight; the caller as the nearest ancestor process a session file names (an ancestor-pid walk; none: `caller: null`); the digest | planner tests over fixtures, every skip reason |
| R1.3 | SB | The runner `restarts::tick` on the session watch's 60 s tick: at most three in flight, turn boundaries only, the resume script under `restarts/` keeping the launch id, in place for a capable host else the duty officer's stop and `start_terminal`, the caller last with its prompt, the duty officer leaving a session in a wave alone, `back` and `failed` at 5 minutes, the record kept 30 days or 200 waves | runner tests on a fake clock with fake sessions and a fake host |
| R1.4 | SB | `Operation::SessionsRestart{Plan, Apply, Status, Cancel}`, CLI `sessions restart plan|apply|status|cancel`, MCP `switchboard_sessions_restart_plan|apply|status|cancel` with the one-sentence warning; the CLI and MCP pass the calling process's pid; parity rows (window: the Sessions page's button is R1.6) | MCP tests: stale digest, `wave_running`, no reason, order |
| R1.5 | SB | Needs attention shows a failed session of a wave; the log event `restart_wave`; docs: CONTRACTS section, CLI.md, AGENTS.md (writes `restarts/`), tools.md, CHANGELOG | docs checks |
| R1.6 | SB | The Sessions page: *Restart all sessions* shows the plan (what restarts, what is skipped and why) before the confirm, then each row's wave state; strings through Copylot in en-US and ru-RU | the window's tests, SCN row, Copylot ids |
| R1.7 | SB | Release, then a live wave on the MacBook: every running Claude Code session (this one included) comes back as itself | the evidence with commands and outputs |

## W1. The core without Switchboard, still inside Switchboard

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W1.0 | SB | Golden answers before anything moves: `sessions --rows`, `sessions` tree, `duty`, `capacity`, `sessions continue` refusals, offline `recovery` (the `needs_owner` refusal), the session tools of MCP `tools/list` and their answers, run with `HOME=<tmp>`, a synthetic data root, and placeholders for `live`, `machine`, `pid`, `*_at`, `at`, `since`, `age` and RFC 3339 values | the golden test passes on `main`; built: `crates/switchboard-cli/tests/golden.rs` and `tests/golden/`, the process table read as empty under `SWITCHBOARD_TEST_NO_PROCESSES` (debug builds only), the same bytes on macOS and Linux |
| W1.1 | SB | Crates `fabric-sessions-core` and `fabric-sessions` (binary) with their own version; the gate covers them; Windows targets compile (the host is `cfg(unix)`) | the gate and the Windows nightly; built: both at 0.1.0 with a test each, in the Windows test steps of the nightly and release and in check.sh's Windows check, and check.sh fails when `cargo tree -p fabric-sessions-core` names a `switchboard-*` crate |
| W1.2 | SB | Into the core: `session_host` and attach, `machine_sessions`, `capacity`, `monitors`, `resumable`, `restarts` (R1), `session_rows` (labels and recovery through `Accounts`), `session_watch` (organizations through `Accounts`), the whole duty officer (`observe`, `decide`, `check`, `act_on`, `stop`, `finish_interrupted`), `recovery`'s storage half, and every helper D-2 lists. Both roots stay Switchboard's data folder in W1 (host socket names are a digest of it) | `cargo tree -p fabric-sessions-core` names no `switchboard-*` crate; the moved tests pass under the same names; progress: a `private_fs` and Windows storage (#787), b `person`, `ids`, `cli`, `clock` (#789), c `machine_sessions` with `Accounts` (#790), d `capacity` (#791), e `monitors`, `stop_spool`, `events` (#792), f `session_rows` (#793), g the Terminal helpers and `Accounts::open_terminal` (#795), h `Provider` and the stops record (#796); next `recovery`'s storage half, `resumable`, `duty`, `restarts`, `session_host`, `session_watch` |
| W1.3 | SB | `Accounts` (D-7's five questions plus labels and a folder's organizations) and `Events`, with the no-Switchboard answers and Switchboard's implementation | unit tests of both; progress: `Accounts` with Switchboard's `session_accounts::Homes` (#790, #795), `Events` reaching oplog (#792) |
| W1.4 | SB | Switchboard uses the core; its copies deleted | W1.0's golden answers byte for byte; the full gate |

Exit: a Switchboard release on the core with no visible change.

## W2. The service runs on its own (with accounts)

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W2.1 | SB | `fabric-sessions serve`: one copy per user (a second exits 75), a drain that never ends a host, `agent-health/1`, the operations digest, a `fabric-service/0.1` descriptor, logs in `~/Library/Logs/Fabric Sessions/` (5 × 5 MB, 0600) | SS-3, SS-5, SS-10 |
| W2.2 | SB | Its data folder `~/Library/Application Support/ai.passioncode.fabric-sessions/`; D-9's hand-over import (Switchboard writes `sessions-handed-over.json`); host sockets in `confstr(_CS_DARWIN_USER_TEMP_DIR)` + `fsx/`, every path at most 103 bytes | import tests; a socket older than three days still answers |
| W2.3 | SB | Adopt hosts by the socket path in each descriptor (`connect` takes a path); a host that died without an end record gets one | SS-2 |
| W2.4 | SB | The socket `service.sock` (folder 0700, socket 0600, peer uid checked, 4-byte length plus JSON up to 1 MiB, `{op, id}` and `{ok}` or `{error: {code, message}}`, 2 s read and 5 s request timeouts); ops `hello` (`{service, version, api, instance, owns, since}`), `rows`, `tree`, `attach` (answers the host's socket path), `end`, `continue`, `event`, `host.start` (provider, folder, config folder, resume id, an environment script that is a 0600 file under a client's data folder named in the receipt, never free argv), `restart.plan|apply|status|cancel`; the spool `spool/` (0700): one event per file `<unix_ms>-<pid>-<rand8>.json`, `{v: 1, id, event, at, hook}` with D-12's fields, at most 5,000 files or 20 MB, applied in name order, idempotent by id (ids remembered 7 days); W0.4's `stop-spool/` imported | socket and spool tests; a fuzz of the framing |
| W2.5 | SB | `fabric-sessions hook` for every hook event (D-12) | SS-7; a timing test; a `Stop` input of 8 MiB is read and stored at most 64 KiB |
| W2.6 | SB | The duty officer, the watch, monitors, waves, Continue and the idle close run in the service; resumes it starts run as hosts, never Terminal under a LaunchDaemon | the SB-396 and R1 tests in the service; a fake-clock idle hour (LC-08) |
| W2.7 | SB | Switchboard's side: `hello` at start and each pass, its duty officer and watch stop while `owns` lists them, `sessions-handed-over.json` written then, its session CLI and MCP and `recovery claim|finish|register` forward to the service, `sessions_owner` log events | R-1 tests both ways; a service restart never makes Switchboard take over |
| W2.8 | SB | `Accounts` over the socket and Switchboard's CLI: `sessions env --session S` (a 0600 resume script with the exports the session needs: a fresh token for an isolated home, the proxy route for a managed one), stop readiness, labels, config folders, the permission choice; D-6's three states drive every resume | an isolated home resumed by the service gets a fresh token; SS-9 both profiles |
| W2.9 | SB | The `fabric-sessions` plugin (hooks and MCP), the stable entry `~/.local/bin/fabric-sessions`; Switchboard's plugin keeps only `project suggest` and stops sending the launch `Start` event | one registration per `StopFailure` with both plugins on |
| W2.10 | SB | `fabric-sessions mcp` (stdio; exits on end of input; `stale` after an update) with the README's tools; Switchboard's names forward | MCP tests; the tools reference page |
| W2.11 | SB, FS, CT, FW, OI, AD | Freeze the socket API v1; the contract decisions with vectors (socket API, the host's `can`, the session row, the wave record, the spool event); move both crates with history to `passioncode-ai/fabric-sessions` and depend on its tag; `lifecycle.md` SL-6 and SL-7 owners and SL-7's wording; `products.md`; the org-index row; the architect's record | the FS gate and SB's gate on the tag; the pages merged |

Exit: on the MacBook the service runs under a LaunchAgent; quitting and updating Switchboard ends no
hosted session and loses no record (SS-1 for Switchboard); SS-9.

## W3. Install and uninstall

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W3.1 | FS | `install --if-newer` (D-10): one lock, the candidate's signature checked before it runs, `releases/<version>-<digest12>/`, the receipt, rollback when the new release does not answer `hello` within 30 s, `held_off`, `launchctl bootstrap` or `kickstart`, LC-07's one notice | SS-3, SS-4 |
| W3.2 | FS | `uninstall`: refuses while hosts run unless `--end-sessions`; leaves the data; `--purge` removes it; a release pruned once no live host uses it | SS-11 |
| W3.3 | SB | The app bundles the signed binary in `Contents/Helpers/` (copied after signing; the smoke checks its identifier) and runs `install --if-newer` at start; `switchboard uninstall` removes itself from `clients[]` and, if last, runs the service's uninstall | a fresh macOS profile gets the service from Switchboard alone |
| W3.4 | FS | Release pipeline: universal Mach-O, Developer ID with hardened runtime, notarized as a zip, `SHA256SUMS` | a release run |

Exit: on a clean macOS user Switchboard alone installs the service; an update of Switchboard updates
it with sessions running.

## W4. Fabric installs it too

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W4.1 | FB | Fabric bundles the binary and runs `install --if-newer`; its terminals start as hosts of the service; the dispatcher's watchdog reads the service | SS-1 for Fabric; a profile with Fabric alone gets the service ("only Fabric installed") |
| W4.2 | FD | Limit-resume consoles start hosted now (`--hosted`), so they survive a quit today | FD's console tests |
| W4.3 | LN | The launcher installs the service and its plugin; its doctor checks presence, version, health and `held_off` | doctor output |

## Later (rows, each with its own task when it starts)

Dashboards' plain consoles as hosts and its bundling of the service; a Linux systemd unit; Studio
spaces as LaunchDaemons; the service's own signed feed; re-arming a session's waits through its
resume prompt in a wave; a retry once with no live copy; carrying the flags a session started with;
the host idle close reading a live wave.

## Order

W0 now. R1 next, on W0. W1 after R1, then W2. W3 after W2; W4 after W3 (W4.2 any time). Acceptance
of each wave is on the MacBook (the operator's machine) with the evidence in `docs/evidence/`.
