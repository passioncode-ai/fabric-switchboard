# CLI and native distribution — 2026-09-26

Operator request: add a CLI, sign the macOS build and produce a Windows build now. This authorizes using the installed Developer ID and a targeted Windows artifact-build workflow, not restoring per-push full suites. Continue autonomously on the inherited model. Task-pipeline route remains specification → build → focused checks → durable handoff.

## Requirements and interfaces

- CLI `switchboard`: accounts list/add/update/remove/select, usage, activity, official login, isolated/managed launch and `serve`. Human output plus JSON; secret input through stdin only, never argv. Stable failure exit status. Reuse core validation and never print Credential.
- One owner of Store per app-data root. A running GUI or `serve` exposes a separate loopback control listener with per-runtime capability stored in private `control.json`. CLI connects to it for mutations; absent listener allows local non-server commands. Stale control metadata must not enable stealing a live store. Bind only loopback, exact Host, no Origin/redirects, bounded requests, constant-time bearer check. Never expose credential retrieval. Persisted descriptor is secret and excluded from Git.
- Share launcher/login logic in `switchboard-runtime` crate. Default data root identical in CLI and GUI. CLI `serve` owns proxy + control until shutdown; managed sessions require this owner to remain running.
- Windows: native protected vault and private metadata, atomic replacement, reparse-point refusal, Windows terminal/process launcher. Build native CLI + desktop installer on windows-latest and upload artifact. Build evidence distinguishes fixture acceptance from actual provider/browser login.
- macOS: Developer ID signing of CLI and app, hardened runtime, timestamp, signature verification. Notarization if configured credentials available; never imply signing alone is notarization. No secret/certificate export into repository/CI.

## Work packets / ownership

1. Shared runtime extraction and contracts (root, then CLI implementer owns runtime/lib/control + src-tauri/main and CLI crate).
2. Windows native adapters (separate implementer owns core platform files + shared runtime/launch only).
3. Packaging/signing and targeted Windows build workflow (root); docs/scenarios/handoff (root).
4. Convergence review, local tests, native Windows artifact build, signature receipt, push and remote SHA verification (root).

Shared contracts: docs/CONTRACTS.md; base 8f2252e. No global provider auth access or existing-session mutation. Source work in separate worktrees; bounded root integration by commits. Actual notary credentials and hosted runner availability are measured prerequisites, not promises.
