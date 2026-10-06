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
| SCN-032 | Find remaining quota and compare account waits | draft |
| SCN-033 | Keep Switchboard working with its window closed | draft |
| SCN-034 | Keep a project on its own accounts | draft |
| SCN-035 | Learn the product in five steps on first start | draft |
| SCN-036 | Run another coding agent on Switchboard | draft |
| SCN-037 | Get new versions without doing anything | draft |
| SCN-038 | Continue a workflow on another account when a limit runs out | draft |
## SCN-001 — First run
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Open the app → An empty account list explains how to add an account.
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel in the banner while Terminal is still open → “Finish or close sign-in in Terminal before cancelling.”; after Terminal closed → the staged sign-in is cleaned. Closing Terminal without signing in → the banner reads “Sign-in ended without an account” with Try again and Dismiss. Sign in on a row reuses its label and pool, and Try again in the banner repeats the sign-in it replaces with the same label and pool (0.6.6, SB-62). A sign-in of an identity already saved — from a row, from + Add account or from Try again — updates every saved copy of it where it is, keeps their labels and pools, and reads “{label} is signed in again in {provider} · {pool}.”; it never renames an account to its email and never copies an account (a project's) into another pool (`a_sign_in_without_a_label_updates_the_saved_account_where_it_is`, `a_row_sign_in_updates_its_row_and_every_other_copy_of_the_identity`). CLI: `login begin` (label optional), `login status`, `login finish`. A second sign-in while one is open reads “A sign-in is already in progress. Finish it in Terminal or cancel it first.” The account was saved but its temporary folder could not be removed yet → the banner closes with “{label} added to {provider} · {pool}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.” The owner restarted and no longer knows the sign-in → the banner reads ended, and Cancel closes it with “Sign-in closed. Switchboard had already ended it; close its Terminal window if it is still open.” (0.5.1, `loginOutcome`). A sign-in reservation whose Terminal never ran expires after ten minutes. The account is saved but its temporary sign-in folder cannot be removed yet → the banner closes with “{label} added to {provider} · {pool}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.”; finishing again returns the same account (SB-42).
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile.
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile. Opening Switchboard again while it runs focuses the open window (0.5.3). Started in the background by the local lifecycle broker (`--background`), it shows no window and takes no focus; opening it again shows the window, with fresh metadata (SB-30; background smoke measured, the live reveal path is operator acceptance). The store is held by `switchboard serve`, a running command or a window opened with `open -n` → the window opens and reads “Switchboard's account store is in use by another Switchboard (a second window, 'switchboard serve' or a command still running). Close it, then retry.”; Retry starts it once the store is free — the app no longer quits unexpectedly (0.5.3).
**Expected result:** Persisted accounts and routes load; proxy gets new capability.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Corrupt or future-schema metadata is an explicit error; a second instance focuses the first.
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
2. Leave focus anywhere (navigation, heading, Retry, Stop, a quota disclosure) while the 60-second background refresh runs → focus stays on the same control; when nothing changed the page is not re-rendered at all (0.4, B-15).
3. From + Add account choose Import from Claude Swap → it runs with no dialog (since 0.5) and focus returns to + Add account; each dialog that does open has its own title and description ids (0.4, B-16).
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in while its Terminal is still open reads “Finish or close sign-in in Terminal before cancelling.”; cancelling after Terminal closed cleans the staged profile. A background refresh that started before a Select, Edit, Remove or other change never overwrites the result of that change (0.4, B-05).
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
**Alt paths:** Use --help without opening storage; cancel before secret input without mutation. Launch in another app's console (SB-75): `switchboard launch --provider claude|codex --working-directory <dir> --in-place [-- <agent args>]` runs the same session in the calling terminal, on the account Switchboard would use for the folder; refused without a terminal on stdin/stdout (“Launching in place needs a terminal on stdin and stdout.”), agent arguments without `--in-place`, and a folder whose project or pool has no selected account (“Select this project's account for the provider first …” / “Select an account for this provider first …”).
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
**Alt paths:** Missing data reads Usage unknown; unavailable quota and failed checks are separate. A failed check preserves the last successful observation; old data is visibly stale. Once any reported reset time has passed, the observation is stale and reads “Reset since check”; that window reads “usage unknown since reset” with the pre-reset figure as history (0.4, B-12). Without a rotation policy an observation stays current for 15 minutes, matching the ten-minute idle check (SB-48); a policy's maximum age governs its pool. Accounts the monitor never polls — disabled, or not OAuth — read “Not checked automatically”; non-OAuth accounts offer no Check usage action because the provider has no quota endpoint for them (0.4, B-11).
**Expected result:** Measured zero is distinct from unavailable, missing, stale or reset quota. The last check and next retry are visible only for accounts the monitor actually checks.
**UI elements:** quota summary, progress, window disclosure, reset timestamps, source, health status, Check usage.
**States covered:** loading, unknown, unavailable, stale, failed, success
**Errors & recovery:** A sanitized check failure preserves the last good result and marks ineligibility; retry manually or wait for the scheduled retry. When the provider answered a check with “too many requests”, Check usage answers “Usage checks are rate limited by the provider. Switchboard waits before the next one.” at once until the provider's wait ends — in the app, the CLI and MCP alike — and a failed row shows “Check failed · next {date}” on its second line (SB-39). An ordinary failure can be retried at once. A Codex account whose one metered feature is exhausted keeps its own percentage in the row; the disclosure lists that window as “{feature} · primary feature limit” (SB-40); an account that reported only feature limits reads “Usage unknown”.
**Status:** validated
**Meaning:** follows the operator-authorized v0.3 plan; scenario approval does not establish real-provider acceptance.
**Coverage:** Window/reset disclosure, stale fixture and failed/unavailable quota states observed in browser on 2026-09-26; reset-passed and not-monitored states observed on 2026-09-29 ([design 0.4 evidence](../evidence/design-0.4.md)), rules in `scripts/test-ui-logic.mjs`. The provider-wait row (“Check failed · Next check {date}”, Check usage refused at once) observed with `?demo=1&quota-review=1` on 2026-10-04 at 1280 CSS px light and dark and at 740 CSS px (SB-39, [run](../runs/2026-10-04-sb-39-quota-deadline/README.md)). Fixture data is synthetic; authenticated provider evidence is separate. Implementation: [interface](../../src/main.ts), [adapter](../../src/adapter.ts), [synthetic fixtures](../../src/demo.ts).
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
**Alt paths:** Edit a saved policy → its boundary stays fixed and values reload. Stop (Stop {pool} when several pools switch) → the saved policy is disabled in one action. Codex cannot select the native Claude target. Claude Swap switches Claude Code itself (`cswap auto`, or its menu bar with automatic switching on) → the bar reads “Claude Swap is switching Claude Code automatically, so Switchboard does not. Turn off automatic switching in one of them; manual switches still work.” and native rotation holds until Swap stops switching (0.5.3). An account that needs a new sign-in is never chosen; the next eligible one is (0.5.3).
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
2. Read account states → Gold selection names the next managed request; the blue “In Claude Code” / “In Codex CLI” badge names the separate local identity.
3. Open a dialog, change navigation, or use a narrow window → Shared roles and visible keyboard focus persist in the active theme.
4. Open About → Appearance offers System (default; follows the operating system's light/dark setting live), Dark and Light; the choice applies at once and is saved on this machine. If it cannot be saved, it applies until restart and About says so.
5. Read About → Version and license: the app version, “Open source under the GNU AGPL-3.0; a commercial license is available at passioncode.ai/business.” with the address https://passioncode.ai/business/, the LICENSE and third-party notice addresses, and “Part of the PassionCode.ai toolkit” with https://passioncode.ai/switchboard/. In the native app the addresses are selectable text (no in-app browser opener exists); in the browser demo they are links opening a new tab.
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
**Alt paths:** Sessions launched from Switchboard get the tools without this step; a launch without a CLI says so in its notice. The app was opened straight from its download (macOS App Translocation) → the panel reads “macOS is running Switchboard from a temporary copy of the download. Move Fabric Switchboard to Applications and open it from there before connecting agents; …”, the link button is not offered and no agent config points into the copy (0.5.3).
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
**Alt paths:** Deny in the dialog → the account stays where it was and the app does not ask again until restart. A development build never reads the user's accounts. Codex keeps its sign-in in the Keychain (`keyring`/`auto`) → it is read without a dialog; an item that does not trust Switchboard reads “Keychain is locked or does not let Switchboard read this sign-in without asking. Unlock it and retry, or add the account with official sign-in.” instead of asking every minute (0.5.3).
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
**Alt paths:** Claude Code's MCP and plugin sign-ins stay signed in across a switch (0.5.1). Claude Code is updating its account → Switchboard waits up to 9 s and takes over a lock left behind by a crashed process (0.5.1). Refusals, each with nothing changed (0.5.1): the signed-in account is not saved in Switchboard (“Claude Code is signed in to an account Switchboard has not saved; switching would sign it out. Add it first (In use now → Add to Switchboard).”); the sign-in belongs to another saved account than its settings name — once the provider confirms whose it is, the live token is filed under that account and the switch goes on (only a stored copy saying so is not enough: “Switchboard could not confirm which account Claude Code is signed in to…”); the sign-in belongs to an account Switchboard does not hold (“The Claude Code sign-in does not match the account named in its settings. Sign in again in Claude Code (claude /login), then retry.”); a sign-in nothing attributes yet (“Switchboard could not confirm which account Claude Code is signed in to. Check the connection, or use Claude Code once, then retry.”); a renewed sign-in not yet stored (“Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.”); a session Switchboard launched (“This session runs in a Switchboard home. Switch the ordinary Claude Code from the app or a normal terminal.”). Switching to the account already in use changes nothing. A `Claude Code-credentials` item that a 0.4 build created while Claude Code was signed out trusts only that build; sign out and in once in Claude Code to recreate it ([operations](../OPERATIONS.md)). Switchboard's own account storage is SCN-027.
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
**Alt paths:** The provider rejects an account's sign-in → its row shows Sign in as the primary action and a red “Sign in again” badge; signing in updates the same row. The account signed in to the ordinary Claude Code is renewed by Claude Code; only when Claude Code has left it expired for more than five minutes does Switchboard renew it under Claude Code's locks, keeping Claude Code signed in (0.5.1). While Claude Swap runs, the accounts it manages are renewed by it and Switchboard follows (0.5.1). Claude refuses the renewal client itself (`invalid_client`) → the Automatic switching bar reads “Claude is refusing sign-in renewals for every account right now. …”, no row turns to Sign in again, and renewals resume on their own within the hour (0.5.2).
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
2. After a reinstall (or `switchboard uninstall` and a new install) → at its first start Switchboard restores the newest backup this Mac can open by itself: accounts, projects, project rules, managed selections and the open-at-login and auto-update settings come back, and the notice says “Switchboard found your backup from {date} and restored it: …”. An empty account list with a backup beside it also offers Restore from backup; About → Backups → Restore restores any listed backup.
**Alt paths:** Back up now writes one at once. A backup from another Mac or after the Keychain was erased reads “This backup was made with another key and cannot be opened on this machine.” A restored account whose sign-in has since rotated shows Sign in again.
**Expected result:** Accounts, projects, rules, selections and settings are back; nothing newer and no choice made on this install is overwritten; policies return switched off. A person who removed their accounts keeps them removed: only a new data folder restores on its own.
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
1. A Claude Code session hits a usage or spend limit → the monitor attempts a switch on its next scan (timing and running-session credential adoption remain SB-06 acceptance), changing Claude Code to the free account with the lowest usage, even inside the cooldown; the bar reads “Switched after the account in use hit a provider limit.”
2. The limited row's status mark turns red and its second line reads “Back in {countdown} · {date}” when Claude Code reported the reset or “Retry in {countdown} · {date} · estimated” when the hold is estimated; the quota disclosure names it “Limit reached” with “Limit resets {date}” or “Retry hold until {date}”. The row is not chosen again before the hold ends (SB-41). The hold survives a restart of Switchboard.
**Alt paths:** No other free account → “The account in use hit a provider limit, and no other account is free. Holding it.” Managed sessions are detected from the proxy's own 429 responses. A session opened before the switch keeps charging the account it runs on; a session opened after it charges the new account, even when both limits reset at the same time (SB-41).
**Expected result:** the saved/native selection changes to another account. The next accepted managed request uses that selection; an already stopped native session may still need operator continuation or restart. Automatic wake/resume is deferred to SB-25. The retry hold expiring does not verify restored quota.
**UI elements:** status mark, quota cell second line, quota disclosure, Automatic switching decision text.
**States covered:** limited, switched on limit, all limited.
**Errors & recovery:** a failed activation reads as before (`activation_failed`).
**Status:** draft
**Meaning:** operator request 2026-10-02 ([PLAN-0.5](../PLAN-0.5.md) D-6); Claude Swap does not do this.
**Coverage:** `limits::tests` (including `an_equal_reset_on_a_new_session_is_the_new_accounts_limit`, `evidence_survives_a_restart_and_a_hold_is_not_a_reset`), `a_limit_error_in_claude_code_switches_with_quota_to_spare`, rotation test, `limitLabel` in `scripts/test-ui-logic.mjs`; marker shape measured on this machine's own transcripts (field names only).
**Product:** unobserved
**Traces:** PLAN-0.5 REQ-16, REQ-17


## SCN-032 — Find remaining quota and compare account waits
**Persona:** P-01
**Goal:** Find saved accounts with quota and understand the wait in hours and days.
**Preconditions:** Account metadata loaded; provider observations may be missing, partial, failed or old.
**Entry point:** SCR-01 Accounts
**Steps:**
1. Within each provider/pool group, view fresh accounts with remaining quota first, most headroom first.
2. Then compare limited/exhausted accounts by earliest known end of wait. If several quota windows are exhausted, use the latest reset; a missing blocking reset reads Reset time unavailable.
3. Read each row in two lines (compact list, 2026-10-05): the meter with the share used and how long ago it was checked, then the time that matters — “Resets in {countdown} · {date}”, “Back in …” or “Retry in … · estimated” — beside a status mark on the provider icon (green available, amber running low at 80% or stale, red limit/failed/sign-in, none for disabled or no quota check). The expanded windows add each window's date and days/hours/minutes.
4. Pause countdown → text stays frozen across metadata renders; resume → wall-clock remaining time returns.
**Alt paths:** Unknown/stale/failed data follow known waits, then sign-in-required and disabled rows. Provider/pool boundaries remain. A runtime retry hold is labelled separately from quota resets. Passed reset reads Due · awaiting check; the pre-reset measurement remains history until a fresh provider observation. Authoritative JSON restores freshness after partial headers; retained windows never acquire a newer evidence timestamp.
**Expected result:** Display order reflects available evidence without selecting accounts, changing policies, refreshing credentials or replaying requests. Date stays visible alongside duration; timer updates do not move focus or announce every minute.
**UI elements:** account groups, ordering note, quota button, date/time, countdown, Pause countdown, Resume countdown.
**States covered:** available, blocked, unknown reset, stale, failed, sign-in-required, disabled, reset-passed, paused, hidden window.
**Errors & recovery:** Failed checks keep prior values as history; Check usage or scheduled polling can update them. Missing evidence never means zero usage or confirmed capacity.
**Status:** draft
**Meaning:** implementation follows the operator's request on 2026-10-04; no provider acceptance inferred.
**Coverage:** [UI logic tests](../../scripts/test-ui-logic.mjs), [renderer](../../src/main.ts), [synthetic demo](../../src/demo.ts), [verification](../../docs/evidence/quota-order-2026-10-04.md). Compact two-line rows observed with `?demo=1&quota-review=1` on 2026-10-05: row height 52–55 CSS px at 1280 px (was four to six lines), 93–95 px at 740 px where the quota moves under the name, no horizontal overflow, light and dark; Pause countdown keeps the compact and long countdowns frozen; the limited row's disclosure 168 px at 1280 px.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-033 — Keep Switchboard working with its window closed
**Persona:** P-01
**Goal:** Rotation, renewal, quota checks and backups keep running without the window open, and Switchboard ends only when the person quits it.
**Preconditions:** the installed app (a development build never registers a login item).
**Entry point:** the window's close button; the menu-bar icon (Windows: notification area); About.
**Steps:**
1. Close the window → it disappears (macOS: so does the Dock icon); the menu-bar icon stays; the owner keeps running.
2. Menu-bar icon → *Open Switchboard* → the window returns in front. Opening the app again (Finder, Spotlight; Windows: launching it, or a left click on the icon) does the same.
3. Log out and in → Switchboard starts in the background with no window and no focus taken.
4. About → *Running in the background* → uncheck *Open at login, in the background* → “Switchboard will no longer open at login.”; the choice survives restarts.
5. Menu-bar icon → *Quit Switchboard* (or the app menu's Quit, Cmd-Q) → the owner drains and the app ends (LC-01).
**Alt paths:** A second launch that is itself a background start (the login item, the lifecycle broker) leaves the window hidden. A login item the system refuses reads “Could not add Switchboard to the login items. Check the system's login item settings.” With the lifecycle broker enrolling the app `always_on`, a Quit is undone by the broker; the operator sets it `on_demand`.
**Expected result:** background work never depends on the window; only an explicit Quit ends it.
**UI elements:** tray menu (Open Switchboard, Quit Switchboard), About residency panel and checkbox.
**States covered:** window shown, window hidden, background start, login item on/off/unavailable, quit.
**Errors & recovery:** a failed login-item change keeps the previous state and shows the error; quitting always drains.
**Status:** draft
**Meaning:** operator decision 2026-10-05 («свитчер должен сам запускаться и не выключаться пока его не закрою через меню выйти»).
**Coverage:** `residency::tests` (default on, saved choice, development build never registers), `the_login_item_is_removed_and_a_failure_is_reported_not_fatal`, `scripts/test_smoke_native.py` (tray required), browser demo of the About panel; Windows build checked with `cargo xwin check -p fabric-switchboard`. Live close/reopen/login on the operator's Mac is acceptance (SB-15).
**Product:** unobserved
**Traces:** SB-28, LC-09

## SCN-034 — Keep a project on its own accounts
**Persona:** P-01
**Goal:** Attach accounts to a project — one or more related repositories — so that only sessions from those folders use them, and agents in other projects never switch to them.
**Preconditions:** saved accounts; the project's folders exist on this computer.
**Entry point:** Projects → + New project.
**Steps:**
1. + New project → name, folders (one absolute path per line), check the accounts → Create project → “Project “{name}” created. Launch its accounts from its folders.” The accounts move into the project's pool; the Accounts page groups them under “Project · {name}”, and their button is Select, not Switch.
2. Launch one of them (isolated or managed) from any of its folders or subfolders → the session starts; managed rotation moves only among the project's accounts.
3. From a folder outside the project → refused: “This account belongs to a project. Launch it from one of the project's folders.”
4. Inside the project's folders, an account of another pool for a provider the project has → refused: “This folder belongs to a project. Launch one of the project's accounts.”
5. Edit → change folders or accounts; an account left out goes back to the default pool. Delete → the accounts stay in their pool, no longer reserved.
**Alt paths:** The account the ordinary Claude Code or Codex is signed in to cannot join (“…Switch the CLI to another account first…”). A folder already in another project is refused. Switching the ordinary Claude Code to a project account, a native rule or native rotation on a project pool are refused: the ordinary Claude Code serves every folder. A provider the project has no account for keeps using the other accounts in its folders.
**Expected result:** a project's accounts serve only its folders; other projects' sessions, rotation and switches never reach them.
**UI elements:** Projects section, project dialog, project cards, “Project · {name}” pool heading.
**States covered:** none, created, edited, deleted, refused (CLI account, overlapping folder, outside folder, foreign account inside).
**Errors & recovery:** every refusal names what to change; nothing moves on a refusal.
**Status:** draft
**Meaning:** operator request 2026-10-05.
**Coverage:** core `a_project_takes_its_accounts_into_its_pool_and_gives_back_the_ones_left_out`, `projects_never_share_a_folder_or_an_account_identity`, `a_project_pool_never_drives_the_ordinary_claude_code`; runtime `a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts`, `a_project_account_never_becomes_the_ordinary_claude_code`, `the_account_the_cli_uses_cannot_join_a_project`; CLI `project_commands_work_offline_from_the_cli`; browser demo 2026-10-05 (create, CLI-account refusal, “Project · Client Alpha” heading, Select on a project account).
**Product:** unobserved
**Traces:** CONTRACTS → Projects (0.6)

## SCN-035 — Learn the product in five steps on first start
**Persona:** P-01
**Goal:** Understand on first start what Switchboard does and where to press.
**Preconditions:** first start in this app's window storage (the tour shows until finished or skipped).
**Entry point:** automatic on first start; About → Tour → Show the tour again.
**Steps:**
1. “Welcome to Switchboard” — what it is for.
2. “Add your accounts” — + Add account is outlined.
3. “Switch, by hand or automatically” — the automatic switching bar is outlined.
4. “Give a project its own accounts” — Projects opens, + New project is outlined.
5. “Agents, and it keeps running” — Agents opens; the menu-bar icon is named. Start using Switchboard ends it.
**Alt paths:** Skip tour or Escape ends it at any step; Back returns. A browser that refuses storage shows the tour again next start rather than never.
**Expected result:** a person knows the five things the product does and the control for each, in under a minute; the tour never blocks the screen it explains.
**UI elements:** tour card (non-modal), outlined target, step dots.
**States covered:** first start, each step, skipped, finished, shown again.
**Errors & recovery:** none; storage failure only means the tour shows again.
**Status:** draft
**Meaning:** operator request 2026-10-05.
**Coverage:** browser demo 2026-10-05: five steps, targets `+ Add account`, `.rotation-bar`, `+ New project`; finish stores `switchboard.tour`; About shows it again; Escape closes.
**Product:** unobserved
**Traces:** SCN-001

## SCN-036 — Run another coding agent on Switchboard
**Persona:** P-01
**Goal:** Use Hermes, Kilo Code, Cline, Goose, OpenCode or another popular agent with Switchboard's tools and accounts, not only Claude Code and Codex.
**Preconditions:** the agent is installed; for model requests through Switchboard, an API-key account selected in the chosen pool.
**Entry point:** Agents → Other agents → Set up; CLI `switchboard agents`.
**Steps:**
1. Agents → Other agents lists 30 agents in three groups (Launch from Switchboard, Through Switchboard, Tools only) with their OpenRouter rank.
2. Set up on an agent → choose the pool → the dialog shows the command that registers the tools, the endpoints, the key command (`switchboard agents key`) and, for launchable agents, Launch in folder.
3. Launch → the agent opens in Terminal in the folder, on the pool's API-key account; Switchboard switches accounts for it.
**Alt paths:** The pool's selected account is a subscription sign-in → “This pool's selected account is a subscription sign-in, which its provider allows only in Claude Code or Codex. Select an API-key account in this pool for other agents.” A project's account outside its folders, or a foreign account inside them, is refused as for any launch. Tools-only agents have no endpoint step.
**Expected result:** every listed agent gets Switchboard's tools; the ones that accept an endpoint use Switchboard's accounts within the providers' terms.
**UI elements:** Other agents section, Set up dialog, copyable commands.
**States covered:** each level, pool chosen, launch refused (subscription, project), MCP-only.
**Errors & recovery:** refusals name the fix; nothing is written to the agent's config by Switchboard.
**Status:** draft
**Meaning:** operator request 2026-10-05.
**Coverage:** proxy `agents_reach_api_key_accounts_and_never_a_subscription_sign_in`, `chat_completions_reach_openai_with_an_api_key_only`; runtime `the_catalog_is_well_formed_and_every_launch_profile_is_complete`, `connect_names_the_key_command_and_never_a_key`, `a_third_party_agent_never_starts_on_a_subscription_account`; browser demo 2026-10-05 (30 agents, Hermes dialog). Live runs with real agents need an API-key account (operator).
**Product:** unobserved
**Traces:** CONTRACTS → Other agents (0.6)

## SCN-037 — Get new versions without doing anything
**Persona:** P-01
**Goal:** Every installed Switchboard moves to each new release on its own, with nothing to download, approve or reinstall, and nothing lost on the way.
**Preconditions:** the installed app from a release (a development build, the smoke check and a copy macOS runs from a translocated path never check).
**Entry point:** none — it runs on its own; About → *Running in the background* shows it; the menu-bar icon offers the restart.
**Steps:**
1. Install and open Switchboard → about 90 s after start, and every six hours after, it reads `latest.json` of the newest GitHub release.
2. A newer version is announced → it downloads in the background and is checked against the update key compiled into the app and the version its signature names; a package that fails is discarded (“The downloaded update did not pass its signature check and was discarded. Switchboard tries again within the hour.”).
3. macOS: the verified version replaces the app bundle at once; Windows: the installer waits for the app to quit. About reads “Version {version} is ready. It starts the next time Switchboard opens, or restart now.”; the menu-bar menu gains *Restart to update to {version}*.
4. Do nothing → the next start (a reboot, a login, a Quit and reopen) runs the new version. Or press *Restart to update* → the owner drains (LC-01) and the new version starts — in the background if the window was hidden.
5. About → uncheck *Install updates automatically* → “Automatic updates are off. Switchboard will not check for new versions.”; the choice is saved in `<data>/auto-update` and travels with backups.
**Alt paths:** An app the person cannot replace without an administrator password (macOS) is not installed in the background; About says “Restarting asks for an administrator password to replace the app.” and *Restart to update* asks for it; a refused password reads “The update needs an administrator password to replace Switchboard in this folder. It was not installed.” No network, a GitHub outage or a failed download reads “Could not check for updates…” / “Could not download the update…” and retries within the hour. A quit by logout or shutdown (a signal) starts no installer on Windows; the next ordinary quit does. A copy run from a translocated path reads “Move Fabric Switchboard to the Applications folder to receive updates.”
**Expected result:** the person runs the newest release without acting; accounts, settings, connections and backups are untouched by an update.
**UI elements:** About residency panel: checkbox “Install updates automatically”, status line, button “Restart to update”; tray item “Restart to update to {version}”.
**States covered:** unavailable (development, translocated), off, checking, current, downloading, ready (installed / needs a password), failed (check, download, signature, install), restarting.
**Errors & recovery:** every failure keeps the running version and retries within the hour; nothing is half-installed — macOS swaps the bundle outside the quit path, Windows installs in NSIS update mode, which never uninstalls and never deletes the app data.
**Status:** draft
**Meaning:** operator request 2026-10-05 («автоматический механизм обновления по дефолту был включен для всех версий… ничего не делая, подтягивать новые версии»).
**Coverage:** `updates::tests` (on by default, saved switch, unavailable copies, relaunch keeps `--background`, bundle replaceable without a password), `residency::tests::the_tray_names_the_version_it_restarts_into`, `scripts/test-ui-logic.mjs` (status line and restart offer), `scripts/test_updater_artifacts.py` (latest.json names only files of the release with their own signatures; the macOS archive layout), the macOS package rebuilt from the installed notarized 0.6.0 app and verified by codesign, stapler and Gatekeeper on the unpacked copy (2026-10-05). The first live update is the release after the one that ships this: acceptance (SB-15).
**Product:** unobserved
**Traces:** SB-55, LC-01, LC-09, SCN-030, SCN-033

## SCN-038 — Continue a workflow on another account when a limit runs out
**Persona:** P-01
**Goal:** An agent's long task (an Observatory workflow) stops because its account hit its limit; another account picks it up from the last checkpoint — of the same provider, or of the other one (Claude Code ↔ Codex, 0.6.7, SB-70) — without redoing finished steps and without the person copying context.
**Preconditions:** Project Observatory installed; the workflow open, its executor silent for two minutes; every key it declares in the Observatory vault; another enabled Claude Code or Codex account; the desktop app or `switchboard serve` running.
**Entry point:** CLI `switchboard continue <wf> --account <id> --dir <checkout>` (a desktop entry and an MCP tool come later).
**Steps:**
1. `project-observatory full workflow list` names the stalled workflow → run `switchboard continue` with it, the account and the checkout.
2. Switchboard reads the workflow and offers it to the account (reason `limit`) → prints the handoff id and until when it is offered.
3. Terminal opens the account's session in the checkout → its first message tells it to accept the handoff, continue from the checkpoint in the acceptance answer and obey its constraints → it does, and writes checkpoints as it goes. Across providers (a Claude Code workflow on a Codex account, or back) the same steps run; the first message also says the work comes from another agent and that the checkpoint is the whole context (`a_workflow_moves_to_another_provider_with_its_context`, `the_prompt_of_a_cross_provider_handoff_names_the_previous_agent_safely`).
**Alt paths:** the executor wrote less than two minutes ago → Observatory's refusal (“…without its lease token a handoff waits until it has been silent for 120 s”), nothing launched; a closed workflow, no checkpoint, a waiting handoff, a key not in the vault (named), a workflow naming no executor, the same account, a folder outside the checkouts → refused before any offer; the session fails to start after the offer → the handoff id and its deadline, the offer lapses on its own.
**Expected result:** the work continues on the new account from where it stopped; the old session, if it writes again, is refused by Observatory (`LeaseLost`) and its step is kept for review.
**UI elements:** CLI output (`workflow`, `handoff`, `offeredUntil`); the launched Terminal session.
**States covered:** ready, refused before the offer (each reason), refused by the engine, offered and launched, offered and launch failed.
**Errors & recovery:** every refusal names the fix; nothing is retried automatically; a lapsed offer can be made again.
**Status:** draft
**Meaning:** SB-52 / N-018 (PB-137/M9), operator yes to the Observatory MCP entry 2026-10-05.
**Coverage:** `continuation::tests` (12, incl. a fake engine), `launch::tests::a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt`, `preflight_refuses_what_a_launch_would_refuse_without_writing`; `real_engine_contract` against engine 0.16.0 in a scratch workspace. Live acceptance with two real accounts and a real limit needs the operator.
**Product:** unobserved
**Traces:** SB-52, CONTRACTS → Workflow continuation

## SCN-039 — Choose which agents take over a task, and in what order

**Persona:** P-01
**Goal:** The operator decides, on their own machine, which agents continue a task when the accounts of its agent run out — for the whole machine, for one project, or for one task — so the switch, when it comes, follows their order and never hands a subscription to an agent that may not use it.
**Preconditions:** Switchboard installed; for a pinned account, the account saved; for a paid key, its name in the Project Observatory vault.
**Entry point:** CLI `switchboard chain set|list|clear`; MCP `switchboard_chain_set` / `switchboard_chain_get` (an agent acts only when the operator asks). An app screen comes later (SB-71 remainder).
**Steps:**
1. `switchboard chain set --preset subscriptions-first` → the machine's chain reads claude-code → codex → kimi-code → hermes (`switchboard chain list`).
2. For one project or one task, `switchboard chain set --scope project:<pool>|workflow:<wf_id> <agent>[@account|#key] …` → `chain list --workflow <wf_id>` shows that chain under “applies”.
3. `switchboard chain clear --scope …` → the next wider chain applies again; with none set, nothing applies.
**Alt paths:** an agent that loads no MCP server or takes no prompt without a person (Aider) → “aider cannot take over a workflow …”, nothing saved; another agent pinned to a Claude or ChatGPT sign-in → “Another agent runs on an API-key account or a paid key, never on a subscription sign-in.”; a key value instead of a vault name → refused; a removed account or project → the executors and chains that named it are dropped.
**Expected result:** the order is stored and shown; when an executor hits its limit, the automatic fallback walks it (SCN-040, SB-73 — Claude Code and Codex so far).
**UI elements:** CLI output (`machine  claude-code → codex → …`, `applies: …`, presets); MCP results `{chains, effective, presets}`, `{chain}`.
**States covered:** none set, machine only, project and task chains over it, cleared, pruned after a removal, refused (agent, pin, key, scope).
**Errors & recovery:** every refusal names the fix and saves nothing; a wrong chain is replaced by setting it again or cleared.
**Status:** draft
**Meaning:** XA-01 decisions D-3 (chains are the operator's, per machine/project/task) and D-4 (MCP for everything), operator 2026-10-06.
**Coverage:** `chains::tests` (5, switchboard-core), `owner_tests::fallback_chains_take_a_preset_refuse_agents_that_cannot_continue_and_resolve`, `fallback_chains_are_set_listed_and_cleared_from_the_cli`, `handshake_lists_tools_and_rejects_malformed_messages` (MCP chain tools).
**Product:** unobserved
**Traces:** SB-71, CONTRACTS → Fallback chains, [XA-01](../packets/cross-agent-continuation.md)

## SCN-040 — Work moves on by itself when an agent's accounts run out

**Persona:** P-01
**Goal:** A long task keeps going when its agent hits its limit and no other account of that provider is ready — the next agent of the operator's chain takes it over from the last checkpoint without the person doing anything.
**Preconditions:** a fallback chain set (SCN-039); the task recorded as an open Project Observatory workflow with its checkout on this machine; the desktop app or `switchboard serve` running; every key the workflow declares in the Observatory vault.
**Entry point:** none — the background monitor (SB-73). The person sees a Terminal window open with the new session.
**Steps:**
1. The executor hits its limit (Claude Code writes a limit marker, or a managed request is refused) → within a minute Switchboard reads the open workflows and finds it.
2. Switchboard walks the chain: the first agent with a usable account (not the executor's, not limited, not needing a new sign-in) gets the workflow, offered with reason `limit` once the executor has been silent for two minutes.
3. Terminal opens that agent's session in the workflow's checkout → it accepts the handoff and continues from the checkpoint (SCN-038).
**Alt paths:** no chain applies → nothing happens; a step's account is refused before the offer → the next step is tried; the launch fails after the offer → nothing else is tried and the offer lapses; the workflow has no checkout here → skipped (`no_checkout` in the log); Kimi Code or Hermes in the chain → skipped until SB-72 and SB-73 phase 2; the same workflow is not tried again for 15 minutes.
**Expected result:** the task continues on another agent from where it stopped; the old session is left alone and, if it writes again, Observatory refuses it (`LeaseLost`).
**UI elements:** the launched Terminal session; log event `fallback` and the store event `fallback` (Activity).
**States covered:** nothing to do (no chain, nothing limited), offered and launched, moved down the chain, launch failed after the offer, skipped (no checkout, agent not automatic yet, no usable account).
**Errors & recovery:** every outcome is logged with a fixed code; a lapsed offer is retried after the window or by hand with `switchboard continue`.
**Status:** draft
**Meaning:** XA-01 decision D-1 (automatic), operator 2026-10-06.
**Coverage:** `fallback::tests` (resolve, executor_limited, cadence, `a_limited_executor_hands_its_workflow_to_the_next_agent_of_its_chain` with a stand-in engine and a real limit marker, `a_refusal_moves_down_the_chain_and_a_failed_launch_after_the_offer_stops`).
**Product:** unobserved
**Traces:** SB-73, CONTRACTS → Automatic fallback, [XA-01](../packets/cross-agent-continuation.md)
