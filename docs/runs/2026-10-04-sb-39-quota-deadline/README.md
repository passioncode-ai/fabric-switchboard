# SB-39 — one provider not-before for every quota check (2026-10-04)

Board row [SB-39](../../evidence/backlog.md); packet in the
[unified plan](../2026-10-04-quota-review/PLAN.md#packet-sb-39--quota-check-cooldown); research
QF-02/QF-03 and QUOTA-03 in [the quota report](../../reports/2026-10-04-system-review/raw/quota.md).
Baseline `5e275ca` (main after PR #35). Branch `agent/sb-39-quota-deadline`.

## Source ledger

| Source | What it gave this run |
|---|---|
| Code: `crates/switchboard-runtime/src/monitor.rs` (`check`, `probe`, `due`, `failure_delay`), `lib.rs` (`Operation::Usage`, `Runtime::execute`), `crates/switchboard-proxy/src/lib.rs` (`retry_after_seconds`, `probe_usage_from`) | The bypass: `Operation::Usage` calls `check` with no look at a recorded 429; `next_check_at` gates only the background `due`. Integer-only `Retry-After`; six-hour cap. |
| `docs/CONTRACTS.md` §0.5.1 refusals and probe contract; `docs/ACCOUNTS-AND-ROTATION.md` (Retry-After paragraph); `docs/PLAN-0.5.md` REQ-40 | The contract being changed: `rate_limited` = numeric seconds; at most six hours, else 900 s, never sooner than the backoff. |
| `docs/evidence/retro.md` standing instructions 1–4 | #1 binds: the gate goes in the shared function every caller reaches (`check`), not in the UI. #3: the new private file gets an owner and a deletion rule. |
| Code graph | none built for this repository (`graphify-out/` absent). Reach was taken from `grep` over callers: `Operation::Usage` ← `src-tauri probe_usage`, CLI `Command::Usage`, MCP `switchboard_usage`; `check` ← `Pass::run`, `Operation::Usage`. |
| Wiki | projects wiki has no Switchboard page newer than the repository docs; repository docs are the source. |
| Board | 43 rows; 24 open before this run (SB-39 among them). Verification ledger: see stage 8. |
| External | [RFC 9110 §10.2.3](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after) (delay-seconds or HTTP-date); §5.6.7 (recipients accept IMF-fixdate, RFC 850 and asctime forms). [anthropics/claude-code#30930](https://github.com/anthropics/claude-code/issues/30930): persistent usage-endpoint 429 with `retry-after: 0`. |

## Decisions (grill resolved autonomously on the operator's instruction, 2026-10-04)

1. **Where the gate lives.** At the top of `monitor::check`, before the token renewal and the
   probe. Every caller — desktop, CLI through the owner, MCP, offline CLI and the background
   pass — reaches it (retro #1). A UI-disabled button alone would leave CLI/MCP open.
2. **Two different waits.** An ordinary failure (network, 5xx, unreadable vault) keeps its
   exponential backoff for the *background* only; a person may check again at once. A provider
   429 records a **not-before**: no check from anyone before it.
3. **Not-before value.** Positive `Retry-After` (seconds, or an HTTP-date in any of the three RFC
   9110 forms, measured against the response's receipt time): exactly that. Zero, past date,
   negative, malformed or absent: the existing 900 s floor (zero is the observed failure mode of
   #30930). The ceiling moves from six hours to **seven days** — a deliberate sanity bound, not a
   shortening of any plausible provider wait — and is documented. The background's next check
   is `max(ordinary backoff, not-before)`.
4. **Persistence and rollback.** The not-before is kept in a new private file
   `usage-holds.json` in the data folder (0600, bounded read), **not** as a field of
   `accounts.json`: `UsageHealth` is `deny_unknown_fields`, so a new field would stop 0.5.3 —
   kept on this Mac for rollback — from opening the store at all while a hold exists. Owner: the
   process holding the store lock (owner or offline CLI). Deletion rule: an entry is dropped when
   it expires, when its account is gone, or when its credential generation is no longer stored;
   the file holds account ids, times and a SHA-256 fingerprint of the access token, never a
   token.
5. **Credential generation.** A hold binds to the access token it was earned with (fingerprint).
   A new sign-in, capture or renewal stores a new token: the hold no longer applies. The
   background `due` filter uses the cheap proxy "usage health still present" (a new generation
   clears it) so no credential is read on a timer (LC-04).
6. **Clock set back.** A hold recorded more than 60 s ahead of the current clock is rebased to
   *now + its remaining duration* and written back once, so a wrong clock neither shortens the
   wait nor stalls the account forever.
7. **Coalescing.** Same-account checks share one in-flight request: a caller that asked while a
   check was running takes that check's answer. The owner's global transaction is **not** held
   during the network wait; the renewal inside `check` still takes it (as the background pass
   already does).
8. **Wording.** The refusal reuses the existing `USAGE_RATE_LIMITED` sentence (no new vocabulary).
   The account row of a failed check without a stored observation gains *Next check <date>*, the
   same caption rows with an observation already show; no new string family.

## REQ table (frozen; adding is free, removing needs the operator)

| REQ | Requirement | Verified by |
|---|---|---|
| R1 | A check during an active provider not-before makes **zero** upstream calls and answers `USAGE_RATE_LIMITED`, through the owner (`Runtime::execute`) and the offline CLI path | runtime test with an upstream call counter |
| R2 | After the not-before passes, exactly one upstream call is made | same test, clock past the deadline |
| R3 | Simultaneous same-account checks make one upstream call; the global transaction is free while it waits | coalescing test; a `Select` completes while a probe is pending |
| R4 | `Retry-After` parses delay-seconds, IMF-fixdate, RFC 850 and asctime against an injected receipt time; zero, negative, malformed, overflow and past dates are handled | proxy parser unit test |
| R5 | No vendor wait is shortened: waits above six hours kept up to the seven-day bound; zero/absent/invalid → 900 s; background next check ≥ not-before | monitor policy test |
| R6 | A hold survives an owner restart and is honoured by the offline CLI | restart test over the same data folder |
| R7 | A new credential generation does not inherit the hold | test replacing the token |
| R8 | A hold stamped ahead of the clock is rebased, not stalled; held rows never take the background's per-pass slots | clock test; `due` filter test |
| R9 | An ordinary failure leaves a manual retry available (no hold) | test: 500 then immediate manual check reaches upstream |
| R10 | No raw provider body or header reaches the renderer, journal or hold file | hold-file content test; existing body test |
| R11 | Contracts and operations docs, scenarios, strings, CHANGELOG, board and HANDOFF describe the new behaviour | docs diff; `./scripts/check.sh` doc checks |
| R12 | A failed row with no stored observation shows when the next check runs | `ui-logic`/render check and demo inspection |

## Carry-over ledger

| Item | Home |
|---|---|
| Live provider 429 with an HTTP-date `Retry-After` never observed on this Mac | SB-15 / SB-01 live acceptance |

## Verification (2026-10-04, this branch, macOS arm64)

| Check | Result |
|---|---|
| `./scripts/check.sh` (final) | exit 0 — 327 Rust tests passed, 2 opt-in native tests ignored (306 before this run); 18 UI-logic cases; Python 23/6/2/33; fmt, clippy `-D warnings`, error vocabulary, docs and notices |
| Fail-first: `retry_after_reads_seconds_and_every_http_date_form` | did not compile before `retry_after_at` existed; green after |
| Planted defect 1 — gate check removed from `monitor::check` | 3 of the 6 owner tests failed (R1, R2, R7 path) |
| Planted defect 2 — `Operation::Usage` back under the owner transaction | `same_account_checks_share_one_request_and_hold_no_transaction` failed: *a quota check does not hold the owner's transaction: Elapsed* |
| Planted defect 3 — no coalescing | the coalescing test failed (2 upstream calls) |
| Planted defect 4 — hold recorded after the health write | `a_wait_is_kept_even_when_its_health_record_cannot_be_written` failed |
| Planted defect 5 — grants left on at quit | `a_quota_check_left_running_at_quit_spends_no_refresh_token` failed: left 1, right 0 |
| `serve_stops_on_sigterm_and_sigint_within_the_deadline` | 1 failure in 6 runs before SB-44's fix (killed by signal 15); 12/12 after |
| Browser, `?demo=1&quota-review=1`, managed Chrome | Harbor row reads *Check failed · Next check 4 Oct, 18:47*; *Check usage* answers the rate-limit sentence at once; light and dark at 1280 CSS px; 740 CSS px with no horizontal overflow (`scrollWidth` 740) |
| `cargo tree -e features,normal` (seam review) | `synthetic-origins` absent from the app, CLI and runtime normal graphs |

### Independent review

Two blind readings ran on the diff before commit: unit tier (changed code) and seam tier
(everything that reaches it). Twelve findings; each was fixed with a test that fails without it,
or ruled on:

| Finding | Tier | Outcome |
|---|---|---|
| A hold stamped under a clock later set back stalled the background for the size of the correction | unit, P2 | fixed — `held()` rebases without a credential (`the_background_rebases_a_hold_without_a_credential`) |
| The hold was lost when the failed-health write refused | unit | fixed — recorded first (`a_wait_is_kept_even_when_its_health_record_cannot_be_written`) |
| Coalescing could hand a new sign-in the old token's answer | unit | fixed — store change count as epoch (`a_new_sign_in_during_a_check_is_checked_on_its_own`) |
| Poisoned lock failed open; unsaturated arithmetic on file values | unit | fixed — poison-tolerant lock, saturating math (`holds_read_from_disk_are_bounded_and_expired_ones_dropped`) |
| Expired/removed-account entries never pruned; loaded waits unbounded | unit | fixed — sane load, `forget_missing` each pass (`a_removed_account_leaves_no_hold_behind`) |
| A dated `Retry-After` measured on the local clock | unit | fixed — provider `Date` (`a_dated_retry_after_is_measured_on_the_providers_clock`) |
| File syncs under the in-memory lock | unit | fixed — versioned write after release |
| A quota check left running at quit could spend a refresh token after the drain | seam, P2 | fixed — grants off at drain start (`a_quota_check_left_running_at_quit_spends_no_refresh_token`) |
| Global change count as coalescing epoch costs an extra request on an unrelated edit | seam | ruled: kept — conservative, never a stale answer; documented in CONTRACTS |
| Gate fingerprint (token) ≠ store generation (token or expiry) | seam | fixed — fingerprint covers both |
| Hold bound to a credential read separately from the one sent | seam | fixed — the probe sends exactly the bound credential |
| No runbook entry for `usage-holds.json` | seam | fixed — OPERATIONS.md |

### Not exercised

A real provider 429 (either `Retry-After` form), installed-app behaviour, Windows file
permissions of `usage-holds.json`, hosted CI (nightly at 23:00 Europe/Warsaw covers `main`
after merge). These remain with SB-15, SB-02 and the nightly run.
