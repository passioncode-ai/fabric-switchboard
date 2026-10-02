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
