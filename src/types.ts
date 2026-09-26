export type Provider = 'claude' | 'codex';
export type AuthKind = 'api_key' | 'setup_token' | 'oauth';
export interface Usage { used_percent: number; observed_at: number; resets_at: number | null; source: string }
export interface Account {
  id: string; label: string; provider: Provider; kind: AuthKind; pool: string;
  enabled: boolean; created_at: number; identity: string | null; usage: Usage | null;
}
export interface Event { at: number; action: string; account_id: string | null; detail: string }
export interface Snapshot { accounts: Account[]; routes: Record<string, string>; events: Event[] }
export interface RuntimeStatus { proxy_address: string; platform: string; live_mode: string }
export interface AddInput { label: string; provider: Provider; kind: AuthKind; pool: string; secret: string }
export interface LoginInput { provider: Provider; label: string; pool: string }
export interface Adapter {
  snapshot(): Promise<Snapshot>;
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
