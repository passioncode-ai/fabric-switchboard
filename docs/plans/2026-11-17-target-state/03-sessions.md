# 03 Sessions and work continuation

Part of the [target state](README.md). Read at `f5e2c77e` (origin/main, 2026-11-17). Every
`file:line` is relative to the repository root; `rt/` stands for `crates/switchboard-runtime/src/`,
`core/` for `crates/fabric-sessions-core/src/`, `cli/` for `crates/switchboard-cli/src/`. Nothing
here was run (no cargo, no npm: the disk had 2.7 GiB free); test names are cited from the source,
not from a run.

## What the area is for

A coding session started anywhere on the machine keeps working until its work is done: it starts
on the right account in the right home, it survives a quit or an update of any app, anyone can
open it, it is never ended before it finishes, it is resumed as itself after a limit, a stall or a
restart, and work that one agent cannot finish moves to the next agent or machine. Board work
(Fabric's dispatched runs) starts when it arrives. AGENTS.md "Read first" 1a names the North Star
sections this serves: §3 (a person's Claude seat runs sessions), §4 (fallback chains and ceilings),
§6 (work continues on the next account or agent) and §10 (seats at boot); the lifecycle contract
rules SL-6 and SL-7 (a session does not depend on the app that showed it; everything that watches
sessions runs under a supervisor) are the reason for Fabric Sessions
([session service spec](../2026-11-16-session-service/README.md)).

The Lifecycle table in AGENTS.md is the operational truth for this area (rows: session host,
dispatched run, session duty officer, background monitor, writes). SPEC.md §8 and §14 are stale for
this area (they still describe "a future session supervisor", `docs/SPEC.md:75`, `:107`, and homes
that carry "only the Fabric project board", `docs/SPEC.md:73`, which SB-137 and SB-259 replaced);
CONTRACTS.md is the real contract.

---

### 1. Launching a session (managed, isolated, in place, headless, hosted, Terminal)

**Target.** One launch path for every surface (window, CLI, MCP, hand-over, fallback, dispatch,
continue) with the same preflight, the same home, the same record and the same capacity rule:
managed (the group's proxy route, no provider token in the home) or isolated (the account's own
token in a private home), in a Terminal window that runs through a session host, in place in the
caller's terminal, hosted (`--in-place --hosted`), or headless for a run nobody watches. A refusal
is one sentence and an exit status (`no-route` = 3). Sources: SPEC §8 (modes), CONTRACTS "Launch in
place (SB-75)", "Launch arguments", "Agents and project rules extension" (H-0, H-1, H-1b), SB-343
in "Projects (0.6)"; SCN-005, SCN-006, SCN-016.

**Now.** `rt/launch.rs` (7,557 lines) holds it: `preflight` (`rt/launch.rs:2698`), `launch_here`
(`:2776`), `launch_core` (`:2862`), script writers `script` (`:299`), `write_launch_script`
(`:611`), Terminal via `start_terminal` (`:3722`), headless `headless_args` (`:3451`), the ask and
bypass flags `bypass_args` (`:3180`), `ask_args` (`:3234`), dispatch programs `dispatch_session`
(`:3575`). A Terminal window runs `attach.command` through `session-open` (H-1b), falling back to
the plain script when no CLI or host (`cli/main.rs:3410-3416`). Tests:
`isolated_launch_script_exports_the_home_and_runs_in_the_project`,
`managed_launch_requires_the_selected_account_and_routes_through_the_proxy`,
`managed_codex_launch_points_codex_at_the_local_proxy`,
`an_in_place_launch_prepares_the_same_session_and_opens_no_terminal`,
`a_terminal_window_runs_its_session_through_a_host_when_one_can_be_prepared`,
`launch_refuses_bad_input_before_touching_a_home`,
`terminal_failure_releases_the_home_and_records_no_launch`,
`generated_zsh_script_survives_hostile_paths_and_scrubs_the_environment` (all in `rt/launch.rs`
tests, `:3835-7525`).

**Gap.**
- P2: one file mixes launch, sign-in (`begin_login` `:681` to `sweep_logins_with` `:1026`), agent
  launches (Hermes, OpenCode, OpenRouter, Anthropic keys `:1652-2299`), dispatch programs and the
  session list (`sessions` `:3738`). A change to one breaks others' tests; 28 sleep or timeout
  sites in the file make it one of the three most timing-dependent files in the area.
- P2: SB-354 (`a_linux_terminal_runs_the_generated_script_to_completion` timed out on a Linux
  nightly; ETXTBSY retry added, not yet confirmed on a nightly).
- P2: the capacity gate covers only `--hosted` launches (`cli/main.rs:3970`, `:4036`) and
  `dispatch`; a window launch, a hand-over or the fallback's Terminal start is not counted against
  the pool before it starts (see capability 16).
- P3: SPEC §8 and §14 contradict CONTRACTS (homes, supervisor).
- P3: Windows: a launch opens PowerShell with no host (`cli/main.rs:3384`, "A session host runs on
  macOS and Linux for now."); SB-02 acceptance open.

**Refactor.** Split `rt/launch.rs` into `launch/{plan, script, home, terminal, agents, login,
dispatch}` around one typed `LaunchPlan` (surface, mode, home, account, permissions, capacity
class) that every caller builds; the plan, not the caller, decides the gate. Terminal and script
helpers already live in `core/terminal.rs` (`open` `:85`, `write_script` `:142`,
`spawn_linux_terminal` `:239`); delete the copies left in `launch.rs` (W1.4). Update SPEC §8/§14 to
point at CONTRACTS.

### 2. Homes and what they carry

**Target.** A home Switchboard launches in (`homes/<account>`, `runtimes/<provider>-<pool>`, Pi's
`agent-home`, shell homes `shell/claude-<pool>`) carries the operator's instructions, skills,
plugins, settings without sign-in and the Fabric board server, plus the MCP sign-ins of the default
home (access token only), and never the person's credentials or a refresh token; a nested launch
never inherits a parent's identity variables. Sources: AGENTS.md Writes row (SB-137), CONTRACTS
"The operator's Claude folder in a Switchboard home (SB-259)", "MCP sign-ins across Claude homes
(SB-358, SB-359)", "Agents and project rules extension" (board server).

**Now.** `rt/carry.rs` (`link_into_home` `:185`, `carried_settings` `:624`, `codex_hook_trust`
`:62`, `pi_settings` `:648`), `rt/mcp_homes.rs`, `rt/launch.rs` `board_server` (`:261`). Tests:
`a_launched_home_carries_the_operators_setup_and_a_continuation_trusts_its_folder`,
`the_operators_fabric_board_reaches_a_launched_session_without_its_env`,
`isolated_claude_launch_writes_only_the_access_token_into_its_home`,
`with_own_sign_in_off_a_launch_carries_no_mcp_sign_in`, `script_unsets_every_conflicting_variable`.

**Gap.**
- P2: a home's freshness for a resume started by anything but Switchboard (the duty officer's or a
  wave's resume script copies the home's `export` lines, CONTRACTS "Session duty officer") depends
  on a home that may have been rewritten since; W2.8 (`sessions env --session S`, a fresh token for
  an isolated home) is not built.
- P3: SPEC §8 still says homes are minimal.

**Refactor.** Put "what a resume of this session needs" behind `Accounts` (W1.3/W2.8): today
`core/accounts.rs:9-38` answers homes, markers, `open_terminal` and `resume_flags` only; add
`resume_env(session) -> script path` so neither the duty officer nor a wave reads a home's
`launch.command` text.

### 3. Session hosts and attach (SB-342)

**Target.** Every interactive session runs in a host process of its own that outlives every app;
any app (Switchboard, Fabric Dashboards, Fabric) attaches by launch id, sees the screen (not a raw
replay), resizes, detaches, ends it; a host closes a session nobody watches only after 65 quiet
minutes and never one that waits (limit stop, wake-up, hold); its end is recorded once whether an
owner runs or not; a restart in place keeps viewers. The host is supervised and its descriptor
cannot be confused with another process. Sources: CONTRACTS "Agents and project rules extension"
H-1, H-1b, H-1c, H-1d; Sessions page "Restart waves"; AGENTS.md Lifecycle "A session host";
session service D-8, W2.2, W2.3; SCN-098.

**Now.** `rt/session_host.rs` (1,764 lines): frames (`:37-55`, `write_frame` `:96`, `read_frame`
`:112`), socket `/tmp/switchboard-<uid>/<16 hex>.sock` (`socket_dir` `:129`, `socket_name` `:145`),
descriptor `<data>/sessions/<id>.json` (`write_descriptor` `:198`), `serve` (`:551`) with the idle
close checking `launches::session_of`, `recovery::session_has_live_stop` and `monitors::holds`
(`:607-645`, recorded in `resumable` as `host_idle`), `attach` (`:976`), `end` (`:985`, one 15 s
deadline `:990`), `restart` (`:1067`, programs only under `restarts/`, `:1027`). The end is recorded
by the `session-host` command through `Operation::SessionEnded` (`cli/main.rs:3388-3407`), which
runs without the owner transaction (`rt/lib.rs:1775`) and maps `idle`, `not_started`, else
`exited` (`rt/lib.rs:6472-6488`). Tests (`rt/session_host.rs:1272-1749`):
`two_viewers_share_one_session_a_late_one_sees_the_scrollback_and_both_get_its_exit`,
`a_restart_in_place_keeps_the_viewers_and_runs_the_next_program_on_the_same_terminal`,
`a_restart_in_place_keeps_its_output_when_the_reader_sees_the_old_end_late` (SB-413),
`an_end_wins_over_a_restart_and_a_restart_after_the_end_is_refused`,
`a_session_nobody_watches_and_nothing_moves_in_is_closed_as_idle`,
`a_host_that_does_not_answer_its_hello_is_busy_and_one_that_closes_is_not_found`,
`the_socket_folder_is_this_users_own`.

**Gap.**
- P2: SB-402: `live_hosts` (`rt/session_host.rs:221-241`) and `attachable` (`:243-260`) trust the
  descriptor's pid alone (`pid_alive`, `:215`); a crashed host leaves its descriptor and a reused
  pid labels another process as that launch.
- P2: nothing supervises a host (SL-7): a host that dies leaves no end record (W2.3 "a host that
  died without an end record gets one" is not built).
- P2: the person's *End* reaches the record as `exited`, not `ended` (`rt/lib.rs:6478-6483` has no
  `ended` or `signal` arm), while CONTRACTS H-1d names `reason: ended`; the history cannot tell an
  ended session from one that exited. Not verified whether a reader depends on it.
- P2: Fabric Dashboards' plain consoles and Fabric's terminals are not hosts yet (H-2, H-3; plan
  "Later", W4.1, W4.2), so a quit of those apps still ends their sessions (session service "Where we
  stand").
- P2: timing: 26 sleep or timeout sites in the file; SB-413 bounded every read at 30 s, which is
  correct but makes the suite slow under a load of 800 to 1000.
- P3: the socket name is a digest of the data folder (`:145-151`), so a second data folder (a
  `serve --data-dir`) and the future service's own folder (W2.2) cannot find each other's hosts.
- P3: Windows has no host (`cli/main.rs:3384`).

**Refactor.** Move `session_host` into `core` (W1.2 lists it): it now imports
`switchboard_core::private_fs` (`:198-212`), `crate::duty::WAKE_UP_FLOOR` (`:61`),
`crate::launches`, `crate::recovery`, `crate::monitors`, `crate::resumable`; replace those with the
core's `private_fs`, `stops::session_has_live_stop` (already in `core/stops.rs:287`),
`monitors::holds` (`core/monitors.rs:140`) and an `Events` call for the end. Put the host's start
time and an instance nonce in the descriptor (SB-402) and verify both on every read; the socket path
is written into the descriptor and read from it (W2.3), not derived.

### 4. The session list and rows (sessions, --all, --history, --rows, --tree, --machine, --target)

**Target.** One session model and one answer per question: the running sessions here, every
session here including ended ones, the launch history, the four-group rows (`needs_you`,
`running`, `waiting`, `ended`), the tree of launches, every agent process of the machine, and the
same from another machine through `--target`; every row carries the acts it has (attach, end,
continue, close copy, stop the run). Sources: CONTRACTS "Session list (SB-95)", "Sessions page
(SB-395)", SB-380 and SB-411 paragraphs; SCN-051, SCN-095, SCN-098.

**Now.** Five producers: `rt/launch.rs` `sessions` (`:3738`, homes' `.session.json` plus
`launches::homeless` `rt/launches.rs:797`), `launches::history_page` (`rt/launches.rs:1280`),
`session_watch::tree` (`rt/session_watch.rs:319`), `machine_sessions::scan` (`core/machine_sessions.rs:398`)
and `session_rows::build` (`core/session_rows.rs:88`, joined from the tree, recovery and
`launch::sessions`). Remote: `targets::command_ssh` (`rt/targets.rs:232`); the window's
`SessionRowsEverywhere` reads each machine at once (`rt/lib.rs:6978`). Golden answers:
`cli/tests/golden.rs` with `tests/golden/sessions-cli.json`, `sessions-mcp.json` (W1.0). Tests:
`every_session_lands_in_one_group_with_the_acts_it_has`,
`one_row_per_session_and_every_ended_one_can_be_continued_as_itself`,
`a_resume_with_no_home_is_listed_while_it_runs_and_with_all_after`,
`the_tree_joins_live_sessions_to_their_launches_and_counts_them`,
`a_command_there_quotes_every_word_and_asks_for_json_and_a_terminal_only_when_told`.

**Gap.**
- P2: five overlapping views with different identity keys (home path, launch id, pid, session id,
  stop event) and different "live" rules (descriptor pid, process table, home state); a row can be
  `running` in one view and absent in another (SB-411 was exactly this).
- P2: SB-407 open item: *Stop the run* only in Needs attention, not on the Sessions page rows.
- P3: SB-197 (the Terminal window's identity for *Show in Terminal*).
- P3: SB-353 open: the *stalled* mark from a held Fabric claim (needs Fabric's claims).

**Refactor.** One `Session` record in `core` (ids, agent, folder, account label, pool, origin,
host, process, launch, stop, wait, state, acts) built once per read from the process table, the
records and the stops; every listed view is a projection of it. `session_rows` is already in the
core; move `launch::sessions` and `launches::homeless` there behind `Accounts::launched_homes`.

### 5. Launch records, homeless launches and hook events (SB-212, SB-411, SB-231)

**Target.** Every launch has one record (who launched it, its parent, its session id, its turns,
its stops, its end, where its work went) written once per event even while no owner runs, kept 90
days; a session resumed as itself has a record with no home and is still listed; nested sessions
never overwrite their parent. Sources: CONTRACTS "Launch records and their chain (SB-212)", SB-411
paragraph; AGENTS.md Writes (`launches/`, `launches/homeless/`); session service D-12, W2.4.

**Now.** `rt/launches.rs` (2,408 lines): `record` (`:724`), `record_resume` (`:729`, homeless
marker first), `homeless` (`:797`), `event_from` (`:1075`), `end_once` (`:428`), `link_handover`
(`:987`), `continued_after` (`:649`), `prune` (`:1449`); hook events come from
`cli/main.rs:4661-4700` (`launch_event`, 1.5 s deadline) as `Operation::LaunchEvent`, served
without the owner transaction (`rt/lib.rs:1769`, `:8895`). Tests:
`the_start_and_stop_hooks_record_the_session_its_transcript_and_its_turns`,
`a_nested_session_in_the_same_home_never_overwrites_its_parent`,
`a_resume_whose_marker_cannot_be_written_records_nothing`,
`linking_a_continuation_among_thousands_of_records_reads_no_full_scan`,
`records_older_than_ninety_days_are_pruned_and_nothing_else_is`.

**Gap.**
- P2: hook events to launch records are lost while no owner runs or a draining owner holds the
  store (session service W0.5 row, "Where we stand"); the stop events have a spool (capability
  10), the turn and start events do not. Not verified in code which path drops them.
- P2: SB-387 open: a Fabric console run's `task` is empty until Fabric names its request at
  launch.
- P3: SB-231: `--resume`/`--continue`/`/clear` inside a nested session and a crash between a record
  and its workflow index (`rt/launches.rs:475,562,596-610` as cited in the row; the nested-start
  test above covers part of it, not verified which part remains).
- P3: three on-disk indices (`by-workflow`, `homeless/` markers with `.complete`, the records'
  stamp cache) each with its own rebuild rule.

**Refactor.** The records' writer moves behind the core's `Events` (`core/events.rs` today carries
log events only) and the W2.4 spool (`spool/<unix_ms>-<pid>-<rand8>.json`, idempotent by id): one
journal of session events from which records and indices are derived, instead of three indices
kept in step by hand.

### 6. The session watch (SB-379)

**Target.** Once a minute, on its own thread, the owner (later the service) reads every agent
process, session file and launch of the last day and keeps findings (`duplicate`, `stuck`, `gone`)
with copies described, logs a new finding once, feeds Needs attention and runs the duty officer and
restart waves; it never blocks the background pass and it runs even when the account snapshot
fails. Sources: AGENTS.md Background monitor (session watch), CONTRACTS "Session duty officer"
(close copy, SB-389), "Attention (SB-365)"; session service W0.3; SCN-095.

**Now.** `rt/session_watch.rs`: `tick` (`:410`, at most every 60 s, a `RUNNING` flag so scans do
not stack, `:404-440`) runs `watch` (`:442`): `duty::tick`, `restarts::tick`, the scan,
`dispatch_refusals::observe`, `findings` (`:89`), `describe_duplicates`, a private write of
`session-watch.json` only when the findings change. Called from `rt/monitor.rs:407`. Tests:
`a_copy_of_a_live_session_a_quiet_board_run_and_a_vanished_launch_are_found`,
`a_board_run_writing_its_log_is_not_stuck_and_a_run_elsewhere_is_never_gone`,
`the_tree_joins_live_sessions_to_their_launches_and_counts_them`.

**Gap.**
- P2: `tick` returns at once under `cfg(test)` (`rt/session_watch.rs:412-414`), so the order and
  isolation of duty, waves and the watch on one thread are untested; a slow duty check (up to
  10.5 s per close, three closes) delays the wave's and the watch's minute.
- P3: the duplicate description re-reads `duty::observe` (`:476`), a second process-table read in
  the same pass.

**Refactor.** Move to `core` (W1.2), with a scheduler object (`Clock`, `ProcessTable`,
`Records`) injected so the minute's order is tested on a fake clock (the duty officer already has
an idle-hour fake-clock test pattern in `rt/monitor.rs`, `an_idle_hour_on_a_fake_clock_stays_inside_the_budget`).
One process-table read per pass shared by watch, duty and waves.

### 7. The duty officer (SB-384, DEC-0021)

**Target.** Every 5 minutes, while on, it closes an interactive session only when finished (idle
past `max(idle_minutes, 65)` and none of its commands running), restarts one only when stalled
(busy, still, no CPU, no command), closes only a finished copy of a duplicate, never touches a
waiting session, a held one, one with a limit stop, one in a wave or one in Fabric Dashboards, acts
on at most 3, journals intent then outcome, and finishes an act a quit cut. Covers Claude Code and
Codex, and board runs (SB-407). Sources: DEC-0021 and its three amendments; CONTRACTS "Session
duty officer"; session service W0.1; SCN-096.

**Now.** `rt/duty.rs` (1,581 lines): `observe` (`:221`), `decide` (`:187`), `check` (`:642`),
`tick` (`:630`), `WAKE_UP_FLOOR` (`:42`), `interrupted` (`:789`, cut acts), `close_copy` (`:968`),
`quiet_copy` (`:902`), `older_copy` (`:923`), `stop` (`:1024`), journal under `duty.lock`
(`journal_line` `:541`). Tests: `only_a_finished_or_a_stalled_session_is_acted_on`,
`a_session_whose_commands_could_not_be_read_is_never_closed_as_finished`,
`a_session_stopped_on_a_limit_is_left_to_its_host`, `only_a_finished_copy_of_a_session_is_closed`,
`an_intent_without_its_outcome_is_an_act_cut_by_a_quit`,
`a_cut_close_is_finished_by_the_next_owner_and_a_live_process_is_left_alone`,
`a_session_restarted_three_times_in_six_hours_is_counted`. Board runs: `dispatch::watch`/`sweep`
(`rt/dispatch.rs:398`, `:726`) with `a_run_that_still_writes_its_log_is_stopped_at_its_deadline_and_not_before`.

**Gap.**
- P2: Codex sessions are not watched (CONTRACTS "Codex sessions are not watched"; SB-384 open).
- P2: a stalled session in another program's host (Orca, iTerm) is resumed in a Terminal window
  (DEC-0021), not in its host; with session hosts that should be a host restart in place (as waves
  do).
- P3: `waiting_minutes` and `max_age_hours` are kept and validated in `duty.json` but unused
  (CONTRACTS); dead settings shown in Settings.
- P3: Windows reports only.

**Refactor.** Move whole into `core` (W1.2: `observe`, `decide`, `check`, `act_on`, `stop`,
`finish_interrupted`); its resume goes through `Accounts::open_terminal` (already there,
`core/accounts.rs:28`) or a host restart. Drop the two dead settings with a schema version. Add a
Codex observer behind the same `Seen` type.

### 8. Restart waves (SB-397 R1)

**Target.** An agent or person restarts every running Claude Code session (to load new skills) with
two calls (plan with digest, then apply with a reason), each session ends only between turns, is
resumed as itself (in place in its host, else a Terminal), and the wave watches until each is
`back`; skips are named; the caller goes last. Sources: CONTRACTS "Restart waves (SB-397 R1)";
session service README "R1" and plan R1.0 to R1.7; SCN-099.

**Now.** `rt/restarts.rs` (1,376 lines): `plan_of` (`:130`), `digest` (`:189`), `advance`
(`:220`), `plan` (`:573`), `apply` (`:585`), `status` (`:643`), `cancel` (`:668`), `tick`
(`:709`), `with_launch_id` (`:863`); `Operation::SessionsRestartApply` needs the owner
(`rt/lib.rs:7042-7050`). Tests: `the_plan_restarts_what_can_come_back_and_names_every_skip`,
`a_wave_restarts_between_turns_three_at_a_time_and_the_caller_last`,
`a_session_now_in_another_process_is_skipped_never_signalled`,
`a_busy_host_is_neither_too_old_nor_gone`, `cancel_keeps_a_wave_open_while_a_session_is_ending`.

**Gap.**
- P1: R1.7 open (SB-397): no live wave on the operator's machine has been recorded as evidence.
- P2: sessions with their own waits (`has_waits`), Dashboards plain consoles and Codex sessions are
  skipped, so a "restart everything to load skills" leaves them on old skills (plan "Later":
  re-arming waits through the resume prompt).
- P3: Windows `UNSUPPORTED` (`rt/restarts.rs:47`, `:579`, `:858`).

**Refactor.** Into `core` (W1.2) with the host; waves read sessions from the one `Session` model
(capability 4) instead of `duty::observe` directly.

### 9. Monitors and holds (SB-396 wave C, SB-399)

**Target.** A session's own wait (a wake-up, a monitor, a cron job) and a hold another tool sets
(Fabric's claim) keep it out of every close, duplicate close, idle close and wave until the wait
ends; files are private, carry no prompt or command, and tolerate fields a newer version adds.
Sources: CONTRACTS "A session's own wait" and "A hold another tool sets".

**Now.** In the core: `core/monitors.rs` `apply` (`:63`), `hook` (`:117`), `holds` (`:140`), `hold`
(`:161`), `shown` (`:207`); Switchboard's `rt/monitors.rs` only logs the hold. Tests:
`a_wake_up_or_a_monitor_holds_its_session_until_its_end_and_a_little_after`,
`a_cron_job_holds_its_session_for_six_hours_after_its_last_turn_then_lets_it_go`,
`a_hold_keeps_its_session_until_its_end_and_is_released_with_zero`,
`a_hold_takes_a_short_reason_and_a_newer_field_never_hides_the_waits`.

**Gap.**
- P3: the producer is Claude Code's hooks only; Codex sessions have no waits recorded.
- P3: a cron job's hold ends 6 hours after its last turn regardless of its schedule (a known
  approximation, CONTRACTS).

**Refactor.** Done structurally (in the core). The hook moves to `fabric-sessions hook` (W2.5).

### 10. Recovery after limits (stops, spool, claim, readiness)

**Target.** A session stopped on a limit is recorded once (even while no owner runs), probed on its
backoff, never claimed while its session still runs, offered to its host once an account of its
group is ready, never started twice, and the stop settles when the session goes on by itself;
another host's console resume and Fabric's watchdog read the same state. Sources: CONTRACTS "Agents
keep working through a limit (SB-98)", "Capacity notices and console hand-overs (SB-114)",
"Recovery guards (SB-396 wave D)"; session service W0.4; SCN-052, SCN-053.

**Now.** Storage in `core/stops.rs` (`read` `:88`, `write` `:224`, `finish` `:232`,
`session_has_live_stop` `:287`) and `core/stop_spool.rs` (`spool` `:30`, `waiting` `:50`, `done`
`:88`); logic in `rt/recovery.rs` (2,331 lines: `register` `:29`, `register_stop` `:123`,
`candidates` `:184`, `readiness` `:343`, `claim` `:440`, `status` `:472`); the spool is drained by
`rt/monitor.rs:353-365`. Tests: `a_stop_reported_with_no_owner_is_kept_and_registered_by_the_next_owner`,
`a_stop_whose_session_still_runs_is_never_claimed`,
`a_probe_waits_for_a_known_reset_while_a_fresh_reading_says_full`,
`simultaneous_host_claims_are_serialized_by_the_owner`,
`a_session_that_goes_on_by_itself_settles_its_stop`,
`concurrent_hooks_on_a_full_file_each_keep_their_stop`.

**Gap.**
- P1: SB-114 REQ-5: the owning host resuming its checkpoint and unfinished children on a capacity
  notice is not wired end to end (row open, "REQ-5 host integration and live outcome remain open").
- P2: SB-116: a silent session (stall) is never registered as a stop; the host must say the task is
  unfinished.
- P2: `claim`, `register` and `status` still need the owner (`rt/lib.rs:5782-5830`), so a host's
  claim fails while Switchboard is quit or updating.

**Refactor.** Move `recovery`'s storage half and readiness rules into `core` (W1.2 next step);
readiness asks `Accounts` "which account of this group is ready" (a new D-7 question), so the
service answers claims without Switchboard's store lock.

### 11. Continue the same session (resumable, sessions continue)

**Target.** Any session Switchboard ended (duty close, host idle close, wave), any ended launch with
a session id and any ready stop can be continued as itself (`claude --resume <id>` in its folder and
config folder, with the person's permission choice and the flags it started with), in a Terminal,
in place or hosted, never as a new session and never while it still runs. Sources: CONTRACTS
"Continue the same session (SB-396 wave B, LC-5, LC-7)"; SB-400 paragraph.

**Now.** `rt/resumable.rs`: `record` under `resumable.lock` (`:39`), `locate` (`:105`),
`continue_as` (`:152`; the non-unix stub `:211` refuses); the hosted path writes a record with no
home (`launches::record_resume` `rt/launches.rs:729`) and ends it as `not_started` when the host
never came up (`cli/main.rs:4040-4071`). Tests: `two_writers_at_once_both_keep_every_line`,
`a_session_without_its_history_or_record_is_never_continued_as_a_new_one`,
`an_end_is_kept_and_found_and_the_newest_line_of_a_session_wins`.

**Gap.**
- P2: flags the session started with (model, permission mode, MCP config) are not carried on a
  resume (session service W0.5 row, plan "Later"); only the person's bypass choice is.
- P3: Windows cannot continue a session.

**Refactor.** `resumable` into `core` (W1.2); the resume script built from `Accounts::resume_env`
(capability 2) plus the flags recorded at launch in the session's record.

### 12. Workflow continuation on another account or agent (switchboard continue)

**Target.** A workflow whose executor hit its limit continues on another account or agent with the
Observatory hand-over pack, from the CLI, the window and MCP (`switchboard_continue`), and a
pipeline owner can ask for the next executor without a launch (`launch: false`). Sources: CONTRACTS
"Workflow continuation (SB-52)"; SCN-038; SB-64, SB-74.

**Now.** `rt/continuation.rs`: `plan` (`:371`), `plan_named` (`:400`), `offer` (`:536`), `prompt`
(`:607`), Observatory engine calls bounded (`a_hung_engine_dies_with_everything_it_started`); launch
through `launch_continuation` (`rt/launch.rs:2625`), `continuation_here` (`:2647`). Tests:
`every_refusal_is_decided_before_an_offer`, `a_workflow_moves_to_another_provider_with_its_context`,
`an_offer_whose_pack_lost_a_key_launches_nothing`.

**Gap.**
- P2: SB-64: no window entry and no `switchboard_continue` MCP tool (CLI only), against DEC-0019
  (every window action has an MCP tool, and here a CLI action has none).
- P2: SB-74: `launch: false` for Fabric's pipelines waits on Fabric's ADR.
- P3: the engine lookup is not available on Windows (dispatch there answers `handoff_refused`,
  SB-247 row).

**Refactor.** None structural; expose the existing plan as an MCP tool once SB-74's contract is
fixed.

### 13. Fallback chains and the automatic fallback (SB-71, SB-73, SB-82)

**Target.** A chain (machine, group, workflow scope) names agents in order with optional account or
key pins; when a workflow's executor is limited and no account of its provider is eligible, the
next usable step is judged (installed, quota known for unattended walks, containment never looser,
spend under the ceiling), offered once, launched, and the walk recorded with every candidate passed
over; the old session is never killed; an exhausted chain is reported; the agent's own risk verdict
keeps risky work off a weaker model. Sources: CONTRACTS "Fallback chains (SB-71)", "Automatic
fallback, phase 1 (SB-73)", "Spending ceilings (SB-72)"; DEC-0002; SCN-039, SCN-040, SCN-080 to
SCN-082.

**Now.** `crates/switchboard-core/src/chains.rs` (`set_chain` `:196`, `chain_for` `:263`);
`rt/fallback.rs` (5,362 lines): `resolve_with` (`:133`), `executor_limited` (`:315`),
`executor_containment` (`:387`), `pass` (`:462`, scan every 60 s, idle 5 min, retry 15 min,
`:27-34`), `queue_handover` (`:630`), the spend gate (`spend_gate` `:1317` to `gate_pinned`
`:1665`); `rt/walks.rs` (`append` `:111`); `rt/chain_view.rs` (`readiness` `:194`). Called from
`rt/monitor.rs:323`. Tests: `a_limited_executor_hands_its_workflow_to_the_next_agent_of_its_chain`,
`a_missing_binary_is_passed_over_before_the_offer_and_the_next_step_runs`,
`a_step_that_asks_less_than_its_executor_is_passed_over`,
`a_board_requests_workflow_is_never_the_fallbacks`,
`two_hand_overs_racing_for_the_last_dollar_are_decided_one_after_the_other`,
`a_walk_names_each_passed_over_candidate_once_in_order_with_its_probe`.

**Gap.**
- P1: SB-96: a chain can pin any project's paid key; it must pin only keys the project's policy
  names (waits on Observatory's grant registry).
- P2: SB-73 phase 2: a Hermes executor's own limit, Kimi Code on a paid key, live acceptance on a
  capped key.
- P2: SB-82 rest: one preference source with Fabric's fallback order (Fabric ADR-0125).
- P2: SB-123: the outgoing agent's risk verdict at the hand-over is not modelled.
- P2: structure: about 60 percent of `fallback.rs` tests are spend-gate tests (`:3229-5335`); the
  gate's budget logic is shared with hand-overs and dispatch through `gate_by_hand` (`:1567`), so a
  change to the fallback walk runs into ceiling code. 29 sleep or timeout sites.

**Refactor.** Split `fallback.rs` into `fallback/{trigger, resolve, offer}` and a `spend_gate`
module of its own (used by fallback, hand-overs, dispatch, agent launches). The walk record and
probes stay; chains move to the contract's runner kinds at the edges as SB-82 designed.

### 14. Hand-overs to consoles (SB-108, SB-405)

**Target.** A hand-over is queued for a console (Fabric Dashboards, Fabric) when *Open hand-overs
in Fabric* is on, taken once by one console, opened in Terminal after two minutes if no console
takes it, gated again for spend when taken, and with `--ask` the taker starts with permission
prompts on. Sources: CONTRACTS "Capacity notices and console hand-overs"; SB-405 row; SCN-054.

**Now.** `rt/handovers.rs`: `record` (`:100`), `pending` (`:138`), `take` (`:167`), `offer_live`
(`:193`), `sweep` (`:226`, called from `rt/monitor.rs:329`); `--ask`: `rt/launch.rs` `ask_args`
(`:3234`), `write_ask_script` (`:3381`). Tests: `a_hand_over_waits_for_a_console_and_is_taken_once`,
`a_hand_over_no_console_takes_opens_in_terminal_after_two_minutes`,
`two_consoles_cannot_take_the_same_handover`,
`a_crash_after_claim_never_reoffers_or_replays_the_pending_record`,
`a_hand_over_taken_with_ask_runs_claude_code_in_manual_mode_without_the_bypass`,
`ask_is_refused_for_another_agent_or_a_script_switchboard_did_not_write`.

**Gap.**
- P2: SB-174: `switchboard --json handovers` still answers slowly at times under load.
- P3: SB-405's row says "in", while the code and tests are on main (release 0.7.112 or later per
  the row): the row needs its receipt.
- P3: a hand-over opened in Terminal is not counted against the session pool (capability 16).

**Refactor.** None beyond moving the spend gate out (capability 13).

### 15. Dispatch for Fabric board requests (SB-247)

**Target.** A board request nobody answers starts the default agent headless in the project's
folder on an account of its group, with the request pack, the exit contract and a time limit; one
run per workflow; the run ends after its report; every refusal is a code with one sentence; runs
are watched (stuck, gone, timed out), stoppable, recorded with `launched_by: fabric`, and on macOS,
Linux and Windows. Sources: CONTRACTS "Dispatch for a board request (SB-247)"; AGENTS.md Lifecycle
"A dispatched run"; SB-407, SB-412.

**Now.** `Operation::Dispatch` (`rt/lib.rs:6400`), `dispatch_start` (`rt/lib.rs:5103`), the gate
order ending with the session pool (`rt/lib.rs:5212-5220`), then account, spend gate and launch;
`rt/dispatch.rs`: refusal codes (`:26-48`), `choose` (`:139`), `prompt` (`:223`), `watch` (`:398`),
`stop` (`:465`), `record_gone` (`:626`), `status` (`:671`), `sweep` (`:726`, from
`rt/monitor.rs:350`). Tests: `a_dispatch_starts_once_headless_records_fabric_and_stops_its_process_group`,
`every_refusal_reachable_here_starts_nothing`,
`a_run_read_as_gone_is_only_recorded_and_a_live_one_is_never_signalled`,
`a_stop_cut_between_its_record_and_its_signal_is_finished_by_the_next_pass`,
`an_inbox_run_settles_by_read_marks_not_by_a_request`.

**Gap.**
- P1: board work starves behind interactive sessions (capability 16).
- P2: Windows: the Observatory engine lookup is missing, a dispatch answers `handoff_refused`
  (SB-247 row).
- P2: a dispatched run is not a hosted session: nobody can attach to watch it, and it is an owner
  child (AGENTS.md Lifecycle row), so its process group outlives a quit but its watch does not
  until the next owner starts.
- P3: SB-300: `a_run_that_ends_by_itself_is_recorded_exited` flaky under load (`unwrap` on a
  missing `launch_id`, `rt/dispatch.rs:1034` per the row).

**Refactor.** Dispatch stays in Switchboard (session service D-4: accounts and spend are
Switchboard's), but its run starts through `host.start` once W2.4 exists, so the service watches it
and a viewer can attach. The capacity decision moves into one admission call (capability 16).

### 16. Board work under the session pool (capacity SB-378, refusals SB-412)

The operator's complaint of 2026-11-17: "session managers seem not to start; tasks arrive and
wait". Measured cause (README "Operator's additions" 3): the cap in `capacity.json` was 20 while 34
sessions counted, most of them interactive, so every board dispatch was refused `at_capacity` with
no end in sight; the cap was raised to 40 at 17:58 CET (used 38, free 2) as a stopgap. 40 with 38
used will starve again with two more interactive sessions.

**Target.** Board work never starves behind interactive sessions:
- a share of the pool is reserved for board runs (dispatch, inbox runs, hand-over takers started by
  Fabric), so an interactive surplus cannot take the last slot from the board; interactive starts
  wait at the interactive limit, board starts at the board limit, and the sum stays under the
  machine cap;
- a session counts only while it does work: an interactive session past its own quiet time (its
  own transcript not written for the quiet minutes and no command of its own running, the same
  facts the duty officer reads) is counted out, whatever other sessions in the same folder write;
  a session on a limit stop or a wait of its own is counted out;
- the wait is visible: a refused start says how many board starts wait, since when, the reason
  (`interactive_full`, `board_share_full`, `machine_full`) and what would free a slot, in the
  refusal, in `switchboard capacity`, in Needs attention and on the Sessions page;
- Fabric is told when a slot frees (a board message or a socket event the dispatcher reads), so it
  retries at once instead of on its next pass; with `--target auto` the session pool of each
  candidate machine is part of the choice;
- every launcher counts against the same pool, including Terminal starts from the window, the
  fallback and hand-overs, so the numbers the board sees are true.

Sources: SB-378 row and CONTRACTS "Session pool (SB-378)" (`docs/CONTRACTS.md:136`), "Attention
(SB-365)" `dispatch_refused` row (`docs/CONTRACTS.md:3219`), README addition 3, AGENTS.md Writes
(`capacity.json`, `dispatch-refusals.json`).

**Now.**
- Counting: `core/capacity.rs` `used` (`:58-63`) counts every row of `machine_sessions::scan` not
  `quiet`; `quiet` is set in `core/machine_sessions.rs:362-370` from the newest transcript write of
  the process's *folder* (`last_write` of the folder, `QUIET_MINUTES` = 30, `:20`), so an idle
  interactive session in a folder where any other session writes counts as working; managed and
  unmanaged, interactive and board runs, sessions on a limit stop and sessions in a wait all count
  the same. `decide` (`:90-99`) refuses at `used >= cap`; an unread process table admits.
- Refusal: `dispatch_start` checks the pool after the target choice and before choosing an account
  (`rt/lib.rs:5212-5220`): `dispatch::refused(.., AT_CAPACITY)` with `used` and `cap`, and
  `dispatch_refusals::record` (`rt/dispatch_refusals.rs:57`). `launch --hosted` and
  `sessions continue --hosted` call `capacity::admit` (`cli/main.rs:3970`, `:4036`) and fail with
  `AT_CAPACITY` (`core/capacity.rs:23`). Window, fallback and hand-over Terminal starts are not
  gated.
- Visibility: `dispatch_refusals::observe` (`rt/dispatch_refusals.rs:78`, called from the watch
  `rt/session_watch.rs:448`) notes `freed_at` once a scan has room; `last_hour` (`:101`) feeds
  `switchboard capacity`'s `refused_last_hour` (`rt/lib.rs:7164`) and one Needs attention item
  `dispatch_refused` with `count`, `newest_request`, `freed`, `freed_at` (`rt/attention.rs:294-310`).
- Retry: Fabric keeps the queue (CO-588) and "reads the pool each pass and takes `at_capacity` as a
  wait" (`docs/CONTRACTS.md:136`); Switchboard sends nothing when a slot frees (`freed_at` is read
  only by `attention.rs:305-306`; no board message, no event). `--target auto` compares quota room
  only (`rt/remote_dispatch.rs:57-79`), never the candidate's session pool.
- Tests: `capacity::tests::a_quiet_session_does_not_count_and_the_cap_refuses_the_start_that_would_pass_it`,
  `an_unread_process_table_admits_the_start`,
  `dispatch_refusals::tests::each_refusal_is_counted_and_only_the_last_hour_is_reported`,
  `room_in_the_pool_after_a_refusal_is_recorded_once_and_a_new_refusal_clears_it`,
  `attention::tests::dispatches_refused_at_the_session_limit_are_one_item_with_their_count`.

**Gap.**
- P0: no reserved share: interactive sessions fill the pool and board starts are refused for as
  long as they stay (observed 2026-11-17, live).
- P1: quiet is per folder, not per session (`core/machine_sessions.rs:361-370`), and waits, holds
  and limit stops are not counted out, so the count overstates load; the duty officer already has
  per-session facts (`duty::observe`) that the pool does not use.
- P1: Fabric learns of a free slot only by polling; `freed_at` reaches no one but Needs attention.
  How often Fabric's pass re-reads the pool is Fabric's (CO-588) and not verified here.
- P2: the refusal carries no reason class, no queue length and no age of the oldest waiting
  request; Needs attention covers only the last hour (`last_hour`), so a refusal older than an hour
  that still waits drops out of view.
- P2: window, fallback and hand-over Terminal starts bypass the gate, so `used` grows past the cap
  without a refusal anywhere.
- P2: `--target auto` can pick this machine with a full pool while a candidate has free slots.
- P3: the cap file has one number (`Saved {cap}`, `core/capacity.rs:29-33`, `deny_unknown_fields`):
  adding a board share needs a schema change that older readers refuse; a session host of an older
  version reading it would fail closed as "no cap" (`full` treats an unreadable file as no cap,
  `:111-112`).

**Refactor.**
- One admission call in `core`, `capacity::admit(class: Interactive | Board, accounts)`, used by
  every launcher (`LaunchPlan` of capability 1 names the class), answering
  `{admitted, reason, used_by_class, limits, waiting}`.
- `capacity.json` v2: `{cap, board_share | board_min}` with a reader that ignores unknown fields
  (as `monitors` already does, `a_hold_takes_a_short_reason_and_a_newer_field_never_hides_the_waits`)
  and a writer that keeps v1 readable.
- Count per session from the one `Session` model (capability 4): own transcript time, own commands,
  stop and wait state.
- `dispatch-refusals.json` becomes the board queue's mirror (each waiting request with its first
  refusal), and a slot freeing emits one event that Fabric reads (W2.4 socket `event`, or a board
  `com.submit` from Switchboard until the service exists), so Fabric's retry is event-driven with
  its pass as the fallback.

### 17. Remote dispatch and sessions on other machines (SB-317, SB-332, SB-380)

**Target.** A board run can run on another machine of the person's (a fresh worktree there, result
back, status read, stopped, its time limit enforced there); sessions there are listed, attached and
ended from here; acts that end a process are allowed only on a machine that proved its key; a
finished run never reads `lost`. Sources: CONTRACTS dispatch "On another machine (SB-317)",
"Sessions on another machine (SB-332)", "Reading, ending and attaching sessions on another machine
(SB-380)"; DEC-0013, DEC-0017; `docs/design/remote-dispatch.md`.

**Now.** `rt/remote_dispatch.rs` (`start` `:188`, `status` `:300`, `stop` `:350`, `capacity`
`:57`, `choose` `:72`), `dispatch_remote` (`rt/lib.rs:5315`), `rt/targets.rs` (`known` `:14`,
`collapse` `:88`, `launch_ssh` `:209`, `command_ssh` `:232`), `SessionEnd` on another machine through
`machine_proof::ssh` (CONTRACTS Sessions page). Tests:
`a_run_there_starts_in_a_fresh_worktree_and_its_result_comes_back`,
`a_gone_run_reads_its_exit_code_and_never_a_half_written_one` (SB-415),
`a_run_past_its_time_limit_stops_itself_and_keeps_its_worktree_with_work_in_it`,
`the_machine_with_the_most_room_runs_the_request`, `one_account_named_two_ways_is_one_machine`.

**Gap.**
- P2: `--target auto` ignores the candidates' session pools (capability 16).
- P2: SB-357 in review: only `SessionEnd` is behind the machine proof; `sessions ... --target` reads
  and `attach --target` trust whatever ssh reaches (CONTRACTS SB-357 note: "Reads only; no proof is
  asked").
- P3: SB-361: `remote_dispatch` tests waited on a 20 s bound, moved to 180 s backstops (in review);
  SB-415 in review per the row though merged at `f5e2c77e`.

**Refactor.** None structural; the remote run's state machine (`running`, `exited`, `stopped`,
`timed_out`, `lost`, `unknown`) should be one enum shared with local dispatch records.

### 18. The agents catalog and other agents (Hermes, OpenCode, Kimi Code, Pi, catalog agents)

**Target.** Every agent in the contract's catalogue is known with its install check, launch
profile, headless form, permission mode and route (subscription, OpenRouter key, Anthropic key,
Kimi account); installed agents are listed; the person enables, disables and picks a default; an
agent launches on a key without the key on a command line; Hermes, OpenCode and Kimi Code take a
workflow over with the hand-over pack. Sources: CONTRACTS "Other agents (0.6)", "Installed agents,
switching and the default agent", "Hermes's model and provider", "Kimi Code accounts", "Agent
keys"; SCN-036, SCN-045 to SCN-048, SCN-057, SCN-058, SCN-060, SCN-097, SCN-101.

**Now.** `catalog/agents.json` (30 agents, among them `hermes` line 7, `pi` 438, `kimi-code` 694,
`opencode` 870) read by `rt/agent_catalog.rs` (`catalog` `:34`, `launchable` `:179`, `containment`
`:71`); `rt/agent_settings.rs` (`installed` `:92`, `set_default` `:158`, `bypass` `:50`);
`rt/hermes.rs` (`status` `:200`, `set` `:205`); `rt/kimi.rs` (`launch` `:751`, `watch` `:417`,
`launch_continuation` `:884`); launches in `rt/launch.rs` (`launch_agent` `:1652`,
`launch_agent_openrouter` `:1948`, `launch_agent_anthropic` `:1991`, `hermes_continuation` `:2113`,
`opencode_continuation` `:2277`). Tests: `the_catalog_agrees_with_the_contracts_catalogue`,
`every_openrouter_recipe_is_complete_and_never_puts_the_key_on_a_command_line`,
`a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key`,
`an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key`,
`a_kimi_step_runs_on_a_usable_kimi_account_pinned_or_least_used`,
`hermes_on_the_pay_per_token_group_runs_through_the_proxy`.

**Gap.**
- P2: only Claude Code is watched by the duty officer, restarted by waves and resumed as itself;
  Codex, Kimi, Hermes and OpenCode sessions are listed but never continued after a stall or a quit
  (capabilities 7, 8, 11).
- P3: several catalogue agents say "Launch by Switchboard waits until its key variable and base
  path are verified" (`catalog/agents.json:330`, `:588`): listed, not launchable.
- P3: Kimi and Hermes home layouts (`kimi/sessions`, `runtimes/hermes-continue`,
  `runtimes/opencode-continue`) are special-cased in `launch::sessions` (`rt/launch.rs:3741-3747`).

**Refactor.** One `AgentProfile` from the catalogue (binary, headless form, resume form, permission
flags, home layout, how its sessions are observed) used by launch, the session model, duty and
waves, instead of per-agent branches in `launch.rs`.

### 19. Fabric Sessions: the service apart from every app (SB-397)

**Target.** One per-user service, its own repository and release, installed by whichever product
starts first, owns hosts, the duty officer, the watch, monitors, waves, Continue, limit stops and
the session records; quitting or updating Switchboard, Fabric or Fabric Dashboards ends no session
and loses no record; Switchboard answers it through `Accounts` only. Sources: session service README
and plan W1 to W4; AGENTS.md "What this repository is" (the two crates, `check.sh` guard); SL-6,
SL-7.

**Now.** In the core (`core/lib.rs:6-34`): `accounts`, `capacity`, `cli`, `events`, `ids`,
`machine_sessions`, `monitors`, `person`, `private_fs`, `session_rows`, `stop_spool`, `stops`,
`terminal`, `windows` (W1.2 a to h, PRs #787 to #796). Still in `switchboard-runtime`: `session_host`,
`session_watch`, `duty`, `restarts`, `resumable`, `recovery`, `launches` (records), the dispatch
watch. The binary only names itself (`crates/fabric-sessions/src/main.rs:6-33`). `Accounts` answers
five questions (`core/accounts.rs:9-38`); `Events` carries log events only
(`core/events.rs:24-33`). Thin re-export shims in runtime: `rt/session_rows.rs`,
`rt/capacity.rs`, `rt/monitors.rs`, `rt/machine_sessions.rs`, `rt/stop_spool.rs`.

**Gap.**
- P1: W1.2 rest (`recovery`'s storage half, `resumable`, `duty`, `restarts`, `session_host`,
  `session_watch`) and W1.4 (Switchboard on the core, its copies deleted) are not done; until W2
  ships, a Switchboard quit still stops the duty officer, the watch, waves, claims and record
  writes (capabilities 5, 10).
- P2: the session modules still take Switchboard's data folder as the core's root (W1 rule), and
  the host socket name depends on it (capability 3), which W2.2 must migrate without losing live
  hosts.
- P3: shims add a layer per module; each must be removed at W1.4.

**Refactor.** Follow plan order: W1.2 rest in this order, `recovery` storage, `resumable`, `duty`,
`restarts`, `session_host`, `session_watch` (each with its tests moved under the same names and
the W1.0 goldens byte for byte), then W1.4; add `Accounts::resume_env`, `Accounts::ready_account`
and an `Events::session` writer before `duty` and `recovery` move, since both need them. W2's
socket and spool replace `Operation::SessionEnded`, `LaunchEvent` and `RecoveryHook` as the
writers' path.

---

## Summary

| # | Capability | State | Gap severity | Backlog ids |
|---|---|---|---|---|
| 1 | Launching (managed, isolated, in place, headless, hosted, Terminal) | partial | P2 | SB-354, SB-02 |
| 2 | Homes and what they carry | done | P2 | (W2.8) |
| 3 | Session hosts and attach | partial | P2 | SB-342, SB-402, SB-413 |
| 4 | Session list and rows | partial | P2 | SB-411, SB-407, SB-197, SB-353 |
| 5 | Launch records, homeless launches, hook events | partial | P2 | SB-231, SB-387, SB-397 (W0.5) |
| 6 | Session watch | done | P2 | SB-379 |
| 7 | Duty officer | partial | P2 | SB-384, SB-407 |
| 8 | Restart waves | partial | P1 | SB-397 (R1.7) |
| 9 | Monitors and holds | done | P3 | SB-396, SB-399 |
| 10 | Recovery after limits | partial | P1 | SB-114, SB-116 |
| 11 | Continue the same session | partial | P2 | SB-397 (W0.5) |
| 12 | Workflow continuation | partial | P2 | SB-64, SB-74 |
| 13 | Fallback chains and automatic fallback | partial | P1 | SB-73, SB-82, SB-96, SB-123 |
| 14 | Hand-overs to consoles | done | P2 | SB-405, SB-174 |
| 15 | Dispatch for board requests | partial | P1 | SB-247, SB-300, SB-407 |
| 16 | Board work under the session pool | broken | P0 | SB-378, SB-412 (new rows needed) |
| 17 | Remote dispatch and remote sessions | partial | P2 | SB-357, SB-361, SB-415 |
| 18 | Agents catalog and other agents | partial | P2 | SB-73 |
| 19 | Fabric Sessions crates and service | partial | P1 | SB-397 |

Counts: done 4, partial 14, broken 1, missing 0.

Top gaps for the waves:
1. P0 (16): no board share in the session pool; board starts starve behind interactive sessions.
2. P1 (16): the pool counts per folder and counts waiting, held and stopped sessions; Fabric is not
   told when a slot frees; Terminal starts bypass the gate.
3. P1 (19, 5, 10): the session lifecycle still lives in the Switchboard owner (W1.2 rest, W1.4,
   W2): a quit or update stops the duty officer, waves, claims and record writes.
4. P1 (13): SB-96 chains can pin any project's paid key.
5. P1 (8, 10): R1.7 live wave evidence and SB-114 REQ-5 host integration are open.
