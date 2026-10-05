# Evidence — 0.5 prompt-free switching, renewal, one-click accounts; 0.5.1 Claude Swap parity

Run: [PLAN-0.5](../PLAN-0.5.md), 2026-10-02, branch `agent/switchboard-0.5`, first built on `main` `5bac51c`,
then rebased onto `main` `1cfc793` (0.4.1-beta.1 and its shared-trust vault). At integration the
withdrawn file vault (D-1) was removed; the rows below marked *(pre-integration)* exercised it and
no longer describe shipped code. The release record is appended at the end once it is cut. Every check below is synthetic or opt-in; no real Claude or Codex credential was
read or written.

## Checks run (macOS 26.6, arm64)

| Check | Command | Result |
|---|---|---|
| Full gate | `./scripts/check.sh` | exit 0 (brand, `npm run build`, read-deadline 5 cases, ui-logic 12 cases, `cargo fmt --check`, `cargo test --workspace --locked`, clippy `-D warnings`, docs links, plugin, notices) |
| Workspace tests | `cargo test --workspace --locked` | 146 passed, 0 failed, 1 ignored (the opt-in Keychain test below) |
| Opt-in native Keychain *(pre-integration)* | `cargo test -p switchboard-core --test storage native_vault_roundtrip_and_legacy_migration_use_only_synthetic_items -- --ignored --exact` | 1 passed in 0.37 s: synthetic credential sealed and read under the shared vault key through `/usr/bin/security`; a synthetic pre-0.5 Security.framework item migrated into `vault/` and removed. The vault key item `ai.passioncode.fabric-switchboard.vault-key` / `v1` is the production one — the test creates it if absent and never prints it |
| `security` exit codes | synthetic item `ai.passioncode.switchboard.selftest-<pid>` | create 0; duplicate 45 through `security -i` and argv; read 0; attribute-only 0; delete 0; delete/read missing 44 |
| Unsigned app bundle | `npm run app:build` | exit 0, `target/release/bundle/macos/Fabric Switchboard.app` |
| Packaged smoke | `python3 scripts/smoke_native.py ".../Contents/MacOS/fabric-switchboard" --version 0.4.0` | `{"status": "PASS", …, "real_provider_auth": "NOT_READ"}` |
| Browser demo | [design-0.5](design-0.5.md) | 1280×720 and 740×560, light and dark, interaction checks verbatim |

## Planted-defect checks (a green test that never failed is not evidence)

| Test | Defect planted | Result before restoring |
|---|---|---|
| `file_vault::tests::tampered_swapped_or_foreign_key_files_are_refused` *(pre-integration, code withdrawn)* | AAD no longer binds the account id | FAILED |
| `monitor::tests::a_rejected_lineage_reports_sign_in_without_probing` | no failed health written for a dead lineage | FAILED |

## Seam review and its fixes

An independent seam reading of the diff (task-pipeline `verifier-seam`) returned **fail** with
three breaks; all three were fixed with tests in this change:

| Finding | Fix | Test |
|---|---|---|
| A dead lineage stayed at the head of the due queue and starved every other account | `check` writes failed health with the 1800 s backoff | `a_rejected_lineage_reports_sign_in_without_probing` |
| Activation, rotation and project apply could hand Claude Code a rejected refresh token | `activate_native` refuses a dead lineage for every caller; the dead state is checked before expiry | `a_rejected_lineage_is_never_handed_to_claude_code` |
| The vault minted a new key while sealed files existed | refused; then the whole file vault was withdrawn at integration (D-1) | *(pre-integration)* |

Risks it raised, and their disposition: renewal skips an identity seen in the ordinary Claude Code
within 15 minutes and rows without a captured identity (`rows_without_identity_and_recently_active_identities_are_not_renewed`);
a separate renewal pass covers tokens expiring while quota checks back off
(`a_token_expiring_between_backed_off_checks_is_renewed`); one 25-second deadline bounds a check
below the control channel's 30 seconds; only 401 maps to “Provider rejected the credential”; the
sign-in poller stops after a failed finish and offers Retry, and a forgotten sign-in reads as ended
(code-reviewed, no automated case); the read-only MCP server's `switchboard_usage` may renew an
inactive account's token inside Switchboard — accepted and stated in the tool description and the
skill.

## Not run

- Windows compile of `switchboard-runtime`/`-cli` on this Mac: `ring`'s C build needs MSVC headers
  that are not installed (pre-existing; `switchboard-core` cross-checks clean for
  `x86_64-pc-windows-msvc`). The Windows workflow is the proof.
- Any real account, live refresh grant against `platform.claude.com`, a running Claude Code picking
  up a switch, and the absence of `Claude Code-credentials` dialogs on the operator's Mac — board SB-15.

## Backups and limit errors (second operator request, 2026-10-02)

Built on branch `agent/switchboard-0.5-backup`. A second seam review returned **fail** with four
breaks; each fix has a test that was watched failing against a planted defect:

| Finding | Fix | Test (planted defect → FAILED) |
|---|---|---|
| Backups of every store shared one folder; pruning deleted another store's or machine's files; offline `backup now` wrote | each backup names its store and key; only the writing store's files are pruned; only the default data folder's owner writes; offline `backup now` refuses | `stores_sharing_a_folder_never_prune_each_other`, `offline_backup_never_writes_and_restore_takes_only_backup_names` |
| Partial backups (an unreadable account) pushed out the complete one | the store's most complete backup is always kept; About shows how many accounts were missing | `partial_backups_never_push_out_the_last_complete_one` (protection removed → FAILED) |
| A second restore changed accounts without identity | the same token in the same provider and pool counts as present | `restoring_twice_changes_nothing_for_accounts_without_identity` (token rule removed → FAILED) |
| After a restart, the previous account's limit was charged to the new one | the journal's last switch to the account in use dates it | `after_a_restart_the_journal_dates_the_last_switch` (journal date removed → FAILED) |

Risks raised and their disposition: the backup folder moved from Documents (a macOS privacy
prompt) to `~/Library/Application Support/Fabric Switchboard Backups`, beside the data folder;
the Windows key moved into the backup folder; a marker with another held account's reset or
within 10 s of a managed 429 is not charged to the account in use; one managed 429 is a burst,
two within five minutes a limit; a limited account in use takes any account below 100%; the
write no longer holds the lock the About listing reads; project rules start the switch grace;
MCP `switchboard_status` reports `limited`. Holds live in memory and are re-read from the
15-minute transcript window after a restart.

## Release v0.5.0-beta.1 (2026-10-02)

| Step | Result |
|---|---|
| Source | `main` `e6c5e54` (merges of [#18](https://github.com/passioncode-ai/fabric-switchboard/pull/18), [#19](https://github.com/passioncode-ai/fabric-switchboard/pull/19)); versions 0.5.0 in step (`check-brand.mjs` → `version 0.5.0 verified`, `check_plugin.py` 0 errors, `claude plugin validate --strict` passed for marketplace and plugin) |
| Gate | `./scripts/check.sh` exit 0; 175 Rust tests passed, 2 opt-in Keychain tests ignored. No hosted dispatch (nightly policy) |
| macOS | `build_macos.py --arch universal --notary-profile …` exit 0, [receipt](build-0.5.0-macos-universal.json): app and CLI `x86_64 arm64`, clean source `e6c5e54`, notary `0b19c796-9e4c-4d2c-a394-9d55846fd10b` **Accepted**, ZIP stapled (`stapler validate` worked), `spctl` accepted / Notarized Developer ID |
| Windows | `build_windows_cross.py` exit 0, [receipt](build-0.5.0-windows-x64.json); unsigned, native run NOT_RUN |
| GitHub | tag `v0.5.0-beta.1` = `e6c5e54`, [prerelease](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.0-beta.1) with 2 ZIPs, 2 receipts, `SHA256SUMS-0.5.0.txt`; [publication receipt](publication-0.5.0.json) |
| Anonymous download | no token: both archives `OK` against the sums; quarantined app `accepted, source=Notarized Developer ID`; CLI `switchboard 0.5.0` |
| Website | `passioncode-ai.github.io` `a7e5a8b` ([PR #27](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/27), check passed), deployed (Worker version `9f72c…`); `/switchboard/download/macos` and `/windows` → 302 to the 0.5.0 archives, fetched hashes equal; page and JSON-LD `0.5.0-beta.1` |
| Local install | 0.4.1 quit, kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.4.1-2026-10-02/` (rollback: move it back); 0.5.0 from the verified published ZIP; `CFBundleShortVersionString` 0.5.0, `spctl` accepted |

### On the operator's Mac, with the operator's accounts (2026-10-02, after install)

| Check | Result |
|---|---|
| First launch | automatic backup written within a minute: `switchboard-backup-1790978340.json`, 15 accounts, 0 missing, own, openable; folder mode 0700, file 0600 |
| Switch Claude Code | `switchboard accounts activate` (through the running app) to the freest account (17% used) from the one in use (79%): `~/.claude.json` `oauthAccount.accountUuid` changed to the target's, `switchboard current` names the target |
| New Claude Code session | `claude -p` after the switch → `is_error: false`, `result: "ok"` |
| Keychain dialogs | `log show --predicate 'process == "SecurityAgent"'` since launch and since the switch: **0** entries |
| Running sessions | the Claude Code session that ran these checks kept working; whether it moved to the new account is not observable from outside it (board SB-06) |

Not observed yet: a renewal of an inactive account and a switch on a limit error with live accounts (SB-15), Windows (SB-02).

## 0.5.1

Branch `agent/switchboard-0.5.1` from 0.5.0 (`67bf1a3`); scope and REQ-18…REQ-48 in
[PLAN-0.5 §0.5.1](../PLAN-0.5.md#051--claude-swap-parity-and-review-fixes); source report
[RPT fabric-switchboard/2026-10-03-claude-swap-comparison](../reports/2026-10-03-claude-swap-comparison/README.md).
Synthetic fixtures only: nothing read or wrote the real Keychain, `~/.claude*` or `~/.codex`.

### Checks run (macOS 26.6, arm64, 2026-10-03)

| Command | Result |
|---|---|
| `./scripts/check.sh` (fmt, clippy `-D warnings`, all Rust tests, UI build and logic, error vocabulary, docs links, plugin, notices) | exit 0 |
| `cargo test --workspace` | 242 passed, 0 failed, 2 ignored (the opt-in synthetic Keychain tests named in README) |
| `node scripts/check-error-vocabulary.mjs` | 231 backend messages: 77 verbatim, 140 mapped, 14 allowlisted |
| `cargo llvm-cov --workspace --summary-only` | 86.10 % of lines (table below) |
| `reports.py check docs/reports/2026-10-03-claude-swap-comparison` | 0 errors |

### Planted-defect checks

Each fix was reverted in place and the suite run; every one failed a named test, then the
source was restored byte for byte.

| Defect planted | Tests that failed |
|---|---|
| grant task does not keep the successor | `a_grant_whose_caller_gives_up_still_keeps_the_successor`, `a_renewed_token_that_cannot_be_stored_is_kept_and_adopted_later`, `an_idle_renewal_that_cannot_be_written_is_stashed` |
| settle ignores the owner the token endpoint named | `a_renewal_issued_to_another_saved_account_reaches_that_account`, `a_token_issued_for_another_account_is_a_dead_lineage_here`, `an_idle_renewal_issued_to_another_account_stays_with_claude_code_only` |
| idle renewal drops a foreign successor | `an_idle_renewal_issued_to_another_account_stays_with_claude_code_only` |
| switch overwrites a newer copy | `a_copy_newer_than_claude_codes_item_is_not_overwritten_on_switch` |
| email-only identities compare equal | `identities_without_an_account_id_are_never_the_same_account` |
| (before this pass) MCP keys dropped, outgoing generation saved before the locks, foreign lineage filed, successor discarded, double renewal beside Claude Swap | five tests, one each, recorded with the comparison report's probes |

### Final adversarial review

An independent read of `git diff main...dc17508 -- crates/` reported twelve findings, R-1…R-12:

- **P1 (two):**
  - R-1: a refresh issued to another account discarded the successor.
  - R-2: a deadline or a dropped request could cancel a grant mid-flight.
- **P2 (four):**
  - R-3: a stash shared by several copies got stuck.
  - R-4: a switch could overwrite a newer copy.
  - R-5: an unattributed lineage was filed.
  - R-6: the owner check ignored the organization.
- **R-7:** Claude Swap read failures and unverified session tokens.
- **R-8:** a failed foreign idle write.
- **P3:**
  - R-9: locks were held across a network call.
  - R-10: email-only identities compared equal.
  - R-11: the plaintext copy went stale on idle renewal.
- **R-12:** two tests that passed for the wrong reason.

All twelve were confirmed against the code and fixed in `13a8712`, each with a test (REQ-41…REQ-48). Checked and found correct by the same review:

- activation re-reads under the locks and refuses to roll back after a lost lock;
- shared keys come from the live item;
- dead lineages are kept as fingerprints only;
- no credential reaches logs or IPC;
- responses are size-capped with redirects off;
- untrusted input has no panic paths.

### Coverage (lines, `cargo llvm-cov`)

| File | Lines | What is not covered, and why |
|---|---|---|
| runtime `refresh.rs` | 95.92 % | network error branches of the profile call |
| runtime `limits.rs` | 94.82 % | |
| runtime `control.rs` | 95.02 % | |
| runtime `monitor.rs` | 88.64 % | the owner loop's native branch runs only with `Owner::native` (the real sign-in) |
| runtime `launch.rs` | 87.07 % | opening Terminal and a live official login |
| runtime `external.rs` | 84.03 % | `Native` reader and writer: the real Keychain, `~/.claude.json` and `ps` |
| runtime `lib.rs` | 80.86 % | `Owner::native` paths and the packaged-app helpers |
| runtime `agents.rs` | 91.60 % (55.88 % before `setup_offers_commands_for_the_found_cli_and_never_a_credential`) | `link_bundled_cli` writes `~/.local/bin`, never run by tests |
| runtime `external_keychain.rs` | 0 % | the `/usr/bin/security` calls for Claude Code's item; its command shape, quoting and exit codes are tested in core `security_cli.rs` |
| core `vault.rs` | 52.78 % | Security.framework vault, exercised by the opt-in synthetic Keychain test only |
| core `security_cli.rs` | 74.04 % | spawning the real `/usr/bin/security` |
| cli `main.rs` | 60.24 % | human-readable output of commands that need a running owner with real accounts |
| core `lib.rs`, `credential.rs`, `rotation.rs`, `projects.rs`, `backup.rs`, proxy `lib.rs`, cli `mcp.rs` | 92–97 %, 79 % | |

100 % line coverage is not reachable without the real Keychain, Terminal and provider accounts, which
AGENTS.md forbids in tests. The uncovered code is the OS boundary. Each of those calls is reached
through a seam, and the seam is covered with fixtures.

### Release v0.5.1-beta.1 (2026-10-03)

| Step | Result |
|---|---|
| Source | `main` `2f4e429` (merge of [#20](https://github.com/passioncode-ai/fabric-switchboard/pull/20)); versions 0.5.1 in step (`check-brand.mjs` → `version 0.5.1 verified`, `check_plugin.py` 0 errors, `claude plugin validate --strict` passed for marketplace and plugin) |
| Gate | `./scripts/check.sh` exit 0; 242 Rust tests passed, 2 opt-in Keychain tests ignored. No hosted dispatch (nightly policy) |
| macOS | `build_macos.py --arch universal --notary-profile …` exit 0, [receipt](build-0.5.1-macos-universal.json): app and CLI `x86_64 arm64`, clean source `2f4e429`, notary `fb3aa43b-fe74-4d4e-af62-30334493acdb` **Accepted**, staple validated, Gatekeeper accepted |
| Windows | `build_windows_cross.py` exit 0, [receipt](build-0.5.1-windows-x64.json); unsigned, native run NOT_RUN |
| GitHub | tag `v0.5.1-beta.1` = `2f4e429`, [prerelease](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.1-beta.1) with 2 ZIPs, 2 receipts, `SHA256SUMS-0.5.1.txt`; [publication receipt](publication-0.5.1.json) |
| Anonymous download | no token: both archives `OK` against the sums; downloaded app `accepted, source=Notarized Developer ID`; CLI `switchboard 0.5.1` |
| Website | `passioncode-ai.github.io` `852cbeb` ([PR #28](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/28), check passed), deployed (Worker version `66574e15-…`); `/switchboard/download/macos` and `/windows` → 302 to the 0.5.1 archives, fetched macOS hash equal; page and `release.json` `0.5.1-beta.1` |
| Local install | 0.5.0 quit and kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.0-2026-10-03/` (rollback: move it back); 0.5.1 from the verified anonymously downloaded ZIP; `CFBundleShortVersionString` 0.5.1, `spctl` accepted |

#### On the operator's Mac after install (2026-10-03, 01:32 onward)

| Check | Result |
|---|---|
| Keychain dialogs | `log show --predicate 'process == "SecurityAgent"'` since the install: **0** entries |
| Monitor | `switchboard --json rotation status`: running; `claude_swap_accounts` **14**, so Claude Swap (running on this Mac) is detected and its accounts are followed, not renewed; `sign_in_required` 0, `limited` 0 |
| Journal since the install | `account_updated success` ×2 (newer generations taken from Claude Swap) and `usage observed` ×10; no failure |
| Backups | newest written at launch: 15 accounts, 0 missing, own, openable; 10 kept |

No switch of the operator's Claude Code was made for this check. Switching with live accounts beside a running Claude Swap remains SB-15.

### Not run

Live provider acceptance (board SB-01, SB-15), Windows on a Windows host (SB-02), and a running
Claude Code picking up a switch without restart (SB-06).

## 0.5.2

Report rows #16 and #20 ([PLAN-0.5 §0.5.2](../PLAN-0.5.md#052--the-last-rows-of-the-comparison)); #19 → SB-20, #21 → SB-06, #22 declined.

| Command | Result |
|---|---|
| `./scripts/check.sh` | exit 0 |
| `cargo test --workspace` (inside the gate) | 245 passed, 0 failed, 2 ignored |
| `cargo llvm-cov --workspace --summary-only` | 86.21 % of lines (`refresh.rs` 96.13 %, `external.rs` 84.26 %) |
| planted defect: `invalid_client` classified as transient | `a_refused_client_holds_every_renewal_without_blaming_an_account`, `a_refused_client_after_the_hold_is_tried_once_more` failed; restored |

The renewal-refused caption was not rendered in a browser. It is one conditional `usage-caption`
beside the Claude Swap caption, and `npm run build` type-checks it.

### Release v0.5.2-beta.1 (2026-10-03)

| Step | Result |
|---|---|
| Source | `main` `e4c2a0d` (merge of [#21](https://github.com/passioncode-ai/fabric-switchboard/pull/21)); built in a detached clean worktree. A first build in the working checkout refused attestation (`Source changed during build`) because a branch was switched under it; it was discarded, not shipped |
| macOS | `build_macos.py --arch universal --notary-profile …` exit 0, [receipt](build-0.5.2-macos-universal.json): `x86_64 arm64`, clean source `e4c2a0d`, notary `d6a85a26-ca18-4f15-bbf3-80e4c91892b5` **Accepted**, staple validated |
| Windows | `build_windows_cross.py` exit 0 from the same worktree, [receipt](build-0.5.2-windows-x64.json); unsigned, native run NOT_RUN |
| GitHub | tag `v0.5.2-beta.1` = `e4c2a0d`, [prerelease](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.2-beta.1) with 2 ZIPs, 2 receipts, `SHA256SUMS-0.5.2.txt`; [publication receipt](publication-0.5.2.json) |
| Anonymous download | both archives `OK` against the sums; app `accepted, source=Notarized Developer ID`; CLI `switchboard 0.5.2` |
| Website | `passioncode-ai.github.io` `4f68dd7` ([PR #30](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/30), check passed; merged on GitHub because agent-sync could not check out `main`, which another worktree holds), deployed; download routes 302 to the 0.5.2 archives, fetched hash equal, `release.json` `0.5.2-beta.1` |
| Local install | 0.5.1 kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.1-2026-10-03/`; 0.5.2 from the verified download, `spctl` accepted |

On the operator's Mac after install (2026-10-03, 03:12 onward):
- `SecurityAgent`: 0 entries since the install.
- Monitor: running; `renewal_blocked` false; `claude_swap_accounts` 0, because Claude Swap is no
  longer running (no process, no LaunchAgent).
- Backups: 15 accounts, 0 missing, own, openable.
- `sign_in_required` holds **one account**: its rejected lineage is in `renewal-state.json`, and
  usage health is failed since about 02:54. The provider answered `invalid_grant` for that token.
  - Hypothesis, not verified: Claude Swap renewed this account after Switchboard's last pass and
    then stopped, so Switchboard renewed from a token that was already spent. Verifying it would
    mean reading Claude Swap's credential store, which tests must not do (AGENTS.md).
  - Board SB-21 records it.
  - Recovery is the operator's: *Sign in again* on that row, or a Claude Swap re-import if its
    backup holds a newer sign-in.

## 0.5.3

Scope: [PLAN-0.5 §0.5.3](../PLAN-0.5.md#053--sessions-are-never-lost-the-app-stays-quiet), REQ-51…REQ-67. Synthetic fixtures only; nothing read or wrote the real Keychain, `~/.claude*`, `~/.codex` or `~/.claude-swap-backup`.

### Checks run (macOS 26.6, arm64, 2026-10-03)

| Command | Result |
|---|---|
| `./scripts/check.sh` | exit 0 |
| Rust tests (inside the gate) | 270 passed, 0 failed, 2 ignored |
| `node scripts/check-error-vocabulary.mjs` | 238 backend messages: 83 verbatim, 140 mapped, 15 allowlisted |
| `cargo llvm-cov --workspace --summary-only` | 86.41 % of lines (`refresh.rs` 96.86 %, `backup.rs` 93.00 %, `monitor.rs` 87.22 %, `lib.rs` 84.82 %, `external.rs` 84.18 %) |

### Audits and review

| Pass | Findings | Outcome |
|---|---|---|
| Token custody (read-only, 3 findings proven by scratch tests) | P1 ×3, P2 ×5, P3 ×6 | all fixed (REQ-52…60) except the persisted successor, declined with reason |
| Desktop architecture, measured on the running 0.5.2 app: 30 `security` + 1 `ps` processes in 200 s | P1 ×3, P2 ×4, P3 ×4 | fixed: REQ-61…65; the 1.5 s sign-in poll makes no IPC without a pending sign-in (left as is); blocking `security` on async threads → SB-23; Windows signing stays SB-03 |
| Review of the 0.5.3 diff | P2 ×3, P3 ×6 | all fixed (REQ-67) |

### Planted-defect checks

Each of the following guards was removed in place, the named test was run, and the source was
restored. Every one failed:

- live sync newer-copy guard;
- rotation exclusion of rejected sign-ins;
- import newest-generation rule;
- capture lineage refusal;
- a failed Swap row keeps its hold;
- offline no-grant;
- restore newest-generation rule;
- email-scoped holds;
- stopped-aware catch-up.

The first version of the stopped-aware catch-up test passed against the planted defect too. It
was rewritten so that it fails.

`a_refused_client_holds_every_renewal_without_blaming_an_account` found a real leak in the new
in-flight marking: a token stayed marked forever after an early return. It was fixed before
commit, with the mark released by a Drop guard.

### Not run

- Live provider acceptance (SB-01, SB-15), and Windows on a Windows host (SB-02).
- The process count and dialog check are repeated on the operator's Mac after install; that
  record follows.

### Release v0.5.3-beta.1 (2026-10-03)

| Step | Result |
|---|---|
| Source | `main` `21d005c` (merge of [#22](https://github.com/passioncode-ai/fabric-switchboard/pull/22)), built in a detached clean worktree |
| macOS | `build_macos.py --arch universal --notary-profile …` exit 0, [receipt](build-0.5.3-macos-universal.json): `x86_64 arm64`, clean source `21d005c`, notary `be798b70-863f-4ba6-b29e-c8be73221675` **Accepted**, staple validated |
| Windows | `build_windows_cross.py` exit 0 from the same worktree, [receipt](build-0.5.3-windows-x64.json); unsigned, native run NOT_RUN |
| Size (release profile: LTO, one codegen unit, stripped) | app executable 20.5 MB (0.5.2: 32.9 MB), CLI 10.9 MB (18.1 MB); bundle 31 MB (49 MB) |
| GitHub | tag `v0.5.3-beta.1` = `21d005c`, [prerelease](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.3-beta.1), [publication receipt](publication-0.5.3.json) |
| Anonymous download | both archives `OK` against the sums; app `accepted, source=Notarized Developer ID`; CLI `switchboard 0.5.3` |
| Website | `passioncode-ai.github.io` `8a13743` ([PR #33](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/33), check passed), deployed; routes 302 to 0.5.3, fetched hash equal |
| Local install | 0.5.2 kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.2-2026-10-03/`; 0.5.3 from the verified download, `spctl` accepted |

On the operator's Mac after install (2026-10-03, 12:33 onward):
- **Background processes**, using the desktop audit's method — child processes of the app sampled
  every 0.2 s for 200 s, which is a lower bound:
  - 0.5.3: **1** `security` and no `ps`;
  - 0.5.2: 30 `security` and 1 `ps`.
- **Keychain dialogs:** `SecurityAgent` logged 0 entries since the install.
- **Monitor:** running. `claude_swap_switching` is false and `renewal_blocked` is false. Claude Swap
  is not running. Rotation reads `cooldown`.
- **Activity log:** `usage observed` ×18 and no failure.
- **Backups:** 15 accounts, 0 missing.
- **The account that read *Sign in again*** still does.
  - Since 0.5.3 Switchboard takes newer generations from Claude Swap's files after Swap stops. So
    Swap's files do not hold a newer sign-in for this account, and the hypothesis recorded under
    0.5.2 (Swap renewed it last, then stopped) is **not confirmed**.
  - Recovery is the operator's: *Sign in again* on that row.

### Release v0.5.3-beta.2 (2026-10-04)

The first release built, signed and published by GitHub Actions. Same app version (0.5.3) and
signing team (`KJ35UYYL22`) as 0.5.3-beta.1, so the Keychain's trust carries over; the Windows
fixes found by the CI rehearsals (#27, #28, #29).

| Step | Result |
|---|---|
| Source | `main` `3123f5f` ([#32](https://github.com/passioncode-ai/fabric-switchboard/pull/32): the `0.5.3-beta.2` notes), tag `v0.5.3-beta.2`; `release_preflight.py --tag v0.5.3-beta.2 --publish true` ok |
| Release run | [37157469157](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37157469157), dispatched with `publish=true` (a `-beta` tag is published only by dispatch): preflight, macos, windows, publish all success |
| Approval | **both `release` environment gates were approved by an agent (Claude Code) through the maintainer's account, on the operator's explicit instruction of 2026-10-04 to release autonomously**; the approval comment says so. The organization's rule (`working-in-passioncode` §8) says an agent never approves a release run; this run is the recorded exception, and the rule is the operator's to keep or amend |
| Published | 2026-10-03T22:22:40Z, prerelease, six assets: both archives, both receipts, `SHA256SUMS`, `SHA256SUMS.asc` |
| Downloaded set | `shasum -a 256 -c SHA256SUMS` 4/4 OK; `gpg --verify SHA256SUMS.asc` good signature, key `63B3 0DC3 24BD 6974 87AA 3194 4FAF B8AE C803 B6A7`; `gh attestation verify … --signer-repo passioncode-ai/.github` 4/4; the app with a quarantine flag: `spctl` *accepted, source=Notarized Developer ID*, `stapler validate` ok; CLI `switchboard 0.5.3`; macOS receipt: app submission `f36cd8ae-…` Accepted, stapled, Gatekeeper accepted, CLI `3de1106b-…` Accepted; Windows receipt `windows_authenticode: NOT_SIGNED`, commit `3123f5f` |
| Website | `passioncode-ai.github.io` `eb7ffe4` ([PR #34](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/34): the update script reads the release workflow's `SHA256SUMS` and receipt shape, tested), deployed (Worker version `4e1e3c06-…`); `/switchboard/download/macos` and `/windows` 302 to `v0.5.3-beta.2`, the fetched macOS archive's SHA-256 equal to the release's |
| Local install | 0.5.3-beta.1 kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.3-beta.1-2026-10-04/`; beta.2 from the verified download, `codesign --verify --deep --strict` and `spctl` accepted; restarted through the lifecycle broker as owner (`lifecycle.restart`, operation `84db82e4-…`): new pid from `/Applications`, `ready`, `runtime-online` |

### Release v0.5.4-beta.3 (2026-10-05)

0.5.4: the lifecycle contract (SB-27), quota and limit work SB-35…41, SB-42, SB-29, SB-30, SB-44,
SB-45. Same signing team (`KJ35UYYL22`) as 0.5.3, so the Keychain's trust carries over.

| Step | Result |
|---|---|
| Earlier tags | `v0.5.4-beta.1` (run 37236938907) stopped in the Windows preflight: `release_preflight.py` read the notes as cp1252 (`UnicodeDecodeError`, fixed in #43). `v0.5.4-beta.2` (run 37237399471) stopped in the Windows fixtures: `external.rs` used the Unix-only `security_cli` (E0433, SB-45, fixed in #45). Neither published anything |
| Source | `main` `023314c` ([#49](https://github.com/passioncode-ai/fabric-switchboard/pull/49): notes), tag `v0.5.4-beta.3`; `release_preflight.py --tag v0.5.4-beta.3 --publish true --windows-signing false` ok; local gate `./scripts/check.sh` exit 0 (352 Rust tests); `cargo xwin check` and `clippy -D warnings` for Windows clean |
| Release run | [37238681118](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37238681118), dispatched with `publish=true`: preflight, windows, macos, publish all success |
| Approval | both `release` environment gates approved by an agent (Claude Code) through the maintainer's account, on the operator's explicit instruction of 2026-10-04 ("full access, enable everything needed, continue"); each approval comment says so. The organization's rule that an agent never approves a release stays the operator's to keep or amend |
| Published | 2026-10-04T22:23:48Z, prerelease, six assets |
| Downloaded set | `shasum -a 256 -c SHA256SUMS` 4/4 OK (macOS `e91040f8c282241dd65dddb1ef5a7dcd07789854696920e11dc4cf23bd6c9665`, Windows `67d890aafab6b0334f26a3e602d92798880bc45699e6708b8a1471063d7caafa`); `gpg --verify SHA256SUMS.asc` good signature, key `63B30DC324BD697487AA31944FAFB8AEC803B6A7` (imported from `passioncode-ai/.github/release-signing/passioncode-release-signing.asc` into a scratch keyring); `gh attestation verify … --signer-repo passioncode-ai/.github` 4/4; the app with a quarantine flag: `spctl` *accepted, source=Notarized Developer ID*, `stapler validate` ok, `codesign --verify --deep --strict` ok; `LSAppNapIsDisabled` true; CLI `switchboard 0.5.4`; macOS receipt: app `8691c83f-96a5-4619-bc8f-eeda8d700908` Accepted, stapled, Gatekeeper accepted, CLI `07bf92bb-3b10-4dc1-b9cf-2a039474f121`, `native_startup_background` PASS (window hidden); Windows receipt `windows_authenticode: NOT_SIGNED`, commit `023314c` |
| Website | `passioncode-ai.github.io` `d5a88d3` ([PR #38](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/38)), deployed (Worker version `3ce45191-4b8b-4c70-ad58-99df3c014835`); `/switchboard/download/macos` and `/windows` 302 to `v0.5.4-beta.3`; the fetched macOS archive's SHA-256 equals the release's |
| Local install | 0.5.3-beta.2 kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.3-beta.2-2026-10-05/`; 0.5.4 from the verified download, `codesign` and `spctl` accepted |
| Broker | `switchboard.desktop` enrolled for background launch (`policy --background-launch on`, policyRevision 5, broker stopped and started as the runbook says); owner restart `cabaa353-692e-4ef9-badc-1f567868191e` → `ready`: pid 68089 started by the broker with `--background`, `windows: 0`, `runtime-online`, the front app unchanged (Orca); log `~/Library/Logs/Fabric Switchboard/switchboard.log` written (`owner_started`, credential sources available); 0 `SecurityAgent` entries in the 15 minutes after install |
| Idle hour (installed 0.5.4, broker-started with `--background`, no window) | sampled every second for 3600 s: `security`/`ps` children of the app seen in 0 samples over the first 200 s (the SB-34 criterion holds) and in 9 one-second samples over the hour — the app reads a sign-in source only when it changed, and this machine runs about twenty Claude Code sessions that renew their own sign-in, but the log records only state changes (`credential_source` 2 lines), so the 9 cannot be attributed from it; CPU 8.93 s in 3620 s ≈ 0.25 % (≈ 0.24 % without the first four minutes), **above the 0.2 % target** → SB-49; RSS 69.7 MB plus WebKit helpers ≈ 56 MB (≤ 250 MB target holds); 0 `SecurityAgent` entries in 70 minutes |

### Release v0.5.5-beta.1 (2026-10-05)

0.5.5: compact account cards with a status mark (#54), adaptive quota cadence SB-48 (#55),
residency SB-28 — hide on close, tray, login item (#56), anonymous usage analytics SB-50 (#57),
`@tauri-apps/api`/`cli` 2.12.1 to match the tauri 2.12 crate the autostart plugin needs (#58).
Same signing team (`KJ35UYYL22`).

| Step | Result |
|---|---|
| Source | `main` `51ffa11` ([#58](https://github.com/passioncode-ai/fabric-switchboard/pull/58)), tag `v0.5.5-beta.1`; `release_preflight.py --tag v0.5.5-beta.1 --publish true` ok; local gate `./scripts/check.sh` exit 0; `npm run app:build` ok and `smoke_native.py` PASS for the local bundle: ordinary (window visible, tray present) and `--background` (window hidden, tray present, front app unchanged) |
| Found before tagging | the first local `npm run app:build` refused mismatched packages (crate tauri 2.12.1, npm `@tauri-apps/api` 2.11.1) — `tauri-plugin-autostart` 2.7 requires tauri ^2.12, so #56 had moved the crate; aligned in #58. The release job would have failed the same way |
| Release run | [37256039225](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37256039225), `publish=true`: preflight, windows, macos, publish all success |
| Approval | both `release` environment gates approved by an agent (Claude Code) on the operator's explicit instruction (2026-10-05: «агент может делать релизы если попросили»; DISTRIBUTION.md and the organization's rule), comment "approved on the operator's explicit instruction" |
| Published | 2026-10-05T02:52:04Z, prerelease, six assets |
| Downloaded set | `shasum -a 256 -c SHA256SUMS` 4/4 OK (macOS `8be804b18a3012dd7b76272dd45aa9c802d147191cc577a1f9ce3fa664f9836a`, Windows `c5b5a0e9d90f6c3d029badd0fa27e2276e485f62e7df534d6e7b4393e8e34cc6`); `gpg --verify SHA256SUMS.asc` good signature from the organization's key `63B30DC324BD697487AA31944FAFB8AEC803B6A7`; `gh attestation verify --signer-repo passioncode-ai/.github` exit 0 for both archives; `spctl -a -vv` on the quarantined app → `accepted, source=Notarized Developer ID`; `stapler validate` ok; receipt `notarization.status` Accepted (`6290bff3-d06e-45b5-9c40-d28744870bea`). The release binary carries the analytics App Key and host (`strings`, value not printed) |
| Website | `passioncode-ai.github.io` `d7ca69d` ([PR #39](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/39)): manifest, checksums, facts, and a FAQ disclosing the anonymous usage counts; deployed (Worker version `d6247657-05d2-4f7e-9d19-8f7f3a7f3b95`); `/switchboard/download/macos` and `/windows` 302 to `v0.5.5-beta.1` |
| Local install | 0.5.4-beta.3 kept at `$HOME/DATA/_archive/switchboard-installed-rollback-0.5.4-beta.3-2026-10-05/`; 0.5.5 from the verified download, Gatekeeper accepted |
| Broker | Switchboard stopped through the broker (owner `lifecycle.stop`, `stopped`); broker stopped; `switchboard.desktop` → `on_demand`, no idle stop, background launch kept (policyRevision 6) so a Quit stays a Quit (SB-28); broker started; `ensure_running` → `ready`: pid 31780 with `--background`, front app unchanged |
| Residency | LaunchAgent `~/Library/LaunchAgents/ai.passioncode.fabric-switchboard.plist` written at the first start (`ProgramArguments` app + `--background`, `RunAtLoad`); log `login_item enabled`; System Events: 0 windows, 1 menu-bar item |
| Analytics | `PassionCode/installation.json` created (version 1, UUID id, analytics on); log `analytics_flush sent` ×2 (3 events); `sshlg-analytics/scripts/stats.sh 1 "Fabric Switchboard"`: release 3 events, 1 session, 1 user |
