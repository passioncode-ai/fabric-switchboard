# Keychain dialogs: shared trust for app and CLI — 2026-10-01

## Objective

Stop the repeated macOS dialog "Fabric Switchboard wants to use your confidential information
stored in ai.passioncode.fabric-switchboard", which "Always Allow" did not end, without losing a
saved account.

## Done

- Cause found and recorded in [KEYCHAIN.md](../KEYCHAIN.md#the-defect): items were written with
  Keychain's default access list, which trusts only the executable that wrote them; the app and its
  bundled CLI have different designated requirements, and development builds a new one per build.
- Decision in [KEYCHAIN.md](../KEYCHAIN.md#decision): file-based login keychain with an explicit
  access list trusting the app and the bundled CLI by designated requirement; namespaces
  `ai.passioncode.fabric-switchboard.shared` (team-signed) and `….development` (everything else);
  no dialog during ordinary use; the desktop app moves legacy items, asking at most once per item.
  The data protection keychain was considered and deferred (provisioning profile, CLI bundle):
  board row SB-13.
- Code: [keychain.rs](../../crates/switchboard-core/src/keychain.rs) (policy and its tests on a fake
  Keychain), [keychain_macos.rs](../../crates/switchboard-core/src/keychain_macos.rs) (Keychain calls,
  throwaway-keychain tests, the hand-run signed-bundle acceptance), `NativeVault::desktop` and
  `Owner::desktop` (used by the app only), and the store now shows the vault's actionable messages
  instead of "Credential storage unavailable".
- Docs: [OPERATIONS](../OPERATIONS.md#data-locations-and-ownership) storage row and Keychain recovery,
  [SCN-027](../ux/scenarios.md#scn-027--keep-saved-accounts-readable-without-repeated-keychain-dialogs).

## Checks actually run

- `./scripts/check.sh` exit 0: 134 Rust tests passed, 2 ignored (the opt-in login-keychain test and
  the new signed-bundle acceptance).
- Planted defects, each watched failing and then reverted: item written without `kSecAttrAccess`
  (fails `created_items_trust_the_listed_executables_by_requirement`); legacy item deleted before
  the copy is verified (fails `failed_or_unverified_copy_keeps_the_original`); any process allowed
  to ask (fails `cli_never_asks_and_names_the_app_as_the_way_out` and
  `delete_removes_both_copies_and_asks_only_in_the_app`).
- Live measurements in throwaway keychains, Developer ID signed probe copies, all deleted after:
  [KEYCHAIN.md → Evidence](../KEYCHAIN.md#evidence-2026-10-01-macos-2662).
- Not run: the installed app against the operator's real items (instructed not to). No release.

## Open

- **Release** (not cut): the macOS release flow is a hand-run script with the operator's Developer
  ID key and notarytool profile, plus the site repository and its Worker; it is not automated. The
  steps are below.
- After the release, the acceptance in [KEYCHAIN.md](../KEYCHAIN.md#acceptance-after-the-next-release-operator):
  board row SB-12.

## Human steps — release 0.4.1-beta.1 (operator)

Same flow as [release-0.4.md](../evidence/release-0.4.md):

1. Version bump to 0.4.1 in step: workspace `Cargo.toml`, `package.json` and its lock,
   `src-tauri/tauri.conf.json`, plugin and marketplace entry, the skill's `metadata.version`;
   `node scripts/check-brand.mjs`, `python3 scripts/check_plugin.py`, `./scripts/check.sh` exit 0;
   land on `main` by fast-forward.
2. `python3 scripts/build_macos.py --identity "Developer ID Application: … (KJ35UYYL22)" --arch universal --notary-profile <saved profile>`
   → receipt with notarization `Accepted`; `spctl -a -vv` on the app → `source=Notarized Developer ID`.
3. `python3 scripts/build_windows_cross.py`; write `SHA256SUMS-0.4.1.txt` over both archives.
4. `gh release create v0.4.1-beta.1 --prerelease` with both archives, both receipts and the sums;
   anonymous download and `shasum -a 256 -c` as in the 0.4 record.
5. Site: point the Switchboard page and download redirects at v0.4.1-beta.1 in
   `passioncode-ai.github.io`, deploy its Worker.
6. Install into `/Applications`, open the app once, then run the acceptance in KEYCHAIN.md.

## Exact next task

Cut 0.4.1-beta.1 by the steps above, then close SB-12 with the ACL metadata receipt.
