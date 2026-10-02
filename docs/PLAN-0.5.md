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
| Release 0.5.0-beta.1 | in progress → board SB-17 |
