# Switchboard 0.4.0 — build and release record

Recorded 2026-09-30. **Published public beta:** [v0.4.0-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.4.0-beta.1) (prerelease). [Anonymous publication receipt](publication-0.4.0.json) records the release ID, the tag's commit and the full-download hash of both archives.

## Source identity

Both artifacts were built from clean source [`bd0cf5daf814cfdc82b3908154d69aa8fac56043`](https://github.com/passioncode-ai/fabric-switchboard/tree/bd0cf5daf814cfdc82b3908154d69aa8fac56043), the version bump to 0.4.0 ([PR #4](https://github.com/passioncode-ai/fabric-switchboard/pull/4)). It landed on `main` by fast-forward, so `main`, the tag `v0.4.0-beta.1` and both build receipts name the same SHA. Later commits (this record) change documentation only.

Versions in step: workspace `Cargo.toml` 0.4.0 (CLI, proxy, runtime, desktop; `switchboard-core` keeps its own 0.1.0), `package.json`/lock 0.4.0, `src-tauri/tauri.conf.json` 0.4.0, plugin and marketplace entry 0.4.0, skill `metadata.version` 0.4.0. `node scripts/check-brand.mjs` → `version 0.4.0 verified`; `python3 scripts/check_plugin.py` → `switchboard plugin: 0 errors` (it compares the package core version with the plugin from 0.4.0).

## Checks

- Local gate on `bd0cf5d`: `npm ci` exit 0; `./scripts/check.sh` exit 0 — 118 Rust tests passed, 1 opt-in synthetic Keychain test ignored; read-deadline 5 cases, ui-logic 9 cases; plugin validator 0 errors and its 23 unit tests OK; notices current; 43 Markdown files, 283 relative links, 0 errors.
- `claude plugin validate . --strict` and `claude plugin validate ./plugins/switchboard --strict` → passed.
- Hosted: [Nightly macOS checks run 36696105070](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/36696105070), dispatched once by hand for this release candidate on `bd0cf5d` → `clock` success, `checks` (macos-14: `./scripts/check.sh`, `npm run app:build`) success. The scheduled nightly runs have skipped `checks` since at least 2026-09-26 ([issue #5](https://github.com/passioncode-ai/fabric-switchboard/issues/5)), so this dispatch is the only hosted run of the suite for this release.
- SB-07 (`project_rules_are_optional_visible_and_applied_on_request` failing once) did not recur: it passed in the local gate and in the hosted run.

## macOS build, signing and notarization

`python3 scripts/build_macos.py --identity "Developer ID Application: Sergei Viktorovich Sheleg (KJ35UYYL22)" --arch universal --notary-profile <saved profile>` → exit 0. [Receipt](build-0.4.0-macos-universal.json):

- App and CLI `x86_64 arm64` (lipo); hardened runtime (`flags=0x10000(runtime)`) and secure timestamp on both; strict `codesign --verify --deep` passed. The CLI is embedded at `Contents/MacOS/switchboard` and signed before the bundle; `cmp` with the archived CLI → equal.
- Native smoke `PASS` (temporary store, memory vault, native IPC, bundled frontend; real provider auth `NOT_READ`).
- Notary submission `573e15d6-50e5-4dae-a487-43eb0080a85a` → **Accepted** (`xcrun notarytool history` lists it). Staple and validate worked; `spctl --assess --type execute` → `accepted, source=Notarized Developer ID`. The stapled app was re-archived; the receipt's `archive_sha256` is the stapled ZIP.

This is the first notarized Switchboard release; 0.3.1-beta.1 and the unreleased 0.3.2 builds were signed only.

## Windows build

`python3 scripts/build_windows_cross.py` → exit 0 on the same commit. [Receipt](build-0.4.0-windows-x64.json): cargo-xwin 0.23.1, LLVM 23.1.2, NSIS 3.12; PE headers checked (`fabric-switchboard.exe` and `switchboard.exe` x64, installer x86), CLI with static CRT and no external VC runtime import. Authenticode **NOT_SIGNED**; native Windows installation and execution **NOT_RUN**.

## Archive checksums

| Archive | SHA-256 |
|---|---|
| `Fabric-Switchboard-0.4.0-macos-universal.zip` | `3ab0f1cf69a9a21f1fd469bfc195c3187e5121ebdfcfb94b36e76c54eec10b6e` |
| `Fabric-Switchboard-0.4.0-windows-x64.zip` | `55bf5d50b2c6701cb582ac1a028fcde013155a3a84fd3d4da339ee3930937f1b` |

The release also carries `SHA256SUMS-0.4.0.txt` and both JSON receipts.

## Newcomer path, from the published release

Run on this Mac with no GitHub token, no netrc and a throwaway `HOME`, so no operator Claude, Codex or Switchboard state was reachable:

| Step | Result |
|---|---|
| `curl -fsSL …/v0.4.0-beta.1/Fabric-Switchboard-0.4.0-macos-universal.zip` and `…/SHA256SUMS-0.4.0.txt` | exit 0 |
| `shasum -a 256 -c SHA256SUMS-0.4.0.txt --ignore-missing` | `Fabric-Switchboard-0.4.0-macos-universal.zip: OK`, exit 0 |
| quarantine set on the ZIP and every extracted file (as a browser download would), `ditto -x -k` | exit 0 |
| `spctl -a -vv "Fabric Switchboard.app"` | `accepted`, `source=Notarized Developer ID`, `origin=Developer ID Application: Sergei Viktorovich Sheleg (KJ35UYYL22)`, exit 0 |
| `spctl -a -vv -t install "Fabric Switchboard.app"` | `accepted`, `source=Notarized Developer ID`, exit 0 |
| `xcrun stapler validate "Fabric Switchboard.app"` | `The validate action worked!`, exit 0 |
| `codesign --verify --check-notarization --verbose=2 switchboard` (the loose CLI; a Mach-O cannot be stapled) | valid, exit 0 |
| `spctl -a -vv -t execute switchboard` | `rejected (the code is valid but does not seem to be an app)`, exit 3 — expected for a bare CLI; the quarantined CLI below ran |
| `./switchboard --version` (quarantined) | `switchboard 0.4.0`, exit 0 |
| `switchboard --data-dir <tmp> mcp` with `initialize`, `tools/list`, `tools/call switchboard_accounts` on stdin | exit 0; protocol `2025-06-18`, server `switchboard 0.4.0`, 8 tools, `switchboard_accounts` → `{"accounts":[]}`, `isError: false` |
| `claude mcp add --scope user switchboard -- switchboard mcp` | exit 0 |
| `claude mcp list` | `switchboard: switchboard mcp - ✔ Connected`, exit 0 |
| plugin, second throwaway `HOME`: `claude plugin marketplace add passioncode-ai/fabric-switchboard`, `claude plugin install switchboard@switchboard` | both exit 0; plugin 0.4.0; `claude mcp list` → `plugin:switchboard:switchboard: switchboard mcp - ✔ Connected` |

The Windows archive was downloaded anonymously in full and matched its checksum ([publication receipt](publication-0.4.0.json)); it was not executed.

## Website

The download redirects and the Switchboard page on passioncode.ai select v0.4.0-beta.1: `passioncode-ai/passioncode-ai.github.io` source `c8a3a65` (PR [#13](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/13), fast-forward to its `main`), Worker version `2cd961df-0de6-4422-9193-f4c726eed857`. `https://passioncode.ai/switchboard/download/macos` and `…/windows` → 302 to the 0.4.0 archives; the archives fetched through them hash to the values above. The receipt is in that repository.

## Local installation

`/Applications/Fabric Switchboard.app` was 0.3.1. It was not running; it was moved to the ignored `artifacts/installed-rollback-0.3.1-2026-09-30/` of the local checkout (rollback: move it back), and the 0.4.0 app from the verified published ZIP was copied in with `ditto`. Installed copy: `CFBundleShortVersionString` 0.4.0, `spctl -a -vv` → `accepted, source=Notarized Developer ID`, `stapler validate` worked. The app was not launched, and no CLI was linked onto `PATH`: launching it would read the operator's real account store, which this release task does not touch.

## Limits

- Live provider acceptance (SB-01): **NOT_RUN**. Real login and end-to-end requests with live accounts remain unverified.
- Native Windows acceptance (SB-02) and Windows signing (SB-03): **NOT_RUN** / **NOT_SIGNED**.
- Native (Tauri webview) rendering of the 0.4 screens was exercised only by the build's native smoke, not inspected by a person.
- The PassionCode launcher does not list the plugin yet; the plugin installs from this repository's marketplace.
