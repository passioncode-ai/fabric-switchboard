<sub>ssheleg skills — task-pipeline · ux-scenarios · copywriting</sub>

# CLI and native distribution evidence — 0.2

Task authorized 2026-09-26: add CLI, sign macOS build, build Windows artifact. Baseline before this task: `8f2252e0ec3b866401929bc781ca3a79ef5c9328`. The [release plan](../CLI-RELEASE-PLAN.md) owns requirements; this report records execution. Full [artifact receipts](artifacts-0.2.json) and [check receipts](checks-0.2.json) are tracked alongside it.

## Delivered artifacts

Engineering artifacts are retained in the private repository's [draft release](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/untagged-58813c81ca48bc2e938c). It has not been published as a generally supported release. Each ZIP has an adjacent receipt naming its actual source commit and checksum.

| Artifact | Source commit | Observed result |
|---|---|---|
| `Fabric-Switchboard-0.2.0-macos-universal.zip` | `5ee501eb9913527e8295f14b3e25843cc9cb5230` | app + CLI, arm64 and x86_64, Developer ID signed; strict codesign verification PASS |
| `Fabric-Switchboard-0.2.0-windows-x64.zip` | `b602e1196d4413e249555cf46f398f578c5fcbe5` | x64 CLI + NSIS installer, cross-build PASS, Authenticode NOT_SIGNED, native execution NOT_RUN |

Mac archive SHA-256: `2215257d134a8f1474f15eeabad38b3060d7c75b7f5c80bfa825f0d49fa99e3a`.
Windows archive SHA-256: `305642f590b35aa9ee87d5afa720c59a2ec7f8115da1f6352064f4e254121df4`.

The commits differ in release scripts/workflow only. `git diff --exit-code 5ee501e b602e11 -- crates src src-tauri Cargo.toml Cargo.lock package.json package-lock.json` exited 0. The application source tested in the fresh clone is the application source of both archives. Documentation commits after these receipts do not imply the binaries were rebuilt.

## Checks actually executed

- `./scripts/check.sh` exited 0: 50 Rust tests (4 CLI, 20 core, 9 proxy, 17 runtime), one opt-in native Keychain test ignored; frontend TypeScript/Vite, fmt and workspace Clippy `-D warnings` passed. At build commit: 30 Markdown files, 116 relative links, 0 errors. GUI has no duplicate launcher test suite: it calls shared Runtime.
- Fresh remote clone of `5ee501e`: `npm ci`, frontend build and full `./scripts/check.sh` passed. Cargo build/dependency cache was reused with explicit `CARGO_TARGET_DIR`; no source or untracked document was copied. Git status remained clean. This proves an independent source checkout, not a cold offline build.
- Signed universal CLI `--version` and `--help` executed successfully on arm64 macOS. A separate temporary root ran signed `serve` → stdin synthetic credential add → select → disable → remove through the authenticated control API and native Keychain. Empty final store and no fixture secret in metadata were checked. No real provider credential or global CLI home was read.
- CUA opened the signed native `.app`: v0.2.0/macOS, empty account state and ready loopback proxy were visible. Add dialog opened and Cancel returned to the list. Signed CLI `--json status` reported the same runtime address and account list was empty. This closes the earlier locked-Mac inspection blocker; it does not claim screen-reader, focus-return or live-provider acceptance.
- Windows Tauri config schema check passed. Platform adapter development checks included Windows-target core strict Clippy and 19 path / 3 label examples. Actual Windows desktop and CLI link/packaging succeeded using cargo-xwin 0.23.1, LLVM 23.1.2 and NSIS 3.12. PE checks found AMD64 GUI and CLI; the NSIS bootstrap is x86 and carries the x64 app.
- Dependency inspection caught the first CLI importing VCRUNTIME140.dll. The delivered CLI was rebuilt with `-C target-feature=+crt-static`; `llvm-readobj --coff-imports` confirms no separate Visual C++ runtime import. Both delivered executables remain unsigned for Windows.

The regressions that closed integration review are tracked in [control.rs](../../crates/switchboard-runtime/src/control.rs), [Runtime](../../crates/switchboard-runtime/src/lib.rs) and [native private reads](../../crates/switchboard-core/src/windows.rs): home deletion versus launch reservation, a proved connection closing before mutation, and previously exposed capability permissions. Generic metadata reads may normalize protection; control reads must refuse an exposed token.

## Signing and external blockers

Developer ID subject: `Sergei Viktorovich Sheleg`, Team `KJ35UYYL22`, the installed certificate for the developer authorized by the operator as Siarhei Sheleh. No private key was exported or Keychain access policy changed. App and CLI carry hardened-runtime flags and a trusted timestamp. The [macOS build script](../../scripts/build_macos.py) verified both architectures and signatures.

**Notarization: NOT_RUN.** No saved notarytool service metadata was found in the login Keychain; no password value was requested. The operator supplied the developer name, not a notary profile. `spctl --assess --type execute --verbose=4` exited 3 with `source=Unnotarized Developer ID`. Local app launch is not a clean-machine Gatekeeper pass. Resume the retained archive with the existing-profile command in [distribution](../DISTRIBUTION.md) once a profile is available; do not disable Gatekeeper or relabel signed as notarized.

**Native Windows tests: NOT_RUN.** [Actions run 36249340921](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/36249340921) failed before any step. Its check annotation reports failed recent payments or an insufficient spending limit. No billing settings were changed and no nightly/full suite was dispatched. Local cross-compilation produced the requested binaries but does not establish DPAPI behavior on two Windows users, real PowerShell/provider login, installer interaction or accessibility. Run the existing manual exact-SHA workflow when a Windows runner is available.

**Live-provider acceptance on both systems: NOT_RUN.** [PA-01](../packets/provider-acceptance.md) requires authorized accounts through official login and an explicit request budget. No synthetic fixture is described as successful Claude/Codex authentication.

## Reproduce and continue

[CLI instructions](../CLI.md), [native build/signing procedure](../DISTRIBUTION.md), [operations](../OPERATIONS.md) and [handoff](../HANDOFF.md) are the entry points. Build output, SDK cache, private app state and original compiler logs remain local-only under ignored paths; tracked receipts retain hashes and the release retains the ZIP files. Source and receipts are pushed to `codex/bootstrap`; final remote equality is verified at handoff.

---

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**

- [`task-pipeline`](https://github.com/ssheleg/task-pipeline) — CLI implementation and release checks
- [`ux-scenarios`](https://github.com/ssheleg/super-ux) — CLI and cross-platform scenarios
- [`copywriting`](https://github.com/ssheleg/super-ux) — CLI messages and product terms
