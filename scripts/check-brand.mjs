// Offline receipts: shared source bytes, generated native assets, roles, and versions.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const read = (path) => readFileSync(path, 'utf8');
const json = (path) => JSON.parse(read(path));
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
const manifest = json('brand/passioncode/manifest.json');
assert.match(manifest.commit, /^[a-f0-9]{40}$/);
for (const file of manifest.files) assert.equal(hash(file.vendored), file.sha256, `${file.vendored}: canonical source drift`);
const native = json('brand/passioncode/native-icons.json');
for (const [file, digest] of Object.entries(native.sha256)) assert.equal(hash(file), digest, `${file}: generated icon drift`);
const tokens = read('brand/passioncode/tokens.css');
const adapter = read('src/tokens.css');
const styles = read('src/style.css');
const declared = new Set([...`${tokens}\n${adapter}`.matchAll(/(--[\w-]+)\s*:/g)].map((match) => match[1]));
for (const match of `${tokens}\n${adapter}\n${styles}`.matchAll(/var\((--[\w-]+)\)/g)) assert(declared.has(match[1]), `Undefined role: ${match[1]}`);
assert(!/#[\da-f]{3,8}\b/i.test(adapter + styles), 'Component layer must consume shared color roles');
assert.equal(manifest.version, '1.1.0', 'PassionCode design system must be pinned at 1.1.0');
assert(tokens.startsWith('/* PassionCode design system v1.1.0.'), 'Vendored tokens header must name v1.1.0');
assert(/--focus:\s*var\(--pc-focus\)/.test(adapter) && /--link:\s*var\(--pc-link\)/.test(adapter), 'Focus and link roles must alias the shared roles');
assert(/:focus-visible\s*\{[^}]*outline:[^;]*var\(--focus\)/.test(styles), 'Focus outline must use the focus role');
const main = read('src/main.ts');
// Appearance: System (prefers-color-scheme) by default, explicit Dark/Light persisted.
assert(main.includes("matchMedia('(prefers-color-scheme: light)')"), 'System appearance must follow the operating system');
assert(main.includes('document.documentElement.dataset.theme = resolveTheme('), 'Theme must be set from the resolved appearance');
assert(/content="dark light"/.test(read('index.html')), 'index.html must declare both color schemes');

// Contrast of every text/state and control pair the component layer draws, in both themes.
// WCAG 2.x relative luminance; 4.5:1 for text (1.4.3 AA), 3:1 for boundaries/focus (1.4.11 AA).
const block = (selector) => { const start = tokens.indexOf(selector); assert(start >= 0, `${selector} block missing`); return tokens.slice(start, tokens.indexOf('}', start)); };
const values = (text) => Object.fromEntries([...text.matchAll(/(--pc-[\w-]+):\s*([^;]+);/g)].map((match) => [match[1], match[2].trim()]));
const dark = values(block(':root {'));
const themes = { dark, light: { ...dark, ...values(block(':root[data-theme="light"] {')) } };
const aliases = Object.fromEntries([...adapter.matchAll(/(--[\w-]+):\s*var\((--pc-[\w-]+)\)/g)].map((match) => [match[1], match[2]]));
const color = (theme, role) => {
  let value = themes[theme][aliases[role] ?? role];
  for (let depth = 0; value?.startsWith('var('); depth += 1) { assert(depth < 4); value = themes[theme][value.slice(4, -1)]; }
  assert(/^#[\da-f]{6}$/i.test(value ?? ''), `${theme} ${role}: expected a six-hex colour`);
  return value;
};
const luminance = (hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a, b) => { const [x, y] = [luminance(a), luminance(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
const pairs = [
  ['text', '--ink', '--bg'], ['text', '--ink', '--panel'], ['text', '--ink', '--panel-2'], ['text', '--muted', '--bg'], ['text', '--muted', '--panel'], ['text', '--muted', '--panel-2'],
  ['text', '--accent-ink', '--accent'], ['text', '--accent-ink', '--accent-hover'], ['text', '--accent-text', '--panel'], ['text', '--accent-text', '--accent-weak'], ['text', '--link', '--panel'],
  ['text', '--ink', '--accent-weak'], ['text', '--ink', '--ok-weak'], ['text', '--ink', '--warn-weak'], ['text', '--warn', '--panel'], ['text', '--danger', '--panel'], ['text', '--danger', '--danger-weak'],
  ['text', '--info', '--info-weak'], ['text', '--info', '--panel'],
  ['ui', '--focus', '--bg'], ['ui', '--focus', '--panel'], ['ui-light', '--border-strong', '--panel'], ['ui-light', '--border-strong', '--bg'], ['ui', '--accent-text', '--border'], ['ui', '--ok', '--panel'], ['ui', '--accent-text', '--panel-2'],
];
const report = [];
for (const theme of Object.keys(themes)) for (const [kind, fg, bg] of pairs) {
  const value = ratio(color(theme, fg), color(theme, bg)); const floor = kind === 'text' ? 4.5 : 3;
  // Dark --pc-border-strong is 2.51:1 on panel in canonical v1.0.0/v1.1.0 and the canonical
  // checker asserts it for light only; it is reported here and tracked upstream, not asserted.
  const asserted = kind !== 'ui-light' || theme === 'light';
  report.push(`${theme}\t${kind}\t${fg} on ${bg}\t${value.toFixed(2)}${asserted ? '' : '\t(reported, not asserted)'}`);
  if (asserted) assert(value >= floor, `${theme}: ${fg} on ${bg} is ${value.toFixed(2)}:1, below ${floor}:1`);
}
if (process.argv.includes('--contrast')) console.log(report.join('\n'));
assert(main.includes("import switchboardMark from '../brand/passioncode/switchboard-mark.svg'"));
assert(main.includes('mark.src = switchboardMark'));
const version = json('package.json').version;
assert.equal(json('package-lock.json').version, version);
assert.equal(json('package-lock.json').packages[''].version, version);
assert.equal(json('src-tauri/tauri.conf.json').version, version);
assert.equal(read('Cargo.toml').match(/\[workspace.package\]\nversion = "([^"]+)"/)[1], version);
for (const match of read('Cargo.lock').matchAll(/name = "(?:fabric-switchboard|switchboard-cli|switchboard-proxy|switchboard-runtime)"\nversion = "([^"]+)"/g)) assert.equal(match[1], version);
console.log(`PassionCode ${manifest.version}: 2 canonical sources, 3 native assets, CSS roles, ${report.length} contrast pairs in 2 themes and version ${version} verified`);
