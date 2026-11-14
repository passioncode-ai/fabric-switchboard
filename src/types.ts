export type Provider = 'claude' | 'codex';
export type AuthKind = 'api_key' | 'setup_token' | 'oauth';
export interface UsageWindow { name: string; used_percent: number; resets_at: number | null }
export interface Usage { used_percent: number; observed_at: number; resets_at: number | null; source: string; windows?: UsageWindow[] }
export interface ExternalIdentity { account_id: string | null; organization_id: string | null; email: string | null }
export interface CurrentAccount { status: 'available' | 'missing' | 'unavailable'; identity: ExternalIdentity | null; account_id: string | null; account_ids?: string[] }
export type CurrentAccounts = Record<Provider, CurrentAccount>;
export interface UsageHealth { status: 'ok' | 'failed' | 'unavailable'; checked_at: number; next_check_at: number }
export interface RotationPolicy {
  provider: Provider; pool: string; target: 'managed' | 'claude_cli'; enabled: boolean;
  threshold_percent: number; hysteresis_percent: number; cooldown_seconds: number;
  max_age_seconds: number; last_switched_at: number | null;
}
/** A provider limit on an account (SB-41). `resets_at` is the provider's own reset; `until` is when the account may be tried again — that reset or an estimated hold. Scope is unknown until a provider states it. */
export interface AccountLimit { account_id: string; until: number; source: 'managed' | 'claude_code'; kind?: 'quota' | 'unknown'; resets_at?: number | null; confidence?: 'attributed' | 'inferred'; scope?: 'unknown'; observed_at?: number; event_id?: string; reason?: string | null; hidden?: boolean }
export interface BackupInfo { file: string; created_at: number; accounts: number; missing?: number; own?: boolean; openable?: boolean }
/** SB-318: a hand-over saved from the window. */
export interface HandOverResult { saved: string | null; location?: string; public_key?: string }
/** SB-318: a received hand-over record as this computer reads it (`account_handover::plan`). */
export interface HandoverPlan { provider: Provider; label: string; email?: string | null; to: string; at: number; signature: 'unsigned' | 'signed' | 'invalid'; signer?: string | null; group: { pool: string; name: string } | null; organizations: { name: string; here: boolean }[]; held: boolean; pool: string; login?: { login_id: string } }
export interface Restored { added: number; skipped: number; failed: number; projects?: number; rules?: number; routes?: number; settings?: number; /** SB-314: accounts restored to the default group because their group here does not take them. */ returned_to_default?: number }
export interface BackupStatus { directory: string | null; enabled: boolean; backups: BackupInfo[]; last_error: string | null; last_written_at: number | null; restored_at_start?: { file: string; created_at: number; restored: Restored } | null }
/** A session Switchboard launched (SB-95): its launch record and its state now. */
export interface SessionInfo { account_id: string; label: string | null; /** The provider the session's route serves; `claude` for an agent through an API-key account's proxy, and since 0.7.0 the recipe id for an agent on the OpenRouter key (SB-201). */ provider: Provider | 'kimi' | 'hermes' | 'opencode' | (string & {}); /** SB-201: the agent's catalog id (`claude-code`, `codex`, `kimi-code`, `goose`...); null or absent when unknown. */ agent?: string | null; pool: string; mode: 'managed' | 'isolated'; working_directory: string; in_place: boolean; launched_at: number; /** SB-110: `personal` or `org:<id>`; absent on sessions launched before 0.6.80. */ context?: string; started_at: number | null; continues: string | null; state: 'starting' | 'running' | 'ended' | 'unknown' }
export interface MonitorStatus { running: boolean; interval_seconds: number; sign_in_required?: string[]; limited?: AccountLimit[]; claude_swap_accounts?: number; renewal_blocked?: boolean; claude_swap_switching?: boolean; decisions?: { provider: Provider; pool: string; target: RotationPolicy['target']; reason: string; candidate_id: string | null }[] }
export interface CaptureInput { provider: Provider; label?: string; pool: string }
export interface ImportResult { imported: Account[]; failed: number; skipped: number; claude_swap_running?: boolean }
/** SB-160 (B-4): the one group of a new seat's organization, when the seat landed outside it. */
export interface OrganizationGroupOffer { pool: string; name: string; in_cli?: boolean }
export interface Account {
  id: string; label: string; provider: Provider; kind: AuthKind; pool: string;
  external_identity?: ExternalIdentity | null; usage_health?: UsageHealth | null;
  enabled: boolean; created_at: number; identity: string | null; usage: Usage | null;
}
export interface Event { at: number; action: string; account_id: string | null; detail: string }
/** WF-11.1 (SB-373): one page of the journal from the `events` command, newest first; `next_before` pages on (null at the end); `kinds` are the filter kinds the owner knows. */
export interface EventsPage { events: Event[]; next_before: number | null; kinds: string[] }
/** WF-11.1: what the window asks the `events` command for; every field optional. */
export interface EventsQuery { kind?: string; account_id?: string; before?: number; limit?: number }
/** WF-2.4 (SB-364): an account given to an agent, as the `given_accounts` command reads it from the seat and launch records. */
export interface GivenAccount { account_id: string; agent: string; machine: string | null; last_launch: { pool: string | null; folder: string | null; at: number } | null; last_activity: number | null }
export interface ProjectRule { path: string; provider: Provider; account_id: string; target: 'managed' | 'claude_cli'; enabled: boolean; created_at: number; expires_at: number | null }
export interface ProjectRuleInput { path: string; accountId: string; target: ProjectRule['target']; enabled: boolean; expiresAt: number | null }
export interface AgentSetup { cli_path: string | null; bundled_cli: string | null; linked_cli: string | null; can_link: boolean; translocated?: boolean; commands: { claude_code: string; codex: string; claude_plugin: string }; connected?: { claude_code: boolean; codex: boolean }; agent_prompt?: string; proved?: { client: string | null; at: number } | null }
/** A project's folders and the pool of accounts reserved for them (0.6). */
export interface Project { pool: string; name: string; folders: string[]; created_at: number; sources?: string[]; /** SB-109: the organization the group runs on; none is personal. One organization arrives here. */ organization?: string | null; /** SB-282: a group of several organizations arrives as this list instead (`groupOrganizations` reads both). */ organizations?: string[]; /** SB-282: accounts at home in another group that serve this one too. */ shared_accounts?: string[] }
/** A folder Switchboard found that looks like a project's (SB-117); it joins only on Add. */
export interface ProjectSuggestion { folder: string; name: string; pool: string; project: string | null; reason: 'worktree' | 'same_repository' | 'ecosystem' | 'worked'; repository: string; first_seen: number }
export interface ProjectInput { pool?: string; name: string; folders: string[]; accountIds: string[]; /** SB-131: the folders the dialog opened with. */ baseFolders?: string[]; /** SB-109: an organization's id or `personal`; left out, the group keeps its own. */ organization?: string; /** SB-282: every organization of the group in order (empty: personal); wins over `organization`. */ organizations?: string[] }
/** SB-158: an organization accounts may belong to; `provider_organizations` are `claude:<uuid>` or `codex:<workspace>`. */
export interface Organization { id: string; name: string; domains?: string[]; provider_organizations?: string[]; created_at: number; /** SB-159: its organization on PassionCode.ai. */ vesta_id?: string | null }
/** SB-159: the PassionCode.ai connection, no token. */
export interface PassioncodeStatus { connected: boolean; base: string; state?: 'ok' | 'sign_in_again' | 'provider_sign_in' | 'unreachable'; connected_at?: number; read_at?: number | null; checked_at?: number | null; organizations?: number; cancelled?: boolean; disconnected?: boolean; /** SB-160 (B-7): organizations a read stopped listing, by local id, since `at`. */ released?: { organization: string; at: number }[] }
export interface Snapshot { accounts: Account[]; routes: Record<string, string>; events: Event[]; policies?: RotationPolicy[]; rules?: ProjectRule[]; projects?: Project[]; organizations?: Organization[]; /** account id → organization id or `personal` */ memberships?: Record<string, string>; /** SB-282: account id → organizations it also works for */ also_organizations?: Record<string, string[]>; /** SB-175: account id → when it was marked as used on another machine */ elsewhere?: Record<string, number>; /** SB-284: account id → where it works, in the person's words */ locations?: Record<string, string>; /** SB-322: account id → the agent it is given to, in the person's words */ assigned_to?: Record<string, string>; /** SB-352 wave 2: accounts a group lent to the default group */ default_shared?: string[]; /** SB-352 wave 2: accounts of the default group lent to a group */ default_lent?: string[]; /** SB-355: the other machines accounts live on, one group each */ machines?: Machine[]; /** SB-355: account on a machine → the group here it comes back to */ returns_to?: Record<string, string> }
/** SB-355 (DEC-0013): another machine an account lives on, a group of its own (no folders). */
export interface Machine { pool: string; name: string; identity?: string | null; aliases?: string[]; created_at: number }
/** SB-355: an account's copy on its machine, as a live read found it. */
export interface MachineCopy { pool: string | null; enabled: boolean | null; used_percent: number | null; usage_status: string | null }
/** SB-355: `machines --live`: each machine group, read over ssh. */
export interface MachinesLive { machines: { pool: string; name: string; accounts: { id: string; there?: MachineCopy | null }[]; live?: { reachable: boolean; error?: string } }[] }
/** WF-3 (SB-365, DEC-0015): why an account (or a ceiling) needs a person; the Tauri command `attention`, the same list as MCP `switchboard_attention`. */
export type AttentionReason = 'sign_in' | 'seat_ended' | 'renewal_refused' | 'usage_failing' | 'move_unknown' | 'refused_there' | 'ceiling_question';
/** WF-3: the window flow that fixes an item; `tool` is the MCP tool that does the same. */
export type AttentionWindow = 'sign_in' | 'answer_ceiling' | 'check_usage' | 'open_machines';
export interface AttentionItem { account_id: string | null; label: string | null; provider: Provider | null; pool: string | null; reason: AttentionReason; detail: string; since: number | null; action: { tool: string; window: AttentionWindow } }
export interface Attention { count: number; items: AttentionItem[] }
export interface RuntimeStatus { proxy_address: string; platform: string; live_mode: string }
export interface AddInput { label: string; provider: Provider; kind: AuthKind; pool: string; secret: string }
export interface LoginInput { provider: Provider; label: string; pool: string }
export interface Adapter {
  snapshot(): Promise<Snapshot>;
  currentAccounts(): Promise<CurrentAccounts>;
  captureCurrent(input: CaptureInput): Promise<Account & { organization_group?: OrganizationGroupOffer }>;
  importClaudeSwap(pool: string): Promise<ImportResult>;
  /** SB-322: `force` uses an account given to an agent anyway. */ activateNative(id: string, force?: boolean): Promise<void>;
  setPolicy(policy: RotationPolicy): Promise<void>;
  monitorStatus(): Promise<MonitorStatus>;
  sessions(): Promise<{ sessions: SessionInfo[] }>;
  runtime(): Promise<RuntimeStatus>;
  add(input: AddInput): Promise<Account>;
  update(id: string, label: string, enabled: boolean): Promise<void>;
  /** SB-175: mark an account as used on another machine (disabled here), or clear the mark. */
  /** SB-284: `location` says where it works (blank clears it). */ markElsewhere(id: string, on: boolean, location?: string): Promise<void>;
  /** SB-322: gives an account to an agent (`to` names it), or takes it back (`to` null). */ assignAccount(id: string, to: string | null): Promise<void>;
  /** SB-283: moves a sign-in to another machine over ssh; `pool` is its group there. */ sendAccount(id: string, to: string, pool: string): Promise<void>;
  /** SB-315: takes a moved sign-in back from `from`, the machine the move named (null: its machine group's); SB-355: into group `pool` here (null: the one it left). */ recallAccount(id: string, from: string | null, pool?: string | null): Promise<void>;
  /** SB-355: the machine groups read over ssh, each with its accounts' copies there. */ machinesLive(): Promise<MachinesLive>;
  /** WF-3 (SB-365): what needs a person now; an empty list when nothing does (or the command is missing). */ attention(): Promise<Attention>;
  /** SB-318: hands an account over: a save picker for the signed record, then the account is used by `to`; `saved` null when the picker was closed. */ handOverAccount(id: string, to: string, name: string): Promise<HandOverResult>;
  /** SB-318: a hand-over record chosen with the open picker; null when the picker was closed. */ openHandover(): Promise<unknown | null>;
  /** SB-318: what receiving the record would do, or with `apply` and the giver's key, the sign-in it starts. */ receiveHandover(record: unknown, signer: string | null, apply: boolean): Promise<HandoverPlan>;
  remove(id: string): Promise<void>;
  /** SB-322: `force` uses an account given to an agent anyway. */ select(account: Account, force?: boolean): Promise<void>;
  launch(id: string, mode: 'isolated' | 'managed', workingDirectory: string): Promise<{ message: string; agent_tools?: boolean }>;
  /** A folder chosen with the system picker, opened at `start` when that is a folder; null when the person closed it. */
  pickFolder(start?: string): Promise<string | null>;
  /** SB-264: several folders from one system picker; empty when it was closed. */
  pickFolders(start?: string): Promise<string[]>;
  beginLogin(input: LoginInput): Promise<{ login_id: string; message: string }>;
  loginStatus(loginId: string): Promise<{ state: 'pending' | 'complete' | 'ended' }>;
  pendingLogins(): Promise<{ logins: { login_id: string; provider: Provider; label: string; pool: string; state: 'pending' | 'complete' | 'ended' }[] }>;
  /** The saved account; `login_cleanup: 'pending'` when its temporary sign-in folder is still to be removed (SB-42). */
  finishLogin(loginId: string): Promise<Account & { login_cleanup?: 'done' | 'pending'; signed_in_again?: boolean; organization_group?: OrganizationGroupOffer }>;
  cancelLogin(loginId: string): Promise<void>;
  probe(id: string): Promise<Usage>;
  setProjectRule(input: ProjectRuleInput): Promise<unknown>;
  removeProjectRule(path: string, provider: Provider): Promise<unknown>;
  agentSetup(): Promise<AgentSetup>;
  backups(): Promise<BackupStatus>;
  backupNow(): Promise<BackupInfo | null>;
  restoreBackup(file: string): Promise<Restored>;
  linkCli(): Promise<{ linked_cli: string }>;
  /** Whether Switchboard opens at login in the background (SB-28); unavailable outside the installed app. */
  /** Create or update a project; the listed accounts are its whole set (core `save_project`). */
  saveProject(input: ProjectInput): Promise<unknown>;
  removeProject(pool: string): Promise<unknown>;
  /** SB-135: each project's folders as roots with their worktrees and clones, by pool. */
  projectRoots(): Promise<Record<string, import('./ui-logic').ProjectRoot[]>>;
  /** SB-122: an account into a project's pool, or back to `default`. */
  moveAccount(id: string, pool: string): Promise<unknown>;
  /** SB-158: organizations. `choice` is an organization's id, `personal`, or null to let the rules decide. */
  organizationAdd(name: string, domains: string[]): Promise<unknown>;
  /** SB-265: the GitHub owners the person's `gh` can see. */
  githubOwners(): Promise<GithubOwners>;
  /** SB-265: one owner's repositories matched with this computer. */
  githubRepositories(owner: string): Promise<GithubRepositories>;
  /** SB-265: clones one repository into `into/<name>`; never over a folder. */
  githubClone(owner: string, name: string, into: string): Promise<{ folder: string }>;
  /** SB-265: owners whose new repositories join the group by themselves. */
  projectSources(pool: string, sources: string[]): Promise<unknown>;
  organizationEdit(id: string, name: string | null, domains: string[] | null): Promise<unknown>;
  organizationRemove(id: string): Promise<unknown>;
  /** SB-282: `organizations` names every context of the account in order and wins over `choice`. */ organizationAssign(account: string, choice: string | null, organizations?: string[]): Promise<unknown>;
  passioncode(action: 'status' | 'connect' | 'cancel' | 'read' | 'disconnect'): Promise<PassioncodeStatus>;
  /** SB-146: `also` adds it to one more group; `from` names the group it leaves when it is in several. */
  moveRoot(folder: string, pool: string, also?: boolean, from?: string | null): Promise<unknown>;
  /** SB-143: Codex in project folders. */
  codexRoute(enabled?: boolean | null): Promise<CodexRoute>;
  /** SB-352: the groups that take part in balancing; with no change, the state. */
  balance(change?: BalanceChange | null): Promise<BalanceStatus>;
  repositoryMap(under: string[]): Promise<import('./ui-logic').RepositoryMap>;
  /** SB-145: what the proxy relayed in the last `days` days. */
  statistics(days: number): Promise<import('./ui-logic').Statistics>;
  /** SB-131: `suggestions` are folders still being added (retried, never asked); `added` and `removed` are the last day's. */
  projectSuggestions(): Promise<{ suggestions: ProjectSuggestion[]; added?: ProjectSuggestion[]; removed?: ProjectSuggestion[] }>;
  acceptProjectSuggestion(folder: string): Promise<unknown>;
  dismissProjectSuggestion(folder: string): Promise<unknown>;
  agentConnect(agent: string, pool: string): Promise<AgentConnection>;
  launchAgent(agent: string, pool: string, workingDirectory: string): Promise<unknown>;
  /** The OpenRouter key agents run on (SB-79): metadata and what it may still spend, never the key. */
  openrouterStatus(): Promise<OpenrouterStatus>;
  /** SB-156: one saved key's status with its spend: the machine's (`null`) or a group's. */
  openrouterKeyStatus(pool: string | null): Promise<OpenrouterStatus>;
  /** Saves the key; the answer is metadata only. */
  openrouterSave(key: string, model: string | null, pool?: string | null): Promise<unknown>;
  openrouterModel(model: string, pool?: string | null): Promise<unknown>;
  openrouterRemove(pool?: string | null): Promise<unknown>;
  /** SB-144: the machine's key and each project's own; never a key. */
  openrouterKeys(): Promise<OpenrouterKeys>;
  /** The ordinary Hermes's model and provider (SB-80). */
  hermesModel(): Promise<HermesModel>;
  agentsOverview(): Promise<AgentsOverview>;
  /** SB-71 (C-2): every fallback chain with what each step would do now and the last hand-over. */
  chains(): Promise<Chains>;
  /** Sets a scope's chain in order; an empty list clears it; `preset` stands for the list. */
  setChain(scope: ChainScope, executors: ChainExecutor[], preset?: string | null): Promise<{ chain: Chain | null }>;
  /** SB-72 (C-7): every spending ceiling, the open questions oldest first and the last hand-over; numbers and codes only. */
  ceilings(): Promise<Ceilings>;
  /** Sets a scope's ceiling in dollars, or clears it with `null`; the window may raise or clear (C-Q1). */
  setCeiling(scope: ChainScope, usd: number | null): Promise<unknown>;
  /** Answers a reached ceiling's question: `double` only from the window, `keep` from anywhere. */
  answerCeiling(id: string, answer: 'keep' | 'double'): Promise<unknown>;
  agentEnable(agent: string, enabled: boolean): Promise<unknown>;
  agentDefault(agent: string | null): Promise<unknown>;
  /** WF-10.1 (SB-372): whether the sessions Switchboard starts bypass permission prompts (`bypass`) or ask (`ask`). */
  agentPermissions(permissions: 'bypass' | 'ask'): Promise<unknown>;
  /** WF-11.1 (SB-373): a page of the journal, filtered by the owner; null when the owner has no `events` command yet. */
  events(query: EventsQuery): Promise<EventsPage | null>;
  /** WF-2.4 (SB-364): where each account given to an agent runs and last ran; null when the owner has no `given_accounts` command yet. */
  givenAccounts(): Promise<GivenAccount[] | null>;
  openrouterModels(): Promise<OpenrouterModels>;
  hermesSetModel(provider: string | null, model: string | null): Promise<HermesModel>;
  /** Kimi Code subscription accounts (SB-81) with plan usage asked of Kimi, and the ordinary `kimi`. */
  kimiAccounts(): Promise<KimiAccounts>;
  kimiLoginBegin(label: string, region: KimiRegion): Promise<{ login_id: string }>;
  kimiLoginStatus(id: string): Promise<{ state: 'pending' | 'complete' | 'ended' }>;
  kimiLoginFinish(id: string): Promise<{ account: KimiAccount }>;
  kimiLoginCancel(id: string): Promise<unknown>;
  kimiSignInAgain(id: string): Promise<unknown>;
  kimiLaunch(id: string, workingDirectory: string): Promise<unknown>;
  kimiRemove(id: string): Promise<unknown>;
  /** Starts an agent with an OpenRouter recipe on the saved key; `model` null: the key's model. */
  launchOnOpenrouter(agent: string, model: string | null, workingDirectory: string): Promise<{ model: string | null; model_choice: 'launch' | 'agent' }>;
  loginItem(): Promise<LoginItem>;
  setLoginItem(enabled: boolean): Promise<LoginItem>;
  /** The terminal hook's first-time notice (SB-183), kept until acknowledged; null when none waits. */
  shellHookNotice(): Promise<ShellHookNotice | null>;
  /** The window showed the notice: it is not given again. */
  acknowledgeShellHookNotice(): Promise<void>;
  /** SB-183 (node A-7): the Groups page's *Terminals in organization folders* section. */
  shellHookStatus(): Promise<ShellHookStatus>;
  /** SB-202 (B-8): the organization checklist for the first run. */
  onboard(): Promise<Onboarding>;
  /** Turn off / Turn on: the same as `switchboard shell off|on`, refused outside the installed app. */
  setShellHook(on: boolean): Promise<ShellHookStatus>;
  /** Anonymous usage analytics (docs/ANALYTICS.md); unavailable outside a release build. */
  analytics(): Promise<AnalyticsStatus>;
  /** The shared switch, and the person's answer in this app: nothing is sent before it (issue #128).
   * `question` is the switch the question showed when this answers it; absent for About's switch, which always sets it. */
  setAnalytics(enabled: boolean): Promise<AnalyticsStatus>;
  /** The first-run usage notice's OK (SB-246). */
  analyticsNoticeSeen(): Promise<AnalyticsStatus>;
  /** Automatic updates (SB-55): on by default; unavailable outside the installed app. */
  updateStatus(): Promise<UpdateStatus>;
  setAutoUpdate(enabled: boolean): Promise<UpdateStatus>;
  /** Quits through the drain and starts the new version; refused when nothing is ready. */
  restartToUpdate(): Promise<UpdateStatus>;
  /** The person's check (LC-16): runs with automatic updates off too; resolves with the status after. */
  checkForUpdates(): Promise<UpdateStatus>;
}
/** SB-71 (C-2): a fallback chain's scope: this computer, a group (`pool`) or one task (a workflow id). */
export type ChainScope = { kind: 'machine' } | { kind: 'project'; pool: string } | { kind: 'workflow'; id: string };
/** One step: an agent, optionally pinned to a saved account or to a key by its vault name (never a value). */
export interface ChainExecutor { agent: string; account_id?: string | null; key?: string | null }
export interface Chain { scope: ChainScope; executors: ChainExecutor[]; updated_at: number }
/** What a step would do now (`chain_view::preview`): codes, never words; `spend` stays null until a hand-over reads it (C-5). */
export interface ChainStepView {
  agent: string; name: string | null;
  state: 'ready' | 'skipped' | 'not_automatic' | 'not_installed';
  /** SB-245: `none` for an agent that asks for no approval, which the automatic fallback never follows a Claude Code or Codex executor with (SB-82 D-6). */
  containment?: 'asks' | 'none';
  /** `agent_disabled`, `no_usable_account`, `sign_in_required`, `key_pinned`, `agent_not_automatic_yet`, `agent_settings_unreadable`, ... */
  reason: string | null;
  account_id?: string; account_label?: string;
  /** Hermes and OpenCode on an OpenRouter key, or Claude Code and Codex on an API-key account. */
  paid: boolean;
  key_scope?: 'group' | 'machine' | null; key?: string; spend?: null;
}
/** The last automatic hand-over pass, from spend.json (written by the hand-over gate, C-5). */
export interface LastHandover { at: number; workflow: string; outcome: string; skipped: { agent: string; reason: string }[] }
export interface ChainPreset { id: string; agents: { agent: string; name: string | null }[] }
/** `readiness` is keyed `machine`, `project:<pool>` or `workflow:<id>`. */
export interface Chains { chains: Chain[]; effective: Chain | null; presets: ChainPreset[]; readiness: Record<string, ChainStepView[]>; last_handover: LastHandover | null; ceilings_unreadable: boolean; limits_known: boolean }
/** One spending ceiling as `spend::view` gives it: machine and group per UTC day, a task since its first paid hand-over. */
export interface CeilingView { scope: ChainScope; cents: number; usd: number; set_at: number; doubled_from: number | null; active: boolean; inactive_reason: 'no_own_key' | null }
/** An open question of a reached ceiling; `double_usd` is null when doubling would pass $1,000. */
export interface CeilingQuestionView { id: string; scope: ChainScope; cents: number; usd: number; double_usd: number | null; spent: number; reached_at: number }
export interface Ceilings { ceilings: CeilingView[]; questions: CeilingQuestionView[]; last_handover: LastHandover | null; limits: { min_usd: number; max_usd: number; min_remaining_usd: number } }
export interface LoginItem { available: boolean; enabled: boolean }
/** The organization and its group whose presence put the terminal hook in place (SB-183). */
export interface ShellHookNotice { organization: string; group: string }
/** One start file the terminal hook changed or left alone, as `shell-hook.json` records it. */
export interface ShellHookFile { shell: string; path: string; state: string }
/** What `switchboard shell status` reads, for the window: `hook` is on, off, default_on (a group belongs to an organization) or default_off; `can_change` is false outside the installed app. */
/** SB-202 (B-8): one step of `switchboard onboard`, as the window reads it through the Tauri `onboard` command. */
export interface OnboardStep { id: string; state: 'done' | 'todo' | 'needs_person' | 'optional'; summary: string; next?: unknown; organization?: { id: string; name: string }; group?: { pool: string; name: string }; groups?: string[]; ended?: number; terminals?: 'active' | 'pending' | 'off' | 'waiting' | 'unread' }
export interface Onboarding { complete: boolean; degraded: string[]; steps: OnboardStep[] }
export interface ShellHookStatus { hook: 'on' | 'off' | 'default_on' | 'default_off'; choice: 'on' | 'off' | 'default'; map: string; files: ShellHookFile[]; organizations: string[]; /** Organizations whose folder map rows carry a Codex account. */ codex_organizations?: string[]; /** SB-222: organization groups the folder map holds no row for, and why. */ unmapped?: { organization: string; group: string; reason: 'held' | 'missing' | 'no_folder'; held_by: string | null }[]; can_change: boolean; home: string | null }
/** Usage analytics (docs/ANALYTICS.md): `enabled` is Switchboard's own switch (SB-246); `notice_pending`, whether the first-run notice still waits for OK. */
export interface AnalyticsStatus { available: boolean; enabled: boolean; notice_pending?: boolean }
/** One entry of catalog/agents.json (third-party agents, 0.6). */
export interface AgentInfo { id: string; name: string; maker?: string | null; kind: string; url: string; level: 'mcp' | 'proxy' | 'launch'; binary?: string | null; openrouter_rank?: number | null; proxy_ok?: boolean | null; mcp: { supported: boolean }; notes?: string | null; subscription_warning?: string | null; openrouter?: { model_flag: string[] | null; model_env: string | null; default: boolean; notes: string | null } | null }
/** What OpenRouter says the saved key may spend (`GET /key`), in US dollars; null where it has no limit. */
export interface OpenrouterCredit { label: string | null; limit: number | null; limit_remaining: number | null; limit_reset: string | null; usage: number | null; usage_daily: number | null; is_free_tier: boolean | null }
export interface HermesModel { installed: boolean; configured: boolean; model?: string | null; provider?: string | null; base_url?: string | null; api_mode?: string | null; providers: string[]; error: string | null }
export type KimiRegion = 'mainland-cn' | 'global';
export interface KimiAccount { id: string; label: string; region: KimiRegion; added_at: number; nickname?: string | null; tier?: string | null }
/** A Kimi Code home's state at this read: the plan windows, or `error` naming why there are none. */
export interface KimiStatus { signed_in: boolean; nickname?: string | null; tier?: string | null; windows?: { name: string; used_percent: number; resets_at: number | null }[]; error: string | null; region?: KimiRegion; checked_at?: number | null }
export interface KimiAccounts { accounts: { account: KimiAccount; status: KimiStatus }[]; current: KimiStatus | null; installed: boolean }
/** SB-120: one catalog agent as this computer has it. */
export interface AgentRow { id: string; name: string; maker?: string | null; url: string; install?: string | null; installed: boolean; switchable: boolean; enabled: boolean; disabled: boolean; default: boolean; openrouter: boolean; mcp: boolean; command: string; argv?: string[]; path?: string | null }
export interface RecommendedModel { id: string; name: string; maker: string; context: number; input_usd_per_million: number; url: string; use?: string }
export interface AgentsOverview { schema?: number; revision?: string; updated_at?: number | null; default: string | null; default_ready?: boolean; /** SB-298, WF-10.1: whether the sessions Switchboard starts bypass permission prompts; absent from an owner that does not report it. */ permissions?: 'bypass' | 'ask'; installed: number; agents: AgentRow[]; recommended_models: RecommendedModel[] }
/** SB-119: OpenRouter's tool-capable models; `available: false` when its list could not be read. */
export interface OpenrouterModel { id: string; name?: string | null; context?: number | null; prompt?: string | null; completion?: string | null }
export interface OpenrouterModels { recommended: RecommendedModel[]; models: OpenrouterModel[]; available: boolean }
/** SB-143: whether Codex sessions in project folders use the projects' Codex accounts. */
/** SB-352: a change of the groups that take part in balancing (pool ids, `default` included). */
export interface BalanceChange { add?: string[]; remove?: string[]; all_new?: boolean | null; backup_add?: string[]; backup_remove?: string[] }
/** SB-352: one loan in `balance status`. */
export interface BalanceLoan { account_id: string; label: string | null; provider: Provider; from: string; from_name: string; to: string; to_name: string; at: number }
/** SB-352: one journal line (`balancing.jsonl`): codes and ids only. */
export interface BalanceEntry { at: number; account: string; provider: Provider; from: string; to: string; action: 'lend' | 'return'; reason: string; outcome: string }
/** SB-352: `balance status`, as `switchboard balance status --json` answers it. */
export interface BalanceStatus { on: boolean; groups: { pool: string; name: string }[]; available: { pool: string; name: string; chosen: boolean; backup?: boolean }[]; all_new: boolean; pass_minutes: number; lent: BalanceLoan[]; recent: BalanceEntry[]; changed?: boolean; /** WF-6, DEC-0016. */ backup?: { pool: string; name: string; accounts: number; lent: number }[] }
export interface CodexRoute { on: boolean; applied: boolean; config: string | null; conflict: string | null; projects: string[] }
/** SB-144: saved OpenRouter keys by project (`pool: null` is the machine's key). */
export interface OpenrouterKeys { service: 'openrouter'; keys: { pool: string | null; project: string | null; saved_at: number; model: string | null }[]; projects: { pool: string; name: string; own: boolean }[] }
export interface OpenrouterStatus { service: 'openrouter'; saved: boolean; saved_at: number | null; model: string | null; credit: OpenrouterCredit | null; credit_error: string | null }
/** What `switchboard agents connect` returns: commands and snippets, never a key. */
export interface AgentConnection { name: string; level: string; mcp: { supported: boolean; add_command: string | null; config_path: string | null; config_snippet: string | null }; anthropic: { base_url: string; env: Record<string, string>; config_snippet: string | null } | null; openai: { base_url: string } | null; key_command: string | null; requires: string | null; launch: string | null; warning: string | null; notes: string | null }
export type UpdatePhase = 'idle' | 'checking' | 'downloading' | 'ready' | 'failed' | 'available';
export interface UpdateStatus {
  available: boolean;
  reason: string | null;
  enabled: boolean;
  state: UpdatePhase;
  current: string;
  version: string | null;
  checked_at: number | null;
  error: string | null;
  needs_permission: boolean;
  /** SB-208: when a ready update may start on its own, while SB-191's pause holds it (Unix seconds); null otherwise. */
  restart_after: number | null;
}

/** SB-265: what `gh` can see: the person's login, then their organizations. */
export interface GithubOwners { available: boolean; reason: 'missing' | 'signed_out' | null; owners: { login: string; kind: 'user' | 'organization' }[] }
/** SB-265: one owner's repositories; `folder` is its checkout on this computer. */
export interface GithubRepository { name: string; url: string | null; archived: boolean; fork: boolean; folder: string | null }
export interface GithubRepositories { owner: string; clone_into: string | null; repositories: GithubRepository[] }
