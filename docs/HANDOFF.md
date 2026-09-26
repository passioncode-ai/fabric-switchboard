# Fabric Switchboard — start here

**Objective:** research four account-switching implementations, design a detailed product, and build a new Fabric project for macOS with Windows planned.
**Owner:** `passioncode-ai/fabric-switchboard`, private repository. **Branch:** `codex/bootstrap`. **Implementation:** `a80d86b9a1eded4646bdd9371c5affdce8c93895`; subsequent commits carry documentation/evidence only unless noted.

Read [Russian product map](PRODUCT.ru.md) → [research](research/README.md) → [specification](SPEC.md) → [shared module contracts](CONTRACTS.md). [Verification](evidence/verification.md) distinguishes tested code, synthetic UI behavior and missing real-provider evidence. [Operations](OPERATIONS.md) names data ownership and recovery limits.

## Completed

- Researched the three supplied repositories plus explicitly chosen `farion1231/cc-switch`; recorded four fixed commits and 69 line-addressed source links.
- Specified account lifecycle, native vault, per-pool routing, isolated/managed sessions, request/stream semantics, usage, failures, OS seams, UI, threat model and acceptance.
- Built Tauri/Rust/TypeScript desktop implementation, macOS Keychain vault, atomic metadata, official-login adapters, per-home launch reservations, fixed-upstream HTTP/SSE proxy and bounded metadata activity.
- Passed 36 default Rust tests, separate synthetic Keychain roundtrip, strict clippy/fmt, frontend build and macOS arm64 release bundle. Browser demo flows inspected with CUA. Source and document validators supplied.
- Added nightly-only hosted workflow; it has not been run. No push/PR full-suite trigger and no signing secret.

## Decisions and limits

“Cloud Swap” interpreted as linked `claude-swap`; fourth source chosen because request contained only three URLs. Organization inferred from existing Fabric ownership; working product name Fabric Switchboard. No global account takeover, global process kill, automatic rotation, background token refresh, Windows release or signed update has been shipped.

Canonical credentials stay in Keychain. Isolated CLI needs a private access-token working copy; managed mode only passes local capability. One account snapshot owns one whole response. Selection affects the next request. No switcher-level replay, even on 429. Codex identity token and last_refresh native requirements are explicitly handled; timestamp is a local snapshot write, not evidence of refresh.

Pending-login recovery is currently in-memory; app restart can leave staged auth to be cleaned deliberately. Managed clients need relaunch after app restart. PID reuse is conservatively refused. These are documented limitations, not concealed green checks.

## Exact next task

Run [PA-01](packets/provider-acceptance.md). First unlock the Mac and inspect the built `.app` window: CUA was blocked because the machine was locked. Then the operator completes official sign-in with authorized test accounts and a bounded request budget; record exact Claude/Codex versions and verify genuine isolated/managed responses plus A→B request-boundary switching. Do not substitute reading current global credentials or fixture success for this test.

Fix any observed adapter defect with a synthetic regression fixture, then rerun affected acceptance. The follow-on [session supervisor](packets/session-supervisor.md), [automatic routing](packets/automatic-routing.md), [Windows](packets/windows.md) and [distribution](packets/distribution.md) packets all link the shared contracts. Their prerequisites and ownership are explicit.

## Resume and local-only state

```sh
git clone --branch codex/bootstrap git@github.com:passioncode-ai/fabric-switchboard.git
cd fabric-switchboard
npm ci
./scripts/check.sh
npm run app:build
```

Source, specs and checks are durable in this repository. Local-only: `node_modules/`, `target/`, `dist/`, `artifacts/`, OS app data/Keychain and external comparison clones. Do not commit these, any user credential, provider home or environment file. Research source validation needs separate clones at `docs/research/sources.json` pins; no third-party program needs to be executed.

The implementation agents' work is integrated; no agent branch is a prerequisite. Observatory remains untouched and this is not yet a Fabric parent submodule. No central multi-repo index is required for a single owning repository.

## Delivery receipt

The first complete source/docs snapshot `a478941c7ab7f467ebfde36d592910d2e1fff79c` was pushed and independently fetched from `origin/codex/bootstrap`; `git ls-remote` returned that exact SHA. GitHub reports the repository private and default branch `codex/bootstrap`.

A new remote clone of that commit ran `npm ci` and `./scripts/check.sh` successfully: TypeScript/Vite, 36 Rust tests, fmt, strict clippy, 26 Markdown files / 83 relative links / 0 errors. Its Git working tree remained clean. Cargo dependency/build cache was reused with an explicit `CARGO_TARGET_DIR`; project crates were recompiled from the fresh checkout. No source file, node_modules tree or untracked spec was copied into it. This proves fresh source checkout, not a cold offline build.

This final handoff commit only adds the receipt and repository instructions; application source remains at the implementation baseline. Final remote HEAD equality is checked after pushing this entry. A source branch is not a package release, merger into an existing production branch, notarization or deployment.
