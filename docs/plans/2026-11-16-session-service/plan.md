# Fabric Sessions: the plan in waves (2026-11-16, draft 2)

The spec is [README.md](README.md); this file decomposes it. Each task has an id, the repository
that owns it, what it changes and the check that closes it. A wave ends when its exit checks are
green on the artefact the operator uses. Tasks of another repository go there as a board request
the day the wave opens.

Repositories: **SB** fabric-switchboard (the service's crates live here until W2's exit, D-1),
**FS** passioncode-ai/fabric-sessions (from W2's exit), **CT** fabric-agent-contract, **FW**
fabric-workspace, **FD** fabric-dashboards, **FB** fabric (the app), **LN** passioncode (the
launcher), **OB** project-observatory, **AD** fabric-agents-database, **OI** org-index.

Every task re-reads the `file:line` it cites before it starts (the inventory was read at
`af209503`).

## W0. The holes that hurt today, the contract, the decisions

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W0.1 | SB | The duty officer journals an act before it acts: `duty.jsonl {act, phase: intent}` before the SIGTERM, then `closed`, then `resumed`; the drain does not wait; the next owner completes an act whose last phase is `intent` or `closed` (finding 1: `duty.rs` `act_on`, `stop`; `lib.rs` drain 8 s, hard exit 10 s) | a test that ends the owner after `closed`: the next owner resumes the session |
| W0.2 | SB | `resumable::record` under an exclusive lock on `resumable.lock` | **done** on `session-w0` (`a4047abc`): `two_writers_at_once_both_keep_every_line` (without the lock 31 of 200 lines survive) |
| W0.3 | SB | The session watch runs when the account snapshot fails (`monitor.rs`, the `if let Ok(snapshot)` gate) | **done** on `session-w0` (`a4047abc`) |
| W0.4 | SB | A `StopFailure` report with no owner (or with the store held by a draining owner) is spooled in `stop-spool/` and registered by the next owner's pass | **done** on `session-w0`: `a_stop_reported_with_no_owner_is_kept_and_registered_by_the_next_owner`, `stop_spool::tests` |
| W0.5 | SB | Backlog SB-397 (done, `session-service`), rows for W0, a Windows row ("no service on Windows; the core runs in-process"), HANDOFF links this plan | rows under lease |
| W0.6 | CT | Decisions with vectors: the socket API v1 (`hello`, framing, ops, errors), host protocol 2 (`RESTART`), the session row, the restart wave record, the spool event format, the service id and health | a CT decision; SB's gate runs the vectors |
| W0.7 | FW | `lifecycle.md`: SL-6 and SL-7 name Fabric Sessions; SL-7's wording "launchd or systemd directly, or the broker"; `products.md` gains the product; the roadmap and the unified plan's 2.8 point here | pages merged |
| W0.8 | OI, AD | The product's row; the architect's record (agent-host template; host-operations' lifecycle-broker variant: immutable releases, receipt, exit 75, rollback on a failed health check) | rows merged |
| W0.9 | SB | The operator asked once (O-1 name, O-2 lending) through the board's question and the communicator; the defaults hold until he answers; the name freezes the day W1 starts | questions filed |
| W0.10 | SB | Release W0.1 to W0.4 | a release with them |

Exit: W0.1 to W0.4 released; W0.6 decided.

## W1. The core without Switchboard, still inside Switchboard

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W1.0 | SB | Golden answers before anything moves: `sessions --rows`, `sessions` tree, `duty`, `capacity`, `sessions continue` refusals, offline `recovery`, MCP `tools/list` and those tools' answers on a synthetic data root, saved under `crates/switchboard-cli/tests/golden/` (times normalised) | the golden test passes on `main` |
| W1.1 | SB | Crates `fabric-sessions-core` and `fabric-sessions` (binary) in the workspace, with their own version, `scripts/check.sh` covering them, Windows targets compiling (the host is `cfg(unix)`) | the gate and the Windows nightly green |
| W1.2 | SB | Move into the core: `session_host` and attach, `machine_sessions`, `capacity`, `monitors`, `resumable`, `session_rows` (labels and recovery through `Accounts`), `session_watch` (organizations through `Accounts`), the duty officer's observation and decisions, `recovery`'s storage half (`read`, `write`, `session_has_live_stop`, `live_stop_sessions`, the file format), the account-free part of `launches` (lifecycle events, pruning), from `launch` the Terminal and script helpers (`start_terminal`, `write_script_at`, `SCRIPT_SHELL`, `CONFLICTS`, `find_program`, `headless_path`, `SESSION_RECORD`, `home_state`, `sessions`), `private_fs` and `Provider` (re-exported by `switchboard-core`). Two roots as parameters: the service's data and Switchboard's. Events leave through a trait `Events` that Switchboard implements with its oplog | `cargo tree -p fabric-sessions-core` names no `switchboard-*` crate; the moved tests pass under the same names |
| W1.3 | SB | The `Accounts` trait (D-7's four questions plus readiness, labels, organizations of a folder) with the no-Switchboard answers, and Switchboard's implementation over its store | unit tests of both |
| W1.4 | SB | Switchboard uses the core; its copies deleted | W1.0's golden answers byte for byte; the full gate |

Exit: a Switchboard release on the core with no visible change.

## W2. The service runs on its own

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W2.1 | SB | `fabric-sessions serve`: one copy per user (lock; a second exits 75), a drain that never ends a host, `agent-health/1`, the operations digest, a `fabric-service/0.1` descriptor, logs in `~/Library/Logs/Fabric Sessions/` (5 × 5 MB, 0600) | SS-3, SS-10, SS-5 |
| W2.2 | SB | The data folder `~/Library/Application Support/ai.passioncode.fabric-sessions/` (Linux XDG); D-9's ownership: read Switchboard's files as the source until Switchboard writes `sessions-handed-over.json`, then import by id, mirror waits and live stops back while a pre-hand-over host lives, re-import old hosts' `resumable` lines by id | tests per row of the ownership table |
| W2.3 | SB | Adopt hosts: Switchboard's (`<sb data>/sessions/`) and the service's, each by the socket path in its descriptor (`connect` takes a path); a host that died without an end record gets one; host sockets in the per-user runtime folder | SS-2 |
| W2.4 | SB | The socket `service.sock` in the runtime folder (folder 0700, socket 0600), peer uid checked (`getpeereid`, `SO_PEERCRED`), 4-byte length plus JSON up to 1 MiB, `{op, id}` and `{ok}` or `{error: {code, message}}`, 2 s read and 5 s request timeouts; ops `hello`, `rows`, `tree`, `attach`, `end`, `continue`, `event`, `host.start` (provider, folder, config folder, resume id, an environment script that must be a 0600 file under a registered client's data folder, never free argv), `spawn` (D-7), `restart.plan`, `restart.apply`, `restart.status`, `restart.cancel`. The spool `spool/` (0700): one event per file `<unix_ms>-<pid>-<rand8>.json`, `{v: 1, id, event, at, hook}` (hook input up to 64 KiB, no transcript text), at most 5,000 files or 20 MB, applied in name order, idempotent by id; W0.4's `stop-spool/` imported | socket and spool tests; a fuzz of the framing |
| W2.5 | SB | `fabric-sessions hook`: every hook event to the socket or the spool, no vault, exit 0 within 1 s, an unknown event spooled | SS-7; a timing test |
| W2.6 | SB | The duty officer, the watch, monitors, Continue and the idle close run in the service, holding while an older Switchboard owns the store (D-13) | the SB-396 tests in the service; a fake-clock idle hour (LC-08) |
| W2.7 | SB | Switchboard's side of D-13: `hello` at start and each pass; its duty officer and watch stop while the service owns them; its session CLI and MCP forward to the service; `sessions_owner` log events | R-1 tests both ways; a service restart (exit 75) never makes Switchboard take over |
| W2.8 | SB | The `fabric-sessions` plugin (hooks and MCP); the stable entry `~/.local/bin/fabric-sessions`; Switchboard's plugin keeps only `project suggest` on `SessionStart` | one registration per `StopFailure` with both plugins on |
| W2.9 | SB | `fabric-sessions mcp` (stdio; exits on end of input; `stale` after an update) with the tools of the README's table; Switchboard's moved tools forward and say `deprecated` | MCP tests; the tools reference page |
| W2.10 | SB, FS | Freeze socket API v1 and move both crates with history to `passioncode-ai/fabric-sessions`; Switchboard depends on its tag | the FS gate green; SB's gate green on the tag |

Exit: on the MacBook the service runs under a LaunchAgent installed from a build; quitting and
updating Switchboard ends no session process and loses no record (SS-1 for Switchboard).

## W3. Accounts through one interface

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W3.1 | SB | `switchboard sessions env --session S` (and the socket call behind it): Switchboard prepares the resume environment (a fresh token for an isolated home, the proxy route for a managed one, Codex routes) as a 0600 script and answers its path; the service never reads `launch.command` or a home's `settings.json` | an isolated home resumed by the service gets a fresh token |
| W3.2 | SB, FS | Readiness: Switchboard answers which accounts may resume a stop and when to look again; the service keeps the stops (D-9); D-6's three states drive every resume | recovery tests split across the interface; SS-9 both profiles |
| W3.3 | FS, SB | A limit stop's attribution forwarded to Switchboard with config folder, launch id, folder and time; a claim pins its account as a lease | SS-7 with Switchboard up, down and absent |
| W3.4 | SB, FS | Launch and dispatch: Switchboard picks the account and writes the environment, the service spawns, reaps, stops and enforces the deadline (`spawn`); Windows dispatch stays in Switchboard | the dispatch tests across both |
| W3.5 | FS | Labels and organizations from Switchboard in rows; ids only without it | row tests |

Exit: SS-9; every recovery and dispatch test green across both repositories.

## W4. Install, update, uninstall

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W4.1 | FS | `install --if-needed --min-api N`: run by the installed binary when it answers `hello`, else the bundled one; one lock; the candidate's signature checked before it runs; `releases/<version>-<digest12>/`, the receipt (`schema: 1`, `clients[]`), `current` switched only after the new release answers `hello` as itself within 30 s, else rolled back; `held_off` for `off` and a disabled Background Items job (LC-07's one notice); `launchctl bootstrap` or `kickstart`; a supervisor marker checked so a space's LaunchDaemon and a LaunchAgent never both run; Linux systemd user unit | SS-3, SS-4 |
| W4.2 | FS | Self-update: the signed feed 90 s after start then every 5 min, the `auto-update` switch, exit 75, a release kept while a live host uses it (pid checked), held during a live wave | update with a live host: it lives |
| W4.3 | FS, SB | `uninstall`: refuses while hosts run unless `--end-sessions`; exports the data back to each client's folder (`exported.json`); Switchboard imports it once when it takes ownership back; `--purge` removes data | SS-11 |
| W4.4 | SB | The app bundles the signed binary in `Contents/Helpers/` (copied after signing, never re-signed; the smoke checks its identifier) and runs `install --if-needed` at start; `switchboard uninstall` removes itself from `clients[]` and, if last, runs the service's uninstall | a fresh macOS profile gets the service from Switchboard alone |
| W4.5 | FD, FB | The same in Fabric Dashboards and Fabric | each product's smoke on a fresh profile |
| W4.6 | LN | The launcher installs the service and its plugin; its doctor checks presence, version, health and `held_off` | doctor output |
| W4.7 | OB | Mac Studio spaces: one LaunchDaemon per space account through the space validator | the space's health `ok` |
| W4.8 | FS | Release pipeline: universal Mach-O, Developer ID with hardened runtime, notarized as a zip, `SHA256SUMS`, minisign `latest.json` | a release run |

Exit: on a clean macOS user, installing any one product leaves the service running; on the MacBook
and the Studio the service updates itself with sessions running.

## W5. Restart waves (D-14)

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W5.0 | FS | Measure on the pinned Claude Code what a running session picks up without a restart (a new skill, a changed skill, a plugin, a hook, an MCP server) and whether `--resume <id>` keeps the id; record it with the version | the measurement in `docs/evidence/` |
| W5.1 | FS | Host protocol 2: `RESTART` keeps the socket and viewers and spawns the resumed child on the same PTY; a protocol-1 host is replaced | host tests with a viewer attached |
| W5.2 | FS | The planner: Claude Code sessions with kind, turn state (`UserPromptSubmit` and `Stop` events, the session file's status, own commands and subagents), waits, limit stop, duplicate, home known or not; the caller found as the nearest ancestor process a session file names; preflight per session; skip reasons; the digest; the count of prompted resumes | planner tests over fixtures |
| W5.3 | FS | The runner: turn-boundary ends only, three at a time through admission, waits copied before the end, resume with the session's own flags and `SWITCHBOARD_LAUNCH_ID`, the three prompts, the caller last, the duty officer's and recovery's hands off while a session is in a wave | SS-12, SS-13, SS-15 |
| W5.4 | FS | The wave record and its watch (`back`, `continuing`, retry once with no live copy, `failed`, Needs attention, one posted outcome); guards (`wave_running`, `too_soon`, `in_use_by_person`, cancel) | SS-14 on a fake clock |
| W5.5 | FS | CLI `sessions restart plan|apply|status|cancel`, MCP `sessions_restart_*` with the one-sentence warning | MCP tests |
| W5.6 | SB | Sessions page: *Restart all sessions*: the plan (what restarts, what is skipped and why, the caller, the prompted count) before the confirm, then the wave per row; strings through Copylot in en-US and ru-RU | the design pass and SCN rows |
| W5.7 | all | A live wave on the MacBook: every running Claude Code session (this one included) comes back as itself and continues | the evidence with commands and outputs |

## W6. Every app is a client

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W6.1 | FD | Consoles start as hosts of the service and attach; Dashboards' idle, stuck and limit-resume code removed; quit and update end no session and wait for no console; the idle time is the service's 65 minutes, stated in its `AGENTS.md`; its saved idle consoles and claims imported | SS-1 for Dashboards |
| W6.2 | FB | Terminals as hosts; the dispatcher's watchdog reads the service; `board-dispatches.json` imported | SS-1 for Fabric |
| W6.3 | SB | The Sessions page reads the service; the forwarding tools removed after one minor release | SS-8; the parity tables |
| W6.4 | CT | The vectors in FD, FB and SB gates | SS-8 |

Exit: SS-1 for all three apps; restart waves include consoles.

## W7. Acceptance and release

| Id | Repo | Task | Closed by |
|---|---|---|---|
| W7.1 | all | On the MacBook and the Studio: quit and update each app with sessions running; kill and update the service; a restart wave; uninstall and reinstall | evidence in `docs/evidence/` with commands and outputs |
| W7.2 | FS, SB | Releases, HANDOFF entries, board rows closed with receipts | release receipts |

## Order

W0 first (its SB fixes and the contract in parallel). W1, then W2. W3 and W4 in parallel after W2.
W5 needs W2, W3.1 and W4 (its live run W5.7 needs W4 on the MacBook). W6 needs W2 and W4 and runs in
the other repositories in parallel with W5; until W6.1 a wave skips consoles (`console_not_hosted`).
