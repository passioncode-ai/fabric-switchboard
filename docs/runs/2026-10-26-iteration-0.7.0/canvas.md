# Iteration 0.7.0 «first organization»: canvas

Release iteration through `/task-pipeline iteration`, target 2026-11-14 (plan
[2026-10-25](../../reports/2026-10-25-audit-and-plan/README.md) §3B, decisions D-3, D-5, D-6).
Survey: that plan. Dossiers, written in parallel on `a9ff8fa`:

| Dossier | Items | Nodes | Risk |
|---|---|---|---|
| [A, context binding](dossier-a-context-binding.md) | SB-183 (shell hook, default on, PowerShell), SB-201 | A-0 … A-9, A-10, A-11 | 2 × R3 (A-2 scripts, A-3 rc installer) |
| [B, the organization's person](dossier-b-organization-person.md) | SB-202, SB-160, SB-125 | B-1 … B-10 | 1 × R3 (B-5 ended seat) |
| [C, chains and ceilings](dossier-c-chains-and-ceilings.md) | SB-71, SB-72 | C-1 … C-9 | 4 × R3 (C-4 … C-7, money) |

Not in this iteration: SB-167 (7 days after Observatory's OBS-191), SB-184 (Observatory OBS-236, a 0.7.x after
2026-11-14), SB-177 (Fabric CO-412). Live checks SB-159, SB-178, SB-143, SB-15 are the operator's
([checklist](../../reports/2026-10-26-operator-acceptance/README.md)).

## Integration pass 1

**Renamed.** Dossier A's SB-201 nodes, written as B-1 and B-2 there, are **A-10** and **A-11** here (dossier B
owns the B ids). Scenario ids proposed by all three started at SCN-074; the last in `docs/ux/scenarios.md` is
SCN-073, so: dossier A takes **SCN-074, SCN-075**; dossier B **SCN-076 to SCN-079**; dossier C **SCN-080 to
SCN-084** and **SCR-03**.

**One seam per pair, designed together.**
- **A with B (the checklist's `hook` step).** B-3's `onboard` step `hook` reads A-1's `switchboard shell status`
  (the map file), never the rc files: A-1 owns the format, B-3 only reads it. Order: A-1 before B-3.
- **A with C (plain `claude` in a group folder).** C-8 resolves a group's chain by the checkout's folder before the
  executor's account: the case A's hook makes common. C-8 lands before A-3 turns the hook on by default.
- **B with C (an ended seat in a chain).** B-5 puts an ended seat in the sign-in-required set; C-5's readiness
  already skips that set, so a chain never hands work to an ended seat. No shared file.
- **Shared files.** `docs/ux/scenarios.md` (A-7, B-1, C-1) becomes one node **S-1**, written once through
  super-ux. `src/main.ts`, `src/ui-logic.ts`, `src/locales/ru.ts` (A-7, B-6 … B-8, C-3, C-7) are touched by one
  UI node per wave, in sequence. `crates/switchboard-cli/src/main.rs` (A-1, B-3, C-6) and
  `crates/switchboard-runtime/src/lib.rs` (A-3, A-4, B-4, C-2, C-4) are edited in disjoint hunks, one node at a
  time per wave.

A second pass changed nothing.

## Waves

| Wave | Nodes | Gate |
|---|---|---|
| 1 | S-1 (scenarios SCN-074 … SCN-084, SCR-03, amendments to SCN-039, 040, 051, 056, 065, 066), A-0 (measurement) | `/ux-lint` green; A-0 recorded |
| 2 | A-1, A-5, A-10, A-11, B-2, B-5, C-2, C-4, C-8 | each node's tests, risk-class rounds |
| 3 | A-2, A-3, A-4, B-3, B-4, C-5, C-6 | same |
| 4 | A-7, A-8, B-6, B-7, B-8, C-3, C-7 (UI, in sequence), Copylot on every string (en-US, ru-RU); D-1 (SB-212 session provenance and transcript: record fields, `launch --by/--parent`, `SessionStart`/`Stop` hook writes, `switchboard sessions` and MCP; no UI, runs beside the UI nodes, R2) | WebKit pass of the window, both themes, en and ru |
| 5 | A-9, B-9, C-9, release | doc gate, `./scripts/check.sh`, release |
| after | B-10 (SB-125 live setup), the operator's acceptance | human gates |

Questions: [questions.md](questions.md).

## Wave 2 state (2026-10-26)

| Node | Branch, last commit | Rounds | State |
|---|---|---|---|
| A-1, A-5 | `w2/shell` `fe36db4` | r1 seam fail (capture, wildcard match, map rule); r2 unit fail (a Windows-only test path); fixed | **merged** (`b0ae360`) |
| A-10, A-11, B-2 | `w2/context` `f4139e3` | r1 unit pass, seam fail (provider collision); r2 blocker closed, consumer fix needed | **merged** (`388cde9`); Fabric Dashboards adopted `agent` in 0.8.6 (`c8e089bb`), published 2026-10-26 03:23 UTC (0.8.7 installed): release gate cleared |
| B-5 (R3) | `w2/seat` `20d9ec1` | r1 unit pass, seam fail; r2 adversarial: 8 plants uncaught, all now caught (each re-planted by the builder) | **merged** (`f7d145f`) |
| C-8, C-4 (R3), C-2 | `w2/chains` `f061d36` | r1 seam fail (SetCeiling not window-only, Chains.effective, a Russian page name); r2 adversarial: 14 of 16 plants caught, an exact-size restore broke; fixed, every plant now caught | **merged** (`1cd483b`) |

Wave 2 merged at `1cd483b`: fmt and clippy clean (core, runtime, CLI, app); core, runtime (587 passed) and CLI tests green; typecheck, ui-logic, locale, error vocabulary and docs checks 0.

Builders run one at a time since 2026-10-26 02:00 UTC: the MacBook's disk has fallen to 0.2 to 6 GB free several times (other
processes too). `sb-070-target` is deleted between builders. A shared `CARGO_TARGET_DIR` served stale binaries twice: a green
run counts only when its output names the node's tests.

Board rows filed by the rounds: SB-211 (proxy failover does not read the ended-seat set). Still to file at A-9/B-9/C-9: `shell::scripts::tests::a_missing_old_or_failing_cli_gives_the_held_values_back` failed once in zsh at load 66 (passed 3 of 3 alone; likely the zsh hook's 2 s wait under load; a sibling of SB-07); SB-204's second source (sessions started before the plugin's 0.6.83 update have no `Stop` hook, so their stops wait for the one-day horizon; seen 2026-10-26 on 7 stops; settle a stop also on the session's first successful request through the proxy); six managed sessions of the PassionCode group stopped on one account at once (about 05:33 UTC 2026-10-26, reported by fabric-workspace) because a group's managed sessions share one route, with one account left holding quota: spread a group's sessions over its accounts; the agent-settings unreadable sentence ("restore it from a backup" when restore never overwrites), one
session per agent home and the `openrouter` pool sentinel (w2/context round 1), detect_session and shell homes, the A-Q8
read split.

## Wave 3 state (2026-10-26)

| Node | Branch, last commit | Rounds | State |
|---|---|---|---|
| A-4, A-2 (R3), A-3 (R3) | `w3/shell` `11019dd` | r1 seam fail (backup lost `off`, notice in memory only, ZDOTDIR, created file left, other data folder); fixed `4d8feb0`; r2 adversarial: 12 of 15 plants caught, bash `set -a` leak; fixed, every plant now caught | **merged** |
| B-3, B-4 | `w3/person` `97a48df`, fix `947b8e3` | r1 seam fail (`shell on` offered after the person turned it off, `shell on/off` dependency, `in_cli` missing on login finish); fixed; r2 clean | **merged** `a6701ad` |
| C-5 (R3), C-6 (R3) | `w3/spend` `49c415f` | fix cycle 1 `c373510` (every paid start gated once, C-Q8, C-Q9); fix cycle 2 `9a4f473` (counted keys window-only, one budget per pass, paid gate before the owner transaction, queued hand-overs re-gated, subscription refusal first); blind seam read: blocker `project delete` removed a counted key; fix cycle 3 `49c415f` (group delete keeps counted keys, canonical hand-over folder, SCN-054); loop guard reached, non-blocking findings filed SB-213 … SB-217 (`4fd6f4e`) | **merged** `dc734fd` |

Merged gate on `dc734fd` (2026-10-26): clippy `-D warnings` exit 0; `cargo test` core, runtime (669 passed, 1 ignored), cli: 0 failed; `check-error-vocabulary`, `check-locale`, `check_docs`, `check_plugin` exit 0. **Wave 3 closed.**

## D-1 (added 2026-10-26)

Operator request 2026-10-26: keep what a launched session did and show who launched it, from which project, where it stopped and where the work went. Backlog SB-212 (`3171e45`). Dashboards part asked on the board (thread `62d78b7f-629f-4867-8c83-265e1a8a5763`, copy to fabric-workspace). Schema to post on that thread when the node starts.

### D-1 design (2026-10-26)

Measured: a launch home is reused (`runtimes/<provider>-<pool>`, `homes/<account>`), and every launch overwrites its `.session.json` (`crates/switchboard-runtime/src/launch.rs:2663-2678`), so only the latest launch of a home is known; `runtimes/claude-agents` held 49 Claude Code transcripts and one record on 2026-10-26. Transcripts are kept by Claude Code itself for the home's `cleanupPeriodDays` (90, from the linked operator settings).

1. **One record per launch.** `<data>/launches/<launch_id>.json` (0600, append-only, written at launch, updated by the hooks below), kept 90 days like the transcripts; `.session.json` keeps the current state and gains `launch_id`.
2. **Who launched it.** `launched_by`: `surface` (`window`, `cli`, `mcp`, `fallback`, `recovery`, `dashboards`), `caller_launch_id` (the `SWITCHBOARD_LAUNCH_ID` the caller's own session carries: every launched session gets it in its environment, so a session that launches another names its parent with no help), `caller_project` (the group or repository of the caller's folder), `caller_program` (process name only, never a command line), and optional `by` and `parent` a runner sets (`switchboard launch --by voc-learn --parent <id>`).
3. **What it did.** The plugin's `SessionStart` hook writes `provider_session_id` and `transcript` (the hook input's `session_id` and `transcript_path`); Codex: its rollout file. `Stop` writes `last_turn_at` and `turns`; the end writes `ended_at` and `ended` (`finished`, `limit`, `killed`, `unknown`).
4. **Where it went.** `continued_as`: the hand-over id, the workflow checkpoint or the launch id of the session that took the work over (recovery, fallback, `continue`).
5. **Reading.** `switchboard sessions --history` (all launches, newest first), `switchboard sessions show <launch_id>` (the record, its chain up through `caller_launch_id`/`parent` and down through `continued_as` and children), MCP `switchboard_sessions` with `history` and `launch_id`. Fields additive; old records read as before.
Risk class R2 (new files and fields, no money, no credentials); tests: chain of three launches through the environment, hook writes with a synthetic transcript path, retention prune, old record compatibility.

### D-1 state (2026-10-26 22:20)

Code and docs on `w4/d1` `713264c` (WIP, pushed): `launches.rs`, one record per launch, `SWITCHBOARD_LAUNCH_ID` chain, `launch --by/--parent/--surface`, SessionStart and Stop hook writes, `sessions --history` and `sessions show`, 90-day prune, docs, Copylot `b2bea0e5-2909-4afc-bc3e-b5646b771307` and `ab7f5dfc-2cf9-4a5e-9952-9386b0e10122`. Not done: Codex transcript, `ended_at` beyond limit stops, `continued_as` of kind `workflow`. **Gate not run:** free disk fell below 1.5 GB twice and the machine ran out of memory (swap 33 of 35 GB, 40 `claude` processes); Claude Code stopped the disk watcher for memory pressure. Next: when swap is back below about 25 GB and the disk has 2.5 GB free, run clippy and the runtime, core and cli tests one crate at a time on `w4/d1`, then one blind round (R2), then merge.

### D-1 merged (2026-10-27)

`w4/d1` `330586b` merged. Gate on `73c3375`: clippy `-D warnings` 0, runtime 682 passed, core and cli 0 failed; the plain session list carries `ended`, `continued_as` and the transcript (Dashboards' ask, board `62d78b7f`). Blind seam read: blocker B-1 (a new CLI facing an older owner failed every launch) and N-1 (the chain linked launches that never started); fix cycle 1 `330586b`: bare retry once on `CONTROL_OWNER_OLDER`, `not_started` records unlinked, `LaunchedBy` accepts unknown fields, a SessionStart without a session or transcript writes nothing. Gate on `330586b`: clippy 0, runtime 687 passed, cli 0 failed; core unchanged by D-1 (0 files), passed on `97e698f` and `73c3375`. Non-blocking findings SB-218 to SB-221 (`2ad07ec`). Lead's own read of the B-1 retry: holds (`control.rs`, only `Launched` on `CONTROL_OWNER_OLDER`, once, logged).

### H-2 (added 2026-10-27, wave 4, backend, R3)

SB-174 day on 0.6.83 (`a5ba4bb`): reads outside a transaction still waited past 5 s. Cause (1): a renewal's `store` phase holds the store's lock up to 69.3 s (a Keychain write under it) and `Sessions`/`Recovery` start with `store.snapshot()`. Node: move the renewal's Keychain write out of the store's lock together with SB-206 (`renew_rows`, `save_account`), keeping metadata and credential writes atomic (write the credential first under its own item, then the metadata under the lock; a crash between leaves the old metadata naming a credential that still works), and log the store lock's wait in `control_slow` as `lock_ms`. Tests: a snapshot answers while a renewal's credential write is slow (fake vault with a delay); `control_slow` carries `lock_ms`. Two rounds incl. adversarial. Cause (2), host starvation, is the machine's swap, not Switchboard's.

### A-7 merged (2026-10-27)

`w4/a7` `7052826` merged. A-7 `6d40024`: Groups section *Terminals in organization folders* (every SCN-075 state), *Turn off / Turn on* through the installed owner only, the first-time notice, MCP `switchboard_shell_hook` (read) and `switchboard_shell_hook_set` (write, hidden under `--read-only`), 33 ru strings, 88 WebKit captures in `evidence/a7/`; Copylot `c4e52902`, `8feda489`, `cedfcad1`. Blind seam read: blocker B1 (the installer and the readers used two rules for the default, so the window said off while blocks were written); fix cycle 1 `7052826`: one rule from `map.json`, Codex line only with a Codex account, notice only from the installed owner, *Add the block again* says it saves `on`, refresh at 60 s, docs; Copylot `fb32a6ed`, `79bc5c67`. Gate on `7052826`: clippy `-D warnings` 0 for runtime, cli and fabric-switchboard; runtime 690 passed; cli and `fabric-switchboard -- suggestions` passed; tsc, ui-logic, locale, vocabulary, brand, docs, plugin 0. After the merge on the iteration: tsc, ui-logic, locale, docs 0. Left: SB-222.

### H-2 merged (2026-10-27)

`w4/h2` `c56ef47` merged. H-2 `cdff8b5`: a credential lock taken by every credential writer, the metadata lock only inside it, so `snapshot()` and reads never wait on a Keychain call (renewal store phase, `save_account`, `renew_rows`, SB-206); `lock_ms` in `control_slow`. Round 1 (seam): one doc claim false (agent keys, group delete). Round 2 (adversarial): 20 plants, every credential or verdict loss caught; missed lock-order inversions, an unlocked restore, mark ordering, `lock_ms` from other tasks, an unbounded replan. Fix cycle 1 `c56ef47`: agent keys and group delete moved out of the metadata lock, panic-safe `WriteMark`, 8 tests each proved by its plant, `SAVE_CONTENDED` message (Copylot `724d0b6f`, `a1ba73fe`, `86bd23b7`); re-read of the fix: pass. Merged gate: clippy `-D warnings` 0 (core, runtime, cli, fabric-switchboard); core and cli 0 failed; runtime 693 passed twice after the SB-221 fix below; tsc, ui-logic, locale, vocabulary, docs, plugin 0.
SB-221 cause found: `fallback::HAND_OVER` is one lock per process and neighbouring tests hold it across 10 s reads, so `the_automatic_fallback_gates_its_step_once_and_never_waits_on_itself` timed out at its 10 s wait for its turn; its timeout is now 60 s (a self-deadlock never ends). Left: SB-223.
