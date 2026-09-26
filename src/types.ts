export type Provider = 'claude' | 'codex';
export type AuthKind = 'api_key' | 'setup_token' | 'oauth';
export interface UsageWindow { name: string; used_percent: number; resets_at: number | null }
export interface Usage { used_percent: number; observed_at: number; resets_at: number | null; source: string; windows?: UsageWindow[] }
export interface ExternalIdentity { account_id: string | null; organization_id: string | null; email: string | null }
export interface CurrentAccount { status: 'available' | 'missing' | 'unavailable'; identity: ExternalIdentity | null; account_id: string | null }
export type CurrentAccounts = Record<Provider, CurrentAccount>;
export interface UsageHealth { status: 'ok' | 'failed' | 'unavailable'; checked_at: number; next_check_at: number }
export interface RotationPolicy {
  provider: Provider; pool: string; target: 'managed' | 'claude_cli'; enabled: boolean;
  threshold_percent: number; hysteresis_percent: number; cooldown_seconds: number;
  max_age_seconds: number; last_switched_at: number | null;
}
export interface MonitorStatus { running: boolean; interval_seconds: number; decisions?: { provider: Provider; pool: string; target: RotationPolicy['target']; reason: string; candidate_id: string | null }[] }
export interface CaptureInput { provider: Provider; label?: string; pool: string }
export interface ImportResult { imported: Account[]; failed: number; skipped: number }
export interface Account {
  id: string; label: string; provider: Provider; kind: AuthKind; pool: string;
  external_identity?: ExternalIdentity | null; usage_health?: UsageHealth | null;
  enabled: boolean; created_at: number; identity: string | null; usage: Usage | null;
}
export interface Event { at: number; action: string; account_id: string | null; detail: string }
export interface Snapshot { accounts: Account[]; routes: Record<string, string>; events: Event[]; policies?: RotationPolicy[] }
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
  launch(id: string, mode: 'isolated' | 'managed', workingDirectory: string): Promise<{ message: string }>;
  beginLogin(input: LoginInput): Promise<{ login_id: string; message: string }>;
  finishLogin(loginId: string): Promise<Account>;
  cancelLogin(loginId: string): Promise<void>;
  probe(id: string): Promise<Usage>;
}
