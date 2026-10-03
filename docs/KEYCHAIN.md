# Keychain storage on macOS — decision record

Status: decided and implemented 2026-10-01. Code: [keychain.rs](../crates/switchboard-core/src/keychain.rs)
(policy), [keychain_macos.rs](../crates/switchboard-core/src/keychain_macos.rs) (Keychain calls),
[vault.rs](../crates/switchboard-core/src/vault.rs) (`NativeVault`). Operation and recovery:
[OPERATIONS.md](OPERATIONS.md#data-locations-and-ownership).

## The defect

Operator report, 2026-10-01: macOS asked about twenty times whether "Fabric Switchboard" may use
information stored in `ai.passioncode.fabric-switchboard`. "Always Allow" did not stop it.

Cause in code (base `4e42584`): `NativeVault::put` wrote with
`security_framework::passwords::set_generic_password(SERVICE, id, &data)`
(`crates/switchboard-core/src/vault.rs:76` at that commit). That call sets no access list, so
Keychain uses the default one, which trusts only the calling executable — Apple, `SecAccessCreate`:
"If you don't explicitly create and set an access instance when you create a protected keychain
item, keychain services uses a default access like this one … If you set `trustedlist` to `nil`,
the list of trusted apps contains only the calling app."

Keychain recognises an executable by its designated requirement. The app and the CLI it carries
have different ones: the app is `identifier "ai.passioncode.fabric-switchboard"`, the bundled CLI
`identifier switchboard` (`scripts/build_macos.py:167` signs it without `--identifier`), and a
development build's requirement is its `cdhash`, which changes on every build. So each item could
be read silently only by the executable that wrote it. Every other one — the CLI that agents start
for `switchboard mcp`, a rebuilt development app, the app after the CLI wrote an item — asked, once
per item per process.

Measured on this Mac (ACL metadata only, `security dump-keychain -a`, no secret read): 6 items, each
trusting a release bundle built in a since-deleted worktree (`cdhash`), the 0.3.2 artifact app
(Developer ID requirement) and `target/debug/switchboard` (`cdhash`). The bundled CLI of the
installed 0.4.0-beta.1 is in none of them.

## Decision

1. **File-based login keychain, with an explicit access list.** Every item Switchboard creates
   carries a `SecAccess` (`kSecAttrAccess`) whose trusted applications are the caller, the running
   app bundle and its `Contents/MacOS/switchboard`, and `/Applications/Fabric Switchboard.app` and
   its CLI — each only if present and signed by the release team (`anchor apple generic and
   certificate leaf[subject.OU] = "KJ35UYYL22"`; a fork sets `SWITCHBOARD_SIGNING_TEAM` at build
   time). Keychain stores each as a designated requirement, so a later update signed by the same
   team with the same identifier matches without a prompt, wherever it is installed.
2. **Namespaces by build.** A team-signed build uses `ai.passioncode.fabric-switchboard.shared`. Any
   other build (ad hoc, unsigned, another team) uses `ai.passioncode.fabric-switchboard.development`
   and never reads, moves or deletes the user's items.
3. **Ordinary operations never show a dialog.** Reads, writes and deletes run with
   `SecKeychainSetUserInteractionAllowed(false)`; Apple: "keychain services functions that
   normally display a user interface will instead return an error." A refusal becomes a message
   naming the next step. The switch is process-wide, so all Keychain operations of the vault run
   under one lock.
4. **One-time move of legacy items**, described below. Only the desktop app (`NativeVault::desktop`,
   `Owner::desktop`) may ask; the CLI, the MCP server and `serve` (`NativeVault::new`) never do.

### Alternative rejected: the data protection keychain

Apple recommends the data protection keychain by default (TN3137: "Default to targeting the data
protection keychain"). It does not fit this product yet:

- Its access groups come from entitlements that "must be authorized by a provisioning profile.
  Your program needs an app-like bundle structure in which to embed that profile. This is standard
  for app and app extensions but not for command-line tools" (TN3137). TN3125: "the
  `keychain-access-groups` entitlement must be authorized by a profile"; "A standalone executable
  can't claim a restricted entitlement because there's no place to embed the provisioning profile";
  "macOS supports provisioning profiles for both App Store and Developer ID distribution."
- The CLI agents launch is a standalone executable in `Contents/MacOS`. Using that keychain means
  registering an App ID, creating a Developer ID provisioning profile in the developer portal (an
  operator action), embedding it, and wrapping the CLI in its own app-like bundle — which moves the
  path that `switchboard mcp` registrations and the `~/.local/bin` link point to.
- Existing items live in the file-based keychain and are read from there regardless ("Programs
  that read existing items in a file-based keychain must target the file-based keychain", TN3137).

The file-based keychain is "on the road to deprecation … not officially deprecated", and
`SecAccessCreate`/`SecTrustedApplicationCreateFromPath` are marked deprecated since macOS 10.10 yet
remain the documented way to set `kSecAttrAccess`. Moving to the data protection keychain is a
follow-on with its own operator step; the namespace-and-move mechanism here is what it would reuse.

## Moving items written by earlier versions

On a read, a signed build looks in `…shared` first. If the item is absent there and present under
the legacy service `ai.passioncode.fabric-switchboard`:

1. Read the legacy item without a dialog. Items written by 0.3.2 or later releases trust the app's
   designated requirement, which the installed app matches, so for them this step is silent.
2. If Keychain wants consent: the desktop app asks once (the macOS dialog), the CLI returns
   "This account was saved by an earlier Switchboard. Open the Fabric Switchboard app once…".
   A declined dialog is not repeated until the app restarts.
3. Create the shared item with the access list above, read it back and compare. On any failure the
   new copy is removed and the original is left untouched — a credential is never lost.
4. Only then delete the original, without a dialog. If Keychain wants a second consent for that
   (after "Allow" rather than "Always Allow"), the stale copy stays; it is never read again, and is
   removed when the account is removed in the app.

At most one dialog per legacy item, in the app only; none for items that already trust the app.
Removing an account deletes both copies; a legacy copy that needs consent is removed from the app
(one dialog, the removal itself being the request) and refused from the CLI with a message.

## Evidence (2026-10-01, macOS 26.6.2)

| Claim | How it was measured | Result |
|---|---|---|
| Untrusted read does not prompt when interaction is off | probe binaries in a throwaway keychain (`security create-keychain` in a temp dir, deleted after) | read → `-25293`, delete → `-25244`, no dialog; data update by an untrusted binary succeeds |
| An access list grants a second executable | same, ad hoc binaries A, B, C; item trusts A and B | B reads, C refused |
| Trust is by designated requirement, not path or build | two different builds signed with the same Developer ID and identifier, a third with another identifier | second build reads an item trusting only the first, also after the first was moved away; third refused |
| The shipped code trusts app + CLI | test binary signed as a bundle (`Probe.app`, main + `Contents/MacOS/switchboard` with a different identifier) running `signed_bundle_acceptance` | writer `build=Signed trusted_siblings=4`; bundled CLI reads with no dialog; same-team binary outside the bundle refused |
| The gate checks the access list | `created_items_trust_the_listed_executables_by_requirement` | planted defect (item written without `kSecAttrAccess`) fails it: "the sibling must be in the item's access list" |
| The move keeps the original until verified | `failed_or_unverified_copy_keeps_the_original` | planted defect (delete before verify) fails it |
| The CLI never asks | `cli_never_asks_and_names_the_app_as_the_way_out`, `delete_removes_both_copies_and_asks_only_in_the_app` | planted defect (any process may ask) fails both |

Not measured: the installed app moving the operator's real items (the instruction was not to run
it against them); that is the post-release acceptance step below.

### Running the signed-bundle acceptance by hand

Needs the Developer ID identity; never part of `./scripts/check.sh`.

```sh
cargo test -p switchboard-core --lib --no-run      # note the unittests path it prints
# lay it out as Probe.app/Contents/MacOS/{fabric-switchboard,switchboard} with an Info.plist,
# codesign the CLI with its own --identifier, then the bundle; create a throwaway keychain:
security create-keychain -p synthetic-test-only "$TMP/acceptance.keychain-db"
SWITCHBOARD_ACCEPTANCE_KEYCHAIN="$TMP/acceptance.keychain-db" SWITCHBOARD_ACCEPTANCE_ROLE=write \
  Probe.app/Contents/MacOS/fabric-switchboard --ignored --exact keychain_macos::tests::signed_bundle_acceptance
# then ROLE=read from Probe.app/Contents/MacOS/switchboard, ROLE=refused from a team-signed copy
# outside the bundle; finally `security delete-keychain` and remove the signed copies.
```

### Acceptance after the next release (operator)

Install the release, open the app once, then `security dump-keychain -a
~/Library/Keychains/login.keychain-db` (ACL metadata only): each account appears under
`ai.passioncode.fabric-switchboard.shared` with both `ai.passioncode.fabric-switchboard` and
`switchboard` requirements, and `switchboard mcp` from an agent shows no dialog.

**Run 2026-10-01 with v0.4.1-beta.1** ([release record](evidence/release-0.4.1.md#local-installation-and-keychain-acceptance-sb-12)):
before the first launch 6 items under the legacy service, trusting two `cdhash` builds and the
team-signed app but not the CLI; after it 0 legacy items and 11 under `….shared` (the 6 moved plus
5 accounts added in the app meanwhile), every one trusting both the team-signed
`ai.passioncode.fabric-switchboard` and `switchboard` requirements. The installed CLI's `status`,
`usage`, `current`, `accounts list` and a `switchboard mcp --read-only` round trip ran with no
`SecurityAgent` process. Whether the move showed a dialog on screen is the operator's observation;
the legacy items already trusted the app, so step 1 predicts none.

## Sources (fetched 2026-10-01)

- [TN3137: On Mac keychain APIs and implementations](https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains) (revision history to 2026-09-24)
- [TN3125: Inside Code Signing: Provisioning Profiles](https://developer.apple.com/documentation/technotes/tn3125-inside-code-signing-provisioning-profiles)
- [Signing a daemon with a restricted entitlement](https://developer.apple.com/documentation/xcode/signing-a-daemon-with-a-restricted-entitlement)
- [keychain-access-groups entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/keychain-access-groups)
- [SecAccessCreate](<https://developer.apple.com/documentation/security/secaccesscreate(_:_:_:)>), [SecTrustedApplicationCreateFromPath](<https://developer.apple.com/documentation/security/sectrustedapplicationcreatefrompath(_:_:)>), [kSecAttrAccess](https://developer.apple.com/documentation/security/ksecattraccess)
- [SecKeychainSetUserInteractionAllowed](<https://developer.apple.com/documentation/security/seckeychainsetuserinteractionallowed(_:)>), [kSecUseDataProtectionKeychain](https://developer.apple.com/documentation/security/ksecusedataprotectionkeychain)

## Related items added in 0.5

Not account storage, and outside the design above: the backup key `ai.passioncode.fabric-switchboard.backup-key` / `v1` and Claude Code's own `Claude Code-credentials` items are reached only through `/usr/bin/security`, so they trust that executable rather than the app ([PLAN-0.5](PLAN-0.5.md), [OPERATIONS](OPERATIONS.md)).

## Items other programs own (0.5.3)

Which path reads an item depends on who created it, because an item trusts its creator:

| Item | Created by | Read by Switchboard through | Why |
|---|---|---|---|
| `Claude Code-credentials` (+ a custom-home suffix) | Claude Code via `/usr/bin/security` | `/usr/bin/security` | the item trusts that executable; no dialog |
| backup key `ai.passioncode.fabric-switchboard.backup-key` | Switchboard via `/usr/bin/security` | `/usr/bin/security` | same |
| `Codex Auth` (`cli|<hash>`, Codex `keyring`/`auto` mode) | Codex through its own keyring library | Security.framework with user interaction **off** (`switchboard_core::external_keychain::read_external_quietly`) | the item trusts Codex only; `/usr/bin/security` would make macOS ask on every read, and the background read it every minute. With interaction off an untrusted item reads as refused — *Keychain does not let Switchboard read this sign-in without asking. Use official sign-in to add the account.* — and nothing is shown (`codexs_keychain_item_is_read_only_the_quiet_way`) |

Each `/usr/bin/security` read is a process. Claude Code's item is read twice per capture (a torn
config/credential pair is refused), so the monitor reuses one capture for 30 seconds and reads
nothing of Claude Code's when no Claude OAuth account is saved ([OPERATIONS](OPERATIONS.md#053-quiet-by-design)).
