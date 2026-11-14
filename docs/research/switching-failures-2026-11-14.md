report: switching-failures-2026-11-14
project: fabric-switchboard
topic: WF-5.1, SB-367

# Switching failure inventory, this MacBook, 2026-10-15 to 2026-11-14 (WF-5.1, SB-367)

Read-only, by an agent of the window-and-fleet run on 2026-11-14 from the MacBook's own records;
the Studio's logs were not read. Counts are from `~/Library/Logs/Fabric Switchboard/switchboard.log`
(one file, 2026-10-04 onward), `launches/*.json` (698 records, from 2026-10-29), `recovery.json`
(379 rows, kept 7 days, from 2026-11-07), `limit-evidence.json`, `usage-holds.json` (empty),
`digests/daily-*.jsonl` (from 2026-11-05). `proxy_failover` lines exist from 2026-11-02 (SB-237).
No `balancing.jsonl`: balancing was never turned on here.

**Not observable:** the monitor's rotation outcomes (`switched`, `switch_failed`,
`activation_failed`, `crates/switchboard-runtime/src/monitor.rs:1245-1257`) go only into the capped
store journal (256 rows, about 48 minutes), never into the log, so `automatic_switch_failed_share`
cannot be computed until they are logged (task WF-5.2 a).

## Classes

| # | Class | Count | Evidence | Code |
|---|---|---|---|---|
| C1 | No candidate: failover exhausted (held, full, other kind or organization, unusable credential) | 2,082 refused requests on 627 of 892 `proxy_failover` lines (2026-11-04 to 2026-11-14) | `{"event":"proxy_failover","exhausted":6,"full":6,"held":24,"moved":0}` | `crates/switchboard-core/src/lib.rs:1287-1292`; tally `rotation.rs:432-466` |
| C1a | a group with one account of the provider (`alone`) | 88 requests | `{"alone":1,"exhausted":1,"held":1}` | `rotation.rs:462-464` |
| C1b | accounts of another organization left out (`context`) | 303 exclusions | `{"context":36,"exhausted":12,"full":24}` | `rotation.rs:454` |
| C2 | Candidate also limited: continued and stopped again | 16 of 29 dispatch continuations ended `limit`; 196 of 379 recovery stops had `attempt >= 1`; 2,581 `recovery_hook result:again` | launch `ended:"limit"` with `continued_as` | `recovery.rs:322-346`, `376-420` |
| C3 | A dispatched run launched onto an account already exhausted (ended `limit` 24 to 28 s after launch) | 31 of 49 `limit` endings lasted under 60 s | launch records | `fallback.rs:509-512` (`account_for_default`) |
| C4 | Stale usage: a failed check makes `fresh_usage` None and the pick falls back to last known usage (99 for one never measured) | 1,244 failed checks: `unreachable` 668, `rate_limited` 365, `http_error` 146 (403: 105, 503: 40), `other` 65 | `{"event":"usage_check","outcome":"rate_limited"}` | `monitor.rs:625-650`; `rotation.rs:532-560` |
| C5 | Credential refused | `usage_check rejected` 493; `credential_source refused` 7; `seat_ended by:renewal` 24; `recovery_hook no_account` 17 | `{"event":"usage_check","outcome":"rejected"}` | `monitor.rs:655`; `lib.rs:4983-4988` |
| C6 | A stop never registered, so never resumed | `recovery_hook refused` 440 | `{"event":"recovery_hook","result":"refused"}` | `recovery.rs:331-335` (the id is not accepted or the folder cannot be canonicalized) |
| C7 | Recovery waited long or gave up | 43 `cancelled` (41 with no selected account), 18 `waiting`; 88 rows reached `attempt >= 3`, one 67 | `recovery.json` | horizon `recovery.rs:24,120-128`; backoff `recovery.rs:32,547` |
| C8 | Stream cut by an owner restart or update | 36 `owner_stopped outcome:abandoned` (drain about 2 s); 4 `update_restart drained` with `in_flight > 0`, 6 requests cut after 90 s; 4 `hard_exit` | `{"event":"update_restart","in_flight":3,"millis":90205,"outcome":"drained"}` | proxy stop 2 s `runtime/src/lib.rs:3355-3358`; `src-tauri/src/updates.rs:1010-1022` |
| C9 | Fallback to another agent never handed over | 183 `fallback` lines, 0 hand-overs: `no_checkout` 140, `engine_unreadable` 22, `fabric_request` 21 | `{"event":"fallback","outcome":"no_checkout"}` | `fallback.rs:929-932`, `920`, `926` |
| C10 | Account lent away | 0 (balancing off) | none | `balancing.rs` |

Per exhausted request the other accounts were left out as: `held` 12,926 (about 6.2 each),
`credential` 7,325 (about 3.5, mostly the 13 accounts marked elsewhere), `full` 2,024, `context` 303,
`kind` 0.

## Shares

- Failed proxy failovers: `sum(exhausted) / (sum(moved) + sum(exhausted))` from 2026-11-02 to
  2026-11-14 = 2,082 / 2,372 = **87.8 %**; 627 of 892 passes (70.3 %) had at least one exhausted
  request. The monitor's own rotations are not in it.
- Interactive limit stops resumed: `resumed` / all rows in `recovery.json` = 318 / 379 = **83.9 %**
  (cancelled 11.3 %, waiting 4.7 %); it counts a resume that stopped again as resumed.
- Dispatched limit stops resolved: of 31 chains from a first `limit` record along `continued_as`,
  11 ended `exited` (**35.5 %**), 18 still at `limit` after at most 3 launches, 2 unknown.

## Fixes, by impact (each test-first; the tasks of WF-5.2)

1. **C1, no candidate.** Capacity ran out. When the tally is `exhausted` with every account `held`
   and a chosen balancing peer or a backup group (WF-6) has room, lend at once instead of answering
   429. Test: `move_route` with all pool accounts held and a lender with room returns the lender's
   account (today None, `lib.rs:1287`).
2. **C2 and C3, continuing onto an account already limited.** `account_for_default`
   (`fallback.rs:511`) builds its exclusions from `limits.limited_ids` only, without the proxy's
   refusals since the last pass (`store.limited()`, used by rotation at `monitor.rs:1166`) and
   without SB-345's headroom rule (`recovery.rs:396-400`). One shared exclusion set for rotation,
   recovery and dispatch. Test: `pick_for_default` never picks an account the proxy refused in the
   current pass.
3. **C6, stops never registered.** Record the stop with the stated folder and a `directory_gone`
   flag, return a typed reason, and log `recovery_hook refused` with `reason`. Test: `register_stop`
   with a folder deleted after the stop keeps the row.
4. **C4, stale usage.** In failover, skip a candidate whose last check failed `rate_limited` or
   `rejected` within its window; with only stale candidates left, probe the best first. Test: a pool
   whose only free account is `rate_limited` returns None or probes, never selects it blind.
5. **C8 and C9.** C8: the owner drain gives streams the same 8 s as the rest; test: a 5 s streaming
   response ends `drained`. C9: when the workflow names no checkout, use the stopped session's folder
   from the recovery row or launch record; test: a workflow without `artifacts[git]` and a launch
   record with a folder hands over.

Before any of these: log `rotation` with `outcome` (`switched`, `switched_on_limit`,
`switch_failed`, `activation_failed`, `no_candidate`) beside the journal write at
`monitor.rs:1257`, so WF-5.3 can measure.
