// SB-379 (W3): the window's own problems, counted in Switchboard's anonymous error report as codes
// only. An uncaught error is its type (TypeError, RangeError…), a rejected promise only that it
// happened, a stall that the page froze for over two seconds while visible. Never a message, a
// stack, a URL or a path: those can carry a name, an address or a folder.
export type ProblemKind = 'js_error' | 'js_rejection' | 'ipc_unknown' | 'ipc_timeout' | 'ui_stall';

const TYPES: Record<string, string> = { TypeError: 'type_error', RangeError: 'range_error', ReferenceError: 'reference_error', SyntaxError: 'syntax_error', Error: 'error' };
/** The code of an error's type, from a closed list; its message is never read. */
export function errorCode(error: unknown): string {
  if (!(error instanceof Error)) return 'other';
  return TYPES[error.constructor?.name ?? ''] ?? 'other';
}

/** At most one report of each kind and code a minute, and ten a minute in all. */
export function limiter(now: () => number = Date.now) {
  const last = new Map<string, number>(); let window = 0; let count = 0;
  return (key: string): boolean => {
    const at = now();
    if (at - window >= 60_000) { window = at; count = 0; }
    if (count >= 10 || at - (last.get(key) ?? -Infinity) < 60_000) return false;
    last.set(key, at); count += 1; return true;
  };
}

/** Listens for the window's problems and sends each through `send`. Returns a stop function. */
export function watchProblems(send: (kind: ProblemKind, code: string) => void, target: Window = window): () => void {
  const allow = limiter();
  const report = (kind: ProblemKind, code: string) => { if (allow(`${kind}:${code}`)) { try { send(kind, code); } catch { /* a report never breaks the page */ } } };
  const onError = (event: ErrorEvent) => report('js_error', errorCode(event.error));
  const onRejection = () => report('js_rejection', 'error');
  target.addEventListener('error', onError);
  target.addEventListener('unhandledrejection', onRejection);
  // A stall: a one-second timer that fires more than two seconds late while the page stays visible.
  let expected = Date.now() + 1000;
  const timer = target.setInterval(() => {
    const late = Date.now() - expected;
    expected = Date.now() + 1000;
    if (late > 2000 && target.document.visibilityState === 'visible') report('ui_stall', 'stall');
  }, 1000);
  return () => { target.removeEventListener('error', onError); target.removeEventListener('unhandledrejection', onRejection); target.clearInterval(timer); };
}
