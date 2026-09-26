# Fabric Switchboard — start here

**Objective:** a Fabric account workbench for Claude Code/Codex, CLI and macOS/Windows builds. Current extension: capture existing CLI authorization, official console login for additional accounts, Claude Swap import, actual active identity, detailed quota and opt-in rotation.
**Owner:** `passioncode-ai/fabric-switchboard`; 0.3.1 source branch `codex/passioncode-design-v031`. Public repository/release publication is owned by the parent launch task; this design packet does not claim it completed. Observatory and other projects are unchanged.

Read [0.3 account/rotation contract](ACCOUNTS-AND-ROTATION.md) → [research delta](RESEARCH-0.3-IMPORTS.md) → [spec](SPEC.md) → [shared contracts](CONTRACTS.md). Current checks/artifacts: [0.3.1 release evidence](evidence/release-0.3.1.md); [0.3 implementation evidence](evidence/release-0.3.md) is historical. [0.2](evidence/release-0.2.md) and [0.1](evidence/verification.md) reports are historical.

## Completed implementation

- Four original comparator repositories researched at fixed commits; Claude Swap revisited for current capture, backups, live identity and compatible locks.
- Tauri/Rust/TypeScript desktop and native CLI share protected vault, immutable HTTP/SSE request identity, isolated official-login homes and macOS/Windows launch adapters.
- Current Claude/Codex capture, read-only Claude Swap import, provider/pool/account/organization upsert, actual native identity distinct from managed selection. Native context stays vault-only.
- Explicit native Claude activation with lock heartbeat/ownership, identity compare, config preservation and guarded rollback. No process kill or prompt replay.
- Detailed quota/reset windows and generation checks, bounded fair polling/backoff. Ordinary CLI owns OAuth refresh; its newer current generation can be adopted for known profiles.
- Persisted same-provider/pool policies with freshness, enabled/expiry eligibility, threshold/headroom/cooldown and hold reasons. Managed route/cooldown publish atomically. One native Claude policy can be enabled across all pools. Manual choice starts cooldown; disabling policy stops rotation.
- Runtime monitor/cache/CLI/Tauri wiring; [CLI](CLI.md) and [operations](OPERATIONS.md) describe usage and recovery.

Packets and shared API: [PLAN-0.3](PLAN-0.3.md). Branches `codex/v03-core`, `codex/v03-imports`, `codex/v03-ui` integrated by cherry-pick; no unmerged packet code is a prerequisite.

## Decisions and boundaries

Metadata writes schema2 and lazily reads schema1; no destructive downgrade. Secrets never reach renderer/control output, logs or Git. Existing streams keep their identity. Native activation explicitly updates ordinary Claude auth; tests use fixtures. Local identity is a source claim until authenticated provider evidence exists.

Inactive snapshots can expire; recapture/reimport or official login is recovery. No competing refresh grant. Codex file/direct macOS keyring capture is supported; newer secrets/ephemeral stores report unavailable. Other shells' transient overrides cannot be inferred. Native Codex takeover, crash-resumable login IDs, indefinite account pins, quarantine, encrypted cross-device export and automatic updates remain follow-ons.

Signing is separate from notarization. Native Windows and provider acceptance remain separate. Hosted full checks remain nightly only; previous hosted run was billing-blocked before any step.

## 0.3.1 PassionCode design packet

Completed: exact shared PassionCode v1.0.0 tokens and S mark vendored with immutable source commit and SHA-256; fixed dark appearance; gold next-request selection separated from blue current identity; native PNG/ICNS/ICO regenerated from that same S vector; app/workspace packages bumped to 0.3.1. The independent switchboard-core crate retains its existing 0.1.0 version. Credentials and routing behavior are unchanged.

Read [design evidence](evidence/design-0.3.1.md), [source manifest](../brand/passioncode/manifest.json), [SCN-023](ux/scenarios.md#scn-023--recognize-switchboard-across-desktop-surfaces) and [contracts](CONTRACTS.md). Parent packet: [public launch](https://github.com/passioncode-ai/passioncode-ai.github.io/blob/508e91793fcb79d6a59bd2265551dbc76f7a8f97/docs/tasks/2026-09-26-public-launch.md).

Checks: npm ci, full `./scripts/check.sh` (81 passed, 1 opt-in Keychain test ignored), 2 canonical asset hashes, 3 generated native hashes, token-reference and version gate; repeat icon generation is byte-identical after sorting ICNS chunks. Synthetic browser review covered Accounts, Activity and Add account at 1280×720 and 740×560. No native/auth test run for this visual patch. Brand lint is NOT_RUN: this repository has no docs/brand/lint.py; identity labels reviewed manually.

## Exact next task

Both 0.3.1 platform builds completed from clean source `9e20a49ad7ef917068c266eff4283180209965dc`. [Release evidence](evidence/release-0.3.1.md) records verified local archive hashes and links exact build receipts. [INSTALL.md](INSTALL.md) now separates end-user installation from developer packaging instructions. macOS is Developer ID signed but not notarized; Windows is unsigned and cross-built.

Parent launch task: publish authorized source/default `main`, tag `v0.3.1-beta.1` at the binary source commit, upload archives/receipts and verify anonymous downloads/hashes; then deploy website links and record public access receipts. This docs-only commit does not claim publication, deployment, native Windows or provider acceptance. The binary source and later documentation commit must remain distinguishable.

Acceptance remains [PA-01](packets/provider-acceptance.md), extended with SCN-018..023. Do not read real auth merely to test. Native Windows acceptance needs an available Windows host. macOS notarization needs the saved notarytool profile; Windows signing needs its separate certificate/service. Prior release facts remain in [0.3 evidence](evidence/release-0.3.md).

## Reproduce and local-only state

```sh
git clone --branch codex/passioncode-design-v031 git@github.com:passioncode-ai/fabric-switchboard.git
cd fabric-switchboard
npm ci
./scripts/check.sh
cargo build --release --locked -p switchboard-cli
```

Keep dependencies, target/dist/artifacts, research clones, app data, Keychain and all provider homes/credentials outside Git. No Fabric parent submodule pin changed. Pushed source is a handoff; draft assets are engineering artifacts, not evidence of provider/native acceptance. Final push/fresh-checkout receipt belongs in the current release report.
