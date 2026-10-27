// Pure interface rules behind B-05, B-11, B-12 and the appearance setting.
import assert from 'node:assert/strict';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
// Compiles a source file and the relative modules it imports (i18n, the dictionaries) into data
// URLs, so the pure logic runs in Node exactly as the app bundles it.
const compiled = new Map();
const url = (path) => {
  const file = new URL(path, import.meta.url);
  const key = file.href;
  if (compiled.has(key)) return compiled.get(key);
  let source = readFileSync(file, 'utf8').replace(/^import type .*$/gm, '');
  source = source.replace(/from '(\.\.?\/[^']+)'/g, (_, rel) => `from '${url(new URL(`${rel}.ts`, file).href)}'`);
  const out = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  const data = `data:text/javascript;base64,${Buffer.from(out).toString('base64')}`;
  compiled.set(key, data);
  return data;
};
const load = async (path) => import(url(path));
const logic = await load('../src/ui-logic.ts');
let cases = 0;
const check = (fn) => { fn(); cases += 1; };

// SB-131: Change… puts the new folder where the old one was; a folder already listed is not doubled.
check(() => {
  assert.deepEqual(logic.replaceFolder(['/a', '/b', '/c'], '/b', '/x'), ['/a', '/x', '/c']);
  assert.deepEqual(logic.replaceFolder(['/a', '/b', '/c'], '/b', '/c'), ['/a', '/c']);
  assert.deepEqual(logic.replaceFolder(['/a'], '/a', ''), ['/a']);
  assert.deepEqual(logic.replaceFolder(['/a'], '/gone', '/x'), ['/a']);
});
// SB-122: the default pool first, projects by name; the CLI's own account cannot join a project.
check(() => {
  const projects = [{ pool: 'web', name: 'Web' }, { pool: 'agents', name: 'Agents' }];
  const targets = logic.moveTargets('web', projects, false);
  assert.deepEqual(targets.map((t) => t.pool), ['default', 'agents', 'web']);
  assert.deepEqual(targets.map((t) => t.current), [false, false, true]);
  assert.ok(targets.every((t) => !t.blocked));
  const inCli = logic.moveTargets('default', projects, true);
  assert.deepEqual(inCli.map((t) => t.blocked), [false, true, true]);
  assert.deepEqual(logic.moveTargets('gone', projects, false).filter((t) => t.current), [], 'a deleted project leaves no current option');
});
// SB-135: worktrees and clones are listed under their root; taking the root out takes them too.
check(() => {
  const roots = [{ folder: '/bot', branch: 'main', derived: [{ folder: '/wt/bot-card', kind: 'worktree', branch: 'card' }, { folder: '/bot-copy', kind: 'clone', branch: null }] }, { folder: '/web', branch: 'main', derived: [] }];
  const folders = ['/wt/bot-card', '/web', '/bot', '/bot-copy', '/new'];
  assert.deepEqual(logic.folderRows(folders, roots).map((r) => [r.folder, r.depth, r.derived]), [['/web', 0, 0], ['/bot', 0, 2], ['/wt/bot-card', 1, 0], ['/bot-copy', 1, 0], ['/new', 0, 0]]);
  assert.deepEqual(logic.folderRows(folders, roots, false).map((r) => r.folder), ['/web', '/bot', '/new'], 'closed: roots only');
  assert.deepEqual(logic.folderRows(folders, null).map((r) => r.depth), [0, 0, 0, 0, 0], 'no tree yet: the flat list');
  // A worktree whose root is not in the project stays a root of its own.
  assert.deepEqual(logic.folderRows(['/wt/bot-card'], roots).map((r) => r.depth), [0]);
  assert.deepEqual(logic.withoutFolder(folders, roots, '/bot'), ['/web', '/new']);
  // SB-135 part 2: the disk map's rows and where a repository can move.
  const map = { under: ['/d'], truncated: false, repositories: [
    { folder: '/d/app', remote: null, branch: 'main', pool: 'p', project: 'P', place: 'folder' },
    { folder: '/d/shared/tool', remote: null, branch: null, pool: 'p', project: 'P', place: 'inside' },
    { folder: '/d/loose', remote: null, branch: 'main', pool: null, project: null, place: null },
  ] };
  assert.deepEqual(logic.mapRows(map, 'unassigned').map((r) => [r.name, r.action]), [['loose', 'assign']]);
  assert.deepEqual(logic.mapRows(map, 'all').map((r) => r.action), ['move', null, 'assign'], 'inside a project folder: no action of its own');
  assert.deepEqual(logic.mapRows(null, 'all'), []);
  assert.deepEqual(logic.rootTargets('b', [{ pool: 'b', name: 'Beta' }, { pool: 'a', name: 'Alpha' }]), [{ pool: 'a', name: 'Alpha', current: false }, { pool: 'b', name: 'Beta', current: true }]);
  assert.deepEqual(logic.storedMapFolders({ getItem: () => '["/x", 3, ""]' }), ['/x']);
  assert.deepEqual(logic.storedMapFolders({ getItem: () => 'not json' }), []);
  assert.equal(logic.folderName('/srv/projects/.worktrees/alpha-web-wt-login'), 'alpha-web-wt-login');
  assert.equal(logic.folderName('C:\\Users\\me\\app\\'), 'app');
  assert.equal(logic.folderName('/'), '/');
  assert.deepEqual(logic.withoutFolder(folders, roots, '/bot-copy'), ['/wt/bot-card', '/web', '/bot', '/new']);
});
// SB-99/100: a Kimi window at 100 % stops the account until its reset; a passed reset no longer shows the old percent.
check(() => {
  const now = 1_000_000;
  assert.equal(logic.kimiLimitUntil([{ name: '5h', used_percent: 40, resets_at: now + 60 }], now), null);
  assert.equal(logic.kimiLimitUntil([{ name: '5h', used_percent: 100, resets_at: now + 600 }, { name: '7d', used_percent: 100, resets_at: now + 9000 }], now), now + 9000);
  assert.equal(logic.kimiLimitUntil([{ name: '5h', used_percent: 100, resets_at: now - 5 }], now), null, 'reset already');
  assert.equal(logic.kimiLimitUntil([{ name: 'month', used_percent: 100, resets_at: null }], now), 0);
  assert.equal(logic.kimiWindowsLine([{ name: '5h', used_percent: 100, resets_at: now - 5 }], now), '5 hours reset, refresh to see it');
});
// SB-100: the ordinary kimi gets its own words, never an account's.
check(() => {
  assert.equal(logic.ordinaryKimiNote({ signed_in: false, error: 'This account is signed out. Sign in to it again.' }), 'Not signed in. Run kimi login in Terminal.');
  assert.equal(logic.ordinaryKimiNote({ signed_in: true, error: 'Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.' }), 'Run kimi once in Terminal to renew its sign-in, then refresh.');
  assert.equal(logic.ordinaryKimiNote({ signed_in: true, error: null }), null);
});
// SB-98: a hidden limit says so first; the provider's reason reads in words.
check(() => {
  assert.equal(logic.limitTitle({ source: 'claude_code', hidden: true }), 'Hidden personal limit · refused although the usage figures show room');
  assert.equal(logic.limitTitle({ source: 'managed' }), 'Limit reached · a managed request was refused');
  assert.equal(logic.limitTitle({ source: 'claude_code', hidden: false }), 'Limit reached · reported by Claude Code');
  assert.match(logic.limitReasonText('org_spend_cap_reached'), /spend cap/);
  assert.equal(logic.limitReasonText('group_zero_credit_limit'), 'No credits are left for extra usage.');
  assert.equal(logic.limitReasonText('something_new'), 'Provider reason: something_new');
  assert.equal(logic.limitReasonText(null), null);
});
// SB-65: a session's state and how it runs.
check(() => {
  assert.equal(logic.sessionStateLabel('running'), 'Running');
  assert.equal(logic.sessionStateLabel('starting'), 'Starting');
  assert.equal(logic.sessionStateLabel('ended'), 'Ended');
  assert.equal(logic.sessionStateLabel('unknown'), 'State unknown');
  assert.equal(logic.sessionHow({ mode: 'managed', in_place: false, pool: 'work' }), 'Managed · follows the selection in group work (no folders) · in Terminal');
  assert.equal(logic.sessionHow({ mode: 'isolated', in_place: true, pool: 'default' }), 'Isolated · in an embedded terminal');
  assert.equal(logic.sessionHow({ mode: 'managed', in_place: false, pool: 'alpha' }, [{ pool: 'alpha', name: 'Alpha Web' }]), 'Managed · follows the selection in group “Alpha Web” · in Terminal', 'a group by its name (D-1)');
});
// SB-93: the Agents panel's setup facts — registration per agent, then whether an agent has called.
check(() => {
  const facts = logic.agentSetupFacts({ connected: { claude_code: true, codex: false }, proved: { client: 'claude-code', at: 1 } });
  assert.deepEqual(facts, [
    { text: 'Claude Code: Switchboard tools registered.', done: true },
    { text: 'Codex CLI: Switchboard tools not registered.', done: false },
    { text: 'An agent has called Switchboard: claude-code.', done: true },
  ]);
  assert.deepEqual(logic.agentSetupFacts({ proved: null }), [{ text: 'No agent has called Switchboard yet.', done: false }]);
  assert.equal(logic.agentSetupFacts({ connected: { claude_code: false, codex: false }, proved: { client: null, at: 1 } })[2].text, 'An agent has called Switchboard.');
});
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
  assert(adapter.includes(`  '${logic.LOGIN_FORGOTTEN}',`), `verbatim: ${logic.LOGIN_FORGOTTEN}`);
  assert(adapter.includes(`  '${logic.LOGIN_SAVED_CLEANUP}': '${logic.LOGIN_SAVED_CLEANUP_SHOWN}',`), 'the cleanup sentence is mapped to the one loginOutcome reads (CPY-05)');
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert(!main.includes(logic.LOGIN_FORGOTTEN) && !main.includes(logic.LOGIN_SAVED_CLEANUP), 'main.ts classifies sign-in errors through loginOutcome');
  assert.equal((main.match(/loginOutcome\(/g) ?? []).length, 4, 'status poll, finish, cancel and Try again (its cancel, SB-67) each classify');
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
  // SB-104: a manual check with automatic updates off still says how it went.
  assert.equal(logic.updateLine({ ...base, enabled: false, state: 'failed', error: 'Could not check for updates. Switchboard tries again within the hour.' }).text, 'Could not check for updates. Switchboard tries again within the hour.');
  assert.equal(logic.updateLine({ ...base, enabled: false, state: 'checking' }).text, 'Checking for updates…');
  assert.equal(logic.updateLine({ ...base, enabled: false, state: 'downloading', version: '0.7.0' }).text, 'Downloading version 0.7.0…');
  const ready = logic.updateLine({ ...base, state: 'ready', version: '0.7.0' });
  assert.equal(ready.restart, true); assert.match(ready.text, /0\.7\.0 is ready/);
  // A ready update stays offered even after the switch is turned off: it is already installed.
  assert.equal(logic.updateLine({ ...base, enabled: false, state: 'ready', version: '0.7.0' }).restart, true);
  assert.match(logic.updateLine({ ...base, state: 'ready', version: '0.7.0', needs_permission: true }).text, /administrator password/);
  // SB-208: during SB-191's six-hour pause the line says from when it starts on its own.
  const paused = logic.updateLine({ ...base, state: 'ready', version: '0.7.0', restart_after: 1_800_000_000 }, (s) => `at ${s}`);
  assert.equal(paused.restart, true);
  assert.equal(paused.text, 'Version 0.7.0 is ready. It starts on its own when Switchboard is idle in the background after at 1800000000 or opens again, or restart now.');
  assert.match(logic.updateLine({ ...base, state: 'ready', version: '0.7.0', restart_after: 1_800_000_000 }).text, /after 2027-01-15 08:00 or opens again/);
  assert.doesNotMatch(logic.updateLine({ ...base, state: 'ready', version: '0.7.0', restart_after: null }).text, /after/);
});

// SB-62: a sign-in of an identity already saved is announced as signed in again, never as added.
check(() => {
  assert.equal(logic.signInNotice({ label: 'Work A', pool: 'work' }, 'Claude Code'), 'Work A added to Claude Code in group work (no folders).');
  assert.equal(logic.signInNotice({ label: 'Work A', pool: 'default' }, 'Claude Code'), 'Work A added to Claude Code in the default group.');
  assert.equal(logic.signInNotice({ label: 'Work A', pool: 'alpha' }, 'Claude Code', [{ pool: 'alpha', name: 'Alpha Web' }]), 'Work A added to Claude Code in group “Alpha Web”.');
  assert.equal(logic.signInNotice({ label: 'Work A', pool: 'work', signed_in_again: true }, 'Claude Code'), 'Work A is signed in again in Claude Code in group work (no folders).');
  assert.match(logic.signInNotice({ label: 'Work A', pool: 'work', signed_in_again: true, login_cleanup: 'pending' }, 'Claude Code'), /^Work A is signed in again in Claude Code in group work \(no folders\)\. Switchboard could not remove its temporary sign-in folder yet/);
});

// SB-160 (B-4, SCN-077): signInNotice offers the organization's group a new seat landed outside of;
// the ordinary CLI's own account is told to switch first; a personal seat is offered nothing.
check(() => {
  const offer = { pool: 'acme-work', name: 'Acme work' };
  assert.equal(logic.signInNotice({ label: 'me@acme.com', pool: 'default', organization_group: offer }, 'Claude Code'), 'me@acme.com added to Claude Code in the default group.');
  assert.deepEqual(logic.signInMove({ id: 'a-1', label: 'me@acme.com', organization_group: offer }), { id: 'a-1', pool: 'acme-work', label: 'Move it to “Acme work”', done: 'me@acme.com moved to group “Acme work”.' });
  assert.equal(logic.signInMove({ id: 'a-1', label: 'me@acme.com' }), null, 'a personal seat: no offer');
  const inCli = { ...offer, in_cli: true };
  assert.equal(logic.signInMove({ id: 'a-1', label: 'me@acme.com', organization_group: inCli }), null, 'the CLI account cannot move yet');
  assert.equal(logic.signInMove({ id: 'a-1', label: 'me@acme.com', pool: 'default', login_cleanup: 'done', signed_in_again: false, organization_group: inCli }), null, 'a login finish receipt with in_cli (B-4): no move');
  assert.equal(logic.organizationMoveNote('me@acme.com', 'Claude Code', inCli), 'Switch Claude Code to another account first to move me@acme.com to group “Acme work”.');
  assert.equal(logic.organizationMoveNote('me@acme.com', 'Claude Code', offer), '');
  assert.equal(logic.organizationMoveNote('me@acme.com', 'Claude Code', undefined), '');
  assert.equal(logic.signInNotice({ label: 'me@acme.com', pool: 'default', organization_group: inCli }, 'Claude Code'), 'me@acme.com added to Claude Code in the default group. Switch Claude Code to another account first to move me@acme.com to group “Acme work”.');
});

// L10N-01…04 (fabric-workspace knowledge/localization.md): language, Russian plurals, placeholders.
const i18n = await load('../src/i18n.ts');
check(() => {
  assert.equal(i18n.detectLocale(['ru-RU', 'en-US']), 'ru');
  assert.equal(i18n.detectLocale(['ru']), 'ru');
  assert.equal(i18n.detectLocale(['en-US', 'ru-RU']), 'en', 'the first preferred language decides');
  assert.equal(i18n.detectLocale([]), 'en');
  assert.equal(i18n.detectLocale(undefined), 'en');
  assert.equal(i18n.parseLocaleChoice('ru'), 'ru');
  assert.equal(i18n.parseLocaleChoice('de'), 'system');
  assert.equal(i18n.parseLocaleChoice(null), 'system');
  assert.equal(i18n.resolveLocale('system', ['ru-RU']), 'ru');
  assert.equal(i18n.resolveLocale('en', ['ru-RU']), 'en');
});
check(() => {
  i18n.setLocale('en');
  assert.equal(i18n.t('Refresh'), 'Refresh');
  assert.equal(i18n.t('{label} enabled.', { label: 'Work' }), 'Work enabled.');
  assert.equal(i18n.plural(1, { one: '{n} account restored', other: '{n} accounts restored' }), '1 account restored');
  i18n.setLocale('ru');
  assert.equal(i18n.t('Refresh'), 'Обновить');
  assert.equal(i18n.t('{label} enabled.', { label: 'Work' }), 'Аккаунт Work включён.');
  assert.equal(i18n.t('A string no dictionary has'), 'A string no dictionary has', 'a missing entry shows English, never a key');
  const restored = (n) => i18n.plural(n, { one: '{n} account restored', other: '{n} accounts restored' });
  assert.deepEqual([1, 2, 5, 11, 21, 22, 25, 111].map(restored), [
    'Восстановлен 1 аккаунт', 'Восстановлено 2 аккаунта', 'Восстановлено 5 аккаунтов', 'Восстановлено 11 аккаунтов',
    'Восстановлен 21 аккаунт', 'Восстановлено 22 аккаунта', 'Восстановлено 25 аккаунтов', 'Восстановлено 111 аккаунтов']);
  // L10N-04: a backend refusal is compared in English and shown in Russian.
  assert.equal(logic.loginOutcome('Sign-in not found. Start again.'), 'forgotten');
  assert.equal(i18n.t('Sign-in not found. Start again.'), 'Вход не найден. Начните заново.');
  assert.match(logic.signInNotice({ label: 'Work', pool: 'work', signed_in_again: true }, 'Claude Code'), /^Вход в аккаунт Work выполнен снова \(Claude Code\) в группе work \(без папок\)\.$/);
  assert.equal(logic.signInMove({ id: 'a-1', label: 'Work', organization_group: { pool: 'acme-work', name: 'Acme work' } }).label, 'Перенести в «Acme work»');
  assert.match(logic.signInNotice({ label: 'Work', pool: 'default', organization_group: { pool: 'acme-work', name: 'Acme work', in_cli: true } }, 'Claude Code'), /Сначала переключите Claude Code на другой аккаунт, чтобы перенести Work в группу «Acme work»\.$/);
  i18n.setLocale('en');
});
// A backend sentence with a name in it is shown, and translated by its template (L10N-04).
const formatted = await load('../src/formatted-errors.ts');
check(() => {
  const text = 'Aider is not installed. Install it first, then launch it again.';
  assert.deepEqual(formatted.matchFormatted(text), { template: '{name} is not installed. Install it first, then launch it again.', name: 'Aider' });
  assert.equal(formatted.matchFormatted('a; rm -rf ~ is not installed. Install it first, then launch it again.'), null, 'only a plain name matches');
  assert.equal(formatted.matchFormatted('Something else.'), null);
  i18n.setLocale('ru');
  assert.equal(i18n.t(text), 'Aider не установлен. Сначала установите его, затем запустите снова.');
  i18n.setLocale('en');
  assert.equal(i18n.t(text), text);
});
// The journal names the core's fixed event vocabulary in the interface language; anything else shows as recorded.
check(() => {
  i18n.setLocale('en');
  assert.equal(logic.eventAction('account_added'), 'Account added');
  assert.equal(logic.eventDetail('rate_limited'), 'rate limited');
  assert.equal(logic.eventDetail('4xx'), 'HTTP 4xx');
  assert.equal(logic.eventAction('future_action'), 'Other event (future action)', 'an unknown action shows as recorded, framed (UX-26)');
  assert.equal(logic.eventDetail('some_new_outcome'), 'some new outcome');
  i18n.setLocale('ru');
  assert.equal(logic.eventAction('usage'), 'Проверка использования');
  assert.equal(logic.eventDetail('observed'), 'получено');
  assert.equal(logic.eventAction('rotation'), 'Автопереключение');
  assert.equal(logic.eventDetail('5xx'), 'HTTP 5xx');
  // Every action and outcome the core accepts has a label, so a new one cannot reach the journal
  // unnamed (checked in Russian, where a label always differs from the recorded word).
  const core = readFileSync(new URL('../crates/switchboard-core/src/lib.rs', import.meta.url), 'utf8');
  const body = core.slice(core.indexOf('pub(crate) fn event_valid'), core.indexOf('/// Two observations report the same quota'));
  const words = [...body.matchAll(/"([a-z_0-9]+)"/g)].map((m) => m[1]);
  assert.ok(words.length > 40, 'event_valid vocabulary found');
  for (const word of words) {
    if (/^[2-5]xx$/.test(word)) continue;
    const named = logic.eventAction(word) !== i18n.t('Other event ({code})', { code: word.replaceAll('_', ' ') }) || logic.eventDetail(word) !== word.replaceAll('_', ' ');
    assert.ok(named, `journal word without a label: ${word}`);
  }
  assert.match(logic.eventAction('brand_new'), /brand new/, 'an unknown word is framed, not dropped (UX-26)');
  i18n.setLocale('en');
});

// SB-79: the OpenRouter balance line shows dollars, the reset period in words, and no limit honestly.
check(() => {
  assert.equal(logic.openrouterCreditLine({ limit: 5, limit_remaining: 3.25, limit_reset: 'daily', usage_daily: 1.75 }), '$3.25 of $5.00 left, resets daily · spent today $1.75');
  assert.equal(logic.openrouterCreditLine({ limit: 5, limit_remaining: 3.25, limit_reset: null, usage_daily: null }), '$3.25 of $5.00 left · spent today unknown');
  assert.equal(logic.openrouterCreditLine({ limit: null, limit_remaining: null, limit_reset: null, usage_daily: 0.5 }), 'No spending limit · spent today $0.50');
  i18n.setLocale('ru');
  assert.equal(logic.openrouterCreditLine({ limit: 5, limit_remaining: 3.25, limit_reset: 'weekly', usage_daily: 0 }), 'Осталось $3.25 из $5.00, сброс еженедельно · сегодня потрачено $0.00');
  i18n.setLocale('en');
});

// SB-81: Kimi Code plan windows read as named shares with the time to their reset.
check(() => {
  const now = 1_800_000_000;
  assert.equal(logic.kimiWindowsLine([{ name: '5h', used_percent: 25.4, resets_at: now + 3 * 3600 + 600 }, { name: '7d', used_percent: 50, resets_at: null }, { name: 'month', used_percent: 100, resets_at: now - 5 }], now), '5 hours 25%, resets in 3h 10m · 7 days 50% · Month reset, refresh to see it', 'a passed reset never shows the old share (SB-100)');
  assert.equal(logic.kimiWindowsLine([], now), '');
  i18n.setLocale('ru');
  assert.equal(logic.kimiWindowsLine([{ name: '5h', used_percent: 10, resets_at: now + 1800 }], now), '5 часов 10%, сброс через 30 мин');
  i18n.setLocale('en');
});

// SCN-049: the provider terms notice — a banner on Accounts from the first start (so it is in sight
// while the first account is added) until dismissed, remembered across starts; always in About.
check(() => {
  const memory = () => { const data = new Map(); return { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => { data.set(k, String(v)); } }; };
  const store = memory();
  assert.equal(logic.termsNoticeDismissed(store), false, 'first start: not dismissed');
  assert.equal(logic.termsNoticeOn('accounts', logic.termsNoticeDismissed(store)), 'banner', 'first start: banner on Accounts');
  assert.equal(logic.termsNoticeOn('about', logic.termsNoticeDismissed(store)), 'panel');
  for (const page of ['projects', 'agents', 'activity']) assert.equal(logic.termsNoticeOn(page, false), null, `${page} never shows it`);
  assert.equal(logic.dismissTermsNotice(store), true);
  assert.equal(store.getItem(logic.TERMS_NOTICE_KEY), '1');
  // The next start reads the same store: the banner stays dismissed, About keeps the panel.
  assert.equal(logic.termsNoticeDismissed(store), true, 'dismissal remembered');
  assert.equal(logic.termsNoticeOn('accounts', true), null);
  assert.equal(logic.termsNoticeOn('about', true), 'panel', 'About shows it permanently');
  // A store that refuses counts as not dismissed: the banner returns next start rather than never.
  const refusing = { getItem: () => { throw new Error('denied'); }, setItem: () => { throw new Error('denied'); } };
  assert.equal(logic.termsNoticeDismissed(refusing), false);
  assert.equal(logic.dismissTermsNotice(refusing), false);
  assert.equal(logic.termsNoticeDismissed(null), false);
  assert.equal(logic.dismissTermsNotice(null), false);
});
check(() => {
  assert.equal(logic.termsNoticeText(), "Use Switchboard in line with the terms of Anthropic and OpenAI. Breaking a provider's terms can get your account blocked. You are responsible for how you use your accounts.");
  i18n.setLocale('ru');
  assert.equal(logic.termsNoticeText(), 'Пользуйтесь Switchboard в соответствии с условиями использования Anthropic и OpenAI. Нарушение условий провайдера может привести к блокировке аккаунта. Ответственность за то, как вы используете свои аккаунты, несёте вы.');
  i18n.setLocale('en');
});
// The wiring: Accounts draws the banner first in every state it renders, including the empty
// first-account state; About draws the panel; the banner's Dismiss remembers the choice.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.match(main, /function renderAccounts\(main: HTMLElement\) \{\n(?:  \/\/[^\n]*\n)?  const status = el\('section', 'status-stack'\);[^\n]*\n  termsBanner\(status\);/, 'renderAccounts starts with the banner, in the status block (VIS-13)');
  assert.match(main, /function renderAbout\(main: HTMLElement\) \{[\s\S]*?main\.append\(section, termsPanel\(\),/, 'About carries the panel');
  assert.match(main, /termsNoticeOn\(page, termsDismissed\) !== 'banner'/, 'the banner follows termsNoticeOn');
  assert.match(main, /dismissTermsNotice\(noticeStore\(\)\); termsDismissed = true;/, 'Dismiss stores the choice');
  assert.match(main, /let termsDismissed = termsNoticeDismissed\(noticeStore\(\)\);/, 'the stored choice is read at start');
});

// Issue #128 (SCN-073, SCN-044): nothing is sent before the person answers the usage switch once.
// The window asks only when the build can send, the switch is on and this app has no answer; the
// tour waits for the question (and for the status read) so one first-run card shows at a time.
check(() => {
  const ask = { available: true, enabled: true, disclosed: false, sent_before: false };
  assert.equal(logic.usageQuestion(ask), true, 'first start of a release build: asked');
  assert.equal(logic.usageQuestion({ ...ask, disclosed: true }), false, 'answered: never asked again');
  assert.equal(logic.usageQuestion({ ...ask, enabled: false }), false, 'switched off (here or by another app): nothing to ask');
  assert.equal(logic.usageQuestion({ ...ask, available: false }), false, 'a development build sends nothing and asks nothing');
  assert.equal(logic.usageQuestion({ available: true, enabled: true }), false, 'an older backend without the field: no question');
  assert.equal(logic.usageQuestion(null), false, 'unread or unreadable: no question');
  assert.equal(logic.usageQuestionText(ask), 'Nothing has been sent yet. Nothing is sent until you answer.');
  assert.equal(logic.usageQuestionText({ ...ask, sent_before: true }), 'An earlier version of Switchboard may already have sent these counts from this computer. Nothing more is sent until you answer.', 'an upgraded install is not told nothing was sent');
  assert.equal(logic.tourWaits(null, false), true, 'the tour waits for the status read');
  assert.equal(logic.tourWaits(null, true), false, 'a failed read never holds the tour');
  assert.equal(logic.tourWaits(ask, false), true, 'the question comes first');
  assert.equal(logic.tourWaits({ ...ask, disclosed: true }, false), false, 'answered: the tour shows');
  // Round 1: an "on" answered after another tool turned the shared switch off stays off, and says so.
  assert.equal(logic.usageAnswerText(true, { ...ask, enabled: true, disclosed: true }), 'Anonymous usage analytics are on.');
  assert.equal(logic.usageAnswerText(true, { ...ask, enabled: false, disclosed: true }), 'Usage counts stay off: another PassionCode.ai tool on this computer turned them off. Switch them on in About to count again.');
  assert.equal(logic.usageAnswerText(false, { ...ask, enabled: false, disclosed: true }), 'Anonymous usage analytics are off for every PassionCode.ai tool on this computer.');
  i18n.setLocale('ru');
  assert.equal(logic.usageQuestionText(ask), 'Пока ничего не отправлено. До вашего ответа ничего не отправится.');
  i18n.setLocale('en');
});
// The wiring: the question sits above every page but About, where the panel asks the same thing;
// both answer through setAnalytics with the one choice; Escape never ends a tour that is not shown.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.match(main, /if \(page !== 'about' && usageQuestion\(analyticsState\)\) main\.append\(usageQuestionCard\(\)\);/, 'the question above the page');
  const about = main.slice(main.indexOf('function analyticsPanel() {'), main.indexOf('function backupsPanel() {'));
  assert.match(about, /if \(usageQuestion\(analyticsState\)\) \{/, 'About asks while unanswered');
  assert.match(about, /usageQuestionText\(analyticsState\)/, 'with the same line on what was sent');
  assert.match(about, /answerUsage\('analytics', shown\)/, 'and Continue answers it, with the switch it showed');
  assert.match(main, /analyticsState = await adapter\.setAnalytics\(wanted\);/, 'the answer goes to the backend');
  assert.match(main, /function tourShown\(\): boolean \{ return tourStep !== null && \(native \|\| demo\) && !tourWaits\(analyticsState, analyticsError\); \}/, 'the tour waits for the question');
  assert.match(main, /event\.key === 'Escape' && !openMenu && tourShown\(\)/, 'Escape ends only a tour on screen');
  // Round 1: the status is read again in the background (every minute and when the window is shown),
  // so a question drawn before another tool turned the switch off is dropped, not answered with a stale "on".
  const refresh = main.slice(main.indexOf('function backgroundRefresh() {'), main.indexOf('setInterval(backgroundRefresh, 60_000);'));
  assert.match(refresh, /adapter\.analytics\(\)\.then\(\(value\) => \{ if \(clock\.accepts\(stamp\)\) \{ analyticsState = value;/, 'backgroundRefresh reads the analytics status');
  assert.match(main, /document\.addEventListener\('visibilitychange', \(\) => \{ if \(!document\.hidden\) backgroundRefresh\(\); \}\);/, 'and so does showing the window');
  assert.match(main, /\(\) => usageAnswerText\(wanted, analyticsState\)/, 'the answer reports what is now true');
});

// Launch (operator decision 2026-10-09): managed only for the pool's selected account while the
// proxy runs; every other launch is isolated. Folders come from the picker, each once.
check(() => {
  assert.deepEqual(logic.launchChoice(true, true), { managed: true, isolatedByDefault: false });
  assert.deepEqual(logic.launchChoice(false, true), { managed: false, isolatedByDefault: true });
  assert.deepEqual(logic.launchChoice(true, false), { managed: false, isolatedByDefault: true });
  assert.deepEqual(logic.withFolder(['/a'], '/b'), ['/a', '/b']);
  assert.deepEqual(logic.withFolder(['/a', '/b'], '/a'), ['/a', '/b']);
  assert.deepEqual(logic.withFolder(['/a'], ''), ['/a']);
  assert.equal(logic.storedFolder(null), '');
  assert.equal(logic.storedFolder({ getItem: () => { throw new Error('blocked'); } }), '');
  assert.equal(logic.storedFolder({ getItem: () => ' /Users/a/p ' }), '/Users/a/p');
});
// No folder is typed anywhere: the dialogs use the picker, never a path input or a textarea.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(main, /projectPathExample/, 'a typed-path placeholder is back');
  assert.doesNotMatch(main, /el\('textarea'\)[^\n]*folders/, 'project folders are typed again');
  for (const dialog of ['function launchDialog', 'function kimiLaunchDialog', 'function ruleDialog', 'function projectDialog', 'function agentDialog']) {
    const start = main.indexOf(dialog); assert.ok(start >= 0, `${dialog} exists`);
    const body = main.slice(start, main.indexOf('\nfunction ', start + 1));
    assert.match(body, /folderPicker|foldersPicker/, `${dialog} picks its folder`);
  }
});

// SB-120: agents that take over work come first, in chain order; a stale default stays visible.
check(() => {
  const rows = [
    { id: 'hermes', name: 'Hermes Agent', enabled: true, switchable: true, installed: true },
    { id: 'kilo', name: 'Kilo Code', enabled: false, switchable: false, installed: true },
    { id: 'claude-code', name: 'Claude Code', enabled: true, switchable: true, installed: true },
    { id: 'goose', name: 'Goose', enabled: false, switchable: false, installed: true },
    { id: 'kimi-code', name: 'Kimi Code', enabled: false, switchable: true, installed: true },
    { id: 'opencode', name: 'OpenCode', enabled: false, switchable: true, installed: false },
  ];
  assert.deepEqual(logic.sortAgents(rows).map((r) => r.id), ['claude-code', 'kimi-code', 'hermes', 'opencode', 'kilo', 'goose']);
  assert.deepEqual(logic.defaultAgentChoices(rows, null).map((c) => [c.id, c.on]), [[null, true], ['claude-code', false], ['hermes', false]]);
  assert.deepEqual(logic.defaultAgentChoices(rows, 'opencode').map((c) => [c.id, c.label, c.on]), [[null, 'Auto', false], ['claude-code', 'Claude Code', false], ['hermes', 'Hermes Agent', false], ['opencode', 'OpenCode (not installed)', true]]);
  assert.equal(logic.defaultAgentChoices(rows, 'kimi-code').find((c) => c.on).label, 'Kimi Code (disabled)');
  assert.equal(logic.tokenCount(1_050_000), '1.05M');
  assert.equal(logic.tokenCount(10_000_000), '10M');
  assert.equal(logic.tokenCount(1_000_000), '1M');
  assert.equal(logic.tokenCount(200_000), '200K');
  assert.equal(logic.modelMeta({ maker: 'DeepSeek', context: 1_048_576, input_usd_per_million: 0.04 }), 'DeepSeek · 1.05M context · $0.04 per 1M input tokens');
});
// SB-145: Statistics counts as people read them; tokens are every kind the provider reported.
check(() => {
  assert.equal(logic.compactCount(950), '950');
  assert.equal(logic.compactCount(12_400), '12.4K');
  assert.equal(logic.compactCount(3_210_000), '3.21M');
  const c = { requests: 8, ok: 6, limited: 1, failed: 1, input_tokens: 10, output_tokens: 5, cache_read_tokens: 100, cache_write_tokens: 2 };
  assert.equal(logic.statTokens(c), 117);
  assert.equal(logic.statTrouble(c), 25);
  assert.equal(logic.statTrouble({ ...c, requests: 0 }), null, 'no requests is no share, not 0%');
  assert.equal(logic.statTroubleText({ ...c, requests: 1000 }), '<1%', 'two failures in a thousand are not 0%');
  assert.equal(logic.statTroubleText(c), '25%');
  assert.equal(logic.statAgentName('codex'), 'Codex CLI');
  assert.equal(logic.statAgentName('something-new'), 'Other client');
  // SB-201: a session names its agent; an agent through the proxy records provider claude.
  assert.equal(logic.sessionAgentName({ agent: 'goose', provider: 'claude' }), 'Goose');
  assert.equal(logic.sessionAgentName({ agent: 'codex', provider: 'claude' }), 'Codex CLI');
  assert.equal(logic.sessionAgentName({ agent: 'kimi-code', provider: 'kimi' }), 'Kimi Code');
  assert.equal(logic.sessionAgentName({ agent: 'deepseek-harness', provider: 'claude' }), 'deepseek-harness', 'an unnamed agent by its id');
  assert.equal(logic.sessionAgentName({ provider: 'claude' }), 'Claude Code', 'an older record without agent');
  assert.equal(logic.sessionAgentName({ agent: null, provider: 'hermes' }), 'Hermes Agent');
  assert.equal(logic.sessionAgentName({ provider: 'crush' }), 'Crush');
});
// SB-146: a group's projects are its repositories by name: no worktree, clone or subfolder, one per origin.
check(() => {
  const folders = ['/d/app', '/d/app/docs', '/w/app-wt-login', '/d/api', '/d/api-copy', '/d/loose'];
  const roots = [
    { folder: '/d/app', branch: 'main', remote: 'github.com/o/app', derived: [{ folder: '/w/app-wt-login', kind: 'worktree', branch: 'login' }] },
    { folder: '/d/api', branch: 'main', remote: 'github.com/o/api', derived: [] },
    { folder: '/d/api-copy', branch: 'main', remote: 'github.com/o/api', derived: [] },
  ];
  assert.deepEqual(logic.groupProjects(folders, roots), ['api', 'app', 'loose']);
  assert.deepEqual(logic.groupProjects(['/d/x', '/d/x/sub'], null), ['x'], 'before Git is read: folder names, subfolders left out');
});
// SB-147: the daily chart fills days without traffic with 0 and sums groups past the top into one series.
check(() => {
  const stats = { from: '2026-10-18', to: '2026-10-20', by_day: [
    { day: '2026-10-20', groups: [{ pool: 'a', requests: 2, tokens: 50 }, { pool: 'b', requests: 1, tokens: 5 }, { pool: 'c', requests: 1, tokens: 1 }] },
    { day: '2026-10-18', groups: [{ pool: 'a', requests: 1, tokens: 10 }] },
  ] };
  const { days, series } = logic.chartSeries(stats, 'tokens', (p) => p.toUpperCase(), 2);
  assert.deepEqual(days, ['2026-10-18', '2026-10-19', '2026-10-20']);
  assert.deepEqual(series.map((x) => [x.name, x.values]), [['A', [10, 0, 50]], ['B', [0, 0, 5]], ['Other groups', [0, 0, 1]]]);
  assert.deepEqual(logic.chartSeries(stats, 'requests', (p) => p, 5).series[0].values, [1, 0, 2]);
  assert.equal(logic.niceMax(0), 1); assert.equal(logic.niceMax(7), 10); assert.equal(logic.niceMax(130), 200); assert.equal(logic.niceMax(4_100_000), 5_000_000);
  assert.equal(logic.statCacheShare({ input_tokens: 10, cache_read_tokens: 80, cache_write_tokens: 10, output_tokens: 5, requests: 1 }), 80);
  assert.equal(logic.statCacheShare({ input_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 }), null);
  assert.equal(logic.statTokensPerRequest({ input_tokens: 10, cache_read_tokens: 80, cache_write_tokens: 10, output_tokens: 100, requests: 4 }), 50);
});
// SB-147: a group's folders by project: a repository with its worktree and a clone of one origin is one project; a plain folder is its own.
check(() => {
  const folders = ['/d/app', '/w/app-wt', '/d/app-copy', '/d/notes'];
  const roots = [
    { folder: '/d/app', branch: 'main', remote: 'github.com/o/app', derived: [{ folder: '/w/app-wt', kind: 'worktree', branch: 'x' }] },
    { folder: '/d/app-copy', branch: 'main', remote: 'github.com/o/app', derived: [] },
  ];
  const projects = logic.groupProjectRows(folders, roots);
  assert.deepEqual(projects.map((p) => [p.name, p.rows.map((r) => r.folder)]), [['app', ['/d/app', '/w/app-wt', '/d/app-copy']], ['notes', ['/d/notes']]]);
});
// SB-153: a click on a column title sorts the table; billions read as B.
check(() => {
  assert.equal(logic.compactCount(3_045_860_000), '3.05B');
  const z = { requests: 0, ok: 0, limited: 0, failed: 0, input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 };
  const rows = [
    { cells: ['Agents'], counts: { ...z, requests: 6600, input_tokens: 100, limited: 405 } },
    { cells: ['PassionCode'], counts: { ...z, requests: 4700, input_tokens: 900, limited: 247 } },
    { cells: [['Apps & Soft', 'note']], counts: { ...z, requests: 619, input_tokens: 50, limited: 42 } },
  ];
  assert.equal(logic.sortStatRows(rows, null, true), rows, 'no sort keeps the order');
  assert.deepEqual(logic.sortStatRows(rows, { column: 0, descending: false }, true).map((r) => r.counts.requests), [6600, 619, 4700], 'by name (Agents, Apps & Soft, PassionCode), the note ignored');
  assert.deepEqual(logic.sortStatRows(rows, { column: 2, descending: true }, true).map((r) => r.counts.requests), [4700, 6600, 619], 'by tokens');
  assert.deepEqual(logic.sortStatRows(rows, { column: 1, descending: false }, true).map((r) => r.counts.requests), [619, 4700, 6600]);
  assert.deepEqual(logic.nextStatSort(null, 1, true), { column: 1, descending: true });
  assert.deepEqual(logic.nextStatSort({ column: 1, descending: true }, 1, true), { column: 1, descending: false });
  assert.deepEqual(logic.nextStatSort({ column: 1, descending: true }, 0, false), { column: 0, descending: false });
});
// SB-155: a project is named by its folder (from Git), not its remote; equal names get their owner.
check(() => {
  const folders = ['/d/project-observatory-open-source', '/d/research-agent', '/d/0xDEV/research-agent', '/d/_worktrees/asset-foundry-w3'];
  const roots = [
    { folder: '/d/project-observatory-open-source', branch: 'main', remote: 'github.com/passioncode-ai/project-observatory-dashboard', name: 'project-observatory-open-source', derived: [] },
    { folder: '/d/research-agent', branch: 'main', remote: 'github.com/ssheleg/research-agent', name: 'research-agent', derived: [] },
    { folder: '/d/0xDEV/research-agent', branch: 'main', remote: 'github.com/prowl-ai/research-agent', name: 'research-agent', derived: [] },
    { folder: '/d/_worktrees/asset-foundry-w3', branch: 'w3', remote: 'github.com/ssheleg/asset-foundry', name: 'asset-foundry', derived: [] },
  ];
  assert.deepEqual(logic.groupProjectRows(folders, roots).map((p) => p.name), ['project-observatory-open-source', 'research-agent (ssheleg)', 'research-agent (prowl-ai)', 'asset-foundry']);
  assert.deepEqual(logic.groupProjects(folders, roots), ['asset-foundry', 'project-observatory-open-source', 'research-agent (prowl-ai)', 'research-agent (ssheleg)']);
});
// SB-158: the same attribution order as Snapshot::attribution: the person's word, the team, the domain.
check(() => {
  const account = (id, provider, email, org) => ({ id, label: email, provider, external_identity: { account_id: org, organization_id: org, email } });
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }, { id: 'o2', name: 'Beta', domains: [], provider_organizations: ['claude:team-b'] }], memberships: {} };
  assert.deepEqual(logic.attribution({}, account('a', 'claude', 'me@acme.example', 'x')), { organization: null, by: null });
  assert.deepEqual(logic.attribution(snap, account('a', 'claude', 'dev@acme.example', 'team-x')), { organization: 'o1', by: 'domain' });
  assert.deepEqual(logic.attribution(snap, account('b', 'claude', 'dev@acme.example', 'team-b')), { organization: 'o2', by: 'provider' });
  snap.memberships.b = 'personal';
  assert.deepEqual(logic.attribution(snap, account('b', 'claude', 'dev@acme.example', 'team-b')), { organization: null, by: 'person' });
  snap.memberships.a = 'o2';
  assert.deepEqual(logic.attribution(snap, account('a', 'claude', 'dev@acme.example', 'team-x')), { organization: 'o2', by: 'person' });
  assert.equal(logic.providerOrganization(account('c', 'codex', 'x@gmail.com', 'ws-1')), 'codex:ws-1');
  assert.equal(logic.accountDomain({ label: 'Work', external_identity: null }), null);
  assert.equal(logic.accountDomain({ label: 'x@Acme.Example.', external_identity: null }), 'acme.example');
  assert.equal(logic.inOrganizationFilter(snap, account('a', 'claude', 'dev@acme.example', 'team-x'), 'o2'), true);
  assert.equal(logic.inOrganizationFilter(snap, account('b', 'claude', 'dev@acme.example', 'team-b'), 'personal'), true);
  assert.deepEqual(logic.domainList(' @Acme.example, beta.example\nacme.example '), ['acme.example', 'beta.example']);
});
// SB-109: a group takes accounts of its own organization, or personal ones; no organization, no rule.
check(() => {
  const account = (id, email) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: null, email } });
  const work = account('w', 'dev@acme.example'); const home = account('h', 'me@home.example');
  assert.equal(logic.groupAdmits({}, null, work), true);
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: {} };
  assert.equal(logic.groupAdmits(snap, 'o1', work), true);
  assert.equal(logic.groupAdmits(snap, null, work), false);
  assert.equal(logic.groupAdmits(snap, null, home), true);
  assert.equal(logic.groupAdmits(snap, 'o1', home), false);
  snap.memberships.w = 'personal';
  assert.equal(logic.groupAdmits(snap, null, work), true);
  // SB-170: a group of Acme whose only account was placed as personal says so on its card.
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', [work]), true);
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', [work, account('w2', 'ops@acme.example')]), false);
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', []), false, 'an empty group has its own note');
  assert.equal(logic.groupSeatsElsewhere(snap, null, [work]), false);
});
// SB-160 (B-6, SCN-078): an ended seat of an organization names it.
check(() => {
  const account = (id, email) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: null, email } });
  const work = account('w', 'dev@acme.example'); const home = account('h', 'me@home.example');
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: {} };
  assert.equal(logic.seatEndedText(snap, work, ['w']), "dev@acme.example's sign-in has ended. If Acme removed your seat, ask its administrator; to keep using it, sign in again.", 'an ended seat of an organization names it');
  assert.equal(logic.seatEndedText(snap, work, []), null, 'a signed-in seat says nothing');
  assert.equal(logic.seatEndedText(snap, work, undefined), null, 'no monitor read yet: nothing');
  assert.equal(logic.seatEndedText(snap, home, ['h']), null, 'a personal account keeps today\'s text');
  assert.equal(logic.seatEndedText({}, work, ['w']), null, 'no organizations: today\'s text');
  snap.memberships.w = 'personal';
  assert.equal(logic.seatEndedText(snap, work, ['w']), null, 'placed as personal: today\'s text');
});
// SB-160 (B-6, SCN-078): a group whose seats all ended says so.
check(() => {
  const account = (id, email) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: null, email } });
  const a = account('a', 'dev@acme.example'); const b = account('b', 'ops@acme.example'); const home = account('h', 'me@home.example');
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: {} };
  const group = { name: 'Acme web', organization: 'o1' };
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], ['a', 'b']), "None of Acme web's accounts is signed in. Sessions in its folders are refused until one is.", 'a group whose seats all ended says so');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], ['a']), null, 'one seat still signed in');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b, home], ['a', 'b']), "None of Acme web's accounts is signed in. Sessions in its folders are refused until one is.", 'an account of another context is refused anyway');
  assert.equal(logic.groupSeatsEndedText(snap, group, [home], ['h']), null, 'every account elsewhere: SB-170\'s line instead');
  assert.equal(logic.groupSeatsEndedText(snap, group, [], []), null, 'an empty group has its own note');
  assert.equal(logic.groupSeatsEndedText(snap, { name: 'Mine', organization: null }, [home], ['h']), null, 'a personal group: no seats');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], undefined), null);
});
// SB-160 (B-7, SCN-079): an organization left on PassionCode.ai is marked with its date until it is linked again.
check(() => {
  const status = { connected: true, released: [{ organization: 'o1', at: 1000 }] };
  assert.equal(logic.organizationReleasedAt(status, { id: 'o1', vesta_id: null }), 1000);
  assert.equal(logic.organizationReleasedAt(status, { id: 'o1', vesta_id: 'v-acme' }), null, 'linked again');
  assert.equal(logic.organizationReleasedAt(status, { id: 'o2', vesta_id: null }), null);
  // SB-227: the minute's background pass reads the PassionCode.ai status again while Organizations
  // is on screen (a local file read), so a mark a background read made shows without reopening it.
  assert.deepEqual(logic.backgroundReads('orgs'), ['onboarding', 'passioncode']);
  assert.deepEqual(logic.backgroundReads('accounts'), ['onboarding']);
  assert.deepEqual(logic.backgroundReads('activity'), ['sessions']);
  assert.deepEqual(logic.backgroundReads('agents'), ['ceilings']);
  assert.deepEqual(logic.backgroundReads('tokens'), ['ceilings']);
  assert.deepEqual(logic.backgroundReads('stats'), []);
  assert.equal(logic.organizationReleasedAt({ connected: false }, { id: 'o1' }), null, 'disconnected: no line');
  assert.equal(logic.organizationReleasedAt(null, { id: 'o1' }), null);
});
// SB-159: the PassionCode.ai panel's sentence for each state.
check(() => {
  assert.match(logic.passioncodeLine(null).key, /^Not connected/);
  assert.match(logic.passioncodeLine({ connected: false }).key, /^Not connected/);
  assert.deepEqual(logic.passioncodeLine({ connected: true, state: 'ok', organizations: 2, read_at: 100 }), { key: 'Connected: {organizations}, read {date}.', values: { count: 2, date: 100 } });
  assert.match(logic.passioncodeLine({ connected: true, state: 'sign_in_again' }).key, /again/);
  assert.match(logic.passioncodeLine({ connected: true, state: 'provider_sign_in' }).key, /in your browser/);
  assert.deepEqual(logic.passioncodeLine({ connected: true, state: 'unreachable', read_at: 5 }).values, { date: 5 });
  assert.match(logic.passioncodeLine({ connected: true, state: 'unreachable', read_at: null }).key, /yet/);
});
// VIS-03: the pool pattern compiles under the v flag WebKit applies to `pattern`, and still says what it said.
check(() => {
  const exact = new RegExp(`^(?:${logic.POOL_PATTERN})$`, 'v');
  for (const ok of ['default', 'work', 'a-b_1', 'client-alpha']) assert.ok(exact.test(ok), ok);
  for (const bad of ['Work', 'a b', '', 'é', 'a/b']) assert.ok(!exact.test(bad), bad);
});
// UX-17 (D-1): the group field offers existing pools by name, the default first, and never a pool the person never made.
check(() => {
  const accounts = [{ pool: 'work' }, { pool: 'default' }, { pool: 'alpha' }, { pool: 'work' }];
  const projects = [{ pool: 'alpha', name: 'Alpha Web' }, { pool: 'zeta', name: 'Beta' }];
  const choices = logic.poolChoices(accounts, projects);
  assert.deepEqual(choices.map((c) => c.pool), ['default', 'alpha', 'zeta', 'work']);
  assert.deepEqual(choices.map((c) => c.group), [false, true, true, false]);
  assert.equal(choices[1].name, 'Alpha Web');
  assert.deepEqual(logic.poolChoices([], [], 'legacy').map((c) => c.pool), ['default', 'legacy'], 'the current pool stays offered');
  assert.equal(logic.poolName('alpha', projects), 'Alpha Web');
  assert.equal(logic.poolName('default', projects), 'Default group');
  assert.equal(logic.poolName('work', projects), 'Group work · no folders', 'a pool with no group is never a bare id (D-5)');
  assert.equal(logic.inGroup('default', projects), 'in the default group');
  assert.equal(logic.inGroup('alpha', projects), 'in group “Alpha Web”');
  assert.equal(logic.inGroup('work', projects), 'in group work (no folders)');
  // Statistics names: a group by its name, the default pool as the default group, never "No group".
  assert.equal(logic.poolName('default', [{ pool: 'default', name: null }].filter((g) => g.name)), 'Default group');
});
// VIS-12: the tour card sits beside its target, never on it, and inside the window.
check(() => {
  const viewport = { width: 740, height: 560 }; const card = { width: 320, height: 200 };
  const overlaps = (a, b) => a.left < b.left + b.width && b.left < a.left + a.width && a.top < b.top + b.height && b.top < a.top + a.height;
  for (const target of [{ left: 400, top: 40, width: 200, height: 40 }, { left: 30, top: 420, width: 300, height: 60 }, { left: 500, top: 250, width: 200, height: 40 }]) {
    const at = logic.tourPlacement(target, card, viewport);
    assert.ok(!overlaps({ ...at, ...card }, target), JSON.stringify({ target, at }));
    assert.ok(at.left >= 0 && at.top >= 0 && at.left + card.width <= viewport.width && at.top + card.height <= viewport.height, JSON.stringify(at));
  }
  assert.equal(logic.tourPlacement(null, card, viewport).side, 'corner', 'a step without a target keeps the corner');
});
// UX-18: the first-run sentence points at a button that is on the page.
check(() => {
  assert.equal(logic.firstRunHint(null), 'reading');
  assert.equal(logic.firstRunHint({ claude: { status: 'missing' }, codex: { status: 'missing' } }), 'sign_in');
  assert.equal(logic.firstRunHint({ claude: { status: 'unavailable' }, codex: { status: 'missing' } }), 'sign_in');
  assert.equal(logic.firstRunHint({ claude: { status: 'available', identity: { email: 'a@b' } }, codex: { status: 'missing' } }), 'add_current');
});
// CPY-05: the window shows the rewritten cleanup sentence, and the banner still reads it as saved.
check(() => {
  assert.equal(logic.loginOutcome(logic.LOGIN_SAVED_CLEANUP_SHOWN), 'saved_with_cleanup');
  assert.notEqual(logic.LOGIN_SAVED_CLEANUP_SHOWN, logic.LOGIN_SAVED_CLEANUP);
});
// UX-24: a running action leaves navigation, Refresh and view-only controls usable.
check(() => {
  for (const key of ['nav-accounts', 'refresh', 'dismiss-notice', 'countdown-toggle', 'org-filter-all', 'quota-x', 'chart-tokens', 'stats-30', 'stat-groups-sort-1', 'folders-alpha', 'disk-map-toggle', 'review-rules', 'open-api-keys', 'tour-next', 'stats-retry']) assert.ok(logic.usableWhileBusy(key), key);
  for (const key of ['primary-x', 'menu-x', 'backup-now', 'restore-x', '', 'auto-on', 'menu-add']) assert.ok(!logic.usableWhileBusy(key), key);
});
// VIS-23: the limits-and-errors cell is an alert from exactly 1 % of requests.
check(() => {
  const c = (requests, limited, failed) => ({ requests, ok: requests - limited - failed, limited, failed, input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 });
  assert.equal(logic.statAlert(c(1000, 5, 4)), false, '0.9 % is not an alert, although it rounds to 1');
  assert.equal(logic.statAlert(c(1000, 6, 4)), true);
  assert.equal(logic.statAlert(c(0, 0, 0)), false);
});
// Product round 1: no Russian sentence reads «группе Группа» (a label put where a locative belongs).
check(() => {
  const ru = readFileSync(new URL('../src/locales/ru.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(ru, /группе Группа|группы Группа|в Группа/, 'a group phrase, not a group label, after a preposition');
  assert.doesNotMatch(ru, /в группе \{group\}|группы \{group\}/, 'sentences take {where} (inGroup), not {group}');
});
// Round 2 (N6): About's switch is a deliberate choice and always sets the shared switch; only the
// question's answer carries the switch it showed, and both notices say what is now true.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const answer = main.slice(main.indexOf('function answerUsage('), main.indexOf('function usageQuestionCard() {'));
  assert.match(answer, /adapter\.setAnalytics\(wanted, shown\)/, 'the answer names the switch the question showed');
  const about = main.slice(main.indexOf('function analyticsPanel() {'), main.indexOf('function backupsPanel() {'));
  assert.match(about, /box\.addEventListener\('change', \(\) => \{ const wanted = box\.checked; void mutate\(async \(\) => \{ analyticsState = await adapter\.setAnalytics\(wanted\); \}, \(\) => usageAnswerText\(wanted, analyticsState\), 'analytics'\); \}\);/, "About's notice reports what is now true");
  assert.doesNotMatch(about, /wanted \? t\('Anonymous usage analytics are on\.'\)/, 'never a fixed "on"');
});
// Round 2: N5's window fixes keep their regression checks (unit tier A to D, G).
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.match(main, /const sync = \(\) => \{ const fresh = picker\.value === '__new'; [^}]*typed\.disabled = !fresh; typed\.dataset\.locked = String\(!fresh\);/, 'a hidden group id is disabled and marked locked');
});
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const stats = main.slice(main.indexOf('async function loadStats() {'), main.indexOf('async function loadStats() {') + 600);
  assert.match(stats, /const days = statsDays;[\s\S]*adapter\.statistics\(days\)[\s\S]*if \(days !== statsDays\) return;/, 'an answer for a range the person left is dropped');
});
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const reload = main.slice(main.indexOf('async function reload() {'), main.indexOf('async function reload() {') + 2000);
  for (const read of ['loadOpenrouter', 'loadKimi', 'loadSessions', 'loadBackups']) assert.match(reload, new RegExp(`void ${read}\\(readStamp\\);`), `${read} takes the Refresh's stamp`);
});
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const place = main.slice(main.indexOf('function placeTour('), main.indexOf("window.addEventListener('resize', () => placeTour());"));
  assert.match(place, /^function placeTour\(scroll = false\) \{/, 'placeTour scrolls only when asked or needed');
  assert.doesNotMatch(place, /if \(target\) target\.scrollIntoView/, 'never an unconditional scroll on a background render');
  const scrolls = place.match(/scrollIntoView/g) ?? [];
  const gated = place.match(/if \(target && move[^)]*\) \{? ?target\.scrollIntoView/g) ?? [];
  assert.equal(scrolls.length, 2); assert.equal(gated.length, scrolls.length, 'every scroll is behind move');
  assert.match(main, /if \(tourStep !== null\) placeTour\(\); return true;/, 'a render places the card without asking to scroll');
  assert.match(main, /restoreFocus\('tour-title'\); placeTour\(true\);/, 'a step change scrolls');
});
// Round 2: on a step whose target appears late (Agents loads after the step shows) or lies off
// screen, the card scrolls to it once; the toast moves aside only while the tour is on screen.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const place = main.slice(main.indexOf('function placeTour('), main.indexOf("window.addEventListener('resize', () => placeTour());"));
  assert.match(place, /const move = target !== null && \(scroll \|\| firstFound \|\| offScreen\);/, 'scroll when asked, first found or off screen');
  assert.match(place, /const firstFound = target !== null && \(tourFound\?\.step !== tourStep \|\| tourFound\.selector !== selector\);/, 'the target is remembered per step');
  assert.match(place, /tourFound = target \? \{ step: tourStep, selector \} : null;/, 'and remembered once placed');
  assert.match(main, /\$\{tourShown\(\) \? ' beside-tour' : ''\}/, 'the toast moves aside only beside a tour on screen');
  assert.doesNotMatch(main, /tourStep !== null \? ' beside-tour'/);
});
const i18nForShell = await load('../src/i18n.ts');
// SB-183 (node A-7), the_shell_hook_section_reads_every_state: the Groups page's *Terminals in
// organization folders* reads reading, failed, on by default with its organization, on, off, off
// by default, installed but inactive, every start file's state (a link and ZDOTDIR with the line
// to add), outside the installed app, a block the person took out, and the notices (SCN-074, SCN-075).
check(() => {
  const home = '/Users/ana';
  const base = { choice: 'default', map: 'ok', files: [], organizations: ['Acme'], codex_organizations: ['Acme'], can_change: true, home };
  const reading = logic.shellHookView(null);
  assert.equal(reading.state, 'reading'); assert.equal(reading.action, null); assert.equal(reading.retry, false);
  const failed = logic.shellHookView(null, 'Switchboard could not read its own state.');
  assert.equal(failed.state, 'failed'); assert.equal(failed.retry, true);
  assert.equal(failed.line, "The terminal hook's state could not be read.");
  assert.equal(failed.error, 'Switchboard could not read its own state.');
  const written = [{ shell: 'zsh', path: `${home}/.zshrc`, state: 'written' }, { shell: 'fish', path: `${home}/.config/fish/conf.d/fabric-switchboard.fish`, state: 'written' }];
  const byDefault = logic.shellHookView({ ...base, hook: 'default_on', files: written });
  assert.equal(byDefault.state, 'default_on');
  assert.equal(byDefault.line, "On: new terminals in Acme's folders work in its context.");
  assert.deepEqual(byDefault.files.map((f) => f.text), ['zsh: ~/.zshrc, added', 'fish: its own file, added']);
  assert.deepEqual(byDefault.action, { label: 'Turn off', on: false, disabled: false });
  assert.equal(byDefault.note, null); assert.equal(byDefault.readd, false); assert.equal(byDefault.error, null);
  assert.equal(logic.shellHookView({ ...base, hook: 'default_on', files: written, organizations: ['Acme', 'Globex'] }).line, 'On: new terminals in organization folders work in their context.');
  const on = logic.shellHookView({ ...base, hook: 'on', choice: 'on', files: written });
  assert.equal(on.state, 'on'); assert.equal(on.line, 'On: new terminals in organization folders work in their context.');
  const off = logic.shellHookView({ ...base, hook: 'off', choice: 'off' });
  assert.equal(off.state, 'off'); assert.equal(off.line, 'Off: new terminals keep your own settings.');
  assert.deepEqual(off.action, { label: 'Turn on', on: true, disabled: false });
  const none = logic.shellHookView({ ...base, hook: 'default_off', organizations: [] });
  assert.equal(none.state, 'default_off'); assert.equal(none.line, 'Off: no group belongs to an organization on this computer.');
  assert.equal(none.action.label, 'Turn on');
  // SB-222: off by default while an organization's group exists, but holds no row: the line names it.
  const held = logic.shellHookView({ ...base, hook: 'default_off', organizations: [], unmapped: [{ organization: 'Acme', group: 'Acme web', reason: 'held', held_by: 'Mine' }, { organization: 'Globex', group: 'G', reason: 'missing', held_by: null }] });
  assert.equal(held.state, 'default_off');
  assert.equal(held.line, 'Off: Acme\'s folders belong to the group “Mine”, so terminals there keep your own settings.');
  assert.equal(logic.shellHookView({ ...base, hook: 'default_off', organizations: [], unmapped: [{ organization: 'Globex', group: 'G', reason: 'missing', held_by: null }] }).line, 'Off: Globex\'s folders are not on this computer.');
  assert.equal(logic.shellHookView({ ...base, hook: 'default_off', organizations: [], unmapped: [{ organization: 'Globex', group: 'G', reason: 'no_folder', held_by: null }] }).line, 'Off: Globex\'s group “G” has no folder yet.');
  assert.equal(logic.shellHookView({ ...base, hook: 'default_off', organizations: [], unmapped: [] }).line, 'Off: no group belongs to an organization on this computer.');
  assert.equal(logic.shellHookView({ ...base, hook: 'off', choice: 'off', unmapped: [{ organization: 'Globex', group: 'G', reason: 'missing', held_by: null }] }).line, 'Off: new terminals keep your own settings.', 'the person\'s own off is not explained away');
  // Wanted, but no start file holds it: installed but inactive, with each file's reason.
  const skipped = [
    { shell: 'zsh', path: `${home}/.zshrc`, state: 'skipped_symlink' },
    { shell: 'zsh', path: `${home}/.zshrc`, state: 'skipped_zdotdir_unknown' },
    { shell: 'bash', path: `${home}/.bash_profile`, state: 'skipped_owner' },
    { shell: 'bash', path: '/etc/bashrc', state: 'unreadable' },
    { shell: 'pwsh', path: 'C:\\Users\\ana\\Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1', state: 'policy_blocked' },
  ];
  const inactive = logic.shellHookView({ ...base, hook: 'default_on', files: skipped });
  assert.equal(inactive.state, 'inactive');
  assert.equal(inactive.line, 'On, but no start file holds the hook yet, so terminals do not follow organization folders.');
  assert.equal(inactive.action.label, 'Turn off', 'the person can still turn the choice off');
  assert.deepEqual(inactive.files.map((f) => f.command), ['eval "$(switchboard shell init zsh)"', 'eval "$(switchboard shell init zsh)"', null, null, null]);
  assert.ok(inactive.files.every((f) => f.problem));
  assert.equal(inactive.files[0].text, '~/.zshrc is a link, so Switchboard did not change it. Add this line yourself:');
  assert.match(inactive.files[1].text, /ZDOTDIR/);
  assert.equal(inactive.files[2].text, '~/.bash_profile belongs to another user, so Switchboard did not change it.');
  assert.equal(inactive.files[3].text, '/etc/bashrc could not be read, so Switchboard did not change it.', 'a path outside the home stays whole');
  assert.match(inactive.files[4].text, /^PowerShell's execution policy stops profiles from loading/);
  assert.equal(logic.shellHookView({ ...base, hook: 'on', files: [] }).state, 'inactive', 'on with no shell present');
  assert.equal(logic.shellInitLine('fish'), 'switchboard shell init fish | source');
  // A linked PowerShell profile (node A-8): the line PowerShell reads.
  assert.equal(logic.shellInitLine('pwsh'), 'switchboard shell init pwsh | Out-String | Invoke-Expression');
  const linked = logic.shellHookView({ ...base, hook: 'default_on', files: [{ shell: 'pwsh', path: 'C:\\Users\\ana\\Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1', state: 'skipped_symlink' }] });
  assert.equal(linked.files[0].command, 'switchboard shell init pwsh | Out-String | Invoke-Expression');
  // A block the person took out: said, and the window can add it back.
  const removed = logic.shellHookView({ ...base, hook: 'default_on', files: [written[0], { shell: 'bash', path: `${home}/.bash_profile`, state: 'removed_by_person' }] });
  assert.equal(removed.state, 'default_on');
  assert.equal(removed.files[1].text, 'You took the block out of ~/.bash_profile, so Switchboard does not add it back.');
  assert.equal(removed.readd, true);
  // Outside the installed app (a development build, serve): read, never changed.
  const dev = logic.shellHookView({ ...base, hook: 'default_on', files: written, can_change: false });
  assert.equal(dev.action.disabled, true); assert.equal(dev.readd, false);
  assert.equal(dev.note, 'Only the installed app changes terminal settings.');
  // The folder map from another version, or unreadable: said under the state.
  assert.match(logic.shellHookView({ ...base, hook: 'default_on', files: written, map: 'unknown_version' }).error, /another version/);
  assert.match(logic.shellHookView({ ...base, hook: 'default_on', files: written, map: 'unreadable' }).error, /could not be read/);
  // A save that failed keeps the state and names the cause.
  const saveFailed = logic.shellHookView({ ...base, hook: 'off', choice: 'off' }, "The terminal hook's choice could not be saved in Switchboard's data folder.");
  assert.equal(saveFailed.state, 'off'); assert.match(saveFailed.error, /could not be saved/);
  // The notices.
  assert.equal(logic.shellHookToggleNotice(false), 'The terminal hook is off. New terminals keep your own settings; terminals already open switch back at their next change of folder.');
  assert.equal(logic.shellHookToggleNotice(true), 'The terminal hook is on. New terminals in organization folders work in their context.');
  assert.deepEqual(logic.shellHookNoticeText({ organization: 'Acme', group: 'Acme web' }), {
    title: "Terminals now work in Acme's context in its folders",
    body: "New terminals opened in Acme web's folders run claude and codex on Acme's accounts and history. Turn this off under Groups.",
  });
  // SCN-066: the Codex section says what the hook does to Codex in an organization's folders.
  assert.equal(logic.codexHookLine({ ...base, hook: 'default_on', files: written }), "In Acme's folders, new terminals use its Codex accounts whether this is on or off.");
  assert.equal(logic.codexHookLine({ ...base, hook: 'off', choice: 'off' }), null);
  assert.equal(logic.codexHookLine({ ...base, hook: 'default_on', files: skipped }), null, 'inactive says nothing about Codex');
  // A-7 fix cycle 1 (N1): only organizations whose map rows carry a Codex account are named.
  assert.equal(logic.codexHookLine({ ...base, hook: 'default_on', files: written, codex_organizations: [] }), null, 'no Codex account: nothing about Codex');
  assert.equal(logic.codexHookLine({ ...base, hook: 'default_on', files: written, organizations: ['Acme', 'Globex'], codex_organizations: ['Globex'] }), "In Globex's folders, new terminals use its Codex accounts whether this is on or off.");
  assert.equal(logic.codexHookLine({ ...base, hook: 'default_on', files: written, organizations: ['Acme', 'Globex'], codex_organizations: ['Acme', 'Globex'] }), 'In organization folders, new terminals use their Codex accounts whether this is on or off.');
  // Russian: every line has its entry, with the organization kept.
  i18nForShell.setLocale('ru');
  try {
    const ru = logic.shellHookView({ ...base, hook: 'default_on', files: written });
    assert.match(ru.line, /Acme/); assert.doesNotMatch(ru.line, /new terminals/);
    assert.notEqual(ru.action.label, 'Turn off');
    assert.doesNotMatch(logic.shellHookToggleNotice(false), /terminal hook/);
    assert.match(logic.shellHookNoticeText({ organization: 'Acme', group: 'Acme web' }).body, /Acme web/);
  } finally { i18nForShell.setLocale('en'); }
});
// SB-202 (B-8, SCN-076): the window's first run on an organization machine.
{
  const org = { id: 'o-acme', name: 'Acme' };
  const group = { pool: 'acme-work', name: 'Acme work' };
  const five = ['owner', 'accounts', 'project', 'mcp', 'proving_call'].map((id) => ({ id, state: 'done', summary: id }));
  const withOrg = (extra) => ({ complete: false, degraded: [], steps: [...five, { id: 'organization', state: 'done', summary: 'x' }, ...extra.map((s) => ({ organization: org, summary: 'english', ...s }))] });
  const noGroup = withOrg([{ id: 'org_group', state: 'needs_person', groups: [] }, { id: 'seat', state: 'needs_person' }, { id: 'hook', state: 'optional', terminals: 'waiting' }, { id: 'launch', state: 'optional' }]);
  const noSeat = withOrg([{ id: 'org_group', state: 'done', group }, { id: 'seat', state: 'needs_person', ended: 0 }, { id: 'hook', state: 'optional', terminals: 'active' }, { id: 'launch', state: 'optional' }]);
  const ready = withOrg([{ id: 'org_group', state: 'done', group }, { id: 'seat', state: 'done' }, { id: 'hook', state: 'optional', terminals: 'active' }, { id: 'launch', state: 'optional' }]);
  const asked = { available: true, enabled: true, disclosed: false };
  const answered = { available: true, enabled: true, disclosed: true };
  // the organization card waits for the usage question and the tour
  check(() => {
    const [setup] = logic.orgSetups(noGroup);
    assert.equal(setup.organization.name, 'Acme');
    assert.equal(logic.orgSetupTitle(setup), 'Set up Acme on this computer');
    assert.deepEqual(setup.lines.map((l) => [l.id, l.state]), [['organization', 'done'], ['org_group', 'needs_person'], ['seat', 'needs_person'], ['hook', 'optional'], ['launch', 'optional']]);
    assert.equal(setup.lines[1].line, 'Acme has no group on this computer yet.');
    assert.deepEqual(logic.orgSetupAction(setup), { kind: 'group', organization: 'o-acme', label: '+ Group for Acme' });
    const shown = (usage, tourStep, page = 'accounts', hidden = [], failed = false) => logic.orgSetupShown({ page, organization: 'o-acme', hidden, usage, usageFailed: failed, tourStep });
    assert.equal(shown(asked, null), false, 'the usage question comes first');
    assert.equal(shown(null, null), false, 'its status is still being read');
    assert.equal(shown(answered, 0), false, 'then the tour');
    assert.equal(shown(answered, null), true, 'then the card');
    assert.equal(shown(null, null, 'accounts', [], true), true, 'a failed status read does not hold the card back');
    assert.equal(shown(answered, null, 'orgs'), true, 'on top of Organizations too');
    assert.equal(shown(answered, null, 'agents'), false, 'nowhere else');
    assert.equal(shown(answered, null, 'accounts', ['o-acme']), false, 'Hide keeps it off Accounts');
    assert.equal(shown(answered, null, 'orgs', ['o-acme']), true, 'and on Organizations');
    const [seat] = logic.orgSetups(noSeat);
    assert.deepEqual(logic.orgSetupAction(seat), { kind: 'sign_in', pool: 'acme-work', label: 'Sign in with your Acme address' });
    assert.equal(seat.lines[2].line, 'Sign in with your Acme address. It goes to the group “Acme work”.');
    // SB-228: the command is code of its own, not words in the line.
    assert.equal(seat.lines[3].line, "Terminals in Acme's folders work in its context. To turn this off:");
    assert.equal(seat.lines[3].command, 'switchboard shell off');
    assert.equal(seat.lines[2].command, undefined);
    const ended = logic.orgSetups(withOrg([{ id: 'org_group', state: 'done', group }, { id: 'seat', state: 'needs_person', ended: 2 }]))[0];
    assert.equal(ended.lines[2].line, 'Your Acme accounts in “Acme work” must sign in again.');
    const two = logic.orgSetups(withOrg([{ id: 'org_group', state: 'needs_person', groups: ['Acme web', 'Acme two'] }, { id: 'seat', state: 'needs_person' }]))[0];
    assert.equal(two.lines[1].line, 'Keep one group for Acme on this computer: Acme web, Acme two.');
    assert.equal(logic.orgSetupAction(two), null, 'which group stays is the person\'s');
    assert.deepEqual(logic.orgSetups(ready), [], 'every step done or optional: the card goes');
    // Several organizations: one card each, in the checklist's order (newest first).
    const globex = { id: 'o-globex', name: 'Globex' };
    const both = { ...noGroup, steps: [...noGroup.steps, { id: 'org_group', state: 'needs_person', organization: globex, groups: [] }] };
    assert.deepEqual(logic.orgSetups(both).map((s) => s.organization.name), ['Acme', 'Globex']);
    assert.equal(logic.orgSetupStateLabel('needs_person'), 'Needs you');
  });
  // SB-228: the card's caveat from `degraded`, one line, the widest read first.
  check(() => {
    assert.equal(logic.orgSetupCaveat(noSeat), null);
    assert.equal(logic.orgSetupCaveat(null), null);
    assert.equal(logic.orgSetupCaveat({ ...noSeat, degraded: ['sessions'] }), 'The recorded sessions could not be read, so First launch may read not done.');
    assert.equal(logic.orgSetupCaveat({ ...noSeat, degraded: ['sessions', 'sign_ins'] }), 'Whether a sign-in ended could not be read, so Your seat may read signed in when it is not.');
    assert.equal(logic.orgSetupCaveat({ ...noSeat, degraded: ['sign_ins', 'snapshot'] }), "Part of Switchboard's saved settings could not be read, so a step may be wrong or missing.");
    assert.equal(logic.orgSetupCaveat({ ...noSeat, degraded: ['something_new'] }), 'Part of the setup could not be read, so a step may be wrong.');
  });
  // SB-228 (VIS-21): the card's own action is the screen's one primary when the card is shown with one.
  check(() => {
    const leads = (onboarding, page = 'accounts', usage = answered, tourStep = null, hidden = []) => logic.orgSetupLeads({ onboarding, page, hidden, usage, usageFailed: false, tourStep });
    assert.equal(leads(noSeat), true);
    assert.equal(leads(noSeat, 'orgs'), true);
    assert.equal(leads(noSeat, 'accounts', asked), false, 'the usage question first: the card is not shown');
    assert.equal(leads(noSeat, 'accounts', answered, 0), false, 'the tour first');
    assert.equal(leads(noSeat, 'accounts', answered, null, ['o-acme']), false, 'hidden on Accounts');
    assert.equal(leads(noSeat, 'orgs', answered, null, ['o-acme']), true, 'still on Organizations');
    assert.equal(leads(ready), false, 'no card');
    const two = withOrg([{ id: 'org_group', state: 'needs_person', groups: ['Acme web', 'Acme two'] }, { id: 'seat', state: 'needs_person' }]);
    assert.equal(leads(two), false, 'a card with no action of its own leads nothing');
  });
  // SB-228 (SCN-076): the first checklist read failed: a card with the note, for the newest organization.
  check(() => {
    const organizations = [{ id: 'o-old', name: 'Old', created_at: 1 }, { id: 'o-acme', name: 'Acme', created_at: 5 }];
    const unread = (input) => logic.orgSetupUnread({ onboarding: null, failed: true, organizations, page: 'accounts', hidden: [], usage: answered, usageFailed: false, tourStep: null, ...input });
    assert.deepEqual(unread({}), { id: 'o-acme', name: 'Acme' });
    assert.deepEqual(unread({ page: 'orgs' }), { id: 'o-acme', name: 'Acme' });
    assert.equal(unread({ onboarding: noSeat }), null, 'a read before: the card keeps its steps and shows the note itself');
    assert.equal(unread({ failed: false }), null, 'still reading');
    assert.equal(unread({ organizations: [] }), null, 'no organization, no card');
    assert.equal(unread({ page: 'agents' }), null);
    assert.equal(unread({ usage: asked }), null, 'the usage question first');
    assert.deepEqual(unread({ hidden: ['o-acme'] }), { id: 'o-old', name: 'Old' }, 'Hide on Accounts holds for that organization');
    assert.equal(unread({ hidden: ['o-acme', 'o-old'] }), null);
  });
  // the empty Accounts page on an organization machine leads with the seat
  check(() => {
    const signedIn = { claude: { status: 'available' }, codex: null };
    assert.deepEqual(logic.accountsEmptyKey({ backup: false, onboarding: noSeat, current: signedIn }), { lead: 'org_seat', organization: 'Acme', group: 'Acme work', pool: 'acme-work', line: 'Sign in with your Acme address. It goes to the group “Acme work”.', label: 'Sign in with your Acme address' });
    assert.equal(logic.accountsEmptyKey({ backup: true, onboarding: noSeat, current: signedIn }).lead, 'backup', 'a backup to restore still comes first');
    assert.equal(logic.accountsEmptyKey({ backup: false, onboarding: noGroup, current: signedIn }).lead, 'add_current', 'no group yet: the card asks for it, the page keeps its line');
  });
  // guard: no organization, no card, today's empty state
  check(() => {
    const plain = { complete: true, degraded: [], steps: five };
    assert.deepEqual(logic.orgSetups(plain), []);
    assert.deepEqual(logic.orgSetups(null), []);
    assert.equal(logic.accountsEmptyKey({ backup: false, onboarding: plain, current: { claude: { status: 'missing' }, codex: null } }).lead, 'sign_in');
    assert.equal(logic.accountsEmptyKey({ backup: false, onboarding: null, current: { claude: { status: 'available' } } }).lead, 'add_current');
    assert.equal(logic.accountsEmptyKey({ backup: false, onboarding: null, current: null }).lead, 'reading');
    const lonely = { complete: true, degraded: [], steps: [...five, { id: 'organization', state: 'optional', summary: 'PassionCode.ai lists no organization for you yet.' }] };
    assert.deepEqual(logic.orgSetups(lonely), [], 'PassionCode.ai listing none: no card');
  });
}
// C-3 (SB-71): the fallback chains panel and its editor.
{
  // SB-229: the window's hand copies stay tied to the runtime: the takeover order is SWITCHABLE
  // (agent_catalog.rs), and a chain's key is chain_view::scope_key's.
  check(() => {
    const catalog = readFileSync(new URL('../crates/switchboard-runtime/src/agent_catalog.rs', import.meta.url), 'utf8');
    const switchable = /pub const SWITCHABLE: &\[&str\] = &\[([^\]]*)\]/.exec(catalog);
    assert.ok(switchable, 'SWITCHABLE is declared in agent_catalog.rs');
    assert.deepEqual(logic.TAKEOVER_ORDER, [...switchable[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]));
    const view = readFileSync(new URL('../crates/switchboard-runtime/src/chain_view.rs', import.meta.url), 'utf8');
    for (const form of ['ChainScope::Machine => "machine".into()', 'ChainScope::Project { pool } => format!("project:{pool}")', 'ChainScope::Workflow { id } => format!("workflow:{id}")']) assert.ok(view.includes(form), form);
    assert.equal(logic.chainScopeKey({ kind: 'machine' }), 'machine');
    assert.equal(logic.chainScopeKey({ kind: 'project', pool: 'alpha' }), 'project:alpha');
    assert.equal(logic.chainScopeKey({ kind: 'workflow', id: 'wf_0123456789abcdef' }), 'workflow:wf_0123456789abcdef');
  });
  // SB-229: a re-read is applied only when no action began after it was issued, and its
  // failure is reported only then too; a success says so, so the caller clears its error.
  {
    const clock = new logic.MutationClock();
    const later = () => { let settle; const promise = new Promise((resolve, reject) => { settle = { resolve, reject }; }); return { promise, settle }; };
    assert.deepEqual(await logic.clockedRead(clock, async () => 7), { state: 'applied', value: 7 });
    const failed = await logic.clockedRead(clock, async () => { throw new Error('read failed'); });
    assert.equal(failed.state, 'failed'); assert.equal(failed.error.message, 'read failed');
    // An action began while the read was out: neither its value nor its failure is applied.
    let pending = later(); let read = logic.clockedRead(clock, () => pending.promise); clock.begin(); clock.end(); pending.settle.resolve(1);
    assert.deepEqual(await read, { state: 'stale', value: 1 }, 'its value comes back marked stale: it may only fill an empty view');
    pending = later(); read = logic.clockedRead(clock, () => pending.promise); clock.begin(); pending.settle.reject(new Error('x'));
    assert.deepEqual(await read, { state: 'stale' }, 'a failure during an action is not reported');
    clock.end();
    // Issued during an action: stale too.
    clock.begin(); read = logic.clockedRead(clock, async () => 2); clock.end();
    assert.deepEqual(await read, { state: 'stale', value: 2 });
    cases += 1;
  }

  // Move up and Move down give the new order; a move past either end changes nothing.
  check(() => {
    assert.deepEqual(logic.moveStep(['claude-code', 'codex'], 1, -1), ['codex', 'claude-code']);
    assert.deepEqual(logic.moveStep(['claude-code', 'codex', 'hermes'], 0, 1), ['codex', 'claude-code', 'hermes']);
    assert.deepEqual(logic.moveStep(['claude-code', 'codex'], 0, -1), ['claude-code', 'codex']);
    assert.deepEqual(logic.moveStep(['claude-code', 'codex'], 1, 1), ['claude-code', 'codex']);
    const list = ['a', 'b']; logic.moveStep(list, 1, -1); assert.deepEqual(list, ['a', 'b'], 'the list given is not changed');
  });
  // The account choice mirrors the core's rules (chains.rs validate_executor) and what the fallback runs.
  check(() => {
    const subscription = { id: 'c1', label: 'Work Max', provider: 'claude', kind: 'oauth', enabled: true };
    const claudeApi = { id: 'c2', label: 'Claude API', provider: 'claude', kind: 'api_key', enabled: true };
    const codex = { id: 'x1', label: 'Codex Pro', provider: 'codex', kind: 'oauth', enabled: true };
    const off = { id: 'c3', label: 'Old', provider: 'claude', kind: 'oauth', enabled: false };
    const kimi = [{ id: 'k1', label: 'Kimi one' }];
    assert.deepEqual(logic.accountsForStep('hermes', [subscription]), [], 'another agent never runs on a subscription sign-in');
    assert.deepEqual(logic.accountsForStep('opencode', [claudeApi], kimi), [], 'OpenRouter agents run on the key, never pinned');
    assert.deepEqual(logic.accountsForStep('claude-code', [subscription, claudeApi, codex, off]).map((a) => a.id), ['c1', 'c2']);
    assert.deepEqual(logic.accountsForStep('codex', [subscription, codex]).map((a) => a.id), ['x1']);
    assert.deepEqual(logic.accountsForStep('kimi-code', [subscription, codex], kimi), [{ id: 'k1', label: 'Kimi one' }]);
    assert.deepEqual(logic.accountsForStep('aider', [claudeApi]), [], 'an agent the fallback cannot run gets no pin');
  });
  // The chain in one line, its steps in order with a pinned account named.
  check(() => {
    const names = { 'claude-code': 'Claude Code', codex: 'Codex', 'kimi-code': 'Kimi Code', hermes: 'Hermes' };
    const nameOf = (id) => names[id] ?? id;
    assert.equal(logic.chainSummary([], nameOf), 'Not set');
    assert.equal(logic.chainSummary(null, nameOf), 'Not set');
    assert.equal(logic.chainSummary(['claude-code', 'codex', 'kimi-code', 'hermes'].map((agent) => ({ agent })), nameOf), 'Claude Code → Codex → Kimi Code → Hermes');
    assert.equal(logic.chainSummary([{ agent: 'codex', account_id: 'x1' }, { agent: 'opencode' }], nameOf, (id) => (id === 'x1' ? 'Codex Pro' : undefined)), 'Codex (Codex Pro) → opencode');
    assert.equal(logic.chainSummary([{ agent: 'hermes', key: 'alpha/prod/OPENROUTER_API_KEY' }], nameOf), 'Hermes (key alpha/prod/OPENROUTER_API_KEY)');
  });
  // One readiness line per step, for every state and reason the chains view gives.
  check(() => {
    const step = (state, reason = null, extra = {}) => ({ agent: 'claude-code', name: 'Claude Code', state, reason, paid: false, ...extra });
    assert.equal(logic.stepReasonText(step('ready', null, { account_label: 'Work Max' })), 'Ready: Work Max');
    assert.equal(logic.stepReasonText(step('ready')), 'Ready');
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, key_scope: 'group', spend: null }), 'Alpha web'), 'Ready: the key of Alpha web. Spend not read yet.');
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, key_scope: 'machine', spend: null })), "Ready: this computer's key. Spend not read yet.");
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, account_label: 'Claude API' })), 'Ready: Claude API. Spend not read yet.');
    assert.equal(logic.stepReasonText(step('skipped', 'no_usable_account')), 'Skipped: no account with room');
    assert.equal(logic.stepReasonText(step('skipped', 'agent_disabled')), 'Skipped: disabled under Installed');
    assert.equal(logic.stepReasonText(step('skipped', 'sign_in_required')), 'Skipped: needs a sign-in');
    assert.equal(logic.stepReasonText(step('skipped', 'agent_settings_unreadable')), 'Skipped: agent settings could not be read');
    assert.equal(logic.stepReasonText(step('skipped', 'ceiling_reached')), "Skipped: today's ceiling reached");
    assert.equal(logic.stepReasonText(step('skipped', 'key_exhausted')), 'Skipped: less than $0.50 left on the key');
    assert.equal(logic.stepReasonText(step('skipped', 'spend_unknown')), 'Skipped: spend could not be read');
    assert.equal(logic.stepReasonText(step('skipped', 'something_new')), 'Skipped (something_new)');
    assert.equal(logic.stepReasonText(step('not_automatic', 'agent_not_automatic_yet')), 'Not automatic yet');
    assert.equal(logic.stepReasonText(step('not_automatic', 'key_pinned', { key: 'alpha/prod/OPENROUTER_API_KEY' })), 'Pinned to key alpha/prod/OPENROUTER_API_KEY: not automatic yet (waits for project grants)');
    assert.equal(logic.stepReasonText(step('not_installed')), 'Not installed');
  });
  // The last hand-over in a line: its outcome, then the steps passed over with their reasons.
  check(() => {
    const nameOf = (id) => ({ hermes: 'Hermes', codex: 'Codex' })[id] ?? id;
    assert.equal(logic.handoverOutcomeText('offered'), 'handed over');
    assert.equal(logic.handoverOutcomeText('launch_failed'), 'the launch failed after the offer');
    assert.equal(logic.handoverOutcomeText('every_step_refused'), 'every step was refused');
    assert.equal(logic.handoverOutcomeText('no_usable_executor'), 'no agent could take it');
    assert.equal(logic.handoverOutcomeText('ceiling_reached'), "today's ceiling reached");
    assert.deepEqual(logic.handoverSkipped([{ agent: 'hermes', reason: 'no_usable_account' }, { agent: 'codex', reason: 'agent_disabled' }], nameOf), ['Hermes: no account with room', 'Codex: disabled under Installed']);
    assert.equal(logic.shortWorkflow('wf_0123456789abcdef'), 'wf_01234567');
  });
}
// C-7 (SB-72): spending ceilings in the window.
{
  // The question has two answers of equal weight: the same class, neither primary, none recommended.
  check(() => {
    const q = logic.ceilingQuestion({ value: 5 });
    assert.equal(q.actions.length, 2);
    const [double, keep] = q.actions;
    assert.equal(double.answer, 'double');
    assert.equal(keep.answer, 'keep');
    assert.equal(double.label, 'Double to $10.00');
    assert.equal(keep.label, 'Keep $5.00');
    assert.equal(double.className, keep.className, 'the same class');
    assert.ok(!double.className.includes('primary') && !keep.className.includes('primary'), 'neither is primary');
    for (const action of q.actions) assert.ok(!('recommended' in action), 'no recommended flag');
    assert.equal(double.disabled, false);
    assert.equal(q.note, null);
    assert.equal(logic.ceilingQuestion({ value: 5, kind: 'machine' }).text, "This computer's daily ceiling of $5.00 is reached.");
    assert.equal(logic.ceilingQuestion({ value: 3, kind: 'project', scope: 'Alpha Web' }).text, "Alpha Web's daily ceiling of $3.00 is reached.");
    assert.equal(logic.ceilingQuestion({ value: 2, kind: 'workflow', scope: 'wf_01234567' }).text, 'The ceiling of task wf_01234567, $2.00, is reached.');
    // Doubling past $1,000 is refused by the core: the button stays beside Keep, unavailable, and the note says why.
    const top = logic.ceilingQuestion({ value: 600, double: null });
    assert.equal(top.actions[0].disabled, true);
    assert.equal(top.actions[0].className, top.actions[1].className);
    assert.equal(top.note, 'Doubling would pass $1,000, the most a ceiling may be. Set a ceiling of up to $1,000 instead.');
    assert.equal(logic.ceilingQuestion({ value: 5, double: 10 }).actions[0].label, 'Double to $10.00');
  });
  // The key's own limit beside Switchboard's ceiling: a warning when a running agent can pass it.
  check(() => {
    const monthly = logic.ceilingWarning(5, { limit: 20, limit_reset: 'monthly' });
    assert.equal(monthly, "The key's own limit resets monthly, not daily. A running agent can spend up to the key's own limit; Switchboard's ceiling only decides whether it is started.");
    assert.match(logic.ceilingWarning(5, { limit: 20, limit_reset: 'weekly' }), /resets weekly, not daily/);
    assert.match(logic.ceilingWarning(5, { limit: 20, limit_reset: null }), /never resets/);
    assert.equal(logic.ceilingWarning(5, { limit: 10, limit_reset: 'daily' }), "The key's own limit, $10.00 a day, is higher than this ceiling. A running agent can spend up to the key's own limit; Switchboard's ceiling only decides whether it is started.");
    assert.match(logic.ceilingWarning(5, { limit: null, limit_reset: null }), /no spending limit of its own/);
    assert.equal(logic.ceilingWarning(5, { limit: 5, limit_reset: 'daily' }), null, 'a daily limit no higher than the ceiling: nothing to warn');
    assert.equal(logic.ceilingWarning(5, null), null, 'the key was not read: nothing to say');
    assert.equal(logic.ceilingWarning(null, { limit: 20, limit_reset: 'monthly' }), null, 'no ceiling: nothing to compare');
  });
  // The ceiling line on the machine panel and each group row.
  check(() => {
    assert.equal(logic.ceilingLine(null), 'Daily ceiling: not set');
    assert.equal(logic.ceilingLine({ usd: 5, doubled_from: null, active: true }), 'Daily ceiling: $5.00');
    assert.equal(logic.ceilingLine({ usd: 10, doubled_from: 5, active: true }), 'Daily ceiling: $10.00, doubled from $5.00');
    assert.equal(logic.ceilingLine({ usd: 3, doubled_from: null, active: false }), 'Inactive: this group has no key of its own now. Daily ceiling: $3.00');
    // SB-230: thousands are grouped, as the window writes $1,000 elsewhere; both languages alike (a window-wide decision).
    assert.equal(logic.dollars(1000), '$1,000.00');
    assert.equal(logic.dollars(1234567.5), '$1,234,567.50');
    assert.equal(logic.dollars(999.999), '$1,000.00');
    assert.equal(logic.dollars(0.5), '$0.50');
    // SB-230: removing a capped group's key says its ceiling stops counting.
    assert.equal(logic.ceilingRemovalNote({ usd: 3, active: true }), 'Its daily ceiling of $3.00 stops counting until the group has a key of its own again.');
    assert.equal(logic.ceilingRemovalNote({ usd: 3, active: false }), null, 'already inactive');
    assert.equal(logic.ceilingRemovalNote(null), null);
    // SB-230: a re-read right after a saved answer that fails keeps what is shown and shows no error;
    // any other failed read names it; a stale read changes nothing; an answer clears the error.
    const shown = { view: { questions: [1] }, error: '' };
    const text = (e) => `read: ${e.message}`;
    assert.deepEqual(logic.afterRead(shown, { state: 'applied', value: { questions: [] } }, false, text), { view: { questions: [] }, error: '' });
    assert.deepEqual(logic.afterRead({ ...shown, error: 'old' }, { state: 'applied', value: shown.view }, false, text).error, '');
    assert.deepEqual(logic.afterRead(shown, { state: 'failed', error: new Error('x') }, true, text), shown);
    assert.deepEqual(logic.afterRead(shown, { state: 'failed', error: new Error('x') }, false, text), { view: shown.view, error: 'read: x' });
    assert.deepEqual(logic.afterRead(shown, { state: 'stale' }, false, text), shown);
    // A stale read never replaces what is shown, but fills an empty view (a page opened while an
    // action ran would otherwise read "Reading…" until the next Refresh); a later read corrects it.
    assert.deepEqual(logic.afterRead(shown, { state: 'stale', value: { questions: [] } }, false, text), shown);
    assert.deepEqual(logic.afterRead({ view: null, error: '' }, { state: 'stale', value: { questions: [2] } }, false, text), { view: { questions: [2] }, error: '' });
    assert.deepEqual(logic.afterRead({ view: null, error: 'old' }, { state: 'stale', value: { questions: [2] } }, false, text), { view: { questions: [2] }, error: 'old' }, 'its error stays: the read is not trusted to clear it');
    assert.equal(logic.keyLimitLine({ limit: 20, limit_reset: 'monthly' }), "Key's own limit: $20.00, resets monthly");
    assert.equal(logic.keyLimitLine({ limit: 10, limit_reset: 'daily' }), "Key's own limit: $10.00, resets daily");
    assert.equal(logic.keyLimitLine({ limit: 10, limit_reset: null }), "Key's own limit: $10.00, never resets");
    assert.equal(logic.keyLimitLine({ limit: null, limit_reset: null }), "Key's own limit: none");
  });
}
console.log(`${cases} ui-logic cases passed, including quota priority and wall-clock countdowns.`);
