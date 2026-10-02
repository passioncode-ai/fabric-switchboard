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
| SCN-025 | Keep an optional project rule in sight | draft |
| SCN-026 | Connect a coding agent to Switchboard | draft |
| SCN-027 | Keep saved accounts readable without repeated Keychain dialogs | draft |
| SCN-028 | Switch Claude Code without Keychain confirmations | draft |
| SCN-029 | Keep saved accounts usable for automatic switching | draft |
| SCN-030 | Get accounts back after reinstalling Switchboard | draft |
| SCN-031 | Move off an account that hit a limit the quota does not show | draft |
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
1. Choose + Add account → Sign in to another Claude (or Codex) account → Terminal opens the official CLI in an isolated home and a banner says to finish there; no form opens (0.5).
2. Finish the provider's sign-in → Switchboard notices completion on its own and adds the account, named by its email, in the default pool; no Finish click (0.5).
**Alt paths:** Cancel in the banner → the staged sign-in is cleaned once its Terminal process has exited. Closing Terminal without signing in → the banner reads “Sign-in ended without an account” with Try again and Dismiss. Sign in again on a row reuses its label and pool, so the same identity updates in place. CLI: `login begin` (label optional), `login status`, `login finish`. A second sign-in while one is open reads “Finish an existing sign-in before starting another.” The account was saved but its temporary folder could not be removed yet → the banner closes with “Account added to Claude. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.” The owner restarted and no longer knows the sign-in → the banner reads ended, and Cancel closes it with “Sign-in closed. Switchboard had already ended it; close its Terminal window if it is still open.” (0.5.1, `loginOutcome`). A sign-in reservation whose Terminal never ran expires after ten minutes.
**Expected result:** Provider login is captured only from the new home.
**UI elements:** Add account menu, sign-in banner (pending, adding, ended, error), account list, notice.
**States covered:** loading, pending, ended, error, success
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
2. Leave focus anywhere (navigation, heading, Retry, Stop rotation, a quota disclosure) while the 60-second background refresh runs → focus stays on the same control; when nothing changed the page is not re-rendered at all (0.4, B-15).
3. From Add account choose Import Claude Swap, then close Import → focus returns to the control that opened Add account; each open dialog has its own title and description ids (0.4, B-16).
**Alt paths:** Cancel a local edit → return without mutation. Official login cancellation waits for its Terminal process to exit, then cleans the staged profile. A background refresh that started before a Select, Edit, Remove or other change never overwrites the result of that change (0.4, B-05).
**Expected result:** Focus visible and restored; actions remain available; the shown selection is the latest one the user made.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Screen reader verification is recorded separately from visual review.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); 0.4 focus retention, unchanged-data skip, dialog handoff and mutation ordering in [design 0.4 evidence](../evidence/design-0.4.md) (browser demo + `scripts/test-ui-logic.mjs`); live-provider outcome and screen reader NOT_RUN.
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
1. Under In use now, the Claude Code or Codex CLI identity that is not yet saved shows Add to Switchboard → one click saves it in the default pool, named by its email; the card then reads “Saved as …” (0.5, no dialog).
**Alt paths:** An identity already saved shows “✓ Saved as …” and no button. Not signed in or unreadable reads so, with Retry for unreadable. Another account → SCN-003.
**Expected result:** The source CLI account stays active; a disabled stored account is not enabled by capture.
**UI elements:** In use now cards, Add to Switchboard, notice.
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
1. Choose + Add account → Import from Claude Swap → native import reads the standard local source into the default pool and reports imported, skipped and failed counts in one notice (0.5, no dialog).
**Alt paths:** Reimport → same identities update without duplicate accounts or enabling disabled rows. A Claude Swap session profile holding a later-expiring sign-in than its backup is imported instead (0.5.1). Claude Swap still running → the notice adds “Claude Swap is still running, so it keeps renewing those accounts and Switchboard follows its newest sign-ins”, and the Automatic switching bar names how many accounts it renews (0.5.1).
**Expected result:** Successful profiles remain available when other profiles fail; the user can correct the source and retry.
**UI elements:** Add account menu item, import result notice, account list.
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
1. Open Accounts → current Claude Code and Codex CLI identities appear above stored accounts, with matches highlighted. Choose Switch on an enabled Claude OAuth row → Claude Code uses that account; the row reads “✓ In use” with the In Claude Code badge (0.5: one click, no confirmation dialog — operator decision D-3).
**Alt paths:** A current identity without a stored match offers Add to Switchboard (SCN-018). Missing/unavailable source has a distinct state. Managed selection is in the row menu (Select for managed sessions) and reads “Next managed request”.
**Expected result:** Current CLI identity remains visibly separate from Selected for next request. Activation does not assert that an existing session reloaded or that a provider response succeeded.
**UI elements:** In use now cards, In Claude Code and Next managed request badges, Switch, row menu.
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
**Alt paths:** Missing data reads Usage unknown; unavailable quota and failed checks are separate. A failed check preserves the last successful observation; old data is visibly stale. Once any reported reset time has passed, the observation is stale and reads “Reset since check”; that window reads “usage unknown since reset” with the pre-reset figure as history (0.4, B-12). Accounts the monitor never polls — disabled, or not OAuth — read “Not checked automatically”; non-OAuth accounts offer no Check usage action because the provider has no quota endpoint for them (0.4, B-11).
**Expected result:** Measured zero is distinct from unavailable, missing, stale or reset quota. The last check and next retry are visible only for accounts the monitor actually checks.
**UI elements:** quota summary, progress, window disclosure, reset timestamps, source, health status, Check usage.
**States covered:** loading, unknown, unavailable, stale, failed, success
**Errors & recovery:** A sanitized check failure preserves the last good result and marks ineligibility; retry manually or wait for the scheduled retry.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Window/reset disclosure, stale fixture and failed/unavailable quota states observed in browser on 2026-09-26; reset-passed and not-monitored states observed on 2026-09-29 ([design 0.4 evidence](../evidence/design-0.4.md)), rules in `scripts/test-ui-logic.mjs`. Fixture data is synthetic; authenticated provider evidence is separate. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-022 — Configure and stop automatic rotation
**Persona:** P-01
**Goal:** Persist an explicit quota policy for one provider, pool and target.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. With two or more switchable Claude accounts and no native policy on, the Automatic switching bar offers Turn on for Claude Code → one click enables the `claude_cli` policy for the pool holding most of them with the defaults below, and the bar reads “Automatic switching is on” with the latest decision (0.5). Stop turns it off in one click.
2. For other boundaries, the ⚙ menu → New policy… or Edit … → defaults are off, threshold 90%, minimum improvement 10 points, cooldown 1800 seconds, maximum age 300 seconds. Choose provider, pool and managed/native Claude target; enable and save → the saved policy is shown.
**Alt paths:** Edit a saved policy → its boundary stays fixed and values reload. Stop rotation → the saved policy is disabled in one action. Codex cannot select the native Claude target.
**Expected result:** The policy is persisted by native storage; fresh eligible same-pool accounts are required. No eligible account means hold. Managed in-flight responses retain their identity.
**UI elements:** Automatic switching bar, Turn on / Stop, ⚙ settings menu, policy dialog with numeric fields, enabled checkbox and target selector, latest decision.
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
3. Open a dialog, change navigation, or use a narrow window → Shared roles and visible keyboard focus persist in the active theme.
4. Open About → Appearance offers System (default; follows the operating system's light/dark setting live), Dark and Light; the choice applies at once and is saved on this machine. If it cannot be saved, it applies until restart and About says so.
5. Read About → Version and license: the app version, “Open source under the GNU AGPL-3.0; a commercial license is available — contact@passioncode.ai.”, the LICENSE and third-party notice addresses, and “Part of the PassionCode.ai toolkit” with https://passioncode.ai/switchboard/. In the native app the addresses are selectable text (no in-app browser opener exists); in the browser demo they are links opening a new tab.
**Alt paths:** Unknown usage remains unknown; error and disabled states retain their words and actions. In Light, gold stays the action fill while gold-as-text (selection label, active navigation, selection bar, meter) and focus use the canonical gold-brown role.
**Expected result:** The shared design system (PassionCode 1.1.0) changes presentation without asserting an account was authenticated or changing routing behavior; every text/state pair drawn meets 4.5:1 and focus/boundary pairs 3:1 in both themes, except the canonical dark strong border (reported).
**UI elements:** native icon, sidebar mark/name, role tokens, selection label, current-identity badge, appearance radio group, version/license rows.
**States covered:** populated, empty, error, dialog, keyboard focus, dark, light, system, appearance not saved; native rendering separately unverified.
**Errors & recovery:** Visual identity never replaces textual state labels; Refresh/recovery actions retain their existing meanings.
**Status:** validated
**Meaning:** operator explicitly requested the dark/gold PassionCode system and S icon; observed product outcome remains separate.
**Coverage:** [0.3.1 design evidence](../evidence/design-0.3.1.md), [0.4 design evidence](../evidence/design-0.4.md); browser fixtures only; native rendering (WKWebView/WebView2), native link behaviour and live acceptance NOT_RUN.
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

## SCN-025 — Keep an optional project rule in sight
**Persona:** P-01
**Goal:** Start one project on a chosen account without forgetting that the rule exists.
**Preconditions:** At least one enabled account; rules are off unless saved (PLAN-0.4 D-2).
**Entry point:** SCR-01 Accounts → strip “N project rules are active” → Projects; or Projects in navigation.
**Steps:**
1. Open Projects → without rules, an empty state explains that selection and rotation apply everywhere.
2. Add rule → folder, account, target (managed sessions, or the Claude Code login for Claude OAuth profiles only), expiry (default 8 hours).
3. Return to Accounts → the strip names each active rule, its account and expiry; navigation shows the count.
4. Pause, Resume (asks for a new expiry), Edit or Remove a rule on Projects.
**Alt paths:** An expired or paused rule stays listed with its state and never applies; removing an account removes its rules.
**Expected result:** A rule applies only when an agent or `switchboard project apply` asks, only to that session, and never turns rotation off.
**UI elements:** rules strip, nav count, rule cards, rule dialog.
**States covered:** empty, active, paused, expired, error.
**Errors & recovery:** Relative or missing folders, past expiry and ineligible targets are refused with the backend's fixed messages.
**Status:** draft
**Meaning:** operator decision D-2 (2026-09-29); not yet observed in the native app.
**Coverage:** browser demo `docs/evidence/design-0.4/projects-light-1280.png`; `crates/switchboard-core/tests/projects.rs`; `scripts/test-ui-logic.mjs` rule cases.
**Product:** unobserved
**Traces:** PLAN-0.4 REQ-1, REQ-6

## SCN-026 — Connect a coding agent to Switchboard
**Persona:** P-01
**Goal:** Let a coding agent read remaining usage and switch accounts itself.
**Preconditions:** The switchboard CLI is on PATH or bundled in the macOS app.
**Entry point:** Agents in navigation.
**Steps:**
1. Open Agents → what agents can do, and the CLI location or “not found”.
2. Link switchboard into ~/.local/bin (macOS, bundled CLI) → the link is created; an existing file or foreign link is never replaced.
3. Copy the Claude Code, Codex or Claude Code plugin command and run it in a terminal.
**Alt paths:** Sessions launched from Switchboard get the tools without this step; a launch without a CLI says so in its notice.
**Expected result:** The agent's `switchboard_status` names its session; switching a managed session takes effect from the next request; the global Claude Code login needs `global: true`.
**UI elements:** capability panel, link button, command rows with Copy.
**States covered:** loading, unavailable, CLI missing, linked.
**Errors & recovery:** Link failures show fixed messages; Copy falls back to selectable text.
**Status:** draft
**Meaning:** operator request for agent self-switching (2026-09-29); live agent sessions not yet observed.
**Coverage:** `crates/switchboard-cli/tests/mcp.rs`; `crates/switchboard-runtime/src/agents.rs` link test; browser demo `docs/evidence/design-0.4/agents-1280.png`.
**Product:** unobserved
**Traces:** PLAN-0.4 REQ-3, REQ-5, REQ-6

## SCN-027 — Keep saved accounts readable without repeated Keychain dialogs
**Persona:** P-01
**Goal:** Use the app and agents (`switchboard mcp`) on the same accounts without macOS asking for Keychain access again and again.
**Preconditions:** macOS, the signed app in Applications; accounts saved by this or an earlier version.
**Entry point:** opening the app; an agent calling the bundled CLI.
**Steps:**
1. Save or sign in to an account in the app → the CLI and later signed updates read it with no dialog.
2. Open the app after updating from 0.4 or earlier → each account saved by an earlier version moves to shared storage; macOS asks at most once per account, and not at all for accounts the installed app could already read.
3. An agent uses the CLI before the app has moved an account → the CLI shows no dialog and says to open the app once.
**Alt paths:** Deny in the dialog → the account stays where it was and the app does not ask again until restart. A development build never reads the user's accounts.
**Expected result:** After the move, no Keychain dialog during ordinary use by the app or the CLI.
**UI elements:** macOS Keychain dialog (app only), status/error message.
**States covered:** moved silently, moved after one consent, declined, CLI before the move, development build.
**Errors & recovery:** Every refusal names the next step ([operations](../OPERATIONS.md)); a failed or unverified copy keeps the original.
**Status:** draft
**Meaning:** operator report of repeated Keychain dialogs (2026-10-01); design in [KEYCHAIN.md](../KEYCHAIN.md).
**Coverage:** `crates/switchboard-core/src/keychain.rs` tests (fake Keychain), `keychain_macos.rs` throwaway-keychain tests, manual signed-bundle acceptance in KEYCHAIN.md; the operator's real items after the next release NOT_RUN.
**Product:** unobserved
**Traces:** REQ-004
## SCN-028 — Switch Claude Code without Keychain confirmations
**Persona:** P-01
**Goal:** Switch the ordinary Claude Code between saved accounts, sign in to another account and let rotation run without macOS asking about `Claude Code-credentials`.
**Preconditions:** macOS; Claude Code signed in or signed out; accounts saved in Switchboard.
**Entry point:** SCR-01 Accounts (Switch), the CLI (`accounts activate`), `switchboard mcp` (`switchboard_switch` with `global: true`), automatic rotation.
**Steps:**
1. Switch, an automatic switch or a finished official sign-in → no Keychain dialog from Switchboard, and Claude Code itself reads its account afterwards without one (0.5).
**Alt paths:** Claude Code's MCP and plugin sign-ins stay signed in across a switch (0.5.1). Claude Code is updating its account → Switchboard waits up to 9 s and takes over a lock left behind by a crashed process (0.5.1). Refusals, each with nothing changed (0.5.1): the signed-in account is not saved in Switchboard (“Claude Code is signed in to an account Switchboard has not saved; switching would sign it out. Add it first (In use now → Add to Switchboard).”); the sign-in belongs to another account than its settings name (“The Claude Code sign-in does not match the account named in its settings. Sign in again in Claude Code (claude /login), then retry.”); a sign-in nothing attributes yet (“Switchboard could not confirm which account Claude Code is signed in to. Check the connection, or use Claude Code once, then retry.”); a renewed sign-in not yet stored (“Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.”); a session Switchboard launched (“This session runs in a Switchboard home. Switch the ordinary Claude Code from the app or a normal terminal.”). Switching to the account already in use changes nothing. A `Claude Code-credentials` item that a 0.4 build created while Claude Code was signed out trusts only that build; sign out and in once in Claude Code to recreate it ([operations](../OPERATIONS.md)). Switchboard's own account storage is SCN-027.
**Expected result:** Repeated switching raises no dialog for Switchboard or Claude Code.
**UI elements:** none new; the absence of a system dialog is the outcome.
**States covered:** Claude Code signed in, signed out, item created by an older build, Keychain locked.
**Errors & recovery:** A locked or denied Keychain reads “Keychain unavailable. Unlock it and allow access, then retry.”
**Status:** draft
**Meaning:** operator request 2026-10-02 ([PLAN-0.5](../PLAN-0.5.md) C-2, C-3): Claude's items are read, written and deleted only through `/usr/bin/security`, the executable Claude Code uses.
**Coverage:** `cargo test -p switchboard-core security_cli` (stdin transport, quoting, exit codes); measured `security` exit codes in [release-0.5](../evidence/release-0.5.md); on-screen absence on the operator's Mac not yet observed.
**Product:** unobserved
**Traces:** PLAN-0.5 REQ-2, REQ-3, REQ-18…REQ-20, REQ-23…REQ-26, REQ-28, REQ-35

## SCN-029 — Keep saved accounts usable for automatic switching
**Persona:** P-01
**Goal:** Saved accounts stay ready to switch to and to measure, hours or days after they were added.
**Preconditions:** Claude OAuth accounts saved; the desktop app or `switchboard serve` running.
**Entry point:** SCR-01 Accounts (rows, Automatic switching bar).
**Steps:**
1. Leave Switchboard running → inactive Claude accounts keep fresh quota and stay eligible; when the account in use reaches the threshold, rotation switches to one of them (0.5).
2. Switch to an account whose access token has expired → it is renewed first and Claude Code starts on it.
**Alt paths:** The provider rejects an account's sign-in → its row shows Sign in again as the primary action and a red badge; signing in updates the same row. The account signed in to the ordinary Claude Code is renewed by Claude Code; only when Claude Code has left it expired for more than five minutes does Switchboard renew it under Claude Code's locks, keeping Claude Code signed in (0.5.1). While Claude Swap runs, the accounts it manages are renewed by it and Switchboard follows (0.5.1).
**Expected result:** Rotation no longer stalls on `no_eligible_account` because saved tokens aged out.
**UI elements:** quota cell, Sign in again badge and action, Automatic switching bar decision text.
**States covered:** fresh, renewing, transient failure, sign-in required.
**Errors & recovery:** “This account's sign-in has ended. Sign in to it again.”; transient failures retry from 60 s up to 30 min.
**Status:** draft
**Meaning:** operator request 2026-10-02 (PLAN-0.5 C-4…C-6, D-2); fixture-tested against a local token endpoint; live provider renewal not yet observed.
**Coverage:** `cargo test -p switchboard-runtime refresh`; `native_rotation_switches_to_an_expired_but_renewable_account`; `an_account_with_an_expired_access_token_still_activates`; browser demo `docs/evidence/design-0.5/accounts-1280-full.png` (Meadow team row).
**Product:** unobserved
**Traces:** PLAN-0.5 REQ-4, REQ-5, REQ-6, REQ-21, REQ-22, REQ-27, REQ-30, REQ-31, REQ-33

## SCN-030 — Get accounts back after reinstalling Switchboard
**Persona:** P-01
**Goal:** Lose no saved account when Switchboard or its data folder is reinstalled on the same Mac.
**Preconditions:** the desktop app has run with accounts saved; the Keychain of this Mac is intact.
**Entry point:** About → Backups; CLI `switchboard backup`.
**Steps:**
1. Use Switchboard as usual → an encrypted backup appears in `~/Library/Application Support/Fabric Switchboard Backups` after changes and daily; About lists the newest five.
2. After a reinstall, open About → Backups → Restore on the newest → the missing accounts come back; the notice says how many were restored and how many were already here.
**Alt paths:** Back up now writes one at once. A backup from another Mac or after the Keychain was erased reads “This backup was made with another key and cannot be opened on this machine.” A restored account whose sign-in has since rotated shows Sign in again.
**Expected result:** Accounts and policies are back; nothing newer is overwritten; policies return switched off.
**UI elements:** Backups panel (folder, limit note, list, Restore, Back up now), notice.
**States covered:** no backups, list, last automatic backup failed, restored, foreign key.
**Errors & recovery:** every refusal is a fixed sentence naming the next step.
**Status:** draft
**Meaning:** operator request 2026-10-02 ([PLAN-0.5](../PLAN-0.5.md) D-5).
**Coverage:** `cargo test -p switchboard-core backup`; `backups_follow_changes_and_restore_into_a_fresh_install`; browser demo `docs/evidence/design-0.5/about-backups.png`.
**Product:** unobserved
**Traces:** PLAN-0.5 REQ-14, REQ-15, REQ-17

## SCN-031 — Move off an account that hit a limit the quota does not show
**Persona:** P-01
**Goal:** Keep working when Claude Code answers “You've hit your individual spend limit” although the quota still shows room.
**Preconditions:** automatic switching on for Claude Code; another saved account with fresh quota.
**Entry point:** automatic; SCR-01 shows the result.
**Steps:**
1. A Claude Code session hits a usage or spend limit → within about 30 seconds Switchboard switches Claude Code to the free account with the lowest usage, even inside the cooldown; the bar reads “Switched after the account in use hit a provider limit.”
2. The limited row shows “Limit reached · until …” and is not chosen again before then.
**Alt paths:** No other free account → “The account in use hit a provider limit, and no other account is free. Holding it.” Managed sessions are detected from the proxy's own 429 responses.
**Expected result:** work continues on another account; the limited one returns after its reset.
**UI elements:** row badge, Automatic switching decision text.
**States covered:** limited, switched on limit, all limited.
**Errors & recovery:** a failed activation reads as before (`activation_failed`).
**Status:** draft
**Meaning:** operator request 2026-10-02 ([PLAN-0.5](../PLAN-0.5.md) D-6); Claude Swap does not do this.
**Coverage:** `limits::tests`, `a_limit_error_in_claude_code_switches_with_quota_to_spare`, rotation test; marker shape measured on this machine's own transcripts (field names only).
**Product:** unobserved
**Traces:** PLAN-0.5 REQ-16, REQ-17
