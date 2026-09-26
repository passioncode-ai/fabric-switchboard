import type { Account, Adapter, LoginInput, Snapshot } from './types';

// Imported only after an explicit browser-only ?demo=1. No credentials are kept,
// no network/CLI/vault calls exist, and all state disappears on page reload.
export function createDemoAdapter(): Adapter {
  const now = () => Math.floor(Date.now() / 1000);
  const state: Snapshot = {
    accounts: [
      { id: 'demo-claude-work', label: 'Studio', provider: 'claude', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 32, observed_at: now() - 95, resets_at: now() + 10800, source: 'Synthetic fixture' } },
      { id: 'demo-codex-work', label: 'Engineering', provider: 'codex', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 68, observed_at: now() - 140, resets_at: now() + 7200, source: 'Synthetic fixture' } },
      { id: 'demo-claude-personal', label: 'Personal', provider: 'claude', kind: 'setup_token', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-codex-lab', label: 'Experiments', provider: 'codex', kind: 'api_key', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-claude-archive', label: 'Previous workspace', provider: 'claude', kind: 'oauth', pool: 'work', enabled: false, created_at: now(), identity: null, usage: { used_percent: 86, observed_at: now() - 7200, resets_at: null, source: 'Synthetic fixture' } },
    ],
    routes: { 'claude:work': 'demo-claude-work', 'codex:work': 'demo-codex-work' },
    events: [{ at: now() - 95, action: 'usage.observed', account_id: 'demo-claude-work', detail: 'Synthetic usage observation' }],
  };
  const log = (action: string, id: string, detail: string) => { state.events.push({ at: now(), action, account_id: id, detail }); state.events = state.events.slice(-100); };
  const pause = () => new Promise<void>((resolve) => setTimeout(resolve, 220));
  const account = (id: string) => { const found = state.accounts.find((item) => item.id === id); if (!found) throw new Error('Account unavailable'); return found; };
  const enabled = (id: string) => { const found = account(id); if (!found.enabled) throw new Error('This account is disabled. Enable it before continuing.'); return found; };
  const logins = new Map<string, LoginInput>();
  return {
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
    async remove(id) { await pause(); if (Object.values(state.routes).includes(id)) throw new Error('Select another account in this pool, or disable this account, before removing it.'); account(id); state.accounts = state.accounts.filter((item) => item.id !== id); log('account.removed', id, 'Synthetic account removed'); },
    async select(item) { await pause(); enabled(item.id); state.routes[`${item.provider}:${item.pool}`] = item.id; log('route.selected', item.id, 'Selected for next request'); },
    async launch(id, mode, workingDirectory) { await pause(); if (!workingDirectory.startsWith('/') || /[\u0000-\u001f]/.test(workingDirectory)) throw new Error('Choose an existing project directory.'); const item = enabled(id); if (mode === 'managed' && state.routes[`${item.provider}:${item.pool}`] !== id) throw new Error('Managed mode requires a selected account in this pool.'); log('session.launched', id, `Synthetic ${mode} launch`); return { message: 'Synthetic launch recorded; no terminal was opened.' }; },
    async beginLogin(input) { await pause(); const login_id = crypto.randomUUID(); logins.set(login_id, input); return { login_id, message: 'Synthetic sign-in is ready to finish.' }; },
    async finishLogin(id) { const input = logins.get(id); if (!input) throw new Error('Sign-in is not complete. Finish in Terminal, then try again.'); const item = await this.add({ ...input, kind: 'oauth', secret: '{}' }); logins.delete(id); return item; },
    async cancelLogin(id) { await pause(); logins.delete(id); },
    async probe(id) { await pause(); const item = enabled(id); if (item.kind !== 'oauth') throw new Error('Usage unavailable for this credential type.'); const usage = { used_percent: 42, observed_at: now(), resets_at: now() + 7200, source: 'Synthetic fixture' }; item.usage = usage; log('usage.observed', id, 'Synthetic usage observation'); return structuredClone(usage); },
  };
}
