# Switchboard 0.5.4 — lifecycle contract (2026-10-03)

**Objective.** Apply the organization's [lifecycle contract](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/lifecycle.md)
(rules LC-01…LC-15, adopted 2026-10-03) to Fabric Switchboard, fixing the lifecycle audit's
findings for this product (`fabric-workspace` `docs/reports/2026-10-03-lifecycle-audit/raw/fabric-switchboard.md`,
host findings H3/H4). Branch `claude/lifecycle-contract`, based on `21d005c` (0.5.3 on `main`).
Board row SB-27 (numbered SB-25 on the branch before it met `main`'s 2026-10-04 board). The 0.5.3 work (single instance, lazy owner start, Codex read quietly, nothing
read without saved Claude accounts) is kept as it is.

## Done, with the test that proves each

| Finding | Rule | Change | Test |
|---|---|---|---|
| F2, H4 (`security -w` every 10–30 s) | LC-04 | The background probes Claude Code's and Codex's sign-in sources in-process with user interaction off — file stamps, the Keychain item's stamps and access list, nothing decrypted, nothing spawned — and reads only when a stamp moved. Locked, refused, absent and "would ask" outcomes are remembered; a locked keychain backs off 1 → 30 min; nothing is retried in a loop. Capture (a person's action) still reads directly | `external::tests::a_refusing_source_is_read_once_in_an_hour_of_passes_not_in_a_loop`, `…a_locked_keychain_is_never_read_and_its_probe_backs_off_to_half_an_hour`, `…an_item_that_would_ask_is_not_read_until_it_changes_or_a_person_acts`, `…a_changed_source_is_read_again_and_a_read_in_the_writes_second_is_not_reused`, core `keychain_macos::tests::a_probe_reads_no_secret_and_tells_absent_trusted_untrusted_and_locked` (throwaway keychain), `…the_tool_must_be_on_the_decrypt_list_and_in_a_partition` |
| F3 (Codex keyring) | LC-04 | Already quiet in 0.5.3; now also gated by the probe, so a refusal is not re-read every sync | same gate tests (`a_probe_names_consent_lock_and_the_files_a_read_would_use`) |
| F4, H4 (idle cost) | LC-08 | 30 s pass (was 10 s); renewal schedule rebuilt only when credentials change; lineage scan memoised by the store's change count; Claude Swap found in the process table in-process (no `ps`); a quota check with unchanged numbers stays in memory (flush ≤ 15 min, at stop, on drop); the window's 1.5 s sign-in poll exists only while a sign-in waits | `monitor::tests::an_idle_hour_on_a_fake_clock_stays_inside_the_budget` (planted defect: a rescan every pass reads 1 940 credentials an hour and fails it), core `an_unchanged_quota_check_is_not_written_until_something_changes`, `pending_timestamps_are_written_once_they_are_older_than_the_flush_interval`, `the_process_table_is_read_in_process_and_includes_this_test`, ui `sign-in poll only while pending` |
| F1 (managed sessions die on restart) | LC-11 | Proxy port and token recorded in `proxy.json` (0600) and reused; a taken port moves the proxy, keeps the token, repoints `runtimes/*/launch.command` and Codex `config.toml` | `lifecycle_tests::a_restarted_owner_keeps_the_address_and_token_a_session_holds`, `…a_taken_port_moves_the_proxy_and_repoints_managed_homes_with_the_same_token` |
| F6 (no signal handler, stale `control.json`) | LC-01 | `Owner::shutdown(deadline)`: monitor stops, control closes and the descriptor goes, the operation in flight gets the deadline, proxy streams ≤ 2 s, store flushed. Desktop: `RunEvent::Exit` + SIGTERM/SIGINT → the same drain; `serve`: SIGTERM and SIGINT. Drain 8 s, hard exit 10 s. `control.json` carries the owner pid; a dead pid makes it stale | `lifecycle_tests::an_idle_owner_stops_at_once_and_removes_its_descriptor`, `…a_busy_owner_finishes_the_operation_in_flight_within_the_deadline`, cli `serve_stops_on_sigterm_and_sigint_within_the_deadline`, control `a_descriptor_whose_owner_died_is_stale_even_when_its_port_was_reused`, `the_descriptor_names_the_owners_pid` |
| F10 (no logs) | LC-12 | `oplog`: `~/Library/Logs/Fabric Switchboard/switchboard.log`, JSON lines of codes and numbers only (type-enforced), 5 × 5 MB, 0600, no symlink follow | `oplog::tests::*` (rotation, privacy, symlink) |
| SB-23 | — | `security` calls and the backup write run as blocking sections (`block_in_place`) | `blocking::tests::a_blocking_call_does_not_stall_the_tasks_beside_it` (planted defect: 2 ticks instead of ≥ 10) |
| F8 (stale copies), build output | LC-15 | `scripts/prune_artifacts.py` keeps the current and previous release per kind, unregisters removed bundles; both build scripts run it; the target bundle copy is unregistered after copying | `scripts/test_prune_artifacts.py` (in the gate) |
| — | LC-09, LC-15 | [AGENTS.md → Lifecycle](../../AGENTS.md#lifecycle): every process, port, cadence, idle budget, residency, output dirs, `target/` cap (10 GB) and clean command | docs gate |
| F7 (second launch) | — | already fixed in 0.5.3 (`f7a1f22`); untouched | — |

Version: 0.5.4 in every manifest.

## Not done, and why

- **F1 residency** (window close stops background work): a product decision — menu bar,
  hide-on-close or opt-in login item. Board SB-28. The stable proxy address removes the part that
  broke running sessions.
- **F5 broker pin by hash:** owned by the local-lifecycle broker, not this repository.
- **F8 orphan `vault-key` item, F9 uninstall, adaptive probe cadence (F4):** board SB-29. The
  probe cadence is coupled to rotation's `max_age_seconds`; backing off probes for rotation
  pools would make rotation refuse every candidate.
- **Desktop SIGTERM end to end:** the drain is shared code tested through `serve` and the owner;
  the packaged app's signal path is not exercised by the gate (it needs a built bundle).

## Checks run

- `CARGO_TARGET_DIR=…/target ./scripts/check.sh` — see the PR for the exit code of the final run.
- Planted defects watched failing: renewal rescan (1 940 reads/h), blocking without
  `block_in_place` (2 ticks), every lifecycle test against the old `Owner` (store still locked,
  port moved, no drain), the storage test before the lazy write (12 writes instead of 2).

## Exact next task

Land SB-27 after v0.5.3-beta.1 is published: review the PR, merge, release v0.5.4-beta.1, install,
then on the operator's Mac (1) sample `ps` for 200 s and confirm no `security`/`ps` children at
idle, (2) read `~/Library/Logs/Fabric Switchboard/switchboard.log`, (3) quit and relaunch with a
managed session open and confirm it keeps working, (4) measure average CPU over an hour against
the 0.2 % target.
