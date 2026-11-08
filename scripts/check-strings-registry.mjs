// docs/brand/strings.md quotes the window's strings by surface and lists the strings the window
// retired (SB-193). This keeps it honest both ways:
// - every string quoted under "Registry by surface" is still shipped: a window key
//   (`check-locale.mjs --list`), or a native or CLI sentence found in the Rust sources;
// - no string quoted under "Retired strings" is a window key again.
// A quote that ends in " …" names the start of a longer string, one that begins with "… " its end.
// Quotes inside parentheses in the retired list name the current form ("now “…”") and are skipped;
// a Copylot voice name (`voice “…”`) is not a string of the product.
//
// Usage: node scripts/check-strings-registry.mjs
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Quoted “…” strings of `text`, nested quotes kept, whitespace folded; `depth` is the parenthesis depth at the opening quote. */
export function quotes(text) {
  const found = [];
  let depth = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '(') depth++;
    else if (c === ')') depth = Math.max(0, depth - 1);
    else if (c === '“') {
      let level = 1;
      let j = i + 1;
      for (; j < text.length && level > 0; j++) {
        if (text[j] === '“') level++;
        else if (text[j] === '”') level--;
      }
      if (level > 0) break;
      const before = text.slice(Math.max(0, i - 6), i);
      found.push({ text: text.slice(i + 1, j - 1).replace(/\s+/g, ' '), depth, voice: before === 'voice ' });
      i = j - 1;
    }
  }
  return found;
}

/** English cells of the registry's two-column tables, split where one cell names several strings. */
export function cells(text) {
  const out = [];
  for (const m of text.matchAll(/^\| ([^|]+?) \| [^|]+ \|$/gm)) {
    const cell = m[1].trim();
    if (cell === 'English' || /^-+$/.test(cell)) continue;
    for (const part of cell.split(/ \/ | · /)) out.push(part.trim());
  }
  return out;
}

/** Placeholders read the same in a window key, a Rust `format!` and the doc: `{n}`, `{}`, `${usd:.2}`. */
const blank = (text) => text.replace(/\$?\{[^{}]*\}/g, '{}');

/** Whether `quoted` names a shipped string: one of `keys`, or text found in `native` (placeholders blanked). */
export function shipped(quoted, keys, native) {
  if (keys.has(quoted) || native.includes(blank(quoted))) return true;
  if (quoted.endsWith(' …')) {
    const start = quoted.slice(0, -2);
    for (const key of keys) if (key.startsWith(start)) return true;
    return native.includes(blank(start));
  }
  if (quoted.startsWith('… ')) {
    const end = quoted.slice(2);
    for (const key of keys) if (key.endsWith(end)) return true;
    return native.includes(blank(end));
  }
  return false;
}

/** Problems of the registry `md` against the window `keys` and the Rust sources' text `native`. */
export function audit(md, keys, native) {
  const section = (from, to) => {
    const start = md.indexOf(from);
    if (start < 0) throw new Error(`docs/brand/strings.md has no "${from}" section`);
    const end = md.indexOf(to, start);
    return md.slice(start, end < 0 ? md.length : end);
  };
  const registry = section('## Registry by surface', '## Retired strings');
  const retired = section('## Retired strings', '\n## ');
  const problems = [];
  for (const q of quotes(registry)) {
    if (q.voice || shipped(q.text, keys, native)) continue;
    problems.push(`registered but not shipped: “${q.text}”`);
  }
  for (const cell of cells(registry)) if (!shipped(cell, keys, native)) problems.push(`registered but not shipped: “${cell}” (table)`);
  for (const q of quotes(retired.slice('## Retired strings'.length))) {
    if (q.depth === 0 && !q.voice && keys.has(q.text)) problems.push(`retired but still a window key: “${q.text}”`);
  }
  return [...new Set(problems)];
}

function rustText(dir) {
  let text = '';
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) text += rustText(path);
    else if (name.endsWith('.rs')) text += readFileSync(path, 'utf8');
  }
  return text;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const listed = execFileSync(process.execPath, [join(root, 'scripts/check-locale.mjs'), '--list'], { encoding: 'utf8' });
  const keys = new Set(listed.split('\n').filter(Boolean));
  const native = blank(rustText(join(root, 'src-tauri/src')) + rustText(join(root, 'crates')));
  const problems = audit(readFileSync(join(root, 'docs/brand/strings.md'), 'utf8'), keys, native);
  if (problems.length) {
    console.error(`docs/brand/strings.md is out of step with the window (${problems.length}):`);
    for (const p of problems) console.error(`  ${p}`);
    console.error('Resync it with `node scripts/check-locale.mjs --list` in the change that adds or retires a string.');
    process.exit(1);
  }
  console.log('docs/brand/strings.md: every registered string ships, no retired string is back.');
}
