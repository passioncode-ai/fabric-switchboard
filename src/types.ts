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
export interface BackupInfo { file: string; created_at: number; accounts: number }
export interface BackupStatus { directory: string | null; enabled: boolean; backups: BackupInfo[]; last_error: string | null; last_written_at: number | null }
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
export interface Snapshot { accounts: Account[]; routes: Record<string, string>; events: Event[]; policies?: RotationPolicy[]; rules?: ProjectRule[] }
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
  finishLogin(loginId: string): Promise<Account & { login_cleanup?: 'done' | 'pending' }>;
  cancelLogin(loginId: string): Promise<void>;
  probe(id: string): Promise<Usage>;
  setProjectRule(input: ProjectRuleInput): Promise<unknown>;
  removeProjectRule(path: string, provider: Provider): Promise<unknown>;
  agentSetup(): Promise<AgentSetup>;
  backups(): Promise<BackupStatus>;
  backupNow(): Promise<BackupInfo | null>;
  restoreBackup(file: string): Promise<{ added: number; skipped: number; failed: number }>;
  linkCli(): Promise<{ linked_cli: string }>;
  /** Whether Switchboard opens at login in the background (SB-28); unavailable outside the installed app. */
  loginItem(): Promise<LoginItem>;
  setLoginItem(enabled: boolean): Promise<LoginItem>;
}
export interface LoginItem { available: boolean; enabled: boolean }
