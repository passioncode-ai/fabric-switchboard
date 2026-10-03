# Switchboard 0.5 — prompt-free switching, reliable rotation, one-click accounts

Run started 2026-10-02 from `main` `5bac51c`. Operator request (Russian, paraphrased): switching
accounts keeps asking to confirm Keychain access and other dialogs that Claude Swap does not show;
make the automatic switcher work well; connect the active console account with one button and no
extra windows; make the account list compact and split into groups; recheck the main features and
fix every bug found. Route: `task-pipeline` (this file is its brief, spec and plan).

## Operator decisions (grill, 2026-10-02)

| # | Question | Answer |
|---|---|---|
| D-1 | How does Switchboard's own vault stop prompting? | **Superseded before release (2026-10-02, integration):** a parallel run had already shipped 0.4.1-beta.1 with shared-trust Security.framework items ([#12](https://github.com/passioncode-ai/fabric-switchboard/pull/12), [KEYCHAIN.md](KEYCHAIN.md)), installed and accepted on the operator's Mac (11 accounts moved, no `SecurityAgent`; board SB-12). Moving the operator's accounts a second time had no benefit, so the file vault below was withdrawn and 0.5 keeps KEYCHAIN.md's design. Original answer: like Claude Swap: credential files encrypted with AES-256-GCM, one 256-bit key in the login Keychain created and read only through `/usr/bin/security`. Same exposure as Claude Code's own credential item. Legacy per-account items migrate once. |
| D-2 | May Switchboard refresh OAuth tokens? | Yes, for **inactive** Claude accounts only (not the identity signed in to the ordinary Claude Code), when the access token has expired or is about to, or the usage endpoint answers 401. This replaces the 0.3 rule "no competing refresh grant": the active account stays owned by Claude Code. |
| D-3 | Primary action on a Claude OAuth row | **Switch Claude Code**: one click, no confirmation dialog. Managed-route selection moves to the row menu. Codex and non-OAuth rows keep **Select** (managed route) as the primary action. |
| D-5 | Automatic backup of accounts (operator, 2026-10-02, second request) | **No passphrase**: the key stays in this machine's Keychain (macOS, through `/usr/bin/security`) or DPAPI (Windows). A backup restores after reinstalling Switchboard on the same machine, not on another machine or after the Keychain is erased — said in the UI. Folder `~/Library/Application Support/Fabric Switchboard Backups` (no privacy consent, unlike Documents; outside the app's data folder), newest ten of this store kept and its most complete always; only the default data folder's owner writes. |
| D-6 | Switch on a provider limit error while quota still shows capacity | Yes. Read only Claude Code's API-error markers (`isApiErrorMessage` + `rate_limit`) from its session transcripts, never conversation content; managed sessions use the proxy's own `request rate_limited` events. Claude Swap does not do this. |
| D-4 | Design surface | In code on the vendored PassionCode tokens, verified in a browser on demo data. Figma stays off (`docs/ux/foundation.md`). |

## Root causes found before the design (stage 0 harvest)

| # | Prompt or failure | Cause (file at `5bac51c`) |
|---|---|---|
| C-1 (fixed by 0.4.1, [KEYCHAIN.md](KEYCHAIN.md)) | macOS asks "Switchboard wants to use … in your keychain" for every stored account, again after each rebuild, and separately for the CLI/MCP server | `crates/switchboard-core/src/vault.rs` stores one Keychain item per account through Security.framework; the item ACL names the creating binary, and the app, the CLI and every rebuilt binary are different requesters. The monitor reads every vault item every 10 s (`rotation_credential_eligible`), so a missing grant re-prompts constantly |
| C-2 | Prompt when switching Claude Code | `external.rs` `auth_write` updates `Claude Code-credentials` through Security.framework from Switchboard's binary. Claude Code creates and reads that item with `/usr/bin/security`; a write from another binary needs consent, and an item Switchboard *creates* (after `claude logout` or a rollback delete) makes Claude Code itself prompt on its next read |
| C-3 | Prompt when finishing or cancelling an official sign-in | `launch.rs` reads and deletes the isolated home's `Claude Code-credentials-<hash>` item through Security.framework; Claude Code created it with `/usr/bin/security` |
| C-4 | Automatic rotation stops choosing accounts after ~8 h | inactive Claude access tokens expire; `probe_usage` and `checked_credential` refuse expired tokens, so every inactive account turns ineligible and rotation holds `no_eligible_account` |
| C-5 | Switching to an account whose access token expired fails | `replace_native` reads the target through `Store::credential`, which refuses an expired token, although Claude Code would refresh it |
| C-6 | Switching away can strand a stale refresh token | `replace_native` preserves the live generation only into the copy in the *target's* pool; a copy of the current identity in another pool keeps a refresh token Claude Code has already rotated |
| C-7 | Too many windows to add an account | capture, official sign-in and Claude Swap import each open a form; sign-in needs a second "Finish sign-in" click |
| C-8 | The account list is long | each account is a three-column card with an action row of up to seven buttons |

Claude Swap reference: `realiti4/claude-swap` at `3a4e5c14873eb5b32f182d55c68da98ac8c0db45`,
`src/claude_swap/macos_keychain.py` (stdin `security -i`, `-X` hex, 4096-byte line limit, argv
only above it — the shape Claude Code uses) and `src/claude_swap/oauth.py` (refresh of inactive
accounts only, `invalid_grant` is permanent, 401 retries once after refresh).

## Source ledger

| Source | Read | Finding |
|---|---|---|
| `AGENTS.md`, `docs/HANDOFF.md`, `CONTRACTS.md`, `ACCOUNTS-AND-ROTATION.md`, `RESEARCH-0.3-IMPORTS.md` | yes | 0.3 decision "no competing refresh grant" is superseded by D-2; synthetic fixtures only |
| code at `5bac51c` | yes | C-1…C-8 |
| `docs/ux/*` | yes | SCN-018…022 describe the capture-first dialog this run replaces |
| board `docs/evidence/backlog.md` | yes | 10 open rows, none covers C-1…C-8 |
| `docs/evidence/retro.md` | none found | no standing instructions |
| verification ledger `docs/evidence/verification.md` | yes | historical 0.1 rows; no `never` status column |
| code graph (`graphify-out/`) | none found | graphify cannot build on this machine (machine `CLAUDE.md`); not a gate |
| wiki `projects/` | checked at stage 9 | |
| Claude Swap source | yes | reference shapes above |

## REQ table (frozen; adding is free, removing needs the operator)

| REQ | Requirement | Verified by |
|---|---|---|
| REQ-1 | ~~macOS vault: encrypted files under `<data root>/vault`, one key item created race-free and read through `/usr/bin/security`, key cached per process; legacy items migrate on first read and are removed; delete removes both~~ — withdrawn at integration (D-1); the outcome, no vault dialog, is delivered by 0.4.1 | [KEYCHAIN.md](KEYCHAIN.md), board SB-12 |
| REQ-2 | Claude Code credential item written and deleted only through `/usr/bin/security`; the secret travels on stdin as hex, argv only when the line exceeds the 4 KB `security -i` buffer | `cargo test -p switchboard-runtime external_keychain` (command shape, quoting, no secret in argv) |
| REQ-3 | Isolated sign-in reads and cleans its Keychain item through `/usr/bin/security` | launch unit test on the command shape; code receipt |
| REQ-4 | Refresh of inactive Claude OAuth: before expiry (10 min margin) or on 401, never for the current native identity, propagated to every stored copy of the same lineage; `invalid_grant` marks the account "Sign in again" | `cargo test -p switchboard-runtime refresh` against a local token server |
| REQ-5 | Native activation refreshes an expired target first and preserves the live generation into every stored copy of the current identity, in every pool | runtime unit tests |
| REQ-6 | Rotation treats a refreshable inactive account as eligible and switches end to end | monitor tests with an expired candidate |
| REQ-7 | One click adds the current Claude Code or Codex account; an already-stored identity shows as connected instead | browser check on demo; `scripts/test-ui-logic.mjs` |
| REQ-8 | One click starts an official sign-in for another account: no form, completion is detected and saved without a Finish click, label defaults to the email, Cancel stays available | runtime `LoginStatus` test; browser check |
| REQ-9 | One click imports Claude Swap profiles into the default pool, no dialog | browser check |
| REQ-10 | Compact list: one row per account grouped by provider (pool sub-groups when several), quota inline, secondary actions in a row menu; Claude row primary is Switch Claude Code without confirmation | browser check at 1280×720 and 740×560 with 12 demo accounts, screenshots in evidence |
| REQ-11 | API key, setup token and OAuth JSON stay reachable from Add account | browser check |
| REQ-12 | Docs in the same change: scenarios, screens, flows, ACCOUNTS-AND-ROTATION, CONTRACTS, CLI if changed, decision record, HANDOFF, evidence | `scripts/check_docs.py`, stage 10 ladder walk |
| REQ-14 | Automatic encrypted backup after every account, credential or policy change (at most once a minute) and daily; newest ten kept; only a real owner writes | `cargo test -p switchboard-core backup`, `backups_follow_changes_and_restore_into_a_fresh_install` |
| REQ-15 | Restore adds the accounts a store lacks and never replaces a newer sign-in; another key or a tampered file is refused; policies come back switched off | `restore_never_replaces_a_newer_sign_in`, `another_key_or_a_tampered_file_is_refused` |
| REQ-16 | A limit error (Claude Code marker or managed 429) moves the account in use at once, bypassing the cooldown; a limited account is never a candidate; errors in the first minute after a switch are not held against the new account | `a_limit_error_switches_despite_spare_quota_and_skips_limited_candidates`, `limits::tests`, `a_limit_error_in_claude_code_switches_with_quota_to_spare` |
| REQ-17 | Backups panel (folder, list, Back up now, Restore) and a Limit reached badge on the row | browser check on demo, `docs/evidence/design-0.5/about-backups.png` |
| REQ-13 | Full gate green: `./scripts/check.sh`; unsigned `npm run app:build`; packaged smoke | command output in evidence |

## Contracts

**Vault (macOS).** Withdrawn — see D-1. Switchboard's own items follow [KEYCHAIN.md](KEYCHAIN.md).

**`/usr/bin/security` writes.** `security -i` with one line
`add-generic-password -U -a "<account>" -s "<service>" -X <hex>` on stdin when the line is at
most 4032 bytes, otherwise the same arguments in argv (Claude Code's own fallback). Delete:
`delete-generic-password -a … -s …`, exit 44 counts as success. 5-second deadline, no stderr
captured, sanitized errors.

**Refresh.** `POST https://platform.claude.com/v1/oauth/token`, JSON
`{grant_type: refresh_token, refresh_token, client_id: 9d1c250a-e61b-44d9-88ed-5944d1962f5e}`,
10-second timeout, no redirects. Success replaces access token, expiry (`now + expires_in`),
refresh token when returned and scopes, both in the credential and in its native envelope.
`invalid_grant` (400/401/403 with that `error`) → permanent: the account is reported as
needing sign-in until a new capture or login replaces its credential. Anything else → transient,
retried on the next due check. Runs under the owner's mutation lock so activation cannot hand
Claude Code a refresh token that is being consumed. Never runs for an account whose identity
matches the current ordinary Claude Code sign-in, or when that sign-in cannot be read.

**Runtime operations added.** `LoginStatus { login_id }` → `{ "state": "pending" | "complete" | "ended" }`
(no credential; `ended` = the Terminal session exited without signing in). `BeginLogin.label` may be empty: the captured email becomes the label.
`MonitorStatus` adds `"sign_in_required": [account ids]`.

## Plan (each task: failing test → implementation → green → review)

| Task | Implements | Files |
|---|---|---|
| T1 vault | REQ-1 | withdrawn at integration (D-1) |
| T2 security writes | REQ-2, REQ-3 | `switchboard-runtime/src/external_keychain.rs`, `external.rs`, `launch.rs` |
| T3 refresh | REQ-4, REQ-5, REQ-6 | new `switchboard-runtime/src/refresh.rs`, `monitor.rs`, `lib.rs`, `switchboard-core` store helpers |
| T4 runtime ops | REQ-8 | `lib.rs`, `src-tauri/src/main.rs`, `control.rs` if the operation crosses it |
| T5 UI | REQ-7…REQ-11 | `src/main.ts`, `src/style.css`, `src/adapter.ts`, `src/demo.ts`, `src/types.ts`, `src/ui-logic.ts` |
| T6 docs and evidence | REQ-12, REQ-13 | `docs/…` |

## Carry-over ledger

| Item | Status |
|---|---|
| Codex OAuth refresh for inactive Codex accounts (Codex has no native rotation target; managed route only) | open → board SB-16 |
| Prompt-free behaviour observed on the operator's own Mac with real accounts (synthetic tests cannot see a macOS dialog) | open → board SB-15 |
| Release 0.5.0-beta.1 | done → board SB-17 |

## 0.5.1 — Claude Swap parity and review fixes

Run started 2026-10-03 on `agent/switchboard-0.5.1` from 0.5.0 (`67bf1a3`). Operator request
(Russian, paraphrased): study how switching, session isolation and credential substitution work,
read Claude Swap's code, review again, finish whatever is missing or unfinished, fix bugs, bring
the documentation up to date, cover the project with tests, release, and update the Fabric
workspace after the child repositories. Source: the comparison report
[RPT fabric-switchboard/2026-10-03-claude-swap-comparison](reports/2026-10-03-claude-swap-comparison/README.md)
(rows #1–#18 below) plus three review packets run in parallel worktrees (packet-d launch,
packet-e proxy, packet-f UI vocabulary) and a final adversarial review of the branch at
`dc17508` (findings R-1…R-12; two P1 — a grant whose successor could be lost — four P2, four
P3 and two test gaps; all fixed in `13a8712`, REQ-41…REQ-48).

No new operator decision was needed: every fix tightens an existing contract (D-2 renewal,
C-2 prompt-free item) toward what Claude Swap already does. The one deliberate extension is
REQ-33 — the live account is renewed when Claude Code itself left it expired and idle, under
Claude Code's own locks — which stays inside D-2's intent (no second refresher of a lineage in
use).

| REQ | Requirement (report row) | Verified by |
|---|---|---|
| REQ-18 | Shared credential keys (`mcpOAuth`, `mcpOAuthClientConfig`, `mcpXaaIdp`, `mcpXaaIdpConfig`, `pluginSecrets`) come from the live item on a switch (#1) | `shared_keys_come_from_the_live_item_and_account_keys_from_the_target` |
| REQ-19 | The outgoing generation is stored under Claude Code's locks before the first write; a failure aborts (#2) | `the_outgoing_generation_is_preserved_under_the_lock_before_any_write`, `switching_away_keeps_the_live_generation_in_every_pool` |
| REQ-20 | A live token of another lineage is never filed under the configured name nor switched from (#3) | `a_foreign_lineage_under_this_name_is_never_filed_or_switched_from`, `background_sync_never_files_another_accounts_lineage`, `the_provider_names_the_owner_of_an_unknown_live_lineage` |
| REQ-21 | A renewed token that cannot be stored is kept and adopted before the next grant or switch (#4) | `a_renewed_token_that_cannot_be_stored_is_kept_and_adopted_later`, `an_idle_renewal_that_cannot_be_written_is_stashed` |
| REQ-22 | While Claude Swap runs, its accounts are not renewed and its newer generations are followed; the UI says so (#5) | `a_running_claude_swap_renews_its_accounts_and_switchboard_follows_it`, `only_claude_swap_itself_counts_as_running`; rotation-bar and import notices |
| REQ-23 | A live sign-in wiped after `invalid_grant` is signed out and does not block a switch (#6) | `a_wiped_live_sign_in_is_signed_out_and_does_not_block_a_switch` |
| REQ-24 | Held locks are waited for up to 9 s; stale ones (60 s, config 10 s) are taken over (#7; closes board SB-04) | `a_live_lock_is_waited_for_and_a_stale_one_is_taken_over` |
| REQ-25 | An existing macOS `.credentials.json` is rewritten and restored on rollback, never created (#8) | `a_plaintext_copy_on_macos_is_rewritten_only_when_it_exists` |
| REQ-26 | A managed `primaryApiKey` is removed on OAuth activation and restored on rollback (#9) | `a_managed_api_key_is_removed_and_returns_on_rollback` |
| REQ-27 | Isolated launches renew below four hours of token life; the account in use launches with the live token (#10) | `an_isolated_launch_wants_hours_of_token_life` |
| REQ-28 | Native operations are refused from inside Switchboard's data folder (#11) | `a_home_inside_switchboards_data_folder_is_refused` |
| REQ-29 | Import prefers a Claude Swap session profile's later-expiring credential (#12) | `a_newer_session_profile_generation_wins_over_the_backup` |
| REQ-30 | A token endpoint naming another owner is a dead lineage for an inactive account; for the live account the successor stays with Claude Code and is remembered as foreign (#13) | `a_token_issued_for_another_account_is_a_dead_lineage_here`, `an_idle_renewal_issued_to_another_account_stays_with_claude_code_only` |
| REQ-31 | Rejected lineages are remembered across restarts as fingerprints only (#15) | `a_rejected_lineage_is_remembered_across_restarts` |
| REQ-32 | The Keychain account at sign-in is `$USER`, else the OS user (#17) | `keychain_user_prefers_a_set_user_then_the_passwd_entry` |
| REQ-33 | The live account Claude Code left expired for more than 300 s is renewed under its locks into the live item and every copy; a changed item is left alone | `an_idle_live_account_is_renewed_in_claude_code_and_every_copy`, `an_idle_live_item_that_changed_is_left_alone`, `an_idle_live_lineage_rejected_by_the_provider_is_dead`, `a_renewed_live_item_keeps_every_other_key` |
| REQ-34 | A reset-less limit marker from a session older than the switch is not charged to the new account | `an_old_sessions_marker_without_a_reset_is_not_charged_to_the_new_account` |
| REQ-35 | Switching away from an unsaved signed-in account is refused; the account in use is a no-op | `switching_away_from_an_unsaved_account_is_refused_and_the_account_in_use_is_a_no_op` |
| REQ-36 | Proxy (packet-e): only unsuccessful requests are journaled; 10 s connect, 600 s idle read, no total cap; 32 MiB body; client identity headers relayed, limit and retry headers returned | `successful_requests_never_push_other_events_out_of_the_journal`, `every_unsuccessful_outcome_is_still_journaled_once`, `a_stream_that_keeps_moving_outlives_the_read_timeout_many_times_over`, `a_stream_idle_past_the_read_timeout_is_cut_and_journaled_as_aborted`, `body_limit_admits_the_providers_32_mib_and_refuses_one_byte_more`, `claude_code_headers_reach_upstream_and_limit_headers_come_back` |
| REQ-37 | Launch (packet-d): a sign-in reservation expires after ten minutes; a reused pid does not hold a home | `reservation_expires_after_ten_minutes_in_either_direction`, `stale_reservation_no_longer_holds_the_home`, `a_process_younger_than_its_marker_is_a_reused_pid`, `live_pid_holds_the_home_only_while_it_can_be_the_session` |
| REQ-38 | UI (packet-f): every backend refusal reaches the user verbatim or through a mapped message; the sign-in banner reports each outcome | `scripts/check-error-vocabulary.mjs` (in `check.sh`), `scripts/test-ui-logic.mjs` (sign-in outcomes) |
| REQ-40 | A usage probe answered 429 waits as the provider asks: numeric `Retry-After` up to six hours, else 900 s, never sooner than the backoff (#14; board SB-18) | `a_rate_limited_usage_check_waits_as_the_provider_asks`, `usage_checks_report_the_providers_wait_and_never_its_body` |
| REQ-41 | Final review R-1/R-8: a grant never loses its successor — kept by the grant task before returning, keyed by the spent token, so a cancelled caller or a failed write keeps it | `a_grant_whose_caller_gives_up_still_keeps_the_successor`, `an_idle_renewal_that_cannot_be_written_is_stashed` (mutation: dropping the keep fails three tests) |
| REQ-42 | Review R-1: a successor the token endpoint issues to another saved account reaches that account; the named account's copies holding the spent token are dead | `a_renewal_issued_to_another_saved_account_reaches_that_account`, `a_renewal_goes_only_to_the_owner_the_token_endpoint_named` (mutation: ignoring the owner fails three tests) |
| REQ-43 | Review R-3: every copy holding one spent token adopts the same kept successor | `an_idle_renewal_that_cannot_be_written_is_stashed` (both pools) |
| REQ-44 | Review R-4: a switch never overwrites a copy newer than Claude Code's item | `a_copy_newer_than_claude_codes_item_is_not_overwritten_on_switch` (mutation-checked) |
| REQ-45 | Review R-5/R-6: a live lineage nothing attributes is never filed; the same login in another organization is another account; the owner is asked before each native rotation, at most once a minute | `a_live_generation_nobody_can_attribute_is_never_filed`, `the_same_login_in_another_organization_is_another_account`, `an_unanswered_owner_question_waits_a_minute`, `background_sync_never_files_another_accounts_lineage` (rotated case) |
| REQ-46 | Review R-7: Claude Swap running but unreadable keeps what it holds; a session profile is trusted only while its own config names the slot's account | `a_running_claude_swap_renews_its_accounts_and_switchboard_follows_it`, `a_newer_session_profile_generation_wins_over_the_backup` |
| REQ-47 | Review R-9/R-11: idle renewal takes only the credential locks and rewrites an existing plaintext copy | `a_renewal_takes_only_the_credential_locks`, `a_renewed_live_item_reaches_an_existing_plaintext_copy_only` |
| REQ-48 | Review R-10: identities without an account id are never the same account | `identities_without_an_account_id_are_never_the_same_account` (mutation-checked) |
| REQ-39 | Docs in the same change (ACCOUNTS-AND-ROTATION, SPEC, CONTRACTS, scenarios, evidence, HANDOFF), coverage measured, full gate green (#18) | `scripts/check_docs.py`, `cargo llvm-cov`, `./scripts/check.sh` |

**Not taken, with reason.** Report §"Accepted": a credential item larger than the `security -i`
line goes to argv as hex, exactly as Claude Code does; no prompt-free alternative exists.

| Item | Status |
|---|---|
| Usage-probe `Retry-After` (report #14) | done in this run (REQ-40), board SB-18 closed |
| Release 0.5.1-beta.1 | in progress → board SB-19 |
| Live acceptance of 0.5.1 on the operator's Mac beside a running Claude Swap | open → board SB-15 (extended) |

## 0.5.2 — the last rows of the comparison

Report rows #16 and #19–#22 were in the working gap table but missing from the published one
(correction in the report). Two are code; three are recorded decisions.

| REQ | Requirement (report row) | Verified by |
|---|---|---|
| REQ-49 | `invalid_client` from the token endpoint holds every renewal for an hour, blames no account (no dead lineage, no backoff), then one grant tests the client again; `MonitorStatus.renewal_blocked` and a caption on the Automatic switching bar say so (#16) | `a_refused_client_holds_every_renewal_without_blaming_an_account`, `a_refused_client_after_the_hold_is_tried_once_more` (mutation: classifying it as transient fails both) |
| REQ-50 | A `~/.claude.json` a switch creates from nothing carries `hasCompletedOnboarding: true`; an existing config is never given it (#20) | `a_config_created_by_a_switch_skips_onboarding_and_an_existing_one_is_kept` |

Recorded, not built:
- **#19** isolated homes stay minimal by contract (SPEC §8). Sharing user-scope settings, skills
  or MCP servers into them is the operator's decision: board SB-20.
- **#21** the adoption delay is a live measurement: board SB-06.
- **#22** a `.prev` copy is declined. Backups keep ten generations, and kept successors cover the
  renewal gap.

## 0.5.3 — sessions are never lost; the app stays quiet

Operator request, 2026-10-03, paraphrased from Russian:
- Make the token handling right, because all of the work rests on it.
- Sessions must never be lost and must always stay current, including while Claude Swap runs in
  the background.
- Review the desktop app's architecture, build and permissions, so that there are no redundant
  services, no errors, and no pop-ups or system messages that bother the user.

How the run worked:
- **Two independent read-only audits:**
  - token custody: 3 P1 · 5 P2 · 6 P3, three of them proven by scratch tests;
  - desktop: 3 P1 · 4 P2 · 4 P3, with a process count measured on the running app.
- **Board SB-21**, observed on the operator's Mac, was taken into the run.
- **No new operator decision was needed.** The one judgement call is the division of work with
  Claude Swap, below. It follows the operator's own goal (manage correctly beside Claude Swap)
  and changes nothing in Claude Swap's files.

**Division of work with Claude Swap:**
- When it **runs**, it renews the accounts it holds, and Switchboard follows its generations.
- When it **switches** Claude Code automatically, Switchboard's native rotation holds.
- When it **has stopped**, Switchboard takes whatever Swap renewed last and renews those accounts
  itself.
- Switchboard never writes to Claude Swap's files.

| REQ | Requirement (audit finding) | Verified by |
|---|---|---|
| REQ-51 | Claude Swap's last renewals are taken after it stops; reads happen on start, on stop, on change and while it runs (SB-21) | `a_stopped_claude_swap_still_hands_over_what_it_renewed_last`, `claude_swaps_files_are_read_when_they_can_hold_something_new`, `claude_swaps_files_change_their_signature`, `a_dead_copy_comes_back_when_claude_swap_holds_its_successor` |
| REQ-52 | An unreadable Swap row keeps its hold (token P1-1) | `a_claude_swap_row_that_could_not_be_read_stays_held` (mutation-checked) |
| REQ-53 | The offline CLI/MCP never spends; a `--data-dir` owner never spends (token P1-2, P3) | `the_offline_cli_never_spends_a_refresh_token`, `the_offline_cli_still_refuses_a_lineage_the_owner_saw_rejected` (mutation-checked) |
| REQ-54 | Import never moves an account backwards and never imports a foreign lineage (token P1-3) | `importing_from_claude_swap_never_moves_an_account_backwards` (mutation-checked) |
| REQ-55 | Capture refuses a foreign lineage, and an unattributed one over a saved copy (token P2-1) | `capturing_another_accounts_lineage_under_this_name_is_refused`, `capturing_an_unattributed_sign_in_never_overwrites_a_saved_copy` (mutation-checked) |
| REQ-56 | A successor issued to a saved account with an older lineage still reaches it (token P2-2) | `a_successor_issued_to_a_saved_account_with_an_older_lineage_still_reaches_it` |
| REQ-57 | Switching to an account Swap holds catches up with Swap first; refused while its row is unreadable (token P2-3) | `switching_to_an_account_claude_swap_renews_takes_its_newest_generation_first` |
| REQ-58 | Native rotation holds while Claude Swap switches automatically; a leftover plist is not "running" (token P2-4, P3) | `native_rotation_leaves_switching_to_an_auto_switching_claude_swap`, `claude_swap_switching_is_told_from_merely_running` |
| REQ-59 | One grant per refresh token, released on every exit; no switch to an account mid-renewal (token P3) | `an_account_whose_renewal_is_in_flight_is_not_switched_to`, `a_refused_client_holds_every_renewal_without_blaming_an_account` (caught a leak in the first ordering) |
| REQ-60 | Live sync never moves a newer copy back; rotation never picks a rejected sign-in; restore starts from the newest generation (token P3) | `background_sync_never_moves_a_newer_copy_back`, `rotation_skips_a_rejected_sign_in_for_the_next_eligible_account`, `a_restored_copy_starts_from_the_newest_generation_saved_elsewhere` (all mutation-checked) |
| REQ-61 | A busy store never crashes the app; Retry recovers; a second launch focuses the first (desktop P1-1) | `src-tauri` `Slot` (build + gate); live check after install |
| REQ-62 | Codex's Keychain item is read without any dialog (desktop P1-3) | `codexs_keychain_item_is_read_only_the_quiet_way` |
| REQ-63 | Background Keychain reads cut: 30 s reuse, nothing without a saved Claude OAuth account, owner lookup only with native rotation on (desktop P1-2, P2) | `the_background_reuses_one_claude_read_until_switchboard_writes`, `without_saved_claude_accounts_the_monitor_leaves_claude_code_alone`; process count on the installed app |
| REQ-64 | A translocated copy is never linked or written into agent configs; About says to move the app (desktop P2) | `a_translocated_copy_is_never_linked_or_offered` |
| REQ-65 | Release profile LTO + one codegen unit + strip (desktop P3) | bundle size before/after in evidence |
| REQ-67 | Review of the 0.5.3 diff (P2 ×3, P3 ×6): fresh reads for "is it in use" and every switch; email-scoped Swap holds; stopped-aware catch-up; partial stopped reads repeated; injectable cache test; STARTUP_FAILED and a background owner retry; honest locked-Keychain message | `only_the_unreadable_rows_account_stays_held`, `switching_to_an_account_swap_no_longer_lists_is_not_refused`, `a_switch_after_claude_swap_stopped_takes_its_generation_and_holds_nothing`, `the_background_reuses_one_claude_read_until_switchboard_writes` (mutation-checked) |
| REQ-66 | Docs, scenarios, strings and evidence in the same change; full gate green | `scripts/check_docs.py`, `./scripts/check.sh` |

Not built:
- **The renewal successor is not persisted.** It is held only while the vault refuses to store
  it, and persisting it elsewhere would mean plaintext; documented in ACCOUNTS-AND-ROTATION.
- **`security` waits on async threads** (desktop P3) → board SB-23. Far fewer calls now; moving
  them to a blocking pool is a refactor across every native read.
- **Windows Authenticode** stays SB-03, an operator decision.
