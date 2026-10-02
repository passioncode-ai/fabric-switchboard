# Evidence — 0.5 prompt-free switching, renewal, one-click accounts (unreleased)

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
