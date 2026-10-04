# Unified execution plan — Fabric Switchboard

Entry: [brief](README.md), [dated research](../../reports/2026-10-04-system-review/README.md), [canonical board](../../evidence/backlog.md). Task status lives only in that board; this document owns order, dependencies and context. Baseline `46ee4b9`; observations are scoped to 2026-10-04. Re-read current HANDOFF/remote before starting a packet.

## Priority goal and constraints

Keep the same local work under the deliberate account, with usable quota visible, credentials preserved, and no repeated tool effects or unintended spending. This is SPEC §3 under the workspace's workflow-control/reliable-work goals. Source: SPEC, CONTRACTS, ACCOUNTS-AND-ROTATION, PLAN-0.5 and the existing canonical board. The requested sorting/countdown is the immediate operator outcome. Reproduced data loss and false quota freshness outrank speculative additions.

P1 blocks an outcome; P2 degrades it; P3 is hygiene. Within a priority: confirmed data loss/incorrect identity or capacity first; then the operator's requested outcome; then a source-proven compatibility gap; then hypotheses/optional features. Effort does not hide severity. Accepted limitations and external acceptance stay visible. A proposed packet is not permission to modify global auth, replay requests, kill clients or approve a release.

## Queue and dependency edges

| Order | Canonical row | Work packet | Dependency/input it carries |
|---|---|---|---|
| 1 | SB-36 | Conservative quota evidence age | Baseline; protects current UI and every future candidate decision |
| 2 | SB-37 | Immutable backup generations | Baseline; protects rollback/recovery before additional automation |
| 3 | SB-38 | Timestamp-only session metadata | Baseline; prevents content-shaped misattribution without wake changes |
| 4 | SB-35 | Remaining quota order + date/countdown | SB-36 freshness semantics; SCN-032; no auth or routing change |
| 5 | SB-39 | Provider retry deadline across UI/CLI/MCP | Quota research QUOTA-03; one not-before contract used by parser, scheduler and callers |
| 6 | SB-40 | Additional Codex limit dimensions | QUOTA-04; scoped/unknown limit fields feed typed evidence |
| 7 | SB-41 | Typed, attributable limit evidence | SB-36/38/40 evidence; SES-B identity/scope; prerequisite to wake |
| 8 | SB-06, SB-01 | Versioned live adoption/resume probe | SES-C; typed evidence and explicit operator-authorized accounts |
| 9 | SB-25 | Deliberate interrupted-session continuation | SB-41 + SB-06 verified identity/process behavior; SES-D/E; preserve no-replay |
| parallel | SB-15, SB-02 | macOS/Windows product acceptance | Installed exact artifact, live accounts/host; QUOTA-01 and SYS-WINDOWS |
| parallel | SB-34, SB-31 | beta/stable release | Local gate, integration, release policy, operator approval; SYS-RELEASE |
| later | SB-42 | Structured sign-in save/cleanup result | SYS-LOGIN; existing saved-with-cleanup UI handling retained |
| later | SB-43 | Real-client protocol compatibility | SYS-MCP; installed CLI versions and agent-stack interop route |
| later | SB-28, SB-30 | Residency/background semantics | SYS-RESIDENCY; explicit existing operator decision; no accidental daemon |
| later | SB-29 | Uninstall/retention and adaptive cadence | SYS-RECOVERY + QUOTA-05; account ownership and freshness budget |
| later | SB-03, SB-32, SB-05 | Windows signatures and CLI packaging | Azure profile/host; SYS-WINDOWS; installer artifact-byte proof |
| later | SB-07, SB-08 | Carry-over reconciliation | Existing canonical receipts/failure reproduction; closure requires proof |
| later | SB-13, SB-16, SB-20 | Deferred vault, Codex renewal, home sharing | Explicit decisions remain; isolated access-only homes and refresh ownership |

Every arrow carries a concrete contract, not a generic “done”: age→eligibility; generation name→nonreplacement; limit event→identity+scope; CLI probe→verified process/identity; release→installed artifact hash. Unordered packets may not edit the same file. Root serializes shared register leases; parallel research files remain disjoint. `agent-sync` task/resource leases and Git refs are separate from collaboration messages.

## Packet SB-35 — list order and waits

Scope: `src/ui-logic.ts`, account/quota rendering in `src/main.ts`, existing CSS token roles, browser demo and UI logic tests; SCN-021/032, screen/brand string records. Input: typed Account/Usage/health/monitor metadata, conservative age from SB-36. No credential IPC or backend rotation change.

Algorithm: retain Claude/Codex and pool groups. Enabled OAuth with finite 0..100 evidence, successful/not-failed health and age within the applicable strictest enabled policy max_age (300 s without policy) ranks available; use maximum utilization across windows. Current-account and selected badges do not pin order. Available rank by most headroom, then constrained accounts by earliest known overall wait, then unknown, sign-in-required, disabled. Stable tie: creation time then UUID. For exhausted windows use latest reset; any missing blocking reset means unknown. A runtime limit hold is separate from a reported reset. No inference of provider acceptance/credits/price.

Time: local absolute date/time remains visible, timezone in timestamp help; relative duration uses days/hours/minutes from current wall clock, under a minute reads <1m; zero reads Due · awaiting check. Past reset invalidates quota. One minute text-only timer exists only while visible Accounts has timers; pause freezes text across background renders, newly disclosed time reads Countdown paused; resume recomputes. No tick announcement, network action, route selection, focus movement or list reordering from the countdown tick.

DoD: fail-first pure-clock order/edge tests, synthetic viewport inspection with actual widths and interaction coverage recorded, unchanged provider/pool boundaries and policies, no fabricated availability. This run inspected 740 CSSpx and a large window, pause/focus/expand; 1280 fixture, dark theme and final semantic-wrapper screenshot remain unobserved. Evidence is [verification](../../evidence/quota-order-2026-10-04.md), not live account acceptance.

## Packet SB-36 — oldest retained quota evidence

Scope: core `merge_windows`/`observe_generation`, rotation/storage regressions; no metadata migration. Partial headers carry omitted unexpired windows as history, so aggregate observed_at retains oldest evidence; latest health timestamp orders responses. Authoritative JSON replaces old windows and can recover. DoD: old low weekly/model window beyond max_age cannot authorize rotation after repeated partials; delayed older responses cannot overwrite a later successful or failed check; credential generation races remain protected; JSON restores eligibility. Full detail and fail-first receipts: [quota research QUOTA-02](../../reports/2026-10-04-system-review/raw/quota.md).

## Packet SB-37 — collision-safe backups

Scope: core backup.rs only plus embedded tests; no key/AAD/schema/retention change. Timestamp+UUID generation with exclusive private reservation, then synced atomic publication. Old timestamp-only names still list/restore. Existing destination, symlink and other-store backup are never replaced. Prune only completed own envelopes; keep fullest previous generation. Same-second ordering is not a chronology guarantee. DoD: fail-first full→partial and cross-store same-time regressions, legacy restore, collision/symlink refusal and private permissions. Context: [SYS-BACKUP](../../reports/2026-10-04-system-review/raw/system.md).

## Packet SB-38 — timestamp-only transcript header

Scope: runtime limits.rs, bounded 64KiB head scan, timestamp-only Deserialize type ignoring content fields. Marker tail remains byte-filtered and bounded; no prompt/error body enters Git/journal/IPC. Explain the 60 s grace as a heuristic; leave actual adoption SB-06. DoD: opaque valid JSON content cannot hide the old session timestamp or charge its reset-less marker to a new account; existing 13 limits tests; no attribution/wake policy change. Context: [SES-A](../../reports/2026-10-04-system-review/raw/sessions.md).

## Packet SB-39 — quota check cooldown

Problem: manual `Operation::Usage` reaches the endpoint despite next_check_at; numeric-only Retry-After and six-hour cap can shorten vendor waits. Scope: runtime check/operation and proxy parser, test fixtures, CLI/MCP contracts; no inference-request retry. Implement typed deadline separating ordinary failure backoff from server not-before. Accept seconds and RFC9110 HTTP-date with injected received-at; invalid/zero gets existing safety floor. Preserve longer valid waits, checked generation, restart handling, actionable manual response. Coalesce simultaneous same-account checks without holding global mutation while waiting; a new credential generation must not inherit unrelated stale failure. Test call counter zero before deadline, one after/coalesced, long/date/invalid/overflow cases, restart, clock rollback, renderer no raw response. DoD full gate plus packet QUOTA-03. First next autonomous code packet after this delivery.

## Packet SB-40 — provider compatibility

Problem: Codex primary/secondary percentages omit newer feature limits, credits, spend control/reached-limit reason. Scope: sanitized payload fixtures and bounded parsed metadata, core eligibility and compatible renderer unknown states. Read/pin current official backend schema; distinguish account/feature/spend/credit dimensions and missing vs zero. No billing purchase/reset operation. Test plan without primary, feature exhausted while primary spare, monetary block independent of percentage, unknown new enum and no secret/body leakage. Dependencies: SB-39 deadline; coordinate SB-41 schema ownership. DoD source pin, synthetic compatibility matrix, conservative unknowns; live payload acceptance remains separate. QUOTA-04 carries provenance.

## Packet SB-41 and SB-25 — continuation work graph

Use [session packets SES-B through SES-F](../../reports/2026-10-04-system-review/raw/sessions.md) as the complete context. SES-B adds bounded event_id/provider/pool/target/account identity revision/session/time/kind/scope/reset/retry-not-before/confidence, with unknown a valid final state. Preserve hierarchy: an organisation spend limit cannot be solved by another seat. Equal reset dates and temporal proximity are not identity proof.

Then SES-C measures exact CLI/OS/mode and resume PID identity with an operator; explicit session ID, no arbitrary latest lookup. Official interrupted-turn resume environment support is a versioned probe candidate, not a generic retry. A running-background attach can reuse old process/auth. SES-D/E define opt-in supervisor states and dedupe, owned process/handoff path, one wake per verified limit event, tool/approval boundaries, failure/ambiguity holds, no exhausted-account loop and off switch. No original prompt/tool replay. SES-F supplies synthetic and separately live acceptance ladder. Each stage updates source scenarios/contract and passes local gate before the next; missing external proof stops only its dependent wake path. Root must never mark stopped-session continuation done from native activation alone.

## Packets for every existing carry-over

| Owner | Context, bounded work and completion criterion |
|---|---|
| SB-01 / SB-15 | Existing provider-acceptance packet + SCN028–031. Exact installed hash/CLI versions; operator drives account sign-in, next-request identity, inactivity renewal, limit rotation, backups and one day without consent dialogs. Keep per-step executed/unobserved states. Never read global credentials to test. |
| SB-02 / SB-05 / SB-03 / SB-32 | SYS-WINDOWS packet: real Windows11 install/upgrade/reinstall, DPAPI user separation, Unicode paths, actual MCP call, keyboard/screen-reader; sidecar installed SHA/PATH if added. Azure certificate is separate; NSIS uninstaller signing requires its own receipt. Build != installation != signing. |
| SB-34 / SB-31 | SYS-RELEASE and DISTRIBUTION. Integrate candidate, local check, exact-SHA beta/stable release PR/tag/CI, required human gate, verify downloads/signatures/attestations, site, install/rollback. Then idle children/CPU, bounded log, managed-session quit/restart acceptance. No full-suite dispatch after ordinary push. |
| SB-07 | Preserve printed tool-error diagnostic; reproduce second project-rule apply failure using synthetic fixture and concurrency/owner ordering. A passing rerun without cause does not close it. Add regression only when mechanism established. |
| SB-08 | Reconcile plugin skill P-E and site P-F with their existing canonical owner and artifact receipts. Source ships today, but historic stopped packet needs explicit completion proof; link existing upstream release work, do not rewrite another repo here. |
| SB-11 | Reconciled in this run against scheduled run/job receipt identified by SYS-RELEASE; canonical closure links SHA/run/job. No new workflow dispatch or implementation remains for that row. |
| SB-13 | Existing rejected Data Protection Keychain alternative; operator decides AppID/provisioning/CLI app-like bundle before migration. Preserve current DeveloperID app/CLI trust. Not a required quota fix. |
| SB-16 | Inactive Codex renewal design reads pinned provider auth manager and RFC9700; single owner, scope/generation/rejected token persistence, active-client exclusion and successor safety. Synthetic tests before operator-assisted auth; no independent refresher in access-only session homes. |
| SB-20 | Decide home-sharing surface and trust boundary first. Allowlisted settings/skills/MCP paths, no credentials/environment copy; capture snapshots and avoid changes to other agents. Scenario for missing/unsafe shared config and project override. Existing minimal homes stay deliberate. |
| SB-28 / SB-30 | SYS-RESIDENCY packet: choose explicit Quit vs hide-on-close, tray Show/Quit, background no-focus and opt-in autostart, one owner, bounded drain. Existing operator-decision boundary remains. |
| SB-29 | QUOTA-05 simulates 1/12/24/64 accounts, slow/failing probes, policies and reset crossings; do not improve quietness by inventing fresh data. SYS-RECOVERY handles only app-owned links/items and keep-data vs purge; destructive purge stays explicit. |
| SB-42 | SYS-LOGIN: structured saved/pending/saved_cleanup result across desktop/CLI/MCP; account already saved must not imply failed provider sign-in. Preserve existing desktop loginOutcome handling. Test repeated finish, cleanup fail/retry and owner restart; pending-login persistence is separate. |
| SB-43 | SYS-MCP: exact pinned Claude/Codex and Inspector proving call, legacy/modern negotiation, bounded framing/EOF, read-only and global-switch authority. Agent-stack interop route when building protocol change; no modern version advertised without implementing its lifecycle. |

## Completion / resume policy

This run delivers the research matrix, one queue and the requested UI plus reproduced bounded fixes. Future roadmap nodes remain open, not claimed implemented. Canonical board stores closure receipts. External provider/OS/release gates remain prerequisites, not synthetic successes. No project role/version/dependency/test-command change is introduced here. Workspace report index is updated after push; source publication follows the repository's scheduled sync. Next agent starts with SB-39 and its explicit synthetic deadline contract.
