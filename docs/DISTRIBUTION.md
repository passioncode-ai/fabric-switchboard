# Native build, signing and release

Installing a downloaded ZIP? Start with [INSTALL.md](INSTALL.md). This document is for building, signing and releasing the artifacts.

**Published builds are made only by the [release workflow](../.github/workflows/release.yml)**, in this repository's protected GitHub `release` environment, under the organization's rules ([passioncode-ai/.github release signing](https://github.com/passioncode-ai/.github/blob/main/release-signing/README.md)). A member of `release-approvers` approves every signing run, and that may be the person who pushed the tag (operator decision, 2026-10-03); an agent approves a release run only when the operator asked for that release (operator decisions 2026-10-03 and 2026-10-05: «агент может делать релизы если попросили»; the organization's [rules for agents](https://github.com/passioncode-ai/.github/blob/main/release-signing/README.md#rules-for-agents-and-people)), with the comment "approved on the operator's explicit instruction"; without such a request it starts the run and says where to approve. Administrators cannot bypass the environment. Nobody's laptop holds a release key. A build signed anywhere else (a laptop, a fork, a dispatch on a branch) is a **debug build** and is never published or attached to a release.

Execution receipts of releases before this workflow, built and notarized by hand: [0.5](evidence/release-0.5.md) (the same file also records the workflow-built 0.5.4–0.6.5 releases), [0.4.1](evidence/release-0.4.1.md), [0.4.0](evidence/release-0.4.md); [0.3.1](evidence/release-0.3.1.md) and [0.2](evidence/release-0.2.md) are historical. A command written here is not by itself evidence that it ran.

## How a release happens

1. Merge the release pull request: the version in `package.json`, `src-tauri/tauri.conf.json` and the workspace `Cargo.toml`, and a `## X.Y.Z` section in [CHANGELOG.md](../CHANGELOG.md) that becomes the release notes. While Windows signing is off, that section must say `windows_authenticode: NOT_SIGNED`; the workflow refuses to publish otherwise.
2. Push an annotated tag on the merge commit: `git tag -a vX.Y.Z <merge commit> -m "Fabric Switchboard X.Y.Z" && git push origin vX.Y.Z`.
3. `release.yml` starts. `preflight` checks that the tag names the version being built and that the CHANGELOG section exists. The `macos` and `windows` jobs wait for the `release` environment.
4. Someone from `release-approvers`, whoever pushed the tag included, opens the run and approves (**Review deployments**). The jobs build, sign, notarize and package.
5. `publish` (the organization's [release-publish workflow](https://github.com/passioncode-ai/.github/blob/main/.github/workflows/release-publish.yml), `@v1`) waits for a second approval because it holds the GPG key. It attests every file with Sigstore build provenance, writes `SHA256SUMS` and `SHA256SUMS.asc` (the organization's release key), then creates the release as a draft and publishes it once every file is up.
6. A published release is never rewritten. A wrong release is fixed with a new tag.

**Rehearsal.** Push `vX.Y.Z-rc.N` on the commit (the push trigger ignores `-rc` tags), then `gh workflow run release.yml --ref vX.Y.Z-rc.N -f publish=false`. Everything above runs, approvals included, but no release is created: the signed set and its notes are kept as the workflow artifact `signed-release-vX.Y.Z-rc.N` for 14 days. A Windows build of a commit is made the same way; the manual `build-windows.yml` workflow it replaces is removed.

**Prereleases.** The push trigger starts only for `vX.Y.Z`. A `-beta.N` tag is published by dispatch: `gh workflow run release.yml --ref vX.Y.Z-beta.N -f publish=true`, with the same approvals. The organization's publish workflow (`prerelease: auto`) marks `-alpha`, `-beta` and `-preview` tags as prereleases. `v0.5.3-beta.2` was released this way (run 37157469157).

## What the workflow does

### macOS (`macos` job, `macos-latest`)

1. `passioncode-ai/.github/actions/apple-signing@v1` imports the CI Developer ID Application certificate into a throwaway keychain and outputs its full name. The cleanup action removes the keychain at the end of the job, also on failure.
2. `python3 scripts/build_macos.py --identity "<that name>" --arch universal --external-notarization` builds the universal app and CLI, signs both with hardened runtime and a trusted timestamp (the CLI first, then the bundle that carries a copy of it), verifies the signatures, runs the native smoke test (`scripts/smoke_native.py`) twice — an ordinary start that must show the window, and a `--background` start with an unknown extra argument that must keep it hidden (SB-30) and writes the folder, ZIP and receipt into `artifacts/`. The receipt's notarization status is `PENDING_EXTERNAL`: the caller notarizes.
3. **The team.** The Keychain code trusts only executables signed by the team compiled into it ([KEYCHAIN.md](KEYCHAIN.md)). The job passes `SWITCHBOARD_SIGNING_TEAM` from the `release` environment's `APPLE_TEAM_ID`; the script refuses to build if it differs from the identity's team, and refuses to sign if either executable was compiled without it. No team id is written in the code.
4. `passioncode-ai/.github/actions/notarize@v1` notarizes the app with the App Store Connect API key, requires `Accepted`, staples the ticket and assesses the app with `spctl`.
5. `build_macos.py --finish-external <receipt> --app-submission <id>` checks the staple and Gatekeeper again, then **notarizes the standalone CLI** and repackages the folder.
   - *Decision.* A bare Mach-O executable cannot carry a stapled ticket, and Apple accepts one only inside a ZIP, so the CLI is submitted as its own ZIP. It is a separate submission even though the app's submission carries an identical copy: the receipt then names a submission, and a ticket, for the very file a person downloads. The step requires `Accepted`, then reads the submission's log and requires the ticket to list the code hash (`CDHash`) of **both** architecture slices of the CLI; otherwise it fails and does not call the CLI notarized.
   - On another Mac, Gatekeeper looks the CLI's ticket up online by that code hash the first time a quarantined copy runs.
6. The ZIP (`Fabric-Switchboard-X.Y.Z-macos-universal.zip`: the stapled app, the CLI, README, LICENSE, THIRD_PARTY_NOTICES.md) and its receipt are uploaded as `release-macos`.
7. `scripts/updater_artifacts.py macos` archives the **stapled** app as `Fabric-Switchboard-X.Y.Z-macos-universal.app.tar.gz` (one top-level `Fabric Switchboard.app/`, no AppleDouble members), unpacks it again and requires the copy to pass `codesign --verify --deep --strict`, `stapler validate` and `spctl`, then signs it for the updater (below). Uploaded as `updater-macos`.

The macOS receipt states, among the build facts (commit, toolchain, architectures, identity, certificate SHA-1, `signing_team`, native smoke result):

```json
"notarization": {
  "status": "Accepted",
  "by": "the release workflow (App Store Connect API key)",
  "app": {"submission": "…", "status": "Accepted", "stapled": true, "staple_validated": true, "gatekeeper_accepted": true},
  "cli": {"submission": "…", "status": "Accepted", "stapled": false, "reason_not_stapled": "…", "ticket_cdhashes": ["…", "…"]}
}
```

A receipt is uploaded only after every check above passed; a failed run uploads nothing.

### Windows (`windows` job, `windows-latest`)

1. Checks the release notes again, now that the `release` environment's `AZURE_SIGNING_ENABLED` is visible.
2. Runs the native storage, runtime and CLI fixtures (`cargo test -p switchboard-core -p switchboard-runtime -p switchboard-cli`).
3. Builds the CLI with a static C runtime (`switchboard.exe`) and the desktop executable (`tauri build --no-bundle`).
4. **When `AZURE_SIGNING_ENABLED` is `true`:** `azure/login` signs in with OIDC (no secret: the `release` environment's federated credential), and `Azure/artifact-signing-action@v2` signs the desktop executable and the CLI (SHA-256 digest, RFC 3161 timestamp from `http://timestamp.acs.microsoft.com`). Both actions are pinned by commit.
5. `tauri bundle --bundles nsis` builds the installer `Fabric Switchboard_X.Y.Z_x64-setup.exe` around the (signed) desktop executable and the (signed) CLI, which `src-tauri/tauri.windows.conf.json` → `bundle.resources` installs beside it (SB-05); the next step installs it silently into a scratch folder and fails unless both executables are there and the installed CLI runs. Because `tauri-build` checks resources at compile time, any Windows build of the desktop crate — including a local `cargo xwin check -p fabric-switchboard` — needs `target/x86_64-pc-windows-msvc/release/switchboard.exe` built first (`scripts/test_windows_bundle.py` guards the order); then, when signing is on, the installer itself is signed. The uninstaller NSIS generates inside the installer is not signed: Tauri signs it only through its own `signCommand`, which this workflow does not use.
6. When signing is on, PowerShell `Get-AuthenticodeSignature` reads all three files and the job fails unless every status is `Valid`; the report goes into the receipt.
7. `scripts/updater_artifacts.py windows` copies the final installer — after the last Authenticode pass, so these are the bytes the updater checks — as `Fabric-Switchboard-X.Y.Z-windows-x64-setup.exe` and signs it for the updater. Uploaded as `updater-windows`.
8. `scripts/package_windows.py` packs `Fabric-Switchboard-X.Y.Z-windows-x64.zip` (installer, CLI, README, LICENSE, THIRD_PARTY_NOTICES.md, an inner `SHA256SUMS.txt` and `build-receipt.json`) and writes `Fabric-Switchboard-X.Y.Z-windows-x64-receipt.json`. The receipt says `"windows_authenticode": "SIGNED"` with each file's signer, thumbprint and timestamper, or `"windows_authenticode": "NOT_SIGNED"` with the reason, and the archive's README says the same.

**Today `AZURE_SIGNING_ENABLED` is `false`**: the Azure account does not exist yet (the human steps below). The job builds and uploads unsigned files, and the receipt and release notes say `windows_authenticode: NOT_SIGNED`. Native fixtures passing is not Windows UI acceptance, and no build here claims live-provider acceptance.

### Updater manifest (`updater` job, `ubuntu-latest`)

`scripts/updater_artifacts.py manifest` writes `latest.json` (Tauri's static update format) from the two packages: `darwin-universal`, `darwin-aarch64` and `darwin-x86_64` all name the universal archive, `windows-x86_64` the setup; each entry carries that file's own `.sig` text and a URL under `releases/download/<tag>/`; the notes are the CHANGELOG section. `check` then refuses the run unless every entry names a file of this release under this tag with its own signature and the version equals the tag (`scripts/test_updater_artifacts.py`). The manifest, both packages and both signatures are uploaded as `release-updater`, so the publish workflow attests and sums them with everything else.

## Automatic updates (SB-55)

Every installed copy updates itself; on by default, off in About → *Install updates automatically* (`<data>/auto-update`). The app (`src-tauri/src/updates.rs`) reads `https://github.com/passioncode-ai/fabric-switchboard/releases/latest/download/latest.json` 90 s after start and every six hours. `releases/latest` skips drafts and prereleases, so a `-beta` never reaches installed copies and a release becomes visible only once the publish workflow has put every file up.

- **Trust.** A package is accepted only if its minisign signature verifies against the public key in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` **and** the version that signature names equals the version `latest.json` announces (`requireSignedVersion: true`; every package is signed with `tauri signer sign --app-version`). A tampered manifest therefore cannot pair a new version number with an older genuine package. **Downgrades:** only a version greater than the running one is offered (`updates::offered`, the plugin's own default stated in code and tested by `only_a_newer_version_is_offered`; `allowDowngrades` stays off), and the signed version must equal the announced one, so neither an older release nor an old package under a new number can be installed. The private key and its password live in Project Observatory's vault (`fabric-switchboard/prod/TAURI_SIGNING_PRIVATE_KEY`, `…_PASSWORD`) and, copied from it over stdin, in the `release` environment's secrets of the same names (2026-10-05). Losing the key means no installed copy can be updated again: a new key reaches users only through a manual download.
- **macOS.** The verified version replaces the bundle at once (a swap that has not finished within 5 minutes is logged `update_install timeout`, and no further check starts until the app restarts) (`tauri-plugin-updater` moves the old bundle aside and the new one into place); the running process keeps its code and the next start runs the new version. Nothing is installed on the quit path, where the 10 s hard exit could cut the swap. A bundle the person cannot replace without an administrator password is installed only after *Restart to update*, which asks for it. A copy running from an App Translocation path does not check (“Move Fabric Switchboard to the Applications folder…”).
- **Windows.** The installer starts from the exit event after `Owner::shutdown` (LC-01), on an ordinary quit or *Restart to update*, never after a signal. It always runs the bytes verified when the update became ready. *Restart to update* re-reads `latest.json` only to obtain an installer handle with the relaunch arguments for the window's state, and logs `update_relaunch_args` as `same`, `newer` or `check_failed`; in the last two cases the ready update's own handle relaunches with the arguments the process started with. The updater runs it as `/P /UPDATE`. In the NSIS template of tauri-cli 2.12.1, update mode skips the uninstall step (`installer.nsi` lines 319–321: "In update mode, always proceeds without uninstalling"), the uninstaller's *Delete the application data* removal runs only when `UpdateMode <> 1` (lines 871–884), and the passive mode skips the page that shows that checkbox (`un.SkipIfPassive`). The app data in `%LOCALAPPDATA%\ai.passioncode.fabric-switchboard` therefore survives every update ([source at the tag](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.1/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi)); backups in Roaming are a second line, not a dependency.
- **Relaunch.** *Restart to update* comes back with `--background` when the window was hidden (macOS: `tauri::process::restart` with those arguments; Windows: `/R /ARGS --background`).
- **0.6.1 is the first release that ships the updater.** Copies of 0.6.0 and earlier have none: those users download a newer release once by hand.

## Human steps: Azure Artifact Signing for Windows (operator)

Opening the account is the operator's step: it needs a legal entity, a payment method and an identity validation. Nothing else in the workflow changes when it is done; switching the variable to `true` turns the signing steps on.

1. **The company.** Public Trust identity validation is available to organizations in the EU (also the USA, Canada and the UK); sign as the EU company that publishes PassionCode.ai, with its registered legal name, address and registration or tax number at hand. That legal name becomes the certificate subject users see.
2. **The subscription.** In the Azure portal, under an Entra ID tenant the company controls, create (or choose) a subscription with a payment method, a resource group (for example `rg-release-signing`, in a region that offers Artifact Signing, such as West Europe or North Europe), and register the resource provider `Microsoft.CodeSigning` on the subscription.
3. **The Artifact Signing account.** Create an *Artifact Signing* (formerly Trusted Signing) account in that resource group; the Basic tier is enough. Note its name and its endpoint, which depends on the region: for example `https://weu.codesigning.azure.net/` for West Europe or `https://neu.codesigning.azure.net/` for North Europe.
4. **Identity validation.** Give your own user the *Artifact Signing Identity Verifier* role on the account, then in the account choose **Identity validation → New → Public**, organization, and submit the company's documents. Validation is done by Microsoft and can take several business days; wait for **Completed**.
5. **The certificate profile.** Create a certificate profile of type **Public Trust** bound to the completed identity validation; note its name (for example `passioncode-public`).
6. **The identity CI signs in as.** In Entra ID, create an app registration (for example `fabric-switchboard-release-signing`) and its service principal. Under **Certificates & secrets → Federated credentials**, add one for *GitHub Actions deploying Azure resources*: organization `passioncode-ai`, repository `fabric-switchboard`, entity type **Environment**, environment `release`. That is the subject `repo:passioncode-ai/fabric-switchboard:environment:release`, issuer `https://token.actions.githubusercontent.com`, audience `api://AzureADTokenExchange`. No client secret is created: tokens are issued only to jobs in this repository's `release` environment.
7. **Its permission.** On the Artifact Signing account (or the certificate profile), assign the role *Artifact Signing Certificate Profile Signer* to that service principal, and nothing broader.
8. **The variables.** Set them on the `release` environment (they are identifiers, not secrets):

   ```sh
   R=passioncode-ai/fabric-switchboard
   gh variable set AZURE_CLIENT_ID          --env release -R "$R" --body "<app registration's Application (client) ID>"
   gh variable set AZURE_TENANT_ID          --env release -R "$R" --body "<Directory (tenant) ID>"
   gh variable set AZURE_SUBSCRIPTION_ID    --env release -R "$R" --body "<subscription ID>"
   gh variable set AZURE_SIGNING_ENDPOINT   --env release -R "$R" --body "https://weu.codesigning.azure.net/"
   gh variable set AZURE_SIGNING_ACCOUNT    --env release -R "$R" --body "<Artifact Signing account name>"
   gh variable set AZURE_CERTIFICATE_PROFILE --env release -R "$R" --body "<certificate profile name>"
   ```

   Add the six names to this product's `vars` in the organization's `release-signing/products.json`, so `setup-release-env.py` keeps them.
9. **Switch it on and rehearse.** `gh variable set AZURE_SIGNING_ENABLED --env release -R "$R" --body true`, then rehearse on an `-rc` tag: the `windows` job must show three `Valid` signatures and its receipt `"windows_authenticode": "SIGNED"`. From then on, the CHANGELOG section of a release no longer needs the `NOT_SIGNED` line, and [INSTALL.md](INSTALL.md) should say the Windows build is signed.

## Local builds (debug only)

These stay for debugging and for the cases the workflow cannot run. Their output is never published.

### macOS

Prerequisites: Node/npm, Rust with the Apple silicon and Intel targets, Xcode Command Line Tools, and a Developer ID Application certificate with its private key in your keychain.

```sh
npm ci
python3 scripts/build_macos.py --identity 'Developer ID Application: YOUR NAME (YOURTEAM)' --arch universal --notary-profile switchboard-notary
```

`--allow-unnotarized` in place of `--notary-profile` makes a local engineering build whose receipt says `NOT_RUN`. Without one of `--notary-profile`, `--external-notarization` or `--allow-unnotarized` the script refuses to start. If the selected Xcode is unusable, the script uses the installed Command Line Tools for its child processes only; no global setting or license acceptance changes.

The build itself is the one the workflow runs: a clean source tree is required and its commit is captured before compilation and checked again; both executables are checked with `lipo`; the team is taken from the identity (or must equal `SWITCHBOARD_SIGNING_TEAM` when that is set) and compiled in; the signed desktop must pass `scripts/smoke_native.py` (bundled UI renders, native IPC completes, a versioned readiness marker is printed, the window is visible on an ordinary start and hidden on a `--background` one, with an empty temporary store, a memory vault and no provider auth read). Existing output folders, ZIPs or receipts are refused.

With a **saved** notarytool profile, the script submits the whole ZIP, waits for `Accepted`, staples and validates the app ticket, assesses it with Gatekeeper and repackages. Submission id and status are saved before waiting; resume with `python3 scripts/build_macos.py --notarize-existing artifacts/RECEIPT.json --notary-profile PROFILE_NAME` (the archive hash is checked first). No Apple password or key is ever a script argument. A Keychain notary profile is never used in CI.

To create the profile, in your own Terminal (never paste an Apple password into chat):

```sh
xcrun notarytool store-credentials switchboard-notary --team-id KJ35UYYL22
```

`KJ35UYYL22` is PassionCode.ai's Apple team; use your own team when building a fork. Enter the Apple ID and its app-specific password in that local prompt, or configure an App Store Connect API key profile instead.

The script never creates certificates, changes Keychain access policy or exports a signing key. A requested signer that is not installed is an explicit blocker.

### Windows cross-build fallback

The 0.2 to 0.5.3 Windows archives were cross-built on a Mac with the [cross-build script](../scripts/build_windows_cross.py), following Tauri's documented NSIS fallback (measured tools: cargo-xwin 0.23.1, LLVM 23.1.2, NSIS 3.12):

```sh
brew install llvm nsis
uv tool install cargo-xwin==0.23.1
rustup target add x86_64-pc-windows-msvc
npm ci
python3 scripts/build_windows_cross.py
```

It adjusts PATH for its child processes only, keeps the Microsoft SDK/CRT cache in ignored `artifacts/xwin`, builds a current-user NSIS installer and a static-CRT x64 CLI, checks PE architecture and CLI imports, and writes a ZIP, checksums and a receipt. It refuses existing output. No Wine execution or native Windows test is implied, and the output is unsigned and never published now that the workflow builds Windows natively.

## Sources

[Tauri updater](https://v2.tauri.app/plugin/updater/), [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/), [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/), [Apple Developer ID](https://developer.apple.com/developer-id/), [Apple: customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [Azure Artifact Signing](https://learn.microsoft.com/azure/artifact-signing/), [Artifact Signing GitHub action and OIDC](https://github.com/Azure/artifact-signing-action/blob/main/docs/OIDC.md), [GitHub OIDC with Azure](https://learn.microsoft.com/azure/developer/github/connect-from-azure-openid-connect). Platform protection relies on native Windows APIs; [DPAPI current-user behavior](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) distinguishes user-bound protection from machine-wide decryption.
