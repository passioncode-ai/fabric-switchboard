Contract: ux-contract v4
# Scenarios
Product: unobserved
Approval basis: operator explicitly authorized autonomous design and implementation; these are design decisions, not observed user outcomes.
## Personas
- P-01: Developer managing authorized personal/work accounts.
## Index
| ID | Scenario | Status |
|---|---|---|
| SCN-001 | First run | validated |
| SCN-002 | Add a credential | validated |
| SCN-003 | Official sign-in | validated |
| SCN-004 | Choose account | validated |
| SCN-005 | Launch isolated | validated |
| SCN-006 | Launch managed | validated |
| SCN-007 | Switch during stream | validated |
| SCN-008 | Check usage | validated |
| SCN-009 | Edit and disable | validated |
| SCN-010 | Remove account | validated |
| SCN-011 | Review activity | validated |
| SCN-012 | Return after restart | validated |
| SCN-013 | Keyboard and narrow window | validated |

| SCN-014 | Command-line account management | validated |
| SCN-015 | CLI with a running desktop or server | validated |
| SCN-016 | CLI login and launch | validated |
| SCN-017 | Install native builds | validated |
| SCN-018 | Capture the current CLI account | validated |
| SCN-019 | Import Claude Swap profiles | validated |
| SCN-020 | Distinguish native identity and managed selection | validated |
| SCN-021 | Inspect quota windows and failed checks | validated |
| SCN-022 | Configure and stop automatic rotation | validated |
| SCN-023 | Recognize Switchboard across desktop surfaces | validated |
| SCN-024 | Open the app and recover a delayed startup | validated |
## SCN-001 — First run
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Open the app → An empty account list explains how to add an account.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** An empty account list explains how to add an account.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Missing vault permission is an error with retry; never show a fabricated account.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-002 — Add a credential
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Choose provider and kind, enter label, pool and secret; submit → One account appears; secret field is cleared.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** One account appears; secret field is cleared.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Malformed JSON, duplicate, incompatible kind, vault denial and disk failure leave no successful account.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-003 — Official sign-in
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Choose provider, label and pool, start sign-in in an isolated home, then finish → Provider login is captured only from the new home.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Provider login is captured only from the new home.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Missing CLI, incomplete login or denied Keychain keeps the previous list; cancel leaves global auth unchanged.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-004 — Choose account
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Select an enabled account → Route shows selected for the next request in its provider and pool.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Route shows selected for the next request in its provider and pool.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Unknown, disabled, expired or mismatched target fails with recovery instruction.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-005 — Launch isolated
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Choose an account, Launch isolated, and an existing absolute Project directory → Terminal starts the provider with its private home; selection affects new launches.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Terminal starts the provider with its private home; selection affects new launches.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Missing CLI or launch failure is shown; no change to existing clients.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-006 — Launch managed
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Select an account, Launch managed, and an existing absolute Project directory → Terminal starts against the local proxy; UI explains next-request effect.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Terminal starts against the local proxy; UI explains next-request effect.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** No route, unavailable proxy or unsupported credential fails before launch.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-007 — Switch during stream
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. While one managed response is streaming, select another same-pool account → First stream keeps its identity; next request uses selected account.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** First stream keeps its identity; next request uses selected account.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Upstream errors do not trigger replay or duplicate effects.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-008 — Check usage
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Choose Check usage → Timestamped provider usage appears with its source.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Timestamped provider usage appears with its source.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Unavailable, stale, expired or invalid observation is never displayed as zero.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-009 — Edit and disable
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Edit label or disable account → Label updates; disabled account stays visible and is removed from active route.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Label updates; disabled account stays visible and is removed from active route.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Blank/oversized labels rejected; persistence error leaves prior state.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-010 — Remove account
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Choose Remove, inspect confirmation, confirm → Unselected account and its vault entry are removed.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Unselected account and its vault entry are removed.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Selected account refuses deletion; cancellation changes nothing.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-011 — Review activity
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Open Activity → Bounded journal shows account IDs and operation outcomes without tokens or prompts.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Bounded journal shows account IDs and operation outcomes without tokens or prompts.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Read failure is visible with retry; empty journal explained.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-012 — Return after restart
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Reopen the app → Persisted accounts and routes load; proxy gets new capability.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Persisted accounts and routes load; proxy gets new capability.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Corrupt or future-schema metadata and second instance are explicit errors.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-013 — Keyboard and narrow window
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Navigate with keyboard, open/cancel dialog; resize window → Focus visible and restored; actions remain available.
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile.
**Expected result:** Focus visible and restored; actions remain available.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Screen reader verification is recorded separately from visual review.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-014 — Command-line account management
**Persona:** P-01
**Goal:** Manage authorized coding accounts from a terminal or native OS build.
**Preconditions:** User-owned accounts; synthetic fixtures for tests.
**Entry point:** CLI or native installer
**Steps:**
1. Run switchboard accounts list/add/select/update/remove with explicit UUID, provider/pool and stdin credential → backend reports the same state as the desktop, never credential material.
**Alt paths:** Use --help without opening storage; cancel before secret input without mutation.
**Expected result:** One consistent account state; credentials never in output.
**UI elements:** terminal help, stdout, stderr, exit code, native shell.
**States covered:** empty, success, error, running
**Errors & recovery:** Invalid input, vault denial, selected removal and unavailable owner return a nonzero exit code with actionable sanitized error.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; native execution and user outcomes are separate.
**Coverage:** [0.2 execution evidence](../evidence/release-0.2.md): CLI/runtime fixtures and signed macOS smoke passed; Windows cross-build passed, native Windows/provider acceptance NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-015 — CLI with a running desktop or server
**Persona:** P-01
**Goal:** Manage authorized coding accounts from a terminal or native OS build.
**Preconditions:** User-owned accounts; synthetic fixtures for tests.
**Entry point:** CLI or native installer
**Steps:**
1. Start GUI or switchboard serve, then select through CLI → active owner persists the route; next request uses the new account.
**Alt paths:** Use --help without opening storage; cancel before secret input without mutation.
**Expected result:** One consistent account state; credentials never in output.
**UI elements:** terminal help, stdout, stderr, exit code, native shell.
**States covered:** empty, success, error, running
**Errors & recovery:** Invalid/stale capability and wrong host/origin refuse; no second process bypasses the Store lock.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; native execution and user outcomes are separate.
**Coverage:** [0.2 execution evidence](../evidence/release-0.2.md): CLI/runtime fixtures and signed macOS smoke passed; Windows cross-build passed, native Windows/provider acceptance NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-016 — CLI login and launch
**Persona:** P-01
**Goal:** Manage authorized coding accounts from a terminal or native OS build.
**Preconditions:** User-owned accounts; synthetic fixtures for tests.
**Entry point:** CLI or native installer
**Steps:**
1. Use login/launch with explicit account and project directory → official provider flow uses a private home and managed launch requires a live owner.
**Alt paths:** Use --help without opening storage; cancel before secret input without mutation.
**Expected result:** One consistent account state; credentials never in output.
**UI elements:** terminal help, stdout, stderr, exit code, native shell.
**States covered:** empty, success, error, running
**Errors & recovery:** Incomplete sign-in, missing CLI, active home, server absence and invalid project path return recovery instructions.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; native execution and user outcomes are separate.
**Coverage:** [0.2 execution evidence](../evidence/release-0.2.md): CLI/runtime fixtures and signed macOS smoke passed; Windows cross-build passed, native Windows/provider acceptance NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-017 — Install native builds
**Persona:** P-01
**Goal:** Manage authorized coding accounts from a terminal or native OS build.
**Preconditions:** User-owned accounts; synthetic fixtures for tests.
**Entry point:** CLI or native installer
**Steps:**
1. Open the signed macOS app or install the Windows build → app starts with the appropriate OS vault and shared CLI metadata root.
**Alt paths:** Use --help without opening storage; cancel before secret input without mutation.
**Expected result:** One consistent account state; credentials never in output.
**UI elements:** terminal help, stdout, stderr, exit code, native shell.
**States covered:** empty, success, error, running
**Errors & recovery:** Signing, notarization, Windows installer build and live provider compatibility remain separately evidenced.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; native execution and user outcomes are separate.
**Coverage:** [0.2 execution evidence](../evidence/release-0.2.md): CLI/runtime fixtures and signed macOS smoke passed; Windows cross-build passed, native Windows/provider acceptance NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-018 — Capture the current CLI account
**Persona:** P-01
**Goal:** Reuse an account already signed in without another login.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Choose Add account → Capture current CLI account is selected. Choose provider and pool, optionally enter a label, then capture → the account appears or its same-pool identity is updated.
**Alt paths:** Choose official sign-in with another account → the existing isolated Terminal login flow remains available. Cancel before capture → no mutation.
**Expected result:** The source CLI account stays active; a disabled stored account is not enabled by capture.
**UI elements:** provider selector, authentication selector, optional account label, pool, capture button, current CLI cards.
**States covered:** loading, missing account, error, success
**Errors & recovery:** A missing, unreadable or incompatible source produces a sanitized recovery message. Choose official sign-in or correct native storage access, then retry.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Capture-first dialog and blank-label capture observed in the synthetic browser demo on 2026-09-26; native capture acceptance is separate. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-019 — Import Claude Swap profiles
**Persona:** P-01
**Goal:** Reuse saved Claude Swap profiles in an explicit routing pool.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Choose Import Claude Swap, enter the pool, then Import profiles → native import reads the standard local source and reports imported, skipped and failed counts.
**Alt paths:** Reimport → same identities update without duplicate accounts or enabling disabled rows. Cancel → no mutation.
**Expected result:** Successful profiles remain available when other profiles fail; the user can correct the source and retry.
**UI elements:** import button, pool input, import result notice, account list.
**States covered:** empty source, loading, partial success, error, success
**Errors & recovery:** Malformed profiles are counted as failed without raw source content in errors. Missing/unreadable source directs the user to check the local source before retrying.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Synthetic partial-import result (two imported, one skipped, one failed) observed in the browser on 2026-09-26; no real profile was read. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-020 — Distinguish native identity and managed selection
**Persona:** P-01
**Goal:** Know which account the local CLI has and deliberately activate another Claude account.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Open Accounts → current Claude Code and Codex CLI identities appear above stored accounts, with matches highlighted. Choose Activate in Claude Code on an enabled Claude OAuth account with external identity, review the effect, then activate → native identity refreshes.
**Alt paths:** A current identity without a stored match says so and offers capture. Missing/unavailable source has a distinct state. Cancel activation → no mutation.
**Expected result:** Current CLI identity remains visibly separate from Selected for next request. Activation does not assert that an existing session reloaded or that a provider response succeeded.
**UI elements:** current CLI cards, account match badges, managed selection label, activation action and dialog.
**States covered:** loading, missing identity, unmatched identity, unavailable, error, success
**Errors & recovery:** Current-source changes, native locks or failed activation produce a sanitized message; refresh identity and retry after the external conflict is resolved.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Matched identities, separate managed selection, explicit activation dialog and synthetic success observed in browser on 2026-09-26; real CLI reload remains unobserved. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-021 — Inspect quota windows and failed checks
**Persona:** P-01
**Goal:** See measured usage, when it resets and whether it is fresh enough for rotation.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Expand Quota windows → each provider window shows usage and reset time, source and observation time. Check usage → the observation updates on success.
**Alt paths:** Missing data reads Usage unknown; unavailable quota and failed checks are separate. A failed check preserves the last successful observation; old data is visibly stale.
**Expected result:** Measured zero is distinct from unavailable, missing or stale quota. The last check and next retry are visible when supplied by the monitor.
**UI elements:** quota summary, progress, window disclosure, reset timestamps, source, health status, Check usage.
**States covered:** loading, unknown, unavailable, stale, failed, success
**Errors & recovery:** A sanitized check failure preserves the last good result and marks ineligibility; retry manually or wait for the scheduled retry.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Window/reset disclosure, stale fixture and failed/unavailable quota states observed in browser on 2026-09-26. Fixture data is synthetic; authenticated provider evidence is separate. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-022 — Configure and stop automatic rotation
**Persona:** P-01
**Goal:** Persist an explicit quota policy for one provider, pool and target.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Expand Automatic rotation, then Configure rotation → defaults are off, threshold 90%, minimum improvement 10 points, cooldown 1800 seconds, maximum age 300 seconds. Choose provider, pool and managed/native Claude target; enable and save → the saved policy is shown.
**Alt paths:** Edit a saved policy → its boundary stays fixed and values reload. Stop rotation → the saved policy is disabled in one action. Codex cannot select the native Claude target.
**Expected result:** The policy is persisted by native storage; fresh eligible same-pool accounts are required. No eligible account means hold. Managed in-flight responses retain their identity.
**UI elements:** monitor status, policy list, numeric fields, enabled checkbox, target selector, save/edit/stop actions, last switch and decision.
**States covered:** empty policies, disabled, enabled, loading, invalid input, monitor unavailable, hold, success
**Errors & recovery:** Invalid bounds or failed persistence keep the dialog open with a sanitized recovery message. A failed or stale quota check never implies spare capacity.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Save-enable and one-action stop observed using the synthetic browser fixture on 2026-09-26; demo never performs automatic switching. Runtime policy execution is separately tested. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-023 — Recognize Switchboard across desktop surfaces
**Persona:** P-01
**Goal:** Recognize the PassionCode tool and distinguish current identity from next-request selection.
**Preconditions:** App or synthetic browser demo; operating-system light/dark preference may vary.
**Entry point:** App launcher → SCR-01 Accounts; SCR-02 Activity; About.
**Steps:**
1. Open Switchboard → Yellow S on the dark app icon matches the sidebar; Switchboard and PassionCode are named.
2. Read account states → Gold selection names the next managed request; blue “Current CLI account” names the separate local identity.
3. Open a dialog, change navigation, or use a narrow window → Shared dark roles and visible keyboard focus persist.
**Alt paths:** Unknown usage remains unknown; error and disabled states retain their words and actions.
**Expected result:** The shared design system changes presentation without asserting an account was authenticated or changing routing behavior.
**UI elements:** native icon, sidebar mark/name, role tokens, selection label, current-identity badge.
**States covered:** populated, empty, error, dialog, keyboard focus; native rendering separately unverified.
**Errors & recovery:** Visual identity never replaces textual state labels; Refresh/recovery actions retain their existing meanings.
**Status:** validated
**Meaning:** operator explicitly requested the dark/gold PassionCode system and S icon; observed product outcome remains separate.
**Coverage:** [0.3.1 design evidence](../evidence/design-0.3.1.md); browser fixtures only; native/live acceptance NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-024 — Open the app and recover a delayed startup
**Persona:** P-01
**Goal:** Open the account workbench without waiting indefinitely for native identity checks.
**Preconditions:** Supported macOS/Windows; native startup verification uses a temporary empty store and memory vault.
**Entry point:** App launcher → SCR-01 Accounts
**Steps:**
1. Open Switchboard → account metadata and runtime status load; native CLI identity loads independently.
2. If a metadata read stops responding → loading ends with an actionable error and Retry.
**Alt paths:** A delayed current-account read leaves the main account list usable and reports identity unavailable. Mutation completion is never inferred from a timeout.
**Expected result:** No indefinite startup spinner. A package smoke check requires frontend render and native round trips from the built executable.
**UI elements:** loading state, account list, current identity, Retry.
**States covered:** loading, empty, ready, unavailable, retry
**Errors & recovery:** Native reads time out after 12 seconds; Retry requests fresh metadata. Existing in-flight mutations are not replayed.
**Status:** validated
**Meaning:** implements the operator request to repair launch and verify the distributed build.
**Coverage:** pending focused and native build checks in launch-repair evidence.
**Product:** unobserved
**Traces:** ST-001, FLW-01
