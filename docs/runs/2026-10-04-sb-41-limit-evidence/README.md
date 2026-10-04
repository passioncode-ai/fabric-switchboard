# SB-41 — typed, attributable limit evidence (2026-10-04)

Board row [SB-41](../../evidence/backlog.md); packet SES-B and findings SES-04…06 in
[the session research](../../reports/2026-10-04-system-review/raw/sessions.md). Baseline
`585c255`. Branch `agent/sb-41-limit-evidence`. Prerequisite of SB-25 (deliberate continuation).

## Source ledger

| Source | What it gave |
|---|---|
| `crates/switchboard-runtime/src/limits.rs` | `Limit {until, source}` only; native markers charged by timing; a marker with the reset of another account's hold skipped (SES-04: distinct accounts can share a reset); holds memory-only (SES-06) |
| `monitor.rs` `scan_limits`/`rotate`, `lib.rs` `MonitorStatus`, MCP `switchboard_status`, UI `accountLimit`/`quotaOrder` | every reader of the hold: rotation exclusion, the status report, the row badge and wait |
| Session research SES-B | the proposed `LimitEvidence` contract; "unknown is a valid final result" when the provider has no field |
| Marker shape | field names only, as measured earlier on this machine: `isApiErrorMessage`, `error: rate_limit`, `apiErrorStatus: 429`, `quotaLimits.resetsAt`; no field states scope or kind of budget |
| Retro standing instructions 1, 3, 5 | readers listed; the new file has an owner and a deletion rule |

## Decisions

1. **Shape:** `Limit {event_id, source, session?, observed_at, kind, resets_at?, until, confidence}`.
   `kind = quota` only when the marker's `quotaLimits.rateLimitType` names a subscription window
   (kept, allowlisted, as `limit_type`); otherwise `unknown` (matching the rendered English
   sentence is refused). `confidence = attributed` for a managed request (the proxy knows the account),
   `inferred` for a transcript marker.
2. **Reset vs hold:** `resets_at` is the provider's reported time, uncapped; `until` is the
   reset or the fallback hold, bounded to seven days from the marker's own time. The UI reads *Limit resets {date}* only when they are the same, else *Retry hold until*.
3. **Scope stays `unknown`.** No marker, 429 or Codex identity carries an organisation/group
   field; no cross-seat propagation is guessed. SB-25 must not wake on unknown scope or inferred
   attribution.
4. **Attribution follows the session**, not reset equality alone: a marker continuing a limit
   already held on its session's accounts stays there; a session opened before the switch with
   another account's reset goes to that account (sessions of one account share its reset);
   anything else — a session opened after the switch, or a new reset — is the current account's
   (fixes SES-04). Whether a running session adopts a switch is not assumed (SB-06 is open). The reset-less old-session
   rule and the managed-echo rule stay.
5. **Durability:** `<data>/limit-evidence.json` (0600; ids, times, session ids), written only on
   change, sanitised on load, bounded to 256 limits and bindings — a full table evicts the hold
   ending soonest rather than dropping a new limit. Owner: the owner process (the offline CLI keeps
   no limit state).

## REQ table

| REQ | Requirement | Verified by |
|---|---|---|
| R1 | Equal resets on distinct accounts: a new session's limit is the new account's; the old bound session stays the old account's | `an_equal_reset_on_a_new_session_is_the_new_accounts_limit` |
| R2 | An old unbound session with another account's reset is not charged to the new account | `a_marker_with_another_accounts_reset_is_not_charged_to_the_new_one` |
| R3 | Reset vs retry hold: a fallback hold reports `resets_at: null`, `kind: unknown` | `evidence_survives_a_restart_and_a_hold_is_not_a_reset` |
| R4 | Evidence survives an owner restart; expired, over-long or foreign entries are not loaded | same, `evidence_read_from_disk_is_bounded` |
| R5 | No provider text, prompt or path in the file or the report | same; `session_id` digest for non-id file names |
| R6 | A full table never drops a new limit | `a_full_table_makes_room_and_never_drops_a_new_limit` |
| R7 | Existing attribution rules unchanged (grace, journal after restart, echo, old reset-less session, opaque content) | the existing 12 `limits::tests` |
| R8 | UI distinguishes a provider reset from an estimated hold | `limitLabel` case in `scripts/test-ui-logic.mjs` |
| R9 | Docs: CONTRACTS, ACCOUNTS-AND-ROTATION, OPERATIONS, AGENTS lifecycle, SCN-031, strings, CHANGELOG, plugin skill | docs diff, `./scripts/check.sh` |

## Verification (2026-10-04, macOS arm64)

| Check | Result |
|---|---|
| `./scripts/check.sh` (final) | exit 0 — 346 Rust tests passed, 2 opt-in ignored (337 before); 20 UI-logic cases; Python suites; fmt, clippy, vocabulary, docs, notices |
| Planted defects | reset equality deciding alone → `an_equal_reset_on_a_new_session_is_the_new_accounts_limit` fails; a binding that always wins → `a_bound_session_that_hits_a_new_limit_holds_the_current_account` fails; the hold bounded from pass time → `a_far_reset_is_written_once_and_reported_as_given` fails |
| Gate intermittent | `managed_session_reads_usage_and_switches_only_inside_its_pool` (MCP) failed once on the `global: true` refusal; 14 solo runs and 18 parallel stress runs green; mechanism not established. The assertion now prints the tool's message; same class as SB-07 (an MCP tool answering an unexpected error only inside the full gate) |
| Disk | the machine ran out of space mid-run (109 MiB free; a hook then refused every shell command); `cargo clean --profile dev` per AGENTS.md freed 13.5 GiB of this repository's own build cache |

### Independent review (unit + seam)

| Finding | Outcome |
|---|---|
| P1 — a session bound to A that adopted B after the switch charged B's own limit to A; rotation could never leave B (regression) | fixed — a binding wins only for a marker continuing a limit held there (`a_bound_session_that_hits_a_new_limit_holds_the_current_account`); the "runs on one sign-in" claim removed from code and docs |
| P2 — a reset beyond seven days, capped at pass time, rewrote the file every pass and reported a wrong reset | fixed — bounded from the marker's own time; `resets_at` uncapped (`a_far_reset_is_written_once_and_reported_as_given`) |
| P2 — a fresh binding was not saved | fixed — insertion counts as a change (`a_new_binding_alone_is_saved`) |
| Removed accounts never pruned | fixed — `forget_missing` each pass (`a_removed_account_loses_its_holds_and_bindings`) |
| `kind: quota` over-claimed; `rateLimitType` ignored | fixed — kind from the allowlisted `limit_type` (`the_kind_comes_from_the_markers_own_limit_type`) |
| A later row replaces an attributed one; `event_id` shared across copies | ruled: the row is the latest evidence per account; `event_id` names the event, documented |
| Eviction at 256 could free a live hold; no cap on load | fixed — cap on load (latest kept); with pruning the table cannot fill with live holds of existing accounts |
| Wrong comment about synthetic owners | fixed |
| Idle-budget and MCP tests never reach the file or the new fields | covered by `a_far_reset_is_written_once_and_reported_as_given` (write once over 20 passes) and the report-field assertions; the MCP status passes the rows through unchanged |
