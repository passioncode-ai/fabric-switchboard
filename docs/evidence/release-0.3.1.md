# Switchboard 0.3.1 — build and release handoff

Recorded 2026-09-26. **Published public beta:** [v0.3.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.3.1-beta.1). Repository visibility is public, default branch is `main`. [Anonymous publication receipt](publication-0.3.1.json) records the release ID, exact tag source and both downloaded archive hashes.

## Source identity

Both artifacts were built from clean source [`9e20a49ad7ef917068c266eff4283180209965dc`](https://github.com/passioncode-ai/fabric-switchboard/tree/9e20a49ad7ef917068c266eff4283180209965dc). That commit contains application version 0.3.1, PassionCode tokens and the S mark. Subsequent installation/release documentation commits do not change those binaries. The release tag is intended to identify the binary source commit; `main` may additionally include these documentation receipts.

Implementation and browser checks are in [design evidence](design-0.3.1.md). Account/runtime behavior is the existing [0.3 contract](../ACCOUNTS-AND-ROTATION.md). User installation: [INSTALL.md](../INSTALL.md).

## Build receipts

- [macOS universal receipt](build-0.3.1-macos-universal.json): Apple silicon and Intel app/CLI, Developer ID signatures verified, clean source. Notarization **NOT_RUN**: no notarytool profile supplied. Successful signature verification is not Gatekeeper acceptance.
- [Windows x64 receipt](build-0.3.1-windows-x64.json): cargo-xwin cross-build on macOS, PE headers checked, NSIS installer and CLI archived; no external Visual C++ runtime import in the CLI. Authenticode **NOT_SIGNED**, native Windows tests **NOT_RUN**.

These JSON files are exact copies of the non-secret build receipts under ignored `artifacts/`. Certificate subject/fingerprint identify the public signer; no private key, provider credential or app data is included. Local verification recomputed each ZIP SHA-256 with Python `hashlib.file_digest` and matched both receipts. ZIP entry inspection confirmed the macOS app and Windows installer/CLI layouts used by the installation guide; no native application was launched for this documentation step.

## Archive checksums

| Archive | SHA-256 |
|---|---|
| `Fabric-Switchboard-0.3.1-macos-universal.zip` | `5bdece37fb9965019d2a1f84af45075b571b7979dca8b8c475495ff4b50bed25` |
| `Fabric-Switchboard-0.3.1-windows-x64.zip` | `f3fb96619e2722bbee954eb5758800c790df6dee6e2cd4763372b1a2a9df4665` |

Individual binary hashes and toolchain versions are in the JSON receipts. Anonymous full downloads of both archives matched these SHA-256 values and GitHub asset sizes; see [publication receipt](publication-0.3.1.json).

## Verification and limits

- Full local source gate at binary source: `npm ci` then `./scripts/check.sh`, PASS: build, Rust formatting, 81 tests passed / 1 explicit synthetic Keychain test ignored, Clippy and docs/whitespace gates ([design evidence](design-0.3.1.md)).
- macOS and Windows build procedures completed successfully; the root packaging task holds the command execution receipts and the JSON above identifies their outputs.
- Native Windows installation/UI acceptance: **NOT_RUN**.
- Real provider login and end-to-end inference: **NOT_RUN**.
- This docs-only follow-up: `python3 scripts/check_docs.py` PASS (37 Markdown files, 212 relative links, 0 errors); `git diff --check` PASS. It does not re-run native packaging or claim hosted CI. Hosted full checks remain nightly; no full workflow dispatched here.

## Exact next task

The repository and release are public; both archives, their build receipts and SHA256SUMS are attached. Next: complete notarization with a saved signing profile, run native Windows acceptance and signing, and perform the explicitly operator-assisted live-provider acceptance packet. Website publication is recorded by its owning repository. Keep signing credentials, local provider state, dependencies and archive build directories out of Git.

## Publication review

Before opening repository visibility, Gitleaks scanned 56 Git commits (1.41 MB). One generic-api-key finding was reviewed at historical source `a80d86b9a1eded4646bdd9371c5affdce8c93895:src-tauri/src/launch.rs:537`: the synthetic fixture for `codex_export_preserves_native_auth_shape_without_refresh_lineage`, with deliberately synthetic access/refresh/account values. Zero actionable secrets were found in that scan; this is an adjudicated finding, not a claim of zero raw matches. No real provider credentials were read.

Packaging used `DEVELOPER_DIR=/Library/Developer/CommandLineTools` for both scripts. The initial active-Xcode build stopped at its unaccepted license; no license or global toolchain setting was changed. The installed standalone Command Line Tools build succeeded with strict app/CLI signature verification. Hosted full CI was not dispatched.

## Fresh public checkout

Anonymous HTTPS clone with credential helpers and prompts disabled resolved `main` to `b6cde090a62a7a96a2a612ada875a9684e1e3b86` and the release tag to `9e20a49ad7ef917068c266eff4283180209965dc`. Both GitHub blob and raw INSTALL/LICENSE requests returned HTTP 200 without authorization. `npm ci` passed with 0 vulnerabilities; `DEVELOPER_DIR=/Library/Developer/CommandLineTools ./scripts/check.sh` passed: 81 tests / 0 failed / 1 explicit Keychain fixture ignored, TS/Vite, brand/version, Rust format/Clippy, 37 Markdown files / 212 links / 0 errors. Fresh tree remained clean. The initial default-Xcode attempt stopped at license before tests; scoped installed CLT retry passed without system changes. Subsequent commits alter documentation only.
