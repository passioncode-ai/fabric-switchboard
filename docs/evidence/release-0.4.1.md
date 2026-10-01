# Switchboard 0.4.1 — build and release record

Recorded 2026-10-01. **Published public beta:** [v0.4.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.4.1-beta.1) (prerelease). It ships the Keychain shared-trust fix ([#12](https://github.com/passioncode-ai/fabric-switchboard/pull/12), [KEYCHAIN.md](../KEYCHAIN.md)) and is the first Switchboard release under `AGPL-3.0-only OR LicenseRef-PassionCode-Commercial` (Fabric ADR-0092); v0.4.0-beta.1 keeps PolyForm and v0.3.1-beta.1 and earlier keep MIT. [Anonymous publication receipt](publication-0.4.1.json) records the release ID, the tag's commit and the full-download hash of both archives.

The flow is the one in [release-0.4.md](release-0.4.md), run from a fresh clone.

## Source identity

Both artifacts were built from clean source [`15851e31e69bc40322c42c48f726dad7798eae73`](https://github.com/passioncode-ai/fabric-switchboard/tree/15851e31e69bc40322c42c48f726dad7798eae73), the version bump to 0.4.1 ([PR #13](https://github.com/passioncode-ai/fabric-switchboard/pull/13)). It landed on `main` by fast-forward, so `main`, the tag `v0.4.1-beta.1` and both build receipts name the same SHA. Later commits (this record) change documentation only.

Versions in step: workspace `Cargo.toml` 0.4.1 (CLI, proxy, runtime, desktop; `switchboard-core` keeps its own 0.1.0), `package.json`/lock 0.4.1, `src-tauri/tauri.conf.json` 0.4.1, plugin and marketplace entry 0.4.1, skill `metadata.version` 0.4.1. `node scripts/check-brand.mjs` → `version 0.4.1 verified`; `python3 scripts/check_plugin.py` → `switchboard plugin: 0 errors`. `scripts/test_check_plugin.py` now reads the shipped version from `plugin.json`: with the old `0.4.0`/`0.4.1` literals, the marketplace and package drift tests would have planted the real version (no defect) and the skill fixture would have drifted.

## Checks

- Local gate on `15851e3`: `npm ci` exit 0; `./scripts/check.sh` exit 0 — 134 Rust tests passed, 0 failed, 2 ignored (the opt-in login-keychain test and the hand-run signed-bundle acceptance); plugin validator 0 errors and its 23 unit tests OK; nightly clock 6 tests OK; notices current; 49 Markdown files, 341 relative links, 0 errors.
- Hosted: none dispatched. Organization policy since 2026-09-25 runs the complete hosted suite nightly; this release did not dispatch one ([issue #5](https://github.com/passioncode-ai/fabric-switchboard/issues/5) tracks whether the scheduled `checks` job runs).
- org-index `check_private.py` on the tree: 5 findings, all the pre-existing third-party author emails in `THIRD_PARTY_NOTICES.md`.

## macOS build, signing and notarization

`python3 scripts/build_macos.py --identity "Developer ID Application: Sergei Viktorovich Sheleg (KJ35UYYL22)" --arch universal --notary-profile <saved profile>` → exit 0. [Receipt](build-0.4.1-macos-universal.json):

- App and CLI `x86_64 arm64`; strict `codesign --verify --deep` passed; CLI embedded at `Contents/MacOS/switchboard`, equal (`cmp`) to the archived one. Designated requirements: app `identifier "ai.passioncode.fabric-switchboard" and anchor apple generic … certificate leaf[subject.OU] = KJ35UYYL22`, CLI `identifier switchboard and anchor apple generic … KJ35UYYL22` — the two the Keychain access list trusts.
- Native smoke `PASS` (temporary store, memory vault, native IPC, bundled frontend; real provider auth `NOT_READ`). The smoke uses `MemoryVault`, so the build touched no Keychain item.
- Notary submission `c93a1bdc-1ad3-4dff-9056-b8250c3eea2a` → **Accepted** (`xcrun notarytool info` → `status: Accepted`). Staple and validate worked; `spctl -a -vv` → `accepted, source=Notarized Developer ID`.

## Windows build

`python3 scripts/build_windows_cross.py` → exit 0 on the same commit. [Receipt](build-0.4.1-windows-x64.json): cargo-xwin 0.23.1, LLVM (Homebrew clang 23.1.2), NSIS 3.12; PE headers checked (`fabric-switchboard.exe` and `switchboard.exe` x64, installer x86), CLI with static CRT and no external VC runtime import. Authenticode **NOT_SIGNED**; native Windows installation and execution **NOT_RUN**. The Keychain change is macOS-only.

## Archive checksums

| Archive | SHA-256 |
|---|---|
| `Fabric-Switchboard-0.4.1-macos-universal.zip` | `d5c4352359d4d1d5ae4e0e9618c6f239edbf1fe7c75012d7c83f516530e67007` |
| `Fabric-Switchboard-0.4.1-windows-x64.zip` | `f6719bdddb8ab46ed0bf03ff148fa37ee7fa9b3742b9398a62cb8d335b111bd3` |

The release also carries `SHA256SUMS-0.4.1.txt` and both JSON receipts.

## From the published release

Downloaded with no GitHub token (plain HTTPS, [publication receipt](publication-0.4.1.json)):

| Step | Result |
|---|---|
| both ZIPs and `SHA256SUMS-0.4.1.txt` | sizes equal the asset sizes; `shasum -a 256 -c SHA256SUMS-0.4.1.txt` → both `OK` |
| `ditto -x -k` the macOS ZIP; `spctl -a -vv "Fabric Switchboard.app"` | `accepted`, `source=Notarized Developer ID`, `origin=Developer ID Application: Sergei Viktorovich Sheleg (KJ35UYYL22)` |
| `spctl -a -vv -t install` | `accepted`, `source=Notarized Developer ID` |
| `xcrun stapler validate` | `The validate action worked!` |
| `cmp switchboard "Fabric Switchboard.app/Contents/MacOS/switchboard"` | equal |
| `CFBundleShortVersionString` | `0.4.1` |

## Website

`passioncode-ai/passioncode-ai.github.io` source `d92f872` ([PR #23](https://github.com/passioncode-ai/passioncode-ai.github.io/pull/23), PR check passed, fast-forward to its `main`) selects v0.4.1-beta.1 through `node scripts/update-switchboard-release.mjs v0.4.1-beta.1` (`checksums: macos, windows; macOS notarized: true; launcher plugin: false`). The page's licence sentence now also names v0.4.0-beta.1 as the PolyForm release. Gates: `npm run check` exit 0 (13 release tests), `extract-public-copy.py --check` exit 0, `npm run build` exit 0 (29 entries). Deployed with `npm run deploy` (`CLOUDFLARE_ACCOUNT_ID` through Project Observatory's `use_secret.py`, Wrangler's OAuth session): Worker version `cacbdcf6-0986-4c88-a249-e4efd9f80c30`.

Live, 2026-10-01:

| Check | Result |
|---|---|
| `curl -sI https://passioncode.ai/switchboard/download/macos` | `302` → `…/v0.4.1-beta.1/Fabric-Switchboard-0.4.1-macos-universal.zip` |
| `…/switchboard/download/windows` | `302` → `…/v0.4.1-beta.1/Fabric-Switchboard-0.4.1-windows-x64.zip` |
| archives fetched through the redirects | `d5c43523…0e67007`, `f6719bdd…b111bd3` — equal to the table above |
| `/switchboard/` | `data-release-version` 0.4.1-beta.1; JSON-LD `softwareVersion` 0.4.1-beta.1, `license` `https://spdx.org/licenses/AGPL-3.0-only.html` |
| `/`, `/switchboard/`, `/fabric/`, `/inbox/`, `/observatory/`, `/design-system/` | all `200` |
| `https://www.passioncode.ai/switchboard/` | `301` → `https://passioncode.ai/switchboard/` |
| live bytes vs `dist/` | 29 of 29 files equal |

## Local installation and Keychain acceptance (SB-12)

`/Applications/Fabric Switchboard.app` was 0.4.0 and running. It was quit with `osascript -e 'tell application id "ai.passioncode.fabric-switchboard" to quit'` (exited in 2 s), moved to `$HOME/DATA/_archive/switchboard-installed-rollback-0.4.0-2026-10-01/` (rollback: quit the app and move it back), and the 0.4.1 app from the verified published ZIP was copied in with `ditto`. Installed copy: `CFBundleShortVersionString` 0.4.1, `spctl -a -vv` → `accepted, source=Notarized Developer ID`, `stapler validate` worked, strict `codesign --verify --deep` passed, no `com.apple.quarantine` attribute. Only the installed copy was launched (`open -b ai.passioncode.fabric-switchboard`, 11:27:27 CEST); no build-directory app was run.

ACL metadata only — `security dump-keychain -a ~/Library/Keychains/login.keychain-db`, never `-d`, no value read; account attributes compared by a 10-character SHA-256 prefix:

| | Before launch | After launch |
|---|---|---|
| items under `ai.passioncode.fabric-switchboard` (legacy) | 6 | **0** |
| items under `ai.passioncode.fabric-switchboard.shared` | 0 | **11** |
| what each item trusts | two `cdhash` requirements (deleted builds) and the team-signed app (`identifier "ai.passioncode.fabric-switchboard" … KJ35UYYL22`); the bundled CLI in none | **all 11**: the team-signed app **and** `identifier switchboard … KJ35UYYL22` (the bundled CLI) |

All 6 legacy account attributes reappear under `….shared` and the legacy copies are gone, so the move completed and removed the originals. Because the legacy items already trusted the team-signed app requirement, KEYCHAIN.md step 1 predicts a silent move. The other 5 items belong to accounts added in the running app at 11:27:56–11:28:38 (4 Claude in one batch, 1 Codex; store events `account_added`), not by this task; they were written directly with the shared access list. The app's usage checks read all 11 (`usage_health.status` `ok`).

Then the installed CLI, with no `SecurityAgent` process before or after any of them (`pgrep -x SecurityAgent`):

| Command | Result |
|---|---|
| `switchboard --version` | `switchboard 0.4.1` |
| `switchboard --help` | exit 0 |
| `switchboard --json status`, `usage`, `current`, `accounts list` | each exit 0, `ok: true`; `usage` lists 11 accounts |
| `switchboard mcp --read-only` over stdio: `initialize`, `tools/list`, `tools/call switchboard_accounts` | server `switchboard 0.4.1`, protocol `2025-06-18`, 4 tools, `isError: false`, 11 accounts, 2 signed in |

Not observable from here: whether a dialog appeared on screen during the move. The unified log holds no `SecurityAgent` entries for the day at all, so it is no evidence either way; the operator saw the screen.

## Limits

- Live provider acceptance (SB-01): **NOT_RUN** as a structured acceptance; the app's usage checks did call the providers for the operator's accounts.
- Native Windows acceptance (SB-02) and Windows signing (SB-03): **NOT_RUN** / **NOT_SIGNED**.
- No hosted run for this release candidate.
