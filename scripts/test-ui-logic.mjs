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

console.log(`${cases} ui-logic cases passed: appearance, monitored accounts, reset-aware staleness, mutation ordering, error vocabulary, project rules.`);
