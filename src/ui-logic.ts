// Pure interface rules, kept free of DOM access so scripts/test-ui-logic.mjs can
// exercise them directly. main.ts owns rendering; these functions own decisions.
import type { Account, AccountLimit, OpenrouterCredit, ProjectRule, RotationPolicy, UpdateStatus, Usage, UsageWindow } from './types';
import { t } from './i18n';

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

/** When the monitor next checks a row whose last check failed, or null. A provider wait
 * (SB-39) is part of this time; no caller checks earlier. */
export function failedNextCheck(account: Pick<Account, 'enabled' | 'kind' | 'usage_health'>): number | null {
  const health = account.usage_health;
  return monitorChecks(account) && health?.status === 'failed' ? health.next_check_at : null;
}

export interface Freshness {
  /** Too old for automatic rotation, or a reported reset time has already passed. */
  stale: boolean;
  /** A reset time has passed since the observation: its figures no longer describe the window. */
  resetPassed: boolean;
}

export function usageFreshness(usage: Pick<Usage, 'observed_at' | 'resets_at' | 'windows'>, maxAgeSeconds: number, nowSeconds: number): Freshness {
  // Only the account's own windows can make the observation stale; a feature window's reset is
  // shown on that window alone (SB-40). With windows stored, the aggregate reset is one of theirs.
  const resets = usage.windows?.length ? usage.windows.filter(w => !isFeatureWindow(w)).map(w => w.resets_at) : [usage.resets_at];
  const resetPassed = resets.some((reset) => typeof reset === 'number' && reset <= nowSeconds);
  return { stale: resetPassed || !Number.isFinite(usage.observed_at) || usage.observed_at > nowSeconds || nowSeconds - usage.observed_at > maxAgeSeconds, resetPassed };
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
/**
 * Journal wording. The core records a fixed vocabulary (`event_valid` in switchboard-core); the
 * window names each action and outcome in the interface language. Anything outside it — an older
 * journal, a newer core — shows as recorded, with underscores as spaces.
 */
export function eventAction(action: string): string {
  switch (action) {
    case 'account_added': return t('Account added');
    case 'account_updated': return t('Account updated');
    case 'account_removed': return t('Account removed');
    case 'account_selected': return t('Account selected');
    case 'usage': return t('Usage check');
    case 'request': return t('Managed request');
    case 'proxy': return t('Proxy');
    case 'project_rule': return t('Project rule');
    case 'project': return t('Project');
    case 'fallback_chain': return t('Fallback chain');
    case 'agent_key': return t('Agent key');
    case 'fallback': return t('Hand-over');
    case 'activation': return t('Switch');
    case 'rotation': return t('Automatic switching');
    case 'launch': return t('Launch');
    case 'login': return t('Sign-in');
    default: return action.replaceAll('.', ' · ').replaceAll('_', ' ');
  }
}
export function eventDetail(detail: string): string {
  switch (detail) {
    case 'success': return t('done');
    case 'observed': return t('observed');
    case 'unavailable': return t('unavailable');
    case 'failed': return t('failed');
    case 'unauthorized': return t('not authorized');
    case 'rate_limited': return t('rate limited');
    case 'upstream_error': return t('provider error');
    case 'network_error': return t('network error');
    case 'started': return t('started');
    case 'stopped': return t('stopped');
    case 'completed': return t('completed');
    case 'aborted': return t('aborted');
    case 'connection_failed': return t('connection failed');
    case 'timeout': return t('timed out');
    case 'rejected': return t('rejected');
    case 'saved': return t('saved');
    case 'paused': return t('paused');
    case 'removed': return t('removed');
    case 'applied': return t('applied');
    case 'created': return t('created');
    case 'updated': return t('updated');
    case 'set': return t('set');
    case 'cleared': return t('cleared');
    case 'offered': return t('offered');
    case 'switched': return t('switched');
    case 'cancelled': return t('cancelled');
    case 'isolated': return t('isolated session');
    case 'managed': return t('managed session');
    default: return /^[2-5]xx$/.test(detail) ? t('HTTP {status}', { status: detail }) : detail.replaceAll('_', ' ');
  }
}
/** Hours offered for a rule's expiry; an empty value keeps the rule until it is paused or removed. */
export const EXPIRY_CHOICES: [string, string][] = [['1', t('For 1 hour')], ['8', t('For 8 hours')], ['24', t('For 24 hours')], ['168', t('For 7 days')], ['', t('Until I pause it')]];
export function expiryFrom(choice: string, nowSeconds: number): number | null {
  const hours = Number(choice);
  return choice && Number.isInteger(hours) && hours > 0 && hours <= 720 ? nowSeconds + hours * 3600 : null;
}
/** The last path segment names a project in lists; the full path stays in the title. */
export function projectName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

// #region quota-order — docs: docs/runs/2026-10-04-quota-review/README.md
export interface QuotaOrderContext {
  nowSeconds: number;
  policies?: Pick<RotationPolicy, 'provider' | 'pool' | 'enabled' | 'max_age_seconds'>[];
  limits?: Pick<AccountLimit, 'account_id' | 'until'>[];
  signInRequired?: string[];
}
/** The reset of the account window in highest use, or null — never a feature window's (SB-40). */
export function accountReset(usage: Usage): number | null {
  const windows = accountWindows(usage);
  if (!windows.length) return null;
  const worst = windows.reduce((a, b) => (b.used_percent > a.used_percent ? b : a));
  return worst.resets_at ?? null;
}
/** How a limit's time reads: the provider's reported reset, or an estimated retry hold (SB-41). */
export function limitLabel(limit: Pick<AccountLimit, 'until' | 'resets_at'>): string {
  return typeof limit.resets_at === 'number' && limit.resets_at === limit.until ? t('Limit resets') : t('Retry hold until');
}
/** A window that limits one metered feature, not the account (core `FEATURE_WINDOW_PREFIX`, SB-40). */
export function isFeatureWindow(window: Pick<UsageWindow, 'name'>): boolean {
  return window.name.startsWith('feature_');
}
/** The account's own windows: every stored window except feature limits; an observation stored
 * without windows is its aggregate. Empty when only feature windows exist — unknown, not zero. */
export function accountWindows(usage: Usage): Pick<UsageWindow, 'used_percent' | 'resets_at'>[] {
  if (!usage.windows?.length) return [{ used_percent: usage.used_percent, resets_at: usage.resets_at }];
  return usage.windows.filter(w => !isFeatureWindow(w));
}
/** The account's capacity in use, or null when no window speaks for the account (core `account_used_percent`). */
export function accountUsedPercent(usage: Usage): number | null {
  const windows = accountWindows(usage);
  return windows.length ? Math.max(...windows.map(w => w.used_percent)) : null;
}
/** A feature window's label: `feature_codex_other_primary` → `codex_other · primary feature limit`. */
export function featureWindowLabel(name: string): string {
  const rest = name.slice('feature_'.length);
  const match = /^(.*)_(primary|secondary)$/.exec(rest);
  return match ? t('{feature} · {window} feature limit', { feature: match[1], window: t(match[2]) }) : t('{feature} · feature limit', { feature: rest });
}
export interface QuotaOrder { state: 'available' | 'blocked' | 'unknown' | 'sign_in' | 'disabled'; until: number | null; used: number }
export function quotaMaxAge(account: Pick<Account, 'provider' | 'pool'>, policies: QuotaOrderContext['policies'] = []): number {
  const ages = policies.filter(p => p.enabled && p.provider === account.provider && p.pool === account.pool).map(p => p.max_age_seconds);
  // Without a policy: core UNPOLICED_MAX_AGE_SECONDS, the ten-minute idle cadence plus slack (SB-48).
  return ages.length ? Math.min(...ages) : 900;
}
const validTime = (value: number | null | undefined): value is number => typeof value === 'number' && Number.isFinite(value) && value > 0 && value * 1000 <= 8.64e15;
/** Display ranking only: never selects an account or alters rotation policy. */
export function quotaOrder(account: Account, context: QuotaOrderContext): QuotaOrder {
  const unknown: QuotaOrder = { state: 'unknown', until: null, used: Infinity };
  if (!account.enabled) return { ...unknown, state: 'disabled' };
  if (context.signInRequired?.includes(account.id)) return { ...unknown, state: 'sign_in' };
  const hold = context.limits?.find(limit => limit.account_id === account.id && validTime(limit.until) && limit.until > context.nowSeconds);
  const usage = account.usage;
  // Feature limits do not rank the account (SB-40); with no account window, usage is unknown.
  const windows = usage ? accountWindows(usage) : [];
  const fresh = account.kind === 'oauth' && usage && account.usage_health?.status !== 'failed' && account.usage_health?.status !== 'unavailable'
    && !usageFreshness(usage, quotaMaxAge(account, context.policies), context.nowSeconds).stale
    && Number.isFinite(usage.used_percent) && windows.length > 0 && windows.every(w => Number.isFinite(w.used_percent) && w.used_percent >= 0 && w.used_percent <= 100);
  if (!fresh) return hold ? { ...unknown, state: 'blocked', until: hold.until } : unknown;
  const used = Math.max(...windows.map(w => w.used_percent));
  const exhausted = windows.filter(w => w.used_percent >= 100);
  if (used >= 100 && !exhausted.length) exhausted.push({ used_percent: used, resets_at: usage.resets_at });
  if (exhausted.length) {
    const known = exhausted.every(w => validTime(w.resets_at) && w.resets_at > context.nowSeconds);
    return { state: 'blocked', used, until: known ? Math.max(hold?.until ?? 0, ...exhausted.map(w => w.resets_at!)) : null };
  }
  return hold ? { state: 'blocked', used, until: hold.until } : { state: 'available', used, until: null };
}
/** Days plus hours stay legible for weekly waits; no decrementing counter or negative values. */
export function resetCountdown(until: number, nowSeconds: number): string {
  if (!validTime(until) || !Number.isFinite(nowSeconds)) return t('Time unavailable');
  const remaining = until - nowSeconds;
  if (remaining <= 0) return t('Due · awaiting check');
  if (remaining < 60) return t('<1m remaining');
  const minutes = Math.ceil(remaining / 60), days = Math.floor(minutes / 1440), hours = Math.floor(minutes % 1440 / 60), mins = minutes % 60;
  return t('{time} remaining', { time: `${days ? `${t('{n}d', { n: days })} ` : ''}${hours || days ? `${t('{n}h', { n: hours })} ` : ''}${t('{n}m', { n: mins })}` });
}

/** The compact wait shown on a card's second line: "4d 22h", "2h 5m", "36m", "<1m", or "now" once due. */
export function compactCountdown(until: number, nowSeconds: number): string {
  if (!validTime(until) || !Number.isFinite(nowSeconds)) return '—';
  const remaining = until - nowSeconds;
  if (remaining <= 0) return t('now');
  if (remaining < 60) return t('<1m');
  const minutes = Math.ceil(remaining / 60), days = Math.floor(minutes / 1440), hours = Math.floor(minutes % 1440 / 60), mins = minutes % 60;
  if (days) return `${t('{n}d', { n: days })} ${t('{n}h', { n: hours })}`;
  return hours ? `${t('{n}h', { n: hours })} ${t('{n}m', { n: mins })}` : t('{n}m', { n: mins });
}

/** A card's at-a-glance state (compact list): what the status mark shows and how line two reads. */
export type CardState = 'available' | 'low' | 'blocked' | 'stale' | 'failed' | 'unknown' | 'sign_in' | 'disabled' | 'no_quota';
/** Used at or above this share of the account's capacity, an available account reads as running low. */
export const LOW_AT_PERCENT = 80;
export function cardState(account: Account, context: QuotaOrderContext): CardState {
  if (!account.enabled) return 'disabled';
  if (context.signInRequired?.includes(account.id)) return 'sign_in';
  if (account.kind !== 'oauth') return 'no_quota';
  const order = quotaOrder(account, context);
  if (order.state === 'blocked') return 'blocked';
  if (order.state === 'available') return order.used >= LOW_AT_PERCENT ? 'low' : 'available';
  const health = account.usage_health?.status;
  if (health === 'failed' || health === 'unavailable') return 'failed';
  return account.usage ? 'stale' : 'unknown';
}

type Groupable = Pick<Account, 'provider' | 'pool' | 'enabled' | 'created_at'>;
export interface AccountGroup<T> { provider: Account['provider']; count: number; pools: { pool: string; accounts: T[] }[] }
/** Provider and pool boundaries survive quota ordering; old callers retain structural order. */
export function groupAccounts<T extends Groupable>(accounts: T[], context?: QuotaOrderContext): AccountGroup<T>[] {
  const groups: AccountGroup<T>[] = [];
  const ranks = { available: 0, blocked: 1, unknown: 2, sign_in: 3, disabled: 4 };
  const compare = (a: T, b: T) => {
    if (context) {
      const left = quotaOrder(a as unknown as Account, context), right = quotaOrder(b as unknown as Account, context);
      const rank = ranks[left.state] - ranks[right.state];
      if (rank) return rank;
      if (left.state === 'available' && left.used !== right.used) return left.used - right.used;
      if (left.state === 'blocked' && left.until !== right.until) return (left.until ?? Infinity) - (right.until ?? Infinity);
    }
    return Number(b.enabled) - Number(a.enabled) || a.created_at - b.created_at || ((a as unknown as Account).id ?? '').localeCompare((b as unknown as Account).id ?? '');
  };
  for (const provider of ['claude', 'codex'] as const) {
    const mine = accounts.filter(account => account.provider === provider);
    if (!mine.length) continue;
    const pools = [...new Set(mine.map(account => account.pool))].sort((a, b) => a === 'default' ? -1 : b === 'default' ? 1 : a.localeCompare(b));
    groups.push({ provider, count: mine.length, pools: pools.map(pool => ({ pool, accounts: mine.filter(account => account.pool === pool).sort(compare) })) });
  }
  return groups;
}
// #endregion quota-order

type Switchable = Pick<Account, 'provider' | 'kind' | 'enabled' | 'external_identity'>;
/** Native Claude Code activation needs a Claude OAuth profile with its captured identity. */
export function canSwitchNative(account: Switchable): boolean {
  return account.provider === 'claude' && account.kind === 'oauth' && !!account.external_identity;
}
export type PrimaryAction = 'switch' | 'in_use' | 'select' | 'selected' | 'sign_in' | 'disabled';
/** The one button a row shows (PLAN-0.5 D-3); everything else lives in the row menu. */
export function primaryAction(account: Switchable, state: { current: boolean; selected: boolean; signIn: boolean; project?: boolean }): PrimaryAction {
  if (!account.enabled) return 'disabled';
  if (state.signIn) return 'sign_in';
  // A project's account never becomes the ordinary Claude Code's (core PROJECT_NOT_NATIVE).
  if (canSwitchNative(account) && !state.project) return state.current ? 'in_use' : 'switch';
  return state.selected ? 'selected' : 'select';
}
/** The pool a one-click "turn on automatic switching" uses, or null when it cannot or need not. */
export function autoSwitchPool(accounts: (Switchable & Pick<Account, 'pool'>)[], policies: Pick<RotationPolicy, 'provider' | 'target' | 'enabled'>[], projectPools: string[] = []): string | null {
  if (policies.some((policy) => policy.provider === 'claude' && policy.target === 'claude_cli' && policy.enabled)) return null;
  const counts = new Map<string, number>();
  for (const account of accounts) if (account.enabled && canSwitchNative(account) && !projectPools.includes(account.pool)) counts.set(account.pool, (counts.get(account.pool) ?? 0) + 1);
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

/** What About says about automatic updates (SB-55), and whether "Restart to update" is offered. */
export function updateLine(status: Pick<UpdateStatus, 'available' | 'reason' | 'enabled' | 'state' | 'current' | 'version' | 'error' | 'needs_permission'>): { text: string; restart: boolean } {
  if (!status.available) return { text: t(status.reason ?? 'Automatic updates work in the installed app only.'), restart: false };
  if (status.state === 'ready' && status.version) {
    return status.needs_permission
      ? { text: t('Version {version} is ready. Restarting asks for an administrator password to replace the app.', { version: status.version }), restart: true }
      : { text: t('Version {version} is ready. It starts on its own when Switchboard is idle in the background or opens again, or restart now.', { version: status.version }), restart: true };
  }
  if (!status.enabled) return { text: t('Version {version}. Automatic updates are off.', { version: status.current }), restart: false };
  if (status.state === 'downloading' && status.version) return { text: t('Downloading version {version}…', { version: status.version }), restart: false };
  if (status.state === 'checking') return { text: t('Checking for updates…'), restart: false };
  if (status.state === 'failed' && status.error) return { text: t(status.error), restart: false };
  return { text: t('Version {version} is the latest. Switchboard checks again every six hours.', { version: status.current }), restart: false };
}

/**
 * The notice after an official sign-in is saved (SB-62). An identity already saved was signed in
 * again in place — same account, label and pool — so it is never announced as added.
 */
export function signInNotice(account: { label: string; pool: string; signed_in_again?: boolean; login_cleanup?: 'done' | 'pending' }, providerLabel: string): string {
  const head = account.signed_in_again
    ? t('{label} is signed in again in {provider} · {pool}.', { label: account.label, provider: providerLabel, pool: account.pool })
    : t('{label} added to {provider} · {pool}.', { label: account.label, provider: providerLabel, pool: account.pool });
  return account.login_cleanup === 'pending'
    ? `${head} ${t('Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.')}`
    : head;
}
/** The balance line of the saved OpenRouter key (SB-79): numbers only, in US dollars. */
export function openrouterCreditLine(credit: Pick<OpenrouterCredit, 'limit' | 'limit_remaining' | 'limit_reset' | 'usage_daily'>): string {
  const money = (value: number | null) => value === null || !Number.isFinite(value) ? null : `$${value.toFixed(2)}`;
  const remaining = money(credit.limit_remaining);
  const today = money(credit.usage_daily) ?? t('unknown');
  const limit = money(credit.limit);
  if (!limit || !remaining) return t('No spending limit · spent today {today}', { today });
  const reset = ({ daily: t('resets daily'), weekly: t('resets weekly'), monthly: t('resets monthly') } as Record<string, string>)[credit.limit_reset ?? ''];
  return reset ? t('{remaining} of {limit} left, {reset} · spent today {today}', { remaining, limit, reset, today }) : t('{remaining} of {limit} left · spent today {today}', { remaining, limit, today });
}
