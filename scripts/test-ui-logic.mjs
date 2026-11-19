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
// SB-189: the runtime's table of the sentences a person reads, shared by the window, the CLI and MCP.
const personErrors = JSON.parse(readFileSync(new URL('../crates/switchboard-runtime/src/person-errors.json', import.meta.url), 'utf8')).sentences;
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
  // SB-314: a group that does not take the account is unavailable; the default group and the current one never are.
  const refusing = logic.moveTargets('web', projects, false, (pool) => pool !== 'agents');
  assert.deepEqual(refusing.map((t) => [t.pool, t.refused, t.blocked]), [['default', false, false], ['agents', true, true], ['web', false, false]]);
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
    { folder: '/d/loose/vendor/lib', remote: null, branch: null, pool: null, project: null, place: null, module_of: '/d/loose' },
    { folder: '/d/app/vendor/lib', remote: null, branch: null, pool: 'p', project: 'P', place: 'inside', module_of: '/d/app' },
  ] };
  assert.deepEqual(logic.mapRows(map, 'unassigned').map((r) => [r.name, r.action]), [['loose', 'assign'], ['lib', null]], 'a module moves with its repository, never on its own');
  assert.deepEqual(logic.mapRows(map, 'all').map((r) => r.action), ['move', null, 'assign', null, null], 'inside a project folder: no action of its own');
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
  // SB-189: the mapped sentences live in the runtime's table, which the window imports.
  for (const message of ['Label, pool or identity is invalid', 'Account identity is ambiguous in this pool', 'Terminal could not open.', 'Sign-in cleanup needs Keychain access.', 'Rotation time precedes the last switch']) assert(Object.hasOwn(personErrors, message), `mapped: ${message}`);
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
  // SB-192 (UX-16): the bar shows once switching has somewhere to go, or a policy is on.
  const row = (provider, pool, enabled = true) => ({ provider, pool, enabled });
  assert.equal(logic.autoSwitchPossible([], []), false, 'no account');
  assert.equal(logic.autoSwitchPossible([row('claude', 'default')], []), false, 'one account');
  assert.equal(logic.autoSwitchPossible([row('claude', 'default'), row('codex', 'default'), row('claude', 'work')], []), false, 'one per provider and group');
  assert.equal(logic.autoSwitchPossible([row('claude', 'work'), row('claude', 'work', false)], []), false, 'a disabled one does not count');
  assert.equal(logic.autoSwitchPossible([row('codex', 'work'), row('codex', 'work')], []), true, 'two of one provider in one group');
  assert.equal(logic.autoSwitchPossible([row('claude', 'work')], [{ enabled: true }]), true, 'a policy on stays stoppable');
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
  assert.equal(personErrors[logic.LOGIN_SAVED_CLEANUP], logic.LOGIN_SAVED_CLEANUP_SHOWN, 'the cleanup sentence is mapped to the one loginOutcome reads (CPY-05)');
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert(!main.includes(logic.LOGIN_FORGOTTEN) && !main.includes(logic.LOGIN_SAVED_CLEANUP), 'main.ts classifies sign-in errors through loginOutcome');
  assert.equal((main.match(/loginOutcome\(/g) ?? []).length, 4, 'status poll, finish, cancel and Try again (its cancel, SB-67) each classify');
});

// SB-213 fix cycle 1, P10: every spend-gate stop sentence (spend.rs `pub const STOP_*`) reaches the
// window as the backend wrote it: listed verbatim in safeErrors and never rewritten by coreErrors.
check(() => {
  const spend = readFileSync(new URL('../crates/switchboard-runtime/src/spend.rs', import.meta.url), 'utf8');
  const stops = [...spend.matchAll(/pub const (STOP_\w+): &str =\s*"((?:[^"\\]|\\.)*)";/g)];
  assert(stops.length >= 4, `the STOP_* sentences were found (${stops.length})`);
  const adapter = readFileSync(new URL('../src/adapter.ts', import.meta.url), 'utf8');
  const safeStart = adapter.indexOf('const safeErrors = new Set([');
  const safe = adapter.slice(safeStart, adapter.indexOf(']);', safeStart));
  for (const [, name, sentence] of stops) {
    const quoted = `'${sentence.replace(/'/g, "\\'")}'`;
    assert(safe.includes(`  ${quoted},`), `${name} is in safeErrors verbatim`);
    assert(!Object.hasOwn(personErrors, sentence), `${name} is not rewritten by the person-errors table`);
  }
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
  assert.equal(logic.compactCountdown(NaN, now), 'unknown', 'SB-195: an invalid time reads as a word, never a lone dash');
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
  assert.equal(logic.updateLine({ ...base, enabled: false }).text, 'Version 0.6.0. Automatic updates are off; Switchboard tells you when a new version is out.');
  // SB-249: off still reads the feed; a newer version is offered with Update, never installed alone.
  assert.deepEqual(logic.updateLine({ ...base, enabled: false, state: 'available', version: '0.7.0' }), { text: 'Version 0.7.0 is out. Press Update to install it.', restart: false, install: true });
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

// SB-414: a window that does not own the data folder says which copy does and its version.
check(() => {
  const base = { available: true, reason: null, enabled: true, state: 'idle', current: '0.7.111', version: null, error: null, needs_permission: false, restart_after: null };
  const owner = { kind: 'serve', version: '0.7.112', copy: '/Applications/Fabric Switchboard.app', same_copy: false, older: true, updates_copy: true };
  const other = logic.updateLine({ ...base, owner });
  assert.match(other.text, /^Switchboard runs in the background as version 0\.7\.112 from \/Applications\/Fabric Switchboard\.app; this window is an older copy, version 0\.7\.111\. Quit it and open Switchboard from \/Applications\/Fabric Switchboard\.app\. Version 0\.7\.111 is the latest/);
  assert.equal(other.owned, undefined);
  // Its own copy: the owner keeps it current, this window installs nothing and offers no button.
  const same = logic.updateLine({ ...base, owner: { ...owner, same_copy: true } });
  assert.deepEqual(same, { text: 'Switchboard runs in the background as version 0.7.112 from this copy; this window is still version 0.7.111. Quit this window and open Switchboard again to use version 0.7.112.', restart: false, owned: true });
  // Review round 1, P1-1: an owner that does not keep the copy current (a serve on Windows, with
  // updates off, or without write access) leaves the window its own line and buttons.
  const notKept = logic.updateLine({ ...base, enabled: false, state: 'available', version: '0.7.113', owner: { ...owner, same_copy: true, updates_copy: false } });
  assert.equal(notKept.owned, undefined); assert.equal(notKept.install, true);
  assert.match(notKept.text, /Press Update to install it\.$/);
  const plain = logic.updateLine({ ...base, owner: { ...owner, older: false, version: '0.7.111', updates_copy: false } }).text;
  assert.match(plain, /^Switchboard runs in the background as switchboard serve, version 0\.7\.111; this window is version 0\.7\.111\./);
  assert.doesNotMatch(plain, /keeps itself current/);
  assert.match(logic.updateLine({ ...base, owner: { ...owner, copy: '/usr/bin' } }).text, /Quit it and open the installed Switchboard\./);
  assert.match(logic.updateLine({ ...base, owner: { ...owner, older: false, version: '0.7.111' } }).text, /^Switchboard runs in the background as switchboard serve, version 0\.7\.111, which keeps itself current; this window is version 0\.7\.111\./);
  assert.match(logic.updateLine({ ...base, owner: { ...owner, kind: 'app', older: false, version: '0.7.110' } }).text, /^Another Switchboard runs in the background, version 0\.7\.110/);
  // Review round 2: the owner holds the data folder and did not answer: said, never silence,
  // and Check for updates stays.
  const unanswered = logic.updateLine({ ...base, owner: { unanswered: true } });
  assert.deepEqual(unanswered, { text: 'Switchboard already runs in the background but did not answer, so this window did not check for updates. Try again in a minute, or quit Switchboard and open it again.', restart: false, check: true });
  // Review round 3: a ready update keeps its Restart to update, its own line first.
  const readyUnanswered = logic.updateLine({ ...base, state: 'ready', version: '0.7.113', owner: { unanswered: true } });
  assert.equal(readyUnanswered.restart, true);
  assert.match(readyUnanswered.text, /^Version 0\.7\.113 is ready\. .* Switchboard already runs in the background but did not answer/);
  const offeredUnanswered = logic.updateLine({ ...base, enabled: false, state: 'available', version: '0.7.113', owner: { unanswered: true } });
  assert.equal(offeredUnanswered.install, true);
  // Nothing answered: the line is the window's own.
  assert.deepEqual(logic.updateLine({ ...base, owner: null }), logic.updateLine(base));
  for (const line of [other.text, same.text]) assert.doesNotMatch(line, /\u2014/);
});

// SB-408: a .deb install is told the latest version and the commands that install it.
check(() => {
  const base = { available: false, reason: 'This copy was installed from the .deb package. Install the newer .deb from the releases page to update it.', enabled: false, state: 'idle', current: '0.7.114', version: null, error: null, needs_permission: false, restart_after: null };
  const command = 'gpgv … && \\\nsha256sum -c SHA256SUMS --ignore-missing && \\\nsudo apt install ./Fabric-Switchboard-0.7.115-linux-x64.deb';
  const fingerprint = '63B30DC324BD697487AA31944FAFB8AEC803B6A7';
  const out = logic.updateLine({ ...base, deb: { package: 'deb', current: '0.7.114', latest: '0.7.115', action: 'available', command, release_key_fingerprint: fingerprint, prerequisites: ['curl', 'gpgv'] } });
  assert.deepEqual(out.needs, ['curl', 'gpgv']);
  assert.equal(out.text, 'Version 0.7.115 is out; this copy is version 0.7.114. It was installed from the .deb package, which does not update itself: download, verify and install the newer one with this command.');
  // Review round 1, P1-2: one command whose steps are joined with &&, never a list of lines.
  assert.equal(out.command, command); assert.equal(out.fingerprint, fingerprint); assert.equal(out.check, true); assert.equal(out.restart, false);
  assert.match(logic.updateLine({ ...base, deb: { package: 'deb', current: '0.7.114', latest: '0.7.115', action: 'available', command, needs_migration: 'x' } }).text, /needs a step by a person first/);
  // Review round 1, P2-3: apt replaced the program since the window started.
  const installed = logic.updateLine({ ...base, deb: { package: 'deb', current: '0.7.114', action: 'installed' } });
  assert.equal(installed.text, 'A newer version was installed from the .deb package. Quit Switchboard and open it again to use it.');
  assert.equal(installed.command, undefined); assert.equal(installed.check, false);
  const current = logic.updateLine({ ...base, deb: { package: 'deb', current: '0.7.114', latest: '0.7.114', action: 'up_to_date' } });
  assert.equal(current.text, 'Version 0.7.114 is the latest. Switchboard checks again every five minutes; this copy came from the .deb package, so a newer version is installed with apt.');
  assert.equal(current.command, undefined);
  assert.equal(logic.updateLine({ ...base, deb: { package: 'deb', current: '0.7.114', action: 'failed', error: 'Could not check for updates. Switchboard tries again within the hour.' } }).text, 'Could not check for updates. Switchboard tries again within the hour.');
  // Not read yet: the copy's reason, as before.
  assert.deepEqual(logic.updateLine({ ...base, deb: null }), { text: base.reason, restart: false });
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
  // SB-47: the causes of a move the journal names.
  assert.equal(logic.eventDetail('switched_on_limit'), 'switched on a limit');
  assert.equal(logic.eventDetail('failover'), 'moved after a refusal');
  assert.equal(logic.eventDetail('unusable'), 'moved off an unusable account');
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

// SB-246 (SCN-073, SCN-044; operator decision 2026-11-04): usage counts are on by default and
// sent from the first start; a first-run notice says so once. It shows only when the build can
// send, Switchboard's switch is on and the notice waits; the tour waits for it (and for the
// status read) so one first-run card shows at a time.
check(() => {
  const pending = { available: true, enabled: true, notice_pending: true };
  assert.equal(logic.usageNotice(pending), true, 'first start of a release build: the notice');
  assert.equal(logic.usageNotice({ ...pending, notice_pending: false }), false, 'acknowledged: never again');
  assert.equal(logic.usageNotice({ ...pending, enabled: false }), false, 'switched off in Switchboard: nothing to announce');
  assert.equal(logic.usageNotice({ ...pending, available: false }), false, 'a development build sends nothing and shows nothing');
  assert.equal(logic.usageNotice({ available: true, enabled: true }), false, 'an older backend without the field: no notice');
  assert.equal(logic.usageNotice({ available: true, enabled: true, disclosed: false }), false, 'the old question field is not read');
  assert.equal(logic.usageNotice(null), false, 'unread or unreadable: no notice');
  assert.equal(logic.tourWaits(null, false), true, 'the tour waits for the status read');
  assert.equal(logic.tourWaits(null, true), false, 'a failed read never holds the tour');
  assert.equal(logic.tourWaits(pending, false), true, 'the notice comes first');
  assert.equal(logic.tourWaits({ ...pending, notice_pending: false }, false), false, 'acknowledged: the tour shows');
  assert.equal(logic.usageAnswerText(true), 'Anonymous usage analytics are on.');
  assert.equal(logic.usageAnswerText(false), 'Anonymous usage analytics are off for Switchboard.');
  i18n.setLocale('ru');
  assert.equal(logic.usageAnswerText(false), 'Анонимная аналитика использования для Switchboard выключена.');
  i18n.setLocale('en');
});
// The wiring: the notice sits above every page but Settings (WF-1.1: its switch lives there); OK
// acknowledges it, Turn off and Settings' checkbox set Switchboard's own switch; Escape never ends a tour that is not shown.
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.match(main, /if \(page !== 'settings' && usageNotice\(analyticsState\)\) main\.append\(usageNoticeCard\(\)\);/, 'the notice above the page');
  const card = main.slice(main.indexOf('function usageNoticeCard() {'), main.indexOf('let termsDismissed'));
  assert.match(card, /adapter\.analyticsNoticeSeen\(\)/, 'OK acknowledges the notice');
  assert.match(card, /setUsage\(false, 'page-title'\)/, 'Turn off turns the switch off');
  assert.match(card, /This applies to Switchboard only\./, 'and says the switch is this app\'s');
  const about = main.slice(main.indexOf('function analyticsPanel() {'), main.indexOf('function backupsPanel() {'));
  assert.match(about, /box\.addEventListener\('change', \(\) => setUsage\(box\.checked, 'analytics'\)\);/, 'Settings sets the same switch');
  assert.match(about, /t\('This switch applies to Switchboard only\.'\)/, 'for Switchboard only');
  assert.doesNotMatch(main, /every PassionCode\.ai tool on this computer/, 'no line says the switch is shared');
  assert.match(main, /analyticsState = await adapter\.setAnalytics\(wanted\);/, 'the choice goes to the backend');
  assert.match(main, /\(\) => usageAnswerText\(wanted\)/, 'and its line says what is now true');
  assert.match(main, /function tourShown\(\): boolean \{ return tourStep !== null && \(native \|\| demo\) && !tourWaits\(analyticsState, analyticsError\); \}/, 'the tour waits for the notice');
  assert.match(main, /event\.key === 'Escape' && !openMenu && tourShown\(\)/, 'Escape ends only a tour on screen');
  const refresh = main.slice(main.indexOf('function backgroundRefresh() {'), main.indexOf('setInterval(backgroundRefresh, 60_000);'));
  assert.match(refresh, /adapter\.analytics\(\)\.then\(\(value\) => \{ if \(clock\.accepts\(stamp\)\) \{ analyticsState = value;/, 'backgroundRefresh reads the analytics status');
  assert.match(main, /document\.addEventListener\('visibilitychange', \(\) => \{ if \(!document\.hidden\) backgroundRefresh\(\); \}\);/, 'and so does showing the window');
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
  // SB-265: the organization dialog's GitHub repositories.
  {
    const repos = [
      { name: 'app', archived: false, fork: false, folder: '/w/app' },
      { name: 'api', archived: false, fork: false, folder: null },
      { name: 'old', archived: true, fork: false, folder: null },
      { name: 'lib', archived: false, fork: true, folder: null },
      { name: 'kept', archived: true, fork: false, folder: '/w/kept' },
    ];
    assert.deepEqual(logic.githubDefaultSelection(repos), ['app', 'api', 'kept']);
    assert.deepEqual(logic.githubPlan(repos, new Set(['app', 'api', 'lib'])), { onDisk: ['/w/app'], toClone: ['api', 'lib'] });
    assert.deepEqual(logic.githubPlan(repos, new Set()), { onDisk: [], toClone: [] });
    assert.equal(logic.githubSource('Acme-Labs'), 'github.com/acme-labs');
  }
  // SB-264: folders picked together join in order, each once.
  assert.deepEqual(logic.withFolders(['/a'], ['/b', '/a', '/c', '/b']), ['/a', '/b', '/c']);
  assert.deepEqual(logic.withFolders(['/a'], []), ['/a']);
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
  // SB-289: every use the catalog names reads as a sentence of the window's, none passes through raw.
  const catalog = JSON.parse(readFileSync(new URL('../catalog/agents.json', import.meta.url), 'utf8'));
  const known = ['Decisions and code', 'Checking work', 'Bulk work', "Users' requests and content"];
  for (const m of catalog.openrouter_recommended) assert.ok(known.includes(m.use), `${m.id}: a use modelUse translates`);
  for (const use of known) assert.equal(logic.modelUse({ use }), use);
  assert.equal(logic.modelUse({}), '');
  assert.equal(logic.modelUse({ use: 'Something new' }), 'Something new');
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
// SB-158: the same attribution order as Snapshot::attribution: the person's word, then the team. SB-285: a domain only suggests.
check(() => {
  const account = (id, provider, email, org) => ({ id, label: email, provider, external_identity: { account_id: org, organization_id: org, email } });
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }, { id: 'o2', name: 'Beta', domains: [], provider_organizations: ['claude:team-b'] }], memberships: {} };
  assert.deepEqual(logic.attribution({}, account('a', 'claude', 'me@acme.example', 'x')), { organization: null, by: null, also: [] });
  assert.deepEqual(logic.attribution(snap, account('a', 'claude', 'dev@acme.example', 'team-x')), { organization: null, by: null, also: [] });
  assert.equal(logic.domainSuggestion(snap, account('a', 'claude', 'dev@acme.example', 'team-x'))?.id, 'o1');
  assert.deepEqual(logic.attribution(snap, account('b', 'claude', 'dev@acme.example', 'team-b')), { organization: 'o2', by: 'provider', also: [] });
  snap.memberships.b = 'personal';
  assert.deepEqual(logic.attribution(snap, account('b', 'claude', 'dev@acme.example', 'team-b')), { organization: null, by: 'person', also: [] });
  snap.memberships.a = 'o2';
  assert.deepEqual(logic.attribution(snap, account('a', 'claude', 'dev@acme.example', 'team-x')), { organization: 'o2', by: 'person', also: [] });
  assert.equal(logic.providerOrganization(account('c', 'codex', 'x@gmail.com', 'ws-1')), 'codex:ws-1');
  assert.equal(logic.accountDomain({ label: 'Work', external_identity: null }), null);
  assert.equal(logic.accountDomain({ label: 'x@Acme.Example.', external_identity: null }), 'acme.example');
  assert.equal(logic.inOrganizationFilter(snap, account('a', 'claude', 'dev@acme.example', 'team-x'), 'o2'), true);
  assert.equal(logic.inOrganizationFilter(snap, account('b', 'claude', 'dev@acme.example', 'team-b'), 'personal'), true);
  assert.deepEqual(logic.domainList(' @Acme.example, beta.example\nacme.example '), ['acme.example', 'beta.example']);
});
// SB-285: the same suggestion as Snapshot::domain_suggestion: none when no domain matches, when the
// person said where the account belongs, or when it already holds that organization.
check(() => {
  const account = (id, email, org = null) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: org, email } });
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }, { id: 'o2', name: 'Beta', domains: [], provider_organizations: ['claude:team-b'] }], memberships: {}, also_organizations: {} };
  const dev = account('d', 'dev@acme.example');
  assert.equal(logic.domainOrganization(snap, dev)?.id, 'o1');
  assert.equal(logic.domainSuggestion(snap, dev)?.id, 'o1');
  assert.equal(logic.domainSuggestion(snap, account('h', 'me@home.example')), null);
  assert.equal(logic.domainSuggestion({}, dev), null, 'no organization: nothing to suggest');
  // A team decides; the domain still suggests the other one, and placing in Acme's group is refused.
  const teamed = account('t', 'ops@acme.example', 'team-b');
  assert.equal(logic.attribution(snap, teamed).by, 'provider');
  assert.equal(logic.domainSuggestion(snap, teamed)?.id, 'o1');
  assert.equal(logic.placementAdmits(snap, ['o1'], teamed), false);
  snap.also_organizations.d = ['o1'];
  assert.equal(logic.domainSuggestion(snap, dev), null, 'it already works for Acme');
  delete snap.also_organizations.d;
  snap.memberships.d = 'personal';
  assert.equal(logic.domainSuggestion(snap, dev), null, 'the person answered');
});
// SB-109: a group takes accounts of its own organization, or personal ones; no organization, no rule.
check(() => {
  const account = (id, email) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: null, email } });
  const work = account('w', 'dev@acme.example'); const home = account('h', 'me@home.example');
  assert.equal(logic.groupAdmits({}, null, work), true);
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: {} };
  // SB-285: its domain suggests Acme without placing it: a personal group keeps it, and placing
  // it in Acme's group confirms the suggestion.
  assert.equal(logic.groupAdmits(snap, 'o1', work), false);
  assert.equal(logic.placementAdmits(snap, 'o1', work), true);
  assert.equal(logic.groupAdmits(snap, null, work), true);
  assert.equal(logic.groupAdmits(snap, null, home), true);
  assert.equal(logic.groupAdmits(snap, 'o1', home), false);
  assert.equal(logic.placementAdmits(snap, 'o1', home), false, 'an address on no domain of Acme');
  snap.memberships.w = 'o1';
  assert.equal(logic.groupAdmits(snap, 'o1', work), true);
  assert.equal(logic.groupAdmits(snap, null, work), false);
  snap.memberships.w = 'personal';
  assert.equal(logic.groupAdmits(snap, null, work), true);
  assert.equal(logic.placementAdmits(snap, 'o1', work), false, 'the person said personal');
  // SB-170: a group of Acme whose only account was placed as personal says so on its card.
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', [work]), true);
  snap.memberships.w2 = 'o1';
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', [work, account('w2', 'ops@acme.example')]), false);
  assert.equal(logic.groupSeatsElsewhere(snap, 'o1', []), false, 'an empty group has its own note');
  assert.equal(logic.groupSeatsElsewhere(snap, null, [work]), false);
});
// SB-160 (B-6, SCN-078): an ended seat of an organization names it.
check(() => {
  const account = (id, email) => ({ id, label: email, provider: 'claude', external_identity: { account_id: id, organization_id: null, email } });
  const work = account('w', 'dev@acme.example'); const home = account('h', 'me@home.example');
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: { w: 'o1' } };
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
  const snap = { organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example'] }], memberships: { a: 'o1', b: 'o1' } };
  const group = { name: 'Acme web', organization: 'o1' };
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], ['a', 'b']), "None of Acme web's accounts is signed in. Sessions in its folders are refused until one is.", 'a group whose seats all ended says so');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], ['a']), null, 'one seat still signed in');
  // DEC-0022 (SB-403): an account of another context serves the group too.
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b, home], ['a', 'b']), null, 'an account of another context still serves it');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b, home], ['a', 'b', 'h']), "None of Acme web's accounts is signed in. Sessions in its folders are refused until one is.");
  assert.equal(logic.groupSeatsEndedText(snap, group, [], []), null, 'an empty group has its own note');
  assert.equal(logic.groupSeatsEndedText(snap, { name: 'Mine', organization: null }, [home], ['h']), null, 'a personal group: no seats');
  assert.equal(logic.groupSeatsEndedText(snap, group, [a, b], undefined), null);
});
// SB-403: Add a seat offers the accounts here, suggested ones first; a domain never decides.
check(() => {
  const account = (id, email, pool = 'default', extra = {}) => ({ id, label: email, provider: 'claude', pool, enabled: true, external_identity: { account_id: id, organization_id: null, email }, ...extra });
  const snap = {
    organizations: [{ id: 'o1', name: 'Acme', domains: ['acme.example', 'acme.io'] }], memberships: {}, also_organizations: { b: ['o1'] },
    accounts: [account('z', 'zed@home.example'), account('a', 'ops@acme.io'), account('b', 'both@home.example'), account('g', 'in@home.example', 'acme-work'), account('off', 'off@acme.io', 'default', { enabled: false }), account('m', 'm@acme.io', 'machine_studio'), account('e', 'e@acme.io')],
    elsewhere: { e: 1 }, projects: [{ pool: 'acme-work', shared_accounts: [] }],
  };
  const choices = logic.seatChoices(snap, 'o1', 'acme-work');
  assert.deepEqual(choices.map((c) => [c.account.id, c.suggested, c.holds]), [['a', true, false], ['b', false, true], ['z', false, false]]);
  snap.organizations[0].domains = [];
  assert.deepEqual(logic.seatChoices(snap, 'o1', 'acme-work').map((c) => c.suggested), [false, false, false], 'no domain: nothing suggested, every account offered');
  assert.equal(logic.assignNotice('dev', { outside: [{ name: 'Mine' }], team: 'not_claimed', teammates: 2 }), 'Saved for dev. These groups keep using it: Mine. Their organizations are not among its own. The 2 other seats of its team stay where they were.');
  // The ordinary Claude Code's account cannot join a group, so it is never offered.
  assert.deepEqual(logic.seatChoices(snap, 'o1', 'acme-work', (a) => a.id === 'b').map((c) => c.account.id), ['a', 'z']);
  // SB-403 (round 1): a move or a call back says what its answer says.
  const name = (pool) => ({ default: 'in the default group', mine: 'in group “Mine”' })[pool] ?? pool;
  assert.equal(logic.placementNotice('dev', 'dev moved to group “Mine”.', { pool: 'mine', warnings: [{ code: 'outside_organization', pool: 'mine', group: 'Mine' }] }, name), 'dev moved to group “Mine”. These groups use it, though its organizations are not theirs: Mine.');
  assert.equal(logic.placementNotice('dev', 'dev is back on this computer, in Mine.', { pool: 'default', placed_error: 'Project not found.' }, name), 'dev is back on this computer, in the default group. It could not join the group you chose: Project not found.');
  assert.equal(logic.placementNotice('dev', 'dev moved.', undefined, name), 'dev moved.');
  // Round 2: a personal group holding an organization's account says so plainly.
  assert.equal(logic.placementNotice('dev', 'dev moved to group “Mine”.', { pool: 'mine', warnings: [{ code: 'outside_organization', pool: 'mine', group: 'Mine', group_personal: true }] }, name), 'dev moved to group “Mine”. It works for an organization; these personal groups use it anyway: Mine.');
  assert.equal(logic.assignNotice('dev', { outside: [], team: 'claimed', teammates: 1 }), 'Saved for dev. The other seat of its team followed.');
  assert.equal(logic.assignNotice('dev', undefined), 'Saved for dev.');
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
  assert.deepEqual(logic.backgroundReads('accounts'), ['onboarding', 'machines']);
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
  // SB-382: an organization's line says how many ran without its context, and why.
  assert.deepEqual(logic.contextLines({ hook: 'off', organizations: [{ context: 'org:a', live: 3, managed: 1, other: 2 }] }, (id) => (id === 'a' ? 'Acme' : undefined)), ['Acme: 3 in its folders, 1 launched by Switchboard in its group; 2 without its context: the terminal hook is off.']);
  assert.deepEqual(logic.contextLines({ hook: 'default_on', organizations: [{ context: 'org:a', live: 1, managed: 1, other: 0 }] }, () => 'Acme'), ['Acme: 1 in its folders, 1 launched by Switchboard in its group.']);
  assert.deepEqual(logic.contextLines(undefined, () => undefined), []);
  // Q3b: one chosen folder gives every repository in it, without module checkouts, each once.
  assert.deepEqual(logic.repositoriesUnder({ repositories: [{ folder: '/w/b', module_of: null }, { folder: '/w/a' }, { folder: '/w/a/vendor/x', module_of: '/w/a' }, { folder: '/w/b' }] }), ['/w/a', '/w/b']);
  // Q2 item 6: a group with accounts and no group record is named, the default group and machine groups are not.
  assert.deepEqual(logic.foldlessGroups({ projects, accounts: [{ id: 'a', pool: 'work' }, { id: 'b', pool: 'default' }, { id: 'c', pool: 'alpha' }, { id: 'd', pool: 'machine_studio' }, { id: 'e', pool: 'work' }, { id: 'f', pool: 'archive' }] }).map((g) => [g.pool, g.accounts.map((a) => a.id)]), [['archive', ['f']], ['work', ['a', 'e']]]);
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
// SB-404 review round 1, P1-1: a failed Observatory read is not read again on every render, and
// the window waits longer than the backend's own bounded calls (5 s + 15 s).
check(() => {
  const main = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.match(main, /if \(!observatoryKeys && !observatoryLoading && !observatoryError\) void loadObservatoryKeys\(\);/, 'a failed read waits for Read again');
  // Round 2: the error, absent or unreadable, and empty states each offer Read again.
  const panel = main.slice(main.indexOf('function observatoryPanel()'), main.indexOf('function observatoryAddDialog('));
  for (const state of ['if (observatoryError)', "if (observatoryKeys.state !== 'ok')", 'if (!observatoryKeys.keys.length)']) {
    const line = panel.split('\n').find((l) => l.includes(state));
    assert.ok(line && line.includes('readAgain()'), `${state} offers Read again`);
  }
  const adapterSource = readFileSync(new URL('../src/adapter.ts', import.meta.url), 'utf8');
  const deadline = Number(/observatoryKeys: \(\) => readWithDeadline\(invoke\('observatory_keys'\), (\d+)_?(\d*)\)/.exec(adapterSource)?.slice(1).join(''));
  const rust = readFileSync(new URL('../crates/switchboard-runtime/src/observatory_keys.rs', import.meta.url), 'utf8');
  const secs = (name) => Number(new RegExp(`const ${name}: Duration = Duration::from_secs\\((\\d+)\\)`).exec(rust)?.[1]);
  assert.ok(deadline >= (secs('PATH_TIMEOUT') + secs('LIST_TIMEOUT') + 5) * 1000, `window deadline ${deadline} ms over the backend's calls`);
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
    { shell: 'pwsh', path: 'C:\\Users\\ana\\Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1', state: 'constrained_language' },
  ];
  const inactive = logic.shellHookView({ ...base, hook: 'default_on', files: skipped });
  assert.equal(inactive.state, 'inactive');
  assert.equal(inactive.line, 'On, but no start file holds the hook yet, so terminals do not follow organization folders.');
  assert.equal(inactive.action.label, 'Turn off', 'the person can still turn the choice off');
  assert.deepEqual(inactive.files.map((f) => f.command), ['eval "$(switchboard shell init zsh)"', 'eval "$(switchboard shell init zsh)"', null, null, null, null]);
  assert.ok(inactive.files.every((f) => f.problem));
  assert.equal(inactive.files[0].text, '~/.zshrc is a link, so Switchboard did not change it. Add this line yourself:');
  assert.match(inactive.files[1].text, /ZDOTDIR/);
  assert.equal(inactive.files[2].text, '~/.bash_profile belongs to another user, so Switchboard did not change it.');
  assert.equal(inactive.files[3].text, '/etc/bashrc could not be read, so Switchboard did not change it.', 'a path outside the home stays whole');
  assert.match(inactive.files[4].text, /^PowerShell's execution policy stops profiles from loading/);
  assert.equal(inactive.files[5].text, 'PowerShell runs in Constrained Language Mode on this computer, so the hook is inactive there. Switchboard does not change this mode. Whoever manages this computer sets it.');
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
    body: "New terminals opened in Acme web's folders run claude and codex on Acme's accounts and history. Turn this off in Settings.",
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
  const asked = { available: true, enabled: true, notice_pending: true };
  const answered = { available: true, enabled: true, notice_pending: false };
  // the organization card waits for the usage notice and the tour
  check(() => {
    const [setup] = logic.orgSetups(noGroup);
    assert.equal(setup.organization.name, 'Acme');
    assert.equal(logic.orgSetupTitle(setup), 'Set up Acme on this computer');
    assert.deepEqual(setup.lines.map((l) => [l.id, l.state]), [['organization', 'done'], ['org_group', 'needs_person'], ['seat', 'needs_person'], ['hook', 'optional'], ['launch', 'optional']]);
    assert.equal(setup.lines[1].line, 'Acme has no group on this computer yet.');
    assert.deepEqual(logic.orgSetupAction(setup), { kind: 'group', organization: 'o-acme', label: '+ Group for Acme' });
    const shown = (usage, tourStep, page = 'accounts', hidden = [], failed = false) => logic.orgSetupShown({ page, organization: 'o-acme', hidden, usage, usageFailed: failed, tourStep });
    assert.equal(shown(asked, null), false, 'the usage notice comes first');
    assert.equal(shown(null, null), false, 'its status is still being read');
    assert.equal(shown(answered, 0), false, 'then the tour');
    assert.equal(shown(answered, null), true, 'then the card');
    assert.equal(shown(null, null, 'accounts', [], true), true, 'a failed status read does not hold the card back');
    assert.equal(shown(answered, null, 'orgs'), true, 'on top of Organizations too');
    assert.equal(shown(answered, null, 'agents'), false, 'nowhere else');
    assert.equal(shown(answered, null, 'accounts', ['o-acme']), false, 'Hide keeps it off Accounts');
    assert.equal(shown(answered, null, 'orgs', ['o-acme']), true, 'and on Organizations');
    // 2026-11-15: every account of the organization lives on another machine; nothing is asked here.
    const away = withOrg([{ id: 'org_group', state: 'optional', away: ['studio'] }, { id: 'seat', state: 'optional', away: ['studio'] }, { id: 'hook', state: 'optional', terminals: 'waiting' }, { id: 'launch', state: 'optional' }]);
    assert.deepEqual(logic.orgSetups(away), [], 'no card: nothing needs the person on this computer');
    const [seat] = logic.orgSetups(noSeat);
    // SB-403: an organization may have several domains or none; the seat is any account, never "your Acme address".
    assert.deepEqual(logic.orgSetupAction(seat), { kind: 'seat', pool: 'acme-work', group: 'Acme work', organization: { id: 'o-acme', name: 'Acme' }, label: 'Add a seat' });
    assert.equal(seat.lines[2].line, 'Give Acme a seat in the group “Acme work”: an account already on this computer, or a sign-in with any address.');
    assert.ok(!seat.lines[2].line.includes('your Acme address'));
    // SB-403: a group here, its accounts on another machine: optional, named, no card.
    const awaySeat = logic.orgSetups(withOrg([{ id: 'org_group', state: 'done', group }, { id: 'seat', state: 'optional', away: ['studio'] }, { id: 'hook', state: 'optional', terminals: 'active' }, { id: 'launch', state: 'optional' }]));
    assert.deepEqual(awaySeat, [], 'nothing needs the person here');
    // SB-228: the command is code of its own, not words in the line.
    assert.equal(seat.lines[3].line, "Terminals in Acme's folders work in its context. To turn this off:");
    assert.equal(seat.lines[3].command, 'switchboard shell off');
    assert.equal(seat.lines[2].command, undefined);
    const ended = logic.orgSetups(withOrg([{ id: 'org_group', state: 'done', group }, { id: 'seat', state: 'needs_person', ended: 2 }]))[0];
    // Note 6 of the wave 4 design pass: the card gives the account card's way out too.
    assert.equal(ended.lines[2].line, 'Accounts in “Acme work” must sign in again. If Acme removed a seat, ask its administrator.');
    // SB-282: an organization may have several groups; the first is the one the card signs in to.
    const two = logic.orgSetups(withOrg([{ id: 'org_group', state: 'done', group, groups: ['Acme web', 'Acme two'] }, { id: 'seat', state: 'needs_person' }]))[0];
    assert.equal(two.lines[1].line, 'Its groups are “Acme web”, “Acme two”.');
    assert.equal(logic.orgSetupAction(two)?.kind, 'seat', 'several groups need no choice');
    // SB-282: many-to-many links, as switchboard-core decides them.
    {
      const orgs = [{ id: 'o-a', name: 'A', domains: ['a.example'], created_at: 1 }, { id: 'o-b', name: 'B', domains: ['b.example'], created_at: 1 }];
      const acct = { id: 'x', label: 'dev@a.example', provider: 'claude', pool: 'one', external_identity: null };
      const snap = { organizations: orgs, memberships: { x: 'o-a' }, also_organizations: { x: ['o-b', 'o-a', 'o-missing'] }, projects: [{ pool: 'one', organizations: ['o-a', 'o-b'] }, { pool: 'two', organization: 'o-b', shared_accounts: ['x'] }, { pool: 'three' }] };
      const at = logic.attribution(snap, acct);
      assert.deepEqual([at.organization, at.also], ['o-a', ['o-b']], 'the first context and the rest from the person, each once');
      assert.equal(logic.groupAdmits(snap, ['o-b'], acct), true);
      assert.equal(logic.groupAdmits(snap, [], acct), false, 'a personal group refuses it');
      assert.deepEqual(logic.groupOrganizations(snap.projects[0]), ['o-a', 'o-b']);
      assert.deepEqual(logic.groupOrganizations(snap.projects[1]), ['o-b']);
      assert.deepEqual(logic.groupOrganizations(snap.projects[2]), []);
      assert.equal(logic.servesPool(snap, acct, 'two'), true);
      assert.equal(logic.servesPool(snap, acct, 'three'), false);
      assert.deepEqual(logic.sharedGroups(snap, acct).map((p) => p.pool), ['two']);
      assert.equal(logic.inOrganizationFilter(snap, acct, 'o-b'), true, 'the filter finds it under each of its organizations');
    }
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
    assert.equal(leads(noSeat, 'accounts', asked), false, 'the usage notice first: the card is not shown');
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
    assert.equal(unread({ usage: asked }), null, 'the usage notice first');
    assert.deepEqual(unread({ hidden: ['o-acme'] }), { id: 'o-old', name: 'Old' }, 'Hide on Accounts holds for that organization');
    assert.equal(unread({ hidden: ['o-acme', 'o-old'] }), null);
  });
  // the empty Accounts page on an organization machine leads with the seat
  check(() => {
    const signedIn = { claude: { status: 'available' }, codex: null };
    assert.deepEqual(logic.accountsEmptyKey({ backup: false, onboarding: noSeat, current: signedIn }), { lead: 'org_seat', organization: 'Acme', organization_id: 'o-acme', group: 'Acme work', pool: 'acme-work', line: 'Give Acme a seat in the group “Acme work”: an account already on this computer, or a sign-in with any address.', label: 'Add a seat' });
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
  // F1 (wave 4 design pass, SCN-079): an organization the person left on PassionCode.ai gets no setup card
  check(() => {
    const contoso = { id: 'o-contoso', name: 'Contoso' };
    const both = { ...noSeat, steps: [...noSeat.steps, { id: 'org_group', state: 'needs_person', organization: contoso, groups: [] }, { id: 'seat', state: 'needs_person', organization: contoso }] };
    const organizations = [{ id: 'o-acme', name: 'Acme', created_at: 1, vesta_id: 'v-acme' }, { id: 'o-contoso', name: 'Contoso', created_at: 5, vesta_id: null }];
    const status = { connected: true, released: [{ organization: 'o-contoso', at: 1000 }] };
    const released = logic.releasedOrganizations(status, organizations);
    assert.deepEqual(released, ['o-contoso']);
    assert.deepEqual(logic.releasedOrganizations({ connected: false, released: status.released }, organizations), [], 'a disconnect is not leaving');
    assert.deepEqual(logic.releasedOrganizations(null, organizations), [], 'not read yet: nothing is left out');
    assert.deepEqual(logic.orgSetups(both).map((s) => s.organization.name), ['Acme', 'Contoso'], 'without the list, both');
    assert.deepEqual(logic.orgSetups(both, released).map((s) => s.organization.name), ['Acme'], 'the left one has no card');
    const leftOnly = withOrg([{ id: 'org_group', state: 'needs_person', groups: [] }, { id: 'seat', state: 'needs_person' }]);
    assert.equal(logic.orgSetupLeads({ onboarding: leftOnly, page: 'orgs', hidden: [], usage: answered, usageFailed: false, tourStep: null, released: ['o-acme'] }), false, 'no card, so nothing leads');
    assert.equal(logic.accountsEmptyKey({ backup: false, onboarding: noSeat, current: { claude: { status: 'available' }, codex: null }, released: ['o-acme'] }).lead, 'add_current', 'the empty page does not lead with a left organization\'s seat');
    const unread = logic.orgSetupUnread({ onboarding: null, failed: true, organizations, page: 'accounts', hidden: [], usage: answered, usageFailed: false, tourStep: null, released });
    assert.deepEqual(unread, { id: 'o-acme', name: 'Acme' }, 'the read-failed card skips the left one too');
    // Above another organization's card, leaving is said at the top of Organizations too.
    assert.equal(logic.releasedSummary(status, organizations), 'Contoso is no longer in your PassionCode.ai organizations. It stays here with its groups until you remove it.');
    const fabrikam = { id: 'o-fab', name: 'Fabrikam', created_at: 2, vesta_id: null };
    assert.equal(logic.releasedSummary({ connected: true, released: [...status.released, { organization: 'o-fab', at: 900 }] }, [...organizations, fabrikam]), 'Contoso, Fabrikam are no longer in your PassionCode.ai organizations. They stay here with their groups until you remove them.');
    assert.equal(logic.releasedSummary({ connected: false, released: status.released }, organizations), null);
    assert.equal(logic.releasedSummary(null, organizations), null);
  });
  // F2, F3 (wave 4 design pass, SCN-076 step 7, VIS-21): one filled button per screen
  check(() => {
    const globex = { id: 'o-globex', name: 'Globex' };
    const two = { ...noSeat, steps: [...noSeat.steps, { id: 'org_group', state: 'needs_person', organization: globex, groups: [] }, { id: 'seat', state: 'needs_person', organization: globex }] };
    const screen = { page: 'orgs', hidden: [], usage: answered, usageFailed: false, tourStep: null };
    assert.equal(logic.orgSetupPrimary({ ...screen, onboarding: two }), 'o-acme', 'two cards: only the first one\'s action is primary');
    assert.equal(logic.orgSetupPrimary({ ...screen, page: 'accounts', hidden: ['o-acme'], onboarding: two }), 'o-globex', 'the first card shown');
    assert.equal(logic.orgSetupPrimary({ ...screen, onboarding: two, released: ['o-acme'] }), 'o-globex');
    const noAction = withOrg([{ id: 'org_group', state: 'needs_person', groups: ['Acme web', 'Acme two'] }, { id: 'seat', state: 'needs_person' }]);
    const twoFirstQuiet = { ...noAction, steps: [...noAction.steps, { id: 'org_group', state: 'needs_person', organization: globex, groups: [] }] };
    assert.equal(logic.orgSetupPrimary({ ...screen, onboarding: twoFirstQuiet }), 'o-globex', 'a card with no action of its own is skipped');
    assert.equal(logic.orgSetupPrimary({ ...screen, onboarding: ready }), null);
    // Accounts: while the organization's action leads, Add to Switchboard and Turn on for Claude Code go quiet.
    assert.equal(logic.accountsOrgActionLeads({ cardLeads: true, emptyLead: null }), true, 'the card leads');
    assert.equal(logic.accountsOrgActionLeads({ cardLeads: false, emptyLead: { lead: 'org_seat', organization: 'Acme', group: 'Acme work', pool: 'acme-work', line: '', label: '' } }), true, 'after Hide: the empty page\'s seat sign-in leads');
    assert.equal(logic.accountsOrgActionLeads({ cardLeads: false, emptyLead: { lead: 'add_current' } }), false, 'no organization lead: the page keeps its buttons');
    assert.equal(logic.accountsOrgActionLeads({ cardLeads: false, emptyLead: null }), false);
    // The first action offered, in the page's reading order, is the one filled button.
    assert.equal(logic.firstPrimary([['chain_preset', true], ['link_cli', true]]), 'chain_preset', 'Agents: the chain preset above Connect');
    assert.equal(logic.firstPrimary([['chain_preset', false], ['link_cli', true]]), 'link_cli');
    assert.equal(logic.firstPrimary([['chain_preset', false], ['link_cli', false]]), null);
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
    { const stale = await read; assert.equal(stale.state, 'stale'); assert.equal(stale.value, undefined); assert.equal(stale.error.message, 'x', 'a failure during an action comes back marked stale with its error (SB-232): afterRead decides'); }
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
    // Q2 item 32: Hermes and OpenCode are held by the OpenRouter key, never by an account.
    assert.equal(logic.stepReasonText({ ...step('skipped', 'no_usable_account'), agent: 'hermes' }), 'Skipped: no OpenRouter key ready');
    assert.equal(logic.stepReasonText({ ...step('skipped', 'no_usable_account'), agent: 'claude-code' }), 'Skipped: no account with room');
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, key_scope: 'group', spend: null }), 'Alpha web'), 'Ready: the key of Alpha web. Spend not read yet.');
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, key_scope: 'machine', spend: null })), "Ready: this computer's key. Spend not read yet.");
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, account_label: 'Claude API' })), 'Ready: Claude API. Spend not read yet.');
    // SB-245: an agent that asks for no approval never takes a Claude Code or Codex task (SB-82 D-6).
    assert.equal(logic.stepReasonText(step('ready', null, { account_label: 'Kimi personal', containment: 'none' })), 'Ready: Kimi personal. Takes no Claude Code or Codex task.');
    assert.equal(logic.stepReasonText(step('ready', null, { paid: true, key_scope: 'machine', spend: null, containment: 'none' })), "Ready: this computer's key. Spend not read yet. Takes no Claude Code or Codex task.");
    assert.equal(logic.stepReasonText(step('ready', null, { account_label: 'Work Max', containment: 'asks' })), 'Ready: Work Max');
    assert.equal(logic.stepReasonText(step('skipped', 'not_installed')), 'Skipped: not installed');
    assert.equal(logic.stepReasonText(step('skipped', 'quota_unknown')), 'Skipped: remaining quota not seen yet');
    assert.equal(logic.stepReasonText(step('skipped', 'permission_mode')), 'Skipped: asks for less approval than the agent before it');
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
    const nameOf = (id) => ({ hermes: 'Hermes', codex: 'Codex', 'kimi-code': 'Kimi Code', 'claude-code': 'Claude Code' })[id] ?? id;
    assert.equal(logic.handoverOutcomeText('offered'), 'handed over');
    assert.equal(logic.handoverOutcomeText('launch_failed'), 'the launch failed after the offer');
    assert.equal(logic.handoverOutcomeText('every_step_refused'), 'every step was refused');
    assert.equal(logic.handoverOutcomeText('no_usable_executor'), 'no agent could take it');
    assert.equal(logic.handoverOutcomeText('ceiling_reached'), "today's ceiling reached");
    assert.deepEqual(logic.handoverSkipped([{ agent: 'hermes', reason: 'no_usable_account' }, { agent: 'codex', reason: 'agent_disabled' }], nameOf), ['Hermes: no OpenRouter key ready', 'Codex: disabled under Installed']);
    assert.deepEqual(logic.handoverSkipped([{ agent: 'kimi-code', reason: 'permission_mode' }, { agent: 'claude-code', reason: 'quota_unknown' }], nameOf), ['Kimi Code: asks for less approval than the agent before it', 'Claude Code: remaining quota not seen yet']);
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
    // SB-232: a first read that failed while an action ran names itself instead of loading forever.
    assert.deepEqual(logic.afterRead({ view: null, error: '' }, { state: 'stale', error: new Error('x') }, false, text), { view: null, error: 'read: x' });
    assert.deepEqual(logic.afterRead(shown, { state: 'stale', error: new Error('x') }, false, text), shown, 'a view on screen stays and says nothing');
    assert.deepEqual(logic.afterRead({ view: null, error: 'old' }, { state: 'stale', error: new Error('x') }, false, text), { view: null, error: 'old' });
    assert.deepEqual(logic.afterRead({ view: null, error: '' }, { state: 'stale', error: new Error('x') }, true, text), { view: null, error: '' }, 'right after a save the next read reports');
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
// SB-366 (DEC-0017), an_account_on_another_machine_reads_its_copy_there: the last background
// read, never "disabled"; a failed read keeps its numbers' age and says the machine did not answer.
check(() => {
  i18nForShell.setLocale('en');
  const now = 100_000;
  const read = (over) => ({ used_percent: 42, usage_status: 'ok', enabled: true, as_of: now - 12 * 60, tried_at: now - 12 * 60, reason: null, ...over });
  assert.equal(logic.remoteUsageLine(read({}), 'studio', now), 'Used 42% on studio, 12 min ago');
  assert.equal(logic.remoteUsageLine(read({ as_of: now - 30 }), 'studio', now), 'Used 42% on studio, just now');
  assert.equal(logic.remoteUsageLine(read({ as_of: now - 3 * 3600 }), 'studio', now), 'Used 42% on studio, 3 h ago');
  assert.equal(logic.remoteUsageLine(read({ reason: 'unreachable', as_of: now - 3 * 86400 }), 'studio', now), 'Not read: studio did not answer, last read 3 d ago');
  assert.equal(logic.remoteUsageLine(read({ reason: 'unreachable', as_of: null }), 'studio', now), 'Not read yet: studio did not answer');
  assert.equal(logic.remoteUsageLine(read({ usage_status: 'rejected' }), 'studio', now), 'Needs a sign-in on studio');
  assert.equal(logic.remoteUsageLine(read({ enabled: false }), 'studio', now), 'Turned off on studio');
  assert.equal(logic.remoteUsageLine(null, 'studio', now), 'Not read on studio yet');
  i18nForShell.setLocale('ru');
  assert.equal(logic.remoteUsageLine(read({}), 'studio', now), 'Использовано 42% на машине studio, 12 мин назад');
  i18nForShell.setLocale('en');
});
// WF-6 (DEC-0016), a_backup_group_is_offered_marked_and_counted: a group that takes part may be
// marked backup (never the default group), and each backup group says how many accounts it lent.
check(() => {
  i18nForShell.setLocale('en');
  const available = [{ pool: 'default', name: 'default', chosen: true }, { pool: 'spare', name: 'Spare', chosen: true, backup: true }, { pool: 'apps', name: 'Apps', chosen: false }];
  const view = logic.balancingView({ on: true, groups: [], available, all_new: false, pass_minutes: 15, lent: [], recent: [], backup: [{ pool: 'spare', name: 'Spare', accounts: 3, lent: 2 }] });
  assert.deepEqual(view.groups.map((g) => [g.pool, g.canBackup, g.backup]), [['default', false, false], ['spare', true, true], ['apps', false, false]]);
  assert.deepEqual(view.backups, ['Spare is a backup group: 2 of 3 lent now.']);
  assert.match(logic.balancingNotice({ group: 'Spare', on: true, backup: true }), /^Spare is a backup group now: it lends its accounts first/);
  assert.equal(logic.balancingNotice({ group: 'Spare', on: false, backup: false }), 'Spare is no longer a backup group and takes part like the others.');
  // A status from a version before backup groups reads as none.
  assert.deepEqual(logic.balancingView({ on: true, groups: [], available: available.map(({ backup, ...g }) => g), all_new: false, pass_minutes: 15, lent: [], recent: [] }).backups, []);
  i18nForShell.setLocale('ru');
  assert.deepEqual(logic.balancingView({ on: true, groups: [], available, all_new: false, pass_minutes: 15, lent: [], recent: [], backup: [{ pool: 'spare', name: 'Spare', accounts: 3, lent: 2 }] }).backups, ['Группа «Spare» резервная: одолжено 2 из 3.']);
  i18nForShell.setLocale('en');
});
// SB-352 wave 2 (SCN-090), the_balancing_section_reads_every_state: reading, failed, off, on
// with the default group named, the accounts lent now, the latest returns (an account lent again
// is not shown as back), the notices, the activity entry and the default group's loans.
check(() => {
  i18nForShell.setLocale('en');
  assert.equal(logic.balancingView(null).state, 'reading');
  const failed = logic.balancingView(null, 'Switchboard could not read its own state.');
  assert.equal(failed.state, 'failed'); assert.ok(failed.retry); assert.equal(failed.error, 'Switchboard could not read its own state.');
  const available = [{ pool: 'default', name: 'default', chosen: false }, { pool: 'agents', name: 'Agents', chosen: false }, { pool: 'apps', name: 'Apps', chosen: false }];
  const off = logic.balancingView({ on: false, groups: [], available, all_new: false, pass_minutes: 15, lent: [], recent: [] });
  assert.equal(off.line, 'Off: no group takes part.');
  assert.deepEqual(off.groups.map((g) => g.label), ['Default group', 'Agents', 'Apps']);
  const chosen = available.map((g) => ({ ...g, chosen: g.pool !== 'apps' }));
  const lent = [{ account_id: 'a1', label: 'Work', provider: 'claude', from: 'default', from_name: 'default', to: 'agents', to_name: 'Agents', at: 1 }];
  const recent = [
    { at: 3, account: 'a1', provider: 'claude', from: 'default', to: 'agents', action: 'lend', reason: 'need', outcome: 'done' },
    { at: 2, account: 'a2', provider: 'claude', from: 'agents', to: 'default', action: 'return', reason: 'quiet', outcome: 'done' },
    { at: 1, account: 'a1', provider: 'claude', from: 'default', to: 'agents', action: 'return', reason: 'quiet', outcome: 'done' },
    { at: 0, account: 'gone', provider: 'claude', from: 'agents', to: 'default', action: 'return', reason: 'quiet', outcome: 'failed' },
  ];
  const on = logic.balancingView({ on: true, groups: [{ pool: 'default', name: 'default' }, { pool: 'agents', name: 'Agents' }], available: chosen, all_new: true, pass_minutes: 15, lent, recent }, null, [{ id: 'a2', label: 'Spare' }]);
  assert.equal(on.state, 'on'); assert.equal(on.line, 'On: 2 groups take part.'); assert.ok(on.allNew);
  assert.deepEqual(on.loans, ['Work is lent from Default group to Agents.']);
  assert.deepEqual(on.returns, ['Spare is back in Agents.'], 'a1 is lent again; a failed return is not shown');
  assert.equal(logic.balancingNotice({ group: 'Agents', on: true }), 'Agents takes part in balancing.');
  assert.match(logic.balancingNotice({ group: 'Agents', on: false }), /back in their own groups/);
  assert.equal(logic.balancingNotice({ on: true, allNew: true }), 'Groups made later take part in balancing.');
  assert.equal(logic.eventAction('balancing'), 'Balancing');
  assert.equal(logic.eventDetail('lent'), 'lent to another group');
  assert.equal(logic.eventDetail('returned'), 'back in its own group');
  // The default group's loans serve it, as a group's shares serve that group.
  const snap = { projects: [{ pool: 'agents', shared_accounts: [] }], default_shared: ['a3'] };
  assert.ok(logic.servesPool(snap, { id: 'a3', pool: 'agents' }, 'default'));
  assert.ok(!logic.servesPool(snap, { id: 'a4', pool: 'agents' }, 'default'));
  i18nForShell.setLocale('ru');
  assert.equal(logic.balancingView({ on: true, groups: [], available: chosen, all_new: false, pass_minutes: 15, lent: [], recent: [] }).line, 'Включено: участвуют 2 группы.');
  assert.equal(logic.balancingView({ on: true, groups: [], available: chosen.map((g) => ({ ...g, chosen: g.pool === 'agents' })), all_new: false, pass_minutes: 15, lent: [], recent: [] }).line, 'Включено: участвует 1 группа.');
  i18nForShell.setLocale('en');
});
// SB-419 (SCN-104, DEC-0028), the_feed_reads_every_state: reading, failed, off, on with the fed
// groups highest priority first, the rule with the person's targets, each move's line, the notices,
// the priority field and the arrows' geometry.
check(() => {
  i18nForShell.setLocale('en');
  assert.equal(logic.feedView(null).state, 'reading');
  const failed = logic.feedView(null, 'Switchboard could not read its own state.');
  assert.equal(failed.state, 'failed'); assert.ok(failed.retry);
  const base = { ready: 2, lookahead_minutes: 30, pass_minutes: 15, reservoir: [], moved: [], recent: [] };
  const off = logic.feedView({ ...base, on: false, fed: [], available: [{ pool: 'agents', name: 'Agents', fed: false, priority: null }] });
  assert.equal(off.line, 'No group is fed from the default group.');
  assert.deepEqual(off.arrows, []);
  assert.deepEqual(off.groups.get('agents'), { fed: false, priority: null });
  const fed = [{ pool: 'apps', name: 'Apps', priority: 2, off: false, providers: [] }, { pool: 'agents', name: 'Agents', priority: 1, off: false, providers: [] }];
  const on = logic.feedView({ ...base, on: true, fed, available: [{ pool: 'agents', name: 'Agents', fed: true, priority: 1 }, { pool: 'apps', name: 'Apps', fed: true, priority: 2 }] });
  assert.equal(on.state, 'on'); assert.equal(on.line, '2 groups are fed from the default group.');
  assert.deepEqual(on.arrows.map((a) => [a.pool, a.priority]), [['agents', 1], ['apps', 2]]);
  assert.match(on.rule, /^A fed group keeps 2 ready accounts: the default group lends it .* within 30 minutes, and takes back one that runs out once another serves the group\.$/);
  assert.equal(logic.feedBadge(1), 'Fed from Default · priority 1');
  const name = (pool) => ({ agents: 'Agents', default: 'Default group' })[pool] ?? pool;
  const line = (reason, from, to, outcome = 'done') => logic.feedMoveLine({ reason, from, to, outcome }, 'Work', name);
  assert.equal(line('feed', 'default', 'agents'), 'Work was placed in Agents from the default group.');
  assert.equal(line('park', 'agents', 'default'), 'Work ran out in Agents and is shared with the default group until its limit resets.');
  assert.equal(line('spent', 'default', 'agents'), 'Work ran out in Agents and went back; another ready account serves the group.');
  assert.equal(line('surplus', 'default', 'agents'), 'Work went back: Agents has more ready accounts than it keeps.');
  assert.equal(line('sign_in', 'default', 'agents'), 'Work went back: it waits for a sign-in.');
  assert.equal(line('rested', 'agents', 'default'), 'Work has room again and is no longer shared with the default group.');
  assert.equal(line('feed_off', 'agents', 'default'), 'Work went back: Agents is no longer fed.');
  assert.match(line('priority', 'default', 'agents'), /higher priority/);
  assert.equal(line('account_gone', 'default', 'agents'), 'Work went back to its own group.');
  assert.equal(line('feed', 'default', 'agents', 'failed'), 'Work could not be moved. The next pass tries again.');
  assert.equal(line('spent', 'default', 'agents', 'held'), 'Work ran out in Agents and stays: the account that would replace it was not placed.');
  assert.equal(logic.feedNotice({ group: 'Agents', on: true, priority: 1 }), 'Agents is fed from the default group, priority 1.');
  assert.match(logic.feedNotice({ group: 'Agents', on: false }), /back in their own groups/);
  assert.equal(logic.feedNotice({ group: 'Agents', priority: 3 }), 'Agents has priority 3 now.');
  assert.deepEqual(['1', ' 7 ', '99', '0', '100', '2.5', '', 'x'].map(logic.feedPriority), [1, 7, 99, null, null, null, null, null]);
  // Every arrow starts at the source's middle and ends 6 px before the edge, at its row's middle.
  assert.deepEqual(logic.feedArrows(2, 32, 56), ['M0 32 C28 32 28 16 50 16', 'M0 32 C28 32 28 48 50 48']);
  assert.deepEqual(logic.feedArrows(0, 32, 56), []);
  i18nForShell.setLocale('ru');
  assert.equal(on.line === '2 groups are fed from the default group.', true);
  assert.equal(logic.feedView({ ...base, on: true, fed: fed.slice(0, 1), available: [] }).line, 'Из группы по умолчанию подпитывается 1 группа.');
  assert.equal(logic.feedBadge(2), 'Подпитка · приоритет 2');
  i18nForShell.setLocale('en');
});
// SB-355 (SCN-092, DEC-0013), the_machine_groups_read_every_state: an account on another machine is
// in its machine's group, the accounts whose machine is not known are named, a machine pool is never
// offered as a group here, and a live read says what each copy there is.
check(() => {
  i18nForShell.setLocale('en');
  const machines = [{ pool: 'machine_studio-sshlg', name: 'studio-sshlg', aliases: ['sshlg@100.69.20.19'], created_at: 1 }];
  const accounts = [
    { id: 'a', pool: 'machine_studio-sshlg', label: 'claude9' },
    { id: 'b', pool: 'agents', label: 'claude10' },
    { id: 'c', pool: 'default', label: 'here' },
  ];
  const snap = { machines, accounts, elsewhere: { a: 1, b: 2 } };
  assert.equal(logic.machineOf(snap, accounts[0])?.name, 'studio-sshlg');
  assert.equal(logic.machineOf(snap, accounts[1]), undefined);
  assert.deepEqual(logic.unplacedAccounts(snap).map((a) => a.label), ['claude10']);
  assert.equal(logic.machineAliasesLine(machines[0]), 'Also reached as sshlg@100.69.20.19.');
  assert.equal(logic.machineAliasesLine({ aliases: [] }), '');
  assert.deepEqual(logic.poolChoices(accounts, [{ pool: 'agents', name: 'Agents' }]).map((c) => c.pool), ['default', 'agents'], 'a machine pool is never a group here');
  assert.equal(logic.machineLiveLine(undefined), '');
  assert.equal(logic.machineLiveLine({ reachable: true }), 'Read just now over ssh.');
  assert.match(logic.machineLiveLine({ reachable: false }), /Did not answer over ssh/);
  assert.equal(logic.machineCopyLine(undefined, true), '');
  assert.equal(logic.machineCopyLine({ pool: 'agents', enabled: true, used_percent: 41.6, usage_status: 'ok' }, false), '', 'not answered: nothing claimed');
  assert.equal(logic.machineCopyLine(null, true), 'Not on that machine any more.');
  assert.equal(logic.machineCopyLine({ pool: 'agents', enabled: true, used_percent: 41.6, usage_status: 'ok' }, true), 'There: agents, 42% used.');
  assert.equal(logic.machineCopyLine({ pool: 'default', enabled: true, used_percent: null, usage_status: null }, true), 'There: Default group.');
  assert.equal(logic.machineCopyLine({ pool: 'agents', enabled: false, used_percent: 3, usage_status: 'ok' }, true), 'Turned off there (agents).');
  assert.equal(logic.eventAction('machine'), 'Machine group');
  assert.equal(logic.eventDetail('joined'), "moved to its machine's group");
  assert.equal(logic.eventDetail('left'), 'back on this computer');
  i18nForShell.setLocale('ru');
  assert.equal(logic.machineCopyLine({ pool: 'agents', enabled: true, used_percent: 41.6, usage_status: 'ok' }, true), 'Там: agents, использовано 42%.');
  i18nForShell.setLocale('en');
});
// WF-1.2 (SB-363): a folding section keeps the person's choice across renders, else its default.
check(() => {
  const choices = new Map();
  assert.equal(logic.foldOpen(choices, 'switching', false), false);
  assert.equal(logic.foldOpen(choices, 'machine_studio', true), true);
  choices.set('switching', true); choices.set('machine_studio', false);
  assert.equal(logic.foldOpen(choices, 'switching', false), true, 'opened by the person stays open');
  assert.equal(logic.foldOpen(choices, 'machine_studio', true), false, 'closed by the person stays closed');
  assert.ok(logic.usableWhileBusy('fold-switching'), 'a section folds while an action runs');
});
// WF-2.1 (SB-364): this computer first (Claude, then Codex), then each machine by name, then given to agents; every account exactly once.
check(() => {
  const a = (id, provider, pool, created_at = 1) => ({ id, provider, pool, enabled: true, created_at, kind: 'oauth', label: id });
  const machines = [{ pool: 'machine_zeta', name: 'zeta-box', created_at: 1 }, { pool: 'machine_studio', name: 'studio-sshlg', created_at: 1 }, { pool: 'machine_empty', name: 'empty-box', created_at: 1 }];
  const accounts = [
    a('m-codex', 'codex', 'machine_studio'), a('h-codex', 'codex', 'default'), a('z-claude', 'claude', 'machine_zeta'),
    a('m-claude', 'claude', 'machine_studio', 2), a('h-claude-work', 'claude', 'work'), a('h-claude', 'claude', 'default'),
    a('given', 'claude', 'machine_studio'), a('given-here', 'codex', 'work'), a('orphan', 'claude', 'machine_gone'),
  ];
  const sections = logic.accountSections(accounts, { machines, assigned_to: { given: 'Frame Agent', 'given-here': 'Copylot' } });
  const ids = (groups) => groups.flatMap((g) => g.pools.flatMap((p) => p.accounts.map((x) => x.id)));
  assert.deepEqual(sections.here.map((g) => g.provider), ['claude', 'codex'], 'Claude, then Codex');
  assert.deepEqual(sections.here[0].pools.map((p) => p.pool), ['default', 'machine_gone', 'work'], 'an unknown machine pool stays here');
  assert.deepEqual(ids(sections.here), ['h-claude', 'orphan', 'h-claude-work', 'h-codex']);
  assert.deepEqual(sections.machines.map((m) => [m.name, m.count]), [['studio-sshlg', 2], ['zeta-box', 1]], 'by name; a machine with no account is not a section');
  assert.deepEqual(sections.machines[0].groups.map((g) => g.provider), ['claude', 'codex']);
  assert.deepEqual(ids(sections.machines[0].groups), ['m-claude', 'm-codex']);
  assert.deepEqual(sections.given.map((x) => x.id), ['given', 'given-here'], 'given to an agent wins over its machine');
  const all = [...ids(sections.here), ...sections.machines.flatMap((m) => ids(m.groups)), ...sections.given.map((x) => x.id)];
  assert.equal(all.length, accounts.length, 'nothing dropped, nothing doubled');
  assert.deepEqual([...all].sort(), accounts.map((x) => x.id).sort());
  const empty = logic.accountSections([], {});
  assert.deepEqual([empty.here, empty.machines, empty.given], [[], [], []]);
  assert.ok(logic.MACHINE_FOLD_LIMIT === 6);
});
// WF-2.2: one filter row: words of the label or e-mail, and Needs attention only.
check(() => {
  const account = { id: 'x', label: 'Studio team', external_identity: { email: 'Studio@Example.test' } };
  assert.ok(logic.accountMatches(account, { query: '' }));
  assert.ok(logic.accountMatches(account, { query: '  studio ' }));
  assert.ok(logic.accountMatches(account, { query: 'example team' }), 'every word, in label or e-mail');
  assert.ok(!logic.accountMatches(account, { query: 'north' }));
  assert.ok(!logic.accountMatches(account, { query: '', attention: new Set(['y']) }), 'attention only hides the rest');
  assert.ok(logic.accountMatches(account, { query: 'studio', attention: new Set(['x']) }));
  assert.ok(logic.accountMatches({ id: 'k', label: 'Key', external_identity: null }, { query: 'key' }));
});
// WF-3 (SB-365): the attention answer is read defensively; reason lines and actions are the window's.
check(() => {
  assert.deepEqual(logic.attentionView(undefined), { count: 0, items: [] }, 'a missing command is an empty list');
  assert.deepEqual(logic.attentionView({ count: 3, items: 'no' }), { count: 0, items: [] });
  const raw = { count: 3, items: [
    { account_id: 'a1', label: 'Meadow', provider: 'claude', pool: 'work', reason: 'sign_in', detail: 'x', since: 10, action: { tool: 'switchboard_login', window: 'sign_in' } },
    { account_id: null, label: null, provider: null, pool: null, reason: 'ceiling_question', detail: '', since: null, action: { tool: 'switchboard_ceiling_answer', window: 'answer_ceiling' } },
    { account_id: 'a2', reason: 'teleported', action: { tool: 't', window: 'sign_in' } },
    { account_id: 'a3', reason: 'usage_failing', action: { tool: 't', window: 'fly' } },
  ] };
  const view = logic.attentionView(raw);
  assert.equal(view.count, 2, 'an unknown reason or window drops that item only');
  assert.deepEqual(view.items.map((i) => i.reason), ['sign_in', 'ceiling_question']);
  assert.deepEqual([...logic.attentionAccounts(view, ['a9', 'a1'])].sort(), ['a1', 'a9']);
  assert.deepEqual([...logic.attentionAccounts(null)], []);
  for (const reason of ['sign_in', 'seat_ended', 'renewal_refused', 'usage_failing', 'move_unknown', 'refused_there', 'key_outside_tokens', 'ceiling_question']) {
    const line = logic.attentionReasonText(reason);
    assert.ok(line && !line.includes('\u2014'), reason);
  }
  assert.deepEqual(['sign_in', 'answer_ceiling', 'check_usage', 'open_machines'].map(logic.attentionActionLabel), ['Sign in again', 'Answer', 'Check again', 'Open Other machines']);
  // SB-388: a duplicate's item carries its session id and offers to close the quiet copy.
  const dup = logic.attentionView({ items: [{ account_id: null, label: 'alpha', reason: 'session_duplicate', session_id: 's-1', action: { tool: 'switchboard_session_close_copy', window: 'close_copy' } }] });
  assert.equal(dup.items[0].session_id, 's-1');
  assert.equal(logic.attentionActionLabel(dup.items[0].action.window), 'Close the quiet copy');
  assert.equal(view.items[0].session_id, null, 'an item without one reads null');
  // SB-389: both copies working: the older one is offered by its pid and the line says its command ends.
  const both = logic.attentionView({ items: [{ reason: 'session_duplicate', session_id: 's-2', close: { pid: 84681, why: 'older', command_age: 8700 }, action: { tool: 'switchboard_session_close_copy', window: 'close_copy' } }] }).items[0];
  const offer = logic.duplicateOffer(both);
  assert.equal(offer.pid, 84681); assert.equal(offer.label, 'Close the older copy'); assert.ok(offer.line.includes('2 h'), offer.line); assert.ok(!offer.disabled);
  const busy = logic.duplicateOffer(logic.attentionView({ items: [{ reason: 'session_duplicate', session_id: 's-3', close: null, action: { tool: 't', window: 'close_copy' } }] }).items[0]);
  // SB-472: both answering offers no button at all (never a disabled one) and says when it will.
  assert.equal(busy.label, null, 'both answering: no button'); assert.equal(busy.pid, null);
  assert.ok(busy.line.includes('finishes its turn'), busy.line); assert.ok(!busy.line.includes('\u2014'));
  assert.equal(logic.duplicateOffer({ session_id: null, close: null }).label, null, 'no session: no button');
  assert.ok(logic.duplicateOffer({ session_id: null, close: null }).line.includes('in its own window'), 'without a session the line says what the person can do');
  assert.equal(logic.duplicateOffer({ session_id: null, close: { pid: 1, why: 'older', command_age: 5 } }).label, null, 'no session: no button even with an offer');
  assert.equal(logic.duplicateOffer(dup.items[0]).label, 'Close the quiet copy', 'an older owner sends no offer: the quiet copy as before');
  assert.ok(!('disabled' in logic.duplicateOffer(dup.items[0])), 'the offer has no disabled state');
  assert.equal(logic.attentionView({ items: [{ reason: 'session_duplicate', session_id: 's', close: { pid: 'x', why: 'older' }, action: { tool: 't', window: 'close_copy' } }] }).items[0].close, null, 'a malformed offer is none');
  // SB-407: a stuck or gone board run carries its launch id and offers *Stop the run*.
  const gone = logic.attentionView({ items: [{ reason: 'session_gone', label: 'beta', launch_id: 'L-3', action: { tool: 'switchboard_dispatch_stop', window: 'dispatch_stop', command: 'switchboard dispatch stop --launch-id L-3' } }] }).items[0];
  assert.equal(gone.launch_id, 'L-3'); assert.equal(gone.reason, 'session_gone');
  assert.equal(gone.action.if_gone, undefined, 'only a true if_gone is kept');
  const goneOnly = logic.attentionView({ items: [{ reason: 'session_gone', launch_id: 'L-4', action: { tool: 'switchboard_dispatch_stop', window: 'dispatch_stop', if_gone: true } }] }).items[0];
  assert.equal(goneOnly.action.if_gone, true, 'a gone run only records its end');
  assert.equal(logic.attentionActionLabel(gone.action.window), 'Stop the run');
  // SB-420: a Claude Code item too large to write is kept, with no button (window none).
  const big = logic.attentionView({ items: [{ reason: 'keychain_too_large', provider: 'claude', item: 'ordinary', during: 'renewal', since: 9000, action: { tool: 'switchboard_status', window: 'none' } }] }).items[0];
  assert.equal(big.reason, 'keychain_too_large'); assert.equal(big.action.window, 'none'); assert.equal(big.since, 9000);
  assert.equal(logic.attentionActionLabel('none'), '', 'no button label');
  assert.ok(logic.attentionReasonText('keychain_too_large').startsWith("Claude Code's sign-in is too large to write to the Keychain safely"));
  assert.ok(!logic.attentionReasonText('keychain_too_large').includes('\u2014'));
  assert.ok(!logic.attentionReasonText('session_gone').includes('\u2014'));
  // SB-510: a watchdog finding keeps its code; one this window does not know is left out.
  const watched = logic.attentionView({ items: [
    { reason: 'watchdog', code: 'act_unfinished', count: 2, since: 7000, action: { tool: 'switchboard_duty', window: 'open_sessions' } },
    { reason: 'watchdog', code: 'something_new', action: { tool: 'switchboard_watchdog', window: 'open_sessions' } },
  ] }).items;
  assert.equal(watched.length, 1); assert.equal(watched[0].code, 'act_unfinished'); assert.equal(watched[0].since, 7000);
  assert.equal(logic.watchdogLine('act_unfinished'), "An act of the duty officer or a board run's stop was left half done more than 10 minutes ago.");
  assert.equal(logic.watchdogLine(undefined), logic.attentionReasonText('watchdog'));
  for (const code of ['owner_down', 'owner_not_answering', 'owner_stopped', 'act_unfinished', 'board_run_gone', 'duplicate_session']) {
    assert.ok(!logic.watchdogLine(code).includes('\u2014'), code);
  }
  assert.equal(logic.dispatchStopNotice('stopped'), 'Stopped the board run.');
  assert.equal(logic.dispatchStopNotice('already_ended'), 'The board run had already ended. Its end is recorded now.');
  assert.equal(logic.dispatchStopLineText({ reason: 'deadline', folder: '/w/app' }, 'app'), 'Stopped a board run in app: it reached its deadline.');
  assert.equal(logic.dispatchStopLineText({ reason: 'person', folder: '/w/app' }, 'app'), 'Stopped a board run in app at your request.');
  assert.equal(logic.dutyOutcomeText('already_ended'), 'it had already ended');
  // SB-509: a gone session with its history offers Continue; sessions on the proxy's old token offer a restart wave.
  const resume = logic.attentionView({ items: [{ reason: 'session_continuable', label: 'gamma', session_id: 's-2', launch_id: 'L-2', action: { tool: 'switchboard_session_continue', window: 'continue_session', command: 'switchboard sessions continue s-2' } }] }).items[0];
  assert.equal(resume.reason, 'session_continuable'); assert.equal(resume.session_id, 's-2');
  assert.equal(logic.attentionActionLabel(resume.action.window), 'Continue');
  const rotated = logic.attentionView({ items: [{ reason: 'token_rotated', since: 9000, count: 2, action: { tool: 'switchboard_sessions_restart_plan', window: 'restart_wave' } }] }).items[0];
  assert.equal(rotated.reason, 'token_rotated'); assert.equal(rotated.count, 2);
  assert.equal(logic.attentionActionLabel(rotated.action.window), 'Restart sessions\u2026');
  for (const reason of ['session_continuable', 'token_rotated']) assert.ok(!logic.attentionReasonText(reason).includes('\u2014'), reason);
  assert.equal(logic.dispatchExtendLineText('app'), 'Gave a board run in app ten more minutes: it still writes its log.');
  // SB-412, SB-417: the refused dispatches that still wait, one item that opens the session limit.
  const refused = logic.attentionView({ items: [{ reason: 'dispatch_refused', count: 3, freed: false, since: 9900, action: { tool: 'switchboard_capacity', window: 'open_capacity' } }] }).items[0];
  assert.equal(refused.count, 3); assert.equal(refused.freed, false);
  assert.equal(logic.attentionActionLabel('open_capacity'), 'Open the session limit');
  // SB-417 (design review 2026-11-17): the default share is a quarter of the limit rounded up, never above the limit.
  for (const [limit, share] of [[1, 1], [2, 1], [4, 1], [5, 2], [8, 2], [12, 3], [20, 5], [500, 125]]) assert.equal(logic.defaultBoardShare(limit), share, `limit ${limit}`);
  assert.equal(logic.defaultBoardShare(0), 1);
  assert.equal(logic.dispatchRefusedLine(refused), "3 board runs are waiting: this computer's session limit refused them. No session has ended since.");
  assert.equal(logic.dispatchRefusedLine({ count: 1, freed: true }), "A board run is waiting: this computer's session limit refused it. A session has ended since, so the next run can start.");
  assert.equal(logic.dutyOutcomeText('stopped'), null);
  i18nForShell.setLocale('ru');
  assert.equal(logic.attentionActionLabel('check_usage'), 'Проверить снова');
  assert.equal(logic.attentionActionLabel('dispatch_stop'), 'Остановить запуск');
  assert.ok(logic.dispatchRefusedLine({ count: 3, freed: false }).startsWith('3 запуска с доски ждут'), logic.dispatchRefusedLine({ count: 3, freed: false }));
  i18nForShell.setLocale('en');
});
// SB-384: the duty officer's reasons and outcomes in words.
check(() => {
  for (const reason of ['idle', 'old', 'waiting', 'stuck', 'duplicate']) {
    const line = logic.dutyReasonText(reason);
    assert.ok(line && line !== reason && !line.includes('\u2014'), reason);
  }
  assert.equal(logic.dutyReasonText('martian'), 'martian', 'an unknown reason shows its code');
  for (const outcome of [undefined, 'restarted', 'closed', 'planned']) assert.equal(logic.dutyOutcomeText(outcome), null, String(outcome));
  for (const outcome of ['still_running', 'closed_held_elsewhere', 'closed_no_folder', 'closed_no_claude', 'closed_not_restarted']) assert.ok(logic.dutyOutcomeText(outcome)?.length, outcome);
  // SB-561: a restart in a session host is as planned; one left running says why.
  assert.equal(logic.dutyOutcomeText('restarted_in_host'), null);
  for (const outcome of ['left_account', 'left_host_refused', 'left_host_busy', 'left_host_gone', 'left_not_prepared', 'closed_account']) {
    const text = logic.dutyOutcomeText(outcome);
    assert.ok(text?.length && text !== outcome, outcome);
  }
  i18nForShell.setLocale('ru');
  assert.equal(logic.dutyReasonText('idle'), 'закончила работу и простаивала');
  i18nForShell.setLocale('en');
});
// WF-2.4 and WF-1.1: the given-to-agent facts and the one switching line on Accounts.
check(() => {
  assert.deepEqual(logic.givenFacts({ organization: 'Northwind', machine: 'studio', launch: { group: 'Agents', folder: 'frame' }, lastActivity: '2h ago' }), ['Organization: Northwind', 'Machine: studio', 'Last launch: Agents, frame', 'Last activity 2h ago']);
  assert.deepEqual(logic.givenFacts({ organization: null, machine: null, launch: null, lastActivity: null }), ['Personal account', 'Machine not recorded', 'No launch recorded', 'No activity recorded']);
  assert.equal(logic.switchingLine([]), 'Automatic switching is off.');
  assert.equal(logic.switchingLine([{ enabled: false, pool: 'default' }]), 'Automatic switching is off.');
  assert.equal(logic.switchingLine([{ enabled: true, pool: 'default' }, { enabled: true, pool: 'default' }]), 'Automatic switching is on for 1 group.');
  assert.equal(logic.switchingLine([{ enabled: true, pool: 'default' }, { enabled: true, pool: 'web' }]), 'Automatic switching is on for 2 groups.');
});
// WF-11.1 (SB-373): an event's kinds, the filter row, days and pages.
check(() => {
  assert.deepEqual(logic.eventKinds({ action: 'rotation', detail: 'switched' }), ['switches']);
  assert.deepEqual(logic.eventKinds({ action: 'login', detail: 'failed' }), ['sign_ins', 'errors']);
  assert.deepEqual(logic.eventKinds({ action: 'machine', detail: 'sent' }), ['moves']);
  assert.deepEqual(logic.eventKinds({ action: 'usage', detail: 'observed' }), ['other']);
  assert.deepEqual(logic.eventKinds({ action: 'usage', detail: 'rate_limited' }), ['other', 'errors']);
  const events = [
    { at: 100, action: 'login', account_id: 'a1', detail: 'failed' },
    { at: 300, action: 'rotation', account_id: 'a2', detail: 'switched' },
    { at: 200, action: 'project', account_id: null, detail: 'saved' },
  ];
  assert.deepEqual(logic.filterEvents(events, { kind: 'all', account: 'all', query: '' }).map((e) => e.at), [300, 200, 100], 'newest first');
  assert.deepEqual(logic.filterEvents(events, { kind: 'errors', account: 'all', query: '' }).map((e) => e.at), [100]);
  assert.deepEqual(logic.filterEvents(events, { kind: 'all', account: 'a2', query: '' }).map((e) => e.at), [300]);
  assert.deepEqual(logic.filterEvents(events, { kind: 'all', account: 'all', query: 'ROTATION switched' }).map((e) => e.at), [300], 'every word, any case');
  const key = (s) => (s < 200 ? 'd1' : 'd2');
  const days = logic.eventsByDay([...events].sort((a, b) => b.at - a.at), key);
  assert.deepEqual(days.map((d) => [d.day, d.events.length]), [['d2', 2], ['d1', 1]]);
  const now = Date.UTC(2026, 10, 14, 12) / 1000;
  assert.equal(logic.relativeDay(logic.dayKey(now), now), 'today');
  assert.equal(logic.relativeDay(logic.dayKey(now - 86_400), now), 'yesterday');
  assert.equal(logic.relativeDay(logic.dayKey(now - 3 * 86_400), now), null);
  const many = Array.from({ length: 120 }, (_, i) => i);
  assert.deepEqual([logic.eventPage(many, 50, null).items.length, logic.eventPage(many, 50, null).more, logic.eventPage(many, 50, null).fetch], [50, true, false]);
  assert.deepEqual([logic.eventPage(many, 200, 77).more, logic.eventPage(many, 200, 77).fetch], [true, true], 'the owner has an older page');
  assert.deepEqual([logic.eventPage(many, 200, null).more], [false]);
  const page = logic.eventsPageView({ events: [{ at: 5, action: 'login', detail: 'ok', account_id: 'x' }, { at: 'bad' }], next_before: 4, kinds: ['sign_ins', 7] });
  assert.deepEqual(page, { events: [{ at: 5, action: 'login', account_id: 'x', detail: 'ok' }], next_before: 4, kinds: ['sign_ins'] });
  assert.deepEqual(logic.eventsPageView(null), { events: [], next_before: null, kinds: [] });
  assert.ok(logic.missingCommand('command events not found'));
  assert.ok(!logic.missingCommand('Network error'));
  assert.deepEqual(logic.sortSessions([{ state: 'ended', launched_at: 9 }, { state: 'running', launched_at: 1 }, { state: 'running', launched_at: 5 }]).map((s) => s.launched_at), [5, 1, 9]);
});
// WF-11.2, WF-7 and WF-8: folded Statistics tables, group key blocks, key tails, spend bars.
check(() => {
  const rows = Array.from({ length: 25 }, (_, i) => i);
  assert.deepEqual([logic.statRowsShown(rows, false).rows.length, logic.statRowsShown(rows, false).hidden], [10, 15]);
  assert.deepEqual([logic.statRowsShown(rows, true).rows.length, logic.statRowsShown(rows, true).hidden], [25, 0]);
  assert.equal(logic.statRowsShown(rows.slice(0, 4), false).hidden, 0);
  const projects = [{ pool: 'a' }, { pool: 'b' }, { pool: 'c' }];
  const blocks = logic.groupKeyBlocks(projects, ['b', null], ['c']);
  assert.deepEqual([blocks.listed.map((p) => p.pool), blocks.rest.map((p) => p.pool)], [['b', 'c'], ['a']]);
  assert.equal(logic.keyTail('sk-or-v1-0e6…96f3'), '…96f3');
  assert.equal(logic.keyTail('Machine key'), null);
  assert.equal(logic.spendShare(2.5, 10), 25);
  assert.equal(logic.spendShare(12, 10), 100);
  assert.equal(logic.spendShare(null, 10), null);
  assert.equal(logic.spendShare(1, 0), null);
  const line = logic.groupStateLine({ accounts: 3, withRoom: 2, repositories: 12, chain: 'Claude Code → Codex' });
  assert.ok(line.includes('3 accounts') && line.includes('12'), line);
  assert.ok(!line.includes('\u2014'));
});
// SB-379 (Q4): the session tree's order and the watch's reasons.
check(() => {
  const s = (launch_id, parent, started_at) => ({ launch_id, parent, started_at });
  const rows = logic.sessionTreeRows([s('child', 'root', 20), s(null, null, 5), s('root', null, 10), s('grand', 'child', 30), s('orphan', 'gone', 1)]);
  assert.deepEqual(rows.map((r) => [r.session.launch_id, r.depth]), [['orphan', 0], [null, 0], ['root', 0], ['child', 1], ['grand', 2]]);
  assert.equal(logic.surfaceLabel('fabric'), 'Fabric board');
  assert.equal(logic.surfaceLabel(null), 'Not started by Switchboard');
  const view = logic.attentionView({ items: [{ reason: 'session_duplicate', detail: 'x', action: { tool: 'switchboard_sessions', window: 'open_sessions' } }] });
  assert.equal(view.count, 1);
  assert.equal(logic.attentionActionLabel('open_sessions'), 'Open sessions');
  assert.ok(logic.attentionReasonText('session_stuck').includes('30 minutes'));
});
// Sessions wave 1 (2026-11-16): a row's colour follows its state; what was not read is said; a restart's sessions read in words.
check(() => {
  i18n.setLocale('en');
  assert.equal(logic.sessionTone('working'), 'ok');
  for (const why of ['duplicate', 'stuck', 'ready_to_continue', 'limit']) assert.equal(logic.sessionTone(why), 'warn', why);
  assert.equal(logic.sessionTone('gone'), 'danger');
  for (const why of ['quiet', 'monitoring', 'closed_idle', 'ended']) assert.equal(logic.sessionTone(why), 'muted', why);
  assert.equal(logic.sessionDegradedText([]), '');
  assert.equal(logic.sessionDegradedText(['recovery']), 'Stops on a limit could not be read, so they are not listed.');
  assert.ok(logic.sessionDegradedText(['recovery', 'tree']).endsWith('Part of this list could not be read, so some sessions may be missing.'));
  assert.equal(logic.sessionGroupTitle('needs_you'), 'Needs attention');
  assert.equal(logic.sessionGroupTitle('waiting'), 'Waiting');
  assert.equal(logic.restartStateText('back', null), 'Came back');
  assert.equal(logic.restartStateText('failed', 'not_back'), 'Not back five minutes after it ended');
  assert.equal(logic.restartStateText('failed', 'something new'), 'Not back');
  assert.equal(logic.restartStateText('skipped', 'still_busy'), 'Left as it was: busy for a day');
  assert.equal(logic.restartSkipText('host_busy'), 'its window did not answer; plan again');
  assert.equal(logic.restartTone('continuing'), 'ok');
  assert.equal(logic.restartTone('failed'), 'danger');
  assert.equal(logic.restartTone('pending'), 'muted');
  assert.equal(logic.sessionWhyText({ why: 'closed_restart' }, 0), 'Ended by a restart and not back, can continue');
  assert.equal(logic.sessionWhyText({ why: 'ended_seen' }, 0), 'Ended outside Switchboard, can continue');
  assert.equal(logic.sessionWhyText({ why: 'reboot' }, 0), 'Cut by a computer restart, continued after it');
  assert.equal(logic.sessionWhyText({ why: 'moved_on' }, 0), 'Went on after its limit stop and ended, can continue');
  assert.equal(logic.sessionWhyText({ why: 'no_transcript' }, 0), 'Stopped on a limit, its history is gone');
  assert.equal(logic.restartStateText('failed', 'gone'), 'Not back: it ended before its turn did');
  i18n.setLocale('ru');
  assert.equal(logic.restartStateText('back', null), 'Вернулась', 'never the navigation «Назад»');
  assert.equal(logic.sessionGroupTitle('waiting'), 'Ждут');
  i18n.setLocale('en');
});
// SB-401: the pay-per-token group: its name, its spending line, and the list-price column.
check(() => {
  i18n.setLocale('en');
  assert.equal(logic.groupDisplayName({ name: 'Pay-per-token keys', tokens: true }), 'Pay-per-token keys');
  assert.equal(logic.poolName('tokens', [{ pool: 'tokens', name: 'Pay-per-token keys', tokens: true }]), 'Pay-per-token keys');
  assert.equal(logic.tokensSpendLine(null).text, 'No daily ceiling. Set one before you turn this group on.');
  assert.equal(logic.tokensSpendLine({ usd: 5, spent_today_usd: 1.25, reached: false }).text, '$1.25 of $5.00 spent today');
  const reached = logic.tokensSpendLine({ usd: 5, spent_today_usd: 5.1, reached: true });
  assert.equal(reached.reached, true);
  assert.ok(reached.text.startsWith('Ceiling reached: $5.10 of $5.00 today.'));
  assert.equal(logic.tokensSpendLine({ usd: 5, spent_today_usd: null }).text, 'Daily ceiling $5.00 · spent today not read');
  assert.equal(logic.statCost({}), '–');
  assert.equal(logic.statCost({ cost_micros: 1_234_000 }), '$1.23');
  assert.equal(logic.statCost({ cost_micros: 1_234_000, cost_estimated: 1 }), '≈$1.23');
  const z = { requests: 1, ok: 1, limited: 0, failed: 0, input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 };
  const rows = [{ cells: ['a'], counts: { ...z, cost_micros: 10 } }, { cells: ['b'], counts: { ...z } }, { cells: ['c'], counts: { ...z, cost_micros: 99 } }];
  assert.deepEqual(logic.sortStatRows(rows, { column: 4, descending: true }, true).map((r) => r.cells[0]), ['c', 'a', 'b'], 'by list price, the last column');
  assert.deepEqual(logic.sortStatRows(rows, { column: 6, descending: true }, false).map((r) => r.cells[0]), ['c', 'a', 'b']);
  // Round 2: on with no ceiling refuses, and says so; the reached question names the group after.
  assert.equal(logic.tokensSpendLine(null, true).reached, true);
  assert.ok(logic.tokensSpendLine(null, true).text.startsWith('On, but refusing every request'));
  assert.equal(logic.ceilingQuestion({ value: 5, kind: 'project', scope: 'Pay-per-token keys', tokens: true }).text, 'The daily ceiling of Pay-per-token keys, $5.00, is reached.');
  // Groups reads the ceilings again only while the pay-per-token group shows its spending.
  assert.deepEqual(logic.backgroundReads('projects'), ['machines']);
  assert.deepEqual(logic.backgroundReads('projects', true), ['ceilings', 'machines']);
  // A move the pay-per-token rule refuses is locked: a key goes only there, nothing else goes there, its keys stay.
  const groups = [{ pool: 'apps', name: 'Apps' }, { pool: 'tokens', name: 'Pay-per-token keys', tokens: true }];
  const pick = (targets) => Object.fromEntries(targets.map((x) => [x.pool, x.blocked]));
  assert.deepEqual(pick(logic.moveTargets('default', groups, false, () => true, true)), { default: false, apps: true, tokens: false }, 'a key in default (now) goes only to the pay-per-token group');
  assert.deepEqual(pick(logic.moveTargets('default', groups, false, () => true, false)), { default: false, apps: false, tokens: true }, 'nothing else goes there');
  assert.deepEqual(pick(logic.moveTargets('tokens', groups, false, () => true, true)), { default: true, apps: true, tokens: false });
  // A key from before, with no pay-per-token group yet: the move offers it, and the move makes it.
  assert.deepEqual(pick(logic.moveTargets('default', [groups[0]], false, () => true, true)), { default: false, apps: true, [logic.TOKENS_TARGET]: false }, 'the move to a missing pay-per-token group is offered');
  assert.equal(logic.movePool(logic.TOKENS_TARGET), 'tokens', 'the move names tokens, which makes the group');
  assert.equal(logic.movePool('apps'), 'apps');
  // An ordinary group whose pool id is `tokens` is one row, and the made group another.
  const named = logic.moveTargets('default', [{ pool: 'tokens', name: 'Tokens' }], false, () => true, true);
  assert.deepEqual(named.map((x) => [x.pool, x.blocked]), [['default', false], ['tokens', true], [logic.TOKENS_TARGET, false]]);
  // A key from before, outside the group, offers the move and never Select; one already selected stays selected.
  const old = { provider: 'claude', kind: 'api_key', enabled: true, external_identity: null };
  assert.equal(logic.primaryAction(old, { current: false, selected: false, signIn: false, project: false, outsideTokens: true }), 'move_tokens');
  assert.equal(logic.primaryAction(old, { current: false, selected: true, signIn: false, project: false, outsideTokens: true }), 'selected');
  assert.equal(logic.primaryAction(old, { current: false, selected: false, signIn: false, project: true, outsideTokens: false }), 'select');
  assert.equal(logic.keyOutsideTokens({ provider: 'claude', kind: 'api_key', pool: 'default' }, 'tokens'), true);
  assert.equal(logic.keyOutsideTokens({ provider: 'claude', kind: 'api_key', pool: 'tokens' }, 'tokens'), false);
  assert.equal(logic.keyOutsideTokens({ provider: 'claude', kind: 'api_key', pool: 'default' }, null), true);
  assert.equal(logic.keyOutsideTokens({ provider: 'codex', kind: 'api_key', pool: 'default' }, null), false);
  const outside = logic.attentionView({ items: [{ account_id: 'k', label: 'Old key', provider: 'claude', pool: 'default', reason: 'key_outside_tokens', detail: '', action: { tool: 'switchboard_account_move', window: 'move_tokens' } }] });
  assert.equal(outside.count, 1);
  assert.equal(logic.attentionActionLabel(outside.items[0].action.window), 'Move to Pay-per-token keys');
  assert.deepEqual(pick(logic.moveTargets('default', [groups[0]], false, () => true, false)), { default: false, apps: false }, 'no synthetic target for anything else');
  // The window never selects or launches a Claude API key outside the pay-per-token group.
  const key = (pool) => ({ provider: 'claude', kind: 'api_key', pool });
  assert.equal(logic.keyLaunch(key('default'), 'tokens', true, true), 'outside', 'a key from before is never launched from the default group');
  assert.equal(logic.keyLaunch(key('default'), null, false, true), 'outside');
  assert.equal(logic.keyLaunch(key('tokens'), 'tokens', false, true), 'select');
  assert.equal(logic.keyLaunch(key('tokens'), 'tokens', true, true), 'managed');
  assert.equal(logic.keyLaunch(key('tokens'), 'tokens', true, false), 'down');
  assert.equal(logic.keyLaunch({ provider: 'claude', kind: 'oauth', pool: 'default' }, 'tokens', false, true), null);
  assert.equal(logic.keyLaunch({ provider: 'codex', kind: 'api_key', pool: 'default' }, 'tokens', false, true), null);
  assert.equal(logic.poolChoices([], groups).find((c) => c.pool === 'tokens').name, 'Pay-per-token keys');
  i18n.setLocale('ru');
  assert.equal(logic.groupDisplayName({ name: 'Pay-per-token keys', tokens: true }), 'Ключи с оплатой за токены');
  assert.equal(logic.poolChoices([], groups).find((c) => c.pool === 'tokens').name, 'Ключи с оплатой за токены');
  assert.equal(logic.groupDisplayName({ name: 'Pay-per-token keys' }), 'Pay-per-token keys', 'a group the person named so stays as typed');
  i18n.setLocale('en');
});
// SB-473 to SB-476: a move to another machine is chosen, read and batched.
check(() => {
  const machines = [{ pool: 'machine_nicegram', name: 'nicegram@10.0.0.2', aliases: ['ng'], created_at: 50 }, { pool: 'machine_studio', name: 'studio', created_at: 10 }];
  const targets = [{ machine: 'ng', known_by: ['moves'], last_at: 90 }, { machine: 'linux-box', known_by: ['moves'], last_at: 80, program: '/usr/bin/switchboard' }, { machine: 'gpu', known_by: ['dispatch'], last_at: 70 }];
  const choices = logic.machineChoices(machines, targets);
  assert.deepEqual(choices.map((c) => c.name), ['ng', 'linux-box', 'gpu', 'studio'], 'each machine once, newest first; an alias of a machine group is that group');
  assert.equal(choices[0].detail, 'accounts live there'); assert.equal(choices[1].detail, 'accounts moved there before'); assert.equal(choices[2].detail, 'runs started there');
  assert.deepEqual(logic.machineChoices(machines, targets, machines[0]).map((c) => c.name), ['linux-box', 'gpu', 'studio'], 'the machine the account is on is left out, by any of its names');
  assert.deepEqual(logic.machineChoices([], []), [], 'nothing known: only the typed entry is left');
  const status = { target: 'ng', groups: [{ pool: 'machine_mac', name: 'mac', organization: null, accounts: 2, folders: 0 }, { pool: 'zeta', name: 'Zeta', organization: 'PassionCode.ai', accounts: 1, folders: 2 }, { pool: 'default', name: null, organization: null, accounts: 3, folders: 0 }, { pool: 'apps', name: 'Apps', organization: null, accounts: 2, folders: 1 }] };
  const read = logic.remoteGroupChoices(status, 'zeta');
  assert.deepEqual(read.choices.map(([pool]) => pool), ['default', 'apps', 'zeta'], 'the default group first, machine groups there left out, the rest by name');
  assert.equal(read.choices[0][1], 'Default group · 3 accounts'); assert.equal(read.choices[2][1], 'Zeta · PassionCode.ai · 1 account');
  assert.equal(read.chosen, 'zeta', "the account's own group when that machine has it");
  assert.equal(logic.remoteGroupChoices(status, 'nowhere').chosen, 'default'); assert.equal(logic.remoteGroupChoices({ target: 'x', groups: [] }, null).choices.length, 1);
  const accounts = [{ id: 'a', kind: 'oauth', enabled: true, pool: 'default' }, { id: 'b', kind: 'api_key', enabled: true, pool: 'tokens' }, { id: 'c', kind: 'oauth', enabled: false, pool: 'default' }, { id: 'd', kind: 'oauth', enabled: false, pool: 'machine_studio' }, { id: 'e', kind: 'oauth', enabled: true, pool: 'apps' }];
  const snap = { accounts, machines, elsewhere: { d: 1, e: 2 } };
  assert.deepEqual(logic.movableAccounts(snap).map((a) => a.id), ['a', 'd'], 'a subscription that is on here, or one on another machine; never a key, one off or one marked elsewhere without a machine');
  assert.deepEqual(logic.movableAccounts(snap, machines[1]).map((a) => a.id), ['a'], 'never one already on the machine it goes to');
  for (const c of choices) assert.ok(!c.detail.includes('\u2014'));
});
// SB-550 (DEC-0033): an account on another machine shows its copy there: bar, share and age from
// the last read, the read's age under it; this computer's own older check only while there is no
// read, and said so. The machine card says what the last read found; Needs attention takes the
// machine reasons.
check(() => {
  i18nForShell.setLocale('en');
  const now = 100_000;
  const read = (over) => ({ used_percent: 42, observed_at: now - 300, usage_status: 'ok', enabled: true, as_of: now - 120, tried_at: now - 120, reason: null, ...over });
  const own = { used: 17, observedAt: now - 8 * 86400 };
  const shared = logic.remoteUsageView(read({}), own, 'studio', now);
  assert.deepEqual([shared.percent, shared.at, shared.tone, shared.ownCheck], [42, now - 300, 'available', false], "the copy there, never this computer's old check");
  assert.equal(shared.line, 'On studio, read 2 min ago');
  assert.equal(logic.remoteUsageView(read({ as_of: now - 20 }), own, 'studio', now).line, 'On studio, read just now');
  assert.equal(logic.remoteUsageView(read({ observed_at: undefined }), null, 'studio', now).at, now - 120, 'without its own time, the read time');
  assert.equal(logic.remoteUsageView(read({ used_percent: 85 }), null, 'studio', now).tone, 'low');
  assert.equal(logic.remoteUsageView(read({ used_percent: 100 }), null, 'studio', now).tone, 'blocked');
  const rejected = logic.remoteUsageView(read({ usage_status: 'rejected' }), null, 'studio', now);
  assert.deepEqual([rejected.tone, rejected.line], ['failed', 'Needs a sign-in on studio']);
  const silent = logic.remoteUsageView(read({ reason: 'unreachable', as_of: now - 3 * 3600 }), own, 'studio', now);
  assert.deepEqual([silent.percent, silent.tone, silent.line], [42, 'stale', 'Not read: studio did not answer, last read 3 h ago']);
  // Review of PR #861 (P2-4): once that machine answered, its read speaks alone, with or without a
  // figure: this computer's own 17% from 8 days ago never comes back.
  const signIn = logic.remoteUsageView(read({ used_percent: null, usage_status: 'rejected' }), own, 'studio', now);
  assert.deepEqual([signIn.percent, signIn.tone, signIn.ownCheck, signIn.line], [null, 'failed', false, 'Needs a sign-in on studio']);
  const unmeasured = logic.remoteUsageView(read({ used_percent: null }), own, 'studio', now);
  assert.deepEqual([unmeasured.percent, unmeasured.at, unmeasured.tone, unmeasured.ownCheck, unmeasured.line], [null, null, 'unknown', false, 'studio has no usage figure for it yet']);
  const noCopy = logic.remoteUsageView(read({ used_percent: null, usage_status: null, enabled: null, listed: false }), own, 'studio', now);
  assert.deepEqual([noCopy.percent, noCopy.ownCheck, noCopy.line], [null, false, 'Not in the account list of studio at its last read']);
  const offThere = logic.remoteUsageView(read({ used_percent: null, enabled: false }), own, 'studio', now);
  assert.deepEqual([offThere.percent, offThere.ownCheck, offThere.line], [null, false, 'Turned off on studio']);
  const neverAnswered = logic.remoteUsageView(read({ used_percent: null, as_of: null, reason: 'unreachable' }), own, 'studio', now);
  assert.deepEqual([neverAnswered.percent, neverAnswered.ownCheck, neverAnswered.line], [17, true, "This computer's last check · Not read yet: studio did not answer"]);
  const before = logic.remoteUsageView(null, own, 'studio', now);
  assert.deepEqual([before.percent, before.tone, before.ownCheck], [17, 'stale', true]);
  assert.equal(before.line, "This computer's last check · Not read on studio yet");
  const nothing = logic.remoteUsageView(null, null, 'studio', now);
  assert.deepEqual([nothing.percent, nothing.tone, nothing.line], [null, 'unknown', 'Not read on studio yet']);
  // The machine card's line.
  const machineRead = (over) => ({ as_of: now - 30, tried_at: now - 30, reason: null, failures: 0, call: 'status', sessions: { counts: { needs_you: 1, running: 3, waiting: 0, ended: 9 }, troubled: [] }, health: { state: 'ok', failing: [] }, ...over });
  assert.equal(logic.machineReadLine(machineRead({}), now), 'Sessions: 3 running, 0 waiting, 1 need you. No failing health check there. Read just now.');
  assert.equal(logic.machineReadLine(machineRead({ health: { state: 'fail', failing: ['crashes', 'owner'] } }), now), 'Sessions: 3 running, 0 waiting, 1 need you. Needs a look there: it crashed in the last day, its Switchboard is not running. Read just now.');
  assert.equal(logic.machineReadLine(machineRead({ call: 'accounts', sessions: null, health: null, as_of: now - 600 }), now), 'Its Switchboard is older and does not report its sessions. Update it there. Read 10 min ago.');
  assert.equal(logic.machineReadLine(machineRead({ failures: 3, reason: 'unreachable', as_of: now - 7200 }), now), 'Not answering: three reads in a row failed. Last answer 2 h ago.');
  assert.equal(logic.machineReadLine(machineRead({ failures: 3, as_of: null }), now), 'Not answering: three reads in a row failed.');
  assert.equal(logic.machineReadLine(null, now), '');
  assert.equal(logic.healthCheckText('something_new'), 'something_new', 'an unknown check by its id');
  // Needs attention keeps the machine items and words them.
  const items = logic.attentionView({ items: ['machine_unreachable', 'machine_health', 'machine_sessions'].map((reason, i) => ({ account_id: null, label: 'studio', provider: null, pool: null, reason, since: 9000, count: i, action: { tool: 'switchboard_machines', window: reason === 'machine_sessions' ? 'open_sessions' : 'open_machines' } })) }).items;
  assert.deepEqual(items.map((i) => i.reason), ['machine_unreachable', 'machine_health', 'machine_sessions']);
  assert.equal(items[0].label, 'studio');
  for (const item of items) { const line = logic.attentionReasonText(item.reason); assert.ok(line && !line.includes('\u2014') && !line.includes('studio'), item.reason); }
  for (const v of [shared, silent, before, nothing, signIn, unmeasured, noCopy, offThere, neverAnswered]) assert.ok(!v.line.includes('\u2014'));
  assert.equal(logic.attentionReasonText('machine_sessions'), 'Sessions on this machine need a look: a second copy, or a board run that has gone quiet.');
  i18nForShell.setLocale('ru');
  assert.equal(logic.remoteUsageView(read({}), own, 'studio', now).line, 'На машине studio, прочитано 2 мин назад');
  assert.equal(logic.attentionReasonText('machine_unreachable'), 'Switchboard на этой машине не ответил на три чтения подряд. Проверьте, что машина включена и доступна по ssh.');
  i18nForShell.setLocale('en');
});
console.log(`${cases} ui-logic cases passed, including quota priority and wall-clock countdowns.`);
