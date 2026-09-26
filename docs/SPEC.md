# Fabric Switchboard — product and engineering specification
Version 0.1, 2026-09-26. Design decisions under the operator's instruction to proceed; outcome validation remains unobserved. This document describes the intended product. [Verification](evidence/verification.md) and [handoff](HANDOFF.md) distinguish built, tested, and deferred functionality.

## 1. Product contract
A local account workbench for developers who own multiple authorized Claude Code and Codex identities. They add an account, see its health and quota, choose its routing pool, and launch an isolated or managed session. The app explains exactly when a selection takes effect. No Fabric cloud dependency is required to run it. “Fabric” here is project ownership; Fabric execution-contract integration is a future adapter, not an invented compatible-agent claim.

Core outcomes: canonical account secrets stay in the OS vault (isolated CLI mode needs a private access-token working copy, §8); no incidental mutation of the user's global client configuration; explicit pool boundaries between work and personal accounts; visible distinction between a configured route and an account observed serving a request; no lost stream or duplicated tool execution caused by the switcher.

## 2. Axes and capability matrix
| Axis | v0.1 macOS | Extension / limitation |
|---|---|---|
| Provider | Claude Code, Codex CLI | provider adapters; Claude Desktop excluded |
| Credential | API key, Claude setup token, imported OAuth JSON | OAuth renewal delegated to official login; no fabricated OAuth app registration |
| Surface | Tauri desktop window, managed child launch | menu bar/tray convenience after core acceptance |
| Scope | account + provider + pool | project bindings later; no cross-pool auto fallback |
| Isolation | managed home per account | no automatic MCP/settings/history sharing |
| Switch | next launch (isolated); next request (proxy) | current stream retains captured identity |
| Existing external CLI | not controlled | launch it through Switchboard to use managed mode |
| Codex Desktop | no live takeover | future explicit idle/restart handoff adapter |
| Routing | explicit manual choice | quota-based recommendation; automatic rotation separately opt-in future |
| Quota | timestamped provider observation | unknown/stale/error remain distinct from zero |
| Storage | macOS Keychain plus private metadata | Windows Credential Manager/DPAPI adapter planned |
| Distribution | locally buildable .app bundle | Developer ID signing/notarization require operator credentials |
| Privacy | local metadata, no analytics, no prompt logging | diagnostics export metadata only, later |
| Windows | source seams + acceptance plan | no Windows support claim until native tests run |

No promise that an HTTP proxy can change credentials inside an already-established WebSocket. v0.1 managed transport is HTTP/SSE only. Unsupported endpoints return an explicit error. Proxy use is a compatibility feature relying on provider client behavior, not a provider-endorsed integration claim.

## 3. User and information architecture
Primary persona: individual developer managing personal and authorized work subscriptions. Main job: continue the same local task under a deliberately selected account without accidentally spending another team's quota. Stakes: credential disclosure, wrong account, repeated tool effects, mixed project data.

Main screen: left navigation (Accounts, Activity, About); provider filter; visible proxy state; account rows with label, provider, credential type, pool, selection, enabled state, quota age; actions Add account, Select, Launch isolated, Launch managed, Check usage, Edit, Remove. The selection badge says “Selected for next request” rather than claiming the upstream accepted it. No installed accounts: explain addition and offer official login or manual credential import. List failure: error with retry, never an empty list.

Add dialog: provider → authentication method → label and pool → official sign-in or secret input. Password-style input for tokens; OAuth JSON pasted explicitly. Secret never appears in the account list, an event or error. Close/cancel clears it. Kind restrictions are enforced in Rust as well as UI. Disabled accounts remain visible but cannot be selected/used until enabled. Remove selected account is refused with recovery instruction to select another. Disable selected account clears its route so later requests fail closed.

Labels and pools are bounded. Pool identifiers use lowercase letters, digits, hyphens and underscores; account IDs are generated UUIDs, never filesystem names supplied by a user. Same provider/pool/secret duplicate is rejected; same label alone is not proof of identity. Email-like labels are user claims. Identity decoded from imported credentials is explicitly unverified until an authenticated provider check.

## 4. Account lifecycle and onboarding
States: added (syntactically valid) → selected → observed (usage/request succeeded); enabled/disabled is independent; expired → needs reauthentication. No background request merely because an account was added. Addition first persists the credential in the OS vault and then publishes metadata; failed metadata write removes the newly-created vault item. On uncertain cleanup failure, stop and report a sanitized storage failure; never claim success.

Official login starts only on explicit user action. Create a fresh private directory beneath app data; invoke the installed official CLI with CODEX_HOME or CLAUDE_CONFIG_DIR scoped to that child. The login happens in a terminal/browser under the provider's own flow. Do not invoke logout on the existing account. Finish re-reads the new home and captures the provider credential into the vault; all tokens pass through memory, not chat or command arguments. Cancel refuses while the isolated login process or terminal launch is pending; after it exits, Cancel removes only that login’s staging home and its scoped Keychain item. Finish is retry-idempotent in the same app lifetime. Pending-login recovery across app restart is not yet implemented (OPERATIONS.md). CLI absent → actionable installation message; timeout/cancel/unreadable Keychain → keep prior account state.

Codex login uses file storage in that isolated home so capture is explicit; unsupported auth schemas fail closed. Claude login may use a per-config-dir Keychain service; derive its name from NFC-normalized raw directory string SHA-256 prefix, as documented in the researched implementation; missing/denied Keychain must not silently read the global item. Manual token/JSON entry is available without CLI installation.

Replacing credentials requires reauthentication into a new profile; do not rotate a token independently while the official client holds another lineage. v0.1 imported OAuth is an explicitly captured credential snapshot: on expiry, reauthenticate. Background refresh-token exchange is not implemented until versioned provider adapters and concurrency ownership pass tests.

## 5. Secret storage, metadata, crash consistency
Storage roots use native application-data directories. macOS vault service `ai.passioncode.fabric-switchboard`, account `UUID`; stored value is a bounded JSON Credential. Use Security.framework, no `security ... -w TOKEN` subprocess arguments. Vault denial is terminal for that operation; plaintext fallback is forbidden.

Metadata is schema-versioned JSON: accounts, routes, bounded events. No tokens, refresh tokens, raw credential JSON, request bodies, provider error bodies, or process environment. Root private (0700), metadata 0600, files reject symlinks. Single process owns an exclusive OS lock; atomic same-directory temp write, file sync, rename. Mutations stage a candidate snapshot and publish memory only after disk success. Corrupt/unknown version refuses startup; an empty registry must never conceal loss.

Removal checks route references first. The vault delete and metadata removal are not a distributed transaction: choose a fail-closed ordering and document recovery. A missing credential leaves a visible unusable account, never a secretless success route. No silent cleanup of user files. The event ring is capped and rotates in the atomic metadata snapshot.

Threat model: malicious web page, corrupted imported credential, parallel app instances, unrelated process with a different OS user, denied vault access, crash between writes, upstream redirect, proxy abuse and accidental logging. A process executing as the same OS user can inspect or control that user's clients; the product does not claim to defend against a fully compromised login session.

## 6. Switching semantics and state machine
Managed session bootstrap obtains one local capability token, scoped to the app lifetime. The caller's API base URL points to loopback. Real provider credentials stay backend-side. Pool and provider identify a route; select updates that route atomically. At request acceptance the proxy obtains `(Account, Credential)` under one store snapshot and releases the store lock before network I/O. That immutable pair is used for the entire request, including its response stream.

Switch states: requested → validated (account exists, provider/pool match, enabled) → persisted → selected. Serving states: authenticated local request → route snapshot → upstream headers → streaming → completed/aborted. These two state machines are separate. A switch during streaming does not cancel, rewrite, or replay it. Next request receives the new identity. Switching is never claimed to alter a request already admitted by the provider.

No automatic retry in v0.1: 401, 403, 429, 5xx and connection failure return to the client. In particular, ambiguous timeout after the upstream received the request is never replayed. Future failover may retry only a proven pre-execution rejection, with a maximum one retry and same provider/pool; never after response body delivery. This deliberately avoids turning a quota feature into duplicated tool side effects.

## 7. Local proxy protocol
Listen on `127.0.0.1:0`, not LAN. Bind before reporting ready. Generate a cryptographically random local capability; constant-time comparison before body reading. Refuse Origin-bearing browser calls, unexpected Host, CONNECT, absolute URLs, path traversal, unsupported methods/routes and WebSocket upgrades. Maximum incoming body 16 MiB, concurrency bounded, connect timeout 10 s, stream lifetime bounded, response backpressure maintained. Refuse credential-bearing redirects (no redirect following). Never forward local Authorization, cookies, x-api-key, ChatGPT account IDs or proxy headers supplied by the caller.

Claude routes: `/claude/{pool}/v1/messages` (and `/v1/messages/count_tokens`, optional exact `?beta=true`) → fixed `https://api.anthropic.com/v1/messages`; API key uses `x-api-key`; setup/OAuth uses Bearer and applicable OAuth beta header. Codex API route `/codex/{pool}/v1/responses` → `https://api.openai.com/v1/responses`; Codex ChatGPT OAuth route maps to `https://chatgpt.com/backend-api/codex/responses` and replaces both bearer and `ChatGPT-Account-Id`. Preserve required model and streaming payloads without model remapping.

Headers are allowlisted (content-type, accept, anthropic-version/beta, selected client protocol headers); strip all hop-by-hop and connection-nominated fields in both directions. Do not expose upstream Set-Cookie. Error bodies from upstream pass only to the requesting provider client, never the UI journal. Events record account UUID, status code class, and timestamps, not payloads. Provider fixtures verify injection and endpoint selection before any real account testing.

## 8. Sessions and homes
Isolated mode materializes only that account's required native credentials/config in a private managed home and launches the official CLI in a terminal. It affects new launches only. Managed mode materializes only the local proxy capability in a provider-compatible configuration; no real provider token enters that child's home. Codex config defines a dedicated custom provider with `wire_api = "responses"`; no mutation of global config.toml. Claude uses scoped environment overrides. Environment sanitization removes inherited competing credentials/base URLs/provider flags.

The user chooses an existing absolute project directory for the CLI working directory; authentication home remains separate. One owned process per home is allowed; a synchronous reservation closes rapid double-launch before Terminal reports its PID. No global process kills, automatic Desktop quit, registry edits, terminal credential echo, or shared history symlinks. Launch commands use an argv array / correctly quoted generated launcher, with labels never embedded as code. Managed runtime home is a stable pool scope so local history can continue across managed route changes; isolated account home is separate. Provider CLI version must be recorded during live acceptance; automatic diagnostics collection is deferred. Child terminal launch success is reported as launch success; provider response is a separate acceptance observation.

A future session supervisor will track owned PIDs, in-flight requests and immutable identity revision. It may offer drain/restart for Desktop only with explicit UI confirmation. Resume is a provider session identifier, never a promise to migrate server-owned conversation state between unrelated accounts.

## 9. Usage and routing policy
Observation schema: percent used, source, observed_at, optional resets_at. Missing/nonfinite/out-of-range data is an error. 0 means a measured zero only. Show stale after five minutes; manual probe errors preserve last observation and display the failure. OAuth endpoints are compatibility adapters, not stable public contracts. Claude setup-token usage may be unavailable from the OAuth endpoint; observe headers on managed requests when available. API billing cannot be inferred from a subscription quota endpoint.

Future automatic policy: off by default; explicit account inclusion; same provider/pool; fresh observations only; threshold, cooldown, hysteresis, minimum remaining quota, expired/quarantined exclusions; no candidate means hold and notify. Never synthesize warm-up requests or silently spend tokens to start a quota window. Recommendation and execution are separate actions. No automatic failover across a work/personal boundary.

## 10. Desktop, design, accessibility
Tauri 2 shell, Rust core and proxy, TypeScript/Vite UI. Native IPC has explicit commands, remote origins denied, CSP restricted to self/IPC, no renderer shell or filesystem capability. Renderer receives no vault contents. The quiet Workbench visual register uses copied semantic tokens; product-specific components use standard HTML controls. No motion beyond focus/hover; respects reduced motion. Density target: five account rows and Add action visible at 1100×760; no horizontal scrolling at 900 px; narrow layout stacks actions. Primary action clearly says its effect. Dark theme follows system with readable state text in addition to color.

Keyboard: controls named, dialog Escape cancels, focus starts on first field and returns to trigger, visible focus, errors in aria-live, usage has numeric text. Screen-reader acceptance and computed contrast are separate from screenshot review. Figma not required for this engineering workbench. No marketing page, tracking, payment or monetization flow in this release.

## 11. Technology decision and alternatives
Rust core: typed state transitions, shared cross-platform logic, mature OS-vault bindings, streaming HTTP via axum/reqwest/Tokio; safer IPC boundary than renderer-owned tokens. Tauri over Electron reduces bundled-browser footprint (no quantitative size promise). TypeScript without React is adequate for this bounded single-window CRUD interface and avoids an unnecessary component dependency layer; UI growth may justify React later. Metadata JSON over SQLite keeps one atomic bounded document; introduce SQLite when request history/indexing volume justifies migration. Never put secret-valued provider configs in unencrypted metadata.

SwiftUI + Swift core would give excellent macOS integration but require another UI implementation for Windows (seen in the Codex comparator). Go CLI is suitable for daemon-first routing but does not itself provide the requested desktop workbench. Python/Textual is excellent for terminal ergonomics and rapid adapters but adds runtime packaging and less direct native IPC. Decisions may be revisited with measured build size/startup time; no invented benchmark table.

## 12. Windows plan
Port Vault to Windows Credential Manager (small records) or DPAPI current-user envelope plus user-only DACL; verify maximum record sizes and fail closed, never emulate chmod as security. Atomic replace must handle sharing violations with bounded retry, flush buffers, reject reparse points; instance lock uses named mutex/file locking with owner-scoped permissions. Root `%LOCALAPPDATA%/Fabric Switchboard`. Terminal adapter prefers Windows Terminal then PowerShell with argv-safe encoding, no token in commandline. Provider homes use environment variables; no symlink privilege requirement. Tauri uses WebView2; installer detection and accessibility checked on Windows 11. Signing/SmartScreen and update signatures are separate release tasks.

Windows acceptance: native vault roundtrip/denial/user separation, Unicode/spaces in paths, concurrent instances, interrupted write/restart, token redaction, Claude file credentials, Codex home, cancel/login, streaming route switch, system dark mode, keyboard and screen reader, installer/uninstall retains profiles unless explicit purge. Cross-compilation is not native verification.

## 13. Test and release gates
Core: credential parsing, duplicate handling, add rollback, disable selected route, delete selected refusal, vault error, disk error, corrupted/version mismatch, symlink refusal, lock contention, exact state across restart, input bounds and event redaction. Proxy: synthetic upstream sees expected auth/account identity; unauthorized requests never reach upstream; switch while first stream open; second request uses new route; upstream 429 and timeout not replayed; body bounds, path/Origin checks, redirects cannot exfiltrate, cancellation and stream error visible.

UI: add/select/edit/remove with synthetic mock marked Demo only; production empty state and launch errors; real .app inspection. Build: Cargo tests/fmt/clippy, TypeScript check/Vite build, Tauri .app bundle. Repository: source pins resolve, document links resolve, secret pattern review, fresh clone build. Hosted full CI runs nightly only; absent run is NOT_RUN, not green.

Live provider acceptance is separate: user supplies/uses authorized accounts through the built app; verify response identity and next-request switch on exact CLI versions. Until exercised, mark provider integration unverified even if fixture tests and app build pass. Distribution signing/notarization likewise not conflated with compiling a bundle.

## 14. Deliberate follow-on scope
Automatic rotation, encrypted cross-device export, menu-bar monitoring, Desktop restart handoff, Windows release, self-update signatures, session supervisor, project-to-pool rules and Fabric provider admission are detailed follow-on packets. v0.1's local/manual capability must remain useful and honest without them. No implementation is marked complete merely because this specification describes it.

## 15. Implementation-specific limits and receipts
The isolated Codex OAuth snapshot preserves `id_token`, access token and account ID; its `last_refresh` is an RFC3339 local snapshot-write time, **not evidence of a provider refresh**. The native client requires the timestamp as well as tokens: [pinned upstream check](https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/login/src/auth/manager.rs#L580-L598). Refresh token is empty by design to avoid two refresh writers; expiry requires reauthentication. Canonical refresh material remains in Keychain. `codex_auth_snapshot` and its fixture guard this shape.

Working copies and generated scripts persist within app-owned directories; isolated removal cleans its home only when idle. Deleting an account does not revoke it at the provider. App restart rotates proxy port/capability, so previously launched managed clients must be closed and relaunched. No crash-resume promise is made. PID checks conservatively refuse an occupied/reused PID and stale launch reservations require deliberate recovery.

Usage currently collapses available valid windows to their maximum utilization, labelled as the highest reported window. It does not show separate per-window limits or predict exhaustion; resets_at is not populated by the JSON adapter. Failed observations preserve previous data. OAuth identity strings are source claims, not verified profile facts.

Startup corruption/instance-lock failure currently writes a sanitized stderr message and stops; a graphical recovery screen is a follow-on task. The built app is unsigned for distribution, initial validation is Apple Silicon only. See [verification](evidence/verification.md) for exact checks, [operations](OPERATIONS.md) for recovery, and [acceptance packet](packets/provider-acceptance.md) for the first live pass.
