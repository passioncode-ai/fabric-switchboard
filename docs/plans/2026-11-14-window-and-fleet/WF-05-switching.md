# WF-5 Switching that never drops a session (SB-367, wave 3)

> грамотно продумать, автоматические смены, где подписок, где они там свапятся, чтобы ничего не
> вылетало

## Now

Switching paths: managed rotation (`crates/switchboard-core/src/rotation.rs`, on by default since
DEC-0008), SB-98's failover of a refused request (DEC-0002, once), native rotation of the ordinary
Claude Code, balancing loans (DEC-0012), recovery of stopped sessions (SB-114, SB-345), automatic
fallback to another agent (SB-73). SB-217 (0.7.79) stopped rotation moving a subscription onto a key.
Metrics exist in the class template (`automatic_switch_failed_share`), not yet measured here.

## Sub-modules and tasks

**5.1 Failure inventory.** From the last 30 days of this MacBook's and the Studio's logs
(`proxy_failover`, `rotation`, `recovery_*`, `fallback`, `limit` events) and the launch records:
every case a session stopped, waited or got an error during or after a switch, counted and
classified (no candidate, candidate also limited, stale usage, account lent away, credential
refused, stream cut by a restart). Written as `docs/research/switching-failures-<date>.md`.

**5.2 Fixes.** The inventory is done: [switching-failures-2026-11-14](../../research/switching-failures-2026-11-14.md)
(87.8 % of proxy failovers found no candidate; 31 dispatched runs started on an exhausted account; 440
stops never registered; 0 hand-overs to another agent in 30 days). Tasks, each test-first:
- 5.2 a. Log `rotation` outcomes (`switched`, `switched_on_limit`, `switch_failed`,
  `activation_failed`, `no_candidate`) so the share can be measured (first).
- 5.2 b. C2/C3: one exclusion set (limits, the proxy's refusals this pass, SB-345 headroom) for
  rotation, recovery and dispatch (`fallback.rs:511`).
- 5.2 c. C6: a stop whose folder is gone is still registered; `recovery_hook refused` logs a reason.
- 5.2 d. C4: failover skips a candidate whose last check failed `rate_limited` or `rejected`, or probes
  first.
- 5.2 e. C1: with every account held, borrow at once from a chosen lender or backup group (with WF-6).
- 5.2 f. C8: streams get the same drain time as the rest; C9: a hand-over uses the stopped session's
  folder when the workflow names no checkout.

**5.3 Measure.** `automatic_switch_failed_share` and `limit_stop_resumed_share` computed from the
operations digests (SB-250), shown on Statistics and in `switchboard_statistics`.

## Checks

Each fix's test seen failing; the share before and after on this MacBook over a week.

## Depends on

WF-6 (backup loans are a switching path).
