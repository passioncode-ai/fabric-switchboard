// Pure interface rules, kept free of DOM access so scripts/test-ui-logic.mjs can
// exercise them directly. main.ts owns rendering; these functions own decisions.
import type { Account, ProjectRule, RotationPolicy, Usage } from './types';

export type Appearance = 'system' | 'dark' | 'light';
export type Theme = 'dark' | 'light';
export const APPEARANCE_KEY = 'switchboard.appearance';

export function parseAppearance(value: unknown): Appearance {
  return value === 'dark' || value === 'light' || value === 'system' ? value : 'system';
}

export function resolveTheme(appearance: Appearance, systemPrefersLight: boolean): Theme {
  if (appearance === 'system') return systemPrefersLight ? 'light' : 'dark';
  return appearance;
}

/** The quota monitor polls enabled OAuth accounts only (runtime monitor.rs). */
export function monitorChecks(account: Pick<Account, 'enabled' | 'kind'>): boolean {
  return account.enabled && account.kind === 'oauth';
}

/** Manual checks reach the provider for OAuth accounts only; other kinds always fail. */
export function canProbe(account: Pick<Account, 'kind'>): boolean {
  return account.kind === 'oauth';
}

export interface Freshness {
  /** Too old for automatic rotation, or a reported reset time has already passed. */
  stale: boolean;
  /** A reset time has passed since the observation: its figures no longer describe the window. */
  resetPassed: boolean;
}

export function usageFreshness(usage: Pick<Usage, 'observed_at' | 'resets_at' | 'windows'>, maxAgeSeconds: number, nowSeconds: number): Freshness {
  const resets = [usage.resets_at, ...(usage.windows ?? []).map((window) => window.resets_at)];
  const resetPassed = resets.some((reset) => typeof reset === 'number' && reset <= nowSeconds);
  return { stale: resetPassed || nowSeconds - usage.observed_at > maxAgeSeconds, resetPassed };
}

export function windowReset(resetsAt: number | null | undefined, nowSeconds: number): boolean {
  return typeof resetsAt === 'number' && resetsAt <= nowSeconds;
}

/**
 * Orders background reads against user mutations. A read that started before
 * the latest mutation began may carry pre-mutation state and must be dropped.
 */
export class MutationClock {
  private generation = 0;
  private active = 0;
  /** Call when a user mutation starts. */
  begin(): void { this.generation += 1; this.active += 1; }
  /** Call when a user mutation settles, successfully or not. */
  end(): void { this.active = Math.max(0, this.active - 1); }
  /** Stamp a background read at the moment it is issued. A read issued while a
   *  mutation is running may observe pre-mutation state, so its stamp never matches. */
  stamp(): number { return this.active > 0 ? -1 : this.generation; }
  /** True when no mutation began after the stamp and none is still running. */
  accepts(stamp: number): boolean { return stamp === this.generation && this.active === 0; }
}

export type RuleState = 'active' | 'paused' | 'expired';
/** Same order as the core: a paused rule reads paused even after its expiry. */
export function ruleState(rule: Pick<ProjectRule, 'enabled' | 'expires_at'>, nowSeconds: number): RuleState {
  if (!rule.enabled) return 'paused';
  return typeof rule.expires_at === 'number' && rule.expires_at <= nowSeconds ? 'expired' : 'active';
}
export function activeRules<T extends Pick<ProjectRule, 'enabled' | 'expires_at'>>(rules: T[] | undefined, nowSeconds: number): T[] {
  return (rules ?? []).filter((rule) => ruleState(rule, nowSeconds) === 'active');
}
/** Hours offered for a rule's expiry; an empty value keeps the rule until it is paused or removed. */
export const EXPIRY_CHOICES: [string, string][] = [['1', 'For 1 hour'], ['8', 'For 8 hours'], ['24', 'For 24 hours'], ['168', 'For 7 days'], ['', 'Until I pause it']];
export function expiryFrom(choice: string, nowSeconds: number): number | null {
  const hours = Number(choice);
  return choice && Number.isInteger(hours) && hours > 0 && hours <= 720 ? nowSeconds + hours * 3600 : null;
}
/** The last path segment names a project in lists; the full path stays in the title. */
export function projectName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

type Groupable = Pick<Account, 'provider' | 'pool' | 'enabled' | 'created_at'>;
export interface AccountGroup<T> { provider: Account['provider']; count: number; pools: { pool: string; accounts: T[] }[] }
/** Provider sections (Claude Code first), pool sub-groups (default first), enabled before disabled. */
export function groupAccounts<T extends Groupable>(accounts: T[]): AccountGroup<T>[] {
  const groups: AccountGroup<T>[] = [];
  for (const provider of ['claude', 'codex'] as const) {
    const mine = accounts.filter((account) => account.provider === provider);
    if (!mine.length) continue;
    const pools = [...new Set(mine.map((account) => account.pool))].sort((a, b) => (a === 'default' ? -1 : b === 'default' ? 1 : a.localeCompare(b)));
    groups.push({
      provider, count: mine.length,
      pools: pools.map((pool) => ({ pool, accounts: mine.filter((account) => account.pool === pool).sort((a, b) => Number(b.enabled) - Number(a.enabled) || a.created_at - b.created_at) })),
    });
  }
  return groups;
}

type Switchable = Pick<Account, 'provider' | 'kind' | 'enabled' | 'external_identity'>;
/** Native Claude Code activation needs a Claude OAuth profile with its captured identity. */
export function canSwitchNative(account: Switchable): boolean {
  return account.provider === 'claude' && account.kind === 'oauth' && !!account.external_identity;
}
export type PrimaryAction = 'switch' | 'in_use' | 'select' | 'selected' | 'sign_in' | 'disabled';
/** The one button a row shows (PLAN-0.5 D-3); everything else lives in the row menu. */
export function primaryAction(account: Switchable, state: { current: boolean; selected: boolean; signIn: boolean }): PrimaryAction {
  if (!account.enabled) return 'disabled';
  if (state.signIn) return 'sign_in';
  if (canSwitchNative(account)) return state.current ? 'in_use' : 'switch';
  return state.selected ? 'selected' : 'select';
}
/** The pool a one-click "turn on automatic switching" uses, or null when it cannot or need not. */
export function autoSwitchPool(accounts: (Switchable & Pick<Account, 'pool'>)[], policies: Pick<RotationPolicy, 'provider' | 'target' | 'enabled'>[]): string | null {
  if (policies.some((policy) => policy.provider === 'claude' && policy.target === 'claude_cli' && policy.enabled)) return null;
  const counts = new Map<string, number>();
  for (const account of accounts) if (account.enabled && canSwitchNative(account)) counts.set(account.pool, (counts.get(account.pool) ?? 0) + 1);
  let best: string | null = null; let most = 1;
  for (const [pool, count] of counts) if (count > most || (count === most && best !== null && pool === 'default')) { best = pool; most = count; }
  return best;
}

/** Backend sign-in outcomes the banner acts on (runtime lib.rs finish_login, cancel_login); adapter.ts shows both verbatim. */
export const LOGIN_FORGOTTEN = 'Sign-in not found. Start again.';
export const LOGIN_SAVED_CLEANUP = 'Account saved; isolated login cleanup needs attention.';
export type LoginOutcome = 'saved_with_cleanup' | 'forgotten' | 'retry';
/**
 * What a failed sign-in step means, read from the already-sanitized error text.
 * saved_with_cleanup: the account was saved and the owner released the sign-in; it retries the
 * staging cleanup before the next sign-in. forgotten: the owner no longer knows this sign-in
 * (it restarted), so nothing can finish or cancel it. retry: anything else keeps the banner.
 */
export function loginOutcome(errorText: string): LoginOutcome {
  if (errorText === LOGIN_SAVED_CLEANUP) return 'saved_with_cleanup';
  if (errorText === LOGIN_FORGOTTEN) return 'forgotten';
  return 'retry';
}

export interface Timers { set: (fn: () => void, ms: number) => unknown; clear: (handle: unknown) => void }
const browserTimers: Timers = { set: (fn, ms) => setInterval(fn, ms), clear: (handle) => clearInterval(handle as ReturnType<typeof setInterval>) };
/**
 * A repeating timer that exists only while `sync(true)` says it should (lifecycle LC-08): the
 * sign-in poll wakes the window only while a sign-in waits, never as a permanent interval.
 * `sync` is idempotent, so it can be called on every render.
 */
export function intervalWhile(tick: () => void, ms: number, timers: Timers = browserTimers) {
  let handle: unknown;
  return {
    sync(active: boolean) {
      if (active && handle === undefined) handle = timers.set(tick, ms);
      else if (!active && handle !== undefined) { timers.clear(handle); handle = undefined; }
    },
    get running() { return handle !== undefined; },
  };
}
