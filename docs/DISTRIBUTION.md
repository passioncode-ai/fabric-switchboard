# Native build and signing

Installing a downloaded ZIP? Start with [INSTALL.md](INSTALL.md). This document is for building and signing artifacts.

This is the procedure for the native artifacts. Current execution receipts are in [0.4.1 evidence](evidence/release-0.4.1.md) (submission Accepted, stapled, Gatekeeper accepted); [0.4.0](evidence/release-0.4.md) was the first release run with `--notary-profile`; [0.3.1](evidence/release-0.3.1.md) and [0.2](evidence/release-0.2.md) evidence are historical; commands here are not by themselves evidence of completion.

## macOS

Build prerequisites: Node/npm, Rust Apple Silicon + Intel targets, Xcode Command Line Tools, installed Developer ID Application certificate and its private key. No certificate is exported to GitHub.

```sh
npm ci
python3 scripts/build_macos.py --identity 'Developer ID Application: YOUR NAME (YOURTEAM)' --arch universal --notary-profile switchboard-notary
```

For local engineering builds while Apple authorization is unavailable, explicitly add `--allow-unnotarized` instead of `--notary-profile`. The default refuses to produce a public signed-only build. If selected Xcode is unusable, the script checks and uses installed Command Line Tools only for its child processes; no global setting or license acceptance is changed.

This builds app plus `switchboard` CLI, combines the CLI architectures with lipo, signs both with hardened runtime and trusted timestamp, then verifies signatures strictly. Output lives in ignored `artifacts/`, with a sibling JSON receipt containing source commit and SHA-256. A clean source tree is required and the exact source SHA is captured before compilation, then checked again. Both executables are checked with lipo. The signed desktop must also pass `scripts/smoke_native.py`: bundled UI renders, native snapshot/status/current-account/monitor IPC completes and the app emits a versioned readiness marker before exiting. This mode uses an empty temporary store, memory vault and stub current identity; no provider auth is read. Existing output folders, ZIPs or receipts are refused to preserve previous artifacts.

If a **saved** notarytool profile exists, add `--notary-profile PROFILE_NAME`. The tool submits the archive to Apple, waits for `Accepted`, staples/validates the app ticket and repackages. CLI notarization is assessed online using the notarized code hash; an individual Mach-O does not accept an app staple. Submission ID/status are saved before waiting. Resume a retained signed archive with `python3 scripts/build_macos.py --notarize-existing artifacts/RECEIPT.json --notary-profile PROFILE_NAME`; the archive hash is checked first. No Apple password or key is a script argument. Do not call a signed-only archive notarized or claim Gatekeeper acceptance without an actual assessment.

The script intentionally has no automatic certificate creation, Keychain policy alteration or signing-key export. A requested signer that is not installed is an explicit blocker. The certificate subject and team are part of the receipt, never guessed from a nickname.

## Windows

The manual `Build Windows artifacts` workflow builds a specified exact commit on `windows-latest`, runs native storage/runtime/CLI fixtures, builds the CLI and NSIS desktop installer, and uploads an artifact with checksums and receipt. Trigger only when a Windows build is requested:

```sh
gh workflow run build-windows.yml --ref main -f expected_sha="$(git rev-parse HEAD)"
```

This example targets `main` after the public-launch task creates it and makes it the default branch; do not dispatch before that integration. The input is compared to the checked-out SHA. No push or PR trigger is installed. Download the produced Actions artifact; it contains `switchboard.exe`, the NSIS installer, SHA256SUMS.txt, LICENSE and THIRD_PARTY_NOTICES.md. The installer uses current-user installation and official WebView2 bootstrapper handling; it does not require a system-wide credential store.

Windows Authenticode signing is a separate prerequisite; an Apple Developer ID cannot sign Windows executables. Unless the receipt says otherwise, these Windows artifacts are unsigned. Native fixture success does not claim actual Claude/Codex login or Windows UI acceptance.

### Local fallback when a Windows runner is unavailable

The 0.2 hosted run was blocked by organization billing before any step. The engineering artifacts were instead built with the [cross-build script](../scripts/build_windows_cross.py), following Tauri's documented NSIS fallback. On this Mac the measured tools are cargo-xwin 0.23.1, LLVM 23.1.2 and NSIS 3.12:

```sh
brew install llvm nsis
uv tool install cargo-xwin==0.23.1
rustup target add x86_64-pc-windows-msvc
npm ci
python3 scripts/build_windows_cross.py
```

The script adjusts PATH only for its child processes and lets cargo-xwin use its resolved linker, including Rust's bundled rust-lld. Microsoft SDK/CRT cache is kept in ignored `artifacts/xwin`. It builds a current-user NSIS installer and a separate x64 CLI with static CRT, checks PE architecture and CLI imports, then writes a ZIP, checksums and source receipt. It refuses an existing folder, ZIP or receipt; preserve all three before rebuilding the same version. No Wine execution or native Windows test is implied by cross-build success. A native Windows runner and interactive acceptance remain required before claiming platform support.

## Sources

The implementation procedure follows [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/), [Tauri Windows installer support](https://v2.tauri.app/distribute/windows-installer/) and [Apple Developer ID distribution](https://developer.apple.com/developer-id/). Platform protection is based on native Windows APIs; [DPAPI current-user behavior](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) distinguishes user-bound protection from machine-wide decryption.

## Configure Apple notarization locally

The installed Developer ID certificate signs code; it does not authenticate submissions to Apple. In your own Terminal, use the interactive prompt (never paste an Apple password into chat):

```sh
xcrun notarytool store-credentials switchboard-notary --team-id KJ35UYYL22
```

Enter the Apple ID and its app-specific password in that local prompt. Alternatively configure an authorized App Store Connect API key profile locally. Then supply only the profile name `switchboard-notary` to the build/resume command. A release can be called notarized only after Accepted, staple validation and Gatekeeper assessment succeed.
