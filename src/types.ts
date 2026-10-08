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
export interface AccountLimit { account_id: string; until: number; source: 'managed' | 'claude_code'; kind?: 'quota' | 'unknown'; resets_at?: number | null; confidence?: 'attributed' | 'inferred'; scope?: 'unknown'; observed_at?: number; event_id?: string }
export interface BackupInfo { file: string; created_at: number; accounts: number; missing?: number; own?: boolean; openable?: boolean }
export interface Restored { added: number; skipped: number; failed: number; projects?: number; rules?: number; routes?: number; settings?: number }
export interface BackupStatus { directory: string | null; enabled: boolean; backups: BackupInfo[]; last_error: string | null; last_written_at: number | null; restored_at_start?: { file: string; created_at: number; restored: Restored } | null }
export interface MonitorStatus { running: boolean; interval_seconds: number; sign_in_required?: string[]; limited?: AccountLimit[]; claude_swap_accounts?: number; renewal_blocked?: boolean; claude_swap_switching?: boolean; decisions?: { provider: Provider; pool: string; target: RotationPolicy['target']; reason: string; candidate_id: string | null }[] }
export interface CaptureInput { provider: Provider; label?: string; pool: string }
export interface ImportResult { imported: Account[]; failed: number; skipped: number; claude_swap_running?: boolean }
export interface Account {
  id: string; label: string; provider: Provider; kind: AuthKind; pool: string;
  external_identity?: ExternalIdentity | null; usage_health?: UsageHealth | null;
  enabled: boolean; created_at: number; identity: string | null; usage: Usage | null;
}
export interface Event { at: number; action: string; account_id: string | null; detail: string }
export interface ProjectRule { path: string; provider: Provider; account_id: string; target: 'managed' | 'claude_cli'; enabled: boolean; created_at: number; expires_at: number | null }
export interface ProjectRuleInput { path: string; accountId: string; target: ProjectRule['target']; enabled: boolean; expiresAt: number | null }
export interface AgentSetup { cli_path: string | null; bundled_cli: string | null; linked_cli: string | null; can_link: boolean; translocated?: boolean; commands: { claude_code: string; codex: string; claude_plugin: string } }
/** A project's folders and the pool of accounts reserved for them (0.6). */
export interface Project { pool: string; name: string; folders: string[]; created_at: number }
export interface ProjectInput { pool?: string; name: string; folders: string[]; accountIds: string[] }
export interface Snapshot { accounts: Account[]; routes: Record<string, string>; events: Event[]; policies?: RotationPolicy[]; rules?: ProjectRule[]; projects?: Project[] }
export interface RuntimeStatus { proxy_address: string; platform: string; live_mode: string }
export interface AddInput { label: string; provider: Provider; kind: AuthKind; pool: string; secret: string }
export interface LoginInput { provider: Provider; label: string; pool: string }
export interface Adapter {
  snapshot(): Promise<Snapshot>;
  currentAccounts(): Promise<CurrentAccounts>;
  captureCurrent(input: CaptureInput): Promise<Account>;
  importClaudeSwap(pool: string): Promise<ImportResult>;
  activateNative(id: string): Promise<void>;
  setPolicy(policy: RotationPolicy): Promise<void>;
  monitorStatus(): Promise<MonitorStatus>;
  runtime(): Promise<RuntimeStatus>;
  add(input: AddInput): Promise<Account>;
  update(id: string, label: string, enabled: boolean): Promise<void>;
  remove(id: string): Promise<void>;
  select(account: Account): Promise<void>;
  launch(id: string, mode: 'isolated' | 'managed', workingDirectory: string): Promise<{ message: string; agent_tools?: boolean }>;
  beginLogin(input: LoginInput): Promise<{ login_id: string; message: string }>;
  loginStatus(loginId: string): Promise<{ state: 'pending' | 'complete' | 'ended' }>;
  /** The saved account; `login_cleanup: 'pending'` when its temporary sign-in folder is still to be removed (SB-42). */
  finishLogin(loginId: string): Promise<Account & { login_cleanup?: 'done' | 'pending'; signed_in_again?: boolean }>;
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
  agentConnect(agent: string, pool: string): Promise<AgentConnection>;
  launchAgent(agent: string, pool: string, workingDirectory: string): Promise<unknown>;
  /** The OpenRouter key agents run on (SB-79): metadata and what it may still spend, never the key. */
  openrouterStatus(): Promise<OpenrouterStatus>;
  /** Saves the key; the answer is metadata only. */
  openrouterSave(key: string, model: string | null): Promise<unknown>;
  openrouterModel(model: string): Promise<unknown>;
  openrouterRemove(): Promise<unknown>;
  /** The ordinary Hermes's model and provider (SB-80). */
  hermesModel(): Promise<HermesModel>;
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
  /** Anonymous usage analytics (docs/ANALYTICS.md); unavailable outside a release build. */
  analytics(): Promise<LoginItem>;
  setAnalytics(enabled: boolean): Promise<LoginItem>;
  /** Automatic updates (SB-55): on by default; unavailable outside the installed app. */
  updateStatus(): Promise<UpdateStatus>;
  setAutoUpdate(enabled: boolean): Promise<UpdateStatus>;
  /** Quits through the drain and starts the new version; refused when nothing is ready. */
  restartToUpdate(): Promise<UpdateStatus>;
  /** The person's check (LC-16): runs with automatic updates off too; resolves with the status after. */
  checkForUpdates(): Promise<UpdateStatus>;
}
export interface LoginItem { available: boolean; enabled: boolean }
/** One entry of catalog/agents.json (third-party agents, 0.6). */
export interface AgentInfo { id: string; name: string; maker?: string | null; kind: string; url: string; level: 'mcp' | 'proxy' | 'launch'; binary?: string | null; openrouter_rank?: number | null; proxy_ok?: boolean | null; mcp: { supported: boolean }; notes?: string | null; subscription_warning?: string | null; openrouter?: { model_flag: string[] | null; model_env: string | null; default: boolean; notes: string | null } | null }
/** What OpenRouter says the saved key may spend (`GET /key`), in US dollars; null where it has no limit. */
export interface OpenrouterCredit { label: string | null; limit: number | null; limit_remaining: number | null; limit_reset: string | null; usage: number | null; usage_daily: number | null; is_free_tier: boolean | null }
export interface HermesModel { installed: boolean; configured: boolean; model?: string | null; provider?: string | null; base_url?: string | null; api_mode?: string | null; providers: string[]; error: string | null }
export type KimiRegion = 'mainland-cn' | 'global';
export interface KimiAccount { id: string; label: string; region: KimiRegion; added_at: number; nickname?: string | null; tier?: string | null }
/** A Kimi Code home's state at this read: the plan windows, or `error` naming why there are none. */
export interface KimiStatus { signed_in: boolean; nickname?: string | null; tier?: string | null; windows?: { name: string; used_percent: number; resets_at: number | null }[]; error: string | null; region?: KimiRegion }
export interface KimiAccounts { accounts: { account: KimiAccount; status: KimiStatus }[]; current: KimiStatus | null; installed: boolean }
export interface OpenrouterStatus { service: 'openrouter'; saved: boolean; saved_at: number | null; model: string | null; credit: OpenrouterCredit | null; credit_error: string | null }
/** What `switchboard agents connect` returns: commands and snippets, never a key. */
export interface AgentConnection { name: string; level: string; mcp: { supported: boolean; add_command: string | null; config_path: string | null; config_snippet: string | null }; anthropic: { base_url: string; env: Record<string, string>; config_snippet: string | null } | null; openai: { base_url: string } | null; key_command: string | null; requires: string | null; launch: string | null; warning: string | null; notes: string | null }
export type UpdatePhase = 'idle' | 'checking' | 'downloading' | 'ready' | 'failed';
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
}
