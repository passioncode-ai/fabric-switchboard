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
// A fallback (`var(--x, …)`) hides an undefined role from the eye, not from this check (VIS-30).
for (const match of `${tokens}\n${adapter}\n${styles}`.matchAll(/var\((--[\w-]+)\s*[,)]/g)) assert(declared.has(match[1]), `Undefined role: ${match[1]}`);
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
// Every role the adapter declares, resolved per theme: an alias of another role, or an sRGB
// color-mix of two roles (the chart series, the hover tint), the way the browser mixes them.
const roles = Object.fromEntries([...adapter.matchAll(/^\s*(--[\w-]+):\s*([^;]+);/gm)].map((match) => [match[1], match[2].trim()]));
const hex = (n) => n.toString(16).padStart(2, '0');
const mix = (a, percent, b) => '#' + [1, 3, 5].map((i) => hex(Math.round(parseInt(a.slice(i, i + 2), 16) * percent / 100 + parseInt(b.slice(i, i + 2), 16) * (100 - percent) / 100))).join('');
const color = (theme, role, depth = 0) => {
  assert(depth < 8, `${role}: alias loop`);
  const value = roles[role] ?? themes[theme][role];
  assert(value, `${theme} ${role}: no value`);
  const alias = value.match(/^var\((--[\w-]+)\)$/);
  if (alias) return color(theme, alias[1], depth + 1);
  const mixed = value.match(/^color-mix\(in srgb, var\((--[\w-]+)\) (\d+)%, var\((--[\w-]+)\)\)$/);
  if (mixed) return mix(color(theme, mixed[1], depth + 1), Number(mixed[2]), color(theme, mixed[3], depth + 1));
  assert(/^#[\da-f]{6}$/i.test(value), `${theme} ${role}: expected a six-hex colour, an alias or an sRGB color-mix of two roles, got ${value}`);
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
  // VIS-10, VIS-17: the chosen language. Ink on the chosen fill; its gold line against the segmented
  // track and the panel; the unselected segment's muted text on the track.
  ['text', '--chosen-ink', '--chosen-bg'], ['ui', '--chosen-border', '--panel-2'], ['ui', '--chosen-border', '--panel'], ['ui', '--chosen-border', '--chosen-bg'],
  // VIS-18: text stays readable on the hover tint.
  ['text', '--ink', '--hover-bg'], ['text', '--muted', '--hover-bg'],
  ['ui', '--focus', '--bg'], ['ui', '--focus', '--panel'], ['ui-light', '--border-strong', '--panel'], ['ui-light', '--border-strong', '--bg'], ['ui', '--accent-text', '--border'], ['ui', '--ok', '--panel'], ['ui', '--accent-text', '--panel-2'],
];
// SB-169: the selected account row and card are tinted with color-mix over the panel; their
// text is measured on the mixed colour. The CSS must still mix exactly what is measured here.
const mixes = {
  'row-selected': ['--accent-weak', 40, '--panel', /\.account-row\.is-selected\s*\{\s*background:\s*color-mix\(in srgb, var\(--accent-weak\) 40%, transparent\)/],
  'card-selected': ['--accent-weak', 45, '--panel', /\.account-card\.is-selected\s*\{[^}]*background:\s*color-mix\(in srgb, var\(--accent-weak\) 45%, var\(--panel\)\)/],
};
for (const [name, [, , , pattern]] of Object.entries(mixes)) assert(pattern.test(styles), `${name}: style.css no longer mixes what check-brand measures`);
const mixed = (theme, name) => { const [top, percent, under] = mixes[name]; return mix(color(theme, top), percent, color(theme, under)); };
for (const name of Object.keys(mixes)) for (const fg of ['--ink', '--muted', '--accent-text']) pairs.push(['text', fg, name]);
// VIS-02, VIS-06: every chart series is a graphical object on the panel (WCAG 1.4.11, 3:1).
const charts = [1, 2, 3, 4, 5, 6].map((n) => `--chart-${n}`);
for (const series of charts) pairs.push(['ui', series, '--panel']);
// VIS-18: a hover must be seen: the tint differs from the panel it sits on (not a WCAG floor).
pairs.push(['hover', '--hover-bg', '--panel']);
// The chosen language and the hover tint live in the adapter and are used by the component layer.
assert(/\.segment\.is-on\s*\{[^}]*background:\s*var\(--chosen-bg\)[^}]*inset 0 0 0 1px var\(--chosen-border\)/.test(styles), 'VIS-10: the selected segment must use the chosen fill and line');
for (const selector of ['\\.choice-chip\\.is-on', '\\.model-card\\.is-on', '\\.appearance-option:has\\(input:checked\\)']) assert(new RegExp(`${selector}\\s*\\{[^}]*var\\(--chosen-bg\\)[^}]*var\\(--chosen-border\\)`).test(styles), `VIS-17: ${selector} must use the chosen language`);
assert(/\.nav-item:not\(\.active\):hover:not\(:disabled\)\s*\{[^}]*var\(--hover-bg\)/.test(styles) && /\.button:hover:not\(:disabled\)\s*\{[^}]*var\(--hover-bg\)/.test(styles), 'VIS-18: navigation and buttons must hover with the hover tint');
const report = [];
for (const theme of Object.keys(themes)) for (const [kind, fg, bg] of pairs) {
  const value = ratio(color(theme, fg), mixes[bg] ? mixed(theme, bg) : color(theme, bg)); const floor = kind === 'text' ? 4.5 : kind === 'hover' ? 1.1 : 3;
  // Dark --pc-border-strong is 2.51:1 on panel in canonical v1.0.0/v1.1.0 and the canonical
  // checker asserts it for light only; it is reported here and tracked upstream, not asserted.
  const asserted = kind !== 'ui-light' || theme === 'light';
  report.push(`${theme}\t${kind}\t${fg} on ${bg}\t${value.toFixed(2)}${asserted ? '' : '\t(reported, not asserted)'}`);
  if (asserted) assert(value >= floor, `${theme}: ${fg} on ${bg} is ${value.toFixed(2)}:1, below ${floor}:1`);
}
// VIS-02: no two chart series alike in either theme: OKLab distance (Ottosson 2020) of at least
// 0.09 between every pair; the dark gold and blue-grey series of 0.6.81 were 0 (the same colour).
const linear = (c) => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
const oklab = (value) => {
  const [r, g, b] = [1, 3, 5].map((i) => linear(parseInt(value.slice(i, i + 2), 16) / 255));
  const [l, m, s] = [0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b, 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b, 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b].map(Math.cbrt);
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
};
let chartPairs = 0;
for (const theme of Object.keys(themes)) for (let i = 0; i < charts.length; i += 1) for (let j = i + 1; j < charts.length; j += 1) {
  const [a, b] = [oklab(color(theme, charts[i])), oklab(color(theme, charts[j]))];
  const distance = Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]); chartPairs += 1;
  report.push(`${theme}\tseries\t${charts[i]} vs ${charts[j]}\tOKLab ${distance.toFixed(3)}`);
  assert(distance >= 0.09, `${theme}: ${charts[i]} and ${charts[j]} are too alike (OKLab ${distance.toFixed(3)}, below 0.09)`);
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
console.log(`PassionCode ${manifest.version}: 2 canonical sources, 3 native assets, CSS roles, ${report.length - chartPairs} contrast pairs and ${chartPairs} chart series pairs in 2 themes and version ${version} verified`);
