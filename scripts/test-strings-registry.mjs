import assert from 'node:assert/strict';
import { audit, cells, quotes, shipped } from './check-strings-registry.mjs';

const keys = new Set(['Add folders…', 'Group “{name}” created.', ' · estimated', 'Tokens', 'Launch…', 'Isolated session', 'Signing in to {provider}']);
const native = '{} folders added ({}).';
const doc = (registry, retired) => `# Interface strings\n\n## Registry by surface\n\n${registry}\n\n## Retired strings\n\n${retired}\n\n## Rules the strings keep\n\n- “Not checked here”\n`;

assert.deepEqual(quotes('a “Group “{name}” created.” and “ · estimated”').map((q) => q.text), ['Group “{name}” created.', ' · estimated']);
assert.equal(quotes('(now “Launch…”)')[0].depth, 1);
assert.equal(quotes('voice “PassionCode.ai site voice v2”')[0].voice, true);
assert.equal(quotes('“Spending ceilings\n(spend.json)”')[0].text, 'Spending ceilings (spend.json)');
assert.deepEqual(cells('| English | Russian |\n|---|---|\n| Launch… · Isolated session | Запуск… |\n| About / About Switchboard | О программе |'), ['Launch…', 'Isolated session', 'About', 'About Switchboard']);

assert.ok(shipped('{n} folders added ({groups}).', keys, native), 'a native format string with other placeholder names');
assert.ok(shipped('Add …', keys, native), 'a quote that names the start of a string');
assert.ok(shipped('… created.', new Set(['Group created.']), ''), 'a quote that names the end of a string');
assert.ok(!shipped('Add folder…', keys, native), 'a string the window renamed');

assert.deepEqual(audit(doc('“Add folders…”, “Group “{name}” created.”, voice “Some voice”', '- Retired: “Old words” (now “Launch…”).'), keys, native), []);
assert.deepEqual(audit(doc('“Add folder…”', ''), keys, native), ['registered but not shipped: “Add folder…”']);
assert.deepEqual(audit(doc('| English | Russian |\n|---|---|\n| Gone | Нет |', ''), keys, native), ['registered but not shipped: “Gone” (table)']);
assert.deepEqual(audit(doc('', '- The page “Tokens”.'), keys, native), ['retired but still a window key: “Tokens”']);
assert.throws(() => audit('# no sections', keys, native), /Registry by surface/);
console.log('14 strings-registry cases passed; a renamed, a dropped and a revived string are caught.');
