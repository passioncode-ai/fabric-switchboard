# SB-40 — Codex limits beyond the primary and secondary windows (2026-10-04)

Board row [SB-40](../../evidence/backlog.md); packet in the
[unified plan](../2026-10-04-quota-review/PLAN.md#packet-sb-40--provider-compatibility); research
QF-05/QUOTA-04 in [the quota report](../../reports/2026-10-04-system-review/raw/quota.md).
Baseline `7033042`. Branch `agent/sb-40-codex-limits`.

## Source ledger

| Source | What it gave |
|---|---|
| Official Codex client, pinned [`openai/codex@afb436d`](https://github.com/openai/codex/blob/afb436df8b70bb5bc57b86d9a3e829968988cd21/codex-rs/backend-client/src/client.rs) — `rate_limit_snapshots_from_payload`, `make_rate_limit_snapshot`, `map_rate_limit_reached_type`; tests `usage_payload_maps_primary_and_additional_rate_limits`, `usage_payload_maps_zero_rate_limit_when_primary_absent` | How the official client reads `/backend-api/wham/usage`: one `codex` snapshot plus one per `additional_rate_limits` entry; credits and spend control on the main snapshot only; an unknown reached-type kind maps to nothing |
| Wire names, same commit, [`codex-backend-openapi-models/src/models/`](https://github.com/openai/codex/tree/afb436df8b70bb5bc57b86d9a3e829968988cd21/codex-rs/codex-backend-openapi-models/src/models) | `plan_type`; `rate_limit {allowed, limit_reached, primary_window, secondary_window}`; window `{used_percent, limit_window_seconds, reset_after_seconds, reset_at}`; `additional_rate_limits[] {limit_name, metered_feature, rate_limit}`; `credits {has_credits, unlimited, balance}`; `spend_control {reached, individual_limit {limit, used, remaining, used_percent, remaining_percent, reset_after_seconds, reset_at, source}}`; `rate_limit_reached_type {type: rate_limit_reached \| workspace_owner_credits_depleted \| workspace_member_credits_depleted \| workspace_owner_usage_limit_reached \| workspace_member_usage_limit_reached \| unknown}` |
| Code: `crates/switchboard-proxy/src/lib.rs` `parse_usage_at`; core `usage_valid`, `rotation::fresh_usage`; `src/ui-logic.ts` `quotaOrder`; MCP `usage_view`; CLI `usage` | Today only `/rate_limit/primary_window` and `/secondary_window` are read; `Usage.used_percent` must equal the maximum over `windows` (core invariant) |
| Retro standing instructions | #1 and #5: every reader of the new dimension listed below; the rollback build is a reader too |

## Decisions

1. **No schema change.** `Usage`/`UsageWindow` are `deny_unknown_fields` and `used_percent` must
   equal the maximum over `windows` in every build, including the 0.5.3 kept for rollback. New
   dimensions are stored as ordinary windows with reserved names; an older build reads them as
   windows and is *more* conservative (a feature limit raises its aggregate), never less.
2. **Account-wide blockers count everywhere.** `spend_limit` — the member's individual spend
   control (`used_percent`, `reset_at`), or 100 % with no reset when `reached` is true without
   details; `workspace_credits` / `workspace_usage_limit` — 100 %, no reset, from the
   workspace reached-types (an organisation limit: another seat in the same workspace will not
   help, which the scope name records); `limit_reached` — 100 %, reset unknown, when
   `rate_limit.limit_reached` is true or `allowed` is false (or the reached-type is
   `rate_limit_reached`) and no main window already reads 100 %.
3. **Feature limits are scoped, not flattened.** Each `additional_rate_limits` entry becomes
   `feature_<metered_feature>_primary` / `_secondary` (sanitised to `[a-z0-9_]`, ≤ 64 bytes, at
   most 12 such windows, duplicates dropped). They stay in `windows` and the stored aggregate
   (decision 1), but the **account capacity** that rotation, list order, CLI and MCP use is the
   maximum over the other windows (`Usage::account_used_percent`). A feature exhausted while
   the primary has room does not move the account in use.
4. **Missing is not zero.** A payload with no account-level window (no main window and no blocker) has unknown account capacity; a measured `spend_limit` alone is account-level and counts (review ruling, usage-based plans)
   (`account_used_percent` = none): never eligible for rotation, listed as unknown. A payload
   with nothing usable at all stays an error, as today.
5. **Credits are not stored.** `has_credits`/`unlimited`/`balance` describe purchase, not a
   reached limit; the reached-types above already carry the blocking case. No purchase, reset
   or billing operation is added.
6. **Unknown enum values are ignored**, as the official client does.

## REQ table

| REQ | Requirement | Verified by |
|---|---|---|
| R1 | The official client's fixture payload (primary 42, secondary 84, `codex_other` 70, spend 32 %, member credits depleted) parses to the main windows, one feature window, `spend_limit` and `workspace_credits` | proxy parser test |
| R2 | Plan without `rate_limit` but with a feature limit: account capacity unknown, feature window kept | proxy + core tests |
| R3 | Feature exhausted while primary has room: rotation does not move off the account; list order and MCP read it as available | core rotation test, UI logic test, MCP view test |
| R4 | Spend control reached (individual limit 100 % or `reached` without details) blocks regardless of the percentage windows | proxy + rotation tests |
| R5 | `limit_reached`/`allowed: false` with no window at 100 % is a block with unknown reset | proxy test |
| R6 | Unknown reached-type, unknown fields, malformed feature names, more than 12 feature limits: no failure, bounded names and counts | proxy test |
| R7 | The stored usage still satisfies `usage_valid`; a 0.5.3 build's validator accepts it | core test on the serialized store |
| R8 | No provider string except a sanitised feature id reaches storage, renderer or journal | proxy test |
| R9 | Docs: CONTRACTS, ACCOUNTS-AND-ROTATION, CLI/MCP description, scenario SCN-021, strings, CHANGELOG, board, HANDOFF | docs diff, `./scripts/check.sh` |

## Verification (2026-10-04, macOS arm64)

| Check | Result |
|---|---|
| `./scripts/check.sh` (final) | exit 0 — 337 Rust tests passed, 2 opt-in ignored (327 before); 19 UI-logic cases; Python suites; fmt, clippy, vocabulary, docs, notices |
| Fail-first | the five parser tests did not compile before `account_used_percent` existed; `feature_limits_neither_move_the_account_nor_make_a_candidate` fails with rotation reading the stored aggregate; `every_blocker_with_many_features_still_fits_the_store` fails with the fixed 12-feature cap |
| Browser, `?demo=1&quota-review=1` | Codex *Engineering*: row reads 68 % (account windows), disclosure lists *codex_other · primary feature limit · 100% used*; list order unchanged by the feature limit |
| Gate intermittent | `the_first_check_after_the_wait_is_one_request` (SB-39) failed once: a 1 s `Retry-After` recorded at x.999 s ends before an immediate second call at whole-second resolution — a test timing race, not product behaviour; now 2 s and polls the hold (5/5) |

### Independent review (unit + seam, one reading)

| Finding | Outcome |
|---|---|
| F1 P2 — 2 main + 3 blockers + 12 features = 17 windows; the store rejects the observation and the account sticks | fixed — features take only the room left (`every_blocker_with_many_features_still_fits_the_store`) |
| F2 — a feature window's reset made the account stale (rotation, list, MCP) | fixed — only account windows (`feature_limits…` extension, UI case, MCP) |
| F3 — quota cell showed a feature's reset | fixed — `accountReset` |
| F4 — unknown drawn as a 0 % meter | fixed — no meter when unknown |
| F5 — overspend above 100 % or a bad reset dropped the spend dimension | fixed — clamped, reset read separately (`an_overspent_limit_is_clamped_and_a_bad_reset_loses_only_itself`) |
| F6 — the shipped agent skill still described the old windows | fixed — `plugins/switchboard/skills/switching-accounts` (`scope`, account-only minimum, provider wait) |
| F7 — no main windows + a measured spend limit gives known capacity | ruled: intended — a spend limit is the account's constraint on a usage-based plan; decision 4 amended |
| F8 — positional `same_quota` vs reordered features; collisions after sanitising | fixed — features sorted by name (`feature_order_does_not_change_the_observation`); collisions keep the first (documented) |

### Not exercised

A live Codex payload with `additional_rate_limits`, spend control or a workspace reached-type
(incidence unknown); whether the server sends a reached-type while nothing is blocked (the
official fixture shows that combination — if it happens live, every seat of that workspace reads
`workspace_credits` until a check stops reporting it). These stay with SB-01/SB-15.
