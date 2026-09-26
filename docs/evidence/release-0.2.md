# CLI and native distribution evidence — 0.2

Task authorized 2026-09-26: add CLI, sign macOS build, build Windows artifact. Source baseline before this change: `8f2252e0ec3b866401929bc781ca3a79ef5c9328`. This record is updated as measured gates finish; planned behavior lives in [release plan](../CLI-RELEASE-PLAN.md).

## Preconditions measured

- Local `security find-identity -v -p codesigning` reports one valid Developer ID Application certificate, subject `Sergei Viktorovich Sheleg`, Team `KJ35UYYL22`. User authorized their developer identity as Siarhei Sheleh; the installed certificate carries its Russian-form subject. An extra clarification was initially asked, then recognized as unnecessary for the already authorized signature. No private key exported.
- No Apple/notary environment variable names are present in the build process. Lookup of standard `com.apple.gke.notary.tool` generic-password metadata returned item-not-found; no password value was requested. A saved notary profile name remains unresolved.
- Apple Silicon and Intel Rust targets present. Windows MSVC target available for type checking. macOS toolchain cannot itself execute the Windows acceptance binaries.
- GitHub repository Actions enabled. This alone is not proof of runner availability or billing capacity; actual targeted build remains the gate.
- Merged Windows Tauri config validates against the installed Tauri CLI JSON schema. UI platform/path changes passed TypeScript/Vite build and 19 path / 3 label examples in the implementation worktree.

## Execution state

CLI/core/runtime integration: PASS. `./scripts/check.sh` exited 0 after the strict-read and control/lifecycle fixes: 50 Rust tests (4 CLI, 20 core, 9 proxy, 17 runtime), one explicit native Keychain test ignored by default; frontend TypeScript/Vite, fmt, workspace strict Clippy and 30 Markdown files / 116 relative links passed. Log retained locally as `artifacts/check-0.2.log`; native fixture names are tracked in the owning crates. Windows native test/build: NOT_RUN. macOS Developer ID signing: PENDING build execution. Notarization: NOT_RUN, profile unresolved. Windows Authenticode signing: NOT_REQUESTED / no certificate configured. Live-provider acceptance on both systems: NOT_RUN.

No source branch, compile check or synthetic response is presented as a notarized release or successful provider login.
