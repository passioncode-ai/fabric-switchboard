# 05 Code structure and the test suite

Part of [the target-state plan](README.md). Measured on branch `target-state` at `f5e2c77e`
(2026-11-17) by reading source text only (`wc`, `grep`, `awk`, a small Python scan of function
spans and module references, `git log -200`). Nothing was compiled or run: the data volume had
2.6 GB free. A function's length is from its `fn` line to the closing brace at the same
indentation; "test lines" are the `#[cfg(test)] mod … { … }` blocks and the files under `tests/`.
The scan's commands are in [Appendix A](#appendix-a-how-the-numbers-were-taken).

Every section ends with its **Target**. Section 10 orders the refactors: test-suite hardening
first, then removal of duplicates, then the structural moves.

## 1. Size

### 1.1 The largest files

| File | Lines | Test lines | Production lines |
|---|---:|---:|---:|
| `crates/switchboard-runtime/src/lib.rs` | 15,655 | 6,171 | 9,484 |
| `crates/switchboard-runtime/src/launch.rs` | 7,558 | 3,726 | 3,832 |
| `crates/switchboard-cli/src/main.rs` | 6,968 | 780 | 6,188 |
| `crates/switchboard-runtime/src/fallback.rs` | 5,363 | 3,648 | 1,715 |
| `crates/switchboard-runtime/src/refresh.rs` | 4,849 | 2,695 | 2,154 |
| `src/main.ts` | 4,675 | 0 | 4,675 |
| `crates/switchboard-runtime/src/external.rs` | 3,853 | 1,813 | 2,040 |
| `crates/switchboard-runtime/src/monitor.rs` | 3,748 | 2,258 | 1,490 |
| `crates/switchboard-runtime/src/spend.rs` | 2,858 | 1,623 | 1,235 |
| `src-tauri/src/main.rs` | 2,446 | 182 | 2,264 |
| `crates/switchboard-cli/src/mcp.rs` | 2,424 | 81 | 2,343 |
| `crates/switchboard-runtime/src/control.rs` | 2,162 | 1,525 | 637 |

All Rust and TypeScript sources together: 176,287 lines. `switchboard-runtime` alone has 86
source files.

### 1.2 The largest functions (production code)

| Lines | Function | Where |
|---:|---|---|
| 3,537 | free `execute` (the operation dispatcher) | `crates/switchboard-runtime/src/lib.rs:5452` |
| 1,552 | `run` (CLI command to operation) | `crates/switchboard-cli/src/main.rs:2379` |
| 844 | `print_result` (CLI text output) | `crates/switchboard-cli/src/main.rs:4970` |
| 465 | `restore` | `crates/switchboard-core/src/backup.rs:497` |
| 464 | `organization_steps` | `crates/switchboard-runtime/src/onboarding.rs:332` |
| 462 | `call_tool` (MCP tool to operation) | `crates/switchboard-cli/src/mcp.rs:1449` |
| 408 | `relay` | `crates/switchboard-proxy/src/lib.rs:651` |
| 345 | `main` (Tauri) | `src-tauri/src/main.rs:1918` |
| 316 | `Runtime::execute` (transaction routing) | `crates/switchboard-runtime/src/lib.rs:1613` |
| 303 | `launch_core` | `crates/switchboard-runtime/src/launch.rs:2862` |
| 282 | `run_with_stop` (background monitor) | `crates/switchboard-runtime/src/monitor.rs:145` |
| 279 | `explain` | `crates/switchboard-runtime/src/attribution.rs:499` |
| 275 | `save_project_with` | `crates/switchboard-core/src/projects.rs:955` |
| 262 | `pass_with_reads` (automatic fallback) | `crates/switchboard-runtime/src/fallback.rs:760` |
| 260 | `send_with` | `crates/switchboard-runtime/src/account_send.rs:276` |
| 250 | `serve` (session host) | `crates/switchboard-runtime/src/session_host.rs:551` |

Of 3,128 production functions, 74 are 100 lines or longer and 21 are 200 or longer.

`src/main.ts` has 259 functions (the largest: `render` 95 lines at `src/main.ts:392`,
`organizationCreateDialog` 75, `projectDialog` 74) and 99 module-level `let` variables
(`src/main.ts:22` to `:216` and on) that hold the whole window's state.

**Target.** No production function over 300 lines; `lib.rs` under 3,000 production lines; the
window's state in one typed store, its pages in their own modules.

## 2. The operation dispatcher

### 2.1 Shape

- `Operation` (`crates/switchboard-runtime/src/lib.rs:121` to `:1225`) has **169 variants**.
- The free `execute` (`lib.rs:5452` to `:8988`, 3,537 lines) is one `match` with **172 arm
  heads**. 88 arms are 8 lines or shorter (a call into a module); **19 arms are 40 lines or
  longer** and carry logic inline: `RecallAccount` 295, `Continue` 225, `SendAccount` 205,
  `MoveRoot` 143, `SaveProject` 141, `OrganizationAssign` 104, `RecoveryHook` 93, `Chains` 93,
  `MoveAccount` 90, `Dispatch` 68, `SettingsImport` 57, `RotatePoolCapability` 56,
  `ProjectEdit` 54, `SessionRows` 53, `CaptureCurrent` 49, and four more.
- `Runtime::execute` (`lib.rs:1613` to `:1928`) decides how an operation runs: a launch gets its
  launch scope (`:1615` to `:1623`), the transaction-free list (`:1639` to `:1784`) runs without
  the owner transaction, `ProjectSuggest` and `Usage` have their own paths (`:1794`, `:1804`),
  a paid start takes the hand-over turn and the spend gate first (`:1825` to `:1890`), and the
  rest runs under `self.mutations`.

### 2.2 The transaction-free list and why it is fragile

`lib.rs:1639` to `:1784` is a `matches!` over **92 of the 169 variants (54 %)**. A variant not
named there takes the owner transaction. Four properties make it fragile:

1. **The default is silent.** A new read operation compiles and works, then waits behind
   renewals and syncs on a busy machine. The comments in the list are the incident history of
   that default: FD-71 (a console's read waited up to 366 s, `lib.rs:1749`), SB-166 (12 to 15 s
   for a host's recovery read, `:1755`), SB-150 (a `StopFailure` hook timed out, `:1759`),
   SB-212 (`:1766`), SB-186 (`ProjectSuggest`, `:1787`), SB-39 (`Usage`, `:1801`). SB-410 is
   the same class one level down (a read that waited on the Keychain).
2. **"Read-only" is the wrong name.** Several listed operations write: `VestaConnect`,
   `MachineTrust`, `HandoverTake`, `RecoveryHook`, `LaunchEvent`, `SessionEnded`,
   `DispatchStop`, `InboxSet`. What the list means is "serialises its own writes with its own
   lock or file", and nothing checks that claim per variant.
3. **No exhaustiveness.** No test enumerates the variants; the classification lives 1,400 lines
   from the enum and 3,800 lines from the arm that implements it.
4. **The offline path does not use it.** `execute_offline` (`lib.rs:4815`) opens the store per
   call and runs every operation the same way, so the two paths can drift.

### 2.3 Per-operation facts in eleven places

| Fact about an operation | Where it is decided |
|---|---|
| Runs without the owner transaction | `lib.rs:1639` (`matches!`, 92 variants) |
| Is a launch (gets a launch record) | `is_launch`, `lib.rs:4567` |
| Is reported to analytics, and how | `Reported::of`, `lib.rs:4396` |
| Is a loosening the operator's words may open | `operator_action`, `lib.rs:4645` |
| Is a seat move | `seat_operation`, `lib.rs:4795` |
| Is refused unless the window or operator asks | `window_only_refusal`, `lib.rs:4614` |
| Is rewritten to the operator's surface | `as_operator`, `lib.rs:4804` |
| How long the control listener waits for it | `limit_secs`, `crates/switchboard-runtime/src/control.rs:433` to `:463` |
| Its kind name in logs | `operation_kind`, `control.rs:380` (serialises the whole operation to JSON to read its tag, then interns it in a static) |
| Whether the MCP tool writes | the `bool` beside each of 106 tools, `crates/switchboard-cli/src/mcp.rs:129` |
| Whether a CLI command runs long | `keeps_running`, `crates/switchboard-cli/src/main.rs:6011` |

### 2.4 The same operation on four surfaces

Adding one operation touches the enum and an arm (`lib.rs`), a clap command, a branch of `run`
and of `print_result` and a `PARITY` row (`crates/switchboard-cli/src/main.rs`), a tool and a
`call_tool` arm (`mcp.rs`), a Tauri command and its `generate_handler!` row (126 commands in
`src-tauri/src/main.rs`) and its `PARITY` row (`src-tauri/src/parity.rs`), the adapter
(`src/adapter.ts`), the demo adapter (`src/demo.ts`, 1,060 lines), the types (`src/types.ts`)
and the strings (`src/locales/ru.ts`). Of the last 200 commits (2026-11-11 to 2026-11-17),
**17 touched all five of** `lib.rs`, CLI `main.rs`, `mcp.rs`, `src-tauri/src/main.rs` and
`src/adapter.ts`, and 29 more touched three or four.

The parity tests (`crates/switchboard-cli/src/main.rs:6305`, `src-tauri/src/parity.rs`) check
that names exist on both sides. They do not check that the answers agree (section 4.4).

**Target.** One `operation.rs` holding the enum and one exhaustive `impl Operation` with every
fact in 2.3 as a method (`match` with no `_` arm, so a new variant does not compile until it is
classified); `execute` a router of one line per arm into domain modules; the transaction-free
property named for what it is (`own_lock`) and its claim tested per variant.

## 3. Module boundaries and cycles

### 3.1 Crates

The crate graph is a DAG: `fabric-sessions-core` has no internal dependency
(`crates/fabric-sessions-core/Cargo.toml:12`, D-2); `switchboard-core` and
`switchboard-runtime` depend on it; `switchboard-proxy` on `switchboard-core`; the CLI and the
Tauri app on `switchboard-core` and `switchboard-runtime`. `scripts/check.sh:22` fails the gate
when `cargo tree -p fabric-sessions-core` names a `switchboard-*` crate.

### 3.2 Inside `switchboard-runtime`

Production references between its 84 modules (counting `crate::<module>` from a module and
`<module>::` from `lib.rs`, test blocks excluded): **351 edges, 42 mutual pairs, and one strongly
connected component of 66 modules** that includes `lib.rs`. Only 17 modules sit outside it.

The component is that large because `lib.rs` does four jobs and the modules reach back into it:

1. the `Operation` type (`crate::Operation` used from 11 places outside `lib.rs`);
2. the dispatcher (`crate::execute` 7, `crate::execute_offline` 4, `crate::launched` 3);
3. the owner: `Runtime` (`lib.rs:1339`), `start_proxy` (`:3476`), `start_with_sources`
   (`:3664`), `shutdown` (`:3768`) and the background passes `bindings_pass` (`:1986`, 152
   lines), `suggest_pass_with_stop` (`:2320`), `absorb_statistics` (`:2447`), `vanish_pass`
   (`:2507`, 126), `maybe_backup` (`:3189`), `balance_pass` (`:3289`);
4. shared helpers (`default_root` 9 uses, `NativeSources` 8, `AgentVia` 8, `no_swap` 8,
   `UNAVAILABLE` 9).

Fan-in: `oplog` 30, `launch` 28, `monitor` 16, `launches` 13, `spend` 12. `launch.rs` is a
utility drawer as much as the launcher: the session modules use it for `find_program`,
`write_script_at`, `start_terminal`, `bypass_args`, `project_allows`, `headless_path`,
`SCRIPT_SHELL` (section 6). Mutual pairs that matter for the moves ahead: `launch`/`launches`,
`launch`/`monitor`, `launch`/`fallback`, `lib`/`monitor`, `lib`/`refresh`, `dispatch`/`duty`,
`duty`/`restarts`, `duty`/`resumable`, `duty`/`session_watch`, `restarts`/`resumable`,
`launches`/`session_host`, `launches`/`session_watch`.

**Target.** `lib.rs` only declares modules and re-exports; the owner in `owner.rs`, the type in
`operation.rs`; utilities (`proc`, `ssh`, `terminal`, `paths`) depended on and depending on
nothing above them; the session cluster acyclic so it can leave the crate.

## 4. Duplicated logic

### 4.1 The process table, read six ways

| Reader | Where | How |
|---|---|---|
| `machine_sessions::processes` | `crates/fabric-sessions-core/src/machine_sessions.rs:176` | `proc_listpids`, `proc_pidinfo`, `/proc` |
| `duty::process_table` | `crates/switchboard-runtime/src/duty.rs:340` | spawns `/bin/ps -axww` |
| `external::process_lines` | `crates/switchboard-runtime/src/external.rs:1180` | `proc_listpids`, `KERN_PROCARGS2`, `/proc` |
| `launch::process_age` / `process_age_from_ps` | `crates/switchboard-runtime/src/launch.rs:1157`, `:1211` | `proc_pidinfo`, `/proc`, then `/bin/ps` |
| `launch::runs_script_of` | `launch.rs:1371` | spawns `/bin/ps -o command=` |
| parent program name | `crates/switchboard-runtime/src/launches.rs:159` (`caller_program`) | `proc_name`, `/proc/<pid>/comm` |

(`crates/switchboard-proxy/src/peer.rs:60` also walks `proc_listpids` to find a socket's peer;
that is a different question and may stay.) Liveness by `kill(pid, 0)` is written in six files,
four of them as their own function: `launch.rs:1203` `pid_alive`, `session_host.rs:215`
`pid_alive`, `duty.rs:876` `process_alive`, `bugs.rs:54` `alive`. The `ps` reader in `duty` is
the one AGENTS.md's Lifecycle table says is avoided elsewhere ("read in-process, no `ps`").

### 4.2 ssh, built five ways

`account_send::ssh_runner` / `ssh_runner_for` (`crates/switchboard-runtime/src/account_send.rs:1016`,
`:1020`) is the batch transport with bounded pipes, used from six modules (`account_send`,
`dispatch`, `lib`, `machine_proof`, `machines`, `remote_usage`) although it lives in a feature
module. Beside it: `targets::ssh_words` (`targets.rs:255`, interactive, its own
`ConnectTimeout=15`), `targets::ssh_identity` (`targets.rs:126`, spawns `ssh -G` itself),
`remote_dispatch::ssh` (`remote_dispatch.rs:26`, the `/bin/sh -c` wrapper), and two direct
`Command::new("ssh")` in the CLI (`crates/switchboard-cli/src/main.rs:4303`, `:4309`).
`machine_proof::ssh` (`machine_proof.rs:232`) adds the key proof around a call.

### 4.3 Session and launch files

Launch records are read through one API (`launches::records`, `launches.rs:540`;
`recent_records`, `:546`), which is right. The session tree, however, crosses from
`session_watch` (`session_watch.rs:228`) to the core's row builder
(`crates/fabric-sessions-core/src/session_rows.rs:99`) as an untyped `serde_json::Value`, so a
renamed field fails at run time only. Claude Code's `sessions/` folder is read in five modules
and its `projects/` folder in ten (`fallback`, `limits`, `carry`, `shell/homes`,
`machine_sessions`, `attribution`, `monitor`, `remote_check`, `resumable`, `uninstall`), each
joining its own paths.

### 4.4 JSON built by hand, three times per answer

`switchboard-runtime` has 1,324 `json!` invocations and 2,758 `["field"]` indexings against 111
`#[derive(Serialize)]`; the CLI has 203 and 694 with no derive. The same views are built
separately per surface:

- **status:** MCP composes `Snapshot`, `CurrentAccounts` and `MonitorStatus` itself
  (`mcp.rs:386`); the CLI and the window compose their own.
- **accounts:** MCP builds rows with organizations (`mcp.rs:449`, adding `Organizations`); the
  CLI prints from `Snapshot` (`main.rs:2652`, then `print_result`); the window builds its rows
  in `src/ui-logic.ts`.
- **usage:** MCP (`mcp.rs:564`) and the CLI (`main.rs:2810` maps `usage` to `Snapshot` and
  formats it) each derive the remaining quota from the snapshot.

`Operation::Snapshot` is called 12 times from the CLI and 10 times from MCP. The window's types
(`src/types.ts`, 406 lines) are written by hand; no generator (`ts-rs`, `specta`, `schemars`) is
in any manifest.

**Target.** One `proc` module and one `ssh` module, each with one implementation per OS; typed
view structs in the runtime (`views::status`, `views::accounts`, `views::usage`, the session
tree) that every surface serialises; TypeScript types generated from them.

## 5. Hidden shared state

- **115 `static` items** across the crates (test and production). Production globals that carry
  state between unrelated callers: `fallback::HAND_OVER` (`fallback.rs:1080`, one
  `tokio::sync::Mutex` per process; its own comment at `lib.rs:4835` says a test that holds it
  must hold it briefly), `launches::WRITES` (`launches.rs:278`), the record cache `RECORDS`
  (`:371`), `PRUNED_AT` (`:1511`), `session_watch::LAST_READ` (`session_watch.rs:245`),
  `LAST_TICK` (`:404`), `RUNNING` (`:406`), `control`'s `KINDS` (`control.rs:384`).
- **Implicit parameters through task-locals:** `launches::SCOPE` (who launched,
  `launches.rs:219`) and `fallback::GATED` (`fallback.rs:1087`) change what `execute` does
  without appearing in its signature.
- **Test seams in production code:** 8 `TEST_*` task-locals (`TEST_GATE_BUDGET`,
  `TEST_READ_DELAY`, `TEST_READ`, `TEST_TAKE_TURN_WAIT`, `TEST_HERMES_READY`,
  `TEST_MISSING_BINARIES`, `TEST_OBSERVATORY`, `TEST_DISPATCH_PROGRAM`) and **85 `#[cfg(test)]`
  attributes inside the production code** of `switchboard-runtime` (170 in all, 85 of them on
  test modules): `fallback.rs` 12, `launch.rs` 10, `refresh.rs` 9, `oplog.rs` 9, `suggest.rs` 8,
  `launches.rs` 7, `lib.rs` 6. The release binary and the tested binary differ at those points.
- **Environment:** 75 `env::var` reads, no `set_var` or `remove_var` anywhere (good: tests do not
  race on the environment). Test switches read from the environment:
  `SWITCHBOARD_TEST_NO_PROCESSES`, `SWITCHBOARD_SYNTHETIC_LOCK_PROBE`,
  `SWITCHBOARD_SECRET_SERVICE_TEST`, `SWITCHBOARD_GOLDEN_UPDATE`.
- **Time:** 158 calls to `monitor::now()` or `clock::now()`, 35 `SystemTime::now()` and 88
  `Instant::now()` in production code; 121 functions already take `now: i64`. No test uses
  `tokio::time::pause` or `start_paused` (0 hits), so every async deadline in a test is wall time.
- **Blocking work:** `blocking::run` (`crates/switchboard-runtime/src/blocking.rs:6`) uses
  `block_in_place` on a multi-thread runtime and runs inline on any other, so a test on the
  default single-thread test runtime exercises a different scheduling than production. It is
  called 138 times (with `spawn_blocking`).

**Target.** No process-global mutable state outside an owner object; the launch scope and the
gate passed as values; one `Clock` the owner, the gates and the session watch take (a test clock
in tests, no `cfg(test)` branches in production paths); test seams as constructor arguments.

## 6. The Fabric Sessions boundary

Done (W1.2 steps a to h): the core holds 4,181 lines; Switchboard keeps thin shims
`machine_sessions.rs` (39 lines), `capacity.rs` (22), `monitors.rs` (18), `person.rs` (30),
`stop_spool.rs` (3), `session_rows.rs` (3). The gate enforces D-2 (`scripts/check.sh:22`).

Left in `switchboard-runtime` (production lines): `session_host` 1,156, `duty` 1,040, `restarts`
952, `recovery` 561 (its storage half moves), `session_watch` 506, `resumable` 215. What they
use from Switchboard:

| From | Items |
|---|---|
| `launch` | `write_script_at`, `start_terminal`, `find_program`, `project_allows`, `bypass_args`, `headless_path`, `sessions`, `session_running`, `SESSION_RECORD`, `SCRIPT_SHELL`, `CONFLICTS` |
| `launches` | `new_id`, `valid_id`, `DIR`, `LOGS`, `recent_records`, `record_resume`, `session_of`, `own_home`, `continued_after`, `by_session`, `LaunchedBy`, `NO_SUCH_LAUNCH` |
| other | `oplog` (bridged by `Events`), `monitor`, `own_sign_in`, `shell`, `dispatch`, `dispatch_refusals`, `switchboard_core` (2 to 5 references each) |

They also form a cycle among themselves (`duty`, `restarts`, `resumable`, `session_watch`, and
`launches` with `session_host` and `session_watch`). The plan's order (recovery storage,
`resumable`, `duty`, `restarts`, `session_host`, `session_watch`) cannot move one module at a time
until two seams exist in the core: the launch-record store (ids and records) and the terminal and
script helpers (the core already has `terminal.rs`, 348 lines). The proof for every move exists:
`crates/switchboard-cli/tests/golden.rs` (291 lines) with `golden/sessions-cli.json` and
`golden/sessions-mcp.json`.

**Target.** As the [session-service plan](../2026-11-16-session-service/plan.md) W1.2 to W1.4:
nothing session-owned left in `switchboard-runtime` except the `Accounts` and `Events`
implementations.

## 7. The test suite

### 7.1 Counts

1,656 Rust tests: `switchboard-runtime` 1,157 (336 `#[tokio::test]`, 27 multi-thread),
`switchboard-core` 255, `switchboard-cli` 77, `switchboard-proxy` 76, `src-tauri` 46,
`fabric-sessions-core` 44, `fabric-sessions` 1; plus 9 Node scripts and the Python unit tests the
gate runs. The runtime's tests run in one process, so they share every global of section 5.

### 7.2 Timing and load

In test code: **103 sleeps, 274 `Duration::from_secs`/`from_millis`, 63 `timeout(`, and 29
assertions on `elapsed()`**. The files with the most: `crates/switchboard-core/tests/storage.rs`
(19 sleeps, 29 timeouts), `lib.rs` (12 sleeps, 8 elapsed assertions), `fallback.rs` (11, 4),
`refresh.rs` (13 sleeps), `suggest.rs` (10), `crates/switchboard-proxy/src/tests.rs` (6, 2),
`session_host.rs`, `control.rs`, `launch.rs`, `crates/switchboard-cli/tests/commands.rs`.

Two shapes fail under load:

1. **An upper bound on wall time**, for example `lib.rs:14168` (`shutdown` within 3 s),
   `lib.rs:14118` (under 1 s), `machines.rs:609` (under 2 s), `github.rs:368` (under 50 ms),
   `handovers.rs:679`, `security_cli.rs:282`, `storage.rs:3354` (under 200 ms),
   `proxy/src/tests.rs:764`. At a load average of 1,000 (the build machine, README of this plan)
   any of these can miss.
2. **A real-time budget racing real-time work.** SB-416,
   `fallback::tests::the_wait_for_the_owner_transaction_is_not_charged_to_a_start_by_hands_gates`
   (`fallback.rs:4748`): a 3 s gate budget (`TEST_GATE_BUDGET`), a 300 ms read delay and a
   holder that keeps the transaction 4 s (`:4766`) on a 4-worker runtime. The budget is charged
   from `tokio::time::Instant` around reads that run through the blocking pool; when the pool and
   the scheduler are starved, the two gates' real time exceeds 3 s and the second gate refuses.
   SB-410 (`control.rs:972`) and SB-415 (`remote_dispatch`) were found the same way and turned
   out to be real defects, so a flake here is first a question, then a bound.

History of the class: SB-07 (open since 2026-09-29, `docs/evidence/backlog.md:15`), SB-410,
SB-413 (a hang, now bounded at 30 s), SB-415, SB-416 (open, no backlog row yet at this commit).

### 7.3 Coverage gaps for refactoring

- The golden answers cover only the session surfaces (two files). Accounts, usage, status,
  groups, chains and ceilings have no byte-for-byte answer fixed for the CLI and MCP.
- No test lists every `Operation` variant (section 2.2).
- No test pins the list of test names, so a move that drops a test module goes unnoticed.

**Target.** No wall-clock upper bound under a few seconds in any test; budgets and deadlines on an
injected clock; a stress run of the timing-sensitive tests under an artificial load in the
nightly; golden answers for every read surface; a test-name inventory checked by the gate.

## 8. The gate

`scripts/check.sh` (47 lines) runs, in order: brand check, `npm run build`, 8 Node checks,
`cargo fmt --check`, `cargo test --workspace` (`:15`), the MCP tests again under
`ulimit -s 1024` (`:19`), `cargo clippy --workspace --all-targets` (`:20`), the D-2 tree check
(`:22`), `cargo xwin check` for Windows (`:31`, when installed), then 17 Python checks and unit
tests, and `git diff --check`.

Its duration was not measured here (nothing could be built) and is recorded nowhere in the
repository. By structure the cost is three compilations of the workspace (the test profile, the
clippy check, the Windows cross-check), then the runtime's single test binary of 1,157 tests,
whose longest tests wait on real time (a 4 s holder in SB-416's test, 2 s and 8 s windows in
`lib.rs:14134`, 10 s threads in `fallback.rs:4832` and `:5005`). The MCP tests run twice.

**Target.** Each step's time printed by the gate; the runtime's tests split so the slow
real-process tests run as their own binary; the MCP stack check folded into the first run.

## 9. Windows-only compile gaps (SB-406 item 7)

`crates/switchboard-proxy/src/peer.rs:43` declares `mod fdinfo` under
`#[cfg(any(target_os = "macos", test))]` and uses `libc::c_int` (`:49`), while `libc` is a
`cfg(unix)` dependency (`crates/switchboard-proxy/Cargo.toml:21`). The test build on Windows
therefore fails. It went unseen because both Windows checks leave the proxy out:
`scripts/check.sh:31` names `-p switchboard-runtime -p switchboard-cli -p fabric-sessions-core
-p fabric-sessions`, and the nightly's Windows test step (`.github/workflows/nightly.yml:87`)
names the same set with `switchboard-core`. `src-tauri` is not in the local cross-check either.

**Target.** Every workspace crate in both Windows checks; `cfg(test)` code gated on the OS it
needs.

## 10. Churn

In the last 200 commits (2026-11-11 to 2026-11-17), by code lines changed: `lib.rs` 3,631 lines
in 49 commits, `src/main.ts` 2,486 in 39, CLI `main.rs` 2,268 in 48, `session_host.rs` 2,126 in
13, `balancing.rs` 1,953 in 6, `duty.rs` 1,919 in 19, `launch.rs` 1,616 in 21. Seven release
files change together each release (60 times each). The files the refactors split are the files
every open branch touches, so a split must be a pure move landed between other merges.

## 11. The refactor sequence

Rules for every step: one pull request; behaviour unchanged; proved by the full gate, the golden
answers, `cargo test -- --list` before and after showing the same test names (moved tests keep
their names), and for Rust changes the Windows cross-check. Pure moves land alone, announced on
the board, when no branch is mid-review on the same file. Risk is the chance of a behaviour change
that the listed proof would not catch.

### Phase A. Harden the tests first

| # | Step | Proof it changed nothing | Risk |
|---|---|---|---|
| A1 | The gate prints each step's duration; a `scripts/test_names.sh` stores the `cargo test -- --list` inventory and the gate fails when a name disappears without a line in the PR | the gate's own run; the inventory equals today's 1,656 | none |
| A2 | Windows: gate `peer.rs`'s `fdinfo` on `all(unix, any(macos, test))`; add `switchboard-proxy` and `src-tauri` to `check.sh:31` and the nightly's Windows test step (SB-406 item 7) | `cargo xwin check --tests` for every crate; proxy tests unchanged on macOS and Linux | low |
| A3 | Fix SB-416 at its cause: the gate budget charged on an injected clock (`Clock` with a test clock), and the test drives that clock instead of real seconds; open the backlog row first | the test passes under the stress run (A5) 50 times; the paid-start tests of `fallback.rs` unchanged | low |
| A4 | Convert the 29 wall-time assertions: each upper bound either becomes a paused-time test (`start_paused`) where no blocking I/O runs, or asserts the structural fact (no read happened, the deadline path was taken) with a bound of 30 s that only catches a hang | each converted test fails against a deliberately broken build (a removed deadline) before the change lands | low |
| A5 | A stress run: `scripts/stress_tests.sh` runs the timing-sensitive tests (the files in 7.2) N times beside a CPU and blocking-pool hog; nightly job | the job itself; a known-flaky revision (SB-413's parent) fails it | none |
| A6 | Golden answers for every read surface: `Snapshot`, `Status`, `Usage`, accounts, groups, chains, ceilings, attention, health, through the CLI (`--json`) and MCP, beside the session goldens (`crates/switchboard-cli/tests/golden.rs`) | the goldens pass on `main` | none |
| A7 | An exhaustive classification of `Operation`: `fn own_lock(&self) -> bool` as a `match` with no `_` arm replacing the `matches!` at `lib.rs:1639`, and a table test pinning today's 92 variants | the table test equals the current list; control tests (`read_only_requests_answer_while_a_slow_keychain_read_holds_the_keychain`) unchanged | low |

### Phase B. Remove the duplicates

| # | Step | Proof | Risk |
|---|---|---|---|
| B1 | One process-table module in `fabric-sessions-core` (`proc`: list, liveness, start time, command line, parent name) and every reader in 4.1 moved to it, one caller per PR; `duty` stops spawning `ps` | `duty`, `launch`, `external`, `launches` tests; golden session answers; `SWITCHBOARD_TEST_NO_PROCESSES` still empties the table | medium (the `ps` columns `etime`/`time` that `duty` reads must come from `proc_pidinfo`/`/proc` with the same meaning) |
| B2 | One `ssh` module: `ssh_runner` out of `account_send.rs`, the options and the `/bin/sh -c` wrapper in one place, `targets` and the CLI on it | `account_send`, `machines`, `remote_usage`, `remote_dispatch`, `targets` tests; argument lists compared in a unit test | low |
| B3 | Operation facts into one place (2.3): `kind`, `is_launch`, `control_limit`, `operator_action`, `seat_move`, `window_only`, MCP's write flag derived from the operation it calls; `operation_kind` stops serialising the operation | `control::tests` (`limit_secs`, `operation_kind`), the SB-299 loosening tests, MCP read-only server tests | low |
| B4 | Claude Code's own paths (`sessions/`, `projects/`, `.claude.json`) behind one `paths` helper used by the ten modules in 4.3 | the modules' tests; golden answers | low |

### Phase C. Split `lib.rs` (pure moves)

| # | Step | Proof | Risk |
|---|---|---|---|
| C1 | `Operation` and its `impl` to `operation.rs`, re-exported from the crate root | it compiles; test names unchanged | very low |
| C2 | The 6,171 test lines of `lib.rs` into `src/lib_tests/*.rs` by topic, names kept | test-name inventory equal | very low |
| C3 | The owner (`Runtime`, start, shutdown, the six background passes) to `owner.rs` | lifecycle and monitor tests (`an_idle_hour_on_a_fake_clock_stays_inside_the_budget`); golden answers | low |
| C4 | The 19 long arms of `execute` (2.1) become functions in their domain modules, one domain per PR: seats (`RecallAccount`, `SendAccount`, `MoveAccount`), groups (`MoveRoot`, `SaveProject`, `ProjectEdit`, `RotatePoolCapability`), launches (`Continue`, `Dispatch`), organizations, recovery, chains, settings | each arm's tests plus the golden answers; the arm body moved verbatim first, simplified in a later PR if at all | medium (borrowed `store`/`root`/`runtime` lifetimes and early returns change shape) |
| C5 | `execute` becomes a router of one line per arm | golden answers; full gate | low once C4 is done |

### Phase D. One view per answer

| # | Step | Proof | Risk |
|---|---|---|---|
| D1 | Typed views in the runtime (`views::status`, `accounts`, `usage`, the session tree) with `Serialize`; MCP's `status`, `accounts`, `usage` and the CLI's printing read them | A6's goldens byte for byte | medium (key order and null versus absent fields) |
| D2 | `print_result` split per command group into `crates/switchboard-cli/src/print/`; `run` maps commands through per-group functions | CLI goldens; `crates/switchboard-cli/tests/commands.rs` | low |
| D3 | TypeScript types generated from D1's structs; `src/types.ts` keeps only window-local types | `npm run build`; `scripts/test-ui-logic.mjs`; the demo adapter type-checks | low |
| D4 | `src/main.ts` split by page with one state module | the window's scenarios and the Node tests; a Safari pass over every page | medium |

### Phase E. Finish the Fabric Sessions move (plan W1.2 to W1.4)

| # | Step | Proof | Risk |
|---|---|---|---|
| E1 | The launch-record store's primitives (`new_id`, `valid_id`, `DIR`, `LOGS`, reads by session and recency, `record_resume`) into the core, Switchboard re-exporting | `launches` tests under the same names; session goldens | medium |
| E2 | The terminal and script helpers (`write_script_at`, `start_terminal`, `find_program`, `bypass_args`, `SCRIPT_SHELL`, `headless_path`) into the core's `terminal` | `launch` and `restarts` tests | low |
| E3 | Break `duty`/`restarts`/`resumable`/`session_watch` cycles behind `Accounts` and `Events` | the R1 and SB-396 tests unchanged | medium |
| E4 | Move in the plan's order: recovery storage, `resumable`, `duty`, `restarts`, `session_host`, `session_watch` | session goldens byte for byte; D-2 tree check; moved tests keep their names | medium (a host's socket names derive from the data folder) |

### Why this order

Phase A first because every later step is proved by tests, and today a red gate on a loaded build
machine does not say whether the change or the load broke it (7.2). A2 and A7 are also defect
guards in their own right. Phase B before C because each duplicate removed is one fewer place a
split has to carry. C before D because D's views need the domain modules C4 creates. E last
because it depends on B1, B2 and the clock of A3, and its proof (the session goldens) is the one
that already exists.

## Appendix A. How the numbers were taken

- File sizes: `wc -l` over `crates`, `src-tauri/src`, `src`. Test and production split: lines
  inside `#[cfg(test)]` followed by `mod … {` up to the closing brace at column 0.
- Function sizes: a line matching `fn <name>` (with `pub`, `async`, `const` prefixes) to the next
  line that is the same indentation followed by `}`.
- Variants: lines under `pub enum Operation` (`lib.rs:121` to `:1225`) that start a variant at
  four spaces. Transaction-free variants: distinct `Operation::` names in `lib.rs:1640` to `:1784`.
  Arms: lines in `lib.rs:5486` to `:8988` starting `        Operation::`.
- Module graph: production text of each module, `crate::<module>` references (and
  `<module>::` from `lib.rs`), a reference to a non-module crate item counted as an edge to
  `lib`; Tarjan's algorithm for components.
- Timing in tests: regular expressions for `sleep(`, `Duration::from_secs(`/`from_millis(`,
  `timeout(`, `Instant::now()` and `elapsed() <`/`>` inside test regions.
- Churn: `git log -200 --numstat`, `.rs` and `.ts` files.
