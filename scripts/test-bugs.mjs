// SB-379 (W3): the window's problem reports carry codes from closed lists and are rate limited.
import assert from 'node:assert/strict';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const compiled = ts.transpileModule(readFileSync(new URL('../src/bugs.ts', import.meta.url), 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { errorCode, limiter, watchProblems } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
let cases = 0; const check = (fn) => { fn(); cases += 1; };
check(() => {
  assert.equal(errorCode(new TypeError('person@example.invalid at /Users/planted/app.js')), 'type_error');
  assert.equal(errorCode(new RangeError('x')), 'range_error');
  class Custom extends Error {}
  assert.equal(errorCode(new Custom('x')), 'other', 'a type outside the list is other');
  assert.equal(errorCode('a string with /Users/planted'), 'other', 'a thrown string is never read');
});
check(() => {
  let now = 0; const allow = limiter(() => now);
  assert.equal(allow('js_error:type_error'), true);
  assert.equal(allow('js_error:type_error'), false, 'the same code once a minute');
  for (let i = 0; i < 9; i += 1) assert.equal(allow(`k${i}`), true);
  assert.equal(allow('k-eleventh'), false, 'ten a minute in all');
  now = 60_000; assert.equal(allow('js_error:type_error'), true, 'a minute later again');
});
check(() => {
  const listeners = {}; const sent = [];
  const target = { addEventListener: (k, f) => { listeners[k] = f; }, removeEventListener: (k) => { delete listeners[k]; }, setInterval: () => 1, clearInterval: () => {}, document: { visibilityState: 'visible' } };
  const stop = watchProblems((kind, code) => sent.push([kind, code]), target);
  listeners.error({ error: new TypeError('secret path /Users/planted'), message: 'secret', filename: '/Users/planted/app.js' });
  listeners.unhandledrejection({ reason: new Error('person@example.invalid') });
  assert.deepEqual(sent, [['js_error', 'type_error'], ['js_rejection', 'error']]);
  assert.ok(!JSON.stringify(sent).includes('planted') && !JSON.stringify(sent).includes('@'), 'no text leaves');
  stop(); assert.equal(Object.keys(listeners).length, 0);
});
console.log(`${cases} bug-report cases passed; only codes leave, rate limited.`);
