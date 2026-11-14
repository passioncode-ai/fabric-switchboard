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

**5.2 Fixes.** One task per class with a count above zero, each test-first; known candidates: a lent
account the lender's rotation still picks, a failover onto an account whose check is older than its
window, a restart that cuts a stream (drain 2 s).

**5.3 Measure.** `automatic_switch_failed_share` and `limit_stop_resumed_share` computed from the
operations digests (SB-250), shown on Statistics and in `switchboard_statistics`.

## Checks

Each fix's test seen failing; the share before and after on this MacBook over a week.

## Depends on

WF-6 (backup loans are a switching path).
