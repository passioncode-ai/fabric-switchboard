# Fabric Switchboard — start here

**Objective:** research four account-switching implementations, specify and build a Fabric product; extension: CLI, Developer ID signed macOS app/CLI and a native Windows build.
**Owner:** private `passioncode-ai/fabric-switchboard`. **Branch:** `codex/bootstrap`. This repository owns all source, decisions, task packets and verification records.

Read [product map](PRODUCT.ru.md) → [research](research/README.md) → [specification](SPEC.md) → [shared contracts](CONTRACTS.md). Current delivery state is [0.2 release evidence](evidence/release-0.2.md); [0.1 verification](evidence/verification.md) is historical. [CLI](CLI.md), [operations](OPERATIONS.md) and [distribution](DISTRIBUTION.md) are the operating instructions.

## Completed implementation

- Four fixed upstream commits researched with 69 line-addressed source links; Claude-swap, codex-account-switcher, subswapper and cc-switch. “Cloud Swap” interpreted as linked claude-swap; the fourth comparator was chosen explicitly because the request supplied three URLs.
- Tauri/Rust/TypeScript workbench, per-provider/pool routing, native vault, atomic metadata, private official-login homes and HTTP/SSE proxy. A response keeps its immutable identity snapshot; selection affects the next request; no switcher-level replay.
- Native `switchboard` CLI shares Runtime/Owner with GUI. CRUD, usage, events, official login, launch, JSON and serve; secret stdin only. Separate authenticated control API, private descriptor, HMAC identity proof and mutation on one TCP connection, no reconnect. Home lifecycle mutations serialize under the owner.
- Windows DPAPI CurrentUser vault, user-only DACL, strict capability reads, reparse/hardlink refusal, atomic replacement, PowerShell child launcher and PID+creation-time identity.
- Exact-commit universal macOS signing script and manual exact-commit Windows build workflow. Their existence is not the artifact receipt; see release evidence for actual execution.

Source milestones: [CLI/shared runtime](https://github.com/passioncode-ai/fabric-switchboard/commit/ad0d42d), [Windows core](https://github.com/passioncode-ai/fabric-switchboard/commit/2d92329), [strict capability reads](https://github.com/passioncode-ai/fabric-switchboard/commit/970495c), [control connection and lifecycle fixes](https://github.com/passioncode-ai/fabric-switchboard/commit/9fba411). Subsequent integration changes and tested source SHAs are recorded in release evidence.

## Decisions and open boundaries

Canonical credentials stay in OS protection; isolated official CLI requires a private access-token working copy. No global account takeover, process kill, automatic account rotation or background token refresh. Pending login registry is in-memory; owner restart loses pending IDs and leaves explicit cleanup work. Managed clients require relaunch after owner restart. Quota is a timestamped observation, never inferred from an API-key subscription.

Native live-provider login/inference, interactive Windows installer/UI, two-Windows-user isolation and screen-reader acceptance are not replaced by synthetic fixtures. Signing and notarization are separate. Apple Developer ID cannot sign Windows binaries. Self-updates and general-availability claims remain outside the engineering beta.

## Exact next task

Artifacts are built and retained in the [private draft release](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/untagged-58813c81ca48bc2e938c); [release evidence](evidence/release-0.2.md) names their exact source SHAs and hashes. First release prerequisite: obtain the existing notarytool profile name from the operator and resume notarization of the retained signed archive using [distribution](DISTRIBUTION.md). Native Windows verification needs an available Windows runner: the attempted hosted job was billing-blocked before executing any step. Do not mark cross-compilation as that verification.

Native Mac window inspection is now complete. Execute [PA-01](packets/provider-acceptance.md) using authorized test accounts through official login, with a bounded request budget. Record exact Claude/Codex versions, isolated and managed responses, and A→B next-request switching. Do not read existing global credentials as a substitute. Fix observed defects with focused fixtures before repeating the affected acceptance.

Bounded follow-ons: [CLI release plan](CLI-RELEASE-PLAN.md), [Windows acceptance](packets/windows.md), [distribution](packets/distribution.md), [session supervisor](packets/session-supervisor.md), [automatic routing](packets/automatic-routing.md). All link the shared contracts; no unmerged implementation-agent branch is a prerequisite.

## Reproduce and local-only state

```sh
git clone --branch codex/bootstrap git@github.com:passioncode-ai/fabric-switchboard.git
cd fabric-switchboard
npm ci
./scripts/check.sh
cargo build --release --locked -p switchboard-cli
```

Keep `node_modules/`, `target/`, `dist/`, `artifacts/`, OS app data, Keychain, provider homes, credentials and external research clones out of Git. Research validation uses separate clones at the pins in `docs/research/sources.json`; it does not execute third-party code. Observatory remains untouched; no Fabric parent submodule pin was changed.

The initial 0.1 delivery at `a478941c7ab7f467ebfde36d592910d2e1fff79c` passed an independently fetched checkout, 36 tests and document validation with reused Cargo cache. This is historical evidence only. The current 0.2 remote SHA/fresh-checkout receipt belongs in the current release evidence. A pushed branch is available for handoff, not proof of a release, merge, notarization or deployment.
