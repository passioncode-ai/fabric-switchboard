import { isAbsoluteProjectPath } from './platform';
import type { Account, Adapter, AgentSetup, CurrentAccounts, ExternalIdentity, LoginInput, Snapshot } from './types';

// Imported only after an explicit browser-only ?demo=1. No credentials are kept,
// no network/CLI/vault calls exist, and all state disappears on page reload.
export function createDemoAdapter(): Adapter {
  const now = () => Math.floor(Date.now() / 1000);
  const studio: ExternalIdentity = { account_id: 'synthetic-studio', organization_id: 'synthetic-work', email: 'studio@example.test' };
  const codexIdentity: ExternalIdentity = { account_id: 'synthetic-codex', organization_id: 'synthetic-work', email: 'engineering@example.test' };
  const current: CurrentAccounts = {
    claude: { status: 'available', identity: studio, account_id: 'demo-claude-work' },
    codex: { status: 'available', identity: codexIdentity, account_id: 'demo-codex-work' },
  };
  const state: Snapshot = {
    accounts: [
      { id: 'demo-claude-work', label: 'Studio', provider: 'claude', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 32, observed_at: now() - 95, resets_at: now() + 10800, source: 'Synthetic fixture' } },
      { id: 'demo-codex-work', label: 'Engineering', provider: 'codex', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 68, observed_at: now() - 140, resets_at: now() + 7200, source: 'Synthetic fixture' } },
      { id: 'demo-claude-personal', label: 'Personal', provider: 'claude', kind: 'setup_token', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-codex-lab', label: 'Experiments', provider: 'codex', kind: 'api_key', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-claude-archive', label: 'Previous workspace', provider: 'claude', kind: 'oauth', pool: 'work', enabled: false, created_at: now(), identity: null, usage: { used_percent: 86, observed_at: now() - 7200, resets_at: now() - 600, source: 'Synthetic fixture' } },
    ],
    policies: [{ provider: 'claude', pool: 'work', target: 'managed', enabled: false, threshold_percent: 90, hysteresis_percent: 10, cooldown_seconds: 1800, max_age_seconds: 300, last_switched_at: null }],
    routes: { 'claude:work': 'demo-claude-work', 'codex:work': 'demo-codex-work' },
    events: [{ at: now() - 95, action: 'usage.observed', account_id: 'demo-claude-work', detail: 'Synthetic usage observation' }],
    rules: [
      { path: '/srv/projects/alpha-web', provider: 'claude', account_id: 'demo-claude-work', target: 'managed', enabled: true, created_at: now() - 3600, expires_at: now() + 6 * 3600 },
      { path: '/srv/projects/beta-api', provider: 'codex', account_id: 'demo-codex-work', target: 'managed', enabled: false, created_at: now() - 86400, expires_at: null },
    ],
  };
  const setup: AgentSetup = { cli_path: null, bundled_cli: '/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard', linked_cli: null, can_link: true, commands: { claude_code: 'claude mcp add --scope user switchboard -- switchboard mcp', codex: 'codex mcp add switchboard -- switchboard mcp', claude_plugin: 'claude plugin marketplace add passioncode-ai/fabric-switchboard && claude plugin install switchboard@switchboard' } };
  state.accounts[0].external_identity = studio;
  state.accounts[1].external_identity = codexIdentity;
  for (const item of state.accounts) {
    if (item.usage) item.usage.windows = [
      { name: 'Session', used_percent: item.usage.used_percent, resets_at: item.usage.resets_at },
      { name: 'Weekly', used_percent: Math.max(0, item.usage.used_percent - 14), resets_at: now() + 172800 },
    ];
    item.usage_health = { status: item.id.endsWith('archive') ? 'failed' : item.kind === 'oauth' ? 'ok' : 'unavailable', checked_at: now() - 95, next_check_at: now() + 85 };
  }
  const log = (action: string, id: string, detail: string) => { state.events.push({ at: now(), action, account_id: id, detail }); state.events = state.events.slice(-100); };
  const pause = () => new Promise<void>((resolve) => setTimeout(resolve, 220));
  const account = (id: string) => { const found = state.accounts.find((item) => item.id === id); if (!found) throw new Error('Account unavailable'); return found; };
  const enabled = (id: string) => { const found = account(id); if (!found.enabled) throw new Error('This account is disabled. Enable it before continuing.'); return found; };
  const logins = new Map<string, LoginInput>();
  return {
    async currentAccounts() { return structuredClone(current); },
    async monitorStatus() { return { running: true, interval_seconds: 180 }; },
    async captureCurrent(input) {
      await pause();
      const source = current[input.provider];
      if (source.status !== 'available' || !source.identity) throw new Error('No current CLI account was found. Sign in with the official CLI, then retry.');
      let item = state.accounts.find((entry) => entry.provider === input.provider && entry.pool === input.pool && entry.external_identity?.account_id === source.identity!.account_id && entry.external_identity?.organization_id === source.identity!.organization_id);
      if (!item) {
        item = { id: crypto.randomUUID(), label: input.label || source.identity.email || 'Captured account', provider: input.provider, kind: 'oauth', pool: input.pool, enabled: true, created_at: now(), identity: null, external_identity: structuredClone(source.identity), usage: null };
        state.accounts.push(item);
      } else if (input.label) item.label = input.label;
      source.account_id = item.id;
      log('account.captured', item.id, 'Synthetic current CLI account captured');
      return structuredClone(item);
    },
    async importClaudeSwap(pool) {
      await pause();
      const imported: Account[] = [];
      for (const [id, label] of [['synthetic-swap-studio', 'Swap studio'], ['synthetic-swap-personal', 'Swap personal']]) {
        let item = state.accounts.find((entry) => entry.provider === 'claude' && entry.pool === pool && entry.external_identity?.account_id === id);
        if (!item) {
          item = { id: crypto.randomUUID(), label, provider: 'claude', kind: 'oauth', pool, enabled: true, created_at: now(), identity: null, external_identity: { account_id: id, organization_id: 'synthetic-swap', email: `${id}@example.test` }, usage: null };
          state.accounts.push(item);
        }
        imported.push(structuredClone(item)); log('account.imported', item.id, 'Synthetic Claude Swap profile imported');
      }
      return { imported, failed: 1, skipped: 1 };
    },
    async activateNative(id) {
      await pause(); const item = enabled(id);
      if (item.provider !== 'claude' || item.kind !== 'oauth' || !item.external_identity) throw new Error('Native activation requires a Claude OAuth account with an external identity.');
      current.claude = { status: 'available', identity: structuredClone(item.external_identity), account_id: id };
      log('native.activated', id, 'Synthetic native Claude account activation');
    },
    async setPolicy(policy) {
      await pause();
      if (policy.provider !== 'claude' && policy.target === 'claude_cli') throw new Error('Native rotation is available only for Claude OAuth accounts.');
      const policies = state.policies!;
      const index = policies.findIndex((entry) => entry.provider === policy.provider && entry.pool === policy.pool && entry.target === policy.target);
      if (index < 0) policies.push(structuredClone(policy)); else policies[index] = structuredClone(policy);
      log('policy.updated', '', 'Synthetic rotation settings saved; no automatic switching runs in the demo');
    },
    async snapshot() { await pause(); return structuredClone(state); },
    async runtime() { return { proxy_address: 'Synthetic · no listener', platform: 'Browser demo', live_mode: 'Synthetic fixture' }; },
    async add(input) {
      await pause();
      if (!input.secret.trim()) throw new Error('Enter a credential before adding the account.');
      if (input.provider === 'codex' && input.kind === 'setup_token') throw new Error('This credential type is not supported by this provider.');
      if (input.kind === 'oauth') {
        try { const data: unknown = JSON.parse(input.secret); if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error(); }
        catch { throw new Error('Enter valid credential JSON for the selected provider.'); }
      }
      const item: Account = { id: crypto.randomUUID(), label: input.label, provider: input.provider, kind: input.kind, pool: input.pool, enabled: true, created_at: now(), identity: null, usage: null };
      state.accounts.push(item); log('account.added', item.id, 'Synthetic account added'); return structuredClone(item);
    },
    async update(id, label, active) { await pause(); Object.assign(account(id), { label, enabled: active }); if (!active) for (const key of Object.keys(state.routes)) if (state.routes[key] === id) delete state.routes[key]; log('account.updated', id, active ? 'Enabled' : 'Disabled; route cleared'); },
    async remove(id) { await pause(); if (Object.values(state.routes).includes(id)) throw new Error('Select another account in this pool, or disable this account, before removing it.'); account(id); state.accounts = state.accounts.filter((item) => item.id !== id); state.rules = state.rules!.filter((rule) => rule.account_id !== id); log('account.removed', id, 'Synthetic account removed'); },
    async select(item) { await pause(); enabled(item.id); state.routes[`${item.provider}:${item.pool}`] = item.id; log('route.selected', item.id, 'Selected for next request'); },
    async launch(id, mode, workingDirectory) { await pause(); if (!isAbsoluteProjectPath(workingDirectory)) throw new Error('Choose an existing project directory.'); const item = enabled(id); if (mode === 'managed' && state.routes[`${item.provider}:${item.pool}`] !== id) throw new Error('Managed mode requires a selected account in this pool.'); log('session.launched', id, `Synthetic ${mode} launch`); return { message: 'Synthetic launch recorded; no terminal was opened.' }; },
    async beginLogin(input) { await pause(); const login_id = crypto.randomUUID(); logins.set(login_id, input); return { login_id, message: 'Synthetic sign-in is ready to finish.' }; },
    async finishLogin(id) { const input = logins.get(id); if (!input) throw new Error('Sign-in is not complete. Finish in Terminal, then try again.'); const item = await this.add({ ...input, kind: 'oauth', secret: '{}' }); logins.delete(id); return item; },
    async cancelLogin(id) { await pause(); logins.delete(id); },
    async setProjectRule(input) {
      await pause();
      if (!isAbsoluteProjectPath(input.path)) throw new Error('Choose an absolute project folder.');
      if (input.expiresAt !== null && input.expiresAt <= now()) throw new Error('Choose an expiry in the future.');
      const item = account(input.accountId);
      if (input.target === 'claude_cli' && (item.provider !== 'claude' || item.kind !== 'oauth')) throw new Error('Native activation requires a Claude OAuth profile.');
      const rules = state.rules!; const index = rules.findIndex((rule) => rule.path === input.path && rule.provider === item.provider);
      const rule = { path: input.path, provider: item.provider, account_id: item.id, target: input.target, enabled: input.enabled, created_at: index < 0 ? now() : rules[index].created_at, expires_at: input.expiresAt };
      if (index < 0) rules.push(rule); else rules[index] = rule;
      log('project_rule', item.id, input.enabled ? 'saved' : 'paused'); return structuredClone(rule);
    },
    async removeProjectRule(path, provider) { await pause(); const before = state.rules!.length; state.rules = state.rules!.filter((rule) => !(rule.path === path && rule.provider === provider)); if (state.rules.length === before) throw new Error('Project rule not found.'); log('project_rule', '', 'removed'); },
    async agentSetup() { return structuredClone(setup); },
    async linkCli() { await pause(); setup.linked_cli = '~/.local/bin/switchboard'; setup.cli_path = setup.linked_cli; setup.commands.claude_code = `claude mcp add --scope user switchboard -- '${setup.linked_cli}' mcp`; setup.commands.codex = `codex mcp add switchboard -- '${setup.linked_cli}' mcp`; return { linked_cli: setup.linked_cli }; },
    async probe(id) { await pause(); const item = enabled(id); if (item.kind !== 'oauth') { item.usage_health = { status: 'unavailable', checked_at: now(), next_check_at: now() + 180 }; throw new Error('Usage unavailable for this credential type.'); } const usage = { used_percent: 42, observed_at: now(), resets_at: now() + 7200, source: 'Synthetic fixture', windows: [{ name: 'Session', used_percent: 42, resets_at: now() + 7200 }, { name: 'Weekly', used_percent: 28, resets_at: now() + 172800 }] }; item.usage = usage; item.usage_health = { status: 'ok', checked_at: now(), next_check_at: now() + 180 }; log('usage.observed', id, 'Synthetic usage observation'); return structuredClone(usage); },
  };
}
