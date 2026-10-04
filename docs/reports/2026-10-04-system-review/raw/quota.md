# Quota, resets, probing and rotation research

Research snapshot: 2026-10-04. Local baseline: `46ee4b9a249b3da9612e7ca40308ea12178b92a6`. This is a raw research attachment to the owning report, not a second canonical backlog. Findings were sent to the parent as they arose; the session researcher was warned of the stale-window issue before designing session continuation.

Scope: source and contract review, developer discussions and official documentation, followed by a parent-assigned bounded repair of QF-01. No global credentials or OS account data were read; no live-provider requests, native activation or device acceptance were performed. Source-level findings below are distinct from observed production incidents. Executed synthetic checks are recorded in the repair receipt below.

## Existing behavior and sound decisions

- Individual windows and the maximum utilization are preserved; the aggregate `resets_at` belongs to the window with maximum utilization, not to all windows. Evidence: [`aggregate_usage`](https://github.com/passioncode-ai/fabric-switchboard/blob/46ee4b9a249b3da9612e7ca40308ea12178b92a6/crates/switchboard-proxy/src/lib.rs#L515), lines 515–529.
- Provider/pool separation, enabled credentials, fresh successful quota, threshold, hysteresis and cooldown gate automatic choices. A known inference-limit error bypasses cooldown but excludes limited candidates. Evidence: `crates/switchboard-core/src/rotation.rs:107–189` at baseline.
- Rotation rejects failed health, future timestamps, expired observations and reset crossings. Reset time passing does not imply measured zero. Evidence: `crates/switchboard-core/src/rotation.rs:221–243`.
- Usage probes use fixed HTTPS destinations, no proxy, no redirects, a 15-second timeout and bounded responses; 401 and 429 remain different error classes. Observation storage checks credential generation. Evidence: `crates/switchboard-proxy/src/lib.rs:598–667`.
- A 30-second owner pass probes four due accounts, oldest due first; normal success schedules 180 seconds later. Failure delays preserve the observation and prevent broken rows from continually occupying the front of the queue. Evidence: `crates/switchboard-runtime/src/monitor.rs:12–18`, `:120–143`, `:260–307`; `docs/ACCOUNTS-AND-ROTATION.md:31–33` explicitly qualifies larger collections.
- Inference-limit holds do not come from quota-probe errors. Two managed 429s within five minutes create a 15-minute retry hold; native transcript markers can provide a reset, otherwise the same fallback applies. Evidence: `crates/switchboard-runtime/src/limits.rs:17–27`, `:121–145`; `docs/ACCOUNTS-AND-ROTATION.md:69`.

## Important findings

### QF-01 — carried-forward quota windows lose their original freshness (P1, reproduced and repaired locally)

`merge_windows` copies old windows omitted by a new header observation, provided their reset is not before the new observation, then stamps the combined usage with `new.observed_at` (`crates/switchboard-core/src/lib.rs:229–246`). `observe_generation` sets health to `ok`, stamps `checked_at` with the new time, and merges the windows (`:1023–1034`). The rotation guard only checks that names `five_hour` and `seven_day` are present, followed by the aggregate observation/health age (`crates/switchboard-core/src/rotation.rs:224–239`).

Example: a candidate has low weekly usage observed yesterday, with its weekly reset still in the future. A fresh response reports only low five-hour usage. The old weekly window survives, receives the aggregate's fresh timestamp, and satisfies rotation's complete-name check. Repeated partial observations can keep an unobserved weekly value fresh indefinitely. This can rank or select a candidate whose actual weekly capacity is unknown.

Existing test `partial_headers_cannot_erase_unknown_weekly_capacity_for_rotation` (`crates/switchboard-core/tests/rotation.rs:662–737`) starts with old weekly utilization at 98%, so preserving that value prevents selection; it does not test a stale low weekly value. This is not proof that the age issue is covered.

Design options: additive per-window `observed_at` with version-compatible defaults and independent age checks, or conservatively prohibit partial header observations from establishing rotation eligibility while retaining historical windows for display. The latter is bounded and safer for an immediate fix. Complete headers cannot refresh model-specific windows they do not report either. UI and core should share the same distinction between historical display and fresh capacity.

### QF-02 — manual quota checks bypass provider cooldown (P2, source-proven gap)

`Operation::Usage` unconditionally calls `monitor::check` (`crates/switchboard-runtime/src/lib.rs:851–854`). Neither the `check` path nor `probe` gates the request on a prior 429's future `usage_health.next_check_at`; that timestamp only gates background `due`. UI exposes `Check usage` on every enabled OAuth row (`src/main.ts:449`). A caller can repeatedly hit a usage endpoint during a recorded provider wait.

Recommendation: retain a typed, persisted probe-specific cooldown and enforce it at the owner operation used by desktop/CLI/MCP. Coalesce concurrent checks for the same credential generation. Generic network backoff and a provider-required wait need distinct semantics so a deliberate manual retry can remain available for an ordinary network error. A UI-disabled button alone would leave CLI/MCP bypasses.

### QF-03 — incomplete Retry-After handling (P2, confirmed compatibility gap)

`retry_after_seconds` accepts integer seconds only (`crates/switchboard-proxy/src/lib.rs:587–597`); valid HTTP dates fall back to 900 seconds. The scheduler truncates provider waits above six hours (`crates/switchboard-runtime/src/monitor.rs:246–257`), so its next request can precede a provider deadline. The current six-hour cap is an explicit existing contract, not an accidental undocumented choice.

[RFC 9110 §10.2.3](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after) permits both delay seconds and HTTP dates. Extend parsing with an injected received-at clock; preserve the server's not-before deadline without shortening it through ordinary exponential-backoff caps. Handle invalid, negative, zero, past-date and overflow inputs. Document any upper bound as a deliberate policy and show a manual recovery path rather than silently retrying early.

### QF-04 — multiple reset clocks and inferred holds need different presentation (P1, requested UX design)

Two exhausted windows resetting at different times require the latest known reset among all exhausted windows before all recorded blockers clear. If any exhausted window lacks a reset, the account's recovery time is unknown. The aggregate's reset is only one maximum-utilization window; ties do not make that value a recovery contract.

The 15-minute limit hold is often a heuristic next eligibility time, not a provider reset. Show it as a limit/retry hold, with source and date. For a genuine window reset, show both local calendar date/time and remaining duration. After crossing zero, show that refreshed usage is pending; never automatically mark the account usable. Keep absolute times accessible and avoid per-second screen-reader announcements. Preserve provider and pool groups.

Proposed row states: fresh measured capacity first; known exhausted/held accounts with a known recovery/hold clock next, earliest clock first; exhaustion with unknown recovery and unknown/stale/failed observations afterwards; disabled or sign-in-required accounts last. Within available rows, descending remaining headroom is reasonable; stable account identity breaks ties. Selection/current badges are independent of order. Distinguish measured capacity from guaranteed ability to run any model or billed operation.

### QF-05 — current provider schemas contain more dimensions than the parser (P2, compatibility risk; no live defect claimed)

The local Codex parser reads only `/rate_limit/primary_window` and `/rate_limit/secondary_window` (`crates/switchboard-proxy/src/lib.rs:682–686`). Official [OpenAI Codex backend client source](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs), function `rate_limit_snapshots_from_payload` and test `usage_payload_maps_primary_and_additional_rate_limits`, additionally handle feature-specific limits, credits, spend control and reached-limit reasons. Ignoring those dimensions can make the local percentage incomplete for newer plans. This does not prove the operator's account currently returns them.

Collect synthetic fixtures for these shapes, decide which dimensions block the selected operation, and preserve their scope instead of flattening every feature into one maximum. Credits, subscription utilization and workspace spending are different measures. Display incomplete support honestly. Do not introduce billing or account refresh as an incidental parser change.

### QF-06 — adaptive scheduling must preserve freshness promises (P3, existing SB-29 continuation)

Four probes per 30-second pass give at most 24 probes per normal 180-second interval when requests finish promptly (computed: `4 × 180 / 30`). Slow requests, renewal and source work reduce that throughput. Rotation permits `max_age_seconds` from 1 to 86400 (`crates/switchboard-core/src/rotation.rs:31`), so the configured freshness requirement can be shorter than the polling period. Documentation already explains larger stores can wait longer; this is not a newly demonstrated starvation bug.

Use deterministic simulation of 1/12/24/64-account stores, slow probes, failures, reset crossings and clock jumps to evaluate SB-29's adaptive cadence. Prioritize current/candidate pools without starving other due rows, and expose when requested freshness cannot be maintained. Never silently increase accepted observation age to make idle measurements look good. Reset-time rechecks should obey provider cooldown.

## External evidence and its limits

- [Claude Code statusline documentation](https://code.claude.com/docs/en/statusline#rate-limit-usage), checked 2026-10-04, documents five-hour and weekly percent/reset fields and gateway spend limits. Fields depend on subscription/gateway and appear after the first response. An opt-in statusline transport is a possible active-session data source, with account attribution and freshness checks. It cannot populate inactive accounts or assume missing means zero; installing/chaining an operator's statusline is a separate feature, not part of the sorting change.
- [Anthropic developer issue #30930](https://github.com/anthropics/claude-code/issues/30930), opened 2026-03-05, reports persistent usage-endpoint 429s with `retry-after: 0`. This is primary developer testimony, not an Anthropic service guarantee or a verified current incident on this Mac. It supports retaining stale values, backing off even on zero hints and distinguishing telemetry refusal from inference exhaustion.
- [OpenAI plan support](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan), checked 2026-10-04, says continuation depends on plan and workspace limits, and that ChatGPT upload/image/voice caps are separate. Do not infer all account usability or token-cost budget from one percentage.
- [OpenAI developer community discussion](https://community.openai.com/t/codex-usage-improvements-plan-stacking-purchase-resets/1386583/5) records a support-team response linking paid resets. Treat it as a discovery lead; [the linked official help article](https://help.openai.com/en/articles/20001507-paid-weekly-work-and-codex-rate-limit-resets) is the validation source. Resets can change; infer none from a fixed five-hour/week formula.

## Bounded packets for the unified plan

| Packet | Priority / dependency | Cold-reader implementation contract | Checks and completion evidence |
|---|---|---|---|
| QUOTA-01 | P1, immediate user outcome | Own `src/ui-logic.ts`, account/usage rendering and demo/test fixtures. Use QF-04 state/order rules, provider/pool groups, stable focus and identity, absolute date plus relative duration, historical stale labels, and independent current/selection badges. Countdown ticking makes no network requests. Update scenarios and brand copy in same change. | Pure-clock fixtures: zero/100, two exhausted windows, unknown reset, passed reset, failed health, future observation, partial headers, disabled/sign-in-required, limit hold; UI 1280/740 widths, expanded detail and open menus across tick/sort. |
| QUOTA-02 | P1, before trusting partial-header candidates | Own core window merge/eligibility, plus compatible renderer freshness semantics. Fix QF-01 with explicit old-window age or conservative complete-observation requirement; preserve generation clearing and quota history. No native switching behavior change. | Regression with old low weekly, new five-hour-only observation after max age; repeated partial headers never resurrect stale capacity; complete headers/JSON still work; carried model-specific stale windows remain historical; reset and credential-race suites. |
| QUOTA-03 | P2, after typed cooldown design | Own probe failure metadata, owner usage operation, scheduler parser and contracts. Enforce provider not-before deadlines across desktop/CLI/MCP, support both RFC Retry-After forms, coalesce same-generation requests, retain generic manual network retry. | Synthetic upstream call counter shows no call before deadline, one coalesced request, new generation invalidates old gate; HTTP date/seconds/zero/malformed/overflow/past date; delay beyond six hours; restart persistence. |
| QUOTA-04 | P2, provider schema compatibility | Own fixture set and scoped usage contract for additional Codex limits/credits/spend; preserve unknown fields safely and avoid aggregate claims over unsupported dimensions. Coordinate with SB-25 spend-limit research. | Sanitized synthetic payload fixtures, plan without primary window, feature block despite primary capacity, credit/spend block separation, no credentials/raw bodies in renderer/journal; official source snapshot or pinned upstream commit. |
| QUOTA-05 | P3, belongs to SB-29 | Own deterministic schedule model and any bounded cadence improvement, not a duplicate editable backlog row. Respect owner shutdown, fairness, mutation serialization and provider holds. | 1/12/24/64-account simulation, slow/failing provider, no active rotation, reset due during cooldown, backward clock; report actual observation lag and idle calls separately. |

Merge these packets into the parent's single plan with stable canonical board IDs and exact dependencies. QUOTA-02 finding affects current UI availability classification and future session continuation: notify both owners whenever the state/age contract changes.

## QF-01 repair receipt

Parent assigned a narrow conservative repair after the research finding. Local modifications, pending parent convergence and commit:

- `crates/switchboard-core/src/lib.rs`: a merged observation retains the oldest aggregate observation time whenever any old window is carried forward. `UsageHealth.checked_at` still records the latest actual check. No schema, source-string or credential behavior change. Fresh complete JSON observations replace prior historical windows, so an endpoint dropping an optional model window does not permanently prevent recovery. Header observations continue to discard a retained window after its reset passes.
- `crates/switchboard-core/src/lib.rs`: observation ordering also checks the latest health timestamp, including failed checks. This preserves rejection of delayed older observations while aggregate freshness can remain older; a later failed check cannot reopen acceptance of an earlier response, and future-clock handling remains bounded by the existing `ahead` predicate.
- `crates/switchboard-core/tests/rotation.rs`: `partial_headers_never_freshen_an_unobserved_low_quota_window` covers low stale weekly and low stale model-specific windows, repeated partial observations, latest check versus oldest evidence, a later failed check followed by delayed older headers, and authoritative JSON recovery.
- `crates/switchboard-core/tests/storage.rs`: historical Opus merge expects the older aggregate timestamp. The existing lifecycle no-write test now captures its reset clock once; previously it recomputed `now()+7200` on every supposedly unchanged write, making the test intermittently describe a real reset change across a wall-clock second.

Fail-first command: `cargo test --locked -p switchboard-core --test rotation partial_headers_never_freshen_an_unobserved_low_quota_window -- --exact` → exit 101 before the repair, actual `threshold_reached`, expected `no_eligible_account`, missing weekly at +301 s. A second fail-first assertion showed delayed older headers were accepted after conservative aggregate aging, before the health-time guard.

Final command: `cargo test --locked -p switchboard-core` → exit 0 after the repair. Rotation: 14 passed. Storage: 24 passed, 1 explicit synthetic Keychain acceptance ignored. Projects: 5 passed. Unit tests include the parallel backup agent's in-progress changes (45 tests, 1 signed-bundle acceptance ignored); these counts describe this shared worktree, not an isolated quota-only commit. No live-provider, native Windows, screen-reader or hosted-CI acceptance is inferred.

Formatting: `rustfmt --edition 2021 crates/switchboard-core/tests/rotation.rs crates/switchboard-core/tests/storage.rs` → exit 0. Parent owns the full repository gate, scenario/contract updates, independent convergence review, commit and push.

Convergence follow-up: parent identified that a failed health update removed the prior success's ordering guard. Extended the regression with partial successes at +301/+302 s, failed health at +303 s, and delayed headers at +301 s; it failed before broadening the guard. The final guard checks any health status. `cargo test --locked -p switchboard-core --test rotation --test storage` → exit 0 (14 rotation passed; 24 storage passed, 1 explicit native test ignored). Focused `git diff --check` → exit 0. This focused run follows the earlier full-core success; parent owns final full-workspace validation after convergence.
