# Shared module contract — 0.3 (IPC v1)
All Rust types use snake_case JSON fields and lowercase enum strings (AuthKind: api_key, setup_token, oauth). Tauri commands receive camelCase argument names (normal Tauri default). Time is Unix seconds UTC. Secret bytes are never returned by an IPC command.

## Core public API (crate switchboard-core; lib switchboard_core)
- `Provider { Claude, Codex }`: Clone, Copy, Eq, Serialize, Deserialize, Hash. `as_str() -> &'static str`.
- `AuthKind { ApiKey, SetupToken, OAuth }`: Clone, Copy, Eq, Serialize, Deserialize.
- `Credential { access_token: String, refresh_token: Option<String>, id_token: Option<String>, expires_at: Option<i64>, account_id: Option<String> }`: Clone, Serialize, Deserialize; deliberately NO Debug. Field data stays in vault. `Credential::parse(provider: Provider, kind: AuthKind, input: &str) -> Result<Self, String>`: token for api_key/setup_token; JSON for oauth (Codex auth.json tokens or Claude claudeAiOauth). Validate kind/provider, nonempty/size bounds, no control chars.
- `Account { id: String, label: String, provider: Provider, kind: AuthKind, pool: String, enabled: bool, created_at: i64, identity: Option<String>, usage: Option<Usage> }`: Clone, Debug, Serialize, Deserialize. identity is account_id claimed by parsed source (not independently verified).
- `Usage { used_percent: f64, observed_at: i64, resets_at: Option<i64>, source: String }`.
- `Event { at: i64, action: String, account_id: Option<String>, detail: String }`: bounded sanitized event vocabulary, never upstream messages.
- `Snapshot { accounts: Vec<Account>, routes: BTreeMap<String,String>, events: Vec<Event> }`.
- `trait Vault: Send + Sync { fn get(&self,id:&str)->Result<Credential,String>; fn put(&self,id:&str,value:&Credential)->Result<(),String>; fn delete(&self,id:&str)->Result<(),String>; }`.
- `MemoryVault::default()` implements Vault for tests only. `NativeVault::new()` (CLI, `serve`) and `NativeVault::desktop()` (the app) use macOS Security.framework items under `ai.passioncode.fabric-switchboard.shared` whose access list trusts the app and its bundled CLI by designated requirement ([KEYCHAIN.md](KEYCHAIN.md), 0.4.1), or Windows DPAPI CurrentUser with encrypted UUID files and user-only DACL. Neither backend falls back to plaintext. Items owned by other programs — Claude Code's `Claude Code-credentials`, an official sign-in's staged item — are reached only through `/usr/bin/security` (0.5, [security_cli](../crates/switchboard-core/src/security_cli.rs)).
- `Store::open(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Store,String>`; process-exclusive filesystem lock for lifetime; never silently fallback on malformed metadata. Store Send+Sync, mutex internally.
- `snapshot(&self) -> Result<Snapshot,String>`
- `add(&self,label:String,provider:Provider,kind:AuthKind,pool:String,credential:Credential)->Result<Account,String>`
- `update(&self,id:&str,label:String,enabled:bool)->Result<(),String>`
- `remove(&self,id:&str)->Result<(),String>` refuses selected accounts; credentials deleted, metadata consistent/fail closed.
- `select(&self,provider:Provider,pool:&str,id:&str)->Result<(),String>` validates same provider/pool/enabled; route key `provider:pool`; atomic metadata publication.
- `route(&self,provider:Provider,pool:&str)->Result<(Account,Credential),String>` returns a consistent snapshot, rejects expired/disabled credentials. Does not auto-refresh or mutate native global auth.
- `credential(&self,id:&str)->Result<Credential,String>`; internal backend use only.
- `observe(&self,id:&str,usage:Usage)->Result<(),String>` validates finite 0..100, timestamps.
- `record(&self,action:&str,id:Option<&str>,detail:&str)->Result<(),String>` only allowlisted detail values (proxy response statuses, launch outcomes etc), avoid taking untrusted raw output.
- Core owns its Cargo.toml, src/*, tests/* only. Other crates/GUI never write metadata directly.

## Proxy public API (crate switchboard-proxy)
`ProxyHandle::start(store:Arc<Store>)->Result<ProxyHandle,String>` async; binds 127.0.0.1:0; random per-launch local capability kept backend-only. `address() -> SocketAddr`; `token() -> &str` used only to launch managed children; `shutdown(self)` async. Routes POST /claude/{pool}/v1/messages, /claude/{pool}/v1/messages/count_tokens and /codex/{pool}/v1/responses; Claude accepts only the optional beta=true query. Local GET /claude/{pool}/api/hello contains no account data; fixed upstream origins and exact paths. Credentials captured at request boundary, auth/account headers replaced, stream relayed, no replay. Local request token never forwarded upstream. Websocket explicitly unsupported. No generic URL-fetcher.

## Tauri IPC v1
- `snapshot() -> Snapshot`
- `add_account(label,provider,kind,pool,secret) -> Account` (secret token or auth JSON)
- `update_account(id,label,enabled) -> ()`; `remove_account(id) -> ()`; `select_account(provider,pool,id) -> ()`
- `runtime_status() -> { proxy_address: String, platform: String, live_mode: String }`
- `launch_account(id,mode,workingDirectory) -> { message:String }`, mode `isolated` or `managed`; workingDirectory is an absolute existing project directory outside app data. Backend fixed provider binaries; no shell interpolation.
- `begin_login(provider,label,pool) -> { login_id:String, message:String }`; `finish_login(loginId) -> Account`; `cancel_login(loginId) -> ()`; isolated official CLI login and native credential capture; no global logout.
- `probe_usage(id) -> Usage` with fixed HTTPS allowlist; unknown is an error, never zero.
Frontend: Rust IPC is authoritative; pure mock fixtures only behind explicit browser demo marker. Errors announced, no credential query endpoints. Add credential input cleared after use. UI reports limitations of launched sessions versus external ones.

## Ownership
Core implementer: crates/switchboard-core only. UI implementer: src/, index.html, package.json, tsconfig.json, vite.config.ts only. Root: docs, proxy, src-tauri, scripts, integration. Every implementation task receives this contract and owns a separate worktree. Integration happens by cherry-pick; never overwrite another task's files.

## CLI/control and native platform extension (0.2)

`switchboard-runtime` owns shared login/launch and the running owner’s control server. Desktop and CLI must use the same default data-root function (macOS Application Support, Windows LOCALAPPDATA) and core’s private filesystem helpers. The secret-bearing control descriptor belongs only in app data; never in project Git files or stdout.

A separate loopback control server dispatches typed metadata/account operations, usage and native login/launch. Commands cannot query credentials. It authenticates before decoding a bounded body, denies browser Origins and wrong Host, and has no arbitrary URL/proxy primitive. CLI must not fall back to writing an already owned Store when a live control call is refused.

Windows protection is an OS implementation behind the same Vault/Store contract; build success does not certify provider login or screen-reader behavior. macOS signing uses an existing Developer ID private key locally without export. Notary acceptance remains a separate receipt.

## Current account and quota extension (0.3)

[PLAN-0.3](PLAN-0.3.md) names the exact additive types/APIs and bounded ownership packets; [account/rotation contract](ACCOUNTS-AND-ROTATION.md) describes behavior. `Credential.native_context` is vault-only. Account adds `external_identity` and `usage_health`; Usage adds `windows`; Snapshot adds `policies`. Metadata writes version 2 and reads version 1 with defaults.

New Tauri/control operations: `current_accounts`, `capture_current(provider,label?,pool)`, `import_claude_swap(pool)`, `activate_native(id)`, `set_policy(policy)`, `monitor_status`. Tauri parameter names remain camelCase; struct fields are snake_case. Monitor status includes sanitized per-policy decisions. Current-source status is available/missing/unavailable, never a token. Runtime invalidates its short source cache after import/capture/activation. Native activation validates the expected live identity again under provider-compatible locks.

`Store::observe_credential` binds an observation to the same credential generation used by the probe/request. A late response cannot assign old-generation usage to a replaced credential. Store owns policy validation and persistence; Runtime owns scheduler lifetime and native side effects. The renderer cannot overwrite persisted cooldown history.

## Agents and project rules extension (0.4)

Behaviour and decisions: [PLAN-0.4](PLAN-0.4.md). Code: `crates/switchboard-core/src/projects.rs`, `crates/switchboard-runtime/src/{projects,agents}.rs`, `crates/switchboard-cli/src/mcp.rs`.

- `ProjectRule { path, provider, account_id, target: "managed"|"claude_cli", enabled, created_at, expires_at? }`; `Snapshot.rules` is omitted when empty. `Store::set_rule(path, account_id, target, enabled, expires_at)` derives the provider from the account and upserts per (`path`, `provider`); `remove_rule(path, provider)`; `resolve_rules(path, now) -> [RuleResolution { provider, effective, nearest }]` uses component-wise longest prefix, and `effective` skips paused and expired rules. Removing an account removes its rules in the same write. `accounts.json` is `schema_version` 3 only while a rule exists, otherwise 2; readers accept 1–3. Event `project_rule`: `saved`, `paused`, `removed`, `applied`.
- Control/Tauri operations: `set_project_rule(path, accountId, target, enabled, expiresAt?)`, `remove_project_rule(path, provider)`, `resolve_project(path)` (read fast path), `apply_project(path, session, global)`. The runtime canonicalizes `path`, requires an existing folder outside app data (removal also accepts a deleted absolute folder). `apply_project` changes state only for the calling session: a managed rule in the session's pool selects with the rotation cooldown; another pool is `other_pool`; an isolated or native session gets `other_session` for managed rules; a `claude_cli` rule in a native session needs `global: true` (`needs_global`) and then activates. Paused, expired or missing rules change nothing.
- `Session` is `{mode:"managed",provider,pool}`, `{mode:"isolated",provider,account_id}` or `{mode:"native",provider?}`. Launches set `SWITCHBOARD_SESSION=managed:<provider>:<pool>` or `isolated:<provider>:<account id>`; detection falls back to an `ANTHROPIC_BASE_URL` naming the running proxy, then `CODEX_HOME` under `<app data>/runtimes/codex-<pool>`, then native (`CLAUDECODE=1` marks Claude).
- Launch attaches the tools when a CLI is found (this CLI, the one bundled beside the desktop executable, PATH, `~/.local/bin`): Claude gets `--mcp-config <home>/switchboard-mcp.json`, Codex gets `[mcp_servers.switchboard]` in its private `config.toml`; isolated sessions get `mcp --read-only`. The launch result carries `agent_tools: bool`. No CLI found is not a launch failure.
- Only `Owner::native` reads the ordinary CLIs' sign-in; `Owner::start` (tests, smoke) reports current accounts as unavailable.
- `switchboard mcp [--read-only]`: stdio JSON-RPC 2.0 per the tool table in PLAN-0.4. Tool failures are results with `isError: true`; protocol faults are JSON-RPC errors (-32700 parse/oversize > 1 MiB, -32600 invalid, -32601 method, -32602 unknown tool or bad arguments). Offline calls retry a held store lock for up to 1.5 s.
- Desktop: `agent_setup() -> { cli_path, bundled_cli, linked_cli, can_link, commands }`; `link_cli()` creates `~/.local/bin/switchboard` → bundled CLI and never replaces a file or a foreign link. The macOS build embeds the signed CLI at `Contents/MacOS/switchboard`, byte-identical to the archived CLI.

## 0.5 additions

- Operations ([runtime](../crates/switchboard-runtime/src/lib.rs)): `LoginStatus { login_id }` → `{state: pending|complete|ended}`; `Backups` → `{directory, enabled, backups: [{file, created_at, accounts, missing, own, openable}], last_error, last_written_at}`; `BackupNow` → backup info or null (refused without a running owner on the default data folder); `RestoreBackup { file }` → `{added, skipped, failed}`, `file` must be a `switchboard-backup-<digits>-<32 hex UUID>.json` name in the backup folder; legacy `switchboard-backup-<digits>.json` names are still accepted. New generations reserve a private destination exclusively and never replace another backup (`backup::tests::same_second_partial_backup_never_replaces_the_complete_one`). Tauri commands `login_status`, `backups`, `backup_now`, `restore_backup`; CLI `login status`, `backup list|now|restore`. None returns a credential.
- `MonitorStatus` adds `sign_in_required: [account id]` and `limited: [{account_id, until, source: managed|claude_code}]`; the MCP `switchboard_status` result adds `limited`. Decision reasons add `limit_reached`, `limit_no_eligible_account`, `switched_on_limit`.
- `Store::rotation_decision_with(policy, current, now, limited)`; `rotation_decision` passes an empty set. `Store::adopt_refreshed`, `Store::export`, `Store::changes`, `Store::backup_id`.
- Backup file ([backup](../crates/switchboard-core/src/backup.rs)): JSON `{format: "fabric-switchboard-backup", version: 1, created_at, accounts, missing, store, key, nonce, data}`; `data` is AES-256-GCM over the accounts, policies and credentials, AAD `format:v1:created_at:store:accounts:missing`; `key` is the first 16 hex digits of SHA-256 of the key.

## 0.5.1 additions

Behaviour: [ACCOUNTS-AND-ROTATION](ACCOUNTS-AND-ROTATION.md); decisions and REQ mapping: [PLAN-0.5 §0.5.1](PLAN-0.5.md#051--claude-swap-parity-and-review-fixes); evidence: [release-0.5](evidence/release-0.5.md#051).

- `MonitorStatus` adds `claude_swap_accounts: number` — identities a running Claude Swap manages, which Switchboard does not renew. The Claude Swap import result adds `claude_swap_running: bool`. Neither carries a token.
- New refusals, in the error vocabulary checked by `scripts/check-error-vocabulary.mjs`: *Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.*; switching away from an unsaved account (`UNSAVED_CURRENT`); *The Claude Code sign-in does not match the account named in its settings…* (`FOREIGN_LIVE`); *Switchboard could not confirm which account Claude Code is signed in to…* (`UNCONFIRMED_LIVE`); *Usage checks are rate limited by the provider…* (`USAGE_RATE_LIMITED`); native operations from inside Switchboard's own data folder.
- Runtime-internal seam `NativeSources { current, activate, live }`. `activate(credential, target, expected, preserve)` calls `preserve(&Outgoing { auth, config })` under Claude Code's locks before the first write; an `Err` aborts the switch with nothing written. `live() -> Box<dyn external::LiveItem>` holds those locks for an idle renewal (`read`, `write`). Synthetic owners use `no_live` and `UNAVAILABLE`, so tests never reach the real sign-in.
- `Store::adopt_refreshed_for(provider, consumed, refreshed, owner?) -> (updated, others)`: stores a successor in the accounts still holding the spent token, only the `owner`'s when named; `others` are the holders left with the spent token. `adopt_refreshed` is the owner-less form and errors when nobody holds it.
- `switchboard_proxy::probe_usage_detailed -> Result<Usage, ProbeFailure { message, rate_limited }>`; `rate_limited` is `Some(Some(seconds))` for a 429 with a numeric `Retry-After`, `Some(None)` without one. `probe_usage` keeps its `Result<Usage, String>` shape.
- `<data>/renewal-state.json` keeps fingerprints of rejected refresh lineages (SHA-256, never the token) so a restart does not retry them.
- Backups (0.5) go to `SWITCHBOARD_BACKUP_DIR` when absolute, else `Fabric Switchboard Backups` beside the data folder (`~/Library/Application Support` on macOS, `%APPDATA%` on Windows), so removing the data folder keeps them (`backup_dir`).

## 0.5.2 additions

- `MonitorStatus` adds `renewal_blocked: bool` — true for an hour after the token endpoint answered `invalid_client`; no renewal is attempted meanwhile. Nothing new is journaled, so a 0.5.1 binary still opens the store.
- A `~/.claude.json` created by a native switch carries `hasCompletedOnboarding: true`.

## 0.5.3 additions

- `MonitorStatus` adds `claude_swap_switching: bool`; decision reason `claude_swap_switching`. `agent_setup()` adds `translocated: bool`.
- New refusals in the error vocabulary: `RENEWING`, `SWAP_BUSY`, `STORE_BUSY` and `STARTUP_FAILED` (desktop only, `src-tauri/src/main.rs`; every other owner start-up error is replaced by the latter), `EXTERNAL_REFUSED` (core `external_keychain`).
- `ImportBatch` adds `failed_emails` (lower-cased); `SwapView::Profiles(profiles, failed_emails)`; Claude Swap holds are kept per identity with its email.
- `NativeSources` adds `swap: fn() -> Result<external::ImportBatch, String>` (synthetic owners: `no_swap`). `external::claude_swap_activity() -> SwapActivity { running, switching }`, `claude_swap_signature()`, `capture_current_cached` (30 s, Claude only) and `forget_current()`. 0.5.4: `capture_current_cached` serves Claude and Codex from a watched read — reused while a quiet probe (`external::Probe`) finds the source unchanged, never retried in a loop when locked, refused or needing consent — and `NativeSources` gains `fresh` (Capture's direct read), `swap_activity`, `swap_signature` and `transcripts`, so the monitor reads nothing real in tests.
- `RefreshState::offline(journal)`: no grants; `set_grants(false)` for a `--data-dir` owner. `refresh::catch_up_with_claude_swap` runs inside `activate_native`, the function every switch path shares.
- `switchboard_core::external_keychain::read_external_quietly(service, account)` (macOS): a generic-password item read with Keychain interaction off.
- Desktop: the `Owner` lives in a `Slot` started at launch and retried by the next request when that failed; `tauri-plugin-single-instance` `~2.4.0` (Tauri stays 2.11.6). Release profile: LTO, one codegen unit, stripped.

## Quota evidence age and presentation (2026-10-04)

Partial response headers retain omitted, unexpired windows as history. When retaining a window, aggregate `Usage.observed_at` is the oldest retained evidence time; `UsageHealth.checked_at` records the actual latest attempt. Repeated partial headers do not grant missing windows new freshness. Authoritative endpoint JSON replaces historical windows. Delayed observations are rejected against check chronology; accepted credential generation must still match (`partial_headers_never_freshen_an_unobserved_low_quota_window`, core `observe_generation`). No schema migration.

Account list order is display-only; it never selects/activates. Within provider/pool boundaries it ranks fresh available quota by headroom, blocked accounts by known wait (latest exhausted-window reset and active hold), unknown, sign-in-required and disabled. A runtime `until` is a retry hold and may be inferred; it is not guaranteed capacity. Date and wall-clock duration are displayed separately, with a pausable minute timer. Passed resets invalidate the measurement until checked (SCN-032, `scripts/test-ui-logic.mjs`).
