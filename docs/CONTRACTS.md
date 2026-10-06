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

## Other agents (0.6)

Operator request 2026-10-05: support the popular coding agents (OpenRouter's ranking), not only Claude Code and Codex. Code: [`catalog/agents.json`](../catalog/agents.json) (30 agents, sourced in [research](research/agents-2026-10-05.md)), `crates/switchboard-runtime/src/agent_catalog.rs`, `launch::launch_agent`, proxy `agent_token`. User page: [AGENT-SUPPORT](AGENT-SUPPORT.md), generated by `scripts/agents_doc.py` (the gate fails when stale).

- **Agent capability.** The proxy accepts a second capability, `agent_token(session token)` = SHA-256 of `switchboard-agents:<token>` in hex: derived, so it survives a restart with the session token and is never stored; knowing it does not reveal the session token. It is accepted as `Authorization: Bearer` or `x-api-key`; the session token only as a bearer. With the agent capability the proxy serves **API-key accounts only**: a subscription sign-in selected in the pool answers 403 `AGENT_SUBSCRIPTION_REFUSED` before any provider call (`agents_reach_api_key_accounts_and_never_a_subscription_sign_in`).
- **Attribution (fabric-workspace roadmap RM-16, OpenRouter app attribution, 2026-10-05).** Traffic Switchboard forwards for someone else's agent keeps that agent's attribution: the proxy relays only the `REQUEST_HEADERS` allowlist, which has no `HTTP-Referer` or `X-OpenRouter-Title`, and adds neither. Switchboard makes no model call of its own; if it ever does, it attributes as `https://passioncode.ai/switchboard/`.
- **Chat Completions.** `POST /codex/<pool>/v1/chat/completions` relays to OpenAI `/v1/chat/completions` for an API-key account; a ChatGPT sign-in answers 404 (`chat_completions_reach_openai_with_an_api_key_only`).
- Operations `AgentCatalog`, `AgentConnect { agent, pool }` (MCP registration, endpoints with the right path suffix, `key_command` — never a key value: `connect_names_the_key_command_and_never_a_key`), `AgentKey`, `LaunchAgent { agent, pool, working_directory }`: a `launch` agent gets its base and key variables; a `proxy` agent with a binary starts with the provider it was given by `connect`; both need the pool's selected Claude account to be an API key, and `project_allows` holds (`a_third_party_agent_never_starts_on_a_subscription_account`). CLI `switchboard agents list|connect|key|launch`; desktop *Agents → Other agents*.

## Workflow continuation (SB-52, 2026-10-05)

N-018 walking skeleton ([packet](packets/n-018-continuation.md)): when the executor of an Observatory workflow runs out of its limit, another account continues it — of the same provider, or (0.6.7, SB-70, N-018 step 2) of the other provider: the handoff then names the receiving harness (`continuation::provider_name`: `claude-code`, `codex`), `Plan.from_provider` and `Continuation.previous_provider` carry the previous executor's provider, and the first prompt says the work comes from another agent whose transcript pointer does not apply and tells the session to accept naming no other provider (the engine refuses an acceptance naming another provider than the offer). An executor of an agent Switchboard does not hold (Hermes, Kimi Code) can be taken over the same way. Code: `crates/switchboard-runtime/src/continuation.rs`, `launch::preflight`, `launch::launch_continuation`, `Operation::Continue`; CLI `switchboard continue`. Engine contract: memory/0.1, `project-observatory full workflow show --json` and `handoff` (engine 0.16.0).

- **Order.** Read (`full workflow show <wf> --json`) → plan → `launch::preflight` → resolve the Observatory MCP server → offer (`full workflow handoff <wf> --to-provider <executor's provider> --to-account <account id> --reason limit`, never `--force`) → launch. Every refusal before the offer leaves the workflow as it was; there is no automatic retry.
- **Plan refuses** a workflow id not shaped `wf_` + 16 hex (before the engine is called), a closed workflow, one without a checkpoint, a waiting handoff, any declared key whose state is not `vault` (named `project/env/name (state)`), a disabled account, an executor provider Switchboard cannot map (`claude`, `claude-code`, `anthropic` → Claude; `codex`, `openai` → Codex) or of another provider than the account, the account that already executes it, and a folder outside every `git` artifact checkout of the checkpoint (`every_refusal_is_decided_before_an_offer`). Without git artifacts the named folder is used. A checkout path component the engine redacted (`[redacted]`, a UUID folder name) matches exactly one component (`a_redacted_folder_name_matches_one_folder_and_nothing_more`).
- **Preflight** is what `launch` refuses before touching a home — folder, mode, account, enabled, `project_allows` — plus the managed route selection or, for an isolated session, a credential that has not expired (`preflight_refuses_what_a_launch_would_refuse_without_writing`).
- **Offer.** The engine's answer `handoff <handoff:16 hex> offered until <time>` is parsed; anything else is unreadable. Its refusal (`HandoffRefused` while the executor wrote less than 120 s ago, a rate limit, a closed workflow) is reported as its own first stderr line, control characters removed, at most 300 characters. An offer whose pack says a key left the vault launches nothing. After the offer, a failed launch names the handoff and its deadline: the offer lapses on its own.
- **Launch.** As `launch`, plus `OBSERVATORY_WORKFLOW_ID` and `OBSERVATORY_HANDOFF_ID` in the script, the Observatory server in the session's own MCP config beside Switchboard's (Claude's `switchboard-mcp.json`, Codex's `[mcp_servers.observatory]`): the interpreter from the engine launcher's `#!` line, `<full-path>/mcp/server.py`, and `--home` when Switchboard's environment names `OBSERVATORY_FULL_HOME` or `OBSERVATORY_HOME`. The first prompt names the workflow and handoff ids and the rules: accept with `observatory_handoff_accept` and its own `sessionId`, continue from the checkpoint in that answer, obey its constraints, checkpoint after every step (`a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt`).
- **Never.** Switchboard does not read or pass the old session's transcript, a lease token or a credential; it does not accept on the session's behalf. Engine calls are bounded (60 s, 1 MiB per stream, killed and reaped).
- **Platform.** macOS (and other Unix): the engine launcher's interpreter is read from its `#!` line. Windows answers `Continuing a workflow runs on macOS for now.`
- **Contract against the installed engine:** `real_engine_contract` (ignored by default; runs with `OBSERVATORY_HOME=<scratch>` and `SWITCHBOARD_OBSERVATORY_CONTRACT=<wf>:<checkout>`) — read, plan, MCP server, a real offer, and the second plan refused with the handoff waiting. Run 2026-10-05 on engine 0.16.0 in a scratch workspace: offered `handoff:34f6193d9e640027`. The engine's silence refusal, measured the same day: `workflow: HandoffRefused: wf_…: the executor wrote a checkpoint 0 s ago; without its lease token a handoff waits until it has been silent for 120 s (retry after 120 s)`.

## Projects (0.6)

Operator request 2026-10-05: a project is one or more folders (related repositories) with accounts of its own, so other projects' agents never switch to them. Code: `crates/switchboard-core/src/projects.rs` (`Project`, `Store::save_project`, `remove_project`, `Snapshot::project_for`, `project_of_pool`), `crates/switchboard-runtime/src/launch.rs` (`project_allows`).

- `Project { pool, name, folders, created_at }`; `Snapshot.projects`, omitted when empty. The project's accounts are the accounts in its `pool`; an account belongs to one project at a time. `accounts.json` is `schema_version` 4 while a project or a `project` journal entry exists (0.5 readers refuse it rather than ignore a reservation); readers accept 1–4.
- `save_project(pool?, name, folders, accounts)`: without `pool` a new project gets one from its name (`pool_from_name`, a suffix `-2`… when taken, never `default`); `accounts` is the whole set — listed accounts move into the pool, the pool's others move to `default`; a moved account's old managed route is cleared; a native (`claude_cli`) policy on the pool is switched off; 1–16 canonical folders, and no folder of one project inside or around another's (`A folder is already part of another project.`); the same identity already in the target pool is refused. Journal `project`: `created`, `updated`, `removed`. `remove_project(pool)` keeps the accounts in the pool, no longer reserved.
- **Reservation, enforced:** a launch (isolated or managed) of a project account outside the project's folders → `This account belongs to a project. Launch it from one of the project's folders.`; inside a project's folders, an account of another pool for a provider the project has an enabled account of → `This folder belongs to a project. Launch one of the project's accounts.` (`a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts`). Native activation — the Switch button, a `claude_cli` rule, native rotation — of a project account and a native policy on a project pool are refused with `PROJECT_NOT_NATIVE`, because the ordinary Claude Code serves every folder (`a_project_account_never_becomes_the_ordinary_claude_code`, `a_project_pool_never_drives_the_ordinary_claude_code`). An account the ordinary Claude Code or Codex is signed in to cannot join a project, and *Capture current account* into a project's pool is refused the same way; *Import from Claude Swap* into a project's pool is refused with `PROJECT_NOT_NATIVE`, since Claude Swap switches the ordinary Claude Code (`PROJECT_ACCOUNT_IN_CLI`, `the_account_the_cli_uses_cannot_join_a_project`). Managed rotation and `switchboard_switch` stay inside the pool, as for any pool.
- Operations `SaveProject { pool?, name, folders, account_ids }`, `RemoveProject { pool }`; `ResolveProject` adds `project` (`{pool, name, folders, created_at, accounts}`, no credential) for the folder. Tauri `save_project`, `remove_project`; CLI `project save|delete`, `project list` → `{projects, rules}`; MCP `switchboard_project_context` returns the project.

## 0.5 additions

- Operations ([runtime](../crates/switchboard-runtime/src/lib.rs)): `LoginStatus { login_id }` → `{state: pending|complete|ended}`; `Backups` → `{directory, enabled, backups: [{file, created_at, accounts, missing, own, openable}], last_error, last_written_at}`; `BackupNow` → backup info or null (refused without a running owner on the default data folder); `RestoreBackup { file }` → `{added, skipped, failed}`, `file` must be a `switchboard-backup-<digits>-<32 hex UUID>.json` name in the backup folder; legacy `switchboard-backup-<digits>.json` names are still accepted. New generations reserve a private destination exclusively and never replace another backup (`backup::tests::same_second_partial_backup_never_replaces_the_complete_one`). Tauri commands `login_status`, `backups`, `backup_now`, `restore_backup`; CLI `login status`, `backup list|now|restore`. None returns a credential.
- `MonitorStatus` adds `sign_in_required: [account id]` and `limited: [{account_id, until, source: managed|claude_code}]`; the MCP `switchboard_status` result adds `limited`. Decision reasons add `limit_reached`, `limit_no_eligible_account`, `switched_on_limit`.
- `Store::rotation_decision_with(policy, current, now, limited)`; `rotation_decision` passes an empty set. `Store::adopt_refreshed`, `Store::export`, `Store::changes`, `Store::backup_id`.
- Backup file ([backup](../crates/switchboard-core/src/backup.rs)): JSON `{format: "fabric-switchboard-backup", version: 1, created_at, accounts, missing, store, key, nonce, data}`; `data` is AES-256-GCM over the accounts, policies and credentials and, since 0.6, the project rules, projects, managed selections (`routes`) and the settings files `login-item` and `auto-update` (`SETTING_FILES`; absent in a 0.5 backup, ignored by a 0.5 reader); a rule change now counts for the next backup (`a_rule_change_counts_for_the_next_backup`). Restore maps each backup account to its id on this install, then puts back projects whose pool and folders are free, unexpired rules and selections not already set, and settings not chosen here (`a_reinstall_gets_projects_rules_selections_and_settings_back`); the result adds `projects`, `rules`, `routes`, `settings`. **A data folder created by this start** (a new install, or after `switchboard uninstall`) with no accounts restores the newest backup this machine can open at once, reported as `Backups.restored_at_start`; a store that existed before the start is never restored on its own (`a_new_data_folder_restores_the_newest_backup_and_an_existing_one_never_does`). AAD `format:v1:created_at:store:accounts:missing`; `key` is the first 16 hex digits of SHA-256 of the key.

## 0.5.1 additions

Behaviour: [ACCOUNTS-AND-ROTATION](ACCOUNTS-AND-ROTATION.md); decisions and REQ mapping: [PLAN-0.5 §0.5.1](PLAN-0.5.md#051--claude-swap-parity-and-review-fixes); evidence: [release-0.5](evidence/release-0.5.md#051).

- `MonitorStatus` adds `claude_swap_accounts: number` — identities a running Claude Swap manages, which Switchboard does not renew. The Claude Swap import result adds `claude_swap_running: bool`. Neither carries a token.
- New refusals, in the error vocabulary checked by `scripts/check-error-vocabulary.mjs`: *Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.*; switching away from an unsaved account (`UNSAVED_CURRENT`); *The Claude Code sign-in does not match the account named in its settings…* (`FOREIGN_LIVE` — only when the token belongs to an account Switchboard does not hold; a token the provider names as another **saved** account's is filed under that account and the switch goes on; one only a stored copy attributes is `UNCONFIRMED_LIVE`. Runtime-internal: `refresh::attribution -> Attribution { Own, Other(Option<ExternalIdentity>), Unresolved }` (cached provider answer first, then stored copies; a token held by copies of two different accounts is unattributed) and `refresh::owner_to_file`); *Switchboard could not confirm which account Claude Code is signed in to…* (`UNCONFIRMED_LIVE`); *Usage checks are rate limited by the provider…* (`USAGE_RATE_LIMITED`); native operations from inside Switchboard's own data folder.
- Runtime-internal seam `NativeSources { current, activate, live }`. `activate(credential, target, expected, preserve)` calls `preserve(&Outgoing { auth, config })` under Claude Code's locks before the first write; an `Err` aborts the switch with nothing written. `live() -> Box<dyn external::LiveItem>` holds those locks for an idle renewal (`read`, `write`). Synthetic owners use `no_live` and `UNAVAILABLE`, so tests never reach the real sign-in.
- `Store::adopt_refreshed_for(provider, consumed, refreshed, owner?) -> (updated, others)`: stores a successor in the accounts still holding the spent token, only the `owner`'s when named; `others` are the holders left with the spent token. `adopt_refreshed` is the owner-less form and errors when nobody holds it.
- `switchboard_proxy::probe_usage_detailed -> Result<Usage, ProbeFailure { message, rate_limited }>`; `rate_limited` is `Some(Some(seconds))` for a 429 with a `Retry-After` (numeric in 0.5.1; HTTP-dates too since SB-39, below), `Some(None)` without one. `probe_usage` keeps its `Result<Usage, String>` shape.
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

## Provider not-before for quota checks (SB-39, 2026-10-04)

- **One gate, every caller.** `monitor::check` — reached by the desktop (`probe_usage`), the CLI and MCP through the owner (`Operation::Usage`), the offline CLI and the background pass — first asks `UsageGate` (`crates/switchboard-runtime/src/usage_gate.rs`). While a provider not-before applies to the account's stored token, it answers `USAGE_RATE_LIMITED` with no renewal and no request (`no_caller_checks_before_the_providers_not_before`, `the_offline_cli_honours_a_wait_the_owner_recorded`).
- **Not-before.** Set only by a usage check the provider answered 429: `checked_at + not_before_delay(retry_after)`, where a positive `Retry-After` is kept whole up to `MAX_NOT_BEFORE` = 7 days, and an absent, malformed, zero or past value gives `RATE_LIMIT_FLOOR` = 900 s (`a_provider_wait_is_kept_whole_and_a_useless_one_gets_the_floor`). The 0.5.1 six-hour cap is retired. The background's next check is `max(ordinary backoff, not-before)`; an ordinary failure (network, 5xx, unreadable vault) sets no not-before, so a person may check again at once (`an_ordinary_failure_leaves_a_manual_retry_open`).
- **`Retry-After` parsing.** `switchboard_proxy::retry_after_at(value, received_at) -> Option<i64>`: delay-seconds, or an HTTP-date in IMF-fixdate, RFC 850 or asctime form (RFC 9110 §10.2.3, §5.6.7), as seconds from the response's own `Date` when valid (the provider's clock), else from receipt (`a_dated_retry_after_is_measured_on_the_providers_clock`); a past date is `0`; overflow saturates; a sign, fraction, other zone or a weekday that contradicts the date is `None` (`retry_after_reads_seconds_and_every_http_date_form`). `ProbeFailure.rate_limited` is now `Some(Some(seconds))` for both forms, `Some(None)` only when absent or malformed.
- **`<data>/usage-holds.json`** (0600, ≤ 256 KiB): `{ version: 1, holds: { <account id>: { not_before, recorded_at, credential } } }`, `credential` being the first 128 bits of SHA-256 over the access token and its expiry — the same generation the store counts — and the very credential the probe sent — never a token, never a provider body. Kept outside `accounts.json` so an older build kept for rollback still opens the store. Owner: the process holding the store lock. Writes happen outside the in-memory lock and never replace a newer state with an older one. An entry is dropped when it passes, when its account is removed (`a_removed_account_leaves_no_hold_behind`), or when the account's stored token differs (`a_new_credential_generation_starts_without_the_old_wait`, `a_new_sign_in_is_checked_despite_the_old_tokens_wait`). Unreadable → ignored and rewritten at the next hold; unsaved → kept in memory and logged (`usage_holds_unsaved`). Read at owner start and by the offline CLI (`a_hold_survives_a_restart_and_never_stores_a_token`); on read, passed entries are dropped and every wait is bounded by `MAX_NOT_BEFORE` from its record time (`holds_read_from_disk_are_bounded_and_expired_ones_dropped`). The hold is recorded before the failed-health write, so a refused write cannot drop it (`a_wait_is_kept_even_when_its_health_record_cannot_be_written`). A poisoned lock still answers from its data rather than letting a check through.
- **Clock.** A hold recorded more than 60 s ahead of the clock is rebased to now plus its remaining length and written back — by a check, and by the background's due filter, which needs no credential for it (`a_hold_written_under_a_clock_set_back_is_rebased_not_frozen`, `the_background_rebases_a_hold_without_a_credential`). The due filter skips a held row (`a_held_row_takes_no_slot_of_the_pass`).
- **Coalescing and the transaction.** Same-account checks share one request: a caller that asked while a check ran takes its answer — unless the store's change count moved after that check began (a new sign-in, a renewal, an edit), when it checks on its own (`a_new_sign_in_during_a_check_is_checked_on_its_own`). `Runtime::execute` answers `Operation::Usage` without the owner transaction; only the renewal inside `check` takes it, as the background pass does (`same_account_checks_share_one_request_and_hold_no_transaction`). A quit therefore does not wait for a quota request in flight; `Owner::shutdown` turns refresh grants off before its drain, so such a check cannot spend a refresh token after it — its observation is lost at worst, never a credential (`a_quota_check_left_running_at_quit_spends_no_refresh_token`). Coalescing compares the store's global change count: a concurrent unrelated edit costs at most one extra request, never a stale answer (judgement, seam review).
- **Renderer.** No new IPC field or error sentence. A failed row with no stored observation shows `Next check {date}` (`failedNextCheck`, `scripts/test-ui-logic.mjs`).
- Tests reach a synthetic usage endpoint through the proxy feature `synthetic-origins`, enabled only by the runtime's dev-dependencies; shipped binaries call only the fixed provider origins.
- **Stop signals at start-up (found by this run's gate).** `StopSignals::listen()` (runtime) registers SIGTERM/SIGINT; `switchboard serve` and the desktop app call it before the owner publishes `control.json`, then await `requested()`. Before, the handler was installed after the descriptor, so a stop in between (launchd, `kill`) ended the process by default action — `serve_stops_on_sigterm_and_sigint_within_the_deadline` failed 1 run in 6 that way; 12 of 12 after. `stop_requested()` remains for callers that need no early listen.

## Codex limits beyond the primary and secondary windows (SB-40, 2026-10-04)

- **Source.** The official client at `openai/codex@afb436d` (`backend-client` `rate_limit_snapshots_from_payload`; wire names in `codex-backend-openapi-models`). Run record: `docs/runs/2026-10-04-sb-40-codex-limits/README.md`.
- **No schema change.** New dimensions are ordinary `UsageWindow`s with reserved names, so `Usage.used_percent` stays the maximum over `windows` and an older build (0.5.3) reads them as windows — conservatively.
- **Account-wide windows** (count everywhere): `spend_limit` — the member's individual spend control (`used_percent` clamped to 100 — the wire value is an unbounded integer — and its reset read separately, so a bad reset loses only itself; 100 when `reached`; `an_overspent_limit_is_clamped_and_a_bad_reset_loses_only_itself`); `workspace_credits` and `workspace_usage_limit` — 100, no reset, from the workspace reached-types (an organisation limit another seat of the same workspace shares); `limit_reached` — 100, no reset, when `rate_limit.limit_reached` or `allowed: false` or the reached-type `rate_limit_reached` arrives with no main window at 100 (`codex_usage_keeps_every_dimension_the_official_client_reads`, `spend_control_and_reached_limits_block_whatever_the_percentages`).
- **Feature windows** `feature_<metered_feature>_primary|secondary` (sanitised to `[a-z0-9_]`, id ≤ 40 bytes, at most 12 and never more than the main windows and blockers leave of the store's 16, sorted by name so a reordered list is no quota change, duplicates dropped; `switchboard_core::FEATURE_WINDOW_PREFIX`; `every_blocker_with_many_features_still_fits_the_store`, `feature_order_does_not_change_the_observation`). They do not stand for the account — neither their percentage nor their reset: `Usage::account_used_percent()` is the maximum over the other windows, `None` when only feature windows exist; a feature window's passed reset leaves the account fresh in rotation, the list and MCP, and the quota cell shows the reset of the account window in highest use (`accountReset`). An account with no primary/secondary window but a measured `spend_limit` has known capacity — the spend limit is the account's constraint on a usage-based plan (review ruling) (`a_feature_limit_is_scoped_and_does_not_stand_for_the_account`, `a_plan_without_main_windows_has_unknown_account_capacity`, `feature_names_are_sanitised_bounded_and_unique`).
- **Readers.** Rotation eligibility (`rotation::fresh_usage` → `account_used_percent`; unknown is never eligible — `feature_limits_neither_move_the_account_nor_make_a_candidate`); the account list order and quota cell (`accountUsedPercent`, `quotaOrder`; feature windows labelled `{feature} · {primary|secondary} feature limit`); MCP `switchboard_usage` (each window carries `scope: account|feature`, `lowest_remaining_percent` covers account windows — `feature_windows_are_scoped_and_leave_the_account_minimum_alone`); CLI `usage` human output (account capacity, or *account usage unknown (only feature limits reported)*). The stored aggregate is unchanged for every other reader.
- **Not kept.** Credits (`has_credits`, `unlimited`, `balance`) — a purchase measure; plan type; unknown reached-types and unknown fields. No billing, purchase or reset operation.

## Typed, attributable limit evidence (SB-41, 2026-10-04)

- **Shape.** `limits::Limit { event_id, source, session?, observed_at, kind, limit_type?, resets_at?, until, confidence }`, one per account (the later `until` wins). `kind`: `quota` when the marker's `quotaLimits.rateLimitType` names a subscription window (`five_hour`, `seven_day`, `seven_day_*`; kept, allowlisted `[a-z0-9_]` ≤ 32, as `limit_type`), else `unknown` — a managed 429 or any other type can be rate, quota or spend (`the_kind_comes_from_the_markers_own_limit_type`). `confidence`: `attributed` (managed request, the proxy served it with that account) or `inferred` (transcript marker). `resets_at` is the provider's reported time, uncapped; `until` is that reset or the fallback hold, bounded to seven days from the marker's own time — stable across passes, so the file is written once per marker (`a_far_reset_is_written_once_and_reported_as_given`). `MonitorStatus.limited[]` adds `kind`, `limit_type`, `resets_at`, `confidence`, `scope: "unknown"`, `observed_at`, `event_id` beside `account_id`, `until`, `source`; MCP `switchboard_status` passes them through. `event_id` names the event, shared by every copy of the identity it was charged to.
- **Claude Code's config as a sign-in source (SB-57, 2026-10-05).** The background probe stamps `~/.claude.json` by a SHA-256 of its `oauthAccount` section (`Stamp::Account`), re-reading the file only when its device, inode, length or modification time moved; a capture refuses as changed only when that section or the credential changed between its two reads. Before, every rewrite of the file (7 of 60 seconds with ~20 sessions, measured 2026-10-05) counted as a changed sign-in: the next pass read the source again through `/usr/bin/security`, and a read that overlapped a rewrite answered `refused` (43 `refused` → `available` flips in a day's log) (`the_config_stamp_follows_the_account_section_only`, `the_native_config_stamp_ignores_unrelated_rewrites` — Unix only, see its comment — `a_config_rewritten_elsewhere_during_capture_is_still_the_same_sign_in`).
- **Reading transcripts (SB-49).** The monitor keeps, per recent transcript, its creation time, length, modification time, the offset past its last complete line and the lines that pass the API-error checks (at most 64). An unchanged file is not opened; a grown one is read from that offset (growth past 256 KiB reads the last 256 KiB, as a first read does); a shorter, older or re-created file is read afresh; a file no longer recent is dropped. Text after the last newline is checked but not counted as read, so a marker written without a newline is seen and read again with the next growth. The markers found equal a fresh read's (`an_unchanged_transcript_is_not_read_again_and_answers_the_same`, `a_grown_transcript_is_read_from_where_the_last_read_stopped`, `a_line_cut_by_the_read_is_finished_on_the_next_pass`, `a_rewritten_or_vanished_transcript_is_read_afresh_or_forgotten`, `a_first_read_and_a_large_growth_stay_within_the_tail_bound`).
- **Attribution.** A marker continuing a limit already held on the accounts its session was charged to (same reset; no reset while that hold lasts) stays there; otherwise a session opened before the switch with another account's reset goes to that account, and anything else — a session opened after the switch, or a new reset — is the current account's (`an_equal_reset_on_a_new_session_is_the_new_accounts_limit`, `a_bound_session_that_hits_a_new_limit_holds_the_current_account`). Whether a running session adopts a switch is not assumed (SB-06). A binding follows the session to the accounts it was last charged to and is saved when created (`a_new_binding_alone_is_saved`). Session id = transcript file stem when it is a plain id, else a 128-bit digest of its path — never a path.
- **Durability.** `<data>/limit-evidence.json` (0600, ≤ 512 KiB, ≤ 256 limits and bindings; a full table evicts the hold ending soonest, never refusing a new limit; removed accounts are pruned every pass — `a_full_table_makes_room_and_never_drops_a_new_limit`, `a_removed_account_loses_its_holds_and_bindings`): written only when evidence changes; on load, expired, over-long (> 7 days), future-stamped or foreign-source entries are dropped; bindings expire 24 h after their last marker (`evidence_survives_a_restart_and_a_hold_is_not_a_reset`, `evidence_read_from_disk_is_bounded`).
- **Scope.** Reported as `unknown`. Neither Claude Code markers nor managed 429s carry a field that states an organisation or group budget, and Codex identities carry no organisation id; no cross-seat propagation is guessed. SB-25 must treat unknown scope and inferred attribution as non-authorising.
- **Renderer.** `limitLabel`: *Limit resets {date}* only when `resets_at` equals `until`; otherwise *Retry hold until {date}*.

## Launch arguments and the background start (SB-30, 2026-10-04)

- `launch(args) -> Launch { smoke, background }` (src-tauri `main.rs`): `--smoke-test` and `--background` are the only arguments with a meaning; any other — Launch Services `-psn_…`, a flag a launcher adds — is ignored (`unknown_arguments_are_ignored_and_background_is_recognised`). The program name is never an argument.
- The main window is declared hidden (`tauri.conf.json` `visible: false`). An ordinary start calls `reveal` (Regular activation policy on macOS, unminimise, show, focus). `--background` leaves it hidden and, on macOS, sets the Prohibited policy on the built app before the event loop launches, so tao's launch-time activation request is refused: no focus, no Dock icon (measured with `lsappinfo front` polled every 50 ms during a background smoke: the front app never changed; with Accessory it did). `reveal` (Regular, show, focus) runs on the next launch (single instance) and on macOS `RunEvent::Reopen` with no visible window. `src-tauri/Info.plist` sets `LSAppNapIsDisabled`. A revealed window refreshes its metadata at once (`visibilitychange`).
- Smoke mode prints `SWITCHBOARD_WINDOW visible|hidden` before the readiness marker; `scripts/smoke_native.py --background` requires `hidden` with an unknown extra argument, the ordinary run requires `visible` (`scripts/test_smoke_native.py`); `build_macos.py` runs both and records `native_startup_background` in the receipt.
- Quit is unchanged: Quit, the last window, `SIGTERM`/`SIGINT` (caught before the owner starts, SB-44) run the one drain (LC-01).

## Structured sign-in result (SB-42, 2026-10-05)

- `Operation::FinishLogin` returns the saved `Account` plus `login_cleanup: "done" | "pending"` and (0.6.6, SB-62) `signed_in_again: bool` — true when the identity was already saved and the sign-in updated it in place (`file_sign_in`: a sign-in without a label updates every saved copy where it is and creates an account only for a new identity; every other copy of the identity gets the new sign-in); a cleanup failure after the account was saved is no longer an error (the CLI exits 0, the desktop shows the account as added with a note). `Runtime::finish_login` keeps the last `COMPLETED_LOGINS` (16) saved sign-ins in memory: a repeated Finish returns the same account without capturing again and retries that sign-in's pending cleanup; `LoginStatus` reads a saved sign-in as `complete`; an unknown id is still *Sign-in not found. Start again.* Saved sign-ins hold no sign-in slot (`failed_login_cleanup_releases_the_slot_after_the_account_is_saved`, `completed_sign_ins_are_bounded`).

## Uninstall (SB-29, 2026-10-05)

- `switchboard uninstall [--keep-data] [--yes]` → `switchboard_runtime::uninstall::uninstall(root, vault, keep_data, apply, places)`. Without `--yes` it returns the plan (`accounts`, `cli_link`, `orphan_keychain_item`, `data_entries`, `kept`, `applied: false`) and changes nothing. The store is opened exclusively, so it refuses while an owner runs.
- With `--yes`: `Vault::delete` for every account (shared and legacy items on macOS; DPAPI files on Windows), the orphaned 0.5.0 `ai.passioncode.fabric-switchboard.vault-key` / `v1` item through `/usr/bin/security` (absent is success), the `<home>/.local/bin/switchboard` symlink only when it points to a file named like the CLI, then — unless `--keep-data` — only the data-folder entries Switchboard writes (`accounts.json`, `instance.lock`, `control.json`, `proxy.json`, `renewal-state.json`, `usage-holds.json`, `limit-evidence.json`, `backup-id`, `homes`, `runtimes`, `logins`, `vault`, `.private-*` temporaries); the folder is removed only when nothing else is in it. Each failure is reported in `failures`; nothing stops at the first. Backups and their key are never touched (`without_apply_nothing_changes`, `apply_removes_owned_things_and_keeps_the_persons_files`, `keep_data_keeps_the_folder_and_a_foreign_link_is_left_alone`, `an_empty_folder_is_removed_and_a_running_owner_refuses`).

## Fallback chains (SB-71, 2026-10-06)

Design: [XA-01](packets/cross-agent-continuation.md). Code: `crates/switchboard-core/src/chains.rs`,
`agent_catalog::{can_continue, PRESETS}`, `Operation::{Chains, SetChain}`, CLI `switchboard chain`,
MCP `switchboard_chain_get` (read) and `switchboard_chain_set` (write).

- `Snapshot.chains: Vec<Chain>` (omitted when empty, so older readers are unaffected);
  `Chain {scope, executors, updated_at}`; `ChainScope` is `{kind: "machine"}`,
  `{kind: "project", pool}` (an existing project) or `{kind: "workflow", id}` (`wf_` + 16 hex);
  `Executor {agent, account_id?, key?}` — at most 8 per chain, one chain per scope, at most 256.
- Validation on every publish (`validate_chains`): an agent id is `[a-z0-9-]{1,40}`; an account or a
  key, not both; a key is an Observatory vault name `project/env/NAME`, never for Claude Code or
  Codex; `claude-code` / `codex` pinned only to an account of that provider; any other agent pinned
  only to an API-key account (never a subscription sign-in). The runtime also requires the agent to
  be in the catalog with `mcp.supported` and a `headless` form (`can_continue`).
- `Store::set_chain(scope, executors)`: an empty list clears the scope; the same agent and pin twice
  is refused; event `fallback_chain` `set` / `cleared`. `Store::chain_for(workflow, pool)`: the
  workflow's chain, else the project's, else the machine's, else none.
- Removing an account drops executors pinned to it; removing a project drops its chain; a chain left
  empty is dropped (`chains::prune`).
- `Operation::Chains {workflow?, pool?}` → `{chains, effective, presets}`; `Operation::SetChain
  {scope, executors, preset?}` → `{chain}` — a preset id (`subscriptions-first`) stands for the list.
- The automatic fallback (SB-73, below) walks the chain that applies; spending ceilings for paid keys
  are SB-72.

## Automatic fallback, phase 1 (SB-73, 2026-10-06)

Design: [XA-01](packets/cross-agent-continuation.md) (operator decision D-1: automatic). Code:
`crates/switchboard-runtime/src/fallback.rs` (`pass`, `pass_with`, `resolve`, `executor_limited`),
`continuation::{list_open, checkouts}`, called by the monitor after rotation.

- **Trigger:** a chain applies (SB-71: the workflow's, else its executor account's project pool,
  else the machine's — none set, nothing happens), and the executor of an open workflow without a
  waiting handoff has a recent limit (`LimitState::limited_ids`). When the engine names the
  executor's `accountRef`, only that account decides (one Switchboard does not hold decides
  nothing). When it names none, the executor is taken to be the ordinary CLI of its provider — but
  only while no session Switchboard launched for that provider is running
  (`launch::launched_session_running`: an isolated home of one of its accounts or a managed home
  that is not idle), since such a session could be the executor on a healthy account (2026-10-07).
- **Cadence:** at most one `project-observatory full workflow list --status open --json` a minute,
  only while a chain exists and some account is limited; a scan that finds nothing to hand over
  (or cannot read the engine) waits 5 minutes (LC-08); at most 4 workflows a pass; a workflow
  handed over or tried is left alone 15 minutes. Each engine read runs off the async workers
  (`spawn_blocking`) with a 15-second deadline (a person's `switchboard continue` waits a minute);
  at the deadline the engine's whole process group is killed (LC-02).
- **Hand-over:** the chain in order; each step resolves to a usable account (enabled, not the
  executor's, not limited, not needing a new sign-in, not another saved row of the executor's or
  a limited account's login (SB-62 rows share one limit); a pinned one only if usable; else the
  provider's least-used); the hand-over is `Operation::Continue` in `isolated` mode in the first of
  the workflow's checkouts that exists here — every refusal of `switchboard continue` applies, and
  the engine's silence (120 s) and rate rules decide. A refusal before the offer moves down the
  chain; a failed launch after an offer stops (the offer lapses).
- **Phase 1** runs Claude Code and Codex. Other agents are skipped (`agent_not_automatic_yet`)
  until their launch recipes and paid-key ceilings exist (SB-72, SB-73 phase 2).
- **Evidence:** log event `fallback` with `offered`, `launch_failed`, `every_step_refused`,
  `no_usable_executor`, `no_checkout` or `engine_unreadable`; store event `fallback`
  `offered` / `failed`. Never: stopping the old session, a `--force` handoff, a credential in the
  pack or the prompt.

## Launch in place (SB-75, 2026-10-06)

Asked for by Fabric Dashboards (an embedded console beside each service, operator decision
2026-10-06). Code: `launch::launch_here`, `launch_full` (`launch_with` is its Terminal form),
`projects::account_for_folder`, `Operation::{Launch, LaunchAccount}`, CLI `launch`.

- `Operation::Launch` gains `in_place: bool` and `args: Vec<String>` (both `#[serde(default)]`, so
  older callers — the desktop app — are unchanged). In place, the owner prepares the home, tools,
  environment and `launch.command` exactly as for Terminal and returns `{script, agent_tools}`
  instead of opening Terminal; the reservation lapses on its own (10 min) if the script never runs.
  `args` are appended after Switchboard's own arguments, each quoted by the script builder; at most
  16 of up to 1024 characters, no control characters (`EXTRA_ARGS_INVALID`), and only in place.
  Windows refuses in place for now (`IN_PLACE_UNSUPPORTED`).
- `Operation::LaunchAccount {provider, working_directory}` → `{id}`: the folder's project's selected
  account (`NO_PROJECT_SELECTION` without one — never another pool's), else the account of the
  folder's rule in force, else the default pool's selected account (`NO_SELECTION`). Metadata only,
  no credential read.
- The CLI refuses `--in-place` without a terminal on stdin and stdout, then `exec`s the script, so
  the agent replaces the `switchboard` process and its exit status is the command's.
- Tests: `an_in_place_launch_prepares_the_same_session_and_opens_no_terminal`,
  `the_account_for_a_folder_is_the_projects_then_the_rules_then_the_default_selection`,
  `an_in_place_launch_is_refused_where_it_cannot_run`. Not observed live with a real agent yet.

## Interface language (SB-76, 2026-10-07)

Code: `src/i18n.ts`, `src/locales/ru.ts`, `crates/switchboard-core/src/language.rs`,
`src-tauri/src/residency.rs` (`set_language`, `labels`, `restart_label_in`).

- The window's language is read once at start: `localStorage['switchboard.locale']` (`en` | `ru`;
  absent = the system's), else `navigator.languages[0]` (`ru`/`ru-*` → Russian, anything else →
  English). Changing it reloads the window.
- `t(source, params)` returns the Russian entry of the English `source` (or `source` itself);
  `{name}` placeholders are filled from `params`. `plural(n, {one, other})` takes Russian's three
  forms from the entry of `other`, separated by `|`. Comparisons with backend text use the English
  sentence; translation happens only where text is shown (L10N-04).
- IPC `set_language(locale: String) -> ()`: the window reports its language at start; the tray menu
  is rebuilt in Russian for `ru` and in English otherwise. Before the window reports, the tray
  follows the system's first preferred language (`language::system_prefers_russian`: macOS
  `CFLocaleCopyPreferredLanguages`, Windows `GetUserDefaultUILanguage`, else `LC_ALL` /
  `LC_MESSAGES` / `LANG`).
- Not localized, by design: the CLI, MCP answers, logs, the store's journal codes and analytics
  (counts and kinds only, unchanged — ANALYTICS.md).
- Gate: `node scripts/check-locale.mjs` (a missing entry, a changed placeholder, a plural without
  three forms fail); `scripts/test-ui-logic.mjs` covers language choice, plural rules and that every
  `event_valid` word has a journal label.
