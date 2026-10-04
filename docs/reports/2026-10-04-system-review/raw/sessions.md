# Session continuation research — 2026-10-04

Bounded read-only research for the parent system review. Source checkout: `46ee4b9a249b3da9612e7ca40308ea12178b92a6` (includes lifecycle work). No ordinary auth, managed homes, real transcripts, environment files or provider prompts were read. No native switch, provider request, process stop, message to a provider session or acceptance test was performed. References below are repository-relative paths at that commit; parent report owns report metadata and prioritization.

## Current behavior and evidence

- The main job is continuing the local task under a deliberate identity without spending another team's quota: `docs/SPEC.md:30`. Snapshot selection and authenticated provider response are separate claims: `AGENTS.md:37`.
- `docs/evidence/backlog.md:13` keeps SB-06 (running-process adoption) open. `docs/evidence/backlog.md:32` keeps SB-25 (continuation of a limited session) open. SB-15 live acceptance is also open (`docs/evidence/backlog.md:22`).
- `monitor::scan_limits` matches the ordinary profile to saved accounts and supplies transcript paths (`crates/switchboard-runtime/src/monitor.rs:501-519`). The rotation outcome only changes the managed route or native auth and journals the result (`monitor.rs:566-608`). It contains no supervisor wake, resume or restart.
- Managed 429s are recorded as one sanitized `rate_limited` outcome, without scope or reset metadata (`crates/switchboard-proxy/src/lib.rs:439-446`). Two failures within five minutes establish a 15-minute hold (`crates/switchboard-runtime/src/limits.rs:121-146`). This is a retry hold, not a provider reset.
- Native detection reads flagged error markers. `quotaLimits.resetsAt` supplies a reported time; missing time becomes a 15-minute hold, capped at seven days (`limits.rs:287-313`). State returned to IPC contains only account ID, until and source (`limits.rs:55-65`). No reset-confidence or spend-scope field exists.
- New launches have isolated homes or provider/pool homes (`crates/switchboard-runtime/src/launch.rs:815-851`). CLI arguments only attach MCP config (`launch.rs:924-938`); no session identifier is captured or resumed. Existing launch reservations prevent home rewrite while a process is live.
- The no-replay boundary is explicit in `docs/SPEC.md:61` and `docs/ACCOUNTS-AND-ROTATION.md:71`. Preserve it while designing SB-25.

## Confirmed mismatches and risks

| ID | Finding | Classification and evidence | Recommended priority |
| --- | --- | --- | --- |
| SES-01 | SCN-031 says work continues automatically although no continuation mechanism exists | Documentation/behavior mismatch: `docs/ux/scenarios.md:605-608` versus `monitor.rs:566-608`, `launch.rs:924-938`, backlog SB-25. The scenario is draft/unobserved, but its expected result still implies an absent feature. | P1, correct with current UI change |
| SES-02 | Fixed native adoption timing is presented as fact | Unsupported claim: `limits.rs:19-21` and `docs/ACCOUNTS-AND-ROTATION.md:69` assert ~30 seconds; SB-06 explicitly requires measuring it. 60-second grace is a heuristic, not proof that an old process adopted auth. | P1, correct wording now; live probe later |
| SES-03 | Ordinary conversation JSON is parsed despite the privacy claim | Code-proven mismatch: `limits.rs:5-6` and `docs/ACCOUNTS-AND-ROTATION.md:69` promise only flagged error lines are parsed, yet `session_start` deserializes every timestamped head line to `serde_json::Value` (`limits.rs:271-284`), including an ordinary message's nested content. No persistence/logging leak was observed. | P1, bounded metadata parser |
| SES-04 | A reset timestamp or nearby managed failure stands in for session identity | Conditional correctness risk, not an observed live incident: all native markers within 10 seconds of any managed 429 are skipped (`limits.rs:172-175`); every hold sharing another account's reset is skipped (`limits.rs:159-177`). Independent accounts can legitimately have equal reset times, and concurrent native/managed failures can occur together. Current tests prove intended heuristic cases, not reliable attribution. | P1 dependency of automatic continuation |
| SES-05 | Account hold cannot identify organization/group spend exhaustion | Confirmed contract gap for future SB-25, not proof current observations were misclassified: `Limit` is only until/source (`limits.rs:30-34`); marker recognizes generic rate_limit/429 (`limits.rs:287-306`); managed journal also erases scope. Shared budget limits must not cause a loop across accounts in the same organization. | P1 before enabling continuation |
| SES-06 | Holds are volatile and age-window reconstruction is incomplete | By-design limitation: memory-only map (`limits.rs:37-40`); discovery accepts recently modified transcripts only (`limits.rs:194-234`). Restart after the scan window can forget a longer spend hold unless a new marker is emitted. Do not represent an inferred hold as durable provider reset. | P2, evaluate with typed limit records |

Proposed truthful SCN-031 expected result: the selection changes to an eligible account; a subsequent accepted managed request uses that route. A native session already stopped on a limit may require deliberate continuation/resume, and adoption by an existing process remains SB-06. No request is replayed. Replace a universal ~30-second switch promise with scheduler-dependent detection and measured acceptance coverage. This wording is a proposal; the parent owns scenario and brand review.

## Internet research and confidence

All sources were opened on 2026-10-04. Official documentation describes the current supported surface; it does not establish the operator's installed CLI behavior. Forum/issue reports identify failure modes, not universal guarantees. Source summaries are intentionally bounded; task designs below are recommendations inferred from these sources and repository constraints.

1. **Local resume is supported.** CLI reference documents ID/name resume, continuation, transcript-path input, fork, session persistence switches and print mode. A live background resume can attach to the existing process rather than create a newly authenticated one; that path is version-dependent (2.1.285). Therefore launching `--resume` cannot by itself prove new identity. [Official CLI reference](https://code.claude.com/docs/en/cli-reference#cli-flags).
2. **Resume is not complete process recovery.** Session docs describe saved conversation, limited restoration of interrupted/background work, reapplication of launch options and permission-mode differences. `--mcp-config`, `--settings`, `--plugin-dir`, fallback model and added directories may need to be supplied again. Missing agent definitions can fall back to default tools. Copying transcripts across homes can create ambiguous resolution. None of these statements prove cross-account provider acceptance. [Official session reference](https://code.claude.com/docs/en/sessions#what-a-resumed-session-restores).
3. **A supported error-event seam exists, without a wake decision.** StopFailure has session/cwd/error context, but its output/exit code cannot continue a failed turn. Stop is for successful stopping, not API errors. An opt-in hook can feed sanitized metadata into a supervisor; it cannot be treated as an automatic retry mechanism. Do not retain error_details or last_assistant_message. [Official hooks reference](https://code.claude.com/docs/en/hooks#stopfailure).
4. **SDK interrupted-turn resume is explicitly opt-in.** `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` allows automatic continuation on restart, with a maximum-age bound and optional continuation message. A future harness should disable it by default, explicitly gate its enabled path and bound age. The retry watchdog is unsuitable for spend exhaustion: current versions stop immediately on spend/credits failures (change documented for 2.1.239). [Official environment reference](https://code.claude.com/docs/en/env-vars).
5. **Spend limits have shared and individual scopes.** Owners can control organization, seat and user budgets; an organization ceiling constrains every member. [Official Team/seat-based Enterprise usage guide](https://support.claude.com/en/articles/12005970-manage-usage-credits-for-team-and-seat-based-enterprise-plans). Enterprise group limits can also include shared monthly budgets. [Official group limits guide](https://support.claude.com/en/articles/13799932-manage-groups-and-group-spend-limits-on-enterprise-plans). Classifying every 429 as one account's quota is insufficient.
6. **Upstream corrected two directly relevant behaviors.** The official changelog records a Team/Enterprise error incorrectly naming the organization instead of the individual limit and a falsy interrupted-resume switch being ignored, both under 2.1.221. It also records retries stopping for spend/credits. Pin capability behavior rather than matching rendered sentences. [Official changelog](https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md) (mutable main, observed 2026-10-04).
7. **Developer bug thread: hot login failed in a live session.** Issue #15007 describes v2.0.53 on macOS remaining in 401 failure after /login, repeated continue/resume and a five-minute wait. Its in-memory caching explanation is the reporter's hypothesis. This establishes a historical counterexample to an unconditional adoption promise, not current CLI behavior. [Upstream issue #15007](https://github.com/anthropics/claude-code/issues/15007).
8. **Developer bug thread: shared refresh chains can race.** Issue #80085 reports many macOS sessions sharing one Keychain chain with repeated logouts on v2.1.216; destructive refresh-race causation is a hypothesis in that report. It reinforces the existing single-renewal-owner boundary. [Upstream issue #80085](https://github.com/anthropics/claude-code/issues/80085).

Not used as technical proof: Reddit discussions and third-party account-switching guides found during discovery. They disagree about hot adoption and often suggest killing processes or touching global auth. The official developer threads above satisfy community research while retaining primary-source provenance. No recommendation depends on those third-party claims.

## Unified-plan task packets for the parent

Order within this module: SES-A → SES-B/SES-C → SES-D → SES-E → SES-F. The parent's immediate quota/list task can proceed independently, except that inferred limit holds must not be labeled as provider resets. SES-C requires an operator-assisted acceptance step; its design and synthetic harness can be delivered beforehand.

### SES-A — Truthful scenarios and transcript metadata parsing

**Problem/context:** SES-01…03. The existing scenario promises stopped-session continuation; the scanner parses ordinary conversation objects to extract a timestamp.

**Scope/ownership:** `limits.rs` plus tests; proposed changes to SCN-031 and the current account/rotation contract. Acquire agent-sync leases for guarded docs; no real transcripts. Do not change native activation or add wake/retry.

**Contract:** Introduce a tiny timestamp-only serde type/visitor whose ignored fields use `IgnoredAny`, with bounded bytes. For marker parsing, keep the early marker check; new fields must remain metadata-only. State explicitly that bytes are read to locate metadata but ordinary content is neither materialized nor retained. Keep timestamp validation and old-session rejection.

**Dependencies:** None beyond current source baseline. Parent UI wording follows brand/scenario route.

**DoD/checks:** Existing limits tests pass; fixtures contain ordinary nested content and missing/invalid timestamp; parser returns only timestamp and cannot expose content. Update source comment, scenario and behavior docs in the same change. Run `cargo test -p switchboard-runtime limits`, docs gate and full local gate at integration. No live adoption claim.

### SES-B — Typed and attributable limit evidence

**Problem/context:** SES-04…06 and spend-scope research. A timestamp or adjacent failure cannot uniquely identify a live account/session.

**Scope/ownership:** Runtime limit scanner, monitor, proxy's bounded error/header metadata, additive core contract as needed. Provider body content remains unlogged. Do not add generic error bodies to IPC or infer scope from a rendered English sentence.

**Proposed contract:** `LimitEvidence { event_id, provider, pool, target, account_id?, identity_revision?, session_id?, observed_at, kind, scope, resets_at?, retry_not_before?, confidence }`; kind distinguishes quota, transient rate, spend and unknown; scope distinguishes account, organization, group and unknown. `resets_at` is provider time only; fallback hold goes in `retry_not_before`. Store only bounded allowlisted metadata. Unknown identity/scope can report hold but cannot authorize wake/replay. Deduplicate by event/session, not reset-time equality. Organisational blocks apply only to explicitly known matching organization IDs, never guessed pool equivalence.

**Dependencies:** SES-A; schema/allowlist decision in parent contracts; research/version probe for available provider structured fields. If fields are absent, unknown is a valid final result.

**DoD/checks:** Synthetic tests for equal resets on distinct accounts, simultaneous managed/native errors, unknown scope, org block across two seats, independent organization unaffected, dropped/stale identity revision, restarted owner and reset vs retry hold. Existing no-replay tests remain. IPC has no raw error, prompt or credential. Bound persistence/TTL if made durable; migration roundtrip and compatibility checks. No requirement to guess missing provider fields.

### SES-C — SB-06 and exact resume capability probe

**Problem/context:** Running CLI may cache credentials; --resume can attach to an existing old process; account/home changes can change session discovery. Current claim is unverified.

**Scope/ownership:** A repository-owned acceptance packet and injectable/synthetic probe harness. Record CLI version, OS, mode, identity source and resumed PID creation identity. Real authorized identities are entered through the app by an operator; never read global credentials for a test, never kill an external process or enumerate its conversation content.

**Contract:** Distinguish (a) existing native session following activation; (b) exited session resumed in same config home; (c) managed next request; (d) isolated target home with no transcript; (e) running background attach. A metadata identity claim or a new terminal is insufficient. Receipt needs sanitized evidence of provider identity for the next accepted request, timestamp and switch latency, plus safe refusal when unsupported. Prefer explicit session ID over latest-session lookup.

**Dependencies:** SES-B for reliable events, SB-01 authorized account acceptance. No automatic auth change in CI. Use process ownership markers; an unowned/native existing process remains unsupported until deliberately enrolled.

**DoD/checks:** Versioned matrix records PASS/FAIL/NOT_RUN per case and rejects future claims from a single CLI version. Synthetic harness validates exits, reused PID, unknown ID, missing home, same-reset accounts, background attach and no extra tool execution. Operator evidence names exact binary/version and intended identity; real acceptance can remain NOT_RUN with a precise next step. Remove universal adoption delay claim regardless of result.

### SES-D — Owned session registry and continuation state machine

**Problem/context:** SB-25 cannot address a stopped session without session identity, ownership, immutable account binding and in-flight state. Existing launch session strings identify a mode/account or pool, not the provider conversation.

**Scope/ownership:** A dedicated runtime module, launch hooks, sanitized control/status contract. App-owned launcher/hook additions only; no blanket user-config edits and no arbitrary session messaging. No process termination as part of a sorting/UI change.

**Proposed contract:** Persist metadata `session_id, provider, pool, target, owned_pid_and_birth, config_home_handle, cwd, identity_revision, launch_capabilities, lifecycle_state, last_safe_boundary, limit_event_id`; config home is internal and excluded from exported reports/IPC where inappropriate. State progresses running → blocked_on_limit → candidate_verified → resume_pending → resumed/held/failed/cancelled. Unknown/in-flight tool outcome forces hold and a task handoff, not repeat. Separate owner lifetime from provider terminal lifetime. Safe retirement/retention and stale reservations must be explicit.

**Dependencies:** SES-B/C; lifecycle baseline landed in 46ee4b9; existing launch reservations and identity locks preserved.

**DoD/checks:** Fake clock/process fixtures cover PID reuse, externally owned session, crash, owner restart, pending tools, expired event, changed cwd/rules, changed identity generation, stale capability and multiple owners. No secret or prompt in persisted state. Existing managed stream identity contract unchanged.

### SES-E — Select one proven wake channel

**Problem/context:** Three possible designs exist: supported session message, owned process resume, or Observatory/Fabric task handoff. None is proven by current rotation tests.

**Scope/ownership:** Design spike plus one adapter chosen from SES-C results. The default path is hold with explicit next action. Do not install global hooks or enable all channels as fallback.

**Contract/design decision:** A StopFailure hook can supply an event but cannot block/wake the failed turn. A supported messaging path needs exact target and authenticated recipient plus acknowledgement; being discoverable does not establish permission. A resume adapter requires old process exit/ownership proof, exact config/project context and safe tool boundary. A workflow handoff carries task context/reason `limit` without copying provider-owned conversation. Resend no prior user prompt. No cross-provider/pool switching. Fresh session creation does not claim cloud conversation migration.

**Dependencies:** SES-C/D; Observatory M9 is externally owned and remains a dependency, not edited here. Cross-session messaging authorization must be explicit in product policy/enrollment.

**DoD/checks:** Produce a decision table explaining chosen supported channel, rejected alternatives and version envelope; fixtures for recipient unavailable, attach-to-old-process, no acknowledgement, duplicate delivery, stale transcript and unknown tools; observable sanitized wake receipt. If no channel passes, deliver a documented manual resume flow and keep automation off.

### SES-F — Opt-in continuation and acceptance

**Problem/context:** A deliberate automatic continuation feature must protect against duplicate actions and shared-budget loops while giving the operator control.

**Scope/ownership:** Runtime state machine/adapter, additive control and UI scenarios; new user-facing text goes through the brand route. Enable separately from rotation, off by default. Account selection alone remains available.

**Proposed contract:** Exactly one claim/wake per limit event; bounded event age and attempts; never exhausted account or same known blocked organization; fresh credential/quota and matching pool/provider; require explicit owned-session capability and supported CLI version. Cancellation stops pending continuation and does not undo an already completed selection. Preserve launch tool restrictions, permission modes and MCP context; never silently broaden permissions. Do not automatically enable interrupted-turn env vars on external sessions. If deliberately supported by a measured adapter, bound age and include receipt explaining continuation boundary.

**Dependencies:** SES-B…E; scenarios and operator acceptance requirements. Product must decide the cancellation/confirmation boundary for any owned drain/restart; current SPEC:75 requires explicit UI confirmation for future Desktop restart.

**DoD/checks:** End-to-end synthetic upstream with request count and unique tool-side-effect count; same event delivered twice; user cancels; unknown spend scope; no candidate; shared org block; old process still alive; ambiguous timeout after accepted request; response stream already started; stale resume; permission context unavailable. Zero automatic replay on ambiguous execution. Exact-version operator acceptance proves resumed task identity and at-most-once tool effects; Windows/provider/live coverage separately marked. Hosted nightly evidence and deployment/release approval remain independent.

## Checks actually performed

- Read repository source/contracts/scenarios/backlog with `rg`, `nl`, `sed` at the named commit.
- Opened official docs/changelog and primary developer issue threads using web search/open/find.
- No tests executed in this read-only packet; parent integration owns executable checks. No live provider, native activation, UI/device or Windows acceptance executed.
- Exact next task for module: parent corrects SCN-031/unsupported timing while shipping the quota list; schedule SES-A parser fix and SES-B evidence contract before starting SB-25 implementation.
