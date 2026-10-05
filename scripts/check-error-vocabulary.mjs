// Every error string the Rust backend can hand to the desktop UI must be known to
// src/adapter.ts: shown verbatim (safeErrors) or mapped to a fixed sentence
// (coreErrors). An unknown string falls back to a generic message, so a new
// backend sentence that is not registered here silently loses its recovery text.
//
// Usage: node scripts/check-error-vocabulary.mjs [--list] [--root DIR]
//   --list  print every extracted message with its source and how it is handled
//   --root  check another tree with the same layout (used for the planted-defect proof)
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, relative, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const argv = process.argv.slice(2);
const rootArg = argv.indexOf('--root');
const ROOT = rootArg >= 0 ? argv[rootArg + 1] : fileURLToPath(new URL('..', import.meta.url));
const LIST = argv.includes('--list');

// Sources whose errors can reach the renderer: the Tauri commands call the runtime
// in-process, the runtime calls core, and the proxy's REJECTED is surfaced by probes.
const SOURCE_DIRS = ['crates/switchboard-core/src', 'crates/switchboard-runtime/src', 'crates/switchboard-proxy/src', 'src-tauri/src'];
// Whole files that never answer a renderer request. Each is excluded with its reason.
const EXCLUDED_FILES = {
  // Unit tests of the proxy (`mod tests;`), never compiled into the app.
  'crates/switchboard-proxy/src/tests.rs': 'test module',
  // Loopback control socket. Its client errors (control::request) reach only the CLI and
  // MCP server; the desktop app answers requests and never sends one. Its listener errors
  // fail Owner::start, which the desktop Slot (src-tauri main.rs) replaces with
  // STARTUP_FAILED before any reply.
  'crates/switchboard-runtime/src/control.rs': 'CLI/MCP control client and owner start-up; never a renderer reply',
  // Lifecycle log (LC-12). Log::event discards every write error: logging never fails or
  // answers a caller.
  'crates/switchboard-runtime/src/oplog.rs': 'log writes; errors are dropped inside Log::event',
};
// Messages that are error literals in the sources above but can never reach the UI.
// Every entry names why. Keep this list short: a doubtful message gets a mapping instead.
const ALLOWLIST = {
  // core projects.rs restore_project(): called only by backup::restore, which counts a refusal
  // as a project not put back and never returns the message.
  'Project already here.': 'swallowed by backup restore',
  // runtime analytics.rs installation()/set_shared_enabled(): internal. status() reads them as
  // "off"; set_enabled() replaces every one with "Could not save the analytics choice.".
  'Installation folder unavailable.': 'replaced by the analytics choice error',
  'Installation unavailable.': 'replaced by the analytics choice error',
  'Installation file unreadable.': 'replaced by the analytics choice error',
  // runtime uninstall.rs remove_login_item(): `switchboard uninstall --yes` lists it under
  // `failures` in CLI output; uninstall is never a desktop operation (SB-28).
  'Could not remove the login item.': 'uninstall CLI output only',
  // runtime lib.rs StopSignals (listen/requested): used by `switchboard serve` (CLI output) and
  // by the desktop app's signal task, which ignores an error; never a renderer reply.
  'Shutdown signal unavailable.': 'serve CLI and the desktop signal task only',
  // runtime lib.rs no_swap(): the synthetic owners' Claude Swap source. Its only caller,
  // refresh::catch_up_with_claude_swap, replaces any error with SWAP_BUSY before it returns.
  'Claude Swap profiles not found.': 'replaced by SWAP_BUSY',
  // runtime lib.rs default_root(): read at owner start-up and by the CLI. In the desktop
  // app the Slot replaces every start-up error with STARTUP_FAILED; --data-dir is a CLI flag.
  'App-data directory unavailable. Use --data-dir with an absolute private directory.': 'start-up (STARTUP_FAILED) and CLI only',
  // runtime lib.rs needs_owner(): only the offline CLI runs without an owner. The desktop
  // app always executes operations through its own Owner, so this never answers the UI.
  "Start the desktop app or 'switchboard serve' before login or launch.": 'offline CLI only',
  // core persistence.rs secure_root(): the desktop app's root comes from default_root(),
  // which is always absolute; only a relative CLI --data-dir reaches this check.
  'Account storage path must be absolute': 'CLI --data-dir only',
  // proxy lib.rs ProxyHandle::start_at(): runs inside Owner::start; the desktop Slot replaces
  // a start-up failure with STARTUP_FAILED (src-tauri main.rs).
  'Local proxy could not start.': 'owner start-up only (STARTUP_FAILED)',
  'Local proxy address unavailable.': 'owner start-up only (STARTUP_FAILED)',
  'HTTP client unavailable.': 'owner start-up only (STARTUP_FAILED)',
  // runtime external.rs import(): per-profile Claude Swap failures inside the row closure.
  // Each is counted in ImportBatch.failed and never returned; the UI reports the count and
  // tells the operator to check those profiles in Claude Swap.
  'Invalid import identity.': 'counted in ImportBatch.failed',
  'Invalid import slot.': 'counted in ImportBatch.failed',
  'Invalid backup credential.': 'counted in ImportBatch.failed',
  'Backup credential missing.': 'counted in ImportBatch.failed',
  'Backup credential too large.': 'counted in ImportBatch.failed',
  'Backup changed during import.': 'counted in ImportBatch.failed',
  'Backup config missing.': 'counted in ImportBatch.failed',
  'Backup identity mismatch.': 'counted in ImportBatch.failed',
};

// ---------------------------------------------------------------------------
// Rust lexing: strings and comments are masked so error patterns can be matched
// across rustfmt line breaks without being fooled by quotes or braces in text.
export function lexRust(source) {
  const literals = []; // { value, line, raw }
  let out = '';
  let i = 0; let line = 1;
  const n = source.length;
  const isIdent = (c) => /[A-Za-z0-9_]/.test(c ?? '');
  const push = (value, startLine, raw) => { out += `"§${literals.length}"`; literals.push({ value, line: startLine, raw }); };
  const newlines = (text) => { for (const c of text) if (c === '\n') { line += 1; out += '\n'; } };
  while (i < n) {
    const c = source[i]; const next = source[i + 1];
    if (c === '/' && next === '/') { const end = source.indexOf('\n', i); i = end < 0 ? n : end; continue; }
    if (c === '/' && next === '*') {
      let depth = 1; let j = i + 2;
      while (j < n && depth) { if (source[j] === '/' && source[j + 1] === '*') { depth += 1; j += 2; } else if (source[j] === '*' && source[j + 1] === '/') { depth -= 1; j += 2; } else j += 1; }
      newlines(source.slice(i, j)); out += ' '; i = j; continue;
    }
    // Raw strings: r"..", r#".."#, br"..", cr".." (prefix must start a token).
    const raw = /^(?:b|c)?r(#*)"/.exec(source.slice(i, i + 260));
    if (raw && !isIdent(source[i - 1])) {
      const close = `"${raw[1]}`; const start = i + raw[0].length; const end = source.indexOf(close, start);
      if (end < 0) throw new Error(`unterminated raw string at line ${line}`);
      const startLine = line; const value = source.slice(start, end);
      push(value, startLine, true); newlines(value); i = end + close.length; continue;
    }
    if (c === '"' || ((c === 'b' || c === 'c') && next === '"' && !isIdent(source[i - 1]))) {
      let j = c === '"' ? i + 1 : i + 2; let value = ''; const startLine = line;
      while (j < n && source[j] !== '"') {
        if (source[j] === '\\') {
          const e = source[j + 1];
          if (e === '\n') { line += 1; out += '\n'; j += 2; while (/\s/.test(source[j])) { if (source[j] === '\n') { line += 1; out += '\n'; } j += 1; } continue; }
          const simple = { n: '\n', t: '\t', r: '\r', '0': '\0', '\\': '\\', '"': '"', "'": "'" };
          if (e in simple) { value += simple[e]; j += 2; continue; }
          if (e === 'x') { value += String.fromCharCode(parseInt(source.slice(j + 2, j + 4), 16)); j += 4; continue; }
          if (e === 'u') { const end = source.indexOf('}', j); value += String.fromCodePoint(parseInt(source.slice(j + 3, end), 16)); j = end + 1; continue; }
          throw new Error(`unknown escape \\${e} at line ${line}`);
        }
        if (source[j] === '\n') { line += 1; out += '\n'; }
        value += source[j]; j += 1;
      }
      push(value, startLine, false); i = j + 1; continue;
    }
    if (c === "'") {
      // Char literal ('x', '\n', '\u{..}') versus lifetime ('a).
      if (next === '\\') { const end = source.indexOf("'", i + 3); out += "' '"; i = end + 1; continue; }
      const cp = source.codePointAt(i + 1); const width = cp > 0xffff ? 2 : 1;
      if (source[i + 1 + width] === "'") { out += "' '"; i += 2 + width; continue; }
      out += c; i += 1; continue;
    }
    if (c === '\n') line += 1;
    out += c; i += 1;
  }
  return { masked: out, literals };
}

/** Index just past the brace block that opens at `open` (masked source, so braces are code). */
function closeBlock(masked, open) {
  let depth = 0;
  for (let j = open; j < masked.length; j += 1) {
    if (masked[j] === '{') depth += 1;
    else if (masked[j] === '}') { depth -= 1; if (depth === 0) return j + 1; }
  }
  return masked.length;
}
/** Items under #[cfg(test)] or #[cfg(all(.., test, ..))] are removed; cfg(any(test, ..)) and not(test) stay. */
export function stripTests(masked) {
  let text = masked;
  const attribute = /#\[cfg\(\s*(test|all\(([^\]]*)\))\s*\)\]/g;
  for (let match; (match = attribute.exec(text));) {
    const testOnly = match[1] === 'test' || (match[2] !== undefined && match[2].split(',').map((part) => part.trim()).includes('test'));
    if (!testOnly) continue;
    const after = match.index + match[0].length;
    const brace = text.indexOf('{', after); const semi = text.indexOf(';', after);
    const end = semi >= 0 && (brace < 0 || semi < brace) ? semi + 1 : closeBlock(text, brace);
    const removed = text.slice(match.index, end).replace(/[^\n]/g, ' ');
    text = text.slice(0, match.index) + removed + text.slice(end);
    attribute.lastIndex = match.index + 1;
  }
  return text;
}

/** End index of the balanced (...) whose "(" is at `open`. */
function closeParen(masked, open) {
  let depth = 0;
  for (let j = open; j < masked.length; j += 1) {
    if (masked[j] === '(') depth += 1;
    else if (masked[j] === ')') { depth -= 1; if (depth === 0) return j; }
  }
  return masked.length;
}
// Calls whose argument is (or computes) the error value. `Err(..)` covers `return Err`,
// `Err(x.into())`, `Err(String::from(x))` and `Err(if .. { a } else { b })`.
const ERROR_CALLS = /\bErr\s*\(|\.ok_or\s*\(|\.ok_or_else\s*\(|\.map_err\s*\(/g;
const PLACEHOLDER = /"§(\d+)"/g;
const CONST = /\b(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"§(\d+)"/g;

// `fn error(_: impl Display) -> String { "Fixed text".into() }`: a helper that always
// returns one literal is a constant an error call can name (`.map_err(error)`).
const CONST_FN = /\bfn\s+([a-z_][a-z0-9_]*)\s*\([^)]*\)\s*->\s*(?:String|&(?:'static\s+)?str)\s*\{\s*"§(\d+)"\s*(?:\.\s*(?:into|to_string|to_owned)\s*\(\s*\)\s*)?\}/g;
// Openers whose argument is still the error value itself. `surface` is core's
// vault::surface(fallback): it returns its literal for every non-actionable vault error.
const VALUE_CALLS = new Set(['Err', 'from', 'Some', 'ok_or', 'ok_or_else', 'map_err', 'into', 'surface']);

/**
 * Classifies the literal at `index` of an error-call span by the innermost bracket it sits in:
 * the call's own parenthesis, a conversion or a closure/if/match body is the error value; an
 * argument of any other call or an index expression (`get("error")`, `value["x"]`) is not.
 */
function position(span, index) {
  const stack = [];
  for (let j = 0; j < index; j += 1) {
    const c = span[j];
    if (c === '(' || c === '[' || c === '{') stack.push(j);
    else if ((c === ')' || c === ']' || c === '}') && stack.length) stack.pop();
  }
  for (let k = stack.length - 1; k >= 0; k -= 1) {
    const at = stack[k]; const opener = span[at];
    if (opener === '{') continue; // a block inside the value: look at what encloses it
    if (opener === '[') return 'other';
    if (at === 0) return 'value';
    const name = /([A-Za-z_][A-Za-z0-9_]*)(!?)\s*$/.exec(span.slice(0, at));
    if (!name) return 'value'; // grouping parenthesis
    // A format! string is the error only where the format! call itself is the value.
    if (name[2]) return name[1] === 'format' && position(span, name.index) === 'value' ? 'format' : 'other';
    return VALUE_CALLS.has(name[1]) ? 'value' : 'other';
  }
  return 'value';
}

export function scanFile(path, text) {
  const { masked, literals } = lexRust(text);
  const code = stripTests(masked);
  const messages = []; const formats = []; const constRefs = [];
  const consts = new Map(); const constFns = new Map();
  for (const match of code.matchAll(CONST)) consts.set(match[1], literals[Number(match[2])]);
  for (const match of code.matchAll(CONST_FN)) constFns.set(match[1], literals[Number(match[2])]);
  for (const match of code.matchAll(ERROR_CALLS)) {
    const open = match.index + match[0].length - 1; const span = code.slice(open, closeParen(code, open) + 1);
    for (const literal of span.matchAll(PLACEHOLDER)) {
      const entry = literals[Number(literal[1])]; const kind = position(span, literal.index);
      if (kind === 'value') messages.push({ path, ...entry });
      else if (kind === 'format') formats.push({ path, ...entry });
    }
    const bare = span.replace(PLACEHOLDER, '""');
    for (const ident of bare.matchAll(/(?:\b([a-z_][a-z0-9_]*)::)?\b([A-Z][A-Z0-9_]{2,})\b/g)) constRefs.push({ path, module: ident[1], name: ident[2] });
    for (const ident of bare.matchAll(/(?<![\w:.])([a-z_][a-z0-9_]*)\b/g)) if (constFns.has(ident[1])) messages.push({ path, ...constFns.get(ident[1]), constant: `${ident[1]}()` });
  }
  return { messages, formats, consts, constRefs };
}

function rustFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => entry.isDirectory() ? rustFiles(join(dir, entry.name)) : entry.name.endsWith('.rs') ? [join(dir, entry.name)] : []);
}

export function extractBackend(root) {
  const files = SOURCE_DIRS.flatMap((dir) => rustFiles(join(root, dir))).filter((file) => !(relative(root, file) in EXCLUDED_FILES));
  const scans = files.map((file) => ({ file: relative(root, file), ...scanFile(relative(root, file), readFileSync(file, 'utf8')) }));
  const messages = scans.flatMap((scan) => scan.messages);
  const formats = scans.flatMap((scan) => scan.formats);
  // A &str constant counts as an error when an error call names it: same file first,
  // then the module it is qualified with, then every constant of that name.
  for (const scan of scans) for (const ref of scan.constRefs) {
    const named = scans.filter((other) => other.consts.has(ref.name));
    const qualified = named.filter((other) => basename(other.file, '.rs') === ref.module);
    const owners = !ref.module && scan.consts.has(ref.name) ? [scan] : qualified.length ? qualified : named;
    for (const owner of owners) messages.push({ path: owner.file, ...owner.consts.get(ref.name), constant: ref.name });
  }
  const unique = new Map();
  for (const message of messages) if (!unique.has(message.value)) unique.set(message.value, message);
  return { messages: [...unique.values()], formats, files: files.length };
}

// ---------------------------------------------------------------------------
// adapter.ts vocabulary, read from its syntax tree rather than by text search.
export function extractAdapter(root) {
  const path = join(root, 'src/adapter.ts');
  const file = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.ES2022, true);
  const constants = new Map(); // identifier -> string (imports from read-deadline resolved below)
  const deadline = join(root, 'src/read-deadline.ts');
  if (existsSync(deadline)) {
    const other = ts.createSourceFile(deadline, readFileSync(deadline, 'utf8'), ts.ScriptTarget.ES2022, true);
    other.forEachChild((node) => { if (ts.isVariableStatement(node)) for (const d of node.declarationList.declarations) if (ts.isIdentifier(d.name) && d.initializer && ts.isStringLiteralLike(d.initializer)) constants.set(d.name.text, d.initializer.text); });
  }
  const text = (node) => {
    if (ts.isStringLiteralLike(node)) return node.text;
    if (ts.isIdentifier(node) && constants.has(node.text)) return constants.get(node.text);
    throw new Error(`adapter.ts:${file.getLineAndCharacterOfPosition(node.getStart()).line + 1}: vocabulary entries must be string literals`);
  };
  const safe = new Set(); const core = new Map();
  const visit = (node) => {
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) {
      if (node.name.text === 'safeErrors' && ts.isNewExpression(node.initializer)) for (const element of node.initializer.arguments[0].elements) safe.add(text(element));
      if (node.name.text === 'coreErrors' && ts.isObjectLiteralExpression(node.initializer)) for (const property of node.initializer.properties) core.set(text(property.name), text(property.initializer));
    }
    // for (const error of ['a', 'b']) coreErrors[error] = 'mapped';
    if (ts.isForOfStatement(node) && ts.isArrayLiteralExpression(node.expression)) {
      const body = ts.isBlock(node.statement) ? node.statement.statements[0] : node.statement;
      const assignment = body && ts.isExpressionStatement(body) && ts.isBinaryExpression(body.expression) ? body.expression : null;
      if (assignment && ts.isElementAccessExpression(assignment.left) && assignment.left.expression.getText() === 'coreErrors') for (const element of node.expression.elements) core.set(text(element), text(assignment.right));
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return { safe, core };
}

// ---------------------------------------------------------------------------
export function check(root) {
  const backend = extractBackend(root);
  const { safe, core } = extractAdapter(root);
  const handled = (value) => safe.has(value) ? 'verbatim' : core.has(value) ? 'mapped' : Object.hasOwn(ALLOWLIST, value) ? 'allowlisted' : null;
  const missing = backend.messages.filter((message) => !handled(message.value));
  // A mapping must itself be a fixed sentence the UI shows, never another backend key.
  const chained = [...core].filter(([, mapped]) => core.has(mapped) && !safe.has(mapped)).map(([key]) => key);
  const stale = Object.keys(ALLOWLIST).filter((value) => !backend.messages.some((message) => message.value === value));
  return { backend, safe, core, handled, missing, chained, stale };
}

// The extractor's own receipts: each pattern it claims to read, and each it must ignore.
export function selfTest() {
  const fixture = [
    'const UNAVAILABLE: &str =\n    "Const error.";',
    'const TOKEN_URL: &str = "https://example.test/token";',
    'fn err(_: impl std::fmt::Display) -> String {\n    "Helper error".into()\n}',
    'fn a() -> Result<(), String> { Err("Direct error.".into()) }',
    'fn b() -> Result<(), String> {\n    return Err(\n        "Wrapped across \\\n         lines."\n            .into(),\n    );\n}',
    'fn c(x: Option<u8>) -> Result<u8, String> { x.ok_or("Ok-or error.")?; x.ok_or_else(|| "Lazy error.".to_string()) }',
    'fn d() -> Result<(), String> { f().map_err(|_| "Mapped error.")?; f().map_err(err)?; g().ok_or(UNAVAILABLE)?; h().map_err(vault::surface("Surface fallback"))?; Ok(()) }',
    'fn e() -> Result<(), String> { Err(if x { "Branch one." } else { "Branch two." }.into()) }',
    'fn f() -> Result<(), String> { Err(error("ignored helper argument")) }',
    'fn g(v: Value) -> Result<(), String> { Err(if v.get("ignored key") == v["ignored index"] { String::from("From error.") } else { format!("Formatted {}", 1) }) }',
    "fn h() -> char { let _quote: char = '\"'; let _brace = '{'; 'x' }",
    '// Err("Commented error.")\n/* Err("Block comment error.") */',
    'fn r() -> Result<(), String> { Err(r#"Raw "quoted" error."#.into()) }',
    '#[cfg(test)]\nmod tests { fn t() -> Result<(), String> { Err("Test-only error.".into()) } }',
    '#[cfg(all(unix, test))]\nfn u() -> Result<(), String> { Err("Unix test error.".into()) }',
    '#[cfg(any(target_os = "macos", test))]\nfn m() -> Result<(), String> { Err("Shared cfg error.".into()) }',
    '#[cfg(not(test))]\nfn n() -> Result<(), String> { Err("Release error.".into()) }',
  ].join('\n');
  const scan = scanFile('fixture.rs', fixture);
  const found = new Set(scan.messages.map((message) => message.value));
  const expected = ['Helper error', 'Direct error.', 'Wrapped across lines.', 'Ok-or error.', 'Lazy error.', 'Mapped error.', 'Surface fallback', 'Branch one.', 'Branch two.', 'From error.', 'Raw "quoted" error.', 'Shared cfg error.', 'Release error.'];
  const ignored = ['ignored helper argument', 'ignored key', 'ignored index', 'Commented error.', 'Block comment error.', 'Test-only error.', 'Unix test error.', 'https://example.test/token'];
  for (const value of expected) if (!found.has(value)) throw new Error(`self-test: extractor missed ${JSON.stringify(value)}`);
  for (const value of ignored) if (found.has(value)) throw new Error(`self-test: extractor took ${JSON.stringify(value)}`);
  if (!scan.constRefs.some((ref) => ref.name === 'UNAVAILABLE') || scan.consts.get('UNAVAILABLE')?.value !== 'Const error.') throw new Error('self-test: constant error not resolved');
  if (scan.formats.map((entry) => entry.value).join() !== 'Formatted {}') throw new Error('self-test: format! error not reported');
  return expected.length + ignored.length + 2;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const receipts = selfTest();
  const result = check(ROOT);
  if (LIST) for (const message of result.backend.messages) console.log(`${result.handled(message.value) ?? 'MISSING'}\t${message.path}:${message.line}\t${message.value}`);
  const problems = [];
  for (const message of result.missing) problems.push(`unmapped backend message (${message.path}:${message.line}${message.constant ? `, ${message.constant}` : ''}): ${JSON.stringify(message.value)}`);
  for (const key of result.chained) problems.push(`coreErrors maps ${JSON.stringify(key)} to another backend key`);
  for (const value of result.stale) problems.push(`allowlist entry no longer in the backend: ${JSON.stringify(value)}`);
  if (problems.length) {
    console.error(`${problems.length} error-vocabulary problem(s). Add each message to src/adapter.ts (safeErrors verbatim, or coreErrors with an actionable sentence), or allowlist it here with the reason it never reaches the UI:`);
    for (const problem of problems) console.error(`  - ${problem}`);
    process.exit(1);
  }
  const counts = { verbatim: 0, mapped: 0, allowlisted: 0 };
  for (const message of result.backend.messages) counts[result.handled(message.value)] += 1;
  console.log(`${result.backend.messages.length} backend error messages in ${result.backend.files} Rust files: ${counts.verbatim} verbatim, ${counts.mapped} mapped, ${counts.allowlisted} allowlisted; ${result.backend.formats.length} formatted messages fall back to the generic text. Extractor self-test: ${receipts} receipts.`);
}
