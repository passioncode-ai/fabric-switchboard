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

// SB-39: a failed row without an observation still says when the monitor checks it next —
// the time a provider's wait ends is part of it; rows the monitor never checks say nothing.
check(() => {
  const failed = { status: 'failed', checked_at: 1_000, next_check_at: 1_900 };
  assert.equal(logic.failedNextCheck({ enabled: true, kind: 'oauth', usage_health: failed }), 1_900);
  assert.equal(logic.failedNextCheck({ enabled: false, kind: 'oauth', usage_health: failed }), null);
  assert.equal(logic.failedNextCheck({ enabled: true, kind: 'api_key', usage_health: failed }), null);
  assert.equal(logic.failedNextCheck({ enabled: true, kind: 'oauth', usage_health: { ...failed, status: 'ok' } }), null);
  assert.equal(logic.failedNextCheck({ enabled: true, kind: 'oauth', usage_health: null }), null);
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

// Lifecycle LC-08: the sign-in poll runs only while a sign-in waits; without one, no timer wakes.
check(() => {
  const timers = new Map(); let next = 0;
  const fake = { set: (fn, ms) => { const id = ++next; timers.set(id, { fn, ms }); return id; }, clear: (id) => { timers.delete(id); } };
  let ticks = 0;
  const poll = logic.intervalWhile(() => { ticks += 1; }, 1500, fake);
  assert.equal(timers.size, 0, 'nothing pending: no timer at all');
  poll.sync(true); poll.sync(true);
  assert.equal(timers.size, 1, 'one timer however often render syncs');
  assert.equal([...timers.values()][0].ms, 1500);
  [...timers.values()][0].fn(); assert.equal(ticks, 1);
  poll.sync(false);
  assert.equal(timers.size, 0, 'the sign-in ended: the timer is cleared');
  assert.equal(poll.running, false);
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert(!/setInterval\(\(\) => \{ void pollLogin\(\); \}, 1500\)/.test(main), 'no permanent sign-in timer');
  assert(main.includes('loginPoll.sync('), 'render keeps the poll in step with the pending sign-in');
});


// SB-40: a metered feature's limit does not rank or block the account; account-wide blockers do;
// an observation with only feature windows is unknown, never zero.
check(() => {
  const now = 1_000_000;
  const row = (id, windows) => ({ id, label: id, provider: 'codex', pool: 'default', enabled: true, kind: 'oauth', created_at: 1,
    usage: { used_percent: Math.max(...windows.map(w => w[1])), observed_at: now - 10, resets_at: now + 3600, source: 'codex_oauth',
      windows: windows.map(([name, used_percent]) => ({ name, used_percent, resets_at: now + 3600 })) } });
  const feature = row('feature', [['primary', 30], ['feature_codex_other_primary', 100]]);
  assert.equal(logic.quotaOrder(feature, { nowSeconds: now }).state, 'available');
  assert.equal(logic.quotaOrder(feature, { nowSeconds: now }).used, 30);
  assert.equal(logic.accountUsedPercent(feature.usage), 30);
  const spend = row('spend', [['primary', 5], ['spend_limit', 100]]);
  assert.equal(logic.quotaOrder(spend, { nowSeconds: now }).state, 'blocked');
  const only = row('only', [['feature_codex_other_primary', 0]]);
  assert.equal(logic.quotaOrder(only, { nowSeconds: now }).state, 'unknown');
  assert.equal(logic.accountUsedPercent(only.usage), null);
  assert.equal(logic.accountUsedPercent({ used_percent: 12, observed_at: now, resets_at: null, source: 'claude_oauth' }), 12, 'pre-0.4 observation without windows');
  assert.equal(logic.featureWindowLabel('feature_codex_other_primary'), 'codex_other · primary feature limit');
  assert.equal(logic.featureWindowLabel('feature_x'), 'x · feature limit');
  assert.equal(logic.isFeatureWindow({ name: 'five_hour' }), false);
  // A feature's reset is neither the account's reset nor a reason to call the account stale.
  const soon = { ...feature.usage, resets_at: now + 900, windows: [
    { name: 'primary', used_percent: 30, resets_at: now + 7200 }, { name: 'feature_codex_other_primary', used_percent: 100, resets_at: now - 5 }] };
  assert.equal(logic.accountReset(soon), now + 7200);
  assert.equal(logic.usageFreshness(soon, 300, now).resetPassed, false);
  assert.equal(logic.quotaOrder({ ...feature, usage: soon }, { nowSeconds: now }).state, 'available');
  assert.equal(logic.accountReset(only.usage), null);
});

// SB-41: a provider-reported reset reads as a reset; an estimated hold never does.
check(() => {
  assert.equal(logic.limitLabel({ until: 100, resets_at: 100 }), 'Limit resets');
  assert.equal(logic.limitLabel({ until: 100, resets_at: null }), 'Retry hold until');
  assert.equal(logic.limitLabel({ until: 100 }), 'Retry hold until', 'an older owner reports no reset field');
  assert.equal(logic.limitLabel({ until: 200, resets_at: 100 }), 'Retry hold until', 'a longer hold is not the reset');
});

// Compact cards: a two-line card's wait and its at-a-glance state.
check(() => {
  const now = 1_000_000;
  assert.equal(logic.compactCountdown(now + 4 * 86400 + 22 * 3600 + 36 * 60, now), '4d 22h');
  assert.equal(logic.compactCountdown(now + 2 * 3600 + 5 * 60, now), '2h 5m');
  assert.equal(logic.compactCountdown(now + 36 * 60, now), '36m');
  assert.equal(logic.compactCountdown(now + 30, now), '<1m');
  assert.equal(logic.compactCountdown(now, now), 'now');
  assert.equal(logic.compactCountdown(NaN, now), '—');
  const base = { id: 'a', label: 'a', provider: 'claude', pool: 'p', enabled: true, kind: 'oauth', created_at: 1 };
  const usage = (used, extra = {}) => ({ used_percent: used, observed_at: now - 10, resets_at: now + 3600, source: 'claude_oauth', windows: [{ name: 'five_hour', used_percent: used, resets_at: now + 3600 }], ...extra });
  const ctx = { nowSeconds: now };
  assert.equal(logic.cardState({ ...base, usage: usage(30), usage_health: { status: 'ok', checked_at: now - 10, next_check_at: now + 170 } }, ctx), 'available');
  assert.equal(logic.cardState({ ...base, usage: usage(85) }, ctx), 'low');
  assert.equal(logic.cardState({ ...base, usage: usage(100) }, ctx), 'blocked');
  // SB-48: without a policy an observation stays current for 900 s; a policy's max age governs its pool.
  assert.equal(logic.quotaMaxAge(base), 900);
  assert.equal(logic.quotaMaxAge(base, [{ enabled: true, provider: 'claude', pool: 'p', max_age_seconds: 300 }]), 300);
  assert.equal(logic.cardState({ ...base, usage: usage(30, { observed_at: now - 600 }) }, ctx), 'available');
  assert.equal(logic.cardState({ ...base, usage: usage(30, { observed_at: now - 901 }) }, ctx), 'stale');
  assert.equal(logic.cardState({ ...base, usage: usage(30, { observed_at: now - 600 }) }, { nowSeconds: now, policies: [{ enabled: true, provider: 'claude', pool: 'p', max_age_seconds: 300 }] }), 'stale');
  assert.equal(logic.cardState({ ...base, usage: usage(30), usage_health: { status: 'failed', checked_at: now, next_check_at: now + 900 } }, ctx), 'failed');
  assert.equal(logic.cardState({ ...base, usage: null }, ctx), 'unknown');
  assert.equal(logic.cardState({ ...base, usage: null }, { nowSeconds: now, signInRequired: ['a'] }), 'sign_in');
  assert.equal(logic.cardState({ ...base, enabled: false, usage: usage(30) }, ctx), 'disabled');
  assert.equal(logic.cardState({ ...base, kind: 'api_key', usage: null }, ctx), 'no_quota');
  assert.equal(logic.cardState({ ...base, usage: usage(30) }, { nowSeconds: now, limits: [{ account_id: 'a', until: now + 600 }] }), 'blocked', 'a hold blocks');
});

// R3–R5: quota ordering is explicit, conservative and independent of input order.
check(() => {
  const now = 1_000_000;
  const account = (id, used, reset = now + 3600, extra = {}) => ({ id, label: id, provider: 'claude', pool: 'default', enabled: true, kind: 'oauth', created_at: 1, usage: used === null ? null : { used_percent: used, observed_at: now - 10, resets_at: reset }, ...extra });
  const a = account('available', 20), b = account('soon', 100, now + 60), c = account('late', 100, now + 7200);
  const d = account('unknown', null), e = account('disabled', 0, now + 1, { enabled: false });
  const bad = account('failed', 0, now + 1, { usage_health: { status: 'failed' } });
  const stale = account('stale', 0, now + 1, { usage: { used_percent: 0, observed_at: now - 1000, resets_at: now + 1 } });
  const result = logic.groupAccounts([e, c, stale, b, d, bad, a], { nowSeconds: now }).flatMap(g => g.pools.flatMap(p => p.accounts.map(a => a.id)));
  assert.deepEqual(result, ['available', 'soon', 'late', 'failed', 'stale', 'unknown', 'disabled']);
  assert.equal(logic.quotaOrder(account('reset', 0, now), { nowSeconds: now }).state, 'unknown');
  assert.equal(logic.quotaOrder(a, { nowSeconds: now, signInRequired: ['available'] }).state, 'sign_in');
  assert.equal(logic.quotaOrder(account('future', 0, now + 20, { usage: { ...a.usage, observed_at: now + 10 } }), { nowSeconds: now }).state, 'unknown');
  const multi = account('multi', 100, now + 60, { usage: { ...a.usage, used_percent: 100, resets_at: now + 60, windows: [{ name: 'session', used_percent: 100, resets_at: now + 60 }, { name: 'weekly', used_percent: 100, resets_at: now + 86400 }] } });
  assert.equal(logic.quotaOrder(multi, { nowSeconds: now }).until, now + 86400);
  assert.equal(logic.quotaOrder(multi, { nowSeconds: now, limits: [{ account_id: multi.id, until: now + 172800 }] }).until, now + 172800, 'later hold governs wait');
  const input = [a, b, c]; const before = structuredClone(input);
  logic.groupAccounts(input, { nowSeconds: now });
  assert.deepEqual(input, before, 'display sorting never mutates metadata');
  assert.deepEqual(logic.groupAccounts([...input].reverse(), { nowSeconds: now }), logic.groupAccounts(input, { nowSeconds: now }));
  multi.usage.windows[1].resets_at = null;
  assert.equal(logic.quotaOrder(multi, { nowSeconds: now }).until, null, 'missing blocking reset never means earliest known window');
  assert.equal(logic.quotaOrder(a, { nowSeconds: now, limits: [{ account_id: a.id, until: now + 900 }] }).state, 'blocked');
  assert.equal(logic.quotaOrder(a, { nowSeconds: now, limits: [{ account_id: a.id, until: now }] }).state, 'available');
  assert.equal(logic.quotaOrder(account('api', 0, now + 20, { kind: 'api_key' }), { nowSeconds: now }).state, 'unknown');
  assert.equal(logic.quotaOrder(account('nan', NaN), { nowSeconds: now }).state, 'unknown');
  assert.equal(logic.quotaOrder(a, { nowSeconds: now, policies: [{ provider: 'claude', pool: 'default', enabled: true, max_age_seconds: 5 }] }).state, 'unknown');
});
check(() => {
  const now = 1_000_000;
  assert.equal(logic.resetCountdown(now + 1, now), '<1m remaining');
  assert.equal(logic.resetCountdown(now + 3660, now), '1h 1m remaining');
  assert.equal(logic.resetCountdown(now + 172860, now), '2d 0h 1m remaining');
  assert.equal(logic.resetCountdown(now + 2592000, now), '30d 0h 0m remaining');
  assert.equal(logic.resetCountdown(now, now), 'Due · awaiting check');
  assert.equal(logic.resetCountdown(now - 50, now), 'Due · awaiting check');
  assert.equal(logic.resetCountdown(NaN, now), 'Time unavailable');
  assert.equal(logic.resetCountdown(9e12, now), 'Time unavailable');
});

// SB-55: what About says about automatic updates, and when "Restart to update" appears.
check(() => {
  const base = { available: true, reason: null, enabled: true, state: 'idle', current: '0.6.0', version: null, error: null, needs_permission: false };
  assert.deepEqual(logic.updateLine({ ...base, available: false, enabled: false, reason: 'Automatic updates work in the installed app only.' }), { text: 'Automatic updates work in the installed app only.', restart: false });
  assert.match(logic.updateLine(base).text, /0\.6\.0 is the latest/);
  assert.equal(logic.updateLine({ ...base, enabled: false }).text, 'Version 0.6.0. Automatic updates are off.');
  assert.equal(logic.updateLine({ ...base, state: 'downloading', version: '0.7.0' }).text, 'Downloading version 0.7.0…');
  assert.equal(logic.updateLine({ ...base, state: 'failed', error: 'Could not check for updates. Switchboard tries again within the hour.' }).text, 'Could not check for updates. Switchboard tries again within the hour.');
  const ready = logic.updateLine({ ...base, state: 'ready', version: '0.7.0' });
  assert.equal(ready.restart, true); assert.match(ready.text, /0\.7\.0 is ready/);
  // A ready update stays offered even after the switch is turned off: it is already installed.
  assert.equal(logic.updateLine({ ...base, enabled: false, state: 'ready', version: '0.7.0' }).restart, true);
  assert.match(logic.updateLine({ ...base, state: 'ready', version: '0.7.0', needs_permission: true }).text, /administrator password/);
});

console.log(`${cases} ui-logic cases passed, including quota priority and wall-clock countdowns.`);
