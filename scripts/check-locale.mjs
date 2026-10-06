// Every English string the interface shows has a Russian entry (fabric-workspace
// knowledge/localization.md, L10N-02/03; Switchboard RM-21).
//
// Collected with the TypeScript parser, not regular expressions:
//   - the first argument of every t('…') call in src/ (English source = key);
//   - the `other` form of every plural(n, { one, other }) call (its Russian entry holds three
//     forms separated by `|`);
//   - every backend sentence src/adapter.ts shows verbatim (safeErrors) or maps (coreErrors
//     values, the generic fallback) — the window translates them where they are shown (L10N-04);
//   - the update texts src-tauri/src/updates.rs hands to the window (its `pub const … &str`).
// Fails when a key has no Russian entry, when an entry's {placeholders} differ from its key's,
// or when a plural entry does not have three forms. `--list` prints every key.
//
// Usage: node scripts/check-locale.mjs [--list] [--missing]
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const LIST = process.argv.includes('--list');
const MISSING = process.argv.includes('--missing');

const keys = new Map(); // key -> { plural: bool, where: string }
const problemsAtScan = [];
const add = (key, where, plural = false) => {
  if (!keys.has(key)) keys.set(key, { plural, where });
  else if (plural) keys.get(key).plural = true;
};
const literal = (node) => (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) ? node.text : null);

const sources = readdirSync(join(ROOT, 'src')).filter((f) => f.endsWith('.ts') && !f.endsWith('.d.ts') && f !== 'demo.ts');
// String constants exported anywhere in src/ (READ_TIMEOUT…), so a safeErrors entry that names one
// is collected as the sentence it is.
const constants = new Map();
for (const file of sources) {
  const sf = ts.createSourceFile(file, readFileSync(join(ROOT, 'src', file), 'utf8'), ts.ScriptTarget.ES2022, true);
  sf.forEachChild((node) => {
    if (!ts.isVariableStatement(node)) return;
    for (const decl of node.declarationList.declarations) {
      const text = decl.initializer && literal(decl.initializer);
      if (text !== null && text !== undefined && ts.isIdentifier(decl.name)) constants.set(decl.name.text, text);
    }
  });
}
for (const file of sources) {
  const path = join(ROOT, 'src', file);
  const sf = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.ES2022, true);
  const visit = (node) => {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      const name = node.expression.text;
      const where = `src/${file}:${sf.getLineAndCharacterOfPosition(node.getStart()).line + 1}`;
      if (name === 't' && node.arguments[0]) {
        const text = literal(node.arguments[0]);
        if (text !== null) add(text, where);
      }
      if (name === 'plural' && node.arguments[1] && ts.isObjectLiteralExpression(node.arguments[1])) {
        for (const prop of node.arguments[1].properties) {
          if (ts.isPropertyAssignment(prop) && prop.name.getText(sf) === 'other') {
            const text = literal(prop.initializer);
            if (text !== null) add(text, where, true);
          }
        }
      }
    }
    // Backend sentences with a name in them: their templates (src/formatted-errors.ts).
    if (file === 'formatted-errors.ts' && ts.isVariableDeclaration(node) && node.name.getText(sf) === 'FORMATTED_ERRORS' && node.initializer && ts.isArrayLiteralExpression(node.initializer)) {
      for (const el of node.initializer.elements) { const text = literal(el); if (text) add(text, 'src/formatted-errors.ts'); }
    }
    // adapter.ts: the verbatim set and the mapped sentences.
    if (file === 'adapter.ts') {
      if (ts.isVariableDeclaration(node) && node.name.getText(sf) === 'safeErrors' && node.initializer && ts.isNewExpression(node.initializer)) {
        const arr = node.initializer.arguments?.[0];
        if (arr && ts.isArrayLiteralExpression(arr)) for (const el of arr.elements) {
          const text = literal(el) ?? (ts.isIdentifier(el) ? constants.get(el.text) ?? null : null);
          if (text) add(text, 'src/adapter.ts safeErrors');
          else if (ts.isIdentifier(el)) problemsAtScan.push(`safeErrors names ${el.text}, which is not a string constant in src/`);
        }
      }
      if (ts.isVariableDeclaration(node) && node.name.getText(sf) === 'coreErrors' && node.initializer && ts.isObjectLiteralExpression(node.initializer)) {
        for (const prop of node.initializer.properties) if (ts.isPropertyAssignment(prop)) { const text = literal(prop.initializer); if (text) add(text, 'src/adapter.ts coreErrors'); }
      }
      if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken && node.left.getText(sf).startsWith('coreErrors[')) {
        const text = literal(node.right); if (text) add(text, 'src/adapter.ts coreErrors');
      }
      if (ts.isReturnStatement(node) || ts.isConditionalExpression(node)) {
        // The generic fallback sentence: the last string in safeError's return.
        node.forEachChild((child) => { const text = literal(child); if (text && /^The operation could not be completed/.test(text)) add(text, 'src/adapter.ts fallback'); });
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
}
// Update texts from the native side.
const updates = readFileSync(join(ROOT, 'src-tauri/src/updates.rs'), 'utf8');
for (const match of updates.matchAll(/pub const [A-Z_]+: &str =\s*"((?:[^"\\]|\\.)*)";/g)) {
  const text = match[1].replace(/\\"/g, '"');
  if (/\s/.test(text)) add(text, 'src-tauri/src/updates.rs');
}
// Window names the feature label translates by value.
for (const word of ['primary', 'secondary']) add(word, 'src/ui-logic.ts featureWindowLabel');

const { RU } = await import(`data:text/javascript;base64,${Buffer.from(ts.transpileModule(readFileSync(join(ROOT, 'src/locales/ru.ts'), 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText).toString('base64')}`);

const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');
const problems = [...problemsAtScan];
const missing = [];
for (const [key, { plural, where }] of keys) {
  const entry = RU[key];
  if (entry === undefined) { missing.push(key); problems.push(`missing (${where}): ${JSON.stringify(key)}`); continue; }
  const forms = plural ? entry.split('|') : [entry];
  if (plural && forms.length !== 3) problems.push(`plural needs three forms (${where}): ${JSON.stringify(key)}`);
  for (const form of forms) if (placeholders(form) !== placeholders(key)) problems.push(`placeholders differ (${where}): ${JSON.stringify(key)} → ${JSON.stringify(form)}`);
}
const unused = Object.keys(RU).filter((k) => !keys.has(k));
if (LIST) for (const [key] of keys) console.log(key);
if (MISSING) { console.log(JSON.stringify(missing, null, 1)); process.exit(0); }
if (problems.length) {
  console.error(`${problems.length} localization problem(s):`);
  for (const p of problems.slice(0, 40)) console.error(`  - ${p}`);
  if (problems.length > 40) console.error(`  … and ${problems.length - 40} more`);
  process.exit(1);
}
console.log(`${keys.size} interface strings, all in Russian${unused.length ? `; ${unused.length} dictionary entries no longer used` : ''}.`);
