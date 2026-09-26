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
