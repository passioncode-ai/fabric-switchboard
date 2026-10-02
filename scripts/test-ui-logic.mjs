// Pure interface rules behind B-05, B-11, B-12 and the appearance setting.
import assert from 'node:assert/strict';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const load = async (path) => {
  const source = readFileSync(new URL(path, import.meta.url), 'utf8').replace(/^import type .*$/gm, '');
  const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
};
const logic = await load('../src/ui-logic.ts');
let cases = 0;
const check = (fn) => { fn(); cases += 1; };

// Appearance: unknown or missing storage falls back to System; System follows the OS.
check(() => { for (const value of [null, undefined, '', 'sepia', 42]) assert.equal(logic.parseAppearance(value), 'system'); });
check(() => { for (const value of ['system', 'dark', 'light']) assert.equal(logic.parseAppearance(value), value); });
check(() => {
  assert.equal(logic.resolveTheme('system', true), 'light');
  assert.equal(logic.resolveTheme('system', false), 'dark');
  assert.equal(logic.resolveTheme('dark', true), 'dark');
  assert.equal(logic.resolveTheme('light', false), 'light');
});

// B-11: the monitor polls enabled OAuth accounts only; manual checks need OAuth.
check(() => {
  assert.equal(logic.monitorChecks({ enabled: true, kind: 'oauth' }), true);
  assert.equal(logic.monitorChecks({ enabled: false, kind: 'oauth' }), false);
  for (const kind of ['api_key', 'setup_token']) { assert.equal(logic.monitorChecks({ enabled: true, kind }), false); assert.equal(logic.canProbe({ kind }), false); }
  assert.equal(logic.canProbe({ kind: 'oauth' }), true);
});

// B-12: a passed reset time makes the observation stale regardless of its age.
check(() => {
  const now = 10_000;
  assert.deepEqual(logic.usageFreshness({ observed_at: now - 60, resets_at: now + 600 }, 300, now), { stale: false, resetPassed: false });
  assert.deepEqual(logic.usageFreshness({ observed_at: now - 400, resets_at: now + 600 }, 300, now), { stale: true, resetPassed: false });
  assert.deepEqual(logic.usageFreshness({ observed_at: now - 60, resets_at: now - 1 }, 300, now), { stale: true, resetPassed: true });
  assert.deepEqual(logic.usageFreshness({ observed_at: now - 60, resets_at: null, windows: [{ name: 'Session', used_percent: 90, resets_at: now }] }, 300, now), { stale: true, resetPassed: true });
  assert.deepEqual(logic.usageFreshness({ observed_at: now - 60, resets_at: null, windows: [{ name: 'Weekly', used_percent: 10, resets_at: null }] }, 300, now), { stale: false, resetPassed: false });
  assert.equal(logic.windowReset(now, now), true);
  assert.equal(logic.windowReset(now + 1, now), false);
  assert.equal(logic.windowReset(null, now), false);
});

// B-05: a background read stamped before or during a mutation is dropped.
check(() => {
  const clock = new logic.MutationClock();
  const idle = clock.stamp();
  assert.equal(clock.accepts(idle), true);
  const before = clock.stamp();
  clock.begin();
  assert.equal(clock.accepts(before), false, 'read issued before the mutation');
  const during = clock.stamp();
  assert.equal(clock.accepts(during), false, 'mutation still running');
  clock.end();
  assert.equal(clock.accepts(during), false, 'read issued while a mutation ran may predate its write');
  assert.equal(clock.accepts(clock.stamp()), true, 'read issued after the mutation settled');
  assert.equal(clock.accepts(before), false);
  clock.begin(); clock.begin(); clock.end();
  assert.equal(clock.accepts(clock.stamp()), false, 'overlapping mutations');
  clock.end(); clock.end();
  assert.equal(clock.accepts(clock.stamp()), true, 'end never underflows');
});

// B-04: newly surfaced backend messages map to fixed text; the rollback warning stays verbatim.
check(() => {
  const source = readFileSync(new URL('../src/adapter.ts', import.meta.url), 'utf8');
  for (const message of [
    'Claude rollback lost its account lock. Check the current Claude sign-in before retrying.',
    'Claude account lock is unavailable. Check permissions of the Claude config directory.',
    'Finish an existing sign-in before starting another.',
    // 0.5.1: actionable backend sentences that used to fall back to the generic text.
    'Part of this account is stored where only the Fabric Switchboard app can remove it. Remove the account in the app.',
    'Enable the account before launch.',
    'Account home cleanup failed. Close its sessions and retry.',
    'Sign-in credential unavailable. Check Keychain access and finish login.',
  ]) assert(source.includes(`  '${message}',`), `verbatim: ${message}`);
  for (const message of ['Label, pool or identity is invalid', 'Account identity is ambiguous in this pool', 'Terminal could not open.', 'Sign-in cleanup needs Keychain access.', 'Rotation time precedes the last switch']) assert(source.includes(`  '${message}': '`), `mapped: ${message}`);
});

// Project rules: paused wins over expired; only active rules raise the Accounts strip.
check(() => {
  const now = 1_000_000;
  assert.equal(logic.ruleState({ enabled: true, expires_at: null }, now), 'active');
  assert.equal(logic.ruleState({ enabled: true, expires_at: now + 1 }, now), 'active');
  assert.equal(logic.ruleState({ enabled: true, expires_at: now }, now), 'expired');
  assert.equal(logic.ruleState({ enabled: false, expires_at: now - 10 }, now), 'paused');
  assert.equal(logic.activeRules([{ enabled: true, expires_at: null }, { enabled: false, expires_at: null }, { enabled: true, expires_at: now - 1 }], now).length, 1);
  assert.equal(logic.activeRules(undefined, now).length, 0);
});
check(() => {
  const now = 1_000_000;
  assert.equal(logic.expiryFrom('8', now), now + 8 * 3600);
  for (const choice of ['', '0', '721', 'x', '1.5']) assert.equal(logic.expiryFrom(choice, now), null);
  assert.deepEqual(logic.EXPIRY_CHOICES.map(([value]) => value), ['1', '8', '24', '168', '']);
  assert.equal(logic.projectName('/srv/projects/alpha-web/'), 'alpha-web');
  assert.equal(logic.projectName('C:\\Work\\beta-api'), 'beta-api');
});

// 0.5: accounts group by provider, then pool; default pool first; enabled before disabled.
check(() => {
  const a = (id, provider, pool, enabled = true, created_at = 1) => ({ id, provider, pool, enabled, created_at, kind: 'oauth' });
  const groups = logic.groupAccounts([a('c2', 'codex', 'work'), a('x', 'claude', 'work', false), a('y', 'claude', 'default', true, 5), a('z', 'claude', 'work', true, 9), a('w', 'claude', 'default', true, 2)]);
  assert.deepEqual(groups.map((g) => g.provider), ['claude', 'codex']);
  assert.deepEqual(groups[0].pools.map((p) => p.pool), ['default', 'work']);
  assert.deepEqual(groups[0].pools[0].accounts.map((x) => x.id), ['w', 'y']);
  assert.deepEqual(groups[0].pools[1].accounts.map((x) => x.id), ['z', 'x'], 'disabled last');
  assert.equal(groups[0].count, 4);
  assert.deepEqual(logic.groupAccounts([]), []);
});
// 0.5 D-3: a Claude OAuth row with identity switches Claude Code; others select the managed route.
check(() => {
  const claude = { provider: 'claude', kind: 'oauth', enabled: true, external_identity: { account_id: 'a', organization_id: null, email: null } };
  const flags = (o = {}) => ({ current: false, selected: false, signIn: false, ...o });
  assert.equal(logic.primaryAction(claude, flags()), 'switch');
  assert.equal(logic.primaryAction(claude, flags({ current: true })), 'in_use');
  assert.equal(logic.primaryAction(claude, flags({ signIn: true })), 'sign_in');
  assert.equal(logic.primaryAction({ ...claude, enabled: false }, flags()), 'disabled');
  assert.equal(logic.primaryAction({ ...claude, external_identity: null }, flags()), 'select');
  const codex = { provider: 'codex', kind: 'oauth', enabled: true, external_identity: claude.external_identity };
  assert.equal(logic.primaryAction(codex, flags()), 'select');
  assert.equal(logic.primaryAction(codex, flags({ selected: true })), 'selected');
  assert.equal(logic.canSwitchNative(claude), true);
  assert.equal(logic.canSwitchNative({ ...claude, kind: 'api_key' }), false);
});
// 0.5: one click turns on Claude Code switching for the pool holding most switchable accounts.
check(() => {
  const acct = (pool, kind = 'oauth') => ({ provider: 'claude', kind, pool, enabled: true, external_identity: { account_id: pool + Math.random(), organization_id: null, email: null } });
  assert.equal(logic.autoSwitchPool([acct('default')], []), null, 'one account cannot rotate');
  assert.equal(logic.autoSwitchPool([acct('default'), acct('work'), acct('work')], []), 'work');
  assert.equal(logic.autoSwitchPool([acct('default'), acct('default'), acct('work', 'api_key')], []), 'default');
  assert.equal(logic.autoSwitchPool([acct('default'), acct('default')], [{ provider: 'claude', target: 'claude_cli', enabled: true }]), null, 'already on');
});

// 0.5.1: a sign-in that saved its account but left its staging folder is a success; one the
// owner forgot (it restarted) can no longer be finished or cancelled; anything else retries.
check(() => {
  assert.equal(logic.loginOutcome('Account saved; isolated login cleanup needs attention.'), 'saved_with_cleanup');
  assert.equal(logic.loginOutcome('Sign-in not found. Start again.'), 'forgotten');
  for (const text of ['Sign-in is not complete. Finish in Terminal, then try again.', 'Finish or close sign-in in Terminal before cancelling.', 'Sign-in cleanup needs Keychain access. Unlock Keychain and allow access, then cancel again.', 'The operation could not be completed. Check your input and native credential storage access, then retry.', '', 'sign-in not found. start again.', 'Account saved; isolated login cleanup needs attention'])
    assert.equal(logic.loginOutcome(text), 'retry', text);
});
// The outcomes compare sanitized text, so adapter.ts must show both backend sentences verbatim,
// and main.ts must decide through loginOutcome rather than its own string comparisons.
check(() => {
  const adapter = readFileSync(new URL('../src/adapter.ts', import.meta.url), 'utf8');
  for (const message of [logic.LOGIN_FORGOTTEN, logic.LOGIN_SAVED_CLEANUP]) assert(adapter.includes(`  '${message}',`), `verbatim: ${message}`);
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert(!main.includes(logic.LOGIN_FORGOTTEN) && !main.includes(logic.LOGIN_SAVED_CLEANUP), 'main.ts classifies sign-in errors through loginOutcome');
  assert.equal((main.match(/loginOutcome\(/g) ?? []).length, 3, 'status poll, finish and cancel each classify');
});

console.log(`${cases} ui-logic cases passed: appearance, monitored accounts, reset-aware staleness, mutation ordering, error vocabulary, project rules, account groups, row actions, auto-switch start, sign-in outcomes.`);
