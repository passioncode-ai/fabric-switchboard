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
| SCN-005 | Launch an account in a folder, isolated if needed | validated |
| SCN-006 | Launch managed (the box cleared) | validated |
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
| SCN-022 | Configure and stop automatic switching | validated |
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
| SCN-039 | Choose which agents take over a task, and in what order | draft |
| SCN-040 | Work moves on by itself when an agent's accounts run out | draft |
| SCN-041 | Use Switchboard in Russian | draft |
| SCN-042 | Delete a project or remove a rule only after confirming | draft |
| SCN-043 | Recover when a read or a refresh after an action fails | draft |
| SCN-044 | Turn usage analytics off or on | draft |
| SCN-045 | Save one OpenRouter key for agents | draft |
| SCN-046 | Launch an agent on the OpenRouter key | draft |
| SCN-047 | Keep several Kimi Code subscription accounts | draft |
| SCN-048 | See and change Hermes's model and provider | draft |
| SCN-049 | See the provider terms notice | draft |
| SCN-050 | A coding agent sets Switchboard up | draft |
| SCN-051 | See which sessions Switchboard launched are running | draft |
| SCN-052 | An agent keeps working when its account hits a limit | draft |
| SCN-053 | A stopped session gets a capacity notice once | draft |
| SCN-054 | Take a workflow hand-over in a console | draft |
| SCN-055 | Git folders and projects register by themselves | draft |
| SCN-056 | Work in a project's folder on the project's accounts | draft |
| SCN-057 | See which coding agents are installed and choose which take over work | draft |
| SCN-058 | Choose the default agent | draft |
| SCN-059 | Pick an OpenRouter model, starting from the recommended ones | draft |
| SCN-060 | A workflow continues in OpenCode on the OpenRouter key | draft |
| SCN-061 | A deleted folder leaves its project by itself | draft |
| SCN-062 | Move an account to another pool from its card | draft |
| SCN-063 | See a project's repositories with their worktrees under them | draft |
| SCN-064 | Find repositories on disk and put each in its project | draft |
| SCN-065 | Give a project its own API key for agents | draft |
| SCN-066 | Codex in a project's folders uses the project's Codex accounts | draft |
| SCN-067 | See which accounts and agents work where, and where tokens go | draft |
| SCN-068 | Groups of projects: one repository in several groups, and the projects a group holds on hover | draft |
| SCN-069 | See which accounts belong to which organization | draft |
| SCN-070 | Keep a group's work on its organization's accounts | draft |
| SCN-071 | Keep organizations current from PassionCode.ai | draft |
| SCN-072 | Keep an account signed in on another machine out of this one | draft |
| SCN-073 | Answer the usage question before anything is sent | draft |
| SCN-074 | Plain `claude` and `codex` in an organization's folders run in its context | draft |
| SCN-075 | Turn the terminal hook off or on | draft |
| SCN-076 | Set up an organization's work on a new computer | draft |
| SCN-077 | A new seat of an organization lands in its group | draft |
| SCN-078 | A seat the organization removed reads as ended | draft |
| SCN-079 | An organization the person left on PassionCode.ai stays here, marked | draft |
| SCN-080 | See and set the fallback chain in the window | draft |
| SCN-081 | Give a group its own fallback chain | draft |
| SCN-082 | See whether a chain can take work now | draft |
| SCN-083 | Cap what paid agents may spend a day | draft |
| SCN-084 | A ceiling is reached: double it or keep it | draft |
## SCN-001 — First run
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Open the app → An empty account list explains how to add an account, and is the page's one primary action: the header carries no second *+ Add account* (0.6.82, UX-01). When a CLI is signed in to an account Switchboard has not saved, the text points at *Add to Switchboard* in In use now; when neither CLI is signed in, it says so and points at *+ Add account* (UX-18).
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
1. Choose provider and kind, enter label, choose the group (Default first, then each group by name, then any other group as “Group {id} · no folders” (D-5); *New id…* opens a *Group id* field for an id of lowercase letters, numbers, hyphens or underscores, checked in the field, also in WebKit) and enter the secret; submit → One account appears; secret field is cleared (0.6.82, UX-17, VIS-03).
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
**Alt paths:** Cancel in the banner while Terminal still waits → Switchboard ends that sign-in in Terminal (SIGHUP, then SIGTERM, to its own script only — identified by its pid marker, age and this sign-in's `launch.command`) and cleans the staged sign-in: “Sign-in cancelled. No account was added.” (SB-83); a session that will not end → “Switchboard could not end the sign-in in Terminal. Close its Terminal window, then cancel again.”; a refused Cancel is shown under the banner and the banner keeps watching Terminal — it never turns into a Finish error with *Retry*; after Terminal closed → the staged sign-in is cleaned. Closing Terminal without signing in → the banner reads “Sign-in ended without an account” with Try again and Dismiss. Sign in on a row reuses its label and pool, and Try again in the banner repeats the sign-in it replaces with the same label and pool (0.6.6, SB-62). A sign-in of an identity already saved — from a row, from + Add account or from Try again — updates every saved copy of it where it is, keeps their labels and pools, and reads “{label} is signed in again in {provider} · {pool}.”; it never renames an account to its email and never copies an account (a project's) into another pool (`a_sign_in_without_a_label_updates_the_saved_account_where_it_is`, `a_row_sign_in_updates_its_row_and_every_other_copy_of_the_identity`). CLI: `login begin` (label optional), `login status`, `login finish`. A second sign-in while one is open reads “A sign-in is already in progress. Finish it in Terminal or cancel it first.” The account was saved but its temporary folder could not be removed yet → the banner closes with “{label} added to {provider} · {pool}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.” Switchboard quit or restarted while a sign-in was open → since 0.6.18 it takes the sign-in back from its journal (`.login.json`: provider, label, pool) when Terminal still runs it or it completed meanwhile, the banner returns when the window opens, and the account is added as usual (SB-65); only a sign-in it cannot take back (no journal, an earlier version's) reads ended, and Cancel closes it with “Sign-in closed. Switchboard had already ended it; close its Terminal window if it is still open.” (0.5.1, `loginOutcome`). A sign-in reservation whose Terminal never ran expires after ten minutes. The account is saved but its temporary sign-in folder cannot be removed yet → the banner closes with “{label} added to {provider} · {pool}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.”; finishing again returns the same account (SB-42).
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
**Expected result:** Route shows selected for the next request in its provider and pool.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Unknown, disabled, expired or mismatched target fails with recovery instruction.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-005 — Launch an account in a folder, isolated if needed
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Row menu → *Launch…* → *Choose folder…* opens the system folder picker (a folder is never typed; it opens at the last folder chosen) → tick *Isolated session* → *Launch* → Terminal starts the provider in that folder with the account's private home; later selections do not move it (operator decision 2026-10-09, SB-86).
2. The account is not the pool's selected one, or the local proxy is not running → *Isolated session* is ticked and locked, with the reason in one line.
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
**Expected result:** Terminal starts the provider with its private home; selection affects new launches.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** Missing CLI or launch failure is shown; no change to existing clients.
**Status:** validated
**Meaning:** scenario design validated against the authorized brief; implementation and user-outcome evidence are separate.
**Coverage:** [verification](../evidence/verification.md); implementation/fixture evidence only; live-provider outcome NOT_RUN.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-006 — Launch managed (the box cleared)
**Persona:** P-01
**Goal:** Deliberately control which account a coding session uses.
**Preconditions:** macOS app with user-owned authorized accounts; tests use synthetic fixtures.
**Entry point:** SCR-01 Accounts or SCR-02 Activity
**Steps:**
1. Select an account, row menu → *Launch…*, pick a folder, leave *Isolated session* cleared → Terminal starts against the local proxy; the dialog explains the next-request effect.
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
1. Choose Remove, inspect confirmation, confirm → the account and its vault entry are removed. A selected account offers *Disable and remove* instead: the dialog says that disabling clears its selection and that managed requests in its group fail until another account is selected (0.6.82, UX-29).
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
**Expected result:** The account and its vault entry are removed; a selected one is disabled first, which clears its selection.
**UI elements:** navigation, account list, labelled actions, dialog, status/error message.
**States covered:** loading, empty, error, success
**Errors & recovery:** A refusal stays in the dialog; cancellation changes nothing.
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
1. Open Activity → Bounded journal shows account IDs and operation outcomes without tokens or prompts. Every word the core records has a name in both languages (a test reads `event_valid`); a word this window does not know yet reads “Other event ({code})”, translated, never bare (0.6.82, UX-26).
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.”
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
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.” Opening Switchboard again while it runs focuses the open window (0.5.3). Started in the background by the local lifecycle broker (`--background`), it shows no window and takes no focus; opening it again shows the window, with fresh metadata (SB-30; background smoke measured, the live reveal path is operator acceptance). The store is held by `switchboard serve`, a running command or a window opened with `open -n` → the window opens and reads “Switchboard's account store is in use by another Switchboard (a second window, 'switchboard serve' or a command still running). Close it, then retry.”; Retry starts it once the store is free — the app no longer quits unexpectedly (0.5.3).
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
4. Open a dialog from a ⋯ menu (Launch, Move, Rename, Remove, Organization) and close it → focus returns to that row's ⋯ (0.6.82, UX-05). A dialog with a text field opens on its first field (*Group name*), not on *Cancel* (VIS-15). A destructive item in a ⋯ menu sits under a separator (VIS-27).
5. While an action runs (a Switch waiting for Claude Code's lock, a restore), navigation, Refresh and view controls (sorting, filters, quota disclosures, the tour) stay usable; only controls that change something wait (UX-24). A Refresh pressed meanwhile never puts back a list from before the action.
6. An action's result appears as a notice fixed at the bottom of the window, in sight wherever the control was (UX-07); it is still announced to screen readers.
**Alt paths:** Cancel a local edit → return without mutation. Cancelling an official sign-in ends its Terminal session the way closing its window would, then cleans the staged profile (SB-83); a Terminal that has not started the sign-in yet reads “The sign-in is still opening in Terminal. Cancel again in a moment.” A background refresh that started before a Select, Edit, Remove or other change never overwrites the result of that change (0.4, B-05).
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

## SCN-022 — Configure and stop automatic switching
**Persona:** P-01
**Goal:** Persist an explicit quota policy for one provider, pool and target.
**Preconditions:** Authorized local accounts; synthetic fixtures for UI checks.
**Entry point:** SCR-01 Accounts
**Steps:**
1. With two or more switchable Claude accounts and no native policy on, the Automatic switching bar offers Turn on for Claude Code → one click enables the `claude_cli` policy for the pool holding most of them with the defaults below, and the bar reads “Automatic switching is on” with the latest decision (0.5). Stop turns it off in one click.
2. For other groups, the ⚙ menu → *New automatic switching…* or *Edit {provider} in {group} ({target})…* → defaults are off, threshold 90%, minimum improvement 10 points, cooldown 1800 seconds, maximum age 300 seconds. Choose provider, group (by name, as in SCN-002) and managed/native Claude target; enable and *Save* → the bar names each setting as “{provider} · {group} · {target}”, a group by its name and the default pool as “Default group”, never a bare pool id (0.6.82, D-1, CPY-07).
**Alt paths:** Edit a saved policy → its boundary stays fixed and values reload. Stop (*Stop in {group}* when several groups switch) → the saved policy is disabled in one action: “Automatic switching stopped for {provider} in {group}.” Codex cannot select the native Claude target. Claude Swap switches Claude Code itself (`cswap auto`, or its menu bar with automatic switching on) → the bar reads “Claude Swap is switching Claude Code automatically, so Switchboard does not. Turn off automatic switching in one of them; manual switches still work.” and automatic switching of Claude Code holds until Swap stops switching (0.5.3). An account that needs a new sign-in is never chosen; the next eligible one is (0.5.3).
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
2. Read account states → In every pool and project the account its sessions use carries the gold bar, and the pool heading says so (“Managed sessions use Studio”, “Sessions in its folders use North team”, or “Nothing selected here yet”); the ordinary CLI's login, one per machine, is named on the provider heading (“Ordinary Claude Code: Studio”) and by the blue “In Claude Code” / “In Codex CLI” badge, never by a bar (SB-142, operator 2026-10-19: the bar read as selected in one project and not in others).
3. Open a dialog, change navigation, or use a narrow window → Shared roles and visible keyboard focus persist in the active theme.
4. Open About → Appearance offers System (default; follows the operating system's light/dark setting live), Dark and Light; the choice applies at once and is saved on this machine. If it cannot be saved, it applies until restart and About says so.
5. Read About → Version and license: the app version, “Open source under the GNU AGPL-3.0; a commercial license is available at passioncode.ai/business.” with the address https://passioncode.ai/business/, the LICENSE and third-party notice addresses, and “Part of the PassionCode.ai toolkit” with https://passioncode.ai/switchboard/. In the native app the addresses are selectable text (no in-app browser opener exists); in the browser demo they are links opening a new tab.
**Alt paths:** Unknown usage remains unknown; error and disabled states retain their words and actions. In Light, gold stays the action fill while gold-as-text (selection bar, active navigation, meter) and focus use the canonical gold-brown role.
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
**Alt paths:** A delayed or failed current-account read leaves the main account list usable; In use now reads “Could not read the sign-in” with *Retry*, which reads the CLIs again (0.6.82, UX-04: before, a read that timed out read “Reading…” for good). Mutation completion is never inferred from a timeout.
**Expected result:** No indefinite startup spinner. A package smoke check requires frontend render and native round trips from the built executable.
**UI elements:** loading state, account list, current identity, Retry.
**States covered:** loading, empty, ready, unavailable, retry
**Errors & recovery:** Native reads time out after 12 seconds; Retry requests fresh metadata. Existing in-flight mutations are not replayed.
**Status:** validated
**Meaning:** implements the operator request to repair launch and verify the distributed build.
**Coverage:** pending focused and native build checks in launch-repair evidence.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-025 — Keep an optional folder rule in sight
**Persona:** P-01
**Goal:** Start one project on a chosen account without forgetting that the rule exists.
**Preconditions:** At least one enabled account; rules are off unless saved (PLAN-0.4 D-2).
**Entry point:** SCR-01 Accounts → the notices block “N folder rules are active” → *Review rules*, which opens Groups with focus on the *Folder rules* heading (0.6.82, UX-14); or Groups in navigation.
**Steps:**
1. Open Groups → *Folder rules* → without rules, an empty state explains that selection and automatic switching apply everywhere.
2. Add rule → folder, account, target (managed sessions, or the Claude Code login for Claude OAuth profiles only), expiry (default 8 hours).
3. Return to Accounts → the notices block (shared with the provider terms, VIS-13) names each active rule, its account and expiry; navigation shows the count.
4. Pause, Resume (asks for a new expiry), Edit or Remove a rule on Groups.
**Alt paths:** An expired or paused rule stays listed with its state and never applies; removing an account removes its rules.
**Expected result:** A rule applies only when an agent or `switchboard project apply` asks, only to that session, and never turns automatic switching off.
**UI elements:** notices block, nav count, rule cards, rule dialog.
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
1. Open Agents → *Connect an agent* (below the installed agents and the OpenRouter key) shows what agents can do through the tools, and the CLI location or “not found”.
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
**Alt paths:** After Claude Code removes its provider OAuth section but leaves only known MCP/plugin sign-ins, switching restores the chosen saved account and preserves those shared sign-ins. Unknown credential envelopes remain refused. Claude Code's MCP and plugin sign-ins stay signed in across a switch (0.5.1). Claude Code is updating its account → Switchboard waits up to 9 s and takes over a lock left behind by a crashed process (0.5.1). Refusals, each with nothing changed (0.5.1): the signed-in account is not saved in Switchboard (“Claude Code is signed in to an account Switchboard has not saved; switching would sign it out. Add it first (In use now → Add to Switchboard).”); the sign-in belongs to another saved account than its settings name — once the provider confirms whose it is, the live token is filed under that account and the switch goes on (only a stored copy saying so is not enough: “Switchboard could not confirm which account Claude Code is signed in to…”); the sign-in belongs to an account Switchboard does not hold (“The Claude Code sign-in does not match the account named in its settings. Sign in again in Claude Code (claude /login), then retry.”); a sign-in nothing attributes yet (“Switchboard could not confirm which account Claude Code is signed in to. Check the connection, or use Claude Code once, then retry.”); a renewed sign-in not yet stored (“Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.”); a session Switchboard launched (“This session runs in a Switchboard home or a terminal in an organization's folder. Run this from another terminal.”). Switching to the account already in use changes nothing. A `Claude Code-credentials` item that a 0.4 build created while Claude Code was signed out trusts only that build; sign out and in once in Claude Code to recreate it ([operations](../OPERATIONS.md)). Switchboard's own account storage is SCN-027.
**Expected result:** Repeated switching raises no dialog for Switchboard or Claude Code.
**UI elements:** none new; the absence of a system dialog is the outcome.
**States covered:** Claude Code signed in, signed out, item created by an older build, Keychain locked.
**Errors & recovery:** A locked or denied Keychain reads “Keychain unavailable. Unlock it and allow access, then retry.”
**Status:** draft
**Meaning:** operator request 2026-10-02 ([PLAN-0.5](../PLAN-0.5.md) C-2, C-3): Claude's items are read, written and deleted only through `/usr/bin/security`, the executable Claude Code uses.
**Coverage:** `external::tests::mcp_only_live_item_is_signed_out_and_activation_preserves_shared_secrets`, `external::tests::signed_out_detection_does_not_accept_unrecognized_credential_envelopes`; `cargo test -p switchboard-core security_cli` (stdin transport, quoting, exit codes); measured `security` exit codes in [release-0.5](../evidence/release-0.5.md); on-screen absence on the operator's Mac not yet observed.
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
2. After a reinstall (or `switchboard uninstall` and a new install) → at its first start Switchboard restores the newest backup this Mac can open by itself: accounts, projects, project rules, managed selections and the open-at-login and auto-update settings come back, and the notice says “Switchboard found your backup from {date} and restored it: …”. An empty account list with a backup beside it also offers Restore from backup; About → Backups → *Restore…* asks first: “Restore the backup from {date}?”, how many accounts it holds, and that accounts, groups, rules and settings in it come back, including any removed since; *Restore* in that dialog restores it (0.6.82, UX-22).
**Alt paths:** Back up now writes one at once. A backup from another Mac or after the Keychain was erased reads “This backup was made with another key and cannot be opened on this machine.” A restored account whose sign-in has since rotated shows Sign in again.
**Expected result:** Accounts, projects, rules, selections and settings are back; nothing newer and no choice made on this install is overwritten; policies return switched off. A person who removed their accounts keeps them removed: only a new data folder restores on its own.
**UI elements:** Backups panel (folder, limit note, list, *Restore…* with its confirmation, Back up now), notice.
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

## SCN-034 — Keep a group on its own accounts
**Persona:** P-01
**Goal:** Attach accounts to a project — one or more related repositories — so that only sessions from those folders use them, and agents in other projects never switch to them.
**Preconditions:** saved accounts; the project's folders exist on this computer.
**Entry point:** Groups → + New group (the empty state's *+ New group* while there is none, VIS-21).
**Steps:**
1. + New group → focus on *Group name*; folders start empty (0.6.82, UX-23: a folder picked for something else is not offered), each added with *Add folder…*, the system picker, removable with ×; check the accounts → *Create group* → “Group “{name}” created. Launch its accounts from its folders.” The accounts move into the group; the Accounts page lists them under “Group · {name}”, and their button is Select, not Switch. The account list in the dialog scrolls with the dialog, not on its own (VIS-15).
2. Launch one of them (isolated or managed) from any of its folders or subfolders → the session starts; managed automatic switching moves only among the project's accounts.
3. From a folder outside the group → refused: “This account belongs to a group. Launch it from one of the group's folders.”
4. Inside the group's folders, an account outside it for a provider the group has → refused: “This folder belongs to a group. Launch one of the group's accounts.”
5. Edit → add a folder (*Add folder…*), change one (*Change…* opens the system picker at it; the new folder takes its place) or remove one (×); every folder is chosen with the system picker, never typed (SB-131); and change accounts; an account left out goes back to Default. A card with more than six repositories shows the first six and *Show all repositories ({n})*. Delete → the accounts stay saved, no longer reserved for it. The *Folders* help says what joins by itself: worktrees and clones of the group's repositories; any other repository is added here or from *Repositories on disk* (0.6.82, UX-09; SB-171 removed joining by work).
**Alt paths:** The account the ordinary Claude Code or Codex is signed in to cannot join (“…Switch the CLI to another account first…”). A folder already in another project is refused. Switching the ordinary Claude Code to a project account, a native rule or native rotation on a project pool are refused: the ordinary Claude Code serves every folder. A provider the project has no account for keeps using the other accounts in its folders.
**Expected result:** a group's accounts serve only its folders; other groups' sessions, automatic switching and switches never reach them.
**UI elements:** Groups section, group dialog, group cards, “Group · {name}” heading on Accounts; a pool that holds accounts but no folders reads “Group {id} · no folders”, the default pool “Default group” (D-5).
**States covered:** none, created, edited, deleted, refused (CLI account, overlapping folder, outside folder, foreign account inside).
**Errors & recovery:** every refusal names what to change; nothing moves on a refusal.
**Status:** draft
**Meaning:** operator request 2026-10-05.
**Coverage:** core `a_project_takes_its_accounts_into_its_pool_and_gives_back_the_ones_left_out`, `projects_never_share_a_folder_or_an_account_identity`, `a_project_pool_never_drives_the_ordinary_claude_code`; runtime `a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts`, `a_project_account_never_becomes_the_ordinary_claude_code`, `the_account_the_cli_uses_cannot_join_a_project`; CLI `project_commands_work_offline_from_the_cli`; browser demo 2026-10-05 (create, CLI-account refusal, “Project · Client Alpha” heading, Select on a project account).; `scripts/test-ui-logic.mjs` (SB-131 `replaceFolder`: *Change…* puts the new folder in the old one's place); owner `a_repository_a_projects_agent_changes_joins_it_and_a_shared_one_joins_neither` (an Edit dialog opened before a folder joined keeps it on Save)
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
4. “Give a group its own accounts”: Groups opens, + New group is outlined (the empty state's one while there is no group).
5. “Agents, and it keeps running”: Agents opens with *Copy* beside the prompt for a coding agent outlined (SB-93: the agent sets Switchboard up, SCN-050); the menu-bar icon is named. When the prompt is not on the page yet (setup still reading or failed), the *Connect an agent* panel is outlined instead (0.6.82, UX-30). Start using Switchboard ends it.
**Alt paths:** Skip tour or Escape ends it at any step; Back returns. A browser that refuses storage shows the tour again next start rather than never. While the usage question is open (SCN-073), or its status is still being read, the tour waits and starts once it is answered; Escape does not end a tour that is not on screen (issue #128).
**Expected result:** a person knows the five things the product does and the control for each, in under a minute; the tour never blocks the screen it explains: the card sits below, above or beside its target, never on it, and the target is scrolled into sight first, also when it appears only after its page has read (Agents) or lies off screen, never otherwise on a background refresh; a result line moves aside only while the tour is on screen (0.6.82, VIS-12; checked at 740 and 1100 px).
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
**Entry point:** Agents → Installed → Set up on the agent's row (since 0.6.31, SB-120; before, a grid of every catalog agent); CLI `switchboard agents`.
**Steps:**
1. Agents → *Installed* lists the agents found on this computer (SCN-057); *Available to install* lists the rest with their install commands. Each agent's level (launch, through Switchboard, tools only) is in [AGENT-SUPPORT.md](../AGENT-SUPPORT.md).
2. Set up on an agent → choose the pool → the dialog shows the command that registers the tools, the endpoints, the key command (`switchboard agents key`) and, for launchable agents, Launch in folder.
3. Launch → the agent opens in Terminal in the folder, on the pool's API-key account; Switchboard switches accounts for it.
**Alt paths:** The pool's selected account is a subscription sign-in → “This pool's selected account is a subscription sign-in, which its provider allows only in Claude Code or Codex. Select an API-key account in this pool for other agents.” A project's account outside its folders, or a foreign account inside them, is refused as for any launch. Tools-only agents have no endpoint step.
**Expected result:** every listed agent gets Switchboard's tools; the ones that accept an endpoint use Switchboard's accounts within the providers' terms.
**UI elements:** Installed rows, Set up dialog, copyable commands.
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
1. Install and open Switchboard → about 90 s after start, and every five minutes after (operator 2026-10-17), it reads `latest.json` of the newest GitHub release.
2. A newer version is announced → it downloads in the background and is checked against the update key compiled into the app and the version its signature names; a package that fails is discarded (“The downloaded update did not pass its signature check and was discarded. Switchboard tries again within the hour.”).
3. macOS: the verified version replaces the app bundle at once; Windows: the installer waits for the app to quit. About reads “Version {version} is ready. It starts the next time Switchboard opens, or restart now.”; the menu-bar menu gains *Restart to update to {version}*. One system notification per version says so: “Restart to finish updating — Fabric Switchboard {version} is ready. Restart from the menu-bar icon, or it starts by itself at a quiet moment.” (lifecycle rule, operator 2026-10-16).
4. Do nothing → when nobody is looking at the window (it is hidden, or open but out of focus for 30 minutes; SB-131: a window simply left open no longer holds an update back for days; the window then closes and Switchboard comes back in the menu bar, and an unsaved dialog in it is lost) and nothing is running through Switchboard (no managed request, no sign-in) on two looks five minutes apart, it restarts into the new version in the background, at most once in six hours since its last restart on its own (`<data>/update-restarted-at`, SB-191; a release marked urgent in `latest.json` at the next quiet moment; *Restart to update* is never held; during that pause About says from when: “Version {version} is ready. It starts on its own when Switchboard is idle in the background after {time} or opens again, or restart now.”, SB-208); otherwise the next start (a reboot, a login, a Quit and reopen) runs it. A newer release published meanwhile replaces the waiting one, and About names the newer version (SB-105). Or press *Restart to update* → the owner drains (LC-01) and the new version starts, in the background if the window was hidden.
5. About → *Check for updates* checks at once, with automatic updates off too. The check runs in the background — the rest of the window stays usable while it downloads — and ends with a notice saying how it went; a failed check shows its sentence even with automatic updates off (SB-104).
6. About → uncheck *Install updates automatically* → “Automatic updates are off. Switchboard will not check for new versions.”; the choice is saved in `<data>/auto-update` and travels with backups.
**Alt paths:** An app the person cannot replace without an administrator password (macOS) is not installed in the background; About says “Restarting asks for an administrator password to replace the app.” and *Restart to update* asks for it; a refused password reads “The update needs an administrator password to replace Switchboard in this folder. It was not installed.” No network, a GitHub outage or a failed download reads “Could not check for updates…” / “Could not download the update…” and retries within the hour. A quit by logout or shutdown (a signal) starts no installer on Windows; the next ordinary quit does. A copy run from a translocated path reads “Move Fabric Switchboard to the Applications folder to receive updates.”
**Expected result:** the person runs the newest release without acting; accounts, settings, connections and backups are untouched by an update.
**UI elements:** About residency panel: checkbox “Install updates automatically”, status line, button “Restart to update”; tray item “Restart to update to {version}”.
**States covered:** unavailable (development, translocated), off, checking, current, downloading, ready (installed / needs a password / replaced by a newer one / paused until a time, SB-208), held (needs a person), failed (check, download, signature, install, stalled), restarting (by the person, at an idle moment).
**Errors & recovery:** every failure keeps the running version and retries within the hour; nothing is half-installed — macOS swaps the bundle outside the quit path, Windows installs in NSIS update mode, which never uninstalls and never deletes the app data.
**Status:** draft
**Meaning:** operator request 2026-10-05 («автоматический механизм обновления по дефолту был включен для всех версий… ничего не делая, подтягивать новые версии»).
**Coverage:** `updates::tests` (on by default, saved switch, unavailable copies, relaunch keeps `--background`, bundle replaceable without a password, `a_waiting_update_says_when_the_pause_ends` for SB-208), `residency::tests::the_tray_names_the_version_it_restarts_into`, `scripts/test-ui-logic.mjs` (status line, restart offer and the paused line), `scripts/test_updater_artifacts.py` (latest.json names only files of the release with their own signatures; the urgent mark, `Urgent`, SB-207; the macOS archive layout), the macOS package rebuilt from the installed notarized 0.6.0 app and verified by codesign, stapler and Gatekeeper on the unpacked copy (2026-10-05). The first live update is the release after the one that ships this: acceptance (SB-15).
**Product:** unobserved
**Traces:** SB-55, LC-01, LC-09, SCN-030, SCN-033

## SCN-038 — Continue a workflow on another account when a limit runs out
**Persona:** P-01
**Goal:** An agent's long task (an Observatory workflow) stops because its account hit its limit; another account picks it up from the last checkpoint — of the same provider, or of the other one (Claude Code ↔ Codex, 0.6.7, SB-70) — without redoing finished steps and without the person copying context.
**Preconditions:** Project Observatory installed; the workflow open, its executor silent for two minutes; every key it declares in the Observatory vault; another enabled Claude Code or Codex account; the desktop app or `switchboard serve` running.
**Entry point:** CLI `switchboard continue <wf> --account <id> --dir <checkout>`; without `--account`, the default agent (SCN-058) — on whichever of its accounts has room, or on the OpenRouter key for Hermes and OpenCode (a desktop entry and an MCP tool come later).
**Steps:**
1. `project-observatory full workflow list` names the stalled workflow → run `switchboard continue` with it, the account and the checkout.
2. Switchboard reads the workflow and offers it to the account (reason `limit`) → prints the handoff id and until when it is offered.
3. Terminal opens the account's session in the checkout → its first message tells it to accept the handoff, continue from the checkpoint in the acceptance answer and obey its constraints → it does, and writes checkpoints as it goes. Across providers (a Claude Code workflow on a Codex account, or back) the same steps run; the first message also says the work comes from another agent and that the checkpoint is the whole context (`a_workflow_moves_to_another_provider_with_its_context`, `the_prompt_of_a_cross_provider_handoff_names_the_previous_agent_safely`).
**Alt paths:** the executor wrote less than two minutes ago → Observatory's refusal (“…without its lease token a handoff waits until it has been silent for 120 s”), nothing launched; a closed workflow, no checkpoint, a waiting handoff, a key not in the vault (named), a workflow naming no executor, the same account, a folder outside the checkouts → refused before any offer; the session fails to start after the offer → the handoff id and its deadline, the offer lapses on its own; no `--account` and the default is *Auto* → “Name the account or agent to continue on (--account), or choose a default agent on the Agents page.”; the default agent has no account with room → “The default agent has no account that can take this workflow now. …”; the account's agent is disabled for taking over work on the Agents page → “This agent is disabled for taking over work. …” (SB-120).
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
**Entry point:** Agents → *Fallback chains* (since 0.7.0: SCN-080 for this computer, SCN-081 for a group, SCN-082 for whether a chain can take work now); CLI `switchboard chain set|list|clear`; MCP `switchboard_chain_set` / `switchboard_chain_get` (an agent acts only when the operator asks).
**Steps:**
1. `switchboard chain set --preset subscriptions-first` → the machine's chain reads claude-code → codex → kimi-code → hermes (`switchboard chain list`).
2. For one project or one task, `switchboard chain set --scope project:<pool>|workflow:<wf_id> <agent>[@account|#key] …` → `chain list --workflow <wf_id>` shows that chain under “applies”.
3. `switchboard chain clear --scope …` → the next wider chain applies again; with none set, nothing applies.
**Alt paths:** an agent that loads no MCP server or takes no prompt without a person (Aider) → “aider cannot take over a workflow …”, nothing saved; another agent pinned to a Claude or ChatGPT sign-in → “Another agent runs on an API-key account or a paid key, never on a subscription sign-in.”; a key value instead of a vault name → refused; a removed account or project → the executors and chains that named it are dropped.
**Expected result:** the order is stored and shown; when an executor hits its limit, the automatic fallback walks it (SCN-040, SB-73 — Claude Code, Codex, since SB-99 Kimi Code on saved Kimi accounts, since SB-107 Hermes and since SB-119 OpenCode on the saved OpenRouter key). An agent disabled on the Agents page stays in the chain and is skipped (SCN-057).
**UI elements:** CLI output (`machine  claude-code → codex → …`, `applies: …`, presets, and since 0.7.0 one readiness line under each chain); MCP results `{chains, effective, presets}` (since 0.7.0 also `readiness` and `last_handover`, additive), `{chain}`; the window's *Fallback chains* panel and its editor (SCR-03).
**States covered:** none set, machine only, project and task chains over it, cleared, pruned after a removal, refused (agent, pin, key, scope).
**Errors & recovery:** every refusal names the fix and saves nothing; a wrong chain is replaced by setting it again or cleared. The window shows the same refusal text in its editor (SCN-080).
**Status:** draft
**Meaning:** XA-01 decisions D-3 (chains are the operator's, per machine/project/task) and D-4 (MCP for everything), operator 2026-10-06. Amended for 0.7.0 (decision D-5 of 2026-10-25, dossier C node C-1): the window screen exists, so "An app screen comes later (SB-71 remainder)" is gone.
**Coverage:** `chains::tests` (5, switchboard-core), `owner_tests::fallback_chains_take_a_preset_refuse_agents_that_cannot_continue_and_resolve`, `fallback_chains_are_set_listed_and_cleared_from_the_cli`, `handshake_lists_tools_and_rejects_malformed_messages` (MCP chain tools). The 0.7.0 additions (window entry, readiness line, `readiness` and `last_handover` fields): NOT_RUN, none yet (nodes C-2, C-3).
**Product:** unobserved
**Traces:** SB-71, CONTRACTS → Fallback chains, [XA-01](../packets/cross-agent-continuation.md), SCN-080, SCN-081, SCN-082

## SCN-040 — Work moves on by itself when an agent's accounts run out

**Persona:** P-01
**Goal:** A long task keeps going when its agent hits its limit and no other account of that provider is ready — the next agent of the operator's chain takes it over from the last checkpoint without the person doing anything.
**Preconditions:** a fallback chain set (SCN-039); the task recorded as an open Project Observatory workflow with its checkout on this machine; the desktop app or `switchboard serve` running; every key the workflow declares in the Observatory vault.
**Entry point:** none — the background monitor (SB-73). The person sees a Terminal window open with the new session.
**Steps:**
1. The executor hits its limit (Claude Code writes a limit marker, or a managed request is refused) → within a minute Switchboard reads the open workflows and finds it.
2. Switchboard walks the chain: the first agent with a usable account (not the executor's, not limited, not needing a new sign-in) gets the workflow, offered with reason `limit` once the executor has been silent for two minutes.
3. Terminal opens that agent's session in the workflow's checkout → it accepts the handoff and continues from the checkpoint (SCN-038).
**Alt paths:** no chain applies → nothing happens; a step's account is refused before the offer → the next step is tried; the launch fails after the offer → nothing else is tried and the offer lapses; the workflow has no checkout here → skipped (`no_checkout` in the log); Kimi Code in the chain → a saved Kimi Code account takes it (the pinned one, else the least used; never a limited, signed-out or busy one); Terminal runs its first turn as `kimi -p` without approvals, then `kimi -c` resumes it interactively (SB-99); the executor is Kimi Code on a saved account → handed on once Switchboard's background view (every 10 minutes while a chain is set) finds that account at 100 %; Hermes in the chain → Hermes Agent takes it on the saved OpenRouter key, on the model the person's Hermes runs on OpenRouter, else the key's default, the handoff as its first turn in an interactive session listed under Sessions as *Hermes Agent* with its model (SB-107; passed over while Hermes, the key or the `switchboard` command is missing, and refused before the offer — the chain moves on — when neither model is set); OpenCode in the chain → OpenCode takes it on the saved OpenRouter key and its default model, the handoff as its first message (SCN-060, SB-119; passed over while OpenCode, the key, the model or the `switchboard` command is missing); an agent disabled for taking over work on the Agents page → skipped as `agent_disabled` before anything else is weighed (SB-120); agents-settings.json unreadable → nothing is handed over in that pass (`agent_settings_unreadable` in the log); a step pinned to a paid key (`agent#project/env/NAME`) → skipped as `agent_not_automatic_yet` until the project's grant can be checked (SB-96; no longer waiting for ceilings, which exist since 0.7.0), and Kimi Code on a paid key → skipped until SB-73 phase 2; a paid step (Hermes or OpenCode on an OpenRouter key, or Claude Code or Codex on an API-key account while a ceiling covers it) whose key has less than $0.50 left of its own limit → skipped as `key_exhausted` before any offer, with or without a Switchboard ceiling (0.7.0, SB-72); a paid step that would pass a spending ceiling → skipped as `ceiling_reached`, and the question of SCN-084 opens; a ceiling covers the step and its spend cannot be read (OpenRouter refused or did not answer, or an API-key account whose spend Switchboard cannot read) → skipped as `spend_unknown`, an unknown read being a denial; no ceiling covers the step and its key's balance cannot be read → it runs, the provider still enforces the key's own limit, and the log says `spend_unread`; `spend.json` unreadable → nothing paid is handed over in that pass (fail closed); a subscription step and Kimi Code on a saved Kimi account are never gated; the workflow is worked in a group's folder by an account outside that group (the ordinary Claude Code's, for example plain `claude` there) → the group's chain applies, chosen by the checkout folder before the executor's account (0.7.0, dossier C part E), and a checkout held by several groups, none of them the executor account's, takes the group its folder's proxy route and paid key use and logs `group_ambiguous`; no chain applies to that group → nothing is handed over, the log says `no_chain`, and the workflow is looked at again at the next scan; the same workflow is not tried again for 15 minutes; a workflow whose executor names no account is handed over only while it is worked — it wrote within the hour (SB-129), so an idle or forgotten workflow never follows the ordinary CLI's limit.
**Expected result:** the task continues on another agent from where it stopped; the old session is left alone and, if it writes again, Observatory refuses it (`LeaseLost`).
**UI elements:** the launched Terminal session; log event `fallback` and the store event `fallback` (Activity, unchanged: *Hand-over offered* or *failed*); since 0.7.0 one system notification per hand-over offered, and the last hand-over with its skipped steps' reasons on Agents → *Fallback chains* (SCN-082) and in MCP `switchboard_chain_get` (`last_handover`).
**States covered:** nothing to do (no chain, nothing limited), offered and launched, moved down the chain, launch failed after the offer, skipped (no checkout, agent not automatic yet, no usable account, agent disabled, key exhausted, ceiling reached, spend unknown), group chain by checkout, group ambiguous.
**Errors & recovery:** every outcome is logged with a fixed code; a lapsed offer is retried after the window or by hand with `switchboard continue`, which onto a paid agent passes the same spend gate and is refused with its reason before anything is offered (C-Q8); a step skipped for money is tried again at the next pass once the ceiling was doubled (SCN-084), the next UTC day began, or the key was topped up.
**Status:** draft
**Meaning:** XA-01 decision D-1 (automatic), operator 2026-10-06. Amended for 0.7.0 (D-5 of 2026-10-25, dossier C parts D and E; answers C-Q3 and C-Q5 in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md)): the spend gate at the switch, the group's chain by the checkout folder, the status of the last hand-over.
**Coverage:** `fallback::tests` (resolve, executor_limited, cadence, `a_limited_executor_hands_its_workflow_to_the_next_agent_of_its_chain` with a stand-in engine and a real limit marker, `a_refusal_moves_down_the_chain_and_a_failed_launch_after_the_offer_stops`); Kimi Code (SB-99): `a_kimi_step_runs_on_a_usable_kimi_account_pinned_or_least_used`, `a_kimi_executor_is_limited_only_by_its_named_saved_account`, `a_continuation_runs_its_first_turn_then_resumes_the_session_interactively`; Hermes (SB-107): `a_hermes_step_runs_on_the_openrouter_key_after_the_subscriptions_run_out`, `a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key`; OpenCode and the disabled skip (SB-119/120): `an_opencode_step_runs_on_the_key_and_a_disabled_agent_is_skipped_first`, `an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key`. The 0.7.0 additions: the spend gate and the last hand-over (C-5) `a_hermes_step_on_a_key_with_nothing_left_is_skipped_not_offered`, `a_reached_group_ceiling_skips_the_paid_step_and_opens_one_question`, `an_unread_balance_with_a_ceiling_is_a_denial_and_without_one_runs`, `a_subscription_step_is_never_gated`, `nothing_is_read_from_openrouter_when_no_paid_step_is_reached`, `an_api_key_claude_step_under_a_ceiling_is_unknown_and_reads_nothing`, `an_opencode_step_under_a_ceiling_it_cannot_read_is_gated_like_hermes`, `the_last_hand_over_is_recorded_with_its_skipped_steps`, `the_automatic_fallback_gates_its_step_once_and_never_waits_on_itself`, `two_slow_paid_steps_end_inside_the_passes_gate_budget`, `a_ceiling_set_meanwhile_for_a_new_group_survives_the_hand_overs_writes`; the group's chain by checkout (C-8) `a_groups_chain_applies_to_a_workflow_in_its_folder_on_the_ordinary_cli`; the notification: NOT_RUN (node C-7).
**Product:** unobserved
**Traces:** SB-73, CONTRACTS → Automatic fallback, [XA-01](../packets/cross-agent-continuation.md), SB-72, SCN-082, SCN-083, SCN-084, FLW-05

## SCN-041 — Use Switchboard in Russian

**Persona:** P-01
**Goal:** Read and operate Switchboard in Russian — or in English on a Russian system — without a setting to find first.
**Preconditions:** any build; the desktop app or the browser demo.
**Entry point:** start the app; About → *Language* to override.
**Steps:**
1. Start Switchboard on a system whose first preferred language is Russian → every screen, dialog, notice, the tour, dates and the menu-bar (Windows: notification-area) menu are in Russian; on any other language they are in English.
2. About → *Language* → choose *Same as system*, *English* or *Русский* → the choice is saved on this machine and the window reloads in that language; the tray menu follows at once.
3. A backend refusal appears → it is shown in the chosen language (the window compares the English sentence and translates it where it is shown, L10N-04); the journal names its events in that language.
**Alt paths:** the choice cannot be saved (storage refused) → “The language could not be saved on this machine.” and the window stays as it was; a string missing from the Russian dictionary shows in English, never as a key (the gate prevents it in a release).
**Expected result:** one language per window; Russian follows the PassionCode.ai glossary (fabric-workspace knowledge/localization.md); CLI output, logs, the journal's stored codes and MCP answers stay English.
**UI elements:** About panel “Language” with three radio options and a help line; tray items “Открыть Switchboard”, “Завершить Switchboard”, “Перезапустить для обновления до {version}”.
**States covered:** system Russian, system other, overridden to English, overridden to Russian, save refused.
**Errors & recovery:** the only failure is the save; choosing again retries it.
**Status:** draft
**Meaning:** operator request 2026-10-07 («нужен перевод на русский»; system language plus a switch).
**Coverage:** `scripts/test-ui-logic.mjs` (detectLocale, parseLocaleChoice, resolveLocale, Russian plurals 1/2/5/11/21/22/25/111, a missing entry shows English, L10N-04, journal labels cover `event_valid`), `scripts/check-locale.mjs` in the gate (every interface string has a Russian entry with the same placeholders and three plural forms), `language::tests` (Russian language tags), `residency::tests` (tray labels in both languages); the browser demo reviewed in Russian on Accounts, Projects, Agents, Activity and About (2026-10-07).
**Product:** unobserved
**Traces:** SB-76, SB-66, RM-25 (fabric-workspace roadmap), L10N-01…06

## SCN-042 — Delete a group or remove a rule only after confirming

**Persona:** P-01
**Goal:** Never lose a project or a project rule to a stray click.
**Preconditions:** at least one group or rule (Groups screen).
**Entry point:** Groups → a group's *Delete*, or a rule's *Remove*.
**Steps:**
1. Press *Delete* on a group → a dialog “Delete group?” says what happens: “Delete “{name}”. Its accounts stay saved and are no longer reserved for it; sessions in its folders use the usual selection again.”, with *Cancel* (focused) and *Delete group*.
2. Press *Delete group* → the group goes; notice “Group “{name}” deleted. Its accounts stay saved and are no longer reserved for it.” (0.6.82, D-1: no pool id in the sentence.)
3. Press *Remove* on a rule → “Remove rule?”: “Remove the {provider} rule for {path}. Sessions there use the usual selection again. To stop it for a while, pause it instead.”, with *Cancel* (focused) and *Remove rule* → “Rule removed.”
**Alt paths:** *Cancel* or Escape closes the dialog and changes nothing; a refusal from the backend stays in the dialog with its text, the dialog open.
**Expected result:** a group or rule disappears only after the second, explicit press; *Pause* stays the one-step reversible choice for a rule.
**UI elements:** dialogs “Delete group?” / “Remove rule?”, buttons *Delete group*, *Remove rule* (danger), *Cancel*.
**States covered:** asking, cancelled, deleted / removed, refused (error in the dialog).
**Errors & recovery:** the dialog keeps the error and the person can retry or cancel.
**Status:** draft
**Meaning:** UX audit 2026-10-06 (SB-69): both acted on one click while removing an account asked first.
**Coverage:** demo walk-through 2026-10-07 (Russian: project created, *Delete* asked, deleted; rule *Remove* asked); `deleteProjectDialog`, `removeRuleDialog` in src/main.ts.
**Product:** unobserved
**Traces:** SB-69, SCN-034

## SCN-043 — Recover when a read or a refresh after an action fails

**Persona:** P-01
**Goal:** Know what failed and how to retry, without mistaking it for lost accounts.
**Preconditions:** the native app's reads fail or time out (the owner restarting, a store error).
**Entry point:** any screen.
**Steps:**
1. A screen's first read fails → an error state titled for that screen: “Unable to load accounts”, “Unable to load groups”, “Unable to load organizations” or “Unable to load activity”, with the reason and *Retry*.
2. An action succeeds but the list cannot be re-read → the action's notice stays, the last list stays on screen, and a line above it says “The action completed, but the list could not be refreshed. Retry loading accounts.” with *Retry* (0.6.82, UX-06: before, the page was replaced by “Unable to load accounts”). A later Refresh that fails keeps the last list the same way.
2a. A refusal the window has no sentence for names the kind of operation and its next step: a change (“Switchboard could not finish this change. Try again; if it fails again, restart Switchboard.”), a read (“…Press Refresh to try again…”), a launch (“The session did not start. Launch it again…”) or a sign-in (UX-08). `scripts/check-error-vocabulary.mjs` keeps every backend sentence in the vocabulary, so this is the rare path.
3. A sign-in's *Finish* fails → the sign-in banner keeps the error and waits for *Retry* instead of repeating on every tick; when the owner forgot the sign-in, the banner offers *Try again*, which starts over only after the old sign-in was cancelled (SB-67).
4. The interface is opened in an ordinary browser (not the app, not the demo) → “Open the native app”: this window has no access to accounts.
**Alt paths:** a native app that does not answer within the read deadline reads “The native app did not respond. Close and reopen Switchboard, then retry.”
**Expected result:** every failure names its screen and its next step; no failure shows as an empty account list.
**UI elements:** error states with *Retry*; the partial-success notice; the sign-in banner's *Retry* / *Try again*; the plain-browser page.
**States covered:** read failed (accounts, groups, organizations, activity), list kept after a failed re-read, read timed out, action done but refresh failed, finish failed, sign-in forgotten, plain browser.
**Errors & recovery:** *Retry* re-reads; nothing is repeated automatically under the owner's lock.
**Status:** draft
**Meaning:** UX audit 2026-10-06 (SB-69): an activity read failure was titled “Unable to load accounts”; these states had no scenario.
**Coverage:** `scripts/test-ui-logic.mjs` (loginOutcome, read deadline), src/main.ts error-state titles per page; the Try-again path in SB-67.
**Product:** unobserved
**Traces:** SB-69, SB-67, SCN-024

## SCN-044 — Turn usage analytics off or on

**Persona:** P-01
**Goal:** Decide whether Switchboard sends anonymous counts, knowing exactly what they are.
**Preconditions:** the installed app (only release builds carry the analytics key).
**Entry point:** About → *Usage analytics*.
**Steps:**
1. Read what is sent: counts of installs, days of use and saved accounts by provider and type; never names, e-mail addresses, sign-ins or group names (docs/ANALYTICS.md).
2. Uncheck *Share anonymous usage counts* → “Anonymous usage analytics are off for every PassionCode.ai tool on this computer.”; check it → “Anonymous usage analytics are on.”
**Alt paths:** not answered yet (issue #128, SCN-073): the panel says the same as the first-run question (“Nothing has been sent yet. Nothing is sent until you answer.”, or for an upgraded install “An earlier version of Switchboard may already have sent these counts from this computer. Nothing more is sent until you answer.”), the switch changes only the choice, and *Continue* answers it with the success line of step 2; afterwards the switch applies at once. Unanswered while another PassionCode.ai tool already turned the shared switch off: no question, the plain switch shows off, and checking it turns counting on for every tool, answers the question and reads “Anonymous usage analytics are on.” (round 2; the line always says what is now true). A development or demo build says “Only installed release builds send analytics; this build sends nothing.”; while reading, “Reading the analytics setting…”; a read failure says “The analytics setting is unavailable. Refresh to retry.”; a save failure shows the error and leaves the checkbox as it was.
**Expected result:** one switch, shared by every PassionCode.ai tool on the machine, and the person sees its state; nothing is sent before the person has answered it once, here or in the first-run question.
**UI elements:** checkbox “Share anonymous usage counts”, help “This switch applies to every PassionCode.ai tool on this computer.”; while unanswered, the line on what was sent and *Continue*.
**States covered:** unanswered, unanswered with the shared switch already off, on, off, unavailable (build), reading, read failed, save failed.
**Errors & recovery:** a failed save keeps the old state and can be retried.
**Status:** draft
**Meaning:** a privacy control without a scenario (UX audit 2026-10-06, SB-69).
**Coverage:** docs/ANALYTICS.md and its planted-identifier test; the issue #128 tests in `crates/switchboard-runtime/src/analytics.rs` (`about_turns_the_switch_on_when_another_app_turned_it_off_before_any_answer`); src/main.ts `analyticsPanel`; `scripts/test-ui-logic.mjs` (the About panel asks while unanswered; its notice goes through `usageAnswerText`).
**Product:** unobserved
**Traces:** SB-69, SCN-073

## SCN-045 — Save one OpenRouter key for agents

**Persona:** P-01
**Goal:** Keep one OpenRouter key that agents launched by Switchboard run on, and see what it may still spend — without the key ever appearing in a file, a command line or a log.
**Preconditions:** an OpenRouter key (starts `sk-or-`); the app, the CLI or an MCP client.
**Entry point:** API keys (called Tokens until 0.6.82, D-2) → *Machine key · OpenRouter* (moved from Agents in 0.6.48; Agents links to it with *Open API keys*) · `switchboard agents openrouter set --key-stdin` (the key on stdin, never an argument) · MCP `switchboard_openrouter_status`.
**Steps:**
1. Save the key with a default model: `switchboard agents openrouter set --key-stdin --model moonshotai/kimi-k2 < key.txt` (the key is piped on stdin, never an argument; interactive input is refused) → “OpenRouter key saved · model moonshotai/kimi-k2”, and the remaining credit OpenRouter reports.
2. Ask the status later: `switchboard agents openrouter status` (or the MCP tool) → saved date, default model, `limit_remaining` and `usage_daily` from OpenRouter itself; never the key.
3. Change the model agents start on: `switchboard agents openrouter model anthropic/claude-sonnet-4.5`.
4. Remove the key: `switchboard agents openrouter remove` → the metadata goes first, then the vault item.
**Alt paths:** a malformed key (“That is not an OpenRouter key: it starts with sk-or-.”) or model id is refused before anything is written; a key OpenRouter refuses is named as such and stays saved, its balance marked with the refusal; an unreachable OpenRouter leaves the key saved with “its balance shows when OpenRouter answers”; a failed save leaves nothing behind — no metadata row, no vault item; after reinstalling Switchboard an encrypted backup puts the key back with its model (SCN-030).
**Expected result:** the key's value exists only in the OS vault; metadata names the service, the saved time and the model; the journal records `agent_key saved|removed`; the value leaves the vault only into a launched agent's environment (`switchboard agents key --service openrouter`, CLI only).
**UI elements:** the *Machine key · OpenRouter* panel on API keys. Before a key: *No key is saved.* and *Add OpenRouter key*; after: the default model, the balance line (“$3.25 of $5.00 left, resets daily · spent today $1.75”, or the refusal), *Change model*, *Replace key*, *Remove key* (asks first); the key field is a password field cleared on submit and never filled again. The journal names the action “Agent key”.
**States covered:** reading, read failed (with *Retry*), no key saved, saved with credit, saved with no spending limit, saved with the credit refused or unreachable, invalid key, invalid model, failed save, removed.
**Errors & recovery:** every refusal names the fix (the key shape, the model id shape, openrouter.ai/settings/keys); a refused or unreachable balance does not delete the key.
**Status:** draft
**Meaning:** one saved key instead of a key pasted per agent (operator request 2026-10-07, XA-02).
**Coverage:** `an_agent_key_lives_in_the_vault_and_its_metadata_names_only_the_service`, `a_failed_agent_key_save_leaves_nothing_behind` (switchboard-core storage tests), `a_reinstall_gets_projects_rules_selections_and_settings_back` (backup carries the key and model), `the_status_reports_what_the_key_may_spend_and_never_the_key` (runtime, against a loopback OpenRouter stand-in), the MCP handshake test listing `switchboard_openrouter_status` and `switchboard_openrouter_model`, `scripts/test-ui-logic.mjs` (the balance line in both languages). A status against the real OpenRouter with a real key is operator acceptance.
**Product:** unobserved
**Traces:** SB-79, XA-02

## SCN-046 — Launch an agent on the OpenRouter key

**Persona:** P-01
**Goal:** Start Hermes, Kimi Code, Goose or another agent that runs on OpenRouter in a project folder, on the saved key and a chosen model, without pasting the key into the agent.
**Preconditions:** an OpenRouter key saved (SCN-045); the agent installed; the `switchboard` command reachable (bundled with the app, or linked under Agents).
**Entry point:** Agents → an agent's *Set up* → *On the OpenRouter key* · `switchboard agents launch <agent> --openrouter [--model <id>] --dir <folder>`.
**Steps:**
1. Open the agent's setup, enter the folder and, if needed, a model other than the default; choose *Launch on OpenRouter* → Terminal opens in the folder and the agent starts on that model; the notice reads “Terminal launch requested for Hermes on OpenRouter.”
2. From the CLI: `switchboard agents launch hermes --openrouter --dir ~/src/app --model moonshotai/kimi-k2` → the same, with no running app needed.
**Alt paths:** no key saved → the agent's setup says “Save an OpenRouter key under API keys first; then {agent} launches on it.” with *Open API keys*, which closes the dialog and opens that page (0.6.82, UX-12); the CLI answers “No OpenRouter key is saved. Add one on the API keys page, or run switchboard agents openrouter set.”; no model given and no default → “Choose a model for this launch, or set the key's default model on the API keys page or with switchboard agents openrouter model.”; a malformed model id → the model refusal; Crush → its setup says it picks the model in its own interface, and it starts with the key only; OpenClaw and Cline → no OpenRouter section, the CLI refuses (“This agent cannot launch on the OpenRouter key…”); the agent not installed → “Hermes is not installed. Install it first, then launch it again.”; a relative folder → the absolute-folder hint; the session cannot read the key when it starts (removed meanwhile, vault locked) → the Terminal prints “Switchboard could not read the OpenRouter key. Check it on the API keys page, then launch again.” and the agent does not start.
**Expected result:** the agent runs on the key and model; the key exists in the agent's process environment only — not in the session script, an argument, the agent's own config or a log; agents whose own saved login would win (Hermes, pi, omp, Qwen Code) start in their own home under Switchboard's data folder.
**UI elements:** the folder field, *On the OpenRouter key* with *Model for this launch* (default named), the agent's caveat from the catalog. With a key saved, *Launch on OpenRouter* is the dialog's primary action (Enter) and *Launch through the proxy* the secondary one — except Kimi Code, which runs on its subscription by default (operator, 2026-10-08): there *Launch on OpenRouter* sits in the section and leaves Kimi's login and config untouched.
**States covered:** no key, key with default model, key without default model, agent picks its own model, not installed, invalid folder, invalid model, key unreadable at start.
**Errors & recovery:** every refusal names the fix; nothing is written to the agent's config; a failed start leaves only the session folder, rewritten by the next launch.
**Status:** draft
**Meaning:** launching an agent on the operator's OpenRouter key from one place (operator request 2026-10-07, XA-02, SB-79b).
**Coverage:** `every_openrouter_recipe_is_complete_and_never_puts_the_key_on_a_command_line` (catalog), `an_openrouter_launch_needs_a_saved_key_and_a_valid_model`, `an_openrouter_session_reads_the_key_when_it_starts_and_never_holds_it`, `a_windows_session_reads_the_key_by_command_and_exits_when_it_cannot`, `a_session_script_hands_the_key_to_the_agent_and_stops_without_it` (runs the zsh script, macOS), `an_openrouter_launch_runs_without_an_owner_and_needs_a_saved_key` (CLI). A real agent answering on a real OpenRouter key is operator acceptance.
**Product:** unobserved
**Traces:** SB-79, XA-02

## SCN-047 — Keep several Kimi Code subscription accounts

**Persona:** P-01
**Goal:** Sign in to each Kimi Code membership once, see what each may still use, and start `kimi` on the one with room left — without the logins overwriting each other.
**Preconditions:** the official `kimi` installed; a Kimi Code membership (kimi.com or kimi.ai).
**Entry point:** Accounts → *Kimi Code* · `switchboard kimi`.
**Steps:**
1. *Add Kimi Code account*, optionally a label and the service, *Open sign-in* → Terminal runs `kimi login`; confirm the code in the browser signed in to that account → the dialog saves it on its own when Terminal reports success: “Kimi Code account saved.”
2. Read the row → the label, tier and service, and “5 hours 64%, resets in 1h 10m · 7 days 31% · Month 42%”, asked of Kimi at each refresh.
3. *Launch…* (a small row button; *Sign in again* and *Remove…* are in the row's ⋯ menu) and a folder from the system picker → Terminal starts `kimi` on that account in the folder (SB-86).
3a. *Sign in again* (or *Sign in* on a signed-out row) → “Sign in to {label} in Terminal. The row updates when you finish.”; the window reads the Kimi accounts every 3 seconds for up to 5 minutes and updates the row with “{label} is signed in again.” as soon as Kimi reads it signed in, with no Refresh (0.6.82, UX-28).
4. The line above the rows names the ordinary `kimi`'s sign-in and its usage; it is never changed. Its login is the slot `config.toml` names (`[providers."managed:kimi-code".oauth]` `key`, `oauth_host`), read as `kimi` 2.1 reads it — a stale `region` marker or an old kimi.com tombstone no longer reads a kimi.ai login as signed out (SB-84).
**Alt paths:** `kimi` not installed → the section says so and offers no sign-in; the sign-in's Terminal closes without success → “This Kimi Code sign-in is no longer waiting. Start it again.” (SB-83: a closed Terminal now reads ended instead of waiting forever); *Open sign-in* is offered again after that, so a new sign-in starts from the same dialog; the account could not be saved after Terminal reported success → the error stays in the dialog and *Save the account* tries again (SB-104); Escape or *Cancel* while it waits → Switchboard ends `kimi login` in Terminal and the sign-in folder goes; a failed cancel keeps the dialog watching; Kimi refuses a saved login → the row says so and offers *Sign in again*; the login waits for Kimi to renew it → “Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.”; Kimi unreachable → “Kimi did not answer. Try again in a minute.”, the account stays; *Remove* asks first and deletes the account's login folder. A session already runs on the account → *Launch*, *Sign in again* and *Remove* answer “A Kimi Code session still runs on this account. Close it in Terminal first.” (SB-99). A plan window at 100 % → the row reads “Limit reached · resets in {time}” with a red dot until that reset; a window whose reset has passed reads “reset, refresh to see it” (SB-100). Kimi answers 404, 429, an unreadable page or no figures → the row says which (SB-100); the figures carry “checked {time} ago”; a refresh that fails says so above the rows while the last figures stay; the ordinary `kimi` line reads “Not signed in. Run kimi login in Terminal.” or “Run kimi once in Terminal to renew its sign-in, then refresh.”.
**Expected result:** each login exists only in its own folder under Switchboard's data, written and renewed only by the official `kimi`; the store keeps the label, region, nickname and tier; no token, e-mail or phone reaches the store, the window, a backup or a log.
**UI elements:** the *Kimi Code* group on Accounts (count, ordinary-kimi line, rows in the account-row layout: K mark with a state dot, label, tier · service, windows line, small *Launch…* or *Sign in*, ⋯ menu with *Sign in again* and *Remove…*), *Add Kimi Code account*, *Refresh*, the sign-in, launch and remove dialogs.
**States covered:** reading, read failed (*Retry*), not installed, no accounts, signed in with windows, refused, stale, unreachable, sign-in waiting, sign-in ended, removed.
**Errors & recovery:** every refusal names the fix; nothing Switchboard does renews or moves a Kimi login.
**Status:** draft
**Meaning:** Kimi Code subscriptions managed beside Claude Code and Codex (operator request 2026-10-07, XA-02, SB-81).
**Coverage:** `finishing_saves_the_account_by_nickname_and_never_the_token` (against a loopback Kimi stand-in, including the ordinary home left byte-for-byte unchanged), `a_token_is_read_only_while_valid_and_a_tombstone_is_signed_out`, `plan_windows_read_ratios_as_numbers_or_strings_and_clamp_them`, `a_sign_in_runs_kimi_login_in_its_own_home_and_a_failed_terminal_leaves_nothing`, `a_launch_needs_a_signed_in_home_and_writes_its_script_outside_it`, `the_base_is_pinned_and_only_a_loopback_stand_in_overrides_it` (runtime), `kimi_accounts_list_offline_and_refuse_unknown_ids_without_touching_the_ordinary_home` (CLI), `scripts/test-ui-logic.mjs` (the windows line in both languages). A real membership signed in through the real Kimi is operator acceptance.
**Product:** unobserved
**Traces:** SB-81, XA-02

## SCN-048 — See and change Hermes's model and provider

**Persona:** P-01
**Goal:** Know which model and provider the ordinary Hermes runs on, and change them without editing its config by hand.
**Preconditions:** Hermes Agent installed.
**Entry point:** Agents → Installed → the Hermes Agent row (since 0.6.31; before, a panel of its own) · `switchboard agents hermes` · MCP `switchboard_hermes_status`.
**Steps:**
1. Open Agents → the Hermes Agent row reads “Model: deepseek/deepseek-v4-pro-0813 · Provider: openrouter”.
2. *Details* → *Change Hermes's model*, pick a provider from the list and/or type a model, *Save* → Hermes's own `hermes config set` runs; the panel shows the new values: “Hermes settings saved.”
**Alt paths:** Hermes not installed → it has no row under Installed, only its install command under Available; Hermes does not answer → “Hermes did not answer. Run hermes config check, then try again.”; a malformed provider or model → refused before Hermes runs; a home with no model section → “not set”.
**Expected result:** only the provider and default model change, in Hermes's own config and format; no key is read, shown or set; sessions already running keep theirs.
**UI elements:** the Hermes row (model, provider, *Details* → *Change Hermes's model*), the change dialog (provider list, model field). The endpoint is in `switchboard agents hermes` and the MCP status.
**States covered:** reading, read failed (shown in the row), not installed, not configured, configured, Hermes error, saved.
**Errors & recovery:** every refusal names the fix; a new provider needs its key inside Hermes (the panel says so).
**Status:** draft
**Meaning:** Hermes's model and provider visible and changeable from Switchboard (operator request 2026-10-07, XA-02, SB-80).
**Coverage:** `only_four_non_secret_keys_survive_the_read`, `provider_and_model_shapes_are_checked_before_hermes_runs`, `a_change_runs_hermes_config_set_provider_first_then_reads_back` (a stand-in `hermes` records the calls), the MCP handshake listing `switchboard_hermes_status` and `switchboard_hermes_set`. Hermes clearing the old endpoint was measured on a scratch `HERMES_HOME`.
**Product:** unobserved
**Traces:** SB-80, XA-02

## SCN-049 — See the provider terms notice

**Persona:** P-01
**Goal:** Know, before and while adding accounts, that Switchboard must be used within the providers' terms and that the person answers for their own accounts.
**Preconditions:** none; first start in this app's window storage for the banner.
**Entry point:** Accounts (banner, from the first start until dismissed) · About → *Provider terms* (always).
**Steps:**
1. Open the app for the first time → Accounts shows, above everything else on the screen, including the empty “Start with one account” state: “Use Switchboard in line with the terms of Anthropic and OpenAI. Breaking a provider's terms can get your account blocked. You are responsible for how you use your accounts.”
2. *Dismiss* → the banner closes, focus returns to the page title, and the choice is saved; later starts do not show it on Accounts.
3. Open About → the *Provider terms* panel shows the same text, dismissed or not.
**Alt paths:** a window store that refuses to save → the banner closes for this run and shows again next start rather than never; Russian → «Пользуйтесь Switchboard в соответствии с условиями использования Anthropic и OpenAI. Нарушение условий провайдера может привести к блокировке аккаунта. Ответственность за то, как вы используете свои аккаунты, несёте вы.»
**Expected result:** the notice is in sight on first run and while the first account is added, can be dismissed once, and stays readable in About; nothing else in the product repeats it.
**UI elements:** the banner on Accounts (text, *Dismiss*), the About panel *Provider terms*.
**States covered:** first start, dismissed, dismissal not saved, About.
**Errors & recovery:** none; storage failure only means the banner shows again.
**Status:** draft
**Meaning:** operator decision 2026-10-08: a plain notice, no legal essay, no mention elsewhere.
**Coverage:** `scripts/test-ui-logic.mjs` (where the notice shows, the dismissal remembered across starts and a refusing store, the text in English and Russian, the wiring in `renderAccounts` and `renderAbout`); browser demo 2026-10-08: banner on Accounts, *Dismiss* stores `switchboard.terms-notice` and survives a reload, About panel present, Russian text after switching the language.
**Product:** unobserved
**Traces:** SCN-001, SCN-035

## SCN-050 — A coding agent sets Switchboard up

**Persona:** P-01, working with a coding agent (Claude Code or Codex)
**Goal:** Get Switchboard working by asking the agent, doing only the steps that are the person's.
**Preconditions:** the `switchboard` CLI is on the agent's `PATH` (or the agent was given its path).
**Entry point:** the person asks the agent to set Switchboard up; the agent runs `switchboard onboard`, or an embedded console's setup session starts it with that guide.
**Steps:**
1. The agent runs `switchboard onboard` and follows the guide; `switchboard onboard --status --json` lists five steps (`owner`, `accounts`, `project`, `mcp`, `proving_call`), each `done`, `todo` (with the command the agent runs), `needs_person` (with what to tell the person, and the command or alternatives the agent runs once the person agreed) or `optional`. Since 0.7.0: with PassionCode.ai connected and listing no organization, one optional `organization` step follows; with organizations, `organization` follows, then `org_group`, `seat`, `hook` and `launch` for each organization, newest first (SCN-076). `next.turn_on` and `next.turn_off` on `hook` are the person's switches: the agent runs one only when the person asks.
2. `owner` not running → the agent asks the person to open the app (or allow `switchboard serve`).
3. `accounts` empty → the agent may start `switchboard login begin --provider <its provider>`; the person finishes the sign-in in the Terminal window. Keeping the account the ordinary CLI is signed in to (`accounts capture`) only when the person says so.
4. `project` → offered once: only if the person wants accounts kept for the folder, and with the accounts named.
5. `mcp` not registered → the agent runs the printed `claude mcp add …` or `codex mcp add …`.
6. `proving_call` → the agent calls `switchboard_accounts`; a server registered during the session is loaded by the next one, so the agent asks for a new session if the tool is missing.
7. The agent re-reads the status after every step and, when it is complete, tells the person what is set up.
**Alt paths:** the agent is not recognised → `mcp` asks for `--host claude|codex`; no data folder yet → no accounts and no project, and nothing is created by reading; a relative `--folder` is refused; a computer with an organization (0.7.0, SB-202) → `complete` stays false until the organization's group holds a signed-in seat of it, and the agent relays the `seat` step's `person` line to the person (SCN-076); a computer without one keeps exactly the five steps; an owner of another version answers, or the sign-ins or sessions cannot be read → the checklist still answers, `degraded` names the read (`snapshot`, `sign_ins`, `sessions`) and the step it touches says so.
**Expected result:** a person who never opened the tour has accounts, the MCP server and a proved call, having done only the sign-in and the app start themselves.
**UI elements:** the CLI's text and JSON answers, the MCP tool `switchboard_onboarding`; in the window, Agents → *Connect an agent* → *Set up with your coding agent*: the prompt (“Set up Fabric Switchboard on this computer: run `switchboard onboard` and follow it.”) with *Copy*, then one line per agent; “Claude Code: Switchboard tools registered.” / “… not registered.”, the same for Codex CLI; and “An agent has called Switchboard: {client}.” or “No agent has called Switchboard yet.”; *Or register the tools yourself* keeps the commands. The tour's last step outlines the prompt's *Copy* (SCN-035). Every *Copy* reads *Copied* for two seconds after it copied, beside the screen-reader announcement (0.6.82, UX-19).
**States covered:** fresh machine, partly set up, complete, unknown agent, server registered but not yet loaded.
**Errors & recovery:** every step is read again on each run; a half-written agent config reads as not registered and is right on the next run.
**Status:** draft
**Meaning:** operator's agents-first strategy, 2026-10-10: the coding agent does every setup step it can; the interface shows the result.
**Coverage:** `onboarding::tests` (11), `onboard_prints_the_guide_and_reads_the_setup_without_creating_storage` (crates/switchboard-cli/tests/commands.rs), `onboarding_reports_the_steps_and_the_first_answered_call_proves_the_connection` (crates/switchboard-cli/tests/mcp.rs); live read on the operator's machine 2026-10-11 (owner and accounts done, MCP not registered with Claude, confirmed by `claude mcp get switchboard`); `agentSetupFacts` in `scripts/test-ui-logic.mjs`; WebKit demo walk-through 2026-10-11 (Agents panel and tour step 5 with the prompt's *Copy* outlined). The 0.7.0 organization steps (node B-3): `a_machine_without_organizations_keeps_the_five_steps`, `an_organization_machine_names_its_group_seat_and_hook`, `a_seat_that_ended_is_not_a_signed_in_seat`, `a_snapshot_from_another_version_still_gives_the_organization_steps`, `an_unread_sign_in_or_session_list_is_named_not_failed` (`crates/switchboard-runtime/src/onboarding.rs`); every command the steps name parses: `onboard_commands::every_command_the_checklist_names_exists` (`crates/switchboard-cli/src/main.rs`). Not run live.
**Product:** unobserved
**Traces:** SCN-035, SCN-001, SCN-076

## SCN-051 — See which sessions Switchboard launched are running

**Persona:** P-01
**Goal:** Know which sessions Switchboard started are still running (Claude Code and Codex, and since 0.7.0 Kimi Code, Hermes Agent, OpenCode, agents launched on the OpenRouter key and agents launched through the proxy), on which account or key, in which folder, and in whose context.
**Preconditions:** the app runs (it owns the list); sessions launched by Switchboard 0.6.18 or later (earlier launches wrote no record).
**Entry point:** Activity → *Sessions*, above the journal; for agents and other products, `switchboard sessions` and the MCP tool `switchboard_sessions` (SB-95).
**Steps:**
1. Open Activity → *Sessions* lists each session that is starting or running: “{agent} · {account}”, its state (*Running*, *Starting*), its folder, and how it runs (“Managed · follows the {pool} pool” or “Isolated”, “in Terminal” or “in an embedded terminal”), with “started {date}” once its script has run. A session that runs in an organization's context carries that organization's badge beside its name, titled “Runs in {name}'s context.” (SB-110, SB-111); a personal session carries none. The context is fixed when the session starts.
1a. Since 0.7.0 (SB-201) the same holds for every launch: a Kimi Code session (launch and continuation), a Hermes Agent or OpenCode continuation, an agent launched directly on the OpenRouter key and an agent launched through the proxy each record their context and set `SWITCHBOARD_CONTEXT`, so their rows carry the badge and `switchboard sessions` names the context. Every row names the agent it runs by its display name (Claude Code, Codex CLI, Kimi Code, Hermes Agent, Goose, …), or by its catalog id when it has none: an agent through the proxy reads “Goose · {account}”, never the provider its route serves. An OpenRouter agent's row reads “{agent} · {model}”; an agent's context follows the group whose key it runs on (the group's own key, else the machine's key, which is personal), and a proxy agent's follows its account. Rows of agents launched before 0.7.0 have no record and are not listed.
2. A session ends (its window closed, the agent quit) → it leaves the list at the next refresh (once a minute, or Refresh).
**Alt paths:** a Kimi Code session in an organization's folder → its row reads personal (no badge): a Kimi account belongs to no organization and the more private context wins; it is launched, not refused, and the row says the truth (A-Q6, assumed; refusing it is a later row); a plain `claude` or `codex` started in a hooked terminal (SCN-074) is not a session Switchboard launched and is not listed; none running → “No session Switchboard launched is running. Sessions started outside Switchboard are not listed.”; the read fails → “Sessions are unavailable. Refresh to retry.”; while reading → “Reading sessions…”. Russian: «Сессии», «Работает», «Запускается», «Управляемая · следует за пулом {pool}», «Изолированная», «в Terminal», «во встроенном терминале».
**Expected result:** a person sees at a glance what Switchboard has running, without opening Terminal windows to find out.
**UI elements:** the *Sessions* panel on Activity (rows with agent, account, state, the organization badge for an organization's context, folder, how it runs, start time).
**States covered:** loading, empty, error, starting, running; personal and organization context for every provider and agent.
**Errors & recovery:** a failed read keeps the last list and says to refresh.
**Status:** draft
**Meaning:** fabric-dashboards' agents-first request item 3 (2026-10-10) and packet SS-01's Sessions view; acting on a session (open its folder, drain, close) is the rest of SB-65. Amended for 0.7.0 (SB-201, dossier A part B).
**Coverage:** `sessionStateLabel`, `sessionHow` in `scripts/test-ui-logic.mjs`; WebKit demo walk-through 2026-10-11 in English and Russian (two sessions: managed running, isolated starting in place); the list itself: `a_launch_records_its_session_and_the_list_follows_its_process`. The 0.7.0 rows (Kimi Code, Hermes, OpenCode, OpenRouter and proxy agent launches with their context): NOT_RUN, none yet (nodes A-10, A-11).
**Product:** unobserved
**Traces:** SCN-016, SCN-050, SB-201, SCN-074

## SCN-052 — An agent keeps working when its account hits a limit

**Persona:** P-01, with several accounts in a pool
**Goal:** Agents go on working when an account reaches a limit, without pauses, as long as another account has room.
**Preconditions:** the session runs in managed mode (launched from Switchboard, or through its proxy).
**Entry point:** none — it happens on its own; Accounts shows the result.
**Steps:**
1. The provider refuses a request for a limit → the same request goes once to the pool's next usable account; the agent receives its answer and does not pause. Later requests go to that account.
2. Accounts: the refused account shows “Limit reached · a managed request was refused” with its reset, or — when the provider's usage figures still showed room — “Hidden personal limit · refused although the usage figures show room”, and the provider's reason in words when it gave one (“The organisation's spend cap is reached, so extra usage is off until it resets.”, “No credits are left for extra usage.”, “Extra usage is turned off for the organisation.”).
3. The ordinary Claude Code (automatic switching on): a limit moves it to the next account even when no account was measured lately; a candidate that fails to activate is set aside and the next one tried in the same pass.
**Alt paths:** every account in the pool refused → the provider's refusal reaches the agent; an agent on an API key keeps the provider's answer (never moved); a native session whose turn already ended does not resume by itself.
**Expected result:** no agent waits on a limit while another account in its pool has room.
**UI elements:** the limit row on each account card (title, reset, reason).
**States covered:** refused and moved, all refused, hidden limit, reason given, no reason.
**Errors & recovery:** a failed switch is retried with the next candidate; the held account becomes usable again at its reset.
**Status:** draft
**Meaning:** operator request 2026-10-11 («агенты должны работать дальше, никаких пауз по лимитам»; «скрытые лимиты … добавлять эту категорию»).
**Coverage:** CONTRACTS → Agents keep working through a limit (tests named there); `limitTitle`, `limitReasonText` in `scripts/test-ui-logic.mjs`.
**Product:** unobserved
**Traces:** SCN-040, SCN-022

## SCN-053 — A stopped session gets a capacity notice once
**Persona:** P-01
**Goal:** Continue unfinished work when a suitable account becomes available.
**Preconditions:** A session host registers a stop with its session, event, account and folder; the Switchboard owner runs.
**Entry point:** CLI `switchboard recovery`
**Steps:**
1. The host registers a stopped session → the wait survives owner restart and account reset.
2. The host reads recovery status → account limits, registered waiting sessions and ready sessions have separate counts; coverage says registered sessions only.
3. Fresh quota arrives for a suitable account of the same provider, in the stopped account's current group (it follows the account, SB-205) → that stop lists the candidate; project restrictions and hidden holds still apply. A waiting stop that is not ready says why in `reason`: `billing`, `account_removed`, `no_account` or `backoff`.
4. The host claims the event and candidate → Switchboard rechecks and durably marks claimed before returning; the host resumes from its checkpoint and reports its result.
**Alt paths:** The original account resets and receives a fresh observation → it can become a candidate. A person cancels work → the host marks cancelled, preventing later claims. The session goes on by itself (on another account, at the reset, the person sends the prompt again) → its next finished turn runs the plugin's `Stop` hook and the stop becomes resumed (SB-204). Nobody resumes it within a day → it reads cancelled (SB-199). A host reports a stop already settled this way → the answer is a success that changes nothing.
**Expected result:** One claim per stop event; a stop becomes resumed by the host's report or by its session's own next finished turn. Switchboard never types a prompt, kills a client or restarts children itself.
**UI elements:** CLI JSON records and error messages; no desktop surface added.
**States covered:** empty, waiting, ready, claimed, resumed, cancelled, error
**Errors & recovery:** A stale observation, wrong project/pool, sign-in requirement or hidden limit prevents a claim. A lost claim response stays claimed for reconciliation. Corrupt storage is an error, never an empty list. Host consumer integration remains unobserved.
**Status:** draft
**Coverage:** `recovery::tests` (0.6.83: `a_session_that_goes_on_by_itself_settles_its_stop`, `a_stop_past_the_hosts_horizon_reads_and_is_written_as_cancelled`, `a_stop_follows_its_account_into_another_group_and_says_why_it_waits`, `a_host_finishing_a_stop_settled_meanwhile_is_answered`), `the_stop_hook_after_a_turn_is_silent_and_never_fails`; [packet](../packets/sb-114-capacity-recovery.md).
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-054 — Take a workflow hand-over in a console
**Persona:** P-01
**Goal:** Let a console take a prepared continuation once.
**Preconditions:** Automatic fallback configured; Fabric hand-overs explicitly enabled with the CLI.
**Entry point:** CLI `switchboard handovers`
**Steps:**
1. A fallback prepares a continuation → its metadata waits in the private hand-over queue.
2. The host takes the hand-over in a terminal → one taker receives the script; concurrent takers are refused.
3. No host takes it within two minutes → the monitor requests Terminal launch while the offer remains live.
**Alt paths:** Hand-overs off (the default) → existing Terminal behavior. Expired offer → removed without launching. A paid hand-over (Hermes, OpenCode or an API-key account) whose spending ceiling is reached, or whose spend cannot be read, when a console takes it → the take is refused with the spend gate's sentence (“A spending ceiling is reached, so no paid agent starts. Its question waits in the Switchboard window.”, or the sentence for an exhausted key or an unknown spend) and the hand-over stays waiting, so the window may double the ceiling meanwhile; the gate reads the folder the hand-over recorded, canonicalized as the first gate read it. A paid hand-over no console took, refused by the gate when the background would open it in Terminal → taken without a start (log `handover outcome=spend_refused`); its offer lapses by itself.
**Expected result:** A queued continuation runs at most once; a launch request is not proof of workflow acceptance; a paid one starts only while its ceiling has room.
**UI elements:** CLI list, setting and run command; the spend gate's refusal sentence; the ceiling's question in the Switchboard window.
**States covered:** empty, waiting, taken, expired, refused by the spend gate, error
**Errors & recovery:** No terminal → refuse before taking; expired offer → never launch; exclusive claim receipt → retain after completion or a crash, hide any leftover pending record and refuse republication. Ambiguous launch → reconcile before any further action. A take refused by the spend gate → nothing is taken; take it again once the window doubled the ceiling or the key has room.
**Status:** draft
**Coverage:** `handovers::tests` (`a_paid_hand_over_the_gate_refuses_is_taken_without_a_start`); `fallback::tests::a_queued_paid_hand_over_is_gated_again_when_it_is_taken`, `fallback::tests::a_hand_over_records_the_canonical_folder_so_its_take_reads_the_groups_key`; CLI `hand_overs_are_listed_and_set_offline_and_run_only_in_a_terminal`.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-055 — Git folders and projects register by themselves
**Persona:** P-01
**Goal:** Keep every coding agent's Git repository and every project's folder list registered without hunting for them, assigning accounts by hand, or being asked about any of them (operator 2026-10-14: «не надо у меня ничего спрашивать, просто показывай, что добавлен в проект, и уведомление один раз»; 2026-10-15: «автодобавление всех репозиториев нужных в проект, когда там агент работает … чтобы не показывать подтверждения»; 2026-10-17: an eligible unassigned Git repository must become a project, not only attach to an existing one).
**Preconditions:** an existing group with at least one folder that is a Git repository (or a Git folder whose parent folder holds a group's repository).
**Entry point:** none — a system notification tells the person afterwards.
**Steps:**
1. A coding agent opens a Git repository in no group → nothing is created: it stays in the default group and is listed under Repositories on disk (SB-146; REQ-002 used to make a group of its own for it, which scattered repositories that belonged together).
2. An agent creates a worktree of an existing project repository, clones one of its repositories again, or clones a new repository of the project's source; or a Claude Code session starts in such a folder → within one background pass (30 s), or at once from the session's hook, Switchboard adds the folder to that existing project (SB-121); its accounts serve it from the next session.
3. An agent working in a group's folder changes another repository → nothing happens: that repository stays where it is, in no group or in its own groups (SB-171, operator 2026-10-22: a folder never moves into a group by itself; SB-131's "joins by work", 0.6.34 to 0.6.67, is gone).
4. One system notification per addition, titled “Folder added to a group”, saying “{folder} is now part of {project}.” (several at once: “Folders added to groups”, “{n} folders added ({groups}).”; native, src-tauri/src/suggestions.rs). Nothing is asked: no question, no banner, no tray count.
5. A session started there by the hook gets one line in its context only when its folder joined a group; a repository in no group gets none.
**Alt paths:** Re-entry returns `in_project` and creates nothing. The person removes a project or folder → it is never created or added back, whatever agents do there. Existing identity matches still attach; two projects matching equally is a conflict, never a guessed new project. Creation is refused by the ordinary `save_project` bounds and overlap checks. A refused existing-project add is retried every ten minutes and logs `project_suggestion add: refused`; the hook's line says it “could not be added yet; Switchboard tries again by itself”. Questions left by 0.6.30/0.6.31 are added by the first pass after the update. A folder in a temporary or cache location (`/tmp`, `$TMPDIR`, `~/.cache`, `~/Library/Caches`), a tool's own folder, the home folder, a filesystem root or a non-Git folder never joins or creates. A worktree of a project repository kept in a tool folder (`~/.codex/worktrees/…`) still joins as a worktree. A repository agents of two projects work in (an organisation wiki, a common docs repository) belongs to neither: if it joined one project by work and another project's agent then works in it, it leaves the first one, and it is never added by work again (log `worked: shared`). A folder the person added is never taken out this way. Saving an Edit dialog opened before a folder joined keeps that folder. A folder already in a project or one containing another project's folder is never added or created. Read: Claude Code sessions and their subagents; Codex sessions and sessions Switchboard launches isolated are not read yet (SB-132). For a provider with an enabled account in the project, accounts of other pools are refused there (SCN-034); a provider with no enabled project account keeps its existing launch fallback, while automatic `--provider` selection still requires the project's selected account. The CLI shows what was added (`switchboard project suggestions` → `added`).
**Expected result:** every new folder of an existing group (a worktree or clone of its repositories) joins that group once, by itself; a repository its agents only work in does not (SB-171); a repository in no group stays in the default group, and no group is ever made for it.
**UI elements:** system notification and one SessionStart context line only.
**States covered:** unassigned left in the default group, existing attached, re-entry idempotent, removed not recreated, conflict refused, non-Git or broad folder refused, added (found, worked in), several added, added then removed by the person, add refused (retried, never asked), temporary or hidden folder ignored, two projects' work, questions left by an older version.
**Errors & recovery:** a refused existing-project add is retried every ten minutes; an unreadable suggestions file adds or creates nothing and is logged; the SessionStart hook always exits 0, and when the owner does not answer within 1.5 s it says nothing and lets the session start (the background pass adds the folder within 30 s; SB-149).
**Status:** draft
**Meaning:** operator request 2026-10-14 (SB-117), changed the same day (SB-121), on 2026-10-15 (SB-131) and automatic account-less registration on 2026-10-17 (REQ-002), superseded by groups on 2026-10-19 (SB-146).
**Coverage:** `suggest::tests` (incl. `temporary_and_cache_folders_never_join_a_project`, `a_worked_repository_is_recorded_once_and_a_deleted_one_is_found_again`, `a_repository_is_the_nearest_folder_with_git_and_never_the_home_folder`, `unassigned_repositories_are_not_guessed_from_conflicts_or_remembered_removals`); `activity::tests` (`a_shell_command_names_the_folders_it_commits_in_and_nothing_it_only_reads`, `edits_and_commits_count_with_the_sessions_folder_and_reads_do_not`, `a_transcript_is_read_only_where_it_grew_and_a_line_being_written_waits`); owner `an_unassigned_git_repository_becomes_a_project_without_changing_provider_fallback`, `the_background_pass_creates_an_unassigned_agents_project_once`, `a_projects_new_folders_join_it_by_themselves_and_a_removed_one_stays_out`, `a_repository_a_projects_agent_changes_joins_it_and_a_shared_one_joins_neither`; CLI `an_unassigned_repository_becomes_a_project_through_the_session_hook`, `project_suggestions_work_from_the_cli_and_the_hook_never_fails`; the app's `suggestions::tests::one_folder_is_named_and_several_are_counted`.
**Product:** unobserved
**Traces:** CONTRACTS → Projects (0.6), SB-117 packet, SB-121, SB-131, REQ-002, SB-146

## SCN-061 — A deleted folder leaves its project by itself
**Persona:** P-01
**Goal:** A project lists only folders that exist: a repository or worktree deleted from disk leaves its project without the person cleaning up (operator 2026-10-15: «следить, чтобы репы, которые в проекте, если они удаляются, то и из свитчера тоже удалять»).
**Preconditions:** a project with two or more folders; Switchboard (the app or `serve`) running.
**Entry point:** none — a system notification tells the person afterwards.
**Steps:**
1. Delete one of the project's folders (`git worktree remove`, `rm -rf`, Finder) → the next look (every five minutes) sees it missing.
2. Still missing five minutes later → it leaves the project; its accounts no longer serve it.
3. One system notification: “Folder removed from a project — {folder} was deleted and is no longer part of {project}.” (several: “{n} deleted folders are no longer part of their projects.”).
**Alt paths:** A folder renamed or moved counts as deleted: add the new place with *Add folder…* (its old route stays in the moved folder's `.claude/settings.local.json` until SB-133). The folder is back before the second look (a checkout being recreated) → it stays. A folder deleted while Switchboard was not running leaves at the first two looks after it starts. A folder whose nearest remaining folder is now another device than it was on (a share or FUSE mount that went away) is not taken as deleted. A folder on a drive that may only be unmounted (`/Volumes/…`, `/media/…`, `/mnt/…`) or whose whole tree is gone up to the filesystem root is not taken as deleted and stays. A project keeps its last folder (log `project_folder_gone kept_last`). A deleted folder that comes back is found again like a new one (SCN-055) — unlike a folder the person removed, which never returns. A removed worktree or clone whose folder kept only tool files (no `.git`; only `.claude/` with its settings files, `.codex` or `.DS_Store`) counts as deleted the same way (SB-152), so it no longer stays in its group as a project.
**Expected result:** project folder lists follow the disk; nothing the person removed comes back, and nothing merely unmounted is lost.
**UI elements:** system notification; the project card shows the shorter list.
**States covered:** deleted and confirmed, back before confirmation, unmounted drive, last folder, deleted then back.
**Errors & recovery:** a refused save leaves the folder and logs `project_folder_gone refused`; the next look tries again.
**Status:** draft
**Meaning:** operator request 2026-10-15 (SB-131).
**Coverage:** owner `a_deleted_project_folder_leaves_its_project_once_it_stays_gone`, `a_removed_worktree_whose_folder_kept_only_its_claude_settings_leaves_the_group`; `projects::tests::a_removed_worktree_left_with_only_tool_files_is_a_husk`; `projects::tests::only_a_folder_really_deleted_counts_as_deleted`; `suggestions::tests::one_folder_is_named_and_several_are_counted` (the notification's words).
**Product:** unobserved
**Traces:** CONTRACTS → Projects (0.6), SB-131, SB-152

## SCN-056 — Work in a project's folder on the project's accounts
**Persona:** P-01
**Goal:** Every Claude Code session started in a project's folders, by the person or an agent, spends the project's subscriptions and not the ordinary sign-in, with every skill, plugin and setting of the ordinary Claude Code still in force.
**Preconditions:** a project with at least one enabled Claude account; Switchboard (the app or `serve`) running.
**Entry point:** start `claude` in any of the project's folders or subfolders, or in a worktree inside them.
**Steps:**
1. Create the project, or add a folder to it → within one background pass (30 s) the folder's `.claude/settings.local.json` carries the project's route; `switchboard project bindings` lists it.
2. Start a new session there → its requests go through Switchboard's proxy to the project's pool; rotation moves only among the project's accounts.
3. Remove the folder or the project → the route goes again; the person's own settings in that file stay.
**Alt paths:** A running session keeps its account until it is restarted. A folder whose settings file is not valid JSON is left alone and reported. A value the person changed is never overwritten back or removed. When every project account is at its limit, sessions there stop on the limit instead of spending another project's accounts. A group bound to an organization, started from a terminal with Switchboard's shell hook (0.7.0, SB-183, SCN-074) → besides the accounts, the session's home moves too: it runs in the group's shell home with `SWITCHBOARD_CONTEXT=org:<id>`, so its sign-in, history, memory and user-scope servers are the organization's and no personal sign-in reaches the proxy; the folder's `settings.local.json` still carries the route and the folder's capability. Without the hook (a non-interactive start, an IDE's own binary, the hook turned off) or in a personal group, this scenario stays as it was: accounts routed, home the person's.
**Expected result:** project work spends project accounts; nothing else changes for the person, except that in an organization's folders a hooked terminal also keeps the organization's history apart from the person's (SCN-074).
**UI elements:** none beyond the project card; CLI `project bindings`.
**States covered:** bound, unchanged, removed, skipped (broken file), port changed; organization folder with and without the shell hook.
**Errors & recovery:** a broken settings file is reported in the log (`claude_binding` skipped) and left for the person to fix. A folder skipped because its values are the person's gets no shell home either: the hook leaves it alone.
**Status:** draft
**Meaning:** operator 2026-10-14 (SB-118). Amended for 0.7.0: what the shell hook adds in organization folders (decision D-3 of 2026-10-25, dossier A part A).
**Coverage:** `bindings::tests`; proxy `a_pool_capability_reaches_its_pool_only`; live measurement in the SB-118 packet. The 0.7.0 addition (shell home in organization folders): NOT_RUN, none yet (nodes A-0, A-4, A-6).
**Product:** unobserved
**Traces:** SB-118 packet, SB-183, SCN-074

## SCN-057 — See which coding agents are installed and choose which take over work
**Persona:** P-01
**Goal:** Know at a glance which coding agents this computer has, and decide which of them Switchboard may hand a workflow to when its account runs out.
**Preconditions:** none.
**Entry point:** Agents (the agents are looked up when the page opens and on the page header's *Refresh*, never in the background; SB-169: the Installed section has no Refresh of its own).
**Steps:**
1. Open Agents → **Installed · N detected** lists every agent found on this computer: agents that can take over work first (Claude Code, Codex, Kimi Code, Hermes, OpenCode), then the tools-only ones. Each row shows its name, the command Switchboard runs (with the OpenRouter model for an OpenRouter agent), **Enabled / Disabled**, **✓ Default** or **Set default**, and **Set up**.
2. Press **Disabled** on an agent → “Kimi Code CLI no longer takes over work.”; the row dims and says *Disabled*; it is skipped in fallback chains and refused by `switchboard continue`. Account switching is not affected.
3. **Details** → what Switchboard can do with it and its documentation link; for Hermes, *Change Hermes's model*. **Set up** opens its tools, endpoint and OpenRouter launch with copyable commands (SCN-036).
4. **Available to install · N** → *Show all* lists the agents not found, each with its install command and **Copy**; after installing one, the header's **Refresh** moves it to Installed.
**Alt paths:** an agent that cannot take over a workflow shows *Tools only* instead of Enabled/Disabled and cannot be the default; disabling the default agent → “… no longer takes over work. The default agent is Auto again.”; the look-up fails → the error and *Retry*.
**Expected result:** the operator sees the real set of agents and chooses which take over work.
**UI elements:** Installed list, segmented Enabled/Disabled, Default mark or Set default, Set up, Details, Available list with Copy, the header's Refresh.
**States covered:** looking up, detected, not detected, enabled, disabled, tools only, default, look-up failed.
**Errors & recovery:** a save that fails keeps the previous state and names the cause; an unreadable agents-settings.json is named with how to restore it.
**Status:** draft
**Meaning:** operator request 2026-10-14 (SB-120), after Orca's agent settings.
**Coverage:** runtime `enabling_disabling_and_the_default_persist_and_are_checked`, `a_newer_file_still_reads_and_an_unusable_default_is_not_ready`; CLI `agents_are_enabled_disabled_and_made_default_offline`; MCP `switchboard_agents` in the handshake test; ui-logic `sortAgents`; WebKit demo [en](../evidence/design-0.6.31/agents-en-1280.png), [ru](../evidence/design-0.6.31/agents-changed-ru-1280.png), [phone width](../evidence/design-0.6.31/agents-en-390.png), [available](../evidence/design-0.6.31/agents-available-en-1280.png).
**Product:** unobserved
**Traces:** SB-119/120 packet D-2, D-3, D-6

## SCN-058 — Choose the default agent
**Persona:** P-01
**Goal:** Say which agent takes over a workflow when none is named — for `switchboard continue` and for Fabric.
**Preconditions:** at least one installed, enabled agent that can take over work.
**Entry point:** Agents → Default agent.
**Steps:**
1. The chips show **Auto** and every installed, enabled agent that can take over work, in chain order; the chosen one is checked.
2. Choose OpenCode → “OpenCode is the default agent.” `switchboard continue` without `--account` hands work to OpenCode on the OpenRouter key; Fabric reads `default` from `switchboard agents status --json` unless a project sets its own.
3. Choose Claude Code, Codex or Kimi Code → `continue` without `--account` takes whichever of that agent's accounts has room now (as a one-step chain would).
4. Choose **Auto** → `continue` asks for an account again.
**Alt paths:** disabling the default agent returns the default to Auto and says so; a saved default that is no longer installed stays shown as “OpenCode (not installed)”, checked, and `agents status` reports `default_ready: false`; fallback chains keep their own order — the default does not change them.
**Expected result:** one machine-wide default, visible in one place, and honoured by `continue` and Fabric.
**UI elements:** default-agent chips, the note under them.
**States covered:** auto, chosen, chosen then disabled, chosen then uninstalled.
**Errors & recovery:** a save that fails keeps the previous default; the default with no account that has room → “The default agent has no account that can take this workflow now. …”.
**Status:** draft
**Meaning:** operator request 2026-10-14 (SB-120).
**Coverage:** CLI `agents_are_enabled_disabled_and_made_default_offline`; runtime `a_newer_file_still_reads_and_an_unusable_default_is_not_ready`; ui-logic `defaultAgentChoices`; WebKit demo [chosen](../evidence/design-0.6.31/agents-changed-en-1280.png).
**Product:** unobserved
**Traces:** SB-119/120 packet D-4

## SCN-059 — Pick an OpenRouter model, starting from the recommended ones
**Persona:** P-01
**Goal:** Launch OpenRouter agents on a good model without hunting through hundreds of ids.
**Preconditions:** an OpenRouter key saved, or about to be.
**Entry point:** API keys → *Machine key · OpenRouter* → *Add OpenRouter key* or *Change model*, or a group's key; or Agents → Set up → launch on the OpenRouter key on an agent's row.
**Steps:**
1. The picker lists **Recommended models** first: MiMo V2.6 Pro, Claude Haiku 5.5, Gemini 3.8 Flash, Qwen3.8 Flash, DeepSeek V4.1 Flash, each with its maker, context and input price per 1M tokens; pressing one fills the model field.
2. Typing in the model field suggests every OpenRouter model that can call tools (“N OpenRouter models that can call tools: type to search, or enter any id.”).
3. Any id can be typed by hand.
4. Save → “Agents launch on {model}.”
**Alt paths:** OpenRouter's list cannot be read (offline, slow) → the recommended models and typing still work and the note says so; the next picker asks again.
**Expected result:** the chosen model is used by every OpenRouter launch and continuation that names none.
**UI elements:** recommended model cards, model field with suggestions, status note.
**States covered:** reading, full list, list unavailable, recommended chosen, typed id.
**Errors & recovery:** an id that is not `vendor/model` is refused with the expected shape; an id OpenRouter does not list is saved as typed and the first launch tells.
**Status:** draft
**Meaning:** operator request 2026-10-14 (SB-119).
**Coverage:** runtime `the_five_recommended_models_lead_and_are_valid_ids`, `only_tool_capable_models_are_listed`; ui-logic `modelMeta`, `tokenCount`; WebKit demo [en](../evidence/design-0.6.31/model-picker-en-1280.png), [ru](../evidence/design-0.6.31/model-picker-ru-1280.png).
**Product:** unobserved
**Traces:** SB-119/120 packet D-1

## SCN-060 — A workflow continues in OpenCode on the OpenRouter key
**Persona:** P-01
**Goal:** When subscriptions run out, the work goes on in OpenCode with a chosen model, without touching the person's own OpenCode setup.
**Preconditions:** OpenCode installed and enabled; an OpenRouter key and model saved; an Observatory workflow; the desktop app or `switchboard serve` running.
**Entry point:** a fallback chain step `opencode`; `switchboard continue <wf> --account opencode`; or `continue` without an account when OpenCode is the default agent.
**Steps:**
1. The workflow's account runs out → the chain offers the next step → Terminal opens OpenCode in the workflow's folder with the handoff as its first message (`--prompt`) and the chosen model (`-m openrouter/<model>`).
2. The session accepts the handoff through Observatory (its own configuration declares the server) and carries on; Sessions lists it as *OpenCode* with its model.
**Alt paths:** OpenCode not installed, no key or no model → the step is passed over (`no_usable_account`), or `continue` refuses before any offer (“OpenCode is not installed. …”); OpenCode disabled → skipped as `agent_disabled`; OpenCode is the executor → not handed to itself; `--mode managed` → refused.
**Expected result:** the work continues on OpenRouter; the person's OpenCode login, sessions and settings stay as they were — only `OPENCODE_CONFIG_CONTENT` for that session carries the key reference, the model and Observatory.
**UI elements:** the Terminal session; the hand-over under Sessions.
**States covered:** offered and launched, skipped, refused before the offer, launch failed after the offer.
**Errors & recovery:** a launch that fails after the offer lets the offer lapse, as for Hermes.
**Status:** draft
**Meaning:** operator request 2026-10-14 (SB-119).
**Coverage:** `an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key`, `an_opencode_step_runs_on_the_key_and_a_disabled_agent_is_skipped_first`. A live run with OpenCode 1.19 and a real key is the operator's.
**Product:** unobserved
**Traces:** SB-119/120 packet D-5

## SCN-062 — Move an account to another group from its card
**Persona:** P-01
**Goal:** Put an account into a project's pool, or back into the default pool, without opening the project and restating its whole account set (operator 2026-10-18: «аккаунты между пулами перемещать через UI, три точки, контекстное меню, добавить в пул, выбор»; CALL-07).
**Preconditions:** at least one saved account; projects are optional.
**Entry point:** an account card's ⋯ menu → *Move to another group…*.
**Steps:**
1. Open ⋯ on the card → *Move to another group…* → a dialog lists *Default group* first, then each group by name (a group's name in a radio label is no tab stop of its own, UX-20); the pool the account is in now is marked “(now)” and chosen.
2. Choose another one → *Move* becomes available.
3. *Move* → the account leaves its old pool (a project gives it up) and joins the chosen one; the notice says where: “{label} moved to group “{name}”.” or “{label} moved to the default group.”
**Alt paths:** the account is the one the ordinary Claude Code or Codex is signed in to → groups are shown but cannot be chosen, and the note says to switch the CLI to another account first; the default group stays available. No groups yet → only the default group, and a note says to create a group under Groups. An account left in a deleted group's pool has no “(now)” option; *Default group* takes it back. The same move from an agent: `switchboard accounts move <id> --pool <pool>` or MCP `switchboard_account_move`.
**Expected result:** the card shows the new pool at once; a managed route the account held in its old pool no longer applies there.
**UI elements:** ⋯ menu item, radio list, *Move* button, notice, dialog error.
**States covered:** into a project, project to project, back to default, from a deleted project's pool, CLI account, no projects, refused.
**Errors & recovery:** a second copy of the same login already in the chosen group → “This account is already saved in that group. Remove the other copy first.”; a group deleted while the dialog was open → “That group no longer exists. Choose a group, or the default group.”; the dialog stays open with the error and nothing moves.
**Status:** draft
**Meaning:** operator request 2026-10-18 (SB-122).
**Coverage:** core `an_account_left_in_a_deleted_projects_pool_returns_to_default`; owner `an_account_moves_between_projects_and_back_to_default`; MCP `accounts_rotation_activity_and_queues_are_reachable_over_mcp`; `scripts/test-ui-logic.mjs` (`moveTargets`).
**Product:** unobserved
**Traces:** CONTRACTS → Projects (0.6), SB-122

## SCN-063 — See a group's repositories with their worktrees under them
**Persona:** P-01
**Goal:** A project reads as its repositories: the main checkout once, its worktrees and clones under it, not as peers (operator 2026-10-16 and 2026-10-18: «вместо того чтобы делать так все репы, ты мог сделать главную, а остальные со звёздочкой, все ворктри»).
**Preconditions:** a project whose folders include a repository's main checkout and one or more of its worktrees (often added by themselves, SCN-055).
**Entry point:** Groups page.
**Steps:**
1. Open Groups → each card lists its repositories, each with its folder count, its main folder and, indented under it, its worktrees and clones by name with their kind and branch (“alpha-web-wt-login · worktree · login”); past six repositories *Show all repositories ({n})* (SB-147; the “+2 worktrees and clones” line is gone).
3. *Edit* → the dialog lists them under their root; × on the root removes it together with them (its label says so), × on one of them removes only that one.
**Alt paths:** a folder that is no Git checkout is a root of its own; a repository whose main checkout is not in the project keeps its first listed folder as root; a clone (same `origin`, its own `.git`) goes under the main checkout as *clone*; the tree cannot be read (Git files unreadable) → the flat list as before. Agents read the same tree in `project list` and MCP `switchboard_projects` (`roots`).
**Expected result:** a project with a dozen worktrees reads as its few repositories; removing a repository leaves none of its worktrees behind.
**UI elements:** group card rows, *Show all repositories ({n})*, dialog folder rows (indented), × with its label.
**States covered:** root with worktrees, root with a clone, no main checkout, non-Git folder, tree unreadable.
**Errors & recovery:** none of its own; a save refused for another reason keeps the dialog open with the error (SCN-034).
**Status:** draft
**Meaning:** operator request 2026-10-18 (SB-135, first part).
**Coverage:** `roots::tests::*` (a real repository and worktree included), owner `taking_a_root_out_of_a_project_takes_its_worktrees_too`, `scripts/test-ui-logic.mjs` (`folderRows`, `withoutFolder`, `folderName`); checked in WebKit on the demo in English and Russian.
**Product:** unobserved
**Traces:** CONTRACTS → Projects (0.6), SB-135

## SCN-064 — Find repositories on disk and put each in its group
**Persona:** P-01
**Goal:** See every repository on disk with its project, and put a loose one in a project or move one with its worktrees to another, without editing folder lists by hand (operator 2026-10-16: «папки на диске должны быть указаны в Switchboard — какая к какому проекту относится, менять это в интерфейсе»).
**Preconditions:** at least one project exists; repositories sit under a folder such as `~/DATA`.
**Entry point:** Groups page → *Repositories on disk*.
**Steps:**
1. *Show repositories* → the repositories under `~/DATA` (else the home folder) are listed; *Not in a group (n)* is chosen, so only loose ones show, each with its name, branch and full path.
2. *Add folder…* (system picker, starts at the last folder chosen) → the list is read again under the chosen folders; × next to one stops looking there; the choice is remembered across restarts.
3. *Add to group…* on a loose repository → a dialog lists the groups; choose one, *Add* → “beta-cli added to group “Alpha Web”.”, the row leaves the loose list; its worktrees join by themselves (SCN-055).
4. *All (n)* → every repository with its groups; *Move…* on one of a group's folders → choose another group, *Move* → it and its worktrees and clones are now that group's.
**Alt paths:** a repository inside a group's folder reads “Inside a folder of “P”” and has no action (it moves with that folder); the group's only repository cannot move (“This is the group's only repository…”), the group keeps it; no groups yet → the dialog says to create one first; more than 2,000 repositories or 20,000 folders → the first ones and a note to choose a narrower folder. Agents: `switchboard project map`, `switchboard project move-root`, MCP `switchboard_repository_map` and `switchboard_project_move_root`.
**Expected result:** every repository on disk is either in a project or visibly loose, and one action puts it where it belongs with its whole tree.
**UI elements:** section heading, *Show repositories* toggle, folder list with × and *Add folder…*, *Look again*, filter (*Not in a group*, *All*), repository rows (name, place, branch, path, one action), move dialog (radio list, current marked “(now)”).
**States covered:** closed, loading, empty (nothing loose / no repositories), list, truncated, error (a folder cannot be read), dialog success and refusal.
**Errors & recovery:** a refused move keeps the dialog open with the reason; when the second save of a move is refused, the first is undone, so nothing is half moved.
**Status:** draft
**Meaning:** operator request 2026-10-16 (SB-135, second part).
**Coverage:** `roots::tests::the_disk_map_lists_repositories_with_their_projects_and_skips_dependencies`, owner `a_root_moves_to_another_project_with_its_worktrees_in_one_step`, MCP `project_edit` test (map and move tools, read-only server), `scripts/test-ui-logic.mjs` (`mapRows`, `rootTargets`, `storedMapFolders`); checked in WebKit on the demo in English and Russian.
**Product:** unobserved
**Traces:** ST-001, FLW-01

## SCN-065 — Give a group its own API key for agents
**Persona:** P-01
**Goal:** An agent on an API key spends the key of the project it works for, picked by the folder it starts in, while every other folder keeps the machine's key, and nothing waits on a choice (operator 2026-10-19: «гермес в одном проекте подключался к open-router одного токена, в другом проекте … к другому»; «мы делаем по тому, откуда запустился агент»).
**Preconditions:** a project; an OpenRouter key for it (`sk-or-`).
**Entry point:** API keys → *Group keys* · `switchboard agents openrouter set --key-stdin --pool <pool>` · MCP `switchboard_openrouter_status` (lists `keys` and `projects`).
**Steps:**
1. Open API keys → *Machine key · OpenRouter* and *Group keys*: each group reads “Own key · model”, “Uses the machine's key”, or “No key: agents here cannot start on OpenRouter”.
2. *Add key…* on a project → the key (a password field) and a default model → “Key saved for “Alpha Web”. Agents started in its folders run on it.”
3. Launch Hermes, OpenCode or any agent on OpenRouter from a folder of that project → it runs on the project's key and model; from a folder of another project, or of none, on the machine's key. Two agents in two projects run at the same time, each on its own key.
4. *Change model*, *Replace key* or *Remove key* on the project → after removal its agents use the machine's key again.
**Alt paths:** an id that names no group is refused (CLI: “That group id names no group. Choose a group, or none for the key every folder uses.”); deleting a project deletes its key, so a later project of the same pool never inherits it; a backup brings a project's key back only together with its project; an agent's key command names its folder (`switchboard agents key --service openrouter --dir <folder>`; without `--dir`, the current folder). Since 0.7.0 (SB-72, SCN-083): each group key row and the machine key panel also show the daily spending ceiling (“Daily ceiling: not set” or “Daily ceiling: ${usd}”, with *Set ceiling…*) beside the key's own limit and its reset; a group on the machine's key cannot take a daily ceiling of its own and says so; *Remove key* on a group with a ceiling leaves the ceiling shown as inactive, never silently deleted.
**Expected result:** the key used is decided at each launch from the launch folder; the key's value never appears in the window, a file, an argument or a log.
**UI elements:** API keys page in the navigation (0.6.82, D-2: Statistics keeps “tokens” for counted usage), *Machine key · OpenRouter* panel, *Group keys* list (group, state, *Add key…* / *Change model* / *Replace key* / *Remove key*, and since 0.7.0 the ceiling line and *Set ceiling…*), key dialog titled with the project's name, notices naming the project; on Agents an *OpenRouter keys* panel with *Open API keys*.
**States covered:** reading, no projects, project with its own key, project on the machine's key, no key at all, saved, replaced, removed, refused pool; with a daily ceiling, without one, ceiling inactive after the key's removal.
**Errors & recovery:** every refusal names the fix; a failed save leaves no metadata row and no vault item.
**Status:** draft
**Meaning:** operator request 2026-10-19 (SB-144). Amended for 0.7.0 (D-5 of 2026-10-25, dossier C node C-1): the group key row shows its daily ceiling.
**Coverage:** `an_agent_starting_in_a_project_folder_gets_that_projects_key_and_others_the_machines` (core storage), launch tests asserting `--dir` in every key command, MCP handshake; checked in WebKit on the demo in English and Russian (a project key added from Tokens, notice, no page errors). The 0.7.0 ceiling line (C-7): `test-ui-logic.mjs` (`ceilingLine`, `keyLimitLine`) and the design review in WebKit, [evidence/c7](../runs/2026-10-26-iteration-0.7.0/evidence/c7/design-review.md).
**Product:** unobserved
**Traces:** SB-144, SB-72, SCN-083

## SCN-066 — Codex in a project's folders uses the project's Codex accounts
**Persona:** P-01
**Goal:** A Codex session started in a project's folder runs on that project's Codex account, exactly like Claude Code there, while Codex anywhere else keeps the person's own sign-in (operator 2026-10-19: «кодекс не атрибуцирован»).
**Preconditions:** a project with an enabled Codex account (the account the ordinary Codex CLI is signed in to cannot join a project).
**Entry point:** Groups → *Codex in group folders* · `switchboard project codex on` · MCP `switchboard_codex_route`.
**Steps:**
1. Read the section → “Off: Codex uses your own sign-in in every folder.” and the projects that have a Codex account.
2. *Turn on* → “Codex sessions in project folders now use their projects' Codex accounts. Start a new Codex session to pick it up.”; the state reads “On: Codex goes through Switchboard.”
3. Start `codex` in a folder of that project → its requests run on the project's account (Activity journals them under it); start it in any other folder → it runs on the person's own sign-in.
4. *Turn off* → Codex's settings lose Switchboard's lines; the person's own lines were never touched.
**Alt paths:** the read fails → the reason and *Retry*; the section reads Codex's settings again on Refresh and whenever the groups or the Codex accounts in them change (0.6.82, UX-03); Codex already has another `model_provider` → the section names the line, *Turn on* is disabled, nothing is written; Codex's settings do not parse → nothing is written, the reason is shown; no project has a Codex account → the section says to move one in first; Switchboard quits → its lines leave Codex's settings and a new Codex uses the person's sign-in; it starts again → they come back. A group bound to an organization, in a terminal with Switchboard's shell hook (0.7.0, SB-183, SCN-074) → `codex` there runs on that group's Codex account through the group's own Codex shell home, whatever this switch says, and the person's `~/.codex/config.toml` is not touched for it; the section says so in one line under the state while the hook is on, a start file holds it and the organization's group has a Codex account in the folder map (“In {organization}'s folders, new terminals use its Codex accounts whether this is on or off.”; several organizations: “In organization folders, new terminals use their Codex accounts whether this is on or off.”; node A-7, ui-logic `codexHookLine`); a folder the proxy does not map is refused upstream with no sign-in rather than run on the person's account; Switchboard not running → that `codex` fails to connect (closed), unlike the switch's own route, which falls back to the person's sign-in when Switchboard quits; an organization's group with no Codex account → `codex` there keeps the person's own sign-in in 0.7.0 (A-Q2, assumed), said by `switchboard shell status`.
**Expected result:** which account a Codex session uses follows the folder it runs in, with no per-folder file.
**UI elements:** section heading, explanation, state line, projects line, conflict line, *Turn on* / *Turn off*; since 0.7.0 the organization line.
**States covered:** reading, off, on and applied, on but not applied, conflict, unreadable, no project with a Codex account; organization folder in a hooked terminal (switch on or off).
**Errors & recovery:** every refusal names its cause and leaves Codex's file as it was.
**Status:** draft
**Meaning:** operator request 2026-10-19 (SB-143). Amended for 0.7.0: organization folders route Codex through the shell home whatever the switch (decision D-3 of 2026-10-25, dossier A part A item 3).
**Coverage:** proxy `codex_through_the_user_provider_uses_the_projects_account_only_in_its_folders`, `a_request_names_its_newest_folder_and_the_deepest_project_folder_wins`; `codex_route::tests::*`; MCP handshake; WebKit demo. The 0.7.0 organization line under the state (node A-7): ui-logic `the_shell_hook_section_reads_every_state` (`codexHookLine`: shown only while the hook is on, a start file holds it and an organization's folder map rows carry a Codex account; one name, several, none, inactive, off; A-7 fix cycle 1); the organization route itself (`codex` through the group's Codex shell home): NOT_RUN, none yet (nodes A-0, A-4, A-6).
**Product:** unobserved
**Traces:** SB-143, SB-183, SCN-074

## SCN-067 — See which accounts and agents work where, and where tokens go
**Persona:** P-01
**Goal:** See, per project, account and coding agent, how many requests went through Switchboard and how many tokens they took (operator 2026-10-19: «отдельный экран статистики … куда токены уходят»).
**Preconditions:** sessions or agents sent requests through Switchboard (managed launches, project folders, the agent capability).
**Entry point:** sidebar → *Statistics* · `switchboard stats --days 30` · MCP `switchboard_statistics`.
**Steps:**
1. Open *Statistics* → the 30-day range is chosen; five tiles read requests (and how many were answered), tokens (sent and received), the share of input read from the cache, tokens per request, and limits and errors with their share.
2. The chart under the tiles shows tokens per day stacked by group (the five busiest, the rest as “Other groups”); *Requests* switches it to requests; a bar names its day and each group's value on hover. Then *By group*, *By repository*, *By account* and *By agent*, one under another at full width so long names fit: requests, tokens, and limits and errors; the busiest first.
2a. A click on a column title sorts that table by it (a count largest first, a name A to Z); a second click reverses it; the title shows ↑ or ↓ and the table keeps its order until the window closes (SB-153).
3. Read *Who works where* → one row per repository, group, account and agent that worked together; *By repository* names each repository's groups under it; a project is a repository with its worktrees and clones, or a folder outside Git; “No folder recorded” holds requests counted before 0.6.53 and, until 0.6.72, requests that named no group folder (since 0.6.72 such a folder counts under the repository that holds it, SB-157); a project keeps its counts and its name after the checkout they came from is removed (SB-156).
3a. *API keys* lists each saved OpenRouter key (every folder's, and each group's own) with its model and OpenRouter's spend today, in all and its limit: agents on OpenRouter (Hermes, OpenCode, Pi) are counted there; none saved → “No OpenRouter key is saved. Add one under API keys.” A note at the end says what is counted where, Kimi Code's usage being on its account card (SB-156).
4. Choose *7 days* or *90 days* → every table and tile shows that range; *By day* lists one row per day with traffic, newest first.
**Alt paths:** nothing counted yet → “Nothing counted yet” and what will appear; an account that was removed reads “Removed account”, the default pool “Default group”, a pool with no group “Group {id} · no folders”, a request with no folder “No folder recorded”; the read fails → “Statistics are unavailable. Refresh to retry.” with *Retry*, and Refresh in the header does read them again (0.6.82, UX-02: before, only changing the range did); the limits-and-errors cell is marked as an alert from exactly 1 % of requests (VIS-23); a few failures among many requests read “<1%”, never “0%”.
**Expected result:** where requests and tokens went over the chosen range is visible without reading a transcript.
**UI elements:** range control, four tiles, five tables, the note on what is counted.
**States covered:** reading, empty, filled, read failed, removed account, default pool.
**Errors & recovery:** a failed read keeps the page and offers *Retry*; the header's Refresh re-reads too.
**Status:** draft
**Meaning:** operator request 2026-10-19 (SB-145). Counts only, from the provider's own `usage`; a session that bypasses Switchboard is not counted, and the page says so.
**Coverage:** proxy `statistics_count_each_request_and_the_providers_own_token_usage_only` (folder by the request's `Working directory`), `stats::tests::*` (`a_request_names_its_folder_without_its_body_being_kept`); runtime `statistics::tests::*` (two checkouts are one project; a 0.6.52 key reads); MCP handshake; ui-logic SB-145 and SB-147 cases (chart days and series, axis, cache share); WebKit demo (ru, 1180 and 960 wide, no overflow, no table scrolling). ui-logic SB-153 case (sort by name and count, second click reverses, B for billions); WebKit demo (tables stacked, sort clicks).
**Product:** unobserved
**Traces:** SB-145, SB-147, SB-153, SB-156

## SCN-068 — Groups of projects: one repository in several groups, and the projects a group holds on hover
**Persona:** P-01
**Goal:** Sessions bind to a group of projects with its accounts; a group may hold one project or many, one project may be in several groups, and the person sees at a glance which projects a group holds (operator 2026-10-19: «проекты, определённые в группы … проект может сразу в несколько групп входить … навёл на группу и всплывашка показала мне проекты»).
**Preconditions:** at least one group.
**Entry point:** sidebar → *Groups*; a group's name anywhere (its card, a pool heading on Accounts, the move dialogs).
**Steps:**
1. Hover or focus a group's name → a tooltip lists “{n} repositories” and their names: one per repository, never a worktree, a clone or a subfolder. Escape closes it until the pointer or focus leaves; a name is a tab stop only where it has a tooltip, and never inside a radio's label (0.6.82, UX-20, WCAG 1.4.13).
1a. A group's card lists its projects, each with its folder count and its folders: the repository's folder, then its worktrees and clones under it (SB-147); past six repositories *Show all repositories (n)*. A project is named by its folder in the person's disk (the main checkout's folder), never by a renamed remote; two projects with one name read as `research-agent (ssheleg)` and `research-agent (prowl-ai)`; a subfolder of a listed folder is part of that folder's project (SB-155).
2. *Groups* → *Repositories on disk* → *All* → a repository in several groups reads “Groups «A», «B»”.
3. *Also add…* on a repository → a dialog lists only the groups it is not in yet → *Add* → “{name} added to group «B».”; it stays in its other groups.
4. *Move…* on a repository in several groups → it leaves the group shown and joins the chosen one.
5. A session started in a folder held by several groups works on the first of them that has an account for its provider; a launch from a group's own card uses that group.
**Alt paths:** one group's folder inside another group's folder → refused (“A folder sits inside a folder of another group. Add the repository itself to that group instead.”); a group's only folder cannot leave it; a repository already in every group has no *Also add…*; a repository in no group stays in the default group and is offered *Add to group…*.
**Expected result:** groups read as sets of projects, a project can be shared, and nothing creates a group by itself.
**UI elements:** group name with tooltip, *Also add…*, the add-to-group dialog, the “Groups …” line.
**States covered:** one group, several groups, no group, nested refused, last folder refused, Git not read yet (folder names until it is).
**Errors & recovery:** a refusal keeps the dialog open with its reason.
**Status:** draft
**Meaning:** operator request 2026-10-19 (SB-146).
**Coverage:** runtime `a_repository_may_be_in_several_groups_and_sessions_take_the_one_with_accounts`, `an_unassigned_git_repository_stays_in_the_default_group_and_no_group_is_made`; CLI `an_unassigned_repository_stays_in_the_default_group_through_the_session_hook`; ui-logic SB-146 case; WebKit demo (tooltip, *Also add…*, “Groups «Alpha Web», «Tools»”).
**Product:** unobserved
**Traces:** SB-146, SB-147
## SCN-069: See which accounts belong to which organization
**Persona:** P-01
**Goal:** A person who works for one or more companies sees which saved accounts are a company's and which are personal, and keeps a company's seats apart; without organizations nothing changes (operator 2026-10-21: «помечали в свитчборде, какие аккаунты к какой организации относятся … если нет, будет просто стандартный switchboard»; 2026-10-22: «это вообще убери в меню куда-то организацию»).
**Preconditions:** at least one saved Claude or Codex account.
**Entry point:** sidebar → *Organizations*.
**Steps:**
1. No organization yet → *Accounts* shows nothing about organizations; *Organizations* shows “No organizations yet” with *+ New organization*.
2. *+ New organization* (page header or empty state) → name and domains (optional, separated by commas) → *Add* → “{name}: added.”; every account with an address on those domains gets the organization's badge on *Accounts*.
3. The *Organizations* page lists each organization as a card: its domains, *Edit…*, ⋯ → *Remove…*, and the accounts it holds, each with its provider and why (“You placed it in {name}.”, “Its team at the provider belongs to {name}.”, “Its address is on a domain of {name}.”).
4. With at least one organization, *Accounts* shows a filter above the accounts: *All · n*, each organization *· n*, *Personal · n*; hovering a badge says why.
5. An account's ⋯ → *Organization…* → *Automatic: {name}* (or *Automatic: personal*), each organization, *Personal* → *Save*. Placing one seat of a Claude Team or ChatGPT workspace brings the team's other seats along.
**Alt paths:** a name or domain another organization has → refused with its reason, the dialog stays open; removing an organization leaves its accounts where they are and places them again by team or domain, else personal.
**Expected result:** the person reads at a glance which accounts are a company's; nothing at the provider changes and nothing leaves the computer.
**UI elements:** the *Organizations* page and its cards, the Accounts filter, the badge, *Organization…*, the add/edit dialog, the remove dialog.
**States covered:** no organization, one, several; attributed by person, team, domain; personal; an organization with no account; filter with no account (“No account here yet.”).
**Errors & recovery:** a refusal keeps the dialog open with its reason.
**Status:** draft
**Meaning:** operator request 2026-10-21 (SB-158); design `docs/research/organizations-2026-10-21.md`.
**Coverage:** core `organizations::tests::*`, storage `assigning_one_seat_brings_its_team_and_removals_clean_up_assignments`, ui-logic SB-158 case, WebKit demo (empty page, add, badges by domain, filter counts, *Personal* through the menu, cards with members).
**Product:** unobserved
**Traces:** SB-158
## SCN-070: Keep a group's work on its organization's accounts
**Persona:** P-01
**Goal:** A person who works for several companies binds a group of projects to one company, so agents in those folders run only on that company's seats, and a group of personal work never spends a company's seat (operator 2026-10-12, RM-28 CX-26, CX-28: «моделируем сейчас, отрабатываем потом»; design `docs/research/organizations-2026-10-21.md` §5).
**Preconditions:** at least one organization exists (SCN-069); at least one group. With no organization, nothing below is shown and nothing is refused.
**Entry point:** *Groups* → a group's *Edit…* (or *+ New group*).
**Steps:**
1. The group dialog shows *Organization*: *Personal* and each organization by name; a new group starts on *Personal*.
2. Choosing an organization lists, among *Accounts*, that organization's accounts as choosable; another context's accounts are shown disabled with why (“Belongs to {name}.” or “Personal account.”).
3. *Save group* → “Group “{name}” saved.”; the group's card carries the organization's badge (no badge for *Personal*).
4. Sessions in the group's folders run only on its accounts; a request on an account that no longer belongs to the group's organization (its domain or team moved) is refused with “This account belongs to another organization than this group. Move it, or change the group's organization.” and rotation skips it.
5. A folder in two groups of different contexts runs in the personal one (the more private wins), fixed for the session.
5a. When none of a group's accounts belongs to its organization any more, the group keeps its folders (requests there are refused, never moved to the ordinary sign-in) and its card says “None of these accounts belongs to {name} any more, so sessions in this group's folders are refused. Add an account of {name}, or change the group's organization.” (SB-170).
6. *Statistics* shows *By organization* above *By group* (each organization and *Personal*, by whose seat the request ran on).
**Alt paths:** the window disables another context's accounts, so a save naming one (CLI, MCP) is refused with “An account chosen for this group belongs to another organization. Choose accounts of the group's organization, or change it.”; choosing an organization while the group still holds another context's account → refused with “This group still holds accounts that belong elsewhere. Move them out first.”, the dialog stays open; removing an organization turns its groups personal.
**Expected result:** a company's work stays on its seats and personal work on personal seats, by the folder alone; nothing at the provider changes and nothing leaves the computer.
**UI elements:** the *Organization* select in the group dialog, disabled account options with their reason, the badge on the group card, the *By organization* table.
**States covered:** no organization (nothing shown); a personal group; an organization's group; a cross-context account in the pool (refused at save, skipped at run time); a folder in groups of two contexts; an organization removed.
**Errors & recovery:** a refusal keeps the dialog open with its reason; a refused request names the account and the way out.
**Status:** draft
**Meaning:** operator decisions 2026-10-12 (RM-28 CX-26, CX-28) and 2026-10-21 (SB-158 design), SB-109.
**Coverage:** core `organizations::tests` SB-109 cases, runtime statistics test, `bindings::tests::a_group_whose_seats_all_left_its_organization_stays_bound`, core backup `a_reinstall_gets_organizations_their_accounts_and_groups_back` (by domain, SB-170), ui-logic SB-109 and SB-170 cases, WebKit demo.
**Product:** unobserved
**Traces:** SB-109, SB-111
## SCN-071: Keep organizations current from PassionCode.ai
**Persona:** P-01
**Goal:** A person who is a member of organizations on PassionCode.ai (Vesta, `org.passioncode.ai`) gets them in Switchboard with their names and domains, kept current, without typing them; Switchboard keeps working the same without it (operator 2026-10-21: «связь с организацией … если нет, будет просто стандартный switchboard»; decision on board thread `63af7559`: a browser sign-in with a loopback redirect and PKCE, a read-only token for `GET /v1/me/organizations`, kept in the OS vault, read at start and at most every 6 hours).
**Preconditions:** none; with no connection nothing changes.
**Entry point:** sidebar → *Organizations* → *Connect PassionCode.ai…*; CLI `switchboard org connect`.
**Steps:**
1. *Connect PassionCode.ai…* → the system browser opens the PassionCode.ai consent page (“Fabric Switchboard wants to read your organizations…”); the window says “Waiting for PassionCode.ai in your browser…” with *Cancel*.
2. *Allow* → the browser shows “Switchboard is connected. You can close this tab.”; the window reads the organizations and says “Connected to PassionCode.ai: {count} organizations.”
3. Each organization of the person appears on *Organizations* with its name and domains and a *PassionCode.ai* mark; one with the same name or a shared domain that the person had typed is linked rather than duplicated, keeping its accounts.
4. Switchboard reads them again at start and every 6 hours; a renamed organization or a new domain shows up by itself, and accounts on the new domain get its badge.
5. *Disconnect* → the token is deleted from the vault; the organizations stay as local ones.
**Alt paths:** *Deny* in the browser → “PassionCode.ai was not connected: access was denied.”; no answer within 5 minutes or *Cancel* → nothing changes, and a cancel reads as a plain notice, not an error; a cancel that itself fails is reported (0.6.82, UX-21); the state line wraps and carries its full text as a title (VIS-11); the person left an organization on PassionCode.ai → it stays as a local organization, its *PassionCode.ai* mark replaced by the line of SCN-079 (since 0.7.0, SB-160: “Not in your PassionCode.ai organizations since {date}…”), its groups still bound; PassionCode.ai unreachable → the last copy stays in force and the panel says when it was read; the grant ended or was revoked → “Sign in to PassionCode.ai again” with *Connect…*; the person has not signed in through their identity provider for a while → “Sign in to PassionCode.ai in your browser, then read again”.
**Expected result:** the organizations Switchboard uses are the person's PassionCode.ai memberships, current within 6 hours; nothing about accounts leaves the computer (only the read of the person's own memberships goes out).
**UI elements:** the PassionCode.ai panel on *Organizations* (state line, *Connect…*, *Read now*, *Disconnect*), the waiting line with *Cancel*, the *PassionCode.ai* mark on a linked organization's card.
**States covered:** not connected; waiting for the browser; connected and current; read failed (unreachable); sign in again; provider sign-in needed; disconnected.
**Errors & recovery:** every failure keeps the last organizations and names the next step.
**Status:** draft
**Meaning:** operator 2026-10-21 (SB-159), board thread `63af7559` (decision `25edcc48`, Vesta VES-APP-TOKEN `9f7c7618`). Amended for 0.7.0: leaving is marked (SB-160, SCN-079); the old alt path said only "no longer marked".
**Coverage:** core `vesta` cases, runtime `vesta::tests` (loopback, exchange, refresh, 401 classes against a synthetic server), ui-logic SB-159 case, WebKit demo. The 0.7.0 released line: NOT_RUN, none yet (node B-7).
**Product:** unobserved
**Traces:** SB-159, SB-160, SCN-079

## SCN-072: Keep an account signed in on another machine out of this one
**Persona:** P-01
**Goal:** A person who works on two computers (a MacBook and a Mac Studio) and has one subscription signed in on both keeps this computer from renewing that account's sign-in, which would sign the other computer out, and from switching or rotating to it (SB-168 D-2, operator decision 2026-10-23 in [two-machines-2026-10-23](../research/two-machines-2026-10-23.md)).
**Preconditions:** at least one saved account.
**Entry point:** *Accounts* → an account's ⋯ → *Used on another machine*; CLI `switchboard accounts update ID --elsewhere true`; MCP `switchboard_account_update` with `elsewhere`.
**Steps:**
1. ⋯ → *Used on another machine* → “{label} is marked as used on another machine. This computer no longer renews it or switches to it, by hand or automatically.”; the card shows *On another machine* (its title says why) and *Use here* in place of *Switch*.
2. Rotation skips the account; its selection in any pool is cleared; a switch to it (CLI, MCP, a project rule) is refused with “This account is in use on another machine. Use one of this machine's accounts, or clear the mark if it moved here.”
3. *Use here* (or enabling the account) → “{label} is used on this computer again.”; the account is enabled and chosen as before.
**Alt paths:** an encrypted backup restored on this machine keeps the mark; removing the account removes the mark.
**Expected result:** an account the person uses elsewhere is never renewed or chosen here, and the way back is one action.
**UI elements:** the ⋯ item, the *On another machine* badge, *Use here*.
**States covered:** marked; cleared; marked and selected before (selection cleared); restore.
**Errors & recovery:** a refused switch names the way out.
**Status:** draft
**Meaning:** SB-175 builds phase 1 of SB-168; marks shared by the machines themselves are SB-177.
**Coverage:** core `an_account_used_on_another_machine_is_never_chosen_here`, backup `a_reinstall_gets_organizations_their_accounts_and_groups_back`, WebKit demo.
**Product:** unobserved
**Traces:** SB-168, SB-175

## SCN-073: Answer the usage question before anything is sent

**Persona:** P-01
**Goal:** Know what Switchboard would count and choose before any count leaves the computer.
**Preconditions:** the installed release app (it carries the analytics key); the shared switch is on; this app has not been answered yet (a first start, or an install from 0.6.81 or earlier).
**Entry point:** automatic, above every page but About, from the first time the window opens; About → *Usage analytics* is the other way to answer (SCN-044).
**Steps:**
1. Open the window → above the page: “Share anonymous usage counts?”, what is counted (installs, days of use, saved accounts by provider and type; never names, e-mail addresses, sign-ins, group names or what you do with your accounts; one random installation number shared by the PassionCode.ai tools), “Nothing has been sent yet. Nothing is sent until you answer.”, the switch *Share anonymous usage counts* (on), *Continue*, and “This switch applies to every PassionCode.ai tool on this computer. You can change it later in About.”
2. *Continue* with the switch on → “Anonymous usage analytics are on.”; the question goes, focus returns to the page title, and the five-step tour (SCN-035) starts.
3. Or clear the switch, then *Continue* → “Anonymous usage analytics are off for every PassionCode.ai tool on this computer.”; the question goes and is not asked again.
**Alt paths:** another PassionCode.ai tool turns the shared switch off while the question is open: the window reads the status again every minute and when it is shown, and the question goes; if *Continue* with the switch on comes first, the switch stays off for every tool and the line reads “Usage counts stay off: another PassionCode.ai tool on this computer turned them off. Switch them on in About to count again.”; an install that sent counts before 0.6.82 reads “An earlier version of Switchboard may already have sent these counts from this computer. Nothing more is sent until you answer.” instead of “Nothing has been sent yet.”; a save failure shows the error, keeps the question and sends nothing; a background start (login item) or a window never opened sends nothing; a development or demo build asks nothing (demo: `?demo=1&usage=ask` or `&usage=sent` shows the question).
**Expected result:** no event, `app_installed` included, leaves before the answer; after an answer of on, only that run's start (and, for a fresh install, the install) is reported, never an account change from before the answer, and the day's activity at the next background pass; after an answer of off, nothing while the shared switch stays off (another PassionCode.ai tool turning it back on resumes counting, Fabric ADR-0127 Amendment 2); uninstalling forgets the answer.
**UI elements:** the question card (heading, two paragraphs, switch, *Continue*, note).
**States covered:** first start, upgraded install, answered on, answered off, answered on after another tool turned the switch off, save failed, background start.
**Errors & recovery:** a failed save leaves the question open; *Continue* again.
**Status:** draft
**Meaning:** operator decision 2026-10-08, issue #128 (Fabric ADR-0127, audit A7-012): on by default, nothing before the answer.
**Coverage:** `crates/switchboard-runtime/src/analytics.rs` tests `nothing_is_queued_or_sent_before_the_answer_not_even_from_a_background_start`, `an_answer_of_on_reports_the_held_start_once_and_nothing_from_before_it`, `an_answer_of_off_sends_nothing_then_or_after_a_restart`, `the_answer_is_this_apps_own_and_never_written_to_the_shared_file`, `an_upgraded_install_asks_once_and_is_not_counted_as_new`, `an_unreadable_state_asks_again_and_never_reports_a_second_install`, `answering_on_after_another_app_turned_the_switch_off_keeps_it_off`, `an_answer_of_off_holds_while_the_shared_switch_stays_off`, `through_the_runtime_nothing_leaves_before_the_answer_then_the_start_and_the_day`; uninstall `uninstall_forgets_the_usage_answer`; `scripts/test-ui-logic.mjs` (`usageQuestion`, `usageQuestionText`, `usageAnswerText`, `tourWaits`, the wiring, the background status read); browser demo 2026-10-24 (WebKit, 740 and 1100 px, light and dark, English and Russian).
**Product:** unobserved
**Traces:** SCN-044, SCN-035, SB-69

## SCN-074: Plain `claude` and `codex` in an organization's folders run in its context

**Persona:** P-01, as a member of an organization (a person who works for a company on the same computer as their own work)
**Goal:** A terminal session the person starts themselves, `claude` or `codex` typed in a folder of a group bound to an organization, runs in that organization's context (its home, its history, its accounts, `SWITCHBOARD_CONTEXT=org:<id>`), so the company's work never mixes with the person's own; leaving the folder gives the person's own settings back exactly (operator decision D-3 of 2026-10-25: on by default on a computer holding an organization's group, off by one command; RM-28 CX-26).
**Preconditions:** the installed app (its owner writes the hook; `serve`, a development build and `--data-dir` never do); at least one group bound to an organization (SCN-070) with at least one folder; an interactive zsh, bash, fish or PowerShell terminal; the hook not turned off (SCN-075).
**Entry point:** none for the set-up: the owner writes it at start, or when the first organization's group appears in a background pass. The person opens a new terminal and changes into a folder of the group. CLI `switchboard shell status` says what applies; `switchboard shell init <zsh|bash|fish|pwsh>` prints the hook for people who keep their own dotfiles.
**Steps:**
1. The first group bound to an organization appears on a computer running the installed app → the owner writes the group's shell homes (`<data>/shell/claude-<pool>` and, for a group with a Codex account, `<data>/shell/codex-<pool>`, one per group, A-Q1), the folder map, the hook files, and one marked block into the start file of each shell present (zsh `.zshrc`; bash the login file on macOS, `.bashrc` on Linux; fish its own file under `~/.config/fish/conf.d/`; PowerShell `profile.ps1` for each edition installed) → one system notification: “Terminals now work in {organization}'s context in its folders” with “New terminals opened in {group}'s folders run claude and codex on {organization}'s accounts and history. Turn this off under Groups.” The app posts it within five minutes of the write (it looks when the folder list changes, and at least every five minutes) and again at each start of the app until the window has shown it; the window shows the same words once, as a notice with *Open Groups*, the next time it is on screen, and from then on neither shows it again (built in node A-7).
2. Open a new terminal and change into a folder of that group (or a worktree inside it) → nothing is printed; the hook keeps the person's own `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `SWITCHBOARD_CONTEXT` (a value, or "was not set") in shell variables that are not exported, then exports the group's shell homes and `SWITCHBOARD_CONTEXT=org:<id>`.
3. Run `claude` → it starts without asking for a sign-in, in the group's shell home: the organization's history, memory, user-scope MCP servers (the Fabric board server included, as launches get it) and settings; the person's carried `CLAUDE.md`, skills, commands, agents, plugins and hooks are linked in (macOS and Linux); its requests go through Switchboard's proxy to the group's accounts (SCN-056), and no personal sign-in travels with them. The person's own `~/.claude` history gains nothing.
4. Run `codex` in a group that has a Codex account → it runs on that group's Codex account through the group's Codex shell home, whatever *Codex in group folders* says (SCN-066); the person's `~/.codex/config.toml` is not touched.
5. Change into a folder of another organization's group → that group's homes and context; the person's saved values stay saved.
6. Change into a folder outside every organization's group → the person's values come back exactly: set back to what they were, or unset if they were not set.
7. `switchboard shell status` → one line each: the hook's state for this computer (on, off, on by default), whether this terminal has the hook loaded, the current folder's group and context, the Claude home, the Codex home or “codex: your own sign-in (this group has no Codex account)”, and each start file with its state (SCN-075).
**Alt paths:** a subfolder of the group's folder (`<group>/src`) → Claude Code reads Switchboard's route only from the folder it starts in, git repository or not (measured 2026-10-26, [shell home measurement](../evidence/shell-home-measurement-2026-10.md)), so there the hook exports no Claude home: `claude` in a subfolder runs in the person's own settings, as it does there without the hook; the group's Codex home and context are still exported when the group has a Codex account, and a group with only Claude reads as no group there. A program with a window started from a hooked terminal in an organization's folder (`code .`, `open -a`, an IDE's launcher) inherits that folder's homes and context, and the `claude` or `codex` it starts runs in the organization's context. A terminal opened before the block was written has no hook until it is reopened; a terminal with the hook picks up a new or changed group at its next change of folder, at most 30 s after the background pass that wrote the map. An organization's group with no Codex account → `codex` there keeps the person's own sign-in in 0.7.0, and `shell status` says so (A-Q2, assumed). A personal group, a folder in no group, or a folder whose `settings.local.json` values are the person's (SCN-056 leaves it alone) → the hook changes nothing. A terminal that Switchboard itself launched (`SWITCHBOARD_SESSION` set) → the hook does nothing. The person sets `CLAUDE_CONFIG_DIR` (or one of the others) by hand while inside → the hook stops managing that variable and never writes over it on leaving. A session started without an interactive shell (`claude -p` from a script or cron, an IDE's own bundled binary, Fabric Dashboards' console) gets no hook: its accounts are still routed by SCN-056 and SCN-066, its home stays the person's. `claude --resume` in a hooked terminal lists the conversations of that shell home, not those of sessions Switchboard launched for the same group (A-Q1, assumed). Claude Code may show its notice that claude.ai connectors are off because another sign-in source is set: an organization's home has no claude.ai login, so nothing is lost (A-Q5, assumed). A Kimi Code session in an organization's folder is not changed by the hook and reads personal (SCN-051). The last organization's group leaves this computer (removed, or turned personal) → the default block is taken out of every start file (an explicit *Turn on* keeps it), the shell homes stay on disk as history, and terminals still open restore the person's values at their next change of folder. A start file that is a symbolic link (dotfile managers) is never written: `shell status` shows the line to add by hand (`switchboard shell init <shell>`, A-Q3, assumed). No `.zshrc` and a `~/.zshenv` (or a `~/.zprofile` naming `ZDOTDIR`) → the app, started from the Dock or at login, cannot see where zsh reads its start file, so it creates none (`skipped_zdotdir_unknown`) and `shell status` gives the line to add (`eval "$(switchboard shell init zsh)"`). A start file Switchboard created (none was there) is deleted when the block comes out and nothing else is in it. `switchboard shell on|off` with another `--data-dir` is refused and changes nothing: only Switchboard's own data folder changes start files. A native sign-in operation (capturing the current account, activating or renewing it) that Switchboard runs inside a hooked terminal in an organization's folder (an offline CLI command such as `switchboard accounts capture` with no app running, or `switchboard serve` started there) is refused like one run from any Switchboard home: “This session runs in a Switchboard home or a terminal in an organization's folder. Run this from another terminal.” A command a running app answers uses the app's own settings and is not refused. Windows (PowerShell 5.1 and 7): the hook wraps the prompt and acts when the location changed, so a PowerShell follows folders at its prompt; a `-Command`, `-File` or `-NonInteractive` run that reads profiles asks nothing as it loads (SB-225), so a script started with them keeps the person's own settings; the block goes into `Microsoft.PowerShell_profile.ps1` under `WindowsPowerShell\` (5.1) and `PowerShell\` (7) of the Documents known folder, OneDrive-redirected Documents included, created when absent, for each edition installed; a prompt the person's profile or a module defines after the block replaces the wrapper, and that PowerShell then keeps the person's settings; a linked profile → the line to add is `switchboard shell init pwsh | Out-String | Invoke-Expression` (it carries its paths as base64, so a user name that is not ASCII survives Windows PowerShell 5.1's console code page); the shell homes there carry the person's `CLAUDE.md`, skills, commands, agents, plugins and hooks as links too (SB-226): a folder as a directory junction (to any local drive), a file as a hard link (on the drive of Switchboard's data only), neither needing an administrator; a file the person's editor replaces is linked again by the next pass, a file the person deletes leaves the home at the next pass, and a file a session replaced in the home is the session's and is kept; a file on another drive than Switchboard's data, and anything on a network share, is not carried, and organization sessions start without it; a profile saved in UTF-16 (Windows PowerShell 5.1's `>`) gets the block in UTF-16 and gives the same bytes back; an execution policy of `Restricted` or `AllSigned` (read, never changed, A-Q4) → that profile is not written and reads `policy_blocked`; PowerShell held to Constrained Language Mode (Application Control holds both editions; `__PSLockdownPolicy` holds Windows PowerShell 5.1 only, PowerShell 7 ignores it) → that edition's profile is not written and reads `constrained_language` (SCN-075). `cmd.exe` and Git Bash are not covered. **The Windows path is not yet verified on Windows (SB-02).**
**Expected result:** in a terminal with the hook, `claude` and `codex` started in an organization's folders use that organization's context (home, history, accounts, `SWITCHBOARD_CONTEXT=org:<id>`); everywhere else the terminal is the person's own, byte for byte; the person was told once and can turn it off in one action.
**UI elements:** one system notification and the same words once in the window, as a notice with *Open Groups* and *Dismiss*; the marked block in each start file (`# >>> Fabric Switchboard: shell hook (SB-183) >>>` … `# <<< Fabric Switchboard <<<`); the CLI's `shell status`, `shell init <shell>` and `shell env` answers; in the window, the Groups section of SCN-075 and one line under *Codex in group folders* (SCN-066).
**States covered:** no organization (nothing written), written and notified, inside an organization's folder, moved between groups, left, personal or unbound folder, launched session's shell, a value the person set inside, non-interactive start, open terminal not yet reloaded, map stale, owner not running, hook file missing, last organization gone, Windows.
**Errors & recovery:** Switchboard not running → the shell home still points at its proxy, as SB-118's folder files already do, so `claude` there fails to connect and `codex` in an organization's folder fails too: closed, never on the person's account; open the app and run it again. The folder map missing, unreadable or of an unknown version, or the hook turned off → `switchboard shell env` answers `none` and the terminal keeps the person's values; it always exits 0 and never writes to the error stream, so a terminal never breaks because of Switchboard, and the fastest of repeated (warm) calls answers within 25 ms without the owner or the store's lock; the first call after a restart is not measured. The hook file or the CLI missing (the app removed) → the guarded line in the start file does nothing and prints nothing. The CLI gone, too old for `shell env` or failing while a terminal is inside an organization's folder → read as no group: the values the hook set are given back at the next change of folder, never kept; zsh waits at most ten seconds for an answer, so a CLI that hangs never holds its prompt longer. A Codex request from a folder the proxy does not map → refused upstream with no sign-in (closed).
**Status:** draft
**Meaning:** SB-183, operator decision D-3 of 2026-10-25 (default on, off by one command; PowerShell added 2026-10-26, X-16); design [dossier A](../runs/2026-10-26-iteration-0.7.0/dossier-a-context-binding.md) part A; answers A-Q1, A-Q2, A-Q3, A-Q5, A-Q7 as assumed in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md) (A-Q4 asked of Appvillis IT).
**Copy:** the notification and the window's notice are checked with Copylot (en-US report c4e52902-fc1c-47dd-85d4-2d03c41b4fd1, ru-RU report 8feda489-b1dd-43ec-a99c-2fa9688b70ff; node A-7); the CLI's `shell status` text is still draft copy.
**Coverage:** partial (code tests; no walk yet). Step 1's notification and the window's notice (node A-7): `src-tauri/src/suggestions.rs` test `the_shell_hook_notice_names_the_organization_and_the_way_out`, ui-logic `the_shell_hook_section_reads_every_state` (`shellHookNoticeText`), WebKit demo `?demo=1&shell=notice` ([evidence](../runs/2026-10-26-iteration-0.7.0/evidence/a7/)). Step 1's homes and folder map (node A-4): `crates/switchboard-runtime/src/shell/homes.rs` tests `an_organization_pools_shell_home_has_the_placeholder_carried_links_and_no_credential`, `a_codex_shell_home_routes_to_auto_with_no_sign_in`, `a_port_change_repoints_shell_homes`, `a_personal_group_gets_no_shell_home_and_no_map_row`, `a_pool_that_leaves_its_organization_keeps_its_home_and_loses_its_rows`, `worktrees_and_folders_the_person_keeps_get_their_own_rows`, `rows_are_written_in_canonical_form_for_folders_that_exist`; `lib.rs` `owner_tests::an_idle_pass_writes_nothing`. Steps 2, 5 and 6 and the alt paths "a terminal Switchboard launched", "the person sets a variable inside" and "the hook file or the CLI missing" (node A-2, real zsh and bash 3.2; fish skipped where not installed): `crates/switchboard-runtime/src/shell/scripts.rs` tests `entering_and_leaving_restores_unset_and_set_values`, `a_value_the_person_set_inside_is_never_restored_over`, `a_launched_sessions_shell_is_never_touched`, `the_saved_values_are_not_exported`, `a_missing_cli_prints_nothing`, `bash_3_2_syntax_is_accepted`, `paths_with_spaces_and_quotes_survive_every_shell`, `a_shell_started_inside_an_organization_folder_gives_the_home_back_on_leaving`, `an_answer_of_another_form_changes_nothing_and_the_prompt_keeps_its_status`, `loading_twice_registers_once_and_bash_keeps_the_persons_prompt_command`, `set_a_stays_on_after_loading_and_after_a_prompt`; CLI `shell_init_prints_a_hook_that_follows_the_folder_map_with_the_real_cli`. Step 7 and the map reader (node A-1): `shell::map::tests`, CLI `shell_env_*` and `shell_status_*`. Step 1's start files and the alt paths "a start file that is a symbolic link" and "the last organization's group leaves" (node A-3): `crates/switchboard-runtime/src/shell/install.rs` tests `the_block_goes_in_and_comes_out_leaving_the_file_byte_identical`, `the_start_files_follow_the_shells_present`, `the_last_organization_group_leaving_removes_the_default_block`, `a_symlinked_foreign_owned_or_changed_rc_is_never_written`; `lib.rs` `owner_tests::the_installed_owner_follows_the_presence_of_an_organization_group`, `lifecycle_tests::the_first_time_notice_waits_for_the_window_across_a_restart` (the notice waits in `shell-hook.json` until the window acknowledges it), `owner_tests::no_test_or_dev_build_touches_the_real_home`. Fix cycle 1 of 2026-10-26: the alt paths "a subfolder" (`shell::map::tests::the_claude_home_is_answered_in_the_bound_folder_itself_only`, measured by `scripts/measure_shell_home.sh`), "no `.zshrc` and a `~/.zshenv`" (`no_zshrc_is_created_when_zshenv_may_move_zdotdir`), "a start file Switchboard created" (`a_start_file_the_installer_created_is_deleted_when_the_block_goes`), "another `--data-dir`" (CLI `another_data_folder_never_touches_the_start_files`, `uninstall::tests::only_the_apps_own_data_folder_takes_the_blocks_out`), the error "the CLI gone, too old or failing" (`a_missing_old_or_failing_cli_gives_the_held_values_back`, `a_hanging_cli_never_holds_a_zsh_prompt`), fish only when interactive (`a_fish_script_never_runs_the_hook`, fish 4.9.3 now installed where the tests ran), one run at a time (`a_second_run_waits_for_the_first`), the choice through a reinstall (`backup::tests::the_terminal_hooks_choice_survives_a_reinstall`). The alt path "a program with a window started from a hooked terminal" follows from inherited environment and has no test of its own. Windows and PowerShell (node A-8): `crates/switchboard-runtime/src/shell/windows.rs` tests `profile_paths_follow_the_documents_folder_for_each_edition`, `a_restricted_or_allsigned_policy_reads_as_policy_blocked`, `the_ps1_hook_names_its_paths_quoted_and_keeps_its_state_global`, `the_ps1_transition_matches_the_posix_one` (steps 2, 5 and 6 and the alt paths above in a real `pwsh`; ran with PowerShell 7.6.9 on macOS on 2026-10-27 and passes after A-8 fix cycle 1, which fixed a variable given back as empty instead of unset; skipped with the reason where PowerShell is not installed); `install.rs` tests `a_powershell_profile_gets_the_block_and_gives_it_back_byte_identical`, `a_powershell_block_the_person_removed_is_not_written_back`, `a_blocked_policy_leaves_the_profile_alone_and_records_policy_blocked`, `a_policy_that_turns_blocked_keeps_what_off_gives_back`, `a_hook_path_that_is_not_ascii_keeps_the_powershell_block_ascii`, `the_cli_a_hook_names_has_no_verbatim_prefix`; CLI `shell_init_pwsh_prints_the_powershell_hook_naming_this_cli`. SB-225 (W2, 2026-10-27): `the_ps1_transition_matches_the_posix_one` runs under every PowerShell present, on the Windows runners Windows PowerShell 5.1 and PowerShell 7 with a batch stub, with scenarios for the non-interactive load and `shell init`'s base64 paths; `install.rs` `a_utf16_profile_is_written_in_its_own_encoding_and_given_back_byte_identical`, `constrained_language_mode_writes_no_profile_and_says_so`; `windows.rs` `constrained_language_mode_is_read_from_application_control_and_the_lockdown_variable`. The folder matched as Windows names it (`\\?\` prefix, case, `/`): `shell::map::tests::a_windows_folder_compares_as_windows_names_it`, on Windows `a_windows_terminal_folder_finds_its_verbatim_row`. Carried links on Windows (SB-226): on the Windows runners `carry::windows_tests::a_shell_home_on_windows_carries_folders_as_junctions_and_files_as_hard_links`, `a_deleted_source_takes_its_hard_link_out_of_the_shell_home`, `a_home_file_a_session_replaced_is_kept`, `a_source_named_with_slashes_gets_a_junction_that_resolves`, `a_junction_names_local_folders_only`; on every platform, the decisions they rest on: `carry::tests::a_shell_homes_hard_link_is_replaced_or_removed_only_while_it_is_still_switchboards`, `the_hard_link_record_keeps_each_files_identity_and_reads_an_earlier_one`, `a_junction_target_is_written_as_windows_names_it_or_not_at_all`. Not yet: a person's Windows host (SB-02).
**Product:** unobserved
**Traces:** SB-183, SB-110, D-3, SCN-056, SCN-066, SCN-070, SCN-075

## SCN-075: Turn the terminal hook off or on

**Persona:** P-01
**Goal:** See whether Switchboard's shell hook is in the person's terminals and which start files it changed, and take it out (or put it back) in one action, from the window, the CLI or an agent, with every file given back byte for byte.
**Preconditions:** the app or the CLI installed. The window section is on the Groups page, under *Codex in group folders*.
**Entry point:** Groups → *Terminals in organization folders* (under *Codex in group folders*); CLI `switchboard shell status|on|off|init <shell>`; MCP `switchboard_shell_hook` (read, also under `--read-only`) and `switchboard_shell_hook_set` (set, when the operator asks; not offered under `--read-only`).
**Steps:**
1. Open Groups → the section reads its state: “On: new terminals in {organization}'s folders work in its context.” (on by default), with one line per start file (“zsh: ~/.zshrc, added”, “fish: its own file, added”) and *Turn off*.
2. *Turn off* → the choice `off` is saved, every block and the fish file are taken out (each file byte-identical to before), and the notice reads “The terminal hook is off. New terminals keep your own settings; terminals already open switch back at their next change of folder.”; the state reads “Off: new terminals keep your own settings.” with *Turn on*.
3. *Turn on* → the choice `on` is saved and the block is written for every shell present, with or without an organization → “The terminal hook is on. New terminals in organization folders work in their context.”
4. The same from a terminal: `switchboard shell off` / `switchboard shell on` / `switchboard shell status`; an agent reads it with `switchboard_shell_hook` and sets it with `switchboard_shell_hook_set` only when the operator asks (dashboard parity); from another data folder the set is refused and nothing changes.
5. `switchboard uninstall` → the blocks come out before the data folder goes; nothing outside the markers changes.
**Alt paths:** no group bound to an organization and no choice made → “Off: no group belongs to an organization on this computer.” with *Turn on*; the hook is wanted but no start file holds it (every file skipped, or no shell present) → “On, but no start file holds the hook yet, so terminals do not follow organization folders.”, each file's line says why, and *Turn off* still saves the choice; a start file that is a symbolic link → “{file} is a link, so Switchboard did not change it. Add this line yourself:” with the `eval "$(switchboard shell init <shell>)"` line (fish: `switchboard shell init fish | source`) and *Copy* (`skipped_symlink`); no `.zshrc` and zsh may read another folder → “zsh may read its start file from another folder (ZDOTDIR), so Switchboard created none. Add this line to the start file zsh reads:” with the line and *Copy* (`skipped_zdotdir_unknown`); a file another user owns → “{file} belongs to another user, so Switchboard did not change it.” (`skipped_owner`); a file that is not readable or not UTF-8 → “{file} could not be read, so Switchboard did not change it.” (`unreadable`), nothing written; the person deleted the block by hand → “You took the block out of {file}, so Switchboard does not add it back.” (`removed_by_person`) with *Add the block again* beside *Turn off*, which writes it back and saves the choice `on` as *Turn on* does, said in the line above them (“Adding the block again saves your choice to keep the hook on, as Turn on does.”); the folder map written by another version or unreadable → one line under the state says no folder applies; a file that changed while it was being written → left as it is and tried again at the next owner start; a development build, `serve` or a test never writes the person's files and the section says “Only the installed app changes terminal settings.”; on Windows, an execution policy of `Restricted` or `AllSigned` → “PowerShell's execution policy stops profiles from loading, so the hook is inactive there. Switchboard does not change it. Whoever manages this computer can allow signed or local scripts for your user.” (`policy_blocked`; the record names no policy, so the line does not either; the policy is read only while writing, never changed, A-Q4); PowerShell held to Constrained Language Mode → “PowerShell runs in Constrained Language Mode on this computer, so the hook is inactive there. Switchboard does not change this mode. Whoever manages this computer sets it.” (`constrained_language`, SB-225); **the Windows states are not yet verified on a person's Windows host (SB-02).**
**Expected result:** the person can see and choose, in one action anywhere, whether their terminals follow organization folders; `off` is final until they turn it on, and no file keeps a trace of Switchboard after `off` or uninstall.
**UI elements:** section heading, explanation, state line, one line per start file with its state, *Turn off* / *Turn on*, *Copy* beside an `init` line, notice; CLI and MCP answers.
**States covered:** reading, on by default (organization present), default with no organization, on, off, on but inactive, per file: added, link skipped, ZDOTDIR skipped, owner skipped, unreadable, removed by the person, policy blocked (Windows), Constrained Language Mode (Windows); folder map unreadable or of another version; development build; read failed; save failed.
**Errors & recovery:** the read fails → “The terminal hook's state could not be read.” with the reason and *Retry*; a save that fails keeps the previous state and names the cause; a file that could not be written leaves the others written and shows its own state; turning off always succeeds for files Switchboard wrote and never touches a line outside its markers.
**Status:** draft
**Meaning:** SB-183, decision D-3 of 2026-10-25 (off by one command); [dossier A](../runs/2026-10-26-iteration-0.7.0/dossier-a-context-binding.md) part A "On by default, safely"; A-Q3 and A-Q4 in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md).
**Copy:** the window's strings are checked with Copylot (en-US report c4e52902-fc1c-47dd-85d4-2d03c41b4fd1, ru-RU report 8feda489-b1dd-43ec-a99c-2fa9688b70ff; node A-7; `constrained_language`'s line, SB-225: en-US report 37280da0-038a-4bed-a59d-3d0f825c90d6, ru-RU report 2fea4a78-151d-41d3-9514-4f112249274d, no findings); Russian calls the feature «привязка терминалов» and a start file «файл запуска» ([terminology](../brand/terminology.md)).
**Coverage:** partial (code tests and a WebKit demo walk; no walk in the installed app yet). The window section, every state (node A-7): ui-logic `the_shell_hook_section_reads_every_state`; `lib.rs` `lifecycle_tests::the_windows_switch_turns_the_hook_off_and_on_through_the_owner` (off gives `.zshrc` back byte for byte, a background pass does not write it back, on writes it again, an owner that is not the installed app refuses); WebKit demo `?demo=1&shell=<state>` at 740 px and 1280 px, light and dark, English and Russian ([evidence](../runs/2026-10-26-iteration-0.7.0/evidence/a7/)). Step 4 over MCP: `crates/switchboard-cli/tests/mcp.rs` `handshake_lists_tools_and_rejects_malformed_messages` (read, refused set from another data folder, `--read-only` without the set) and the parity table `mcp_parity` in `crates/switchboard-cli/src/main.rs`. Steps 2 and 3 without the window, step 4 and step 5, and the alt paths link, another user's file, not readable, removed by the person, changed while written, development build (node A-3): `crates/switchboard-runtime/src/shell/install.rs` tests `the_block_goes_in_and_comes_out_leaving_the_file_byte_identical`, `a_symlinked_foreign_owned_or_changed_rc_is_never_written`, `a_block_the_person_removed_is_not_written_back`, `default_on_only_on_an_installed_build_with_an_organization_group_and_off_wins`, `a_block_with_a_changed_marker_is_the_persons_and_never_touched`, `remove_all_takes_every_block_out_whatever_the_choice`; `lib.rs` `owner_tests::no_test_or_dev_build_touches_the_real_home`, `owner_tests::an_installed_app_over_another_data_folder_writes_no_start_file`; `uninstall::tests::apply_removes_owned_things_and_keeps_the_persons_files`; CLI `shell_on_and_off_write_and_take_out_the_block_in_the_given_home_only`. One rule for “on by default” (A-7 fix cycle 1): `lib.rs` `owner_tests::the_hook_reported_is_the_hook_written` (an organization's group whose only folder a personal group takes has no map row: no block is written and `default_off` is reported, by the window, MCP and onboarding's `hook` step alike; once the folder is its own, the block and `default_on` come together); the first-time notice only from the installed app's owner: `owner_tests::only_the_installed_owner_reads_or_acknowledges_the_notice`. `policy_blocked` and a linked PowerShell profile (node A-8): `install.rs` `a_blocked_policy_leaves_the_profile_alone_and_records_policy_blocked` (nothing written, the record and `shell status` read `policy_blocked`, a policy that later allows local scripts gets the block), `a_policy_that_turns_blocked_keeps_what_off_gives_back` (off during or after the block gives the person's bytes back and deletes the created profile), `windows::tests::a_restricted_or_allsigned_policy_reads_as_policy_blocked`; ui-logic `the_shell_hook_section_reads_every_state` (the PowerShell line `switchboard shell init pwsh | Out-String | Invoke-Expression`, and `constrained_language`'s line); `constrained_language`: `install.rs` `constrained_language_mode_writes_no_profile_and_says_so` (`__PSLockdownPolicy` alone holds the 5.1 profile only), WebKit demo `?demo=1&shell=constrained`. A NUL in a zsh, bash or fish start file is read as before SB-225 (only a PowerShell profile with one reads `unreadable`): `install.rs` `a_zsh_start_file_with_a_nul_keeps_todays_behaviour`. Not yet: the Windows host (SB-02).
**Product:** unobserved
**Traces:** SB-183, D-3, SCN-074, SCN-066

## SCN-076: Set up an organization's work on a new computer

**Persona:** P-01, as a member of an organization (a new employee, or a person adding a company to their own computer)
**Goal:** Get a computer ready for an organization's work by following one checklist that the window, the person's coding agent and the runbook all read the same way: the organization is known, its group exists, the person's own seat is in it and signed in, terminals in its folders work in its context, and one launch proved it (SB-202).
**Preconditions:** Switchboard installed; the person has, or will get, a seat of the organization at the provider (an invitation to their own address). For the window card, an organization exists on this computer (typed, SCN-069, or from PassionCode.ai, SCN-071).
**Entry point:** the window, after the usage question (SCN-073) and the tour (SCN-035): the card above Accounts and at the top of *Organizations*; the person's coding agent: `switchboard onboard`, MCP `switchboard_onboarding` (SCN-050); the runbook `docs/ORGANIZATION.md` (English and Russian, B-Q4).
**Steps:**
1. Connect PassionCode.ai on *Organizations* (SCN-071; the browser consent is the person's) or add the organization by hand → the organization appears; once the usage question and the tour are done, the card “Set up {organization} on this computer” shows above Accounts with five steps, each marked *Done*, *To do*, *Needs you* or *Optional*, and one action for the first step not done.
2. *Group for {organization}* is to do → *+ Group for {organization}* opens the group dialog (SCN-070) with the organization chosen and its seats already on this computer preselected; *Save group* → the step reads *Done*. Or, from a terminal, import the administrator's settings file: `switchboard settings import <file> --under <your code folder> --apply`.
3. *Your seat* needs you → *Sign in with your {organization} address* → the official sign-in starts in that group (SCN-003) → when it finishes, the account is in the group with the organization's badge (by its address's domain) and the step reads *Done* (SCN-077).
4. *Terminals* reads its state from the terminal hook (SCN-074, SCN-075): “Terminals in {organization}'s folders work in its context. To turn this off: switchboard shell off” (*Optional*: on by default; the line names the one command that turns it off).
5. *First launch* (*Optional*) → launch a session in a folder of the group → Activity shows it with the organization's badge (SCN-051).
6. Every step done or optional → the card goes; `switchboard onboard --status --json` answers `"complete": true`.
7. The empty Accounts page on a computer with an organization's group and no account leads with the seat instead of the general first-account line: “Sign in with your {organization} address. It goes to the group “{group}”.” with *Sign in with your {organization} address* as the page's one primary action (*+ Add account* stays beside it, quiet). While the card is on screen above, its own *Sign in with your {organization} address* is the screen's one primary action and the page's two buttons are quiet: the page's line sits below the fold at 900 px, the card's never does (design review B-8). A backup this computer can open still leads (SCN-001's restore), before the seat.
**Alt paths:** *Hide* on the card → it stays only at the top of *Organizations* until the checklist is complete (kept in this window's storage, `switchboard.orgSetupHidden`). The card's and the empty page's sign-in is Claude Code's official sign-in in the group; a Codex seat is added with *+ Add account* → *Sign in to another Codex account* and offered its group (SCN-077). Several organizations → one card per organization not set up yet, newest first. No organization on this computer → no card, today's empty Accounts page (SCN-001) and today's five `onboard` steps (SCN-050). PassionCode.ai connected but listing no organization for the person → the `organization` step reads *Optional* in `onboard` (“PassionCode.ai lists no organization for you yet.”) and the window shows no card. Two groups bound to the same organization → *Group for {organization}* reads *Needs you* and names both: “Keep one group for {organization} on this computer.”, with no action of its own; in `onboard` the person says which group stays, then the agent unbinds the others (`project edit --organization personal`). No group yet → in `onboard` the step is `needs_person` too: whether the administrator gave a settings file, and which folders the group takes, are the person's to say. The hook is on but no start file holds it yet (A-3's record) → the *Terminals* line says the setting, not that terminals already follow. The seat is signed in on another computer as well → mark it there or here as used on another machine (SCN-072). The coding agent path: `onboard` lists the five extra steps after the first five; `seat` is `needs_person` and the agent relays “Sign in with your {organization} address.” to the person. Windows: the same checklist; the runbook's Windows column names `switchboard.exe` beside the app (no `~/.local/bin` link), the PowerShell profile hook, a `--under` folder holding the person's checkouts, the Windows credential store and the notification-area icon; **that column reads “not yet verified on a Windows machine” until SB-02 runs (B-Q6).**
**Expected result:** a person with no prior setup ends with the organization's group holding their own signed-in seat and a launch in its context, having done only the browser consent and the sign-in themselves; the window, the agent and the runbook agree on what is left.
**UI elements:** the organization card (title, five step lines with their state, one action, *Hide*), the empty Accounts line and its primary action, the group dialog prefilled, the sign-in banner; `onboard` text and JSON; the runbook.
**States covered:** no organization (nothing shown), card waiting for the usage question and the tour, each step in each state, hidden, several organizations, two groups for one organization, complete (card gone), empty Accounts on an organization computer, read failed, Windows (unverified).
**Errors & recovery:** the checklist cannot be read → the card shows “The setup could not be read. Refresh to retry.” and keeps the last steps it showed; a sign-in that fails or is cancelled → SCN-003's recovery, the step stays *Needs you*; a group save refused because it holds another context's account → SCN-070's refusal, the dialog stays open; a refused settings import → the CLI names the fix and changes nothing.
**Status:** draft
**Meaning:** SB-202 (window first run and runbook) and the agent half of SB-125, decision D-6 of 2026-10-25; [dossier B](../runs/2026-10-26-iteration-0.7.0/dossier-b-organization-person.md) 3.1, 3.2, 3.5. Resolution recorded here: the dossier lists *Connect PassionCode.ai…* as the card's first action, but the card appears only once an organization exists, so connecting stays on *Organizations* (SCN-071) and in the runbook.
**Copy:** the window's strings passed Copylot `copylot_check` (register ui, PassionCode.ai site voice v2): en-US report `734ec3a4-8162-4ac1-b862-ffc18f65dc79`, ru-RU report `3912f66c-a8f9-41c2-b763-d52f1bbbb286`, no findings (B-8). The runbook is still draft copy (B-9).
**Coverage:** the window (node B-8): `scripts/test-ui-logic.mjs` cases “the organization card waits for the usage question and the tour”, “the empty Accounts page on an organization machine leads with the seat” and the guard “no organization, no card, today's empty state” (`orgSetups`, `orgSetupAction`, `orgSetupShown`, `accountsEmptyKey`); the Tauri `onboard` command `tests::the_window_checklist_reads_the_owner_and_names_what_failed` (`src-tauri/src/main.rs`); the fields the window words `onboarding::tests::the_organization_steps_carry_what_the_window_words`; browser demo `?demo=1&org=setup` (and `&group=none`) 2026-10-27, WebKit, 740 and 1100 px, light and dark, English and Russian ([design review](../runs/2026-10-26-iteration-0.7.0/evidence/b8/design-review.md)). The checklist (node B-3): `onboarding::tests::a_machine_without_organizations_keeps_the_five_steps`, `an_organization_machine_names_its_group_seat_and_hook`, `a_seat_that_ended_is_not_a_signed_in_seat`, `a_snapshot_from_another_version_still_gives_the_organization_steps`, `an_unread_sign_in_or_session_list_is_named_not_failed`, and `onboard_commands::every_command_the_checklist_names_exists` (`crates/switchboard-cli/src/main.rs`). NOT_RUN: the installed app's card on a real organization machine, the runbook (B-9) and the live run on SB-125 (B-10).
**Product:** unobserved
**Traces:** SB-202, SB-125, D-6, SCN-050, SCN-035, SCN-073, SCN-069, SCN-070, SCN-071, SCN-074, FLW-04

## SCN-077: A new seat of an organization lands in its group

**Persona:** P-01, as a member of an organization
**Goal:** When the person signs in to a seat of their organization, it ends up in the organization's group, where its folders will use it, and never in the default group where the ordinary Claude Code's automatic switching could spend it; Switchboard never moves an account without the person (SB-160, joining).
**Preconditions:** an organization with exactly one group bound to it on this computer (SCN-070); the person's seat at the provider.
**Entry point:** the organization card or the empty Accounts page (SCN-076); or any other sign-in or *Add to Switchboard* (SCN-003, SCN-018).
**Steps:**
1. *Sign in with your {organization} address* → the sign-in starts in the organization's group → on finishing, “{label} signed in. It is in group “{group}”.” and the card's seat step reads *Done*.
2. The person signs in from *+ Add account* instead (the default group), or keeps the account the CLI is signed in to → the account is attributed to the organization by its address (or by the person's word, SCN-069), and the notice adds *Move it to {group}*.
3. *Move it to {group}* → the account moves as in SCN-062 → “{label} moved to group “{group}”.”
4. An agent finishing the sign-in reads `organization_group: {pool, name}` in the `login finish` receipt (with `in_cli: true` when the ordinary CLI is signed in to that account, as in the `accounts capture` receipt), tells the person, and moves the account only when the person says so (`switchboard accounts move`). A repeated `login finish` reads the account where it is now: after the move it carries no offer.
**Alt paths:** the organization has no group, or two → no offer; the account is already in the organization's group, or is personal → no offer and no receipt field; the account is the one the ordinary Claude Code or Codex is signed in to → it cannot join a group, so the notice says “Switch Claude Code to another account first to move {label} to {group}.” instead of offering the move; a seat on an address outside the organization's domains is personal until the person places it (SCN-069) or until PassionCode.ai supplies the provider organization (a later release).
**Expected result:** an organization's seat is in its group after one action at most, and never moved without the person.
**UI elements:** sign-in banner, the notice with *Move it to {group}*, the move as in SCN-062; the receipt field in CLI and MCP.
**States covered:** signed in from the card (lands in the group), signed in elsewhere (offered), moved, offer not shown (no group, several groups, already there, personal, ordinary CLI's account), refused move.
**Errors & recovery:** the move is refused (a second copy of the same login in that group, the group deleted meanwhile) → SCN-062's messages, nothing moves; a failed sign-in → SCN-003.
**Status:** draft
**Meaning:** SB-160 (Switchboard's part of joining), [dossier B](../runs/2026-10-26-iteration-0.7.0/dossier-b-organization-person.md) 3.3; AGENTS.md and the MCP instructions keep account moves explicit.
**Copy:** every quoted string here is draft copy; it goes through copywriting and Copylot (en-US, ru-RU) in wave 4 before it ships.
**Coverage:** the receipt (node B-4): `a_new_seat_of_an_organization_outside_its_only_group_is_offered_it`, `a_new_seat_the_cli_is_signed_in_to_says_so_in_its_receipt`, `a_repeated_finish_reads_where_the_account_is_now` (`crates/switchboard-runtime/src/lib.rs`); `signInMove` and `organizationMoveNote` in `scripts/test-ui-logic.mjs` (no move offered for an `in_cli` account). NOT_RUN: the window walk-through and a live sign-in.
**Product:** unobserved
**Traces:** SB-160, SB-202, SCN-062, SCN-069, SCN-070, SCN-076, FLW-04

## SCN-078: A seat the organization removed reads as ended

**Persona:** P-01, as a member of an organization that removed their seat (they left, or the seat was reassigned)
**Goal:** When an organization takes a seat away at the provider, the person sees in plain words that the seat has ended and whom to ask, and Switchboard stops choosing it, instead of a repeating failed check (SB-160, leaving).
**Preconditions:** a saved account attributed to an organization; the provider rejects its credential.
**Entry point:** none: the background monitor finds it; Accounts and Groups show it.
**Steps:**
1. The provider rejects the seat: a Claude renewal answered `invalid_grant`, or a usage check whose 401 survives a fresh renewal; for Codex (never renewed while inactive) a 401 on two checks in a row, at least a minute apart → the account joins the set of accounts that need a sign-in, until its credential changes.
2. Accounts → its card carries the *Sign in again* mark and the *Sign in* button, its usage line reads “Sign in to check usage”, and under it “{label}'s sign-in has ended. If {organization} removed your seat, ask its administrator; to keep using it, sign in again.” (the generic “This account's sign-in has ended. Sign in to it again.” of a check is not repeated under it); automatic switching, fallback chains (SCN-082 shows the step skipped) and recovery never choose it.
3. A group bound to an organization whose accounts of that organization have all ended → its card reads “None of {group}'s accounts is signed in. Sessions in its folders are refused until one is.” (in the warning colour, as SB-170's line); an account of another context in the group does not count, since the group refuses it anyway.
4. *Sign in again* with a seat that still exists → the account is usable again; the line goes.
**Alt paths:** a personal account rejected the same way → today's text, “This account's sign-in has ended. Sign in to it again.”; a single Codex 401 → not a verdict: the card shows a failed check as today; a Codex 401 within 5 minutes of the token's local expiry, or on the account the ordinary Codex CLI is signed in to → not a verdict; the person presses *Check* on an ended account → the provider is asked once more, and any answer but a 401 lifts the verdict; a 403 → never a verdict (it can be a plan or region refusal); any other status → not a verdict; a group where none of the accounts belongs to its organization any more → SCN-070's line (SB-170) is shown instead of this one; with automatic switching on, a group's next requests go to another of its signed-in seats as for any account that needs a sign-in; a request already in flight on the ended seat gets the provider's answer, and nothing is replayed (only SB-98's 429 moves a request; B-Q3, assumed); Fabric Dashboards' recovery read sees the account as needing a sign-in.
**Expected result:** a removed seat is named as ended with the organization and the way out; nothing keeps trying to spend it.
**UI elements:** the account card's line and *Sign in again*; the group card's line.
**States covered:** ended by renewal, ended by usage (Claude), ended by two checks (Codex), single 401 (not ended), 403 (not ended), signed in again, all of a group's seats ended, personal account.
**Errors & recovery:** a check that cannot reach the provider is never a verdict; a sign-in again that fails → SCN-003's recovery; the log records `seat_ended {provider, by, organization}` with codes only, no label and no organization name.
**Status:** draft
**Meaning:** SB-160 (Switchboard's part of leaving), [dossier B](../runs/2026-10-26-iteration-0.7.0/dossier-b-organization-person.md) 3.4; B-Q3 assumed in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md). Whether Anthropic answers a removed Team seat with `invalid_grant` or a 401 is not measured; both are covered.
**Copy:** the two card lines are checked with Copylot (register ui, voice “PassionCode.ai site voice v2”): en-US report 553c66d0-8647-44fe-8d93-191b47ec9671, ru-RU report ec6cf99d-0f97-4ca8-a81f-b05a53e2270b (node B-6); Russian: «Вход в аккаунт {label} больше не действует. Если ваше место в {organization} убрали, обратитесь к администратору организации; чтобы продолжить работу с аккаунтом, войдите снова.» and «Ни в один аккаунт группы «{group}» не выполнен вход. Сессии в её папках получают отказ, пока вы не войдёте хотя бы в один.» (a seat is «место», [terminology](../brand/terminology.md)).
**Coverage:** partial (code tests and a WebKit demo; no walk in the installed app yet). Detection (node B-5): `monitor::tests::a_claude_seat_rejected_after_a_fresh_renewal_reads_sign_in_required`, `a_codex_seat_rejected_twice_reads_sign_in_required` and the guards beside them. Text (node B-6): ui-logic cases “an ended seat of an organization names it” and “a group whose seats all ended says so” (`seatEndedText`, `groupSeatsEndedText` in `src/ui-logic.ts`); WebKit demo `?demo=1&org=ended` at 740 and 1100 px, light and dark, English and Russian ([evidence](../runs/2026-10-26-iteration-0.7.0/evidence/b6/)).
**Product:** unobserved
**Traces:** SB-160, SB-170, SCN-070, SCN-052, SCN-082

## SCN-079: An organization the person left on PassionCode.ai stays here, marked

**Persona:** P-01, as a former member of an organization
**Goal:** When the person leaves an organization on PassionCode.ai, Switchboard says so on its card and keeps everything as it was, so the former employer's folders still refuse the person's own accounts and the person decides what to remove (SB-159 decided "keep on leaving").
**Preconditions:** PassionCode.ai connected (SCN-071); an organization that came from it.
**Entry point:** none: the next read (at start, or every 6 hours, or *Read now*); *Organizations* shows it.
**Steps:**
1. A read no longer lists the organization → `vesta.json` records its local id and the time of that read (never its name; a later read keeps the first date), the log says `organization_released {count}`, and its card loses the *PassionCode.ai* mark and reads, under its name and domains, “Not in your PassionCode.ai organizations since {date}. Its groups still refuse other accounts. Remove it when you no longer need it.”
2. The organization, its group's binding, its accounts and their statistics stay; nothing is deleted.
3. *Remove…* on the card (SCN-069) → the confirmation → the organization goes and its groups turn personal (SCN-070).
**Alt paths:** the person is added back → the next read links it again by name or domain and the line goes; the person removes it → its mark leaves `vesta.json` at the next read; PassionCode.ai unreachable → nothing changes, the last copy stays in force (SCN-071); *Disconnect* is not leaving: organizations stay local with no released line; an older build reading the connection file after a downgrade may read the connection as unreadable, and connecting again repairs it.
**Expected result:** leaving is visible, with its date, and costs nothing until the person removes it.
**UI elements:** the released line on the organization card, *Remove…*.
**States covered:** released, rejoined, read failed, removed by the person, disconnected.
**Errors & recovery:** a read that fails changes nothing; the log records `organization_released {count}` only.
**Status:** draft
**Meaning:** SB-160, [dossier B](../runs/2026-10-26-iteration-0.7.0/dossier-b-organization-person.md) 3.4 "Leaving PassionCode.ai"; amends SCN-071's alt path.
**Copy:** the released line is checked with Copylot (register ui, voice “PassionCode.ai site voice v2”): en-US report 2ec0173b-984d-402c-b77b-b413e524d161, ru-RU report d5a65746-bb0c-422b-a513-11d782f6ef59 (node B-7); Russian: «Этой организации нет среди ваших организаций в PassionCode.ai с {date}. Её группы по-прежнему не принимают другие аккаунты. Удалите её, когда она станет не нужна.»
**Coverage:** partial (code tests and a WebKit demo; no live PassionCode.ai read). Node B-7: `vesta::tests::an_organization_left_on_passioncode_is_marked_with_its_date` (released with its id and date, nothing removed, no name in the file, logged once, the first date kept, linked again clears it, removed by the person clears it); storage `organizations_from_passioncode_are_linked_kept_current_and_released` (unchanged, green); ui-logic case “an organization left on PassionCode.ai is marked with its date” (`organizationReleasedAt`); WebKit demo `?demo=1&org=ended` (Contoso) at 740 and 1100 px, light and dark, English and Russian ([evidence](../runs/2026-10-26-iteration-0.7.0/evidence/b7/)).
**Product:** unobserved
**Traces:** SB-160, SB-159, SCN-069, SCN-070, SCN-071

## SCN-080: See and set the fallback chain in the window

**Persona:** P-01
**Goal:** The operator sees and sets, in the window, which agents take over a task when its accounts run out, and in what order, without the CLI (SB-71; XA-01 "a Fallback section").
**Preconditions:** the app runs. At least one agent that can take over work is installed for a chain to be useful.
**Entry point:** Agents → *Fallback chains*, between *Installed* and *Available to install* (SCR-03).
**Steps:**
1. Open Agents → the panel's lead line reads “When a task's accounts run out, Switchboard hands it to the next agent in its chain. Nothing is handed over until a chain is set.”; the row *This computer* reads “Not set” with *Use Subscriptions first* and *Edit…*.
2. *Use Subscriptions first* → the row shows the chain in order, “Claude Code → Codex → Kimi Code → Hermes”, with *Edit…* and *Clear*; the notice reads “This computer's chain is set.”; readiness lines appear under it (SCN-082).
3. *Edit…* → the dialog “Fallback chain: This computer” lists the steps in order; each step has an agent (the five that can take over work: Claude Code, Codex, Kimi Code, Hermes, OpenCode), an account (“Any account with room” by default, or one of the saved accounts that agent may use), *Move up*, *Move down* and *Remove*; *+ Add agent* until there are 8.
4. *Save* → the whole list is saved → “This computer's chain is saved.”; the row shows the new order.
5. *Clear* → “Clear this computer's chain? Nothing is handed over while no chain applies.” → *Clear chain* → the row reads “Not set”.
**Alt paths:** a step set from the CLI naming another agent → kept, its agent shown as “{agent} (not automatic)” in the editor and its readiness line “Not automatic yet”; a step pinned to a key (`agent#project/env/NAME`) → shown read-only as “Pinned to key {name}: not automatic yet (waits for project grants)” and kept unless removed; the window never creates a key pin (SB-96); an agent named in the chain but not installed → “Not installed”; an agent disabled under *Installed* stays in the chain and reads as skipped (SCN-057); the account choice offers only accounts the core rules accept for that agent and the fallback can run (Claude Code only on enabled Claude accounts, Codex only on enabled Codex accounts, Kimi Code only on saved Kimi Code accounts; Hermes and OpenCode read “The OpenRouter key” and offer no account, since a pinned account would only make them wait, and another agent never runs on a subscription sign-in); *Save* with no step left → “Add an agent, or clear the chain instead.” and nothing is saved; *Move up* on the first step and *Move down* on the last are disabled, and after a move the focus stays on the moved step's button; every control is a button with an accessible name, no drag-only control; *Cancel* or Escape closes the dialog and keeps the saved chain.
**Expected result:** the chain the operator sees is the one the automatic fallback will walk (SCN-040), and it is set without a terminal.
**UI elements:** the *Fallback chains* panel, the *This computer* row with its chips, *Use Subscriptions first*, *Edit…*, *Clear*, the editor dialog (step rows with agent and account selects, *Move up*, *Move down*, *Remove*, *+ Add agent*, *Save*, *Cancel*), the clear confirmation, notices.
**States covered:** reading, not set, set, editing, saving, saved, refused, cleared, read failed, eight steps (no more added), a step not automatic, a key pin, an agent not installed.
**Errors & recovery:** the read fails → “Chains are unavailable. Refresh to retry.” and the last known chains stay, marked as possibly out of date; a save refused by the core (for example an agent on a subscription sign-in, or an account removed meanwhile) → its own text in the dialog, which stays open, and nothing is saved; a save that fails → the previous chain stays and the cause is named.
**Status:** draft
**Meaning:** SB-71 remainder (the screen), decision D-5 of 2026-10-25; [dossier C](../runs/2026-10-26-iteration-0.7.0/dossier-c-chains-and-ceilings.md) part B. The CLI and MCP of SCN-039 stay as they are.
**Copy:** checked with Copylot (register ui, voice “PassionCode.ai site voice v2”) in node C-3: en-US `bb2eb9c8-ec0f-4327-b138-3af309e1fd95`, ru-RU `2a00da56-1a9a-4c8b-b73b-b3d2c42b7528` (pass, 100); the editor's “{agent} (not automatic)” en-US `1e72bf52-baeb-4130-8195-e33d1679282b`, ru-RU `7932468a-5a28-4af9-912b-244deb360cd9`.
**Coverage:** `test-ui-logic.mjs` C-3 cases (`moveStep`, `accountsForStep`, `chainSummary`); design review in WebKit on the demo (`?demo=1&chains=none|reasons|failed|stale|skipped|launch_failed`), [evidence/c3](../runs/2026-10-26-iteration-0.7.0/evidence/c3/design-review.md); the read and the write through the owner: C-2 (`Operation::Chains`, `SetChain`).
**Product:** unobserved
**Traces:** SB-71, D-5, SCN-039, SCN-040, SCN-057, SCR-03

## SCN-081: Give a group its own fallback chain

**Persona:** P-01
**Goal:** A group's tasks fall back along the group's own order (its own agents and accounts), while the rest of the computer keeps this computer's chain; tasks worked in a group's folder get the group's chain whoever's account started them.
**Preconditions:** at least one group (SCN-068).
**Entry point:** Agents → *Fallback chains* → a group's row; Groups → a group card's *Fallback* line, which leads to that row.
**Steps:**
1. The panel lists one row per group under *This computer*: “Uses this computer's chain” with *Set for this group…*, or the group's own chain with *Edit…* and *Clear*.
2. *Set for this group…* → the editor of SCN-080 titled “Fallback chain: {group}” → *Save* → “The chain of {group} is saved.”; the Groups card reads “Fallback: own chain”.
3. *Clear* on the group → “Clear the chain of {group}? Its tasks use this computer's chain again.” → *Clear chain* → “The chain of {group} is cleared.”; the row and the Groups card read “Uses this computer's chain” / “Fallback: this computer's chain”.
4. Rows per task (chains an agent or the CLI set for one workflow) show “Task wf_” and the id's first eight digits, and its chain, with *Edit…* and *Clear* (“Clear the chain of task {id}? It uses its group's or this computer's chain again.”); there is no form to add one in the window.
5. A task worked in a folder of the group runs out of accounts → the group's chain is the one walked, even when the task ran on an account outside the group (the ordinary Claude Code's account, for example plain `claude` there).
**Alt paths:** no groups → only the *This computer* row and the line “A group gets its own chain once you create one under Groups.”; neither the group nor this computer has a chain → the group's row reads “Uses this computer's chain, which is not set” and its card “Fallback: not set”; the card's line leads to the group's *Set for this group…* or *Edit…* on Agents, focused; a group deleted → its chain goes with it; a task's checkout in several groups, none of them the executor account's → the group its folder's proxy route and paid key use applies and the log says `group_ambiguous` (`switchboard chain list --workflow <id> --dir <checkout>` shows which chain that is); a step pinned to an account another group keeps for itself → saved, and its readiness reads skipped (SCN-082).
**Expected result:** each group's tasks follow the order set for that group, chosen by the folder the task works in.
**UI elements:** group rows with *Set for this group…*, *Edit…*, *Clear*; task rows; the Groups card's *Fallback* line as a link; the editor and confirmation of SCN-080.
**States covered:** no groups, group on this computer's chain, group with its own chain, cleared, task rows, ambiguous folder, read failed.
**Errors & recovery:** as SCN-080: refusals in the dialog, nothing saved; a failed read keeps the last rows.
**Status:** draft
**Meaning:** SB-71, D-5 of 2026-10-25; [dossier C](../runs/2026-10-26-iteration-0.7.0/dossier-c-chains-and-ceilings.md) parts B and E (the group's chain by the checkout).
**Copy:** checked with Copylot in node C-3 (the reports of SCN-080).
**Coverage:** the window rows, the Groups line and its link: design review in WebKit (C-3, [evidence/c3](../runs/2026-10-26-iteration-0.7.0/evidence/c3/design-review.md)); the group's chain by checkout: C-8 (`a_groups_chain_applies_to_a_workflow_in_its_folder_on_the_ordinary_cli`).
**Product:** unobserved
**Traces:** SB-71, D-5, SCN-039, SCN-040, SCN-068, SCN-080, SCR-03

## SCN-082: See whether a chain can take work now

**Persona:** P-01
**Goal:** Before a limit is hit, see whether each step of a chain could take a task right now and why not, and after a hand-over see what happened, so a chain that would silently skip everything is noticed (SB-73 status).
**Preconditions:** at least one chain set (SCN-080, SCN-081).
**Entry point:** Agents → *Fallback chains*; CLI `switchboard chain list` (one readiness line under each chain); MCP `switchboard_chain_get` (`readiness`, `last_handover`).
**Steps:**
1. Under each chain row, one short line per step, the agent's name then: “Ready: {account}”, for a paid step “Ready: the key of {group}. Spend not read yet.” / “Ready: this computer's key. Spend not read yet.” / “Ready: {API-key account}. Spend not read yet.”, “Skipped: no account with room”, “Skipped: disabled under Installed”, “Not automatic yet”, “Pinned to key {name}: not automatic yet (waits for project grants)”, “Not installed”, “Skipped: today's ceiling reached”, “Skipped: less than $0.50 left on the key”, “Skipped: spend could not be read”, “Skipped: agent settings could not be read”, or “Skipped: needs a sign-in” (an ended seat, SCN-078); a code the window does not know reads “Skipped ({code})”.
2. Under the panel: “Last hand-over: {time}, task {id}: {outcome}.” (handed over; the launch failed after the offer; every step was refused; no agent could take it; or the gate's reason, for example today's ceiling reached) and “Passed over: {agent}: {reason}; …”; none yet → “No hand-over yet.”
3. A hand-over is offered → one system notification: “{agent} took over a task in {folder}.”
**Alt paths:** a paid step → “Spend not read yet”: opening the panel never starts a hand-over, the workflow engine or a network call, and the chains view carries no spend read (`spend: null`, C-2), so the window shows no amount (a remaining amount with its age waits until the view carries one); spending ceilings unreadable → the panel says so above the rows (the core's sentence); the owner does not know the account limits (`limits_known: false`) → “Account limits are not known right now, so a step may read ready when its accounts have no room.”; a group's readiness is read for its first folder, so a folder the group's rules refuse reads skipped; no chain set → no readiness lines; a `switchboard continue` by hand is not a hand-over: it passes the same spend gate (C-Q8) but leaves the last hand-over line as it was; Activity keeps its *Hand-over offered* or *failed* entries as before.
**Expected result:** the operator knows, at a glance, which agent would take the next task and why the others would not.
**UI elements:** readiness lines under each row, the last hand-over line, the system notification; the CLI line and MCP fields.
**States covered:** ready (account, key), each skip reason, not automatic, not installed, spend not read yet, last hand-over offered, failed, every step skipped, none yet, read failed.
**Errors & recovery:** a failed read keeps the last lines under the panel's “Chains are unavailable. Refresh to retry.” (SCN-080); readiness never acts, so nothing is retried from it.
**Status:** draft
**Meaning:** SB-73 status remainder, SB-71; [dossier C](../runs/2026-10-26-iteration-0.7.0/dossier-c-chains-and-ceilings.md) part A and D (notification).
**Copy:** checked with Copylot in node C-3 (the reports of SCN-080); the notification's copy is node C-7's.
**Coverage:** the last hand-over record (C-5): `fallback::tests::the_last_hand_over_is_recorded_with_its_skipped_steps`; the readiness view (C-2): `chain_view::tests`; its lines and the last hand-over line (C-3): `test-ui-logic.mjs` (`stepReasonText` per state and reason, `handoverOutcomeText`, `handoverSkipped`) and the design review in WebKit ([evidence/c3](../runs/2026-10-26-iteration-0.7.0/evidence/c3/design-review.md)); the notification: NOT_RUN (node C-7).
**Product:** unobserved
**Traces:** SB-73, SB-71, SCN-040, SCN-080, SCN-081, SCR-03, FLW-05

## SCN-083: Cap what paid agents may spend a day

**Persona:** P-01
**Goal:** Put a daily dollar ceiling on what paid fallback agents may start spending, for this computer and for a group, and a ceiling for one task, checked before every paid hand-over, beside the key's own provider limit (SB-72, decision D-5 of 2026-10-25; authority rule: a ceiling is a question, not a wall).
**Preconditions:** at least one saved OpenRouter key (SCN-045, SCN-065); for a group ceiling, the group's own key.
**Entry point:** API keys → the *Machine key · OpenRouter* panel and each row of *Group keys*; CLI `switchboard ceiling list|set|keep` and `switchboard continue --ceiling <usd>` (clearing, like raising, is the window's); MCP `switchboard_ceilings` (read), `switchboard_ceiling_set` (write, when the operator asks).
**Steps:**
1. Open API keys → the machine key panel reads “Daily ceiling: not set” with *Set ceiling…*, beside “Key's own limit: ${limit}, resets daily” read from OpenRouter (measured 2026-10-26 on this computer: one machine key, limit $10, daily reset, C-Q5).
2. *Set ceiling…* → a dialog: amount in dollars per UTC day, and “Counts every OpenRouter key saved in Switchboard, added up per UTC day.” → *Save* → “Daily ceiling for this computer: ${usd}.”
3. On a group with its own key → *Set ceiling…* → the same dialog for that group's key → “Daily ceiling for {group}: ${usd}.”
4. A task ceiling: `switchboard continue --ceiling <usd>`, or `switchboard ceiling set --scope workflow:<id>`, counted on the key since the task's first paid hand-over and checked only at a switch; the window lists it read-only with the label “Counted on the key since the first paid hand-over; other work on the same key counts too.”
5. At every paid hand-over (Hermes or OpenCode on an OpenRouter key; Claude Code or Codex on an API-key account while a ceiling covers them) Switchboard reads the key's spend first: less than $0.50 left on the key's own limit → skipped; a ceiling reached → skipped and the question of SCN-084 opens; spend unreadable while a ceiling covers the step → skipped; otherwise the step runs (SCN-040). Subscriptions and Kimi Code on a saved Kimi account are never gated. A paid start by hand (`switchboard continue` onto Hermes, OpenCode or an API-key account, also to the console and on the default agent; launching an agent on the OpenRouter key or through the proxy on an API-key account; launching an API-key account) passes the same gate and is refused with its reason before anything is offered or started (C-Q8).
6. *Clear* → “Remove {scope}'s daily ceiling? Paid agents are then bound only by the key's own limit.” → *Remove ceiling* → “Daily ceiling: not set”.
**Alt paths:** a group on the machine key → *Set ceiling…* is unavailable with “Give this group its own key under API keys first.” (CLI and MCP refuse with the same words); the key's own limit is monthly, or higher than the group's daily ceiling → “A running agent can spend up to the key's own limit; Switchboard's ceiling only decides whether it is started.”; the group's key removed → its ceiling reads “Inactive: this group has no key of its own now.” and is never silently deleted; an API-key Claude Code or Codex step under a ceiling → its spend cannot be read, so it is always skipped as spend unknown, and readiness says so (C-Q3, assumed); no ceiling covers a step and its balance cannot be read → it runs, the provider enforces the key's own limit; the machine ceiling also counts spend on the same keys from launches outside the fallback, which errs toward stopping; Switchboard never changes a key's own limit at the provider (C-Q4, assumed); values outside $0.01 to $1,000 → “Enter an amount from $0.01 to $1,000.”; a second ceiling for the same scope replaces the first; from the CLI or MCP → a first ceiling of a scope or a lower value is accepted, a higher value or *clear* is refused with “Raising or clearing a ceiling is done in the Switchboard window. From here, set a first ceiling or a lower one.” (C-Q1, so an agent never loosens a ceiling); deleting a group that holds a ceiling from the CLI or MCP → refused with “This group has a spending ceiling. Delete the group in the Switchboard window, where its ceiling goes with it.” (C-Q7), and in the window the group's ceiling, questions and task counts go with it, so a group saved again under the same id starts with none; removing or replacing a capped group's own key from the CLI or MCP → refused with “This group has a spending ceiling. Remove or replace its own key in the Switchboard window.” (C-Q9); removing or replacing any key while this computer's ceiling is set (it adds every key up, the machine's key included), or the key a task ceiling counts from, from the CLI or MCP → refused with “A spending ceiling counts this key's spend. Remove or replace the key in the Switchboard window.” (C-Q9); moving a folder out of a capped group, adding it to another group or disabling the group's accounts stays allowed, and a paid start there then counts against this computer's ceiling when one is set, and against no ceiling otherwise (C-N1); a task's first paid hand-over ran while its key's total could not be read → a task ceiling set later reads its spend as unknown, never as zero; 192 task ceilings already set → “At most 192 task ceilings. Clear one in the Switchboard window first.” (a task ceiling stays until the window clears it), 63 group ceilings → “At most 63 group ceilings. Clear one in the Switchboard window first.”, and this computer's ceiling always has room.
**Expected result:** no paid agent is started past a ceiling the operator set, and the operator sees both limits, Switchboard's and the key's own, side by side.
**UI elements:** the ceiling line and *Set ceiling…* on the machine key panel and on each group key row, the key's own limit and its reset, the warning line, the ceiling dialog (amount, explanation, *Save*, *Cancel*), *Clear* with its confirmation, the inactive line, the read-only task ceiling rows; CLI and MCP answers.
**States covered:** reading, no key, not set, set (machine, group, task), inactive, warning (monthly or higher key limit), refused (no own key, out of range), saved, cleared, read failed, ceilings unreadable.
**Errors & recovery:** a save that fails keeps the previous value and names the cause; the ceilings file cannot be read → “Spending ceilings could not be read, so no paid agent is handed work until they are.” with the order of recovery: delete spend.json first, then restore a backup (a restore never writes over an existing file), or leave it deleted to start over; OpenRouter does not answer at a switch → the covered step is skipped as spend unknown and tried again at the next pass.
**Status:** draft
**Meaning:** SB-72, decision D-5 of 2026-10-25; [dossier C](../runs/2026-10-26-iteration-0.7.0/dossier-c-chains-and-ceilings.md) parts C and D; C-Q2 (task measure, labelled), C-Q3, C-Q4 assumed and C-Q5 closed in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md); analytics: counts only, `chains` and `ceilings` in `app_active`, no new event (C-Q6, assumed). Terms proposed for the brand pack: spending ceiling «потолок расходов», daily ceiling «дневной потолок», the key's own limit «собственный лимит ключа».
**Copy:** checked with Copylot in node C-7 (register ui, voice “PassionCode.ai site voice v2”): en-US `2a756d70-a44e-4f39-adf1-67c8a8628074` (pass, 100), ru-RU `7cf97fda-11ec-4d8c-8859-4954c3f0e1e7` (pass, 100; advisory notes only: the window's `$5.00` money form is kept). The window's exact words are in `src/ui-logic.ts`, `src/main.ts` and `src/locales/ru.ts`; a quoted string here that differs is the earlier draft.
**Coverage:** step 5, the gate (C-5): `fallback::tests` `a_hermes_step_on_a_key_with_nothing_left_is_skipped_not_offered`, `a_reached_group_ceiling_skips_the_paid_step_and_opens_one_question`, `an_unread_balance_with_a_ceiling_is_a_denial_and_without_one_runs`, `a_subscription_step_is_never_gated`, `a_key_whose_read_times_out_is_unknown_under_a_ceiling_and_runs_without_one`, `a_key_whose_total_went_down_is_never_counted_as_less_spend`, `two_hand_overs_racing_for_the_last_dollar_are_decided_one_after_the_other`, `a_ceiling_set_while_the_spend_is_read_is_obeyed`, `an_api_key_claude_step_under_a_ceiling_is_unknown_and_reads_nothing`, `a_restart_between_the_decision_and_the_offer_keeps_the_tasks_baseline`, `a_first_paid_hand_over_with_its_total_unread_leaves_a_later_task_ceiling_unknown`; starts by hand (C-Q8): `a_paid_continue_by_hand_past_a_reached_machine_ceiling_is_refused_and_offers_nothing`, `a_paid_continue_by_hand_writes_the_tasks_baseline`, `an_agent_launched_on_the_openrouter_key_past_a_ceiling_is_refused`; a capped group's key (C-Q9): `control::tests::a_capped_groups_own_key_is_removed_or_replaced_only_in_the_window`; storage: `spend::tests` (C-4); CLI and MCP (C-6): `ceilings_are_set_lowered_listed_and_kept_from_the_cli_and_never_raised`, `ceiling_answer_double_over_mcp_is_refused_with_the_window_named`; the window (C-7): `test-ui-logic.mjs` (`ceilingLine`, `keyLimitLine`, `ceilingWarning` for a monthly, weekly, never-resetting, higher or absent key limit), `tests::the_windows_ceiling_commands_speak_as_the_window` (src-tauri), design review in WebKit, 740 and 1100 px, both themes, English and Russian: [evidence/c7](../runs/2026-10-26-iteration-0.7.0/evidence/c7/design-review.md).
**Product:** unobserved
**Traces:** SB-72, D-5, SCN-040, SCN-045, SCN-065, SCN-084, FLW-05

## SCN-084: A ceiling is reached: double it or keep it

**Persona:** P-01, the operator, in their own window
**Goal:** When a paid step reaches a ceiling, the operator is asked one question, “double it or keep it”, with no option recommended; only the operator in the window may double, anyone may keep, and a doubled ceiling asks again when it is reached (authority rule on ceilings; C-Q1).
**Preconditions:** a ceiling set (SCN-083); a paid step reached it at a hand-over.
**Entry point:** a system notification; a banner on Agents and on API keys; CLI `switchboard ceiling keep`; MCP `switchboard_ceiling_answer` (keep only).
**Steps:**
1. A paid step reaches the ceiling → the step is skipped and the chain moves on; one question opens for that scope and value; one system notification: “Daily ceiling ${usd} for {scope} reached. Double it or keep it in Switchboard.” (nothing to answer in the notification itself).
2. Open the window → a banner on Agents and on API keys: “{scope}'s daily ceiling of ${usd} is reached.” with two buttons of equal weight, *Double to ${2 × usd}* and *Keep ${usd}*: neither is primary, neither is preselected, none is recommended.
3. *Double to ${2 × usd}* → the ceiling becomes twice the value, the question closes, and the task may be tried again at the next pass → “Daily ceiling for {scope} is now ${2 × usd}.” Reaching the new value asks again.
4. *Keep ${usd}* → the question closes and the value stays → “Daily ceiling for {scope} stays ${usd}. Paid agents wait until it resets.”
**Alt paths:** *double* from the CLI or MCP → refused with “Doubling a ceiling is answered in the Switchboard window.”; a paid start by hand past the reached ceiling (`switchboard continue` onto a paid agent, an agent launched on a paid key) → refused with “A spending ceiling is reached, so no paid agent starts. Its question waits in the Switchboard window.” and the same question stays the one open (C-Q8); *keep* from the CLI or MCP → accepted; raising the ceiling another way from the CLI or MCP (a higher `set`, or *clear*) → refused with “Raising or clearing a ceiling is done in the Switchboard window. From here, set a first ceiling or a lower one.”, the file unchanged, and a request that claims to be the window but comes through the CLI's or MCP's connection is refused before it runs (C-Q1); several questions open → one banner per question, oldest first; a question whose group was deleted, or whose ceiling was set again or cleared → it goes with it; an open question about a task → kept until answered or 7 days after it was asked; 64 questions already kept → the oldest answered one makes room, then the oldest open task question, and a question about this computer or a group always gets a place; a task ceiling asks the same question for the task; a new UTC day under a daily ceiling → paid steps may run again, and an open question that no longer applies stays answerable until answered or until the day after; answered questions are kept 30 days.
**Expected result:** no ceiling rises without the operator's own answer in the window, and every rise is tied to the value it doubled.
**UI elements:** system notification; the banner with two equal buttons on Agents and API keys; notices; CLI and MCP answers.
**States covered:** open question, doubled, kept, double refused outside the window, several open, removed scope, new day.
**Errors & recovery:** a save that fails leaves the question open and names the cause; the ceilings file cannot be read → no paid hand-over (SCN-083) and the banner says it could not read the question.
**Status:** draft
**Meaning:** SB-72, D-5 of 2026-10-25; fabric-workspace `knowledge/authority.md` ceiling question (cited in [dossier C](../runs/2026-10-26-iteration-0.7.0/dossier-c-chains-and-ceilings.md) section 2); C-Q1 assumed in [questions](../runs/2026-10-26-iteration-0.7.0/questions.md): the window only.
**Copy:** checked with Copylot in node C-7 (register ui, voice “PassionCode.ai site voice v2”): en-US `2a756d70-a44e-4f39-adf1-67c8a8628074` (pass, 100), ru-RU `7cf97fda-11ec-4d8c-8859-4954c3f0e1e7` (pass, 100; advisory notes only: the window's `$5.00` money form is kept). The window's exact words are in `src/ui-logic.ts`, `src/main.ts` and `src/locales/ru.ts`; a quoted string here that differs is the earlier draft.
**Coverage:** questions and answers (C-4): `spend::tests::a_doubled_ceiling_is_tied_to_its_value_and_the_next_hit_asks_again`, `double_is_refused_from_mcp_and_cli`; the question opened at a hand-over (C-5): `fallback::tests::a_reached_group_ceiling_skips_the_paid_step_and_opens_one_question`; keep from the CLI and MCP, double refused there (C-6): `ceilings_are_set_lowered_listed_and_kept_from_the_cli_and_never_raised`, `ceiling_answer_double_over_mcp_is_refused_with_the_window_named`; the banner and the notification (C-7): `test-ui-logic.mjs` (`ceilingQuestion`: two answers with one class, no primary, no `recommended` flag, double unavailable past $1,000), `suggestions::tests::a_reached_ceiling_is_announced_once_per_question`, `the_spend_stamp_moves_with_the_file`, design review in WebKit: [evidence/c7](../runs/2026-10-26-iteration-0.7.0/evidence/c7/design-review.md).
**Product:** unobserved
**Traces:** SB-72, D-5, SCN-083, SCN-040, FLW-05
