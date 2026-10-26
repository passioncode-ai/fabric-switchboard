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
- `runtime_status() -> { proxy_address: String, platform: String, live_mode: String, owner_version: String, online: bool }`: `owner_version` is the answering owner's version (a client of another version reads the owner's here; null offline, where no owner answered), `online` whether an owner runs (PRD-12; `status_and_the_start_line_name_the_owners_version`)
- `launch_account(id,mode,workingDirectory) -> { message:String }`, mode `isolated` or `managed`; workingDirectory is an absolute existing project directory outside app data. Backend fixed provider binaries; no shell interpolation.
- `begin_login(provider,label,pool) -> { login_id:String, message:String }`; `finish_login(loginId) -> Account`; `cancel_login(loginId) -> ()`; isolated official CLI login and native credential capture; no global logout.
- `probe_usage(id) -> Usage` with fixed HTTPS allowlist; unknown is an error, never zero.
Frontend: Rust IPC is authoritative; pure mock fixtures only behind explicit browser demo marker. Errors announced, no credential query endpoints. Add credential input cleared after use. UI reports limitations of launched sessions versus external ones.

## Ownership
Core implementer: crates/switchboard-core only. UI implementer: src/, index.html, package.json, tsconfig.json, vite.config.ts only. Root: docs, proxy, src-tauri, scripts, integration. Every implementation task receives this contract and owns a separate worktree. Integration happens by cherry-pick; never overwrite another task's files.

## CLI/control and native platform extension (0.2)

`switchboard-runtime` owns shared login/launch and the running owner’s control server. Desktop and CLI must use the same default data-root function (macOS Application Support, Windows LOCALAPPDATA) and core’s private filesystem helpers. The secret-bearing control descriptor belongs only in app data; never in project Git files or stdout.

A separate loopback control server dispatches typed metadata/account operations, usage and native login/launch. Commands cannot query credentials. It authenticates before decoding a bounded body, denies browser Origins and wrong Host, and has no arbitrary URL/proxy primitive. CLI must not fall back to writing an already owned Store when a live control call is refused.

Version skew (2026-10-08, found by fabric-dashboards FD-25): an update installs a newer bundle — CLI included — while the earlier owner keeps running from a temporary copy until it restarts, so the CLI can send an operation the owner cannot decode. Since 0.6.12 the identity answer (`GET /v1/hello`) carries the owner's version in the `x-switchboard-version` header (a header, because an earlier client refuses unknown fields in the `{proof}` body). A 400 then reads `CONTROL_OWNER_OLDER` (restart Switchboard to finish its update) when the owner is older or sends no version, `CONTROL_OWNER_NEWER` when the CLI is older, the plain refusal when both match; 429 reads `CONTROL_BUSY`. Every refusal of the listener is logged as `control_refused` with its status and a code (`unsafe_request`, `unauthorized`, `busy`, `too_large`, `body_timeout`, `unknown_operation`), never the request (`a_malformed_request_is_named_by_which_side_is_older`, `the_owner_names_its_version_and_an_unknown_operation_reads_as_a_skew`). A request the owner took 5 s or longer to answer is logged as `control_slow` with the operation's tag and the milliseconds, never its arguments (SB-174, `an_operation_kind_is_its_tag_and_never_its_arguments`), so a client's stall can be told apart from a wait on the way to the owner. A credential read of a second or more is logged by the next background pass as `vault_slow` with the slowest read's milliseconds; since 0.6.81 such a read no longer holds the account metadata, so `snapshot()` and every read built on it (recovery, hand-overs, agents) answer while it runs (SB-185, `a_slow_credential_read_does_not_hold_the_metadata`).

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
- Launch attaches the tools when a CLI is found (this CLI, the one bundled beside the desktop executable, PATH, `~/.local/bin`): Claude gets `--mcp-config=<home>/switchboard-mcp.json` (one argument: the flag is variadic, SB-94), Codex gets `[mcp_servers.switchboard]` in its private `config.toml`; isolated sessions get `mcp --read-only`. The launch result carries `agent_tools: bool`. No CLI found is not a launch failure. Every launched session, managed or isolated, also gets the Fabric project board when the operator registered it for that provider in their own user scope — Claude Code's `~/.claude.json` `mcpServers.fabric`, Codex's `$CODEX_HOME`/`~/.codex` `config.toml` `[mcp_servers.fabric]` — beside Switchboard's server in the same file or section (SB-20). Only a stdio entry with an absolute command is carried, and only its `command` and `args`, never its `env`; none registered, a URL server or an unreadable file means no board, never a failed launch (`the_operators_fabric_board_reaches_a_launched_session_without_its_env`, `no_registered_board_or_a_doubtful_entry_means_no_board_never_a_failed_launch`).
- **The operator's setup in a launched home (SB-137, SB-130).** A Claude Code home (`CLAUDE_CONFIG_DIR`, managed or isolated) links `CLAUDE.md`, `skills`, `commands`, `agents`, `output-styles`, `plugins` and `hooks` from the operator's own folder (`CLAUDE_CONFIG_DIR`, else `~/.claude`; `carry::link_into`: a link it made before is renewed, a real file in the home is never replaced, links are made on macOS and Linux only). Its `settings.json` is the operator's `settings.json` without `env`, `apiKeyHelper`, `awsAuthRefresh`, `awsCredentialExport`, `otelHeadersHelper`, `forceLoginMethod`, `forceLoginOrgUUID` or any top-level key shaped like a secret (`key`, `token`, `secret`, `password`, `auth`, `credential`, `cookie`), then, isolated, the account's own `env`; managed writes it only when something is carried. Its `.claude.json` gets `hasCompletedOnboarding: true`, and for a continuation `projects[<working folder>].hasTrustDialogAccepted: true` (that folder only: the previous agent of the workflow worked there; operator rule of 2026-10-17, decided 2026-10-19); an ordinary launch keeps the trust question, and a `.claude.json` that is not a JSON object is left untouched. A Codex home links `AGENTS.md` only (its `config.toml` may hold secrets). Pi's own `PI_CODING_AGENT_DIR` links `AGENTS.md`, `skills`, `extensions`, `prompts`, `themes` from `PI_CODING_AGENT_DIR` or `~/.pi/agent` and gets their `settings.json` without secret-shaped keys; `auth.json` is never carried, so the launch's key still wins. Tests: `carry::tests::*`, `a_launched_home_carries_the_operators_setup_and_a_continuation_trusts_its_folder`.
- **Project keys for agents (SB-144).** `AgentKey` carries an optional `pool`; at most one key per service and pool, the pool a project's (`KEY_POOL_INVALID` otherwise; `default` is not a project pool). `Store::agent_key_in(service, folder)` returns the key of the project whose folder contains `folder` (the one `Snapshot::project_for` names when several hold it), else the machine's (`pool: null`); the rule is `agent_keys::key_in`, which the runtime's spending gate and chains view call too. Operations `AgentKeySet|AgentKeyStatus|AgentKeyModel|AgentKeyRemove` take `pool`; `AgentKeys { service }` lists `{keys:[{pool, project, saved_at, model}], projects:[{pool, name, own}]}` (never a value); `AgentKeyValue { service, folder }` resolves by folder. Every launch script's key command is `switchboard --data-dir <root> agents key --service openrouter --dir <launch folder>` (it runs before the script's `cd`), and Hermes, OpenCode and the OpenRouter launches take the default model from the same key. Deleting a project deletes its keys (metadata, then vault items); a backup restores a project key only when its project exists. CLI `agents openrouter set|status|model|remove --pool`, `agents openrouter list`, `agents key --service openrouter --dir`; MCP `switchboard_openrouter_status` (`pool`, plus `keys` and `projects`), `switchboard_openrouter_model` (`pool`); Tauri `openrouter_keys`, `openrouter_save|model|remove|status` with `pool`; the window's Tokens page (SCN-045, SCN-065). Snapshots with a pooled key are unreadable to 0.6.47 and older (`deny_unknown_fields`). Test: `an_agent_starting_in_a_project_folder_gets_that_projects_key_and_others_the_machines`.
- **Codex in project folders (SB-143).** Codex strips `model_provider`, `model_providers` and `openai_base_url` from a project's `.codex/config.toml`, so one user-level provider does it: with `<data>/codex-route` = `on`, the owner writes between marker lines into `<CODEX_HOME or ~/.codex>/config.toml` a top-level `model_provider = "switchboard-auto"` (first in the file) and a `[model_providers.switchboard-auto]` table (last): `base_url = http://<proxy>/codex/_auto/v1`, `wire_api = "responses"`, `requires_openai_auth = true` (the person's own ChatGPT or API-key sign-in stays Codex's), `supports_websockets = false`, `http_headers = {X-Switchboard-Capability = <auto capability>}`; the result is parsed before it is written (0600), and the person's text outside the markers is never changed. A `model_provider` the person chose is refused (`CODEX_ROUTE_FOREIGN`), an unparsable file too (`CODEX_ROUTE_UNREADABLE`). Written at owner start, removed at owner stop, so a Codex started while Switchboard is not running reaches OpenAI directly. The proxy's `/codex/_auto/v1/responses` requires that capability (a pool's own does not count), reads the last `<cwd>` of the request's environment context, and for a folder of a project with an enabled Codex account (`codex_route::folders`, deepest folder wins) relays it as that pool's managed request with the session token (its account, fail-over and journal; the person's Authorization and `chatgpt-account-id` are dropped); any other request goes on unchanged to `<chatgpt>/backend-api/codex/responses` (when it carries `chatgpt-account-id`) or `<openai>/v1/responses`, without the capability header, recorded nowhere. Threat accepted: the capability sits in the person's own Codex config beside their `auth.json`, so a local process of that user can name another project's folder; it can already read that sign-in. Operation `CodexRoute { enabled? }` → `{on, applied, config, conflict, projects}`; CLI `project codex [on|off]`, MCP `switchboard_codex_route`, Tauri `codex_route`, Projects page *Codex in project folders* (SCN-066). Log `codex_route {outcome|switch}`. Tests: proxy `codex_through_the_user_provider_uses_the_projects_account_only_in_its_folders`, `a_request_names_its_newest_folder_and_the_deepest_project_folder_wins`; `codex_route::tests::*`.
- **Groups (SB-146).** A `Project` is shown as a group: it holds repositories (folders) with accounts of its own. The same folder may be in several groups (`save_project` and `validate_projects` refuse only one group's folder strictly inside another group's: `FOLDER_NESTED`). `Snapshot::projects_for(path)` lists every group holding a path in creation order; `Snapshot::project_for(path)` is the group a session there works in: the first with an enabled account, else the first. Bindings (`bindings::desired`) and the Codex route (`codex_route::folders`) write a shared folder once, for the first group with an account of that provider; `launch::project_allows` accepts any of the folder's groups. `MoveRoot { folder, pool, from?, also? }`: `also` adds the repository with its worktrees and clones to one more group and leaves none; `from` names the group it leaves when it is in several (`MOVE_ROOT_NOT_IN_GROUP` when that group does not hold it); a repository the target already holds just leaves `from`. `RepositoryMap` rows carry `groups[{pool, name}]`; `project_roots` roots carry `remote`, so the window lists a group's unique projects (one per `origin`, never a worktree, clone or subfolder) when its name is hovered or focused.
- **Project names (SB-155).** `roots::Root::name` is the name a project shows: the main checkout's folder name (a worktree root is named by the folder of its main checkout, the parent of the common `.git`; a bare repository by its folder without `.git`), else the folder's own name; never the remote's last segment (a renamed remote such as `project-observatory-dashboard` for the folder `project-observatory-open-source` keeps the folder's name). Statistics (`ProjectIndex`) and the window's group cards use it; two projects with one name read as `name (owner)`, the owner being the second-to-last segment of the project key. A subfolder of a listed folder belongs to that folder's project in the window too. A bindings sync that changed nothing writes no `claude_binding` event: a folder bound, removed or skipped is a change, and so is the set of folders holding the person's own `ANTHROPIC_*` values, compared with the previous sync's set rather than counted each time, and so is the set of folders skipped (PRD-13; `a_sync_that_changed_nothing_is_not_logged_again`).
- **Attribution kept with the project (SB-156).** The owner hands the proxy's counts to the statistics through `Runtime::absorb_statistics`: each count's folder is replaced by its project's key (`ProjectIndex::project_of`: the repository identity, or the folder outside Git) as the groups stand at that moment, and the project's name is kept in `statistics.json` (`names`, pruned with the rows). The index is rebuilt only when a group's folders change (a digest of pools and folders). A count therefore stays with its project after the checkout it came from is removed; 0.6.53 to 0.6.58 rows that still hold a folder resolve through the current groups, and a folder no group holds now (taken out of its group) through Git to its repository (`ProjectIndex::with_strays`, SB-173); only a folder outside Git or gone stays a key of its own. The report names a project by the current groups first, then the kept name, then the key's last segment. Attribution needs nothing but the local groups and Git: neither Fabric nor Observatory takes part. The window's *Statistics* adds *API keys*: each saved OpenRouter key (the machine's and each group's, SB-144) with its model and OpenRouter's own spend today, in all and its limit (`openrouter_status` per pool), which is where agents on OpenRouter (Hermes, OpenCode, Pi) are counted; Kimi Code's usage stays on its account card.
- **Organizations (SB-158).** The window shows them on their own page in the sidebar (*Organizations*: a card per organization with its domains and the accounts it holds, why each) and, only while one exists, a filter above the accounts; nothing about organizations shows on *Accounts* before (operator 2026-10-22). The rest: `Snapshot.organizations[{id, name, domains[], provider_organizations[], created_at}]` and `Snapshot.memberships{account id → organization id | "personal"}`, both in `accounts.json` (omitted while empty) and in backups. `Snapshot::attribution(account)` answers `{organization, by}` in this order: `person` (the membership; `personal` keeps it personal), `provider` (the account's provider organization, `claude:<organization uuid>` or `codex:<ChatGPT workspace id>`, is listed on an organization), `domain` (the domain of its identity's e-mail, else of a label that is an address, is one of an organization's), else none (personal, `by: null`). An address on an organization's domain belongs to it whatever the plan: the organization holds that mailbox, so the sign-in is its own (design §10). `Store::assign_organization(account, choice)` records the person's word and, for an organization, moves the account's provider organization onto it (off every other), so the team's other seats follow; `personal` takes it off every organization; none forgets the word. Names are unique (case-insensitive), a domain belongs to one organization, at most 32 organizations of 16 domains. Removing an organization drops the memberships naming it; removing an account drops its membership. Refusals: `ORGANIZATION_NOT_FOUND`, `ORGANIZATION_NAME_TAKEN`, `DOMAIN_TAKEN`, `DOMAIN_INVALID`, `ORGANIZATION_NAME_INVALID`. Journal `organization` with `added`, `updated`, `removed`, `assigned`. A found folder that cannot join its group logs `project_suggestion {add: refused, reason}` with `reason` one of `nested`, `gone`, `group_removed`, `too_many_folders`, `account_in_cli`, `other` (`add_refusal`, test `a_refused_found_folder_logs_why`). Operations `Organizations` (read fast path; `{schema: 1, organizations[{id, name, domains, provider_organizations (count), accounts, created_at}], accounts[{id, label, provider, pool, organization, organization_name, by}]}`), `OrganizationAdd {name, domains}`, `OrganizationEdit {id, name?, domains?}`, `OrganizationRemove {id}`, `OrganizationAssign {account, choice?}`; Tauri `organization_add`, `organization_edit`, `organization_remove`, `organization_assign`; CLI `org list|add|edit|remove|assign`; MCP `switchboard_organizations` (read), `switchboard_organization_set` (write, hidden under `--read-only`), and `organization {id, name, by}` on each `switchboard_accounts` row. The window computes the same order from the snapshot (`ui-logic` `attribution`, same cases as `organizations::tests`). Nothing reads the network, and no organization or membership leaves the machine (analytics unchanged). Tests: `organizations::tests::*`, storage `assigning_one_seat_brings_its_team_and_removals_clean_up_assignments`, MCP `accounts_rotation_activity_and_queues_are_reachable_over_mcp`, ui-logic SB-158 case, WebKit demo. Design: [organizations-2026-10-21](research/organizations-2026-10-21.md).
- **An account used on another machine (SB-175, SB-168 D-2).** `Snapshot.elsewhere`: account id → the Unix time it was marked, written as schema version 8 while one exists (an older reader says "Unsupported account metadata version"). `Store::mark_elsewhere(id, on, now)` disables the account and clears every route to it when marking, enables it when clearing; enabling the account (`update_fields`) clears the mark; removing it removes the mark. Every rule that reads `enabled` (selection, rotation, renewal of inactive accounts, SB-98 failover, project bindings) therefore leaves it alone; `select` refuses it with `ELSEWHERE` ("This account is in use on another machine. Use one of this machine's accounts, or clear the mark if it moved here."). `Operation::Update` takes `elsewhere: bool` (CLI `accounts update --elsewhere`, MCP `switchboard_account_update.elsewhere`, Tauri `mark_elsewhere`). Encrypted backups carry the marks by old account id. Test `an_account_used_on_another_machine_is_never_chosen_here`.
- **Settings that move to another machine (SB-176, SB-168 D-4).** `Operation::SettingsExport` (read fast path) answers a `switchboard-settings/1` bundle (`settings_bundle::export`): `{format, exported_at, groups[{name, pool, organization (name), repositories (DEC-0089 identities of the group's roots, `roots::tree`), plain_folders (roots without an origin), sources}], organizations[{name, domains}], agents {default, disabled}, policies (without `last_switched_at`)}`; no account id, credential, rule, chain or capability. `Operation::SettingsImport {bundle, under, apply}` parses it (at most 1 MiB, `format` checked, unknown fields refused), finds checkouts by identity under `under` (`roots::disk_map`, default `roots::default_under`) and plain folders at their path, and answers `{preview: {groups[{name, pool, exists, organization, folders, missing}], organizations, agents, policies}, applied}`; with `apply` it adds the missing organizations by name, makes missing groups by name with no accounts and their organization, adds found folders to an existing group of the same pool keeping its accounts and organization, sets the agents' settings and the policies (`claude_cli` ones disabled), and names each refused part in `result.refused`. CLI `settings export|import` only; no MCP tool (the parity table says why). Test `settings_bundle::tests::a_bundle_round_trips_and_names_no_account`.
- **A group's organization (SB-109).** `Project.organization: id | null` (omitted when none; null is personal) in `accounts.json`, written as schema version 6 while a group has one (a 0.6.64 or older reader says "Unsupported account metadata version" instead of "corrupt"); in backups beside the projects as `project_organizations` (pool → id), with `organizations` and `memberships` (old account id → choice) beside them, so a 0.6.29 or older reader still opens the backup; before 0.6.65 a backup carried none of the three. Restore puts back organizations whose id, name and domains are free (`Restored.organizations`); a backup organization whose name this install already has maps onto that one, so its groups and the person's word follow it; the person's word returns only where this install has none. `Snapshot::context_admits(pool, account)`: true while no organization exists or no group reserves `pool` (`default`); else the account's attribution equals the group's organization (personal included). Enforced at placement (`Store::save_project_in`: an account moving into a group, or every account of a group whose organization changes, must fit, else `SEAT_CHOSEN`), at binding (`Store::set_project_organization` refuses while the group holds an account of another context: `SEATS_ELSEWHERE`), at selection and on the proxy's route (`Store::select`, `Store::route`: `SEAT_OUTSIDE`, for an account whose attribution moved after it was placed), in rotation and failover (such an account is never a candidate) and at launch (`launch::project_allows`). A folder in groups of different contexts: personal groups first, then creation order, in `Snapshot::project_for`, the Claude folder bindings (`bindings::desired`) and the Codex folders (`codex_route::folders`), so the more private wins (RM-28 CX-26); two organizations are not ranked. A group whose seats all left its organization keeps its folders bound: the proxy refuses the seat (closed, 409 to the session, no account text) rather than the folder falling back to the ordinary login (open); launch refuses there too. Removing an organization makes its groups personal. Operations `SaveProject` and `ProjectEdit` take `organization` (an id, a name in any case, or `personal`; left out, the group keeps its own); Tauri `save_project {organization}`; CLI `project edit --organization`; MCP `switchboard_project_edit {organization}`; `project list`, `switchboard_projects` and the window's projects carry `organization` and `organization_name`. Statistics: `organizations[{organization, name}]` (an id, `personal`, or `unknown` for a removed account) by each account's current attribution, only while an organization exists. Window: *Organization* in the group dialog (only with organizations; another context's accounts disabled with why), the organization's badge on the group card, *By organization* above *By group*. Tests: storage `a_groups_organization_keeps_its_folders_on_its_own_seats`, rotation `a_seat_outside_its_groups_organization_is_never_guessed`, codex_route `folders` SB-109 case, `a_folder_in_groups_of_two_contexts_runs_in_the_personal_one`, backup `a_reinstall_gets_organizations_their_accounts_and_groups_back`, launch `a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts`, statistics `requests_are_counted_by_whose_seat_they_ran_on_once_an_organization_exists`, MCP `a_project_is_listed_and_changed_by_delta_over_mcp`, ui-logic SB-109 case, WebKit demo (SCN-070).
- **PassionCode.ai organizations (SB-159).** `vesta.rs`: `connect` binds `127.0.0.1:0`, opens `<base>/auth/app?client_id=switchboard&redirect_uri=http://127.0.0.1:<port>/callback&code_challenge=<S256>&code_challenge_method=S256&state=<random>&scope=me.organizations` in the system browser (`/usr/bin/open`, `rundll32 url.dll,FileProtocolHandler`, `xdg-open`; spawned and reaped, never waited on), accepts loopback connections only, ignores requests that are not `/callback` with this `state`, refuses an answer whose `iss` is missing or is not the base (RFC 9207), maps `error=access_denied` to `VESTA_DENIED`, waits at most 5 minutes (`VESTA_TIMEOUT`) or until `VestaCancel` (`VESTA_CANCELLED`), then exchanges the code (`POST /v1/app-tokens/token`, form, `client_id=switchboard`, the verifier) and reads `GET /v1/me/organizations` (`{organizations: [{orgId, name, domains, role, current}]}`) with the access token. The refresh token is the only thing kept: in the OS vault (`Store::put_secret`, a UUID item), rotated on every read and saved before the read uses the new access token; `<data>/vesta.json` (0600) holds the item, the base, `connected_at`, `read_at`, `checked_at`, `organizations` and `state` (`ok`, `sign_in_again` for a refused grant or token, `provider_sign_in` for 401 `idp-sign-in-required`, `unreachable`), never a token. `Store::apply_vesta` (names compared trimmed and lowercased, as `validate` does): a linked organization (`Organization.vesta_id`, the Vesta `orgId`) takes Vesta's name unless another organization here holds it; domains add up (one typed here stays with its accounts, Vesta's join it once each, one another organization holds is left out, at most 16), so a domain Vesta drops stays until the person edits it; an unlinked one of the same name or with a shared domain is linked, keeping its accounts and assignments; the rest are added, under Vesta's name or, when that is taken, its first 68 characters and the start of its id, refused rather than published if it still cannot fit; one linked here that Vesta no longer lists keeps its accounts and loses the link. A read whose vault read or merge fails is recorded as tried (`unreachable`), so the monitor neither rotates the token nor asks a locked keychain every pass. One read, connect or disconnect at a time in a process (`vesta::TURN`): a refresh token is single use and Vesta revokes the grant when a rotated one is presented again; a new token the vault would not take turns the state to `sign_in_again`. `accounts.json` is schema version 7 while an organization is linked; backups carry organizations without the link (made again by connecting). The owner's monitor reads again when a connection is `ok` or `unreachable` and was not tried for 6 hours (`vesta::due`), never once the drain began (`grants_allowed`, checked again with `due` after the turn is taken: `sync_if_due`, so reads spawned on consecutive passes never queue a second refresh), in a spawned task outside the pass and the owner transaction, and logs `vesta_read {outcome}`; `sign_in_again` and `provider_sign_in` wait for the person. Operations (all on the fast path, none under the owner mutation lock): `VestaStatus`, `VestaConnect` (logs `vesta_connect {outcome: connected, connected_unread, denied, timeout, cancelled, failed}`; one at a time, `VESTA_BUSY`), `VestaCancel`, `VestaSync`, `VestaDisconnect` (deletes the vault item, an absent one counting as deleted and any other failure, a locked keychain, keeping the connection rather than orphaning its token, then every link, then `vesta.json`; organizations stay as local ones and the schema drops back below 7). The control client waits 420 s for `VestaConnect` (the browser's 300 s, a read holding the turn, the exchange and the read; 30 s for the rest). `switchboard uninstall` deletes `vesta.json` and the token's vault item. `Organizations` carries `passioncode` (the status) and `passioncode: bool` per organization. Tauri `passioncode {action}`; CLI `org connect|read|passioncode|disconnect`; the window's PassionCode.ai panel on *Organizations* and a *PassionCode.ai* mark on a linked organization (SCN-071). Nothing is sent but the person's own token to the person's own server; no account data leaves the machine. Tests: storage `organizations_from_passioncode_are_linked_kept_current_and_released`, runtime `vesta::tests` (a synthetic Vesta on loopback with its reuse detection: PKCE checked by the server, a stray request ignored, rotation, four reads at once, `idp-sign-in-required`, `app-token-invalid`, a refused refresh, denied, refused code, cancelled, an answer without `iss`), ui-logic SB-159 case, WebKit demo.
- **Statistics (SB-145, SB-147).** The proxy counts each relayed request under `day\taccount\tpool\tclient\tfolder` (`switchboard_proxy::stats`: UTC day, account id, pool, client family from `user-agent` such as `claude-code`, `codex`, `opencode`, `hermes`, else `other`, and the group folder the request came from, empty when unknown; a 0.6.52 key without the folder still reads): `requests`, `ok`, `limited`, `failed`, and `input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens` as the provider reported them in its own response. The folder (SB-147) is the folder whose own capability the request presented, else the deepest group folder holding the folder the request names (`stats::folder_hint`: the newest Codex `<cwd>`, or Claude Code's `Working directory:` line in the body's first MiB; a byte scan, nothing of the body kept), else that named folder itself (SB-157: a session outside every group, or an agent Switchboard launched on a key), which the statistics resolve through Git to the repository whose checkout holds it (`ProjectIndex::with_strays`). The owner passes every group folder with its pool (`ProxyHandle::set_stat_folders`) wherever it sets the folders' capabilities. A 429 that moved (SB-98) counts once on the refusing account; a network failure counts as failed. `ProxyHandle::take_statistics()` hands the counts over and clears them. The owner adds them to `statistics.json` (0600, `{version: 1, tallies}`, 90 days, at most 50 000 rows) every pass, writing at most every 15 min and at stop; an unreadable file starts empty. `Statistics { days? }` (1 to 90, default 30; read fast path, offline reads the file) answers `{days, from, to, since, total, groups[{pool, name}], projects[{project, name, groups[{pool, name}]}], accounts[{account_id, label, provider, kind}], agents[{agent}], routes[group, project, project_name, account and agent], by_day[{day, groups[{pool, requests, tokens}]}]}`, each row with the counts, busiest first. A project (`statistics::ProjectIndex`, built from `roots::tree` per group) is a repository with all its checkouts, keyed by its `origin` identity, or a folder outside Git keyed by its path (DEC-0089 AT-6, AT-7, AT-15); `project: ""` holds requests that named no folder. Not counted: an off-project Codex request passed through on the person's own sign-in (SB-143), and anything that does not go through the proxy. No prompt, answer or header value is kept; folder paths stay in this file on this computer.
- Only `Owner::native` reads the ordinary CLIs' sign-in; `Owner::start` (tests, smoke) reports current accounts as unavailable.
- `switchboard mcp [--read-only]`: stdio JSON-RPC 2.0 per the tool table in PLAN-0.4. Tool failures are results with `isError: true`; protocol faults are JSON-RPC errors (-32700 parse/oversize > 1 MiB, -32600 invalid, -32601 method, -32602 unknown tool or bad arguments). Calls wait out a held store lock for up to 8 s, asking the owner first (`STORE_WAIT`, SB-181).
- Desktop: `agent_setup() -> { cli_path, bundled_cli, linked_cli, can_link, commands }`; `link_cli()` creates `~/.local/bin/switchboard` → bundled CLI and never replaces a file or a foreign link. The macOS build embeds the signed CLI at `Contents/MacOS/switchboard`, byte-identical to the archived CLI.

## Other agents (0.6)

Operator request 2026-10-05: support the popular coding agents (OpenRouter's ranking), not only Claude Code and Codex. Code: [`catalog/agents.json`](../catalog/agents.json) (30 agents, sourced in [research](research/agents-2026-10-05.md)), `crates/switchboard-runtime/src/agent_catalog.rs`, `launch::launch_agent`, proxy `agent_token`. User page: [AGENT-SUPPORT](AGENT-SUPPORT.md), generated by `scripts/agents_doc.py` (the gate fails when stale). SB-154 (DEC-0093): the contract's catalogue `catalogue/coding-agents.json` is pinned in [`catalog/coding-agents.contract.json`](../catalog/coding-agents.contract.json) (refreshed by `scripts/sync_agent_catalogue.py`); `agent_catalog::tests::the_catalog_agrees_with_the_contracts_catalogue` keeps both naming the same 30 agents with the same name, maker, URL, binary and MCP support, Switchboard's richer fields (launch profiles, Anthropic and OpenAI routes, MCP snippets, OpenRouter recommendations) staying its own evidence; `agent_catalog::contract_id` maps `cursor-cli` to `cursor-agent` and `kiro-cli` to `kiro`, and `switchboard_agents` rows carry it as `contract_id`.

- **Agent capability.** The proxy accepts a second capability, `agent_token(session token)` = SHA-256 of `switchboard-agents:<token>` in hex: derived, so it survives a restart with the session token and is never stored; knowing it does not reveal the session token. It is accepted as `Authorization: Bearer` or `x-api-key`; the session token only as a bearer. With the agent capability the proxy serves **API-key accounts only**: a subscription sign-in selected in the pool answers 403 `AGENT_SUBSCRIPTION_REFUSED` before any provider call (`agents_reach_api_key_accounts_and_never_a_subscription_sign_in`).
- **Attribution (fabric-workspace roadmap RM-16, OpenRouter app attribution, 2026-10-05).** Traffic Switchboard forwards for someone else's agent keeps that agent's attribution: the proxy relays only the `REQUEST_HEADERS` allowlist, which has no `HTTP-Referer` or `X-OpenRouter-Title`, and adds neither. Switchboard makes no model call of its own; if it ever does, it attributes as `https://passioncode.ai/switchboard/`.
- **Chat Completions.** `POST /codex/<pool>/v1/chat/completions` relays to OpenAI `/v1/chat/completions` for an API-key account; a ChatGPT sign-in answers 404 (`chat_completions_reach_openai_with_an_api_key_only`).
- Operations `AgentCatalog`, `AgentConnect { agent, pool }` (MCP registration, endpoints with the right path suffix, `key_command` — never a key value: `connect_names_the_key_command_and_never_a_key`), `AgentKey`, `LaunchAgent { agent, pool, working_directory }`: a `launch` agent gets its base and key variables; a `proxy` agent with a binary starts with the provider it was given by `connect`; both need the pool's selected Claude account to be an API key, and `project_allows` holds (`a_third_party_agent_never_starts_on_a_subscription_account`). CLI `switchboard agents list|connect|key|launch`; desktop *Agents → Installed → Set up* (since 0.6.31; before, *Agents → Other agents*).

## Workflow continuation (SB-52, 2026-10-05)

N-018 walking skeleton ([packet](packets/n-018-continuation.md)): when the executor of an Observatory workflow runs out of its limit, another account continues it — of the same provider, or (0.6.7, SB-70, N-018 step 2) of the other provider: the handoff then names the receiving harness (`continuation::provider_name`: `claude-code`, `codex`), `Plan.from_provider` and `Continuation.previous_provider` carry the previous executor's provider, and the first prompt says the work comes from another agent whose transcript pointer does not apply and tells the session to accept naming no other provider (the engine refuses an acceptance naming another provider than the offer). An executor of an agent Switchboard does not hold (Hermes, Kimi Code) can be taken over the same way. Code: `crates/switchboard-runtime/src/continuation.rs`, `launch::preflight`, `launch::launch_continuation`, `Operation::Continue`; CLI `switchboard continue`. Engine contract: memory/0.1, `project-observatory full workflow show --json` and `handoff` (engine 0.16.0).

- **Order.** Read (`full workflow show <wf> --json`) → plan → `launch::preflight` → resolve the Observatory MCP server → offer (`full workflow handoff <wf> --to-provider <executor's provider> --to-account <account handle> --reason limit`, never `--force`) → launch. **In place** (`Operation::Continue { in_place: true }`, `switchboard continue --in-place`, SB-108): the launch writes the same session script and returns it as `script` instead of opening Terminal (`launch::continuation_here`; Kimi and Hermes through their `start` callback), and the CLI replaces itself with it; without a terminal on stdin and stdout the CLI refuses before the owner is asked (`continuing_in_place_without_a_terminal_is_refused_before_anything_is_offered`, `a_continuation_in_place_returns_its_script_and_opens_no_terminal`). The automatic fallback still opens Terminal; handing its sessions to Fabric's console is the open half of SB-108. The handle is `continuation::account_ref(id)`: a saved account's id is a UUID, which the engine refuses anywhere in an identifier as credential-shaped (its `credential_shape`), so a UUID goes as `sb-` and the first 12 hexadecimal digits of its SHA-256; another reference (`hermes`) goes as it is. An executor's `accountRef` names an account by its handle or, written before 0.6.22, by its id (`names_account`). Found by the live Kimi rehearsal 2026-10-12: Project Observatory refused `--to-account <uuid>` ("looks like a credential (uuid)"), so every hand-over to a saved Claude, Codex or Kimi account was refused in 0.6.20–0.6.21; the engine accepted the handle (`handoff:a6f7171bfcbcbd75`). Tests `an_account_reaches_the_engine_as_a_handle_never_as_its_uuid`, `an_executor_named_by_its_handle_is_found_and_judged`; 2000 random handles pass the engine's own check. Every refusal before the offer leaves the workflow as it was; there is no automatic retry.
- **Plan refuses** a workflow id not shaped `wf_` + 16 hex (before the engine is called), a closed workflow, one without a checkpoint, a waiting handoff, any declared key whose state is not `vault` (named `project/env/name (state)`), a disabled account, an executor provider Switchboard cannot map (`claude`, `claude-code`, `anthropic` → Claude; `codex`, `openai` → Codex) or of another provider than the account, the account that already executes it, and a folder outside every `git` artifact checkout of the checkpoint (`every_refusal_is_decided_before_an_offer`). Without git artifacts the named folder is used. A checkout path component the engine redacted (`[redacted]`, a UUID folder name) matches exactly one component (`a_redacted_folder_name_matches_one_folder_and_nothing_more`).
- **Preflight** is what `launch` refuses before touching a home — folder, mode, account, enabled, `project_allows` — plus the managed route selection or, for an isolated session, a credential that has not expired (`preflight_refuses_what_a_launch_would_refuse_without_writing`).
- **Offer.** The engine's answer `handoff <handoff:16 hex> offered until <time>` is parsed; anything else is unreadable. Its refusal (`HandoffRefused` while the executor wrote less than 120 s ago, a rate limit, a closed workflow) is reported as its own first stderr line, control characters removed, at most 300 characters. An offer whose pack says a key left the vault launches nothing. After the offer, a failed launch names the handoff and its deadline: the offer lapses on its own.
- **Launch.** As `launch`, plus `OBSERVATORY_WORKFLOW_ID` and `OBSERVATORY_HANDOFF_ID` in the script, the Observatory server in the session's own MCP config beside Switchboard's (Claude's `switchboard-mcp.json`, Codex's `[mcp_servers.observatory]`): the interpreter from the engine launcher's `#!` line, `<full-path>/mcp/server.py`, and `--home` when Switchboard's environment names `OBSERVATORY_FULL_HOME` or `OBSERVATORY_HOME`. The first prompt names the workflow and handoff ids and the rules: accept with `observatory_handoff_accept` and its own `sessionId`, continue from the checkpoint in that answer, obey its constraints, checkpoint after every step (`a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt`).
- **Never.** Switchboard does not read or pass the old session's transcript, a lease token or a credential; it does not accept on the session's behalf. Engine calls are bounded (60 s, 1 MiB per stream, killed and reaped).
- **Platform.** macOS (and other Unix): the engine launcher's interpreter is read from its `#!` line. Windows answers `Continuing a workflow runs on macOS for now.`
- **Contract against the installed engine:** `real_engine_contract` (ignored by default; runs with `OBSERVATORY_HOME=<scratch>` and `SWITCHBOARD_OBSERVATORY_CONTRACT=<wf>:<checkout>`) — read, plan, MCP server, a real offer, and the second plan refused with the handoff waiting. Run 2026-10-05 on engine 0.16.0 in a scratch workspace: offered `handoff:34f6193d9e640027`. The engine's silence refusal, measured the same day: `workflow: HandoffRefused: wf_…: the executor wrote a checkpoint 0 s ago; without its lease token a handoff waits until it has been silent for 120 s (retry after 120 s)`.

## Projects (0.6)

Operator request 2026-10-05: a project is one or more folders (related repositories) and may reserve accounts of its own, so other projects' agents never switch to them. Code: `crates/switchboard-core/src/projects.rs` (`Project`, `Store::save_project`, `remove_project`, `Snapshot::project_for`, `project_of_pool`), `crates/switchboard-runtime/src/launch.rs` (`project_allows`).

- `Project { pool, name, folders, created_at }`; `Snapshot.projects`, omitted when empty. The project's accounts are the accounts in its `pool`; an account belongs to one project at a time. `accounts.json` is `schema_version` 4 while a project or a `project` journal entry exists (0.5 readers refuse it rather than ignore a reservation); readers accept 1–4.
- `save_project(pool?, name, folders, accounts)`: without `pool` a new project gets one from its name (`pool_from_name`, a suffix `-2`… when taken, never `default`); `accounts` is the whole set — listed accounts move into the pool, the pool's others move to `default`; a moved account's old managed route is cleared; a native (`claude_cli`) policy on the pool is switched off; canonical folders, at most 1024 across all projects, and no folder of one project inside or around another's (`A folder sits inside a folder of another group. Add the repository itself to that group instead.`); the same identity already in the target pool is refused. Journal `project`: `created`, `updated`, `removed`. `remove_project(pool)` keeps the accounts in the pool, no longer reserved.
- **Reservation, enforced:** a launch (isolated or managed) of a project account outside the project's folders → `This account belongs to a group. Launch it from one of the group's folders.`; inside a project's folders, an account of another pool for a provider the project has an enabled account of → `This folder belongs to a group. Launch one of the group's accounts.` (`a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts`). A provider with no enabled account in the project keeps its existing launch fallback. Native activation — the Switch button, a `claude_cli` rule, native rotation — of a project account and a native policy on a project pool are refused with `PROJECT_NOT_NATIVE`, because the ordinary Claude Code serves every folder (`a_project_account_never_becomes_the_ordinary_claude_code`, `a_project_pool_never_drives_the_ordinary_claude_code`). An account the ordinary Claude Code or Codex is signed in to cannot join a project, and *Capture current account* into a project's pool is refused the same way; *Import from Claude Swap* into a project's pool is refused with `PROJECT_NOT_NATIVE`, since Claude Swap switches the ordinary Claude Code (`PROJECT_ACCOUNT_IN_CLI`, `the_account_the_cli_uses_cannot_join_a_project`). Managed rotation and `switchboard_switch` stay inside the pool, as for any pool.
- Operations `SaveProject { pool?, name, folders, account_ids }`, `RemoveProject { pool }`; `ResolveProject` adds `project` (`{pool, name, folders, created_at, accounts}`, no credential) for the folder. Tauri `save_project`, `remove_project`; CLI `project save|delete`, `project list` → `{projects, rules}`; MCP `switchboard_project_context` returns the project. **SB-134:** `ProjectEdit { pool, name?, add_accounts, remove_accounts, add_folders, remove_folders }` builds the whole `SaveProject` from the current project (its folders as `base_folders`, so only the folders named here count as the person's removals) and runs it under the same admission; an account or folder named for removal that the project does not hold is refused (`PROJECT_EDIT_NO_ACCOUNT`, `PROJECT_EDIT_NO_FOLDER`), one named for adding that it holds is no change. CLI `project edit`; MCP `switchboard_projects` (read: `{projects, rules}`) and `switchboard_project_edit` (write, hidden under `--read-only`). **SB-134 part 2:** MCP `switchboard_events`, `switchboard_account_update`, `switchboard_rotation`, `switchboard_rotation_set` (merged with the saved policy, like the CLI), `switchboard_backups`, `switchboard_backup_now`, `switchboard_recovery`, `switchboard_handovers`, `switchboard_agent_set` and `switchboard_login` (begin, status, finish, cancel: the person signs in, the tool never takes a secret). Every CLI command has an MCP tool or a written reason in `mcp_parity::PARITY`, and `every_cli_command_has_an_mcp_tool_or_a_reason` fails when a new command has neither. **Capability rotation (SB-139):** `RotatePoolCapability { pool }` (CLI `project rotate-capability --pool`) stores a random salt for the pool in `<data>/pool-salts.json` (0600, removed by uninstall); the folder capability becomes `pool_token_salted(session, pool, salt)`, the proxy refuses the old one at once, and the pool's folders get the new one written in the same operation. Other pools and the session token are unchanged. Log `capability_rotated {folders}`. Tests: proxy `pool_tokens_*` (rotation part), `a_rotated_pool_gets_its_new_capability_written_over_the_old_one`. Tests: `a_project_changes_by_delta_and_keeps_what_is_not_named`, `a_project_is_listed_and_changed_by_delta_over_mcp`, `project_commands_work_offline_from_the_cli`. **Folder capabilities (SB-133):** each bound folder (and worktree) gets its own capability, `folder_token(pool_token_salted(session, pool, salt), folder)`, written as the `X-Switchboard-Capability` header; the proxy accepts, per pool, exactly the capabilities of the folders that are part of its project and exist now (`bindings::folder_tokens(desired)`, set at start, after every bindings pass and after a rotation). A folder moved or renamed away from its project, or taken out of it, keeps a value the proxy refuses with `FOLDER_NOT_IN_PROJECT` (401), so it can no longer spend the project's accounts. The pool's own capability, which 0.6.39 and earlier wrote into every folder, is still accepted until `legacy_until` in `<data>/capability-migration.json` (0600, recorded at the first start of 0.6.40 as that time plus 7 days; removed by uninstall), so sessions started before keep working until they restart. Tests: proxy `a_folder_capability_works_only_while_its_folder_is_in_the_project`, owner `a_folder_moved_out_of_its_project_cannot_spend_the_projects_accounts`, bindings `a_project_folder_gets_its_pool_route_and_keeps_the_persons_settings`. **Roots and derived folders (SB-135, first part):** `roots::tree(folders)` reads Git for a project's folders: the folders of one repository go under its main checkout (else the first listed) as `worktree`, and a repository with the same `origin` as a main checkout goes under it as `clone` with its own worktrees; a folder that is no checkout is a root of its own. Each root carries its branch, each derived folder its kind and branch. The project still stores its flat folder list. Shown by Tauri `project_roots` (the Projects page: roots, `+n worktrees and clones`, expandable; the project dialog lists derived folders under their root by name), CLI `project list` and MCP `switchboard_projects` (`roots` beside `folders`). Taking a root out (dialog ×, or `ProjectEdit remove_folders`) takes its derived folders with it. Tests: `roots::tests::*`, `taking_a_root_out_of_a_project_takes_its_worktrees_too`, `folderRows`/`withoutFolder` in `scripts/test-ui-logic.mjs`. **Disk map and root moves (SB-135, second part):** `RepositoryMap { under }` (read-only, outside the owner transaction) walks each chosen folder up to three levels deep, not into a repository, a hidden folder or `node_modules`/`target`/`vendor`/`build`/`dist`/`Library`/`Pods`/`DerivedData`, and lists each main checkout (`.git` a folder) as `{folder, remote, branch, pool, project, place}`, `place` being `folder` (one of a project's folders) or `inside` (inside one); at most 2,000 repositories and 20,000 folders (`truncated`). No folder named: `~/DATA` when it exists, else the home folder. `MoveRoot { folder, pool }` takes a project folder with the folders derived from it (`roots::derived_of`) out of its project (`ProjectEdit remove_folders`) and into `pool`'s project (`ProjectEdit add_folders`); a refused second save puts them back. A folder in no project joins `pool`'s project. Refusals: `MOVE_ROOT_NO_PROJECT`, `MOVE_ROOT_LAST` (a project keeps a folder), `MOVE_ROOT_INSIDE`. Answer `{folder, from, pool, moved, folders}`. Tauri `repository_map`, `move_root` (Projects page → *Repositories on disk*, SCN-064), CLI `project map [--under]`, `project move-root --folder --pool`, MCP `switchboard_repository_map` (read) and `switchboard_project_move_root` (write). Tests: `roots::tests::the_disk_map_lists_repositories_with_their_projects_and_skips_dependencies`, `a_root_moves_to_another_project_with_its_worktrees_in_one_step`, `mapRows`/`rootTargets`/`storedMapFolders` in `scripts/test-ui-logic.mjs`. Not yet: module checkouts inside a repository (the walk does not go into one). **Account move (SB-122):** `MoveAccount { id, pool }` puts one account into a project's pool (`ProjectEdit` with `add_accounts`, so the old project gives it up and every refusal of a project save applies, `PROJECT_ACCOUNT_IN_CLI` included) or back to `default` (`ProjectEdit` with `remove_accounts` from its project, or `Store::return_to_default` for an account left in a deleted project's pool, which drops its old route and refuses a second copy of the same login). Any other pool is refused (`MOVE_ACCOUNT_NO_PROJECT`); the pool it is already in is no change (`moved: false`). Answer `{id, from, pool, moved}`. Tauri `move_account` (the account card's ⋯ → *Move to another pool…*, SCN-062), CLI `accounts move <id> --pool`, MCP `switchboard_account_move` (write). Tests: `an_account_left_in_a_deleted_projects_pool_returns_to_default`, `an_account_moves_between_projects_and_back_to_default`, `accounts_rotation_activity_and_queues_are_reachable_over_mcp`.

- **SB-117 (operator 2026-10-14; [packet](packets/sb-117-project-suggestions.md)).**
  - `Project.sources: [host/owner]`, lowercase, at most 16, omitted when empty; set by `Store::set_project_sources` (event `project`/`sources`) and kept across `save_project`.
  - `SaveProject` (SB-128): a folder the project already holds that no longer exists on disk is dropped and listed in the answer's `dropped_missing`; any other missing folder is still refused (`Choose an existing project folder.`).
  - A project has no folder limit. All projects together hold at most 1024 folders (`Projects hold at most 1024 folders in all.`), which keeps the metadata worst case at 1 MiB.
  - Suggestions live in `<data>/project-suggestions.json`: 0600, at most 256 waiting and 2048 remembered rows `{folder, pool, reason: worktree|same_repository|ecosystem|worked|unassigned, repository, first_seen, state: pending|dismissed|added|gone|shared}` (`added`, SB-121: `first_seen` is when it was added; a row in `added` or `dismissed` is never added again). They are operational state: not backed up, and removed by uninstall.
  - Matching reads only git metadata: `.git`, `gitdir:`, `commondir`, and `[remote "origin"] url` normalized to the repository identity of fabric-agent-contract DEC-0089 item 2 (`host/path`: schemes `http`, `https`, `ssh`, `git`, `git+ssh`, `git+https` or the scp form; user, port, query, fragment and one `.git` dropped; ASCII-lowercased; a host of `a-z0-9.-` with a dot; at least two path segments, so a GitLab group path `host/group/sub/name` counts), checked against the contract's 28 origin vectors (SB-141); a source is the identity without its last segment (`host/owner` or `host/group/sub`). Which group a found checkout joins is the contract's `attributeFolder` (SB-141 part 2, `attribution::attribute_folder`, ported from fabric-agent-contract `src/folder-attribution.ts` and checked against its 135 attribution and 28 record vectors, all three groups pinned at `4e5440cf` (the contract's release `v0.1.0+2026-10-24`; the vectors are unchanged since `0a4ce773`) in `crates/switchboard-runtime/tests/fixtures/folder-attribution.json` and refreshed only by `scripts/sync_attribution_vectors.py`): the snapshot is the checkout's Git facts, each group as a Project by its pool with its folders' common dir, raw origin and top level, its sources, home and the temporary roots that exist. Attached by `worktree`, `repository` or `owner` joins that group; `folder` means it is a group's already; `ambiguous` is a conflict and attaches nothing; `new` and every refusal (temporary, container, tool, tombstoned) join nothing, and Switchboard never makes a group by itself (SB-146). Switchboard passes no work records (SB-171) and no explicit attach.
  - **SB-121 (operator 2026-10-14, later the same day):** a newly found folder is added at once, by the background pass and by `ProjectSuggest`, through `add_suggested_folder` (the same admission and `save_project` as Accept), and its row becomes `added`. A refused add leaves it `pending` (the question, as before). Rows still `pending` from 0.6.30/0.6.31 are added once per owner run by the first pass. `SaveProject` records every folder it takes out of an existing project as `dismissed` (`suggest::remember_removed`), whoever added it, so nothing removed by the person comes back. `pending` never lists a temporary or cache folder. A 0.6.31 or older build cannot read a file with an `added` row (its enum lacks the state): downgrading makes project suggestions answer unreadable until the file is deleted. `suggest::transient` keeps folders under `/tmp`, `/private/tmp`, `/var/tmp`, `$TMPDIR`, `~/.cache`, `~/Library/Caches` and `AppData/Local/Temp` out entirely. Log `project_suggestion` `{found, added}`, `add: refused`.
  - **SB-131 (operator 2026-10-15):** nothing is ever asked. A `pending` row is a folder whose add was refused; the owner retries every `pending` row every 10 minutes (`RETRY_PENDING`) instead of once per run, and the window shows no banner, the tray no count, the app no question notification.
    - **Worked repositories: removed in 0.6.68 (SB-171, operator 2026-10-22: «не может одна папка … самостоятельно переноситься»).** A repository no longer joins a group because a group's agent edited or committed in it; the transcript and Codex rollout scan that found such work (`activity`) is gone, and a `pending` row an older version recorded as `worked` is never added (`suggest::pending`; `a_folder_never_joins_a_group_because_its_agent_worked_there`). Rows with `reason: worked` and `state: shared` stay readable. A folder joins a group by itself only as a worktree or clone of a repository already in it, or as a new repository of a source the person set; otherwise only by the person's own act.
    - **A folder taken out of its group keeps its running sessions (SB-172).** A Claude Code session reads `ANTHROPIC_BASE_URL` and the folder's capability header once, at start, so a folder taken out of its group (by the person, by `project edit --remove-folder`, or as deleted) kept sending a capability the proxy no longer accepted (401 on every request until restarted). When a bindings sync takes a folder's lines out, it records `{folder, pool, at}` in `<data>/released-folders.json` (0600; no capability); `bindings::released_tokens` drops entries older than `RELEASED_FOR` (7 days) and folders that are in a group again, and hands the proxy the capabilities still honoured, by the pool they left (`ProxyHandle::set_released_tokens`). The proxy serves a request carrying one of them on the `default` pool, never on the group's accounts; a header for another pool is refused. Without a released match the 401 now reads "This folder is not in a Switchboard group now. Restart this session: it then uses your own sign-in. …". Tests: proxy `a_folder_taken_out_lately_is_served_on_the_default_pool`, bindings `a_folder_bound_by_an_older_version_loses_its_auth_token_for_the_header` (released part). `switchboard uninstall` removes the file.
    - **Codex sessions (SB-132): removed with the worked scan in 0.6.68.**
    - **Deleted folders.** Every 5 minutes (`VANISH_LOOK`) the owner records the device of every present project folder and checks every missing one with `projects::deleted` (missing, its parent folder present and on the device the folder was on when last seen — so an unmounted share or FUSE mount is not a deletion — and not under `/Volumes`, `/media`, `/mnt`, `/run/media` or a missing Windows drive; a folder never seen present by this run is not deleted); one still missing `VANISH_CONFIRM` (5 min) after it was first seen missing is removed through `save_project`, and its row becomes `gone` (`first_seen`: when). A project keeps its last folder (`project_folder_gone kept_last`). A `gone` row is found again like a new folder; a `dismissed` row stays dismissed. Log `project_folder_gone {removed}` or `refused`.
    - **Husks (SB-152).** `projects::husk(path)`: a folder with no `.git` whose only entries are `.claude` (holding only `settings.local.json`, `settings.json` or `.DS_Store` files), `.codex` or `.DS_Store` is what a removed worktree or clone leaves when tool files sit in it (Switchboard's own binding writes `.claude/settings.local.json`, SB-118). The vanish pass treats it as deleted (same two looks, `VANISH_CONFIRM`), so such a folder leaves its group instead of staying as a project of its own; an empty folder is not a husk. `suggest::repo_root` refuses a repository right under home (DEC-0089 AT-5: a stray `git init` in `~/DATA` made every folder under it look like one repository's subfolder).
    - `state` gains `gone` and `shared`, `reason` gains `worked`: a 0.6.33 or older build cannot read a file with them (same downgrade rule as SB-121).
    - `suggest::tool_folder` (`~/Library`, `AppData`, every hidden folder directly in the home folder) keeps tools' own folders from joining by work; a worktree a tool keeps there (`~/.codex/worktrees`) still joins as a worktree.
    - `SaveProject` gains `base_folders` (the window's dialog sends the list it opened with): folders Switchboard added meanwhile are kept, only `base − folders` counts as the person's removals (`dismissed`), and a folder Switchboard took out meanwhile is dropped quietly. A held folder that is missing but `projects::maybe_unmounted` is kept as it is; one dropped as gone from disk is remembered `gone`, not `dismissed`.
    - The app's idle restart for an update counts a window open and out of focus for 30 minutes as nobody looking and relaunches it hidden; *Restart to update* keeps the window as the person left it.
  - **REQ-002, superseded by SB-146 (operator 2026-10-19):** an agent's Git repository in no group used to become an account-less project of its own; such projects scattered repositories that belonged to an existing group, and their sessions had no accounts. `suggest::unassigned` still resolves identity the same way, but its `Outcome::Unassigned` creates nothing: `ProjectSuggest` answers `{suggested: false, reason: "unassigned", message}` (`suggest::UNASSIGNED`), the background activity pass skips it, and the repository stays in the default group, listed by `RepositoryMap`. A repository a group's agents work in still joins that group (SB-131), as worktrees and clones do (SB-117/121).
  - Operations: `ProjectSuggestions` → `{suggestions:[{folder, name, pool, project, reason, repository, first_seen}], added:[same, rows added in the last 24 h], removed:[same, rows taken out as deleted in the last 24 h (SB-131)]}` (suggestions: pending, folder present, in no project); `ProjectSuggest { folder }` → (since SB-146 never `created`) `{added: true, suggestion, project}` for an existing-project attach, `{suggested: true, added: false, suggestion, message}` when an existing-project add waits, or `{suggested: false, reason, message}` for idempotent/refused input; `Continue` answers carry the `account` it resolved; `ContinueToConsole` → `{handover, queued, workflow, agent, folder, offeredUntil}` (`queued: false`: the queue could not take it and Terminal opened it); `ProjectSuggestionAccept { folder }` adds a waiting folder the same way; `ProjectSuggestionDismiss { folder }`; `ProjectSources { pool, sources }`.
  - Background: a real owner's pass runs `suggest::Scanner` under the owner transaction. It reads a parent folder or `.git/worktrees` only when its modification time changed; a new folder without `.git` is looked at again for 10 minutes. The activity pass never creates a group (SB-146).
  - `Runtime::suggestions_seq()` moves on every change. The app reads the list only then (every 15 s), notifies each added folder once (“{folder} is now part of {project}.”) and each folder taken out as deleted once (“{folder} was deleted and is no longer part of {project}.”) — only rows since ten minutes before the app started — and emits `project-suggestions`, on which the window reads the projects again (SB-131: no tray count, no question).
  - Tauri: `project_suggestions`, `accept_project_suggestion`, `dismiss_project_suggestion`. CLI: `project suggest [--folder|--from-hook]`, `suggestions`, `accept`, `dismiss`, `source`.
  - Plugin `SessionStart` hook: `switchboard project suggest --from-hook 2>/dev/null || true`, hook `timeout` 3 s. The command bounds itself at 1.5 s (`SESSION_HOOK_DEADLINE`, stdin read and owner call together) and on that deadline prints nothing and exits 0; the owner may still finish the add, and the background pass makes it within 30 s otherwise (SB-149; CLI `the_session_start_hook_gives_up_quickly_when_the_owner_does_not_answer`, `the_session_start_hook_waits_for_no_input_it_was_not_given`, `the_plugin_bounds_its_session_start_hook`). Since 0.6.81 an answer that writes nothing (the folder is already in its group, or no group claims it) is given without the owner transaction, so a background step no longer makes the hook give up; only a folder to record and add waits for it (SB-186, `a_session_start_in_its_groups_folder_is_answered_while_a_step_holds_the_transaction`). The StopFailure hook is wrapped the same way, so a CLI older than the plugin stays silent. A created project reports that no accounts were moved.
  - Compatibility: `accounts.json` is `schema_version` 5 while a project has sources, so a 0.6.29 or older build says "Unsupported account metadata version" instead of "corrupt". Backups carry sources in `Payload.project_sources`, beside `projects`, so an older build still restores everything else. Sources changes are journaled as `project`/`updated`. A `project-suggestions.json` row with `reason: unassigned` is unreadable to 0.6.36 and older; delete that operational file before downgrading.
  - The scanner lists folders outside the owner transaction and records under it. Folders one pass found join with one `save_project` per group and one write of `project-suggestions.json` (`add_suggested_folders`, SB-180); a group whose save is refused is tried folder by folder, so one folder that cannot join keeps no other out (`folders_found_in_one_pass_join_their_group_in_one_save`). It never watches the home folder or a filesystem root as a parent. A folder that contains another project's folder is never suggested. Accept admits the folder through `project_dir`; nothing creates a group (SB-146).
  - Evidence: `suggest::tests` including `unassigned_repositories_are_not_guessed_from_conflicts_or_remembered_removals`, core `a_project_has_no_folder_limit_and_all_projects_hold_1024`, owner `an_unassigned_git_repository_becomes_a_project_without_changing_provider_fallback` and `the_background_pass_creates_an_unassigned_agents_project_once`, CLI `an_unassigned_repository_becomes_a_project_through_the_session_hook` and `project_suggestions_work_from_the_cli_and_the_hook_never_fails`, the app's `suggestions::tests`.

- **SB-118 (operator 2026-10-14; [packet](packets/sb-118-claude-bindings.md)).**
  - `switchboard_proxy::pool_token(session, pool)` = SHA-256 of `switchboard-pool:<pool>:<session>`, in hex. It is accepted as the session token for `/claude|codex/<pool>/…`, and only as a bearer.
  - `bindings::sync` writes `env.ANTHROPIC_BASE_URL` and `env.ANTHROPIC_CUSTOM_HEADERS` (`X-Switchboard-Capability: <pool capability>`, SB-140; up to 0.6.37 `env.ANTHROPIC_AUTH_TOKEN`, which Claude Code counts as an auth source and which turned off claude.ai connectors; a sync removes that older value wherever it is still Switchboard's) into `<folder>/.claude/settings.local.json` (0600, atomic) for every folder of a project with an enabled Claude account, and for worktrees of its repositories inside a project folder.
  - It appends `.claude/settings.local.json` to the repository's `info/exclude`.
  - It records `{folder, pool, base_url, token_sha256, created}` in `<data>/claude-bindings.json` (0600; operational, not backed up, removed by uninstall after `unbind_all`).
  - It removes only values equal to what it wrote and never touches a file that is not a JSON object.
  - Runs from a real owner's pass when `bindings::key` moved, under the owner transaction; log `claude_binding` (bound, removed, skipped counts).
  - `Operation::ProjectBindings` → `{bindings:[{folder, pool, base_url}]}`; CLI `project bindings`.

## 0.5 additions

- Operations ([runtime](../crates/switchboard-runtime/src/lib.rs)): `LoginStatus { login_id }` → `{state: pending|complete|ended}`; `Backups` → `{directory, enabled, backups: [{file, created_at, accounts, missing, own, openable}], last_error, last_written_at}`; `BackupNow` → backup info or null (refused without a running owner on the default data folder); `RestoreBackup { file }` → `{added, skipped, failed}`, `file` must be a `switchboard-backup-<digits>-<32 hex UUID>.json` name in the backup folder; legacy `switchboard-backup-<digits>.json` names are still accepted. New generations reserve a private destination exclusively and never replace another backup (`backup::tests::same_second_partial_backup_never_replaces_the_complete_one`). Tauri commands `login_status`, `backups`, `backup_now`, `restore_backup`; CLI `login status`, `backup list|now|restore`. None returns a credential.
- `MonitorStatus` adds `sign_in_required: [account id]` and `limited: [{account_id, until, source: managed|claude_code}]`; the MCP `switchboard_status` result adds `limited`. Decision reasons add `limit_reached`, `limit_no_eligible_account`, `switched_on_limit`.
- `Store::rotation_decision_with(policy, current, now, limited)`; `rotation_decision` passes an empty set. `Store::adopt_refreshed`, `Store::export`, `Store::changes`, `Store::backup_id`.
- Backup file ([backup](../crates/switchboard-core/src/backup.rs)): JSON `{format: "fabric-switchboard-backup", version: 1, created_at, accounts, missing, store, key, nonce, data}`; `data` is AES-256-GCM over the accounts, policies and credentials and, since 0.6, the project rules, projects, managed selections (`routes`) and the settings files `login-item`, `auto-update`, `agents-settings.json` and, since 0.7.0, `spend.json` (`SETTING_FILES`; absent in a 0.5 backup, ignored by an older reader; each at most 4 KiB, `spend.json` at most 128 KiB, and a file past its limit is not carried: `a_backup_carries_the_spending_ceilings_whole`; restore writes a JSON setting back byte for byte, a switch word with its line end, so a `spend.json` of exactly its limit stays readable: `a_spend_file_of_exactly_the_limit_comes_back_from_a_backup_readable`); a rule change now counts for the next backup (`a_rule_change_counts_for_the_next_backup`), and so does a spending ceiling set or answered (`Store::setting_changed`; the gate's own records in `spend.json` wait for the next change-driven or daily backup). Restore maps each backup account to its id on this install, then puts back projects whose pool and folders are free, unexpired rules and selections not already set, and settings not chosen here (`a_reinstall_gets_projects_rules_selections_and_settings_back`); the result adds `projects`, `rules`, `routes`, `settings`. **A data folder created by this start** (a new install, or after `switchboard uninstall`) with no accounts restores the newest backup this machine can open at once, reported as `Backups.restored_at_start`; a store that existed before the start is never restored on its own (`a_new_data_folder_restores_the_newest_backup_and_an_existing_one_never_does`). AAD `format:v1:created_at:store:accounts:missing`; `key` is the first 16 hex digits of SHA-256 of the key.

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
- **Coalescing and the transaction.** Same-account checks share one request: a caller that asked while a check ran takes its answer — unless the store's change count moved after that check began (a new sign-in, a renewal, an edit), when it checks on its own (`a_new_sign_in_during_a_check_is_checked_on_its_own`). `Runtime::execute` answers `Operation::Usage` without the owner transaction; only the renewal inside `check` takes it, as the background pass does (`same_account_checks_share_one_request_and_hold_no_transaction`). **A grant holds no transaction (SB-179).** `refresh::ensure_fresh_with(…, lock)` takes the transaction for the checks and the token's reservation (`reserve`: the refresh token is marked in flight before the transaction is let go), releases it while the grant is on the network, and takes it again to store the successor. While the token is in flight a native activation of the account is refused (`RENEWING`) and a second grant of it too; a credential stored meanwhile (a new sign-in) stands, since `adopt_refreshed_for` stores the successor only where the spent token still is, and the grant answers `NotNeeded` (`a_grant_runs_outside_the_transaction_and_a_sign_in_stored_meanwhile_stands`). `Owner::shutdown`, once it holds the transaction, waits inside its deadline until no grant is on the network (`RefreshState::grants_ended`; grants are already off, so none starts) and then stores every kept successor (`refresh::adopt_all_kept`): a quota check from the window, the CLI or MCP renews outside the transaction and the monitor's stop does not cover it, so before 0.6.79 its successor could die with the process and the account needed a new sign-in (`the_drain_stores_the_token_a_quota_checks_renewal_returns_during_it`, `the_drain_stores_a_successor_kept_in_memory`). A caller that holds the transaction (a switch, a launch, a continue) and finds the account's token in flight waits up to 12 s for that grant and takes its successor (`IN_FLIGHT_WAIT`, `Step::InFlight`; `an_account_whose_renewal_is_in_flight_is_not_switched_to`), so an isolated launch no longer starts on a token with minutes left. When the endpoint names the token's owner, a successor is filed through the owner's other copies only if the account still holds the spent token: a sign-in stored during the grant stands (`settle_as`). A grant the provider rejects marks the account only if it still holds that lineage (`a_rejected_grant_does_not_mark_a_sign_in_stored_meanwhile`). Each span under the transaction is logged as `transaction_held` when long: `step` `renewal` (background) or `usage_renewal` (a quota check), phase `checks` (marked after the checks and the reservation ran, PRD-02; before 0.6.82 it was marked first and read 0) or `store`, and `rest`, the time after the last mark (`crate::hold_on`; `a_renewals_hold_names_the_time_its_checks_took`, `a_long_hold_names_the_time_after_its_last_mark`). The background renewal step and quota checks use it; operations already under the transaction (a switch, a launch) call `ensure_fresh` as before. `source_sync`'s renewal of the ordinary Claude Code's expired token still runs under the transaction, under Claude Code's own locks. A quit therefore does not wait for a quota request in flight; `Owner::shutdown` turns refresh grants off before its drain, so such a check cannot spend a refresh token after it — its observation is lost at worst, never a credential (`a_quota_check_left_running_at_quit_spends_no_refresh_token`). Coalescing compares the store's global change count: a concurrent unrelated edit costs at most one extra request, never a stale answer (judgement, seam review).
- **Renderer.** No new IPC field or error sentence. A failed row with no stored observation shows `Next check {date}` (`failedNextCheck`, `scripts/test-ui-logic.mjs`).
- Tests reach a synthetic usage endpoint through the proxy feature `synthetic-origins`, enabled only by the runtime's dev-dependencies; shipped binaries call only the fixed provider origins.
- **A command that finds the store locked waits (SB-181).** `switchboard_runtime::call` (every CLI command and `switchboard mcp` tool that goes to the owner) asks the owner through `control.json`, else runs offline; when the store's lock is held (`switchboard_core::STORE_LOCKED`: an owner draining or starting, or another offline command) it asks again every 200 ms, the owner first, for up to `STORE_WAIT` = 8 s, under the 10 s Fabric Dashboards gives a read. Before, the CLI refused at once and MCP waited 1.5 s, so on 2026-10-23 02:35 UTC an update restart refused Dashboards' `handovers` and `recovery` for its whole length (board finding `0ab48130`). A lock still held after the wait is reported as before. After an update restart's drain, with the relaunch helper started, the app exits at once (`update_restart outcome=exiting`) instead of waiting for the event loop's teardown, which kept the lock 11 s longer, up to the hard exit. Tests: `a_command_waits_for_a_store_a_draining_owner_still_holds`, `a_command_waiting_for_the_store_is_answered_by_the_owner_that_starts`, `a_store_held_past_the_wait_is_reported`.
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
- SB-160 (0.7.0 node B-4, SCN-077): the `FinishLogin` receipt, and the `CaptureCurrent` receipt, carry `organization_group: {pool, name}` when the account belongs to an organization (`Snapshot::attribution`) that has exactly one group (`Snapshot::organization_group`) and the account is not in it; the capture receipt adds `in_cli: true` (the ordinary CLI's account cannot join a group until the CLI switches, `PROJECT_ACCOUNT_IN_CLI`). Absent for a personal account, an organization with no group or several, or an account already in its group. Nothing moves: the window's notice offers *Move it to “{group}”* (`signInMove`, `Operation::MoveAccount` on press) or, for the CLI's account, says to switch first (`organizationMoveNote`). Test `a_new_seat_of_an_organization_outside_its_only_group_is_offered_it`.

## Uninstall (SB-29, 2026-10-05)

- `switchboard uninstall [--keep-data] [--yes]` → `switchboard_runtime::uninstall::uninstall(root, vault, keep_data, apply, places)`. Without `--yes` it returns the plan (`accounts`, `cli_link`, `orphan_keychain_item`, `data_entries`, `kept`, `applied: false`) and changes nothing. The store is opened exclusively, so it refuses while an owner runs.
- With `--yes`: `Vault::delete` for every account (shared and legacy items on macOS; DPAPI files on Windows), the orphaned 0.5.0 `ai.passioncode.fabric-switchboard.vault-key` / `v1` item through `/usr/bin/security` (absent is success), the `<home>/.local/bin/switchboard` symlink only when it points to a file named like the CLI, then — unless `--keep-data` — only the data-folder entries Switchboard writes (`accounts.json`, `instance.lock`, `control.json`, `proxy.json`, `renewal-state.json`, `usage-holds.json`, `limit-evidence.json`, `backup-id`, `homes`, `runtimes`, `logins`, `vault`, `kimi` — the Kimi Code homes, each the only copy of its login (SB-81) — `.private-*` temporaries); the folder is removed only when nothing else is in it. Each failure is reported in `failures`; nothing stops at the first. Backups and their key are never touched (`without_apply_nothing_changes`, `apply_removes_owned_things_and_keeps_the_persons_files`, `keep_data_keeps_the_folder_and_a_foreign_link_is_left_alone`, `an_empty_folder_is_removed_and_a_running_owner_refuses`).

## Near-limit quota checks (SB-106, 2026-10-12)

- **Who.** The account a session works on where a switch can follow it: the pool's managed route,
  or the ordinary CLI's account (`Store::set_in_use`) in a pool with an enabled policy. A pool
  member that is neither keeps the SB-48 cadence, near its limit or not.
- **Cadence.** `switchboard_core::near_limit_check(usage, observed)` on the account-wide usage
  (`Usage::account_used_percent`, feature windows excluded): `NEAR_LIMIT_PERCENT` 95 →
  `CHECK_NEAR_LIMIT_SECONDS` 60; `AT_LIMIT_PERCENT` 98 → `CHECK_AT_LIMIT_SECONDS` 30, one pass
  (LC-08 holds: nothing faster than the pass); 100 → none (full until a reset); both less
  `NEAR_LIMIT_SLACK_SECONDS` (below). Applied in
  `observe_generation` as `min(next_quota_check, near_limit_check)`; response headers still
  never move the schedule.
- **The next account.** `Store::pull_in_next_accounts(now)`: for each such pool whose current
  account was measured ≥ 95 % within ten minutes, or is held by a limit (`Store::limited`), the
  account a failover would most likely take (`Store::next_to_check`: `failover_candidate`'s order
  over enabled OAuth accounts with `ok` health, not held, not current — from metadata alone, no
  credential read) is due `checked_at + 50` — only when that is sooner, in memory (`hold`, no
  write; LC-08). The monitor calls it at most once a minute.
- **Slack.** A check is stamped when its answer arrives, seconds after its pass began, so both
  near-limit schedules are set `NEAR_LIMIT_SLACK_SECONDS` = 10 s early (50 s and 20 s after the
  answer): they fall due on the pass they name — otherwise the 30-second step would run every
  minute (found by review, 2026-10-12).
- **Refused checks pace.** `UsageGate::refused_within(now, PACED_AFTER_REFUSAL_SECONDS = 3600)`
  — the accounts whose usage check the provider answered 429 in the last hour, memory only,
  seeded at start from the holds in `usage-holds.json` that still apply (a refusal whose wait already ended before a restart is not remembered), a time ahead of the clock counted from now — is handed
  to `Store::set_paced` every pass; a paced account gets neither cadence nor a pull-in. Measured
  2026-10-06…12 on the operator's Mac: 1–33 refused checks a day at the 180-second cadence.
- **No schema change**, no new log event, no analytics field. Tests
  `the_account_a_session_works_on_is_checked_sooner_near_its_limit`,
  `the_pools_next_account_is_checked_every_minute_while_the_current_one_nears_its_limit`
  (core), `a_refused_account_is_paced_for_a_while_across_a_restart` (runtime); the idle-hour
  budget test is unchanged.
- **Hidden limits read as personal.** A Claude Code "session limit" (`rateLimitType`
  `five_hour`, `resetsAt`) is read from the marker's fields, never from its wording
  (`a_session_limit_with_the_usage_figures_below_full_is_read_with_its_reset`); the hidden row
  now says “Hidden personal limit · …”.

## Agents keep working through a limit (SB-98, 2026-10-11)

Measured on the operator's machine (Claude Code transcripts, 3 days): 264 sessions hit a limit;
a session that went on did so after a median 77 min (p90 8 h), 156 never went on. Causes found
and closed here:

- **A limited account in use never waits for fresh figures.** `rotation_decision_with`, with the
  current account limited and no candidate measured within `max_age_seconds`, takes the one whose
  last known usage leaves room (`last_known_usage`: windows all reset since → 0; never measured →
  99, last in line; still full → never). Without a limit nothing is guessed. Test
  `a_limited_account_moves_on_last_known_usage_when_nothing_is_fresh`.
- **Managed failover** (operator decision 2026-10-11; AGENTS.md rule amended). A Claude Code or
  Codex session's request answered 429 is journaled `rate_limited` on its account, its usage
  headers kept, and `Store::failover` holds the account (`limited`, `take_refusals`) and moves
  the pool's route to the next usable account — the managed policy's figures when the pool has
  one, on or off, else 90 / 10 / 300 s — then the same buffered body is sent **once** there.
  A second refusal comes back to the session as is. An agent's API-key request is never moved.
  Tests `a_session_refused_for_a_limit_continues_on_the_next_account_without_seeing_it`,
  `a_refusal_moves_once_to_the_next_account_and_events_contain_no_secrets`,
  `failover_moves_the_pool_off_a_refused_account_and_holds_it`.
- **A refusal is evidence at once.** Each monitor pass takes the proxy's refusals and holds the
  account (`LimitState::refused`, `Attributed`) until the provider's reset when a window is full,
  else 900 s; then publishes the held accounts to the store for the proxy (`set_limited`). Test
  `a_refused_managed_request_holds_its_account_at_once_until_the_reset`.
- **A failed switch tries the next candidate.** A candidate whose activation or selection fails is
  set aside for 10 min (`Runtime::failed_switches`) and the next one is tried in the same pass,
  up to three; with none left the decision reports the failure. Test
  `a_candidate_that_fails_to_activate_is_set_aside_and_the_next_one_tried_at_once`.
- **Hidden limits.** A Claude Code marker's `quotaLimits.overageDisabledReason`
  (`org_spend_cap_reached`, `group_zero_credit_limit`, `org_level_disabled`) is kept as the
  limit's `reason`, allowlisted; `hidden` is set when the provider refused while its own usage
  figures for that window (or, for a managed refusal, every window) still showed room. `limited`
  rows carry `reason` and `hidden`; the account row says “Hidden personal limit · refused although
  the usage figures show room” (SB-106 wording) and the reason in words. Test
  `a_spend_cap_reason_is_kept_and_a_limit_the_usage_figures_miss_is_hidden`.
- Not changed: an ordinary (native) Claude Code session whose turn already ended on a limit does
  not resume by itself — run agents in managed mode for no pauses.

## Sign-ins, sessions and Kimi without loose ends (SB-103, 2026-10-12)

From the same review, the second part:

- **A saved sign-in is never taken back.** `finish_login` removes the sign-in's journal
  (`forget_login_journal`) before its folder; a restart that finds the folder still there no longer
  offers to finish it again, which would have written its first credential over a newer one.
- **Pending sign-ins in the order they began.** The owner lists them by their journal's
  `started_at` (`login_started`), oldest first, so a window restoring one banner takes the newest.
- **The start-up sweep leaves young folders alone.** `sweep_logins` skips a sign-in folder made in
  the last 60 s: the sweep runs beside a starting owner, and a sign-in begun right then is not a
  leftover.
- **Sessions off the async workers, Kimi labelled.** `Operation::Sessions` runs the liveness
  checks (`kill -0`, `ps`) through `blocking::run`, and a Kimi session carries its account's label.
- **Kimi.** A re-sign-in removes the folder's last session record (a sign-in is not a session);
  one account's failed status task leaves `KIMI_UNREACHABLE` on its own row only; the shared HTTP
  client is built again after a failure instead of failing for the life of the process.
- **Onboarding on Windows.** `switchboard onboard --status` reads the home from `USERPROFILE` on
  Windows.
- Tests: `a_sign_in_folder_made_a_moment_ago_is_never_swept`, and the extended
  `a_sign_in_open_across_a_restart_is_taken_back_and_kept_from_the_sweep` (order, and a forgotten
  journal is not taken back).

## Failover without side effects (SB-102, 2026-10-12)

A review on 2026-10-12 (three independent readings of the switching core) found SB-98's failover
could hurt as well as help. Decided and built:

- **Only a compatible account.** `Store::failover` and `failover_unusable` take a `Fits`: the
  kind of the account the request leaves (OAuth ↔ OAuth, API key ↔ API key; any when that account
  is gone) and `api_key_only` for Chat Completions. A subscription session never moves onto a
  pay-per-token key, and a pool's key route shared with agents never moves onto a subscription
  sign-in (agents would get 403). Candidate: `failover_candidate` — measured lately and below full
  first, then last known usage, never still full.
- **Compare and move.** The route changes only while it still names the refused (or broken)
  account (`select_transaction` with an expectation); a refusal that comes back after another
  request or the person moved the pool takes the route as it is. Concurrent refusals no longer
  repeat the move, and concurrent repairs of a broken route end on one account.
- **One held set.** `set_limited` keeps refusals not yet taken by the runtime; `rotate` adds
  `Store::limited` to its exclusions, so the pass never moves a pool back onto a refused account.
- **Set-aside candidates are candidates only.** `failed_switches` never excludes the account in
  use, drops expired entries, and is cleared for an account a switch then reaches; a successful
  native switch is never reported failed because noting its time failed.
- **Holds and hidden limits read the right figures.** A refusal holds until the reset of a full
  `five_hour` or `seven_day` window only (a model's weekly window no longer holds the account);
  `hidden` needs figures from the last ten minutes.
- **Fallback respects projects before acting.** `fallback::resolve` takes the workflow's folder
  and skips accounts `project_allows` refuses there, before any renewal or launch.
- Tests: `a_failover_moves_only_to_an_account_of_the_same_kind`,
  `a_late_refusal_never_moves_a_route_that_already_moved`,
  `a_pool_left_without_a_usable_selection_still_moves_and_concurrent_repairs_agree`,
  `publishing_the_held_accounts_keeps_a_refusal_not_yet_taken`,
  `an_account_set_aside_earlier_is_not_limited_once_it_is_in_use`,
  `a_projects_reserved_account_never_takes_over_work_outside_its_folders`, and the extended
  `a_spend_cap_reason_is_kept_and_a_limit_the_usage_figures_miss_is_hidden`.

## Switch reliability (SB-101, 2026-10-11)

From the switch audit of 2026-10-11:

- **Lock waits leave the async workers.** `external::acquire_lock` (up to 9 s per lock, three
  locks per activation) runs through `blocking::run`, so a native switch or an idle renewal
  waiting for Claude Code's lock no longer stalls proxy streams and UI, CLI or MCP requests.
- **Claude Code's own writes survive a switch.** Just before writing `~/.claude.json`, the file
  is read again; when Claude Code wrote it meanwhile, only the sign-in's keys (`oauthAccount`,
  `primaryApiKey`) are carried into the current content (`with_account_of`). A rollback puts
  back only those keys when the file changed, and the old bytes unchanged otherwise. Test
  `a_switch_keeps_what_claude_code_wrote_into_its_config_meanwhile`.
- **No 409 while the pool has a usable account.** A session's request whose selected account
  cannot serve it — its token expired, or it is gone — moves the pool to its next usable account
  (`Store::failover_unusable`); the account is not held as limited. Tests
  `an_unusable_selection_moves_the_pool_without_holding_the_account`,
  `a_session_never_meets_409_while_the_pool_has_a_usable_account`.
- **A renewed sign-in is on disk before anything can lose it** (2026-10-12). When the idle
  renewal of the ordinary Claude Code's account got a successor but could not write Claude Code's
  item, the successor went only to memory; a quit or crash then left the account with a spent
  token. It is now filed into the owner's vault copies at once (`file_with_owner`), and the kept
  copy still makes the next pass write Claude Code's item first. Test
  `an_idle_renewal_that_cannot_be_written_is_stashed` (extended).
- **No overall proxy deadline — decided.** A hung provider is already cut by the read timeout
  (600 s of silence, journaled `aborted`); a cap on a whole response would cut legitimate long
  streams of a working model.

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
- `Operation::Chains {workflow?, pool?, folder?}` → `{chains, effective, presets, readiness,
  last_handover, ceilings_unreadable, limits_known}`; `Operation::SetChain {scope, executors,
  preset?}` → `{chain}`; a preset id (`subscriptions-first`) stands for the list. With `folder` (a
  workflow's checkout, canonicalized when it exists) `effective` is resolved through the group the
  automatic fallback would choose (`fallback::chain_pool_of`, below), `pool` then standing for the
  executor account's group; without it, `pool` alone as before. `limits_known` is false in an
  offline answer (no owner, so no limit evidence: `readiness` cannot name a limited account).
  MCP `switchboard_chain_get` (`workflow_id?`, `pool?`, `folder?`, an absolute folder) and CLI
  `chain list [--workflow] [--pool] [--dir]` (`--dir` absolute) return the same answer. Tests
  `the_chain_that_applies_is_found_through_the_checkout_folder_like_the_fallback`,
  `handshake_lists_tools_and_rejects_malformed_messages` (`crates/switchboard-cli/tests/mcp.rs`).
- **Readiness (0.7.0, C-2; SB-71 screen, SB-73 status).** Code:
  `crates/switchboard-runtime/src/chain_view.rs`. `readiness` holds, for every chain set, keyed
  `machine`, `project:<pool>` or `workflow:<id>`, one view per step of what it would do now:
  `{agent, name, state: ready|skipped|not_automatic|not_installed, reason, paid, account_id?,
  account_label?, key_scope?: group|machine, key?, spend?}`, built on the fallback's own
  `resolve_with` with no executor and no workflow, after these checks in order: agents-settings.json
  unreadable (`skipped`, `agent_settings_unreadable`), a step pinned to a key (`not_automatic`,
  `key_pinned`, the vault name in `key`, never a value), an agent the fallback cannot run
  (`not_automatic`, `agent_not_automatic_yet`), a disabled agent (`skipped`, `agent_disabled`), an
  agent not on this computer (`not_installed`); then the resolution's skip reason
  (`no_usable_account`, or `sign_in_required` for a pinned account that needs a new sign-in). A
  group's chain is read for the group's first folder, so its account rules (SB-102) apply; this
  computer's and a task's chains with no folder. `paid` is Hermes and OpenCode (with the
  OpenRouter key they spend, `key_scope`) and Claude Code or Codex on an API-key account (C-Q3);
  `spend` is null: a read never starts the workflow engine, a hand-over or a network call, and a
  key's spend is read only at a paid hand-over (C-5). `last_handover` is spend.json's record
  (written by C-5; null until then); `ceilings_unreadable: true` when spend.json cannot be read,
  the chains staying readable. Tauri commands `chains` and `set_chain {scope, executors, preset?}`
  (labels and codes only). Tests `chain_view::tests` and
  `owner_tests::the_chain_view_names_why_each_step_would_be_skipped`.
- The automatic fallback (SB-73, below) walks the chain that applies; spending ceilings for paid keys
  are SB-72.
- **Spending ceilings, storage (0.7.0, C-4; the hand-over gate that enforces them is C-5).** Code:
  `crates/switchboard-runtime/src/spend.rs`. `<data>/spend.json` (0600, at most 128 KiB, carried in
  backups, removed by `switchboard uninstall`), never `Snapshot`, so a 0.6.x build still reads the
  store after a downgrade. Content: `{version: 1, ceilings: [{scope, cents, set_at,
  doubled_from?}], questions: [{id: "q_" + 16 hex, scope, cents, spent, reached_at, answer?:
  keep|double, answered_at?, answered_by?: window|cli|mcp}], baselines: [{workflow, key: machine
  or a pool, usage, at}], last_handover?}`; at most 256 ceilings (one per `ChainScope`: the
  machine's, at most 63 groups' and at most 192 tasks', so the machine's and a group's always
  have room: `At most 192 task ceilings. ...`, `At most 63 group ceilings. ...`;
  `task_ceilings_leave_room_for_the_machine_and_the_groups`), 64 questions, 256 baselines. The
  largest file these caps allow is about 104 KiB (`the_largest_file_the_caps_allow_fits_under_the_limit`);
  a write past 128 KiB is refused with its own sentence (`The spending ceilings file is full. ...`,
  `a_write_past_the_limit_is_refused_with_its_own_sentence_and_nothing_written`), never as
  unreadable. A task ceiling stays until the window clears it. A ceiling is whole cents from $0.01 to $1,000 (`spend::cents`: zero,
  negative, NaN, infinite or out-of-range amounts are refused and nothing is written). Machine
  and group ceilings count per UTC day; a group takes one only with its own OpenRouter key
  (`Give this group its own key under API keys first.`), and one whose key was removed is kept
  and shown inactive (`inactive_reason: no_own_key`); a task ceiling counts the key's usage since
  the task's first paid hand-over (C-Q2). Setting a scope's value closes its open questions: an
  answer is tied to the value it was asked about. `keep` is accepted from every surface,
  `double` only with `by: window` (`Doubling a ceiling is answered in the Switchboard window.`,
  C-Q1), doubling past $1,000 is refused and the question stays open. `by: window` is accepted
  only in the app's own process: the control listener and an offline command refuse it before it
  runs (`Only the Switchboard window answers as the window. ...`;
  `a_ceiling_answer_naming_the_window_is_refused_through_the_control_listener`,
  `an_offline_command_cannot_answer_a_ceiling_as_the_window`). A removed group takes its
  ceiling, questions and baselines along; answered questions and baselines go after 30 days, an
  open daily question after the next UTC day, an open task question 7 days after it was asked; a
  clock set back drops nothing. When 64 questions are held, a new one takes the slot of the
  oldest answered question, else of the oldest open task question, and a machine or group
  question then of the oldest open question, so it always gets one
  (`a_machine_question_always_fits_and_open_task_questions_age_out`). Deleting a group
  (`Operation::RemoveProject {pool, by}`, `by` defaulting to `cli`) prunes spend.json under its
  write lock, so a group saved again under the same id inherits nothing; a group that holds a
  ceiling is deleted only with `by: window` in the app's own process (C-Q7): from the CLI, MCP,
  the control listener or an offline command it is refused (`This group has a spending ceiling.
  Delete the group in the Switchboard window, where its ceiling goes with it.`, `by: window`
  there refused as `WINDOW_ONLY`), the group and the ceiling kept, and a spend.json that cannot be
  read refuses every group deletion
  (`a_group_holding_a_ceiling_is_deleted_only_in_the_window_and_takes_it_along`,
  `deleting_a_group_takes_its_ceiling_questions_and_baselines_and_keeps_the_rest`). A file that cannot
  be read, is not a JSON object, or breaks any of these rules is an error, never "no ceilings":
  `spend::check` then skips every paid step (`ceilings_unreadable`) and every write is refused.
  Ceilings are no boundary against a process running as the operator: a hand edit that keeps
  every rule, or deleting the file, loosens or removes them (a backup or the log shows it later).
  After a downgrade to 0.6.x paid steps run with no ceiling (0.6.x has no gate and never reads
  spend.json), and its `switchboard uninstall` leaves `spend.json` behind, since 0.6.x does not know
  the file. A ceiling set or answered through `SetCeiling` or `AnswerCeiling` counts as a change
  for the next backup (`Store::setting_changed`, so the change-driven backup takes it within its
  60 s spacing; `spending_ceilings_are_set_read_and_answered_through_the_runtime`); records the
  gate writes (questions it opens, baselines, the last hand-over) ride the next change-driven or
  daily backup until C-5 bumps the count for them too. `backup::MAX_SPEND_SETTING` equals
  `spend::MAX_BYTES`, so every file this build writes is carried
  (`every_spend_file_this_build_writes_fits_a_backup`).
  `spend::decide` (pure; called by C-5) counts no ceiling of a group the snapshot no longer holds
  (`a_ceiling_of_a_removed_group_covers_no_step`), counts a negative or NaN usage read as unknown
  and a huge one as spend (`a_provider_read_that_is_negative_nan_or_huge_is_never_counted_as_spend`),
  and skips a paid step, in order: `key_exhausted` when the
  key's own limit has less than $0.50 left, with or without a ceiling; `ceiling_reached` naming
  the strictest reached ceiling; `spend_unknown` when a covering ceiling's spend could not be
  read (an API-key Claude Code or Codex account under a ceiling is always unknown, C-Q3); with no
  covering ceiling an unread key runs (`spend_unread`). Log event `ceiling` with `action`
  (`set`, `cleared`, `reached`, `kept`, `doubled`, `removed` when a deleted group's ceiling went
  with it, `remove_failed` when spend.json could not be written after a group was deleted, the
  next change pruning it) and `scope` (`machine`, `project`,
  `workflow`). `Operation::Ceilings` → `{ceilings, questions, last_handover, limits}`;
  `Operation::SetCeiling {scope, usd, by}` (`usd: null` clears; a body without `usd` or `by` is
  refused, never read as a clear) → `{ceiling}`. From `by: cli` or `by: mcp` only a first ceiling
  of a scope or a value no higher than its current one is accepted; raising or clearing one is
  refused (`Raising or clearing a ceiling is done in the Switchboard window. ...`, decided against
  the file under its write lock, nothing written), and `by: window` through the control listener
  or an offline command is refused like a ceiling answer (`WINDOW_ONLY`;
  `a_ceiling_is_raised_or_cleared_only_by_the_window_never_through_the_listener`);
  `Operation::AnswerCeiling {id, answer, by}` → `{answer, ceiling}`. One read-modify-write of
  spend.json at a time in a process (`two_surfaces_setting_one_ceiling_at_once_never_loosen_it`,
  `four_scopes_set_at_once_are_all_kept`). `spend::key_slot(snapshot, folder)` names the key a
  paid step in `folder` spends, for the gate's `PaidStep::key` (C-5) and the chains view's
  `key_scope`; it and `Store::agent_key_in` share one rule, `agent_keys::key_in`
  (`the_view_and_the_gate_name_the_key_a_launch_in_the_folder_uses`). An unreadable spend.json
  names the order of recovery: delete it first, since a restore never writes over an existing
  setting. Tests in `spend::tests` and
  `spending_ceilings_are_set_read_and_answered_through_the_runtime`.

## Automatic fallback, phase 1 (SB-73, 2026-10-06)

Design: [XA-01](packets/cross-agent-continuation.md) (operator decision D-1: automatic). Code:
`crates/switchboard-runtime/src/fallback.rs` (`pass`, `pass_with`, `resolve`, `executor_limited`),
`continuation::{list_open, checkouts}`, called by the monitor after rotation.

- **Trigger:** a chain applies (SB-71: the workflow's, else its group's, else the machine's; none
  set, nothing happens), and the executor of an open workflow without a
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
- **Phase 1** runs Claude Code and Codex; **Kimi Code on a saved Kimi Code account** joins it
  (SB-99, below), and **Hermes Agent on the saved OpenRouter key** (SB-107): a `hermes` step with
  no account and no key pin is tried when Hermes is installed, the key is saved and the
  `switchboard` command exists (`launch::hermes_continuation_basics`, no process started), never
  for a Hermes executor. At the hand-over those basics come first; only then is the model read —
  `hermes config get model --json` within 5 s in its own process group, killed with everything it
  started at the deadline (`hermes::run_within`; a Hermes that does not answer gives no model).
  The model is the one the ordinary Hermes runs on OpenRouter (`hermes::openrouter_model`, from
  `hermes config get model`; operator 2026-10-12: «у меня Гермес был на DeepSeek V4 Pro»), else the
  key's default model; with neither, `hermes_continuation_preflight` refuses before the offer and
  the chain moves on. The answer says which (`modelFrom`: `hermes` or `key`). Hermes runs in
  `runtimes/hermes-continue/agent-home` (its `HERMES_HOME`, a `config.yaml` Switchboard writes:
  Observatory's MCP server, `auth.adopt_external_logins: false`), the handoff prompt in
  `runtimes/hermes-continue/<wf>/handoff-prompt.txt` (0600; the folder also holds the script and
  a `.session.json`, so the hand-over is listed under Sessions as `hermes` labelled with its model;
  folders of hand-overs that ended more than 7 days ago go at the next one,
  `prune_hermes_sessions`; `config.yaml` values are written as YAML double-quoted scalars with
  everything outside printable ASCII escaped) naming `owner agent:hermes` and a UUID
  `sessionId` Switchboard made — Observatory wants a UUID and Hermes's own ids are not one (seen in
  the rehearsal below: the session had to work both out) — and Terminal running `hermes chat
  --query-file <prompt> --provider openrouter -m <model>`; on a terminal that seeds an interactive
  session with the prompt as its first turn; the key is read from `switchboard agents key --service
  openrouter` when the session starts, never written. Spend is bounded by the key's own OpenRouter
  ceiling. Measured 2026-10-12 with Hermes 0.21.4: an empty
  `HERMES_HOME` starts without the setup wizard, and the session lists
  `observatory_handoff_accept`. `switchboard continue <wf> --account hermes` is the manual path.
  **Rehearsed live 2026-10-12** (a 0.6.19 build on its own data folder, the real engine and
  OpenRouter): workflow `wf_932609d34cf14593` with a Claude executor, offered after the engine's
  two-minute silence (`handoff:9c1cc21b63c64cc2`); the Hermes session accepted it within about 30 s
  (executor `hermes`), wrote its final checkpoint and closed the workflow — "Accepted by
  agent:hermes running model moonshotai/kimi-k2 via provider openrouter". Spend of the day's probes
  and the rehearsal together: $0.07 of the key's $10.
  `--mode managed` with Hermes or a Kimi account is refused (`MANAGED_NOT_FOR_AGENT`). Tests
  `a_hermes_step_runs_on_the_openrouter_key_after_the_subscriptions_run_out`,
  `a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key`,
  `a_hermes_that_hangs_is_cut_off_with_everything_it_started`,
  `hermes_config_escapes_every_value_into_plain_ascii`,
  `finished_hermes_hand_overs_go_after_a_week_and_nothing_else_does`. Other agents,
  a step pinned to a project's paid key, and Kimi Code on a paid key are skipped
  (`agent_not_automatic_yet`) until their launch recipes and ceilings exist (SB-72, SB-96, SB-73
  phase 2). A Hermes executor at its limit is not detected yet.
- **Evidence:** log event `fallback` with `offered`, `launch_failed`, `every_step_refused`,
  `no_usable_executor`, `no_checkout`, `engine_unreadable`, `group_ambiguous` or `no_chain`; store event `fallback`
  `offered` / `failed`. Never: stopping the old session, a `--force` handoff, a credential in the
  pack or the prompt.
- **The group's chain follows the checkout (0.7.0, C-8):** the group is chosen after the engine
  names the workflow's checkout (`fallback::chain_pool`): the executor account's group when it
  holds the checkout; else the one group holding the checkout, whoever's account worked there
  (plain `claude` in a group's folder runs on the ordinary sign-in, which stays in the default
  group); else, with several groups holding it (SB-146) and none of them the executor's, the
  group `Snapshot::project_for` names (the first holding group with an enabled account, an
  organization's group last when organizations exist; else the first), the one the folder's proxy
  route and paid key use, logged `group_ambiguous`; with no group holding it, the executor
  account's group as before. The engine is asked for the checkout whenever the workflow's, the
  executor group's or the machine's chain is set, or any group has a chain. When no chain applies
  to the chosen group the pass logs `no_chain` and leaves no retry mark, so a chain set meanwhile
  is used at the next scan; the 15-minute mark is set once a chain resolves, and also after
  `engine_unreadable` and `no_checkout`, which are attempts not repeated every minute. Tests
  `a_groups_chain_applies_to_a_workflow_in_its_folder_on_the_ordinary_cli`,
  `a_folder_in_several_groups_takes_the_group_its_proxy_route_uses_and_logs_ambiguous`,
  `no_chain_for_the_checkouts_group_is_logged_and_tried_again_at_the_next_scan`.

- **SB-129 (2026-10-14):** an executor that names no account (`lease.executor` without `accountRef`) is taken to be the ordinary CLI of its provider only while its workflow is being worked: the engine's `silentSeconds` is at most `UNNAMED_EXECUTOR_SILENCE` (3600). A workflow silent longer, or whose silence the engine does not report, is skipped (`unnamed_and_stale`; test `an_unnamed_executor_counts_only_while_its_workflow_is_worked`). An executor with an `accountRef` is decided by that account alone, as before.

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
- **A caller's context (SB-94, 2026-10-11).** A console hands its own context in `args`, as
  files, so every argument stays inside the limit — no `--context` option; the per-runtime mapping
  is the caller's. Claude Code's `--mcp-config <configs...>` is variadic: a plain value after a
  separate `--mcp-config <path>` is read as one more config file (measured on Claude Code
  2.1.296: `MCP config file not found: …/<the prompt>`). Switchboard therefore passes its own as
  one argument, `--mcp-config=<path>`, and a caller should do the same; repeated flags, joined or
  not, add up (both configs are read). Fabric Dashboards' shape (fabric-dashboards 47e5aa6,
  `src/core/handoff.ts`): Claude `--mcp-config=<consoles/<key>/mcp.json> --append-system-prompt
  <brief> [--continue] [<first prompt>]`; Codex `-c mcp_servers.fabric-dashboards.command=… -c
  …args=[…] -c …env.ELECTRON_RUN_AS_NODE="1" [resume --last] <first prompt>` — Codex gets
  Switchboard's server from its private `config.toml`, so the `-c` entries add to it. The session
  script unsets only the provider variables in `CONFLICTS`, so the caller's own environment
  (`FABRIC_DASHBOARDS_CONTEXT`, `FABRIC_DASHBOARDS_TASK`) reaches the agent. Test
  `a_dashboards_console_launch_reaches_the_agent_as_sent` runs the generated script with a
  recording agent and compares the exact argv and environment.

## Sign-ins across a restart (SB-65, 2026-10-11)

Code: `launch::LOGIN_JOURNAL`, `login_journal`, `adopt_logins`, `MAX_LOGINS`; `Runtime::adopt_logins`,
`pending_logins`; `Operation::PendingLogins`; Tauri `pending_logins`; window `restorePendingLogin`.

- `begin_login` writes `<logins>/<id>/.login.json` (0600: `provider`, `label`, `pool`, `started_at`)
  before Terminal opens.
- Every owner start, before the sweep, takes back the sign-ins whose journal is valid (a known
  provider, a valid label and pool) and that completed in Terminal (`.completed`), hold an
  unexpired reservation, or still run (`session_running`): newest first, at most four. Log
  `login_adopted` (count). Their ids are the folder names, so a window's `login_status` for them
  works again.
- The sweep keeps a completed sign-in with a valid journal (`Sweep::finishable`); one without a
  journal (an earlier version's) is removed as before.
- `Operation::PendingLogins` → `{logins: [{login_id, provider, label, pool, state}]}` (no
  credential; `[]` without an owner). The window asks once at start and shows the newest as its
  banner; the usual poll finishes it.
- Test: `a_sign_in_open_across_a_restart_is_taken_back_and_kept_from_the_sweep`.

## Forgotten sign-ins at start (SB-87, 2026-10-11)

Code: `launch::sweep_logins` (`sweep_logins_with` for tests), called once from
`Owner::start_with_sources` for a native owner, on a blocking thread off the start path.

- Considers only folders directly under `logins/` named by a UUID, never a link.
- Keeps a home whose sign-in is still open in Terminal (`.launch-pending` not expired, or
  `session_running`); a later start removes it once it ended — an earlier owner's sign-in cannot
  be finished by this one (its registry was in memory).
- Otherwise, on macOS, probes the derived Claude Code Keychain item by attributes only
  (`probe_external` with `/usr/bin/security`): absent → the home is removed; present and trusted
  by `/usr/bin/security` in an unlocked keychain → deleted silently, then the home; locked,
  unavailable or an access list that would ask → both kept. A failed delete keeps the home, so an
  item is never orphaned. Elsewhere the credentials live inside the home and go with it.
- Log: `login_leftovers` with `removed`, `running`, `kept` (numbers only), only when one is
  non-zero. Test: `a_start_removes_forgotten_sign_ins_and_keeps_what_still_runs_or_would_ask`.

## Session list (SB-95, 2026-10-11)

Code: `launch::sessions`, `launch::session_running` (factored out of `ensure_idle`, reads only),
`SESSION_RECORD`, `Operation::Sessions { all }`, CLI `sessions`, MCP `switchboard_sessions`.

- Every Claude Code or Codex launch (Terminal or in place, managed or isolated, continuation
  included) writes `<home>/.session.json` (0600) before the session starts: `account_id`,
  `provider`, `pool`, `mode`, `working_directory` (canonical), `in_place`, `launched_at`,
  `continues`. A failed write fails the launch before Terminal opens. Since 0.7.0 (SB-201) a
  Kimi Code launch or continuation, a Hermes or OpenCode continuation, an agent launched on the
  OpenRouter key (`runtimes/agent-<recipe>-openrouter`, `provider`, `agent` and `account_id` the
  recipe id, `pool` `openrouter`, `label` its model or null) and an agent launched through the
  proxy (`runtimes/agent-<agent>-<pool>`, `provider` `claude`: the provider the proxy route
  `/claude/<pool>` serves, so a reader comparing it with the route sees no false move; `agent` the
  catalog id, `account_id` its API-key account, `mode` `managed`) write one too, `context`
  included. `agent` is optional in the record: `Operation::Sessions` fills it for a record that
  names none from its provider (`claude` gives `claude-code`, `kimi` gives `kimi-code`; `codex`,
  `hermes` and `opencode` are their own ids; any other leaves it null), so the list has one field
  for the agent. Tests: `a_proxy_agent_launch_records_its_context` (goose and codex: `provider`
  `claude`), `an_openrouter_launch_records_its_session_and_context`,
  `the_session_list_names_an_organizations_context`,
  `the_session_list_names_a_proxy_agent_and_not_its_route` (the CLI line names the agent).
- **Context (SB-110, 0.6.80).** The record also carries `context`, and the session's environment
  `SWITCHBOARD_CONTEXT` (scrubbed from the inherited environment like `SWITCHBOARD_SESSION`):
  `Snapshot::session_context` in the contract's form (`personal`, `org:<id>`): the group of the
  account's pool, else the group holding the folder, else the account's own organization; a group
  and the account that disagree give `personal`; `personal` while no organization exists
  ([design](research/session-home-per-context-2026-10-24.md) H-1). `Operation::Sessions` adds
  `context_name`; Activity → *Sessions* shows the organization's badge beside the name (SCN-051).
  Tests: core `a_sessions_context_follows_its_group_else_its_account_and_the_more_private_wins`,
  `a_launch_records_its_session_and_the_list_follows_its_process`,
  `the_session_list_names_an_organizations_context`.
  SB-201 (0.7.0): the rule is `Snapshot::context_for(own, pool, folder)`, which
  `session_context` calls with the account's organization and pool. A session on no account
  takes `Snapshot::key_context`: the group's own OpenRouter key gives that group's context, the
  machine's key and a Kimi account (no organization, no pool) give `personal`, also in an
  organization's folder (the more private wins; a Kimi session there is launched, not refused).
  A proxy agent takes its account's `session_context`. Tests: core
  `a_session_without_an_account_takes_its_keys_group_else_personal`; runtime
  `a_launch_needs_a_signed_in_home_and_writes_its_script_outside_it`,
  `a_continuation_runs_its_first_turn_then_resumes_the_session_interactively`,
  `a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key`,
  `an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key`,
  `an_openrouter_launch_records_its_session_and_context`,
  `a_proxy_agent_launch_records_its_context`.
- The list scans `homes/` and `runtimes/` only, directory entries only (a link is skipped), and
  homes with a record. State comes from the same markers that keep one session per home:
  `.launch-pending` not expired → `starting`; `session_running` → `running` / `ended`; an
  unreadable marker → `unknown`. The owner answers it without its transaction lock (a read).
- The window reads it through the Tauri command `sessions` (`all: false`) and shows it on Activity → *Sessions* (SB-65, SCN-051), refreshed with the metadata (once a minute) and on Refresh.
- Not covered yet (packet SS-01, SB-65): active requests per session, and acting on a session
  (open, drain, close).
- Test: `a_launch_records_its_session_and_the_list_follows_its_process`; MCP
  `handshake_lists_tools_and_rejects_malformed_messages` (`switchboard_sessions`).

## Installed agents, switching and the default agent (SB-119/SB-120, 2026-10-14)

Design: [packet](packets/sb-119-120-agents.md) D-1..D-6; scenarios SCN-057..SCN-060. Code:
`crates/switchboard-runtime/src/agent_settings.rs`, `agent_catalog::SWITCHABLE`,
`launch::{opencode_config, opencode_continuation_session}`, `fallback::KimiChoice.{opencode_ready,
disabled}`, `Operation::{AgentsOverview, AgentEnable, AgentDefault, OpenrouterModels}`, CLI
`switchboard agents status|enable|disable|default|models`, MCP `switchboard_agents` (read), Tauri
`agents_overview`, `agent_enable`, `agent_default`, `openrouter_models`.

- **Installed** is `launch::find_program(binary)` for the catalog agent: `PATH`, `~/.local/bin`,
  `~/.cargo/bin`, `~/.bun/bin`, `~/.npm-global/bin`, Homebrew, then — after Homebrew, so a
  program found before keeps being launched — `~/.opencode/bin`, `~/.kimi-code/bin`,
  `~/.hermes/bin`, `~/.deno/bin`, `~/Library/pnpm`, `~/.yarn/bin`; version-manager shims are not
  searched (they resolve to the manager). On Windows a program under `%SystemRoot%` is never an
  agent (`cmd` is Command Code's name and cmd.exe's). It is computed per call (the Agents page opening, *Refresh*,
  `agents status`), never stored and never on a timer.
- **Switchable** agents are `SWITCHABLE` = `claude-code`, `codex`, `kimi-code`, `hermes`,
  `opencode`; the others are tools-only rows. `<data>/agents-settings.json` (0600, ≤ 4 KiB — the
  backup's per-setting limit) holds `{default: id | null, disabled: [id]}`; unknown fields are
  ignored, so a newer file still reads; absent means *Auto* and nothing disabled. It is a setting file in encrypted backups and in `switchboard uninstall`'s
  owned list. Unreadable → `Agent settings (agents-settings.json) could not be read…` and nothing is changed.
- `AgentEnable{agent, enabled}`: any catalog id (`No such agent in the catalog…` otherwise);
  disabling the default agent sets `default` to null. `AgentDefault{agent: Some(id)}` requires the
  agent to be switchable (`…cannot be the default.`), not disabled (`This agent is disabled for
  taking over work…`) and installed (`This agent is not installed…`); `None` is *Auto*.
- `AgentsOverview` answers `{schema: 1, revision, updated_at, default, default_ready, installed, agents: [{id, name,
  maker, url, install, installed, path, switchable, enabled, disabled, default, openrouter, mcp,
  command, argv}],
  recommended_models}`. `enabled` is `switchable && installed && !disabled`; `path` is the program found (null when not installed); `argv` is the
  argument vector and `command` the same joined for display: the program and the arguments of an OpenRouter launch with the key's default model, never a key or
  an environment value. `default_ready` says whether the saved default is installed, enabled and
  can take over work; it does not check that the OpenRouter key is saved or that an account has
  room now — `continue` checks those and refuses by name. `revision` (16 hex digits of a SHA-256 over the default, `default_ready` and the
  rows) changes whenever the answer does, so a reader can cache by it; `updated_at` is when the
  person last changed the settings (Unix seconds, null before the first change). Fabric reads
  this answer; `schema` changes only when a field changes meaning or goes. `id`s are the catalog's
  (`switchboard agents list`). The agent's own version is not read (it would start every agent's
  program); a reader that needs it runs `<path> --version` itself.
- **Fallback chains** read the settings once per pass; unreadable settings hand nothing over in
  that pass (`agent_settings_unreadable`), never "nothing disabled". They skip a disabled agent first, as `{agent, skipped: agent_disabled}`, before
  pins, keys or accounts are weighed. An `opencode` step runs like a `hermes` one (SB-107): on the
  saved OpenRouter key and its default model when OpenCode is installed, the key and model are
  saved and the `switchboard` command exists (`opencode_continuation_ready`, read only while a chain
  names the agent, like Hermes's); never when the
  executor is OpenCode itself; a step pinning an account or a key is `agent_not_automatic_yet`.
- **`Continue`**: an empty account id resolves to the default agent: `hermes` and `opencode` as
  themselves; `claude-code`, `codex` and `kimi-code` to the account a one-step chain of that agent
  would choose now (`fallback::account_for_default`: not limited, not needing a new sign-in,
  allowed in the folder — else `The default agent has no account that can take this workflow
  now…`); *Auto* → `Name the account or agent to continue on…`. An account or agent that is
  disabled is refused before the offer; unreadable settings refuse too. `opencode` with `--mode managed` is refused like Hermes.
  The OpenCode session lives in `runtimes/opencode-continue/<wf>/` (0700; finished ones pruned
  after a week like Hermes's), runs `opencode --prompt <handoff> -m openrouter/<model>` with
  `OPENCODE_CONFIG_CONTENT` = `{provider: {openrouter: {options: {apiKey:
  "{env:OPENROUTER_API_KEY}"}, models: {<model>: {}}}}, mcp: {observatory: {type: local, command:
  [...], enabled: true}}}` — OpenCode merges that layer last, so the session's key wins over one
  in `auth.json` and an id missing from OpenCode's model list is declared. The key reaches the
  session only through `switchboard agents key --service openrouter` when the script runs. The
  person's own `~/.config/opencode`, `auth.json` and sessions are never written. The prompt names
  `owner agent:opencode` and a UUID `sessionId`; the session is listed under Sessions with its
  model as the label.
- **`OpenrouterModels`** (`agents models`, the Tauri picker): `GET
  https://openrouter.ai/api/v1/models` (public, no key, 10 s timeout, body ≤ 8 MiB, read by chunks and dropped once over), kept 6 hours
  in memory, filtered to models whose `supported_parameters` contain `tools` and whose id passes
  `model_valid`, sorted by id: `{recommended, models: [{id, name, context, prompt, completion}],
  available}`. Any failure is `available: false` with the recommended models, never an error.
  `recommended` comes from `catalog/agents.json` `openrouter_recommended`.

## Agent onboarding (SB-93, 2026-10-11)

Code: `switchboard_runtime::onboarding` (`steps`, `registered`, `proof`, `record_proof`, `GUIDE`),
CLI `onboard` (`onboard_status`), MCP `switchboard_onboarding`. Scenario SCN-050.

- `switchboard onboard` prints `docs/AGENT-ONBOARDING.md` (`include_str!`, so the binary and the
  repository say the same); `--status` answers `{complete, host, folder, steps[]}`, steps in a
  fixed order `owner`, `accounts`, `project`, `mcp`, `proving_call`, states `done`, `todo`,
  `needs_person`, `optional`. `complete` is true when every step is `done` or `optional`
  (`project` is never required: a repository in no group uses the default group, and
  `next.command` is `project move-root --folder <f> --pool <group>` to put it in one (SB-146). No
  account IDs move. A provider with no enabled account in the folder's groups keeps its existing
  launch fallback; automatic `--provider` selection still requires the group's selected account).
- Reads only: the owner through `Operation::Status` on the control channel; accounts through
  `Snapshot` and the folder's project through `ResolveProject`, and only when the data folder
  exists (no storage is created); the agent's configuration by server name only — Claude Code
  `~/.claude.json` (`mcpServers`, `projects[<folder>].mcpServers`, read up to 32 MiB, a
  half-written file reads as absent), `<folder>/.mcp.json`, `~/.claude/settings.json`
  `enabledPlugins["switchboard@…"]`; Codex `config.toml` in `CODEX_HOME` or `~/.codex`.
- The proving call: `switchboard mcp` writes `<data>/mcp-proof.json` (0600, `{client, at}`; the
  client name from `initialize`, ASCII letters, digits, space, `._-`, at most 64) after its first
  answered tool call, once per server process, and only into a data folder that exists.
- Commands in `next` use the CLI's absolute path when `agent_cli()` finds it, shell-quoted.
- The organization checklist (SB-202, 0.7.0 node B-3, SCN-076): after the five steps, only on a
  computer with an organization, `organization` (done), then per organization, newest first,
  `org_group` (done when `Snapshot::organization_group` finds its one group; `todo` otherwise,
  with `next.command` the settings import and `next.alternatives` `project save` then
  `project edit --organization`, or no `next` when two groups are bound), `seat` (done when
  `organization_seats` holds an account that is enabled, not in `elsewhere` and not in the
  monitor's `sign_in_required`; else `needs_person` with `login begin --pool <group>`), `hook`
  (always `optional`; read from `switchboard shell status`'s `hook`, never the start files;
  `next.turn_off` names `shell off` when it is on, `next.command` `shell on` when it is off) and
  `launch` (done when a recorded session's context is `org:<id>`, else `optional` with a managed
  launch in the group's first folder). Each carries `organization: {id, name}`. With no
  organization and PassionCode.ai connected (`vesta.json`), one optional `organization` step;
  with neither, exactly the five steps. The CLI reads `MonitorStatus`, `Sessions {all}` and
  `shell status` only when an organization exists.
- Tests: `onboarding::tests` (9), `onboard_prints_the_guide_and_reads_the_setup_without_creating_storage`,
  `onboard_lists_an_organizations_group_seat_and_hook`,
  `onboarding_reports_the_steps_and_the_first_answered_call_proves_the_connection`.

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

## Updates to LC-16 (SB-77, 2026-10-07)

Code: `src-tauri/src/updates.rs` (`check_now`, `watch_for_idle`, `idle_moment`, `needs_migration`),
`Runtime::busy`, `ProxyHandle::in_flight`. Behaviour and events: [DISTRIBUTION → Automatic
updates](DISTRIBUTION.md#automatic-updates-sb-55).

- IPC `check_for_updates() -> UpdateStatus`: one check now, with the switch off too; refused with
  the copy's reason where updates are unavailable.
- `Runtime::busy() -> bool`: a managed request in flight (its slot is held until the response
  stream ends) or a sign-in waiting. An update activates on its own only when this is false and
  nobody is looking at the window (hidden, or open and out of focus for 30 minutes — SB-131, `updates::nobody_looking`, tested by `a_window_left_open_out_of_focus_stops_holding_an_update_back`; the idle restart relaunches it hidden), on two looks five minutes apart, and only while an update waits.
- Tests: `a_ready_update_starts_alone_only_on_two_idle_looks_in_a_row`,
  `a_release_that_needs_a_person_is_held`, `in_flight_counts_the_requests_holding_a_slot`.
  Refusal of tampered, unsigned or other-key packages is the updater plugin's minisign check with
  `requireSignedVersion` (DISTRIBUTION → Trust); no test here feeds it a forged package yet.

## A cut download continues where it stopped (SB-162, 2026-10-22)

The package is fetched into `<data>/updates/<version>-<FNV-1a of its URL>.part` (the folder is
private, 0700, and `switchboard uninstall` removes it). When a part is there, the next check asks
for the rest with `Range: bytes=<size>-`: a 206 appends, a 200 starts the file over, a 416 means
it is whole. The file is whole when the stream ended at the length the server gave (Content-Range
on a 206, Content-Length on a 200). Only then is it served once over `127.0.0.1` at a random path,
and the updater plugin's own `download()` reads it from there, so its signature and signed-version
checks run on the whole file exactly as before; a file that fails them is deleted. Parts of an
older release are deleted at the next start of a download. Log: `update_download` with
`started` or `resumed` and `from` (bytes already there), `partial` with `bytes` when an attempt
is cut, `done` with `bytes`. Measured cause: 2026-10-21 the 22 MB macOS package came at about
16 KB/s and every attempt was cut after 6 to 10 minutes, each starting again from byte 0. Tests:
`updates::tests::a_cut_download_resumes_where_it_stopped_and_completes`,
`the_loopback_answers_only_its_own_path_and_parts_are_kept_per_release`.

## A waiting update never waits forever (SB-105, 2026-10-12)

Code: `src-tauri/src/updates.rs` (`replaces`, `idle_blocker`, `IdleReport`, `recheck_done`,
`Updates::swap`), `Runtime::busy_reason`. Behaviour and events: [DISTRIBUTION → Automatic
updates](DISTRIBUTION.md#automatic-updates-sb-55).

- The check loop runs every 5 min (`CHECK_EVERY`, operator 2026-10-17), also while an update waits; a check that finds nothing new is logged once, not each time. Offered: newer than the running version and
  than the waiting one. A newer verified release replaces the waiting one; nothing newer, a failed
  check, download or signature, or a held release keeps it; a failed or stalled swap drops it.
- `Updates::swap` (an async mutex) is held across each bundle swap and across *Restart to update*
  until the restart is recorded. Waiters: the idle watch and both *Restart to update* entries
  (tray, About) reach `restart`, so all wait out a swap; a swap that finds a restart recorded
  installs nothing. Not covered, unchanged from SB-55: an ordinary Quit during a first or
  replacing swap (the drain does not wait for it) — the swap runs outside the quit path only by
  timing.
- `Runtime::busy_reason() -> Option<&'static str>`: `request_in_flight` or `sign_in_waiting`;
  `busy()` is `busy_reason().is_some()`.
- Log: `update_waiting {reason}` at the first look with a reason, then at most hourly;
  `update_check replaced` — or `deferred` when a restart into the waiting one is already recorded
  (the newer release waits for the next start); a check while an update waits adds `waiting=yes`
  to its outcome.
- Tests: `the_idle_watch_names_its_reason_exactly_when_it_is_not_idle`,
  `the_idle_watch_reports_its_reason_at_once_then_at_most_hourly` (a day on a fake clock),
  `a_newer_release_replaces_a_waiting_update_and_the_same_one_is_not_fetched_again`,
  `failed_login_cleanup_releases_the_slot_after_the_account_is_saved` (`busy_reason`),
  `a_sign_in_home_from_before_its_journal_is_swept_once_its_terminal_ended`. The second and
  third fail against planted defects (a five-minute report interval; the waiting version ignored).
  Not tested: the replacement and the swap guard end to end — they need a running app and a
  signed feed; the release smoke check does not cover them.

## Hermes's model and provider (SB-80, 2026-10-08)

Design: [XA-02](packets/agent-keys-and-kimi.md) A-4; facts:
[research](research/kimi-hermes-openrouter-2026-10-07.md) Part 2. Code:
`crates/switchboard-runtime/src/hermes.rs`, `Operation::{HermesModel, HermesSetModel {provider?,
model?}}`, CLI `switchboard agents hermes [status|set --provider --model]`, MCP
`switchboard_hermes_status` (read) and `switchboard_hermes_set` (write).

- Read: `hermes config get model --json` (15 s deadline, killed and reaped, stdout only, at most
  64 KiB, no shell). Only `default` (alias `model`), `provider`, `base_url` (scheme, host, port
  and path — no query or credentials) and `api_mode` are kept; Hermes masks secret-shaped values
  and Switchboard drops them anyway. An empty home prints `""`: `configured: false`.
- Change: the one write to another agent's config, only on the person's request, through
  Hermes's own `hermes config set model.provider <id>` (first — Hermes then clears a `base_url`
  that belonged to the previous provider, measured on a scratch `HERMES_HOME`) and
  `model.default <id>`, then the status is read back. Hermes accepts any string, so Switchboard
  checks the shapes first: a provider `[a-z0-9][a-z0-9._:-]{0,39}`, a model
  `[A-Za-z0-9][A-Za-z0-9/._:-]{0,119}`. `COMMON_PROVIDERS` is the app's list; any id of the shape
  is accepted, since Hermes adds providers between releases. A key for the new provider is
  Hermes's own business; Switchboard never sets one.

## Kimi Code accounts (SB-81, 2026-10-08)

Design: [XA-02](packets/agent-keys-and-kimi.md) A-5..A-7; facts:
[research](research/kimi-hermes-openrouter-2026-10-07.md) Part 1. Code:
`crates/switchboard-core/src/kimi_accounts.rs`, `crates/switchboard-runtime/src/kimi.rs`,
`Operation::{KimiAccounts, KimiLoginBegin, KimiLoginStatus, KimiLoginFinish, KimiLoginCancel,
KimiSignInAgain, KimiLaunch, KimiRename, KimiRemove}`, CLI `switchboard kimi
list|login|finish|cancel|relogin|launch|rename|remove`, MCP `switchboard_kimi_accounts` (read).

- `Snapshot.kimi_accounts: Vec<KimiAccount {id, label, region, added_at, nickname?, tier?}>`
  (omitted when empty) — metadata only. `region` is `mainland-cn` or `global`; `nickname` and
  `tier` (`user_level_name`) are cut to 80 characters without control characters. Validation:
  unique UUIDs no account or agent key uses, a valid label, a known region, a positive time.
  Events `kimi_account` `added` / `removed`.
- The credential lives only in the account's own `KIMI_CODE_HOME`, `<data>/kimi/<id>/`, written
  and renewed only by the official `kimi`: never in the vault, a backup or another home, never
  renewed by Switchboard (Kimi's refresh tokens rotate). Encrypted backups do not carry Kimi
  accounts; after a reinstall each is signed in again.
- Sign-in: `KimiLoginBegin` makes the home (0700) with a `.kimi-login` marker and runs
  `kimi login --region <region>` in Terminal (the device code shows there and in the browser);
  the script marks `.completed` on success. `KimiLoginStatus` answers `pending | complete |
  ended`. `KimiLoginFinish` needs `.completed` and a non-empty access token in
  `credentials/kimi-code.json` (mainland) or the newest `credentials/kimi-code-env-*.json`
  (global), asks `GET {base}/me` for the nickname and tier (a failure saves the account without
  them) and names it by the label, else the nickname. `KimiLoginCancel` removes an unsaved home
  once its Terminal closed. A failed Terminal start removes the home at once.
- Status (`KimiAccounts`): for each account and for the ordinary home (`KIMI_CODE_HOME`, else
  `~/.kimi-code`, shown and never written — A-6) the token is read in-process while it is valid
  (`expires_at` more than 60 s ahead) and sent only to `GET {base}/usages` and `GET {base}/me`
  (8 s timeout, redirects off, an honest `FabricSwitchboard/<version>` User-Agent). `base` is
  pinned per region — `https://api.kimi.com/coding/v1`, `https://api.kimi.ai/coding/v1` — and a
  test overrides it only with `http://127.0.0.1:<port>` (`SWITCHBOARD_KIMI_BASE`). Windows
  `5h`, `7d`, `month`, `month_code` from `usages.limit_*` (`used_ratio` as a number or a string,
  clamped to 0..1; `reset_time` RFC 3339). Without a valid token the status names why
  (`KIMI_SIGNED_OUT`, `KIMI_TOKEN_STALE` — Kimi renews it when it next runs); 401/403 is
  `KIMI_REFUSED`; anything else `KIMI_UNREACHABLE`. A newer nickname or tier is saved.
- `KimiLaunch` runs the official `kimi` with `KIMI_CODE_HOME` in a folder (Terminal); its script
  lives in `<data>/kimi/sessions/<id>/`, outside the home. `KimiSignInAgain` reruns `kimi login`
  in the same home. `KimiRemove` forgets the account, then deletes its home.
- The token is never handed to another agent or the proxy (A-7); chains name Kimi Code by agent
  (`kimi-code`), and handing a workflow to a saved Kimi account is SB-73 phase 2.

- **Status quality (SB-100, 2026-10-12).** Kimi's answers read as what they are: 404 → “Kimi
  offers no usage figures for this account.”, 429 → “Kimi is limiting requests right now.
  Switchboard asks again shortly.”, an unreadable 200 → “Kimi answered with something Switchboard
  cannot read. Try again later.”, a 200 without figures → “Kimi returned no usage figures for this
  account.”; 5xx and network failures stay “Kimi did not answer…”. One HTTP client serves every
  request; `/me` and `/usages` are asked together and all accounts at once (`JoinSet`, order
  kept); a saved account is asked in the region its own folder names. The window shows “checked
  {time} ago”, says a failed refresh while keeping the last figures, refreshes Kimi in the
  background every five minutes, and gives the ordinary `kimi` its own words (“Not signed in. Run
  kimi login in Terminal.”); `switchboard kimi` no longer prints “signed in” for a signed-out
  ordinary `kimi`. Tests `each_kimi_answer_reads_as_what_it_is`, `ordinaryKimiNote`.
- **Safe homes and limits (SB-99, 2026-10-11).** *Launch* and *Sign in again* reserve the
  account's session folder `kimi/sessions/<id>/` (`Reservation`): a second `kimi` or a `kimi login`
  beside a running one is refused with “A Kimi Code session still runs on this account. Close it in
  Terminal first.”, and *Remove* is refused while it runs. A launch writes the session record, so
  `switchboard sessions` and Activity → *Sessions* list Kimi sessions (`provider: "kimi"`). In the
  window a plan window at 100 % marks the row as limited (“Limit reached · resets in {time}”, red
  dot) until its reset, and a window whose reset has passed reads “reset, refresh to see it”
  instead of its old share. Tests: `a_launch_needs_a_signed_in_home_and_writes_its_script_outside_it`
  (extended), `kimiLimitUntil` / `kimiWindowsLine` in `scripts/test-ui-logic.mjs`. Not yet: Kimi in
  automatic fallback chains (SB-99 left, with SB-73 phase 2) — done below.
- **Waiting Kimi sign-ins (SB-99, 2026-10-12).** At most four Kimi sign-ins wait at once
  (`launch::MAX_LOGINS`, as for Claude and Codex): open in Terminal, or completed and not yet
  saved; a fifth is refused with “Finish an existing sign-in before starting another.”. An owner's
  start (`kimi::sweep_logins`, beside SB-87's sweep, real data folder only) removes the
  `<data>/kimi/<id>/` homes whose `kimi login` ended without signing in — never a saved account,
  one still open in Terminal, one that completed (`switchboard kimi finish <id>` still saves it),
  or a folder younger than a minute; log `kimi_login_leftovers {removed}`. Test
  `waiting_sign_ins_are_capped_and_a_start_sweeps_only_the_ended_ones` (fails with the ended-state
  check removed).

- **Kimi Code in fallback chains (SB-99, 2026-10-12; operator decisions the same day).**
  - *Background view.* While a fallback chain is set, the monitor asks each saved Kimi account's
    `GET /usages` every 10 minutes (`WATCH_SECONDS`), at most 4 a pass (`WATCH_PER_PASS`): the
    access token read from the account's own home in-process, never `/me`, never a refresh. A
    window at 100 % whose reset has not passed limits the account until the latest such reset — the
    window's rule (`kimiLimitUntil`), here `kimi::limited_until`; a full window with no reset time,
    until the next look. In memory only (`KimiWatch`). No chain: no file read, nothing sent.
  - *A Kimi executor at its limit.* A workflow whose executor is Kimi Code (`kimi`, `kimi-code`,
    `moonshot`) and whose `accountRef` names a saved Kimi account the view finds limited is handed
    on along its chain (`fallback::kimi_executor_limited`); the ordinary `kimi` is not watched and
    decides nothing.
  - *A Kimi step.* `kimi-code` with no pin, or pinned (`@ID`) to a saved Kimi Code account, runs on
    the pinned one when usable, else on the usable one with the lowest window use; usable = not the
    executor's, not limited, signed in, no `kimi` running in its folder
    (`continuation_preflight`). Only Kimi Code may pin a Kimi account (“A Kimi Code account runs
    only Kimi Code.”); removing the account drops the step that pinned it.
  - *The continuation* (`Operation::Continue` on a Kimi account id; also `switchboard continue`):
    the engine is read and planned with the harness name `kimi` (`continuation::plan_named`), the
    account is checked, then the offer; after it, Terminal runs `kimi -p <first prompt>` — Kimi
    2.1.1 asks for no approval in prompt mode (measured 2026-10-12: a shell command ran without a
    question) — and then `exec kimi -c`, the same session, interactive, in the same folder
    (`launch::script_then`). Project Observatory is added to the account home's own `mcp.json`
    (`mcpServers.observatory`; other servers kept; a file that is not a JSON object is never
    overwritten — “This account's Kimi MCP settings (mcp.json) cannot be read…”). The session
    record carries `continues`.
  - Tests: `a_kimi_account_takes_a_workflow_by_its_harness_name`,
    `kimi_code_pins_a_saved_kimi_account_and_loses_the_pin_when_it_goes`,
    `a_kimi_step_runs_on_a_usable_kimi_account_pinned_or_least_used`,
    `a_kimi_executor_is_limited_only_by_its_named_saved_account`,
    `a_full_window_limits_until_its_latest_reset_as_the_window_shows_it`,
    `the_background_view_asks_each_account_every_ten_minutes_four_at_a_time` (an hour on a fake
    clock), `the_background_view_runs_only_while_a_chain_is_set`,
    `observatory_joins_the_accounts_mcp_servers_and_an_unreadable_file_is_left_alone`,
    `a_continuation_runs_its_first_turn_then_resumes_the_session_interactively`. Watched failing:
    the chain gate removed; the most-used account chosen. Not done: a live run against a real
    workflow and a signed-in Kimi account; `kimi -c` resuming the `-p` session is read from
    `kimi --help`, not measured.

## Agent keys (SB-79, slice 1, 2026-10-07)

Design: [XA-02](packets/agent-keys-and-kimi.md). Code: `crates/switchboard-core/src/agent_keys.rs`,
`crates/switchboard-runtime/src/agent_keys.rs`, `backup.rs` (`AgentKeyCopy`),
`Operation::{AgentKeySet, AgentKeyStatus, AgentKeyModel, AgentKeyRemove, AgentKeyValue}`, CLI
`switchboard agents openrouter set|status|model|remove` and `agents key --service openrouter`,
MCP `switchboard_openrouter_status` (read) and `switchboard_openrouter_model` (write).

- `Snapshot.agent_keys: Vec<AgentKey>` (omitted when empty); `AgentKey {service, id, saved_at,
  model?}` — metadata only; the value lives in the vault under `id`, like every credential.
  `service` is today only `openrouter`; the value must start `sk-or-`; a model is an OpenRouter id
  (`vendor/model`, letters, digits and `/ . _ : -`, at most 120).
- Validation on every publish (`validate_agent_keys`): one entry per service, a unique UUID that
  no account uses, a positive `saved_at`, a valid model.
- `Store::set_agent_key(service, key, model)`: replaces in place (same vault item, the model kept
  when not given); a metadata failure puts the previous value back, so a failed save leaves
  nothing behind. `set_agent_key_model`, `agent_key` (metadata), `agent_key_value` (the value —
  a launched agent's environment and `agents key` only, never the window or MCP),
  `remove_agent_key` (metadata first, then the vault item). Events `agent_key` `saved` /
  `removed`.
- Status (`AgentKeyStatus {service, credit}`) asks OpenRouter itself what the key may still spend:
  `GET {base}/key` with a 10 s timeout, redirects off, the base pinned to
  `https://openrouter.ai/api/v1` (a test overrides it through `SWITCHBOARD_OPENROUTER_BASE`, which
  accepts only `http://127.0.0.1:<port>`). The answer carries the label (control characters
  dropped, at most 80), `limit`, `limit_remaining`, `limit_reset`, `usage`, `usage_daily`,
  `is_free_tier` — never the key. A 401/403 is `KEY_REFUSED` (the key stays saved); anything else
  is `UNREACHABLE`.
- Encrypted backups carry the key with its value (`AgentKeyCopy`, sealed like every credential);
  a restore puts it back where the install has none and counts it in `Restored.agent_keys`.
- Launching on the key (SB-79b, 2026-10-08) — `crates/switchboard-runtime/src/agent_catalog.rs`
  (`OpenrouterRecipe`, `openrouter`), `launch.rs` (`launch_agent_openrouter`, `script_with`),
  `Operation::LaunchAgent {agent, pool, working_directory, via, model}`:
  - The catalog's `openrouter` object is the recipe: `key_env`, `base_env` + `base_url` (an
    `https://openrouter.ai/` URL, Kimi Code and Qwen Code only), the model as `model_flag` (argv
    entries with `{model}`) or `model_env`, never both; fixed `args`, `extra_env`, `isolate_env`,
    `default` (the app offers the key as the agent's first launch; `false` for Kimi Code, which
    runs on its subscription by default — operator, 2026-10-08) and `notes`. Ten agents carry one (Hermes, Kilo, omp, pi, Kimi Code, Aider, OpenCode, Goose,
    Qwen Code, Crush). OpenClaw and Cline carry none: OpenClaw's interactive sessions run in its
    Gateway daemon, which never sees a launch's environment; Cline's TUI sends a fresh install to
    onboarding. Any other id is refused with "This agent cannot launch on the OpenRouter key…".
  - `via: "openrouter"` needs no owner, proxy, pool or account. The model is the launch's
    (`model_valid`) or the key's default; an agent that takes a model and has neither is refused
    (`OPENROUTER_MODEL_NEEDED`); Crush picks its model in its own TUI (`model_choice: "agent"`).
  - The session lives in `runtimes/agent-<id>-openrouter/` (0700); `isolate_env` points at its
    own `agent-home/` there, so a login the agent saved itself cannot win over the key.
  - The key never touches disk (XA-02 A-2): the script clears the conflicting variables, exports
    the recipe's plain variables, then runs `<cli> --data-dir <root> agents key --service
    openrouter` and exports its output under `key_env`; a failed or empty answer prints
    `KEY_UNREADABLE` and exits before the agent starts. PowerShell does the same with
    `$LASTEXITCODE`. `a_session_script_hands_the_key_to_the_agent_and_stops_without_it` runs the
    zsh script with a stand-in CLI and agent.
  - The desktop window gets `openrouter_status`, `openrouter_save`, `openrouter_model`,
    `openrouter_remove` and `launch_agent {via, model}` — never `AgentKeyValue`.

## Capacity notices and console hand-overs (SB-114, SB-108)

[Recovery packet](packets/sb-114-capacity-recovery.md) owns the host protocol;
[CLI](CLI.md#capacity-recovery-sb-114) names the commands. `Operation::Recovery*`
is owner-only, except `RecoveryContinued`, which with no owner running settles under the
store's own lock. Its host writes (register, claim, finish) run under the owner mutation lock;
the status read `Recovery` and the session hooks (`RecoveryHook`, `RecoveryContinued`) do
not, and take only the recovery file's own lock (`recovery::writing`), so a background step
holding the owner lock never delays a host (SB-166: the file is replaced by rename, the limits
are read from memory; `recovery_answers_while_a_background_step_holds_the_transaction`). A
host-reported stop keeps provider, account, folder and event/session binding in
`<data>/recovery.json` (0600); readiness uses the account's current group (SB-205, below).
Candidates use post-stop successful observations at most 300 seconds old; a passed
reset invalidates the observation. Hidden holds, sign-in requirements, disabled
accounts, API keys, other pools/providers and project-folder refusals are excluded.
A duplicate row of a held identity is excluded too. Claim rechecks and validates
the credential before persisting `claimed`; it never returns a credential. An
ambiguous claim survives restart without automatic replay. Completion is the
host's report or the session's own `Stop` hook (SB-204, below), not independent provider
acceptance. Evidence: `recovery::tests`.

**Retention (PRD-03, D-4 of iteration 2026-10-24).** The file keeps stops of the last 7 days and
at most 512 rows. A stop older than 7 days is left out of every answer at once (a read writes
nothing) and out of the file at the next write (register, the hook, claim); past 512 rows the
oldest finished stops (resumed, cancelled) go before the oldest live ones, and the stop being
written (registered, stopped again, claimed or finished) is always kept, whatever the rows' dates:
a full file no longer refuses a stop as "invalid or conflicts with an existing stop". Every write,
`finish` included, prunes. A write that dropped rows logs `recovery_pruned` with `dropped` and
adds the events to `<data>/recovery-pruned.json` (0600, ids only, the newest 2048; unreadable
reads as empty; removed by uninstall): `register` of a pruned event answers as a replay and stores
nothing, `finish` of one is a no-op, `claim` of one answers not ready. A file of up to 1024 rows
written by an earlier version still reads. Evidence:
`stops_older_than_a_week_are_dropped_at_read_and_at_the_next_write`,
`a_full_file_keeps_a_new_stop_and_drops_finished_ones_first`,
`the_stop_just_written_is_kept_however_the_rows_are_dated`, `finishing_a_stop_prunes_the_file_too`,
`a_pruned_event_replayed_is_not_a_new_stop`.

**Stops nobody resumes through a claim are settled (0.6.83).**
- *The session went on by itself (SB-204).* The plugin's `Stop` hook runs after every finished
  turn of a Claude Code session (`StopFailure` runs instead when a turn ends on an API error).
  `switchboard recovery hook` reads `hook_event_name: Stop`, checks the file without the owner
  (`recovery::session_has_live_stop`), and only when the session has a waiting or claimed stop
  asks the owner (`Operation::RecoveryContinued`, at most 1.5 s, like the start hook), which
  marks it `resumed` and logs `recovery_continued` with `settled`; with no owner running the
  CLI settles it under the store's lock. An unreadable file asks nothing. The input is read up
  to 8 MiB, since a Stop input carries the turn's whole last message. Its stdout is `{}`:
  Claude Code reads a Stop hook's output as a decision. Evidence:
  `a_session_that_goes_on_by_itself_settles_its_stop`,
  `the_stop_hook_after_a_turn_is_silent_and_never_fails`,
  `a_long_last_message_still_settles_the_stop_with_no_owner_running`,
  `the_owner_settles_a_stop_its_session_continued`.
- *The hosts' horizon (SB-199).* A waiting or claimed stop older than one day reads as
  `cancelled`, and is written so at the next write; the 7-day retention above is unchanged.
  Evidence: `a_stop_past_the_hosts_horizon_reads_and_is_written_as_cancelled`.
- *A host's report on a settled stop.* `finish` of a stop already `resumed` or `cancelled`, by
  the host itself, the `Stop` hook or the horizon, answers success and changes
  nothing. Evidence: `a_host_finishing_a_stop_settled_meanwhile_is_answered`.
- *Not settled by a continuation (SB-196, decided 2026-10-25).* A workflow continued in a stop's
  folder leaves the stop as it is: an Observatory checkpoint names no session, so Switchboard
  cannot tell which stop the continuation replaces, and cancelling every stop of the folder
  would drop other sessions' safe resumes. The host holds a stop while a session runs in its
  folder (Fabric Dashboards), and the continued session's own `Stop` hook settles nothing
  that is not its own.

**A stop follows its account (SB-205).** Readiness uses the stopped account's current group, so an
account moved to another group after the stop keeps the stop ready or probed among that group's
accounts. A waiting stop that is not ready carries `reason`: `billing`, `account_removed`,
`no_account` (none may be probed) or `backoff` (with `probe_at`). Evidence:
`a_stop_follows_its_account_into_another_group_and_says_why_it_waits`.

**A renewal is stored without holding the account list (H-1, 0.6.83).** `adopt_refreshed_for`
reads the saved OAuth accounts' items without the store's lock (a Keychain read has no deadline
and every read in the process waits behind one module lock), then, per account still holding
the spent token, takes the lock, re-reads that one item, and writes only if the account is
still saved and still holds that token: a removal or a sign-in meanwhile keeps its place
(SB-179). Evidence: `adopting_a_renewal_never_holds_the_account_list_across_keychain_reads`,
`a_renewal_never_writes_an_account_removed_while_its_reads_ran`,
`a_sign_in_stored_while_a_renewal_reads_keeps_its_place`,
`a_renewal_writes_under_the_lock_so_a_removal_waits_for_it`.

**Sign-ins needed (SB-182, PRD-01).** `recovery`, `RecoveryClaim` and `MonitorStatus` read the
accounts whose rejected lineage nothing replaced (`RefreshState::sign_in_required`): each such
account's stored credential, from the vault, off the async worker (`blocking::run`), never under
the store's metadata lock (SB-185). The answer is cached until a credential is written to or
removed from the vault (`Store::credential_changes`, counted by the store's vault wrapper) or a
lineage is rejected; a project save, a policy or a quota change keeps it (before 0.6.82 the
cache was keyed on `Store::changes`, which ~740 folder joins a day moved, and `recovery` still
took 5 to 30 s). An answer is cached only when every read succeeded or found no account; a read
that failed (a `security` timeout, a locked keychain) is not cached, and the account keeps the
answer it had for that same rejected lineage, or, with no such answer, is reported as needing a
sign-in, so an error never hides a needed sign-in. The token holder map
(`RefreshState::holder`) and the renewal schedule (`monitor::renew_due`) are keyed the same way,
on `refresh::credentials_key` (credential writes, and each account's id, provider, kind, enabled
flag and identity), not on `Store::changes`, and follow the same failed-read rule: each is kept
under its key only when every read succeeded or found no account, so a failed read is read again
at the next use instead of leaving the holder unknown or the account out of the renewal schedule
until a credential is written (the schedule keeps the account's earlier due time meanwhile). A
quota check's answer or failure is kept only for the
credential the provider answered: that credential is read before the metadata lock and compared
under it, read again only if a credential was written meanwhile. Evidence:
`rejected_sign_ins_are_read_from_the_vault_only_after_a_change`,
`a_folder_joining_its_group_does_not_cost_a_vault_read`,
`a_failed_vault_read_is_not_cached_and_does_not_hide_a_needed_sign_in`,
`a_failed_first_read_does_not_hide_a_new_rejection`,
`a_failed_read_does_not_stick_in_the_token_holders`,
`a_failed_read_does_not_stick_in_the_renewal_schedule`,
`the_credentials_key_names_each_accounts_identity_and_state`,
`the_token_holders_follow_each_accounts_identity`,
`a_project_save_does_not_rebuild_the_token_holders`,
`a_project_save_does_not_reread_the_renewal_schedule`, core
`only_credential_writes_move_the_credential_count`,
`a_quota_checks_generation_read_does_not_hold_the_metadata`,
`a_credential_written_after_the_generation_read_is_still_seen`.

**A seat the provider ended (SB-160 leaving, iteration 0.7.0 node B-5).** `sign_in_required` and
`RefreshState::is_dead` also name an account whose stored credential a quota check found rejected:
a Claude usage 401 that survives a fresh renewal (`monitor::check` renews once after a 401 and
probes again; a 401 on the renewed token is the verdict), or, for Codex (never renewed while
inactive), a 401 on two checks in a row on the same credential, the second at least
`refresh::REJECTION_SPACING` (60 s) after the first. Only the provider's own 401 counts: a token
that expired on this machine is refused before the network and says nothing; a Codex 401 within
`monitor::CODEX_EXPIRY_SKEW` (300 s) of the token's local expiry may be a clock running ahead here
and is no verdict; a 403, a 429, a 5xx, an unreachable provider or any answer between two Codex
401s is no verdict and breaks the row; the account Claude Code is signed in to is never renewed
here, so its 401 never is one, and the account the ordinary Codex CLI is signed in to (the monitor's
in-use set, `Store::is_in_use`) is exempt the same way. The verdict is
the fingerprint of the whole stored credential (access and refresh token) in memory
(`RefreshState::rejected`), never in the renewal journal: any new credential (a sign-in, a
renewal by Claude Code or Claude Swap) clears it, and a restart forgets it, so a provider's mistake
costs one run at most and the next check finds a real removal again. Until then the account is
handled as a dead lineage: no quota check reaches the provider (the account leaves the due queue
for 30 minutes and its check answers the sign-in message), no renewal is granted, native
activation refuses it, and native rotation, fallback chains and recovery skip it, as they skip
every account in `sign_in_required`. An ended Claude seat leaves the renewal schedule until its
credential or a verdict changes (`RefreshState::verdict_generation` is part of the schedule's key),
so its due token is not read every minute. A quota check a person asks for (the owner's `Usage`
operation and the offline CLI's, `monitor::check_by_person`; never the background pass) asks the
provider once past a usage verdict, without a renewal: a 401 keeps the verdict, any other answer
takes it back (`RefreshState::lift_verdict`); a lineage the token endpoint refused is not asked
past. Nothing moves a session or replays a request (B-Q3). Known limit: the proxy's own failover
(SB-98's 429 move and SB-101's unusable-route failover) does not read this set, so a request on a
group's route can still be moved to an ended seat (board row SB-211). An ended seat that is a
group's selected route stops renewing, so its token runs out and the route falls to the recorded
SB-101 failover for an expired token: the pool moves to its next usable account, and a group with
no other usable seat answers the session 409 ("Select a usable account in this pool first.").
The Codex row's 60 s spacing is measured in wall-clock time (`monitor::now`), not a monotonic
clock: a forward jump of the machine's clock between two Codex 401s can shorten it, so two checks
less than a minute apart may give a verdict; a person's check (`monitor::check_by_person`) takes a
wrong verdict back on the provider's next answer that is not a 401.
Both an `invalid_grant` (also from `refresh::renew_idle_live`, once per lineage) and this verdict
log `seat_ended {provider, by: renewal|usage, organization: true|false}` once. Evidence:
`monitor::tests::seat::*`
(`a_claude_seat_rejected_after_a_fresh_renewal_reads_sign_in_required`,
`a_codex_seat_rejected_twice_reads_sign_in_required`, `a_single_codex_401_is_not_a_verdict`,
`two_quick_codex_401s_are_not_two_scheduled_checks`, `a_403_is_never_a_verdict`,
`a_new_credential_clears_the_rejection`, `a_transient_401_or_a_5xx_never_ends_a_claude_seat`,
`an_expired_codex_token_is_not_a_removed_seat`,
`the_account_claude_code_uses_is_never_marked_ended`,
`a_person_check_lifts_a_verdict_the_provider_takes_back`,
`a_person_check_answered_401_keeps_the_verdict`,
`a_codex_401_near_its_local_expiry_is_no_verdict`,
`the_account_codex_uses_is_never_marked_ended`),
`monitor::tests::an_ended_seat_leaves_the_renewal_schedule_until_its_credential_changes`,
`monitor::tests::a_verdict_after_the_schedule_was_built_takes_the_seat_out`,
`usage_gate_tests::a_persons_usage_operation_lifts_a_verdict_through_the_owner_and_the_offline_arm`,
`refresh::tests::invalid_grant_logs_the_seat_as_ended_by_renewal`,
`refresh::tests::an_idle_live_lineage_rejected_by_the_provider_is_dead`,
`oplog::tests::a_flag_is_written_as_a_json_boolean`.

The hand-over queue is `<data>/handovers/`, records 0600, at most 16 pending before
the monitor starts overflow in Terminal; recording refuses at 256 directory entries
(including permanent `.taken` receipts). Individual records are at most 16 KiB.
Exclusive private-file creation chooses one taker on every platform; the receipt remains
after completion or a crash and prevents republication. A pending record with a receipt
is not offered again. Expired/unparseable deadlines never launch.
`fabric-handovers` absent means off. Pending/taken records and recovery state are
operational state, excluded from account backups and removed by uninstall. A queue
record never contains a prompt or credential. Evidence: `handovers::tests`,
`uninstall::OWNED`, CLI command test.

SB-114 retains expired limit evidence for the five-minute quota freshness window.
`limited_accounts` counts active holds; `awaiting_post_limit_probe_accounts` counts
ended holds without a newer quota observation. Both are excluded from recovery
candidates (`limit_unavailable_accounts`). Ordinary routing's existing hold deadline is
unchanged. `recovery::tests::a_probe_during_a_hidden_hold_cannot_wake_work_after_its_deadline`
checks this across restart; `simultaneous_host_claims_are_serialized_by_the_owner`
checks competing runtime callers.

SB-115 (operator decision 2026-10-13, hybrid; [packet](packets/sb-115-limit-stop-resume.md)).
A stop also becomes ready as a `probe` once `stopped_at + [120, 300, 600, 900][min(attempt, 3)]`
has passed, on any compatible account (the same provider, pool and folder permission),
enabled and OAuth, that has no needed sign-in and no active spend-cap hold (`Limit.reason`). Fresh
quota is not required, and the reset is not consulted. Billing stops (`kind: billing`)
are never probed. `recovery::readiness` is the one rule that both `status` and `claim`
read. New record fields `kind`, `attempt` and `mode` default for SB-114 files.
`Operation::RecoveryHook` registers a session's own stop: the account is
`LimitState::bound(session)`, else the ordinary Claude Code's. One live stop per session
holds: a waiting stop moves to now with `attempt + 1`; a claimed one becomes `resumed` and
a new stop follows; a stop within 1800 s of the session's last resumed stop is one attempt
later. Evidence: `recovery::tests`, CLI `recovery_hook_never_fails_the_session`.

## The terminal hook's folder map (SB-183, node A-1, 2026-10-26)

Code: `switchboard_runtime::shell::map` (`read`, `write`, `Map::lookup`, `answer`, `env_line`,
`status`), CLI `shell status` and `shell env` (`crates/switchboard-cli/src/shell.rs`). Design:
[dossier A](runs/2026-10-26-iteration-0.7.0/dossier-a-context-binding.md) part A; scenario SCN-074.

- **Map.** `<data>/shell/map.json`, 0600, at most 1 MiB, written in one rename by `map::write`
  (the owner's bindings pass calls it from node A-4):
  `{"version": 1, "rows": [{"folder", "pool", "context", "claude", "codex"}]}`. `folder` is an
  absolute group folder (its subfolders and worktrees belong to it); `context` is `org:<id>`;
  `claude` is true when the folder's Claude Code is bound to the group, `codex` when the pool has
  an enabled Codex account. Folders, pools and organization ids only: never a capability or a
  token. Shell homes are `<data>/shell/claude-<pool>` and `<data>/shell/codex-<pool>`.
- **One row per folder.** Which group a folder belongs to is decided once, by the writer (node
  A-4), never by the lookup: a folder gets a row only when the group `Snapshot::project_for`
  chooses for it (`crates/switchboard-core/src/projects.rs:297`) is bound to an organization, and
  no row when that group is personal; `folder` is written in `canonicalize()` form.
  `map::write` refuses two rows that name the same folder, compared component by component, with
  “Two rows of the terminal folder map name the same folder.” and writes nothing. Test:
  `shell::map::tests::the_map_holds_one_row_per_folder`.
- **Lookup.** The longest row folder that holds the folder wins, compared whole component by
  whole component, the folder matched with its links resolved: a link inside a group's folder
  that leads out of it (`/w/acme/out -> /w/mine`) is the folder it leads to and answers nothing
  of the group's, and a link from outside that leads into a group's folder answers that group.
  Only a folder that cannot be resolved (it does not exist or cannot be read) is matched as
  given. A row is skipped when its folder is not absolute, its pool is not `[A-Za-z0-9._-]` (or
  starts with `.`), or its pool or context is empty, longer than 256 characters or holds a space
  or a control character. A folder with a `..` step is never matched (`/w/acme/../mine` is not
  inside `/w/acme`). Of two rows of the same length the earlier one wins, every time (a map with
  two rows for one folder, written by hand or by an older writer, answers its first); a row that
  names its folder through a link holds no folder that resolves. Tests:
  `shell::map::tests::a_link_out_of_a_group_folder_never_answers_its_homes`,
  `shell::map::tests::a_folder_with_a_parent_step_is_never_matched`,
  `shell::map::tests::a_row_stored_through_a_link_loses_to_the_resolved_folder`.
- **`shell env --shell <zsh|bash|fish|pwsh> --dir <folder>`** prints `none` or
  `<pool>\t<context>\t<claude home or ->\t<codex home or ->`. `none` also when `<data>/shell-hook`
  reads `off`, the map is missing, unreadable, over 1 MiB or of another version, the folder is
  not absolute, the row has neither home, or an argument is malformed. It is answered in `main`
  before the async runtime starts, takes no store lock, opens no control connection, silences
  panics, exits 0 and writes nothing to stderr; `--help` still reaches the ordinary parser. Tests:
  `shell::map::tests::the_longest_folder_of_an_organization_group_answers_its_homes_and_context`,
  CLI `shell_env_answers_without_the_owner_or_the_store_lock_within_25_ms` (the store's lock held
  and a control port that never answers; the fastest of seven warm calls within 25 ms),
  `shell_env_says_none_when_off_missing_unknown_version_or_outside`,
  `shell_env_never_writes_to_stderr_and_exits_zero`.
- **Choice.** `<data>/shell-hook`: `on`, `off`, anything else or absent is the default. The
  writer is node A-3.
- **`shell status [--dir <folder>]`** (`--json`: `{"ok": true, "data": ...}`) answers `hook`
  (`on`, `off`, `default_on` when no choice is made and the map holds a valid row, else
  `default_off`), `choice`, `map` (`ok`, `missing`, `unreadable`, `unknown_version`), `folder`,
  `terminal` (`hook_loaded`: `SWITCHBOARD_SHELL_HOOK` is `1`; `launched_session`:
  `SWITCHBOARD_SESSION` is set), `here` (`group`, `context`, `claude_home`, `codex_home`, or null:
  what `shell env` answers there) and `files`. It reads files only and never fails. `files` comes
  from the installer's record `<data>/shell-hook.json` (node A-3 writes it): the reader takes
  `{"files": [{"shell", "path", "state"}]}`, ignores other fields and skips an entry without the
  three strings; an absent or broken record gives `[]`. Test:
  `shell_status_reads_the_map_and_the_record_without_the_owner_and_never_fails`.

## The person's own Claude Code and Codex folders (SB-183, node A-5, 2026-10-26)

Code: `switchboard_runtime::person` (`claude_dir`, `codex_dir`, `codex_home`). A process started
in a hooked terminal or inside a launched session sees one of Switchboard's homes as
`CLAUDE_CONFIG_DIR` or `CODEX_HOME`; a value inside Switchboard's data folder (the default data
folder, compared as written and with links resolved through the nearest existing parent) is never
the person's, and `~/.claude` or `~/.codex` is used instead (`~/.pi/agent` for Pi's
`PI_CODING_AGENT_DIR`). These readers of the person's folders go through it:
`carry::claude_user_dir` and `carry::pi_user_dir` (what a launched home links from),
`limits::transcript_files` (the limit scan), `codex_route::config_path` (SB-143's provider),
`launch::user_scope` and `launch::board_server` (the MCP servers a launch copies), `agents::setup`
and the CLI's `onboard --status`.

Two readers still follow the process's variables as they are. `projects::detect_session` (which
session is asking) reads `CODEX_HOME` only to recognise a home Switchboard launched. The native
sign-in context, `external::context`, reads `CLAUDE_CONFIG_DIR` or `CODEX_HOME` and refuses a
Switchboard home rather than reading past it: `capture_current` and `capture_current_cached`
(`accounts capture` and the owner's “in use now” probe; the background probe may answer the source's locked or needs-access state first, and never captures), `activate_claude` and `lock_live` answer
“This session runs in a Switchboard home or a terminal in an organization's folder. Run this from
another terminal.” when it lies inside the data folder (Copylot reports 21ec4932 en-US and
a38bab30 ru-RU; `src/adapter.ts` and `src/locales/ru.ts` carry it). The refusal applies where the
operation runs: an offline CLI command, or `serve` started in such a terminal; a CLI command
answered by a running owner uses the owner's own environment. Whether the context's reads should
go through `person` instead, keeping the refusal for writes, is question A-Q8 (assumed, not done
in 0.7.0 node A-5). Tests: `person::tests::a_config_dir_inside_switchboards_data_is_never_the_persons_own`,
`carry::tests::a_launch_from_a_hooked_terminal_links_from_the_persons_folder`,
`carry::tests::a_pi_launch_from_a_switchboard_home_links_from_the_persons_pi_folder`,
`codex_route::tests::the_route_is_never_written_into_a_shell_home`,
`external::tests::a_capture_from_a_switchboard_home_is_refused`,
`external::tests::a_home_inside_switchboards_data_folder_is_refused`.
