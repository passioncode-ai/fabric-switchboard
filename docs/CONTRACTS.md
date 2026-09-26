# Shared module contract v1
All Rust types use snake_case JSON fields and lowercase enum strings (AuthKind: api_key, setup_token, oauth). Tauri commands receive camelCase argument names (normal Tauri default). Time is Unix seconds UTC. Secret bytes are never returned by an IPC command.

## Core public API (crate switchboard-core; lib switchboard_core)
- `Provider { Claude, Codex }`: Clone, Copy, Eq, Serialize, Deserialize, Hash. `as_str() -> &'static str`.
- `AuthKind { ApiKey, SetupToken, OAuth }`: Clone, Copy, Eq, Serialize, Deserialize.
- `Credential { access_token: String, refresh_token: Option<String>, expires_at: Option<i64>, account_id: Option<String> }`: Clone, Serialize, Deserialize; deliberately NO Debug. Field data stays in vault. `Credential::parse(provider: Provider, kind: AuthKind, input: &str) -> Result<Self, String>`: token for api_key/setup_token; JSON for oauth (Codex auth.json tokens or Claude claudeAiOauth). Validate kind/provider, nonempty/size bounds, no control chars.
- `Account { id: String, label: String, provider: Provider, kind: AuthKind, pool: String, enabled: bool, created_at: i64, identity: Option<String>, usage: Option<Usage> }`: Clone, Debug, Serialize, Deserialize. identity is account_id claimed by parsed source (not independently verified).
- `Usage { used_percent: f64, observed_at: i64, resets_at: Option<i64>, source: String }`.
- `Event { at: i64, action: String, account_id: Option<String>, detail: String }`: bounded sanitized event vocabulary, never upstream messages.
- `Snapshot { accounts: Vec<Account>, routes: BTreeMap<String,String>, events: Vec<Event> }`.
- `trait Vault: Send + Sync { fn get(&self,id:&str)->Result<Credential,String>; fn put(&self,id:&str,value:&Credential)->Result<(),String>; fn delete(&self,id:&str)->Result<(),String>; }`.
- `MemoryVault::default()` implements Vault for tests only. `NativeVault::new()` macOS Security.framework, Windows unsupported with explicit error until implemented.
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
`ProxyHandle::start(store:Arc<Store>)->Result<ProxyHandle,String>` async; binds 127.0.0.1:0; random per-launch local capability kept backend-only. `address() -> SocketAddr`; `token() -> &str` used only to launch managed children; `shutdown(self)` async. Routes POST /claude/{pool}/v1/messages and /codex/{pool}/v1/responses; fixed upstream origins and exact paths. Credentials captured at request boundary, auth/account headers replaced, stream relayed, no replay. Local request token never forwarded upstream. Websocket explicitly unsupported. No generic URL-fetcher.

## Tauri IPC v1
- `snapshot() -> Snapshot`
- `add_account(label,provider,kind,pool,secret) -> Account` (secret token or auth JSON)
- `update_account(id,label,enabled) -> ()`; `remove_account(id) -> ()`; `select_account(provider,pool,id) -> ()`
- `runtime_status() -> { proxy_address: String, platform: String, live_mode: String }`
- `launch_account(id,mode) -> { message:String }`, mode `isolated` or `managed`. Backend fixed provider binaries; no shell interpolation.
- `begin_login(provider,label,pool) -> { login_id:String, message:String }`; `finish_login(loginId) -> Account`; isolated official CLI login and native credential capture; no global logout.
- `probe_usage(id) -> Usage` with fixed HTTPS allowlist; unknown is an error, never zero.
Frontend: Rust IPC is authoritative; pure mock fixtures only behind explicit browser demo marker. Errors announced, no credential query endpoints. Add credential input cleared after use. UI reports limitations of launched sessions versus external ones.

## Ownership
Core implementer: crates/switchboard-core only. UI implementer: src/, index.html, package.json, tsconfig.json, vite.config.ts only. Root: docs, proxy, src-tauri, scripts, integration. Every implementation task receives this contract and owns a separate worktree. Integration happens by cherry-pick; never overwrite another task's files.
