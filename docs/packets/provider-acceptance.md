# Packet PA-01 — real provider acceptance

**State:** NOT_RUN. First task for the next operator-assisted session. No live-provider success may be inferred from fixtures.
**Owns:** provider adapters in `src-tauri/src/launch.rs`, proxy endpoint/headers in `crates/switchboard-proxy`, evidence in `docs/evidence`; UI only for observed recovery defects.
**Shared context:** [contracts](../CONTRACTS.md), [spec §§4–9](../SPEC.md), [research](../research/README.md), [operations](../OPERATIONS.md).

Prerequisites: unlocked Mac, installed official CLIs, operator-owned authorized accounts, operator completing browser authentication, a disposable project with no hooks/MCP side effects, and an explicit bounded usage budget. Never mine another application's active credential store to avoid these prerequisites.

Record OS/architecture, app implementation commit, `claude --version`, `codex --version`, credential kind, scope of account authorization and execution time. Do not record emails, account IDs, tokens, prompts containing private data, or raw provider error bodies in Git. Use stable aliases A/B in evidence.

| Case | Action | Required observation |
|---|---|---|
| Claude onboarding | official sign-in A into staged home, Finish | app adds one account; only staged Keychain record is removed; global client remains usable |
| Codex onboarding | same, official Codex login | file captured into vault; id_token/access/account fields usable; staging removed |
| OAuth isolated | launch each with chosen project | CLI starts in project, first bounded response succeeds, correct profile identity observed |
| API/setup isolated | explicit permitted test credential | applicable CLI succeeds; unsupported combinations clearly refuse |
| Managed A | select A then launch | fixed endpoint accepts auth; CLI receives a real response; no fake verified badge |
| Live A → B | keep a long A response in progress, select authorized same-pool B, then send next request | first completes A, next uses B; corroborate provider identity/usage where available; local route selection alone is insufficient |
| Error | provoke controlled invalid credential/rejection | one upstream attempt by proxy, clear CLI error, no silent account fallthrough |
| Usage | explicit check on supported subscriptions | parse actual response; display unknown/errors honestly; timestamp/percent accurate |
| Cancel | start then close sign-in, Cancel; retry Finish after cleanup error | no duplicate account, no global auth mutation, staging cleanup bounded |
| Restart | close app while managed client exists, reopen | old client fails, deliberate new launch works, account/routes persist |
| Expiry | controlled expired fixture plus observed native refresh behavior | reauthentication instruction, no hidden second refresh writer |

Stop on unanticipated permission, billable behavior outside the agreed bound, invalid provider contract or repeated refresh. Preserve a sanitized finding, fix it with fixtures first, and rerun only affected cases. Completion is an exact-version compatibility table, not “all future CLI versions supported.”
