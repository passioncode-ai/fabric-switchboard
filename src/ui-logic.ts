// Pure interface rules, kept free of DOM access so scripts/test-ui-logic.mjs can
// exercise them directly. main.ts owns rendering; these functions own decisions.
import type { ShellHookNotice, ShellHookStatus } from './types';
import type { Account, AccountLimit, Organization, OrganizationGroupOffer, AgentRow, AgentSetup, OpenrouterCredit, RecommendedModel, SessionInfo, ProjectRule, RotationPolicy, UpdateStatus, Usage, UsageWindow } from './types';
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
    case 'project_rule': return t('Folder rule');
    case 'project': return t('Group');
    case 'fallback_chain': return t('Fallback chain');
    case 'agent_key': return t('Agent key');
    case 'kimi_account': return t('Kimi Code account');
    case 'organization': return t('Organization');
    case 'fallback': return t('Hand-over');
    case 'activation': return t('Switch');
    case 'rotation': return t('Automatic switching');
    case 'launch': return t('Launch');
    case 'login': return t('Sign-in');
    // UX-26: a word this window does not know yet is framed in the interface language, never shown bare.
    default: return t('Other event ({code})', { code: action.replaceAll('.', ' · ').replaceAll('_', ' ') });
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
    case 'added': return t('added');
    case 'assigned': return t('assigned');
    case 'applied': return t('applied');
    case 'created': return t('created');
    case 'updated': return t('updated');
    case 'sources': return t('sources changed');
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
/**
 * SB-158: where an account belongs. Mirrors `Snapshot::attribution` in switchboard-core: the
 * person's word (`memberships`), then the account's provider organization (a Claude Team or a
 * ChatGPT workspace recorded on an organization), then its address's domain. Same cases in
 * `organizations::tests` and `scripts/test-ui-logic.mjs`.
 */
export type AttributedBy = 'person' | 'provider' | 'domain';
export interface OrgSnapshot { organizations?: Organization[]; memberships?: Record<string, string> }
export function providerOrganization(account: Pick<Account, 'provider' | 'external_identity'>): string | null {
  const identity = account.external_identity;
  const id = account.provider === 'claude' ? identity?.organization_id : identity?.account_id;
  return id ? `${account.provider}:${id}` : null;
}
export function accountDomain(account: Pick<Account, 'label' | 'external_identity'>): string | null {
  const email = account.external_identity?.email ?? account.label;
  const at = email.lastIndexOf('@');
  if (at <= 0 || /\s/.test(email.slice(0, at))) return null;
  const domain = email.slice(at + 1).trim().replace(/\.$/, '').toLowerCase();
  return /^([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]*[a-z0-9])?$/.test(domain) ? domain : null;
}
export function attribution(snapshot: OrgSnapshot, account: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>): { organization: string | null; by: AttributedBy | null } {
  const organizations = snapshot.organizations ?? [];
  const choice = snapshot.memberships?.[account.id];
  if (choice === 'personal') return { organization: null, by: 'person' };
  if (choice && organizations.some((o) => o.id === choice)) return { organization: choice, by: 'person' };
  const key = providerOrganization(account);
  const team = key ? organizations.find((o) => (o.provider_organizations ?? []).includes(key)) : undefined;
  if (team) return { organization: team.id, by: 'provider' };
  const domain = accountDomain(account);
  const owner = domain ? organizations.find((o) => (o.domains ?? []).includes(domain)) : undefined;
  if (owner) return { organization: owner.id, by: 'domain' };
  return { organization: null, by: null };
}
/** SB-109: whether `account` may serve a group bound to `organization` (null: personal), as
 * `Snapshot::context_admits`: always while no organization exists. */
export function groupAdmits(snapshot: OrgSnapshot, organization: string | null, account: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>): boolean {
  if (!(snapshot.organizations ?? []).length) return true;
  return attribution(snapshot, account).organization === organization;
}
/** SB-170: a group bound to an organization whose accounts all belong elsewhere now (their
 * domain or team moved, or the person placed them): its folders keep the group's route and every
 * request is refused (fail closed), so its card says why. */
export function groupSeatsElsewhere(snapshot: OrgSnapshot, organization: string | null, accounts: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>[]): boolean {
  if (!organization || !accounts.length) return false;
  return !accounts.some((a) => groupAdmits(snapshot, organization, a));
}
/** SB-160 (B-6, SCN-078): the card line of a seat the provider no longer accepts (the monitor's
 * `sign_in_required`), naming its organization; null for a personal account or a signed-in one,
 * which keep today's text. */
export function seatEndedText(snapshot: OrgSnapshot, account: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>, signInRequired: string[] | undefined): string | null {
  if (!signInRequired?.includes(account.id)) return null;
  const id = attribution(snapshot, account).organization;
  const organization = id ? (snapshot.organizations ?? []).find((o) => o.id === id) : undefined;
  if (!organization) return null;
  return t("{label}'s sign-in has ended. If {organization} removed your seat, ask its administrator; to keep using it, sign in again.", { label: account.label, organization: organization.name });
}
/** SB-160 (B-6, SCN-078): a group bound to an organization whose seats (its accounts of that
 * organization) have all ended says so; a group whose accounts all belong elsewhere keeps
 * SB-170's line instead, and an empty group its own note. */
export function groupSeatsEndedText(snapshot: OrgSnapshot, group: { name: string; organization?: string | null }, accounts: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>[], signInRequired: string[] | undefined): string | null {
  const organization = group.organization ?? null;
  if (!organization || !signInRequired) return null;
  const seats = accounts.filter((a) => groupAdmits(snapshot, organization, a));
  if (!seats.length || !seats.every((a) => signInRequired.includes(a.id))) return null;
  return t("None of {group}'s accounts is signed in. Sessions in its folders are refused until one is.", { group: group.name });
}
/** SB-159: the PassionCode.ai panel's line as a translation key and its values. */
export function passioncodeLine(status: { connected: boolean; state?: string; read_at?: number | null; organizations?: number } | null): { key: string; values?: Record<string, string | number> } {
  if (!status || !status.connected) return { key: 'Not connected. Connect to bring your PassionCode.ai organizations here with their names and domains, kept current.' };
  switch (status.state) {
    case 'sign_in_again': return { key: 'Sign in to PassionCode.ai again: the connection ended or was revoked.' };
    case 'provider_sign_in': return { key: 'Sign in to PassionCode.ai in your browser, then read again.' };
    case 'unreachable': return status.read_at ? { key: 'PassionCode.ai could not be reached. The organizations read {date} stay.', values: { date: status.read_at } } : { key: 'PassionCode.ai could not be reached yet.' };
    default: return { key: 'Connected: {organizations}, read {date}.', values: { count: status.organizations ?? 0, date: status.read_at ?? 0 } };
  }
}
/** The Accounts filter (SB-158): every account, the personal ones, or one organization's. */
export function inOrganizationFilter(snapshot: OrgSnapshot, account: Pick<Account, 'id' | 'label' | 'provider' | 'external_identity'>, filter: string): boolean {
  if (filter === 'all') return true;
  const at = attribution(snapshot, account).organization;
  return filter === 'personal' ? at === null : at === filter;
}
/** "a.example, b.example" from what a person typed: commas, spaces or new lines. */
export function domainList(text: string): string[] {
  return [...new Set(text.split(/[\s,;]+/).map((d) => d.trim().replace(/^@/, '').replace(/\.$/, '').toLowerCase()).filter(Boolean))];
}
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
/** What adapter.ts shows for LOGIN_SAVED_CLEANUP (CPY-05): the effect, then what happens next. */
export const LOGIN_SAVED_CLEANUP_SHOWN = 'The account was saved. A temporary sign-in folder was left behind; Switchboard removes it before the next sign-in.';
export type LoginOutcome = 'saved_with_cleanup' | 'forgotten' | 'retry';
/**
 * What a failed sign-in step means, read from the already-sanitized error text.
 * saved_with_cleanup: the account was saved and the owner released the sign-in; it retries the
 * staging cleanup before the next sign-in. forgotten: the owner no longer knows this sign-in
 * (it restarted), so nothing can finish or cancel it. retry: anything else keeps the banner.
 */
export function loginOutcome(errorText: string): LoginOutcome {
  if (errorText === LOGIN_SAVED_CLEANUP || errorText === LOGIN_SAVED_CLEANUP_SHOWN) return 'saved_with_cleanup';
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
  // SB-104: a check that failed says so, even with automatic updates off (a manual check).
  if (status.state === 'failed' && status.error) return { text: t(status.error), restart: false };
  if (status.state === 'checking') return { text: t('Checking for updates…'), restart: false };
  if (status.state === 'downloading' && status.version) return { text: t('Downloading version {version}…', { version: status.version }), restart: false };
  if (!status.enabled) return { text: t('Version {version}. Automatic updates are off.', { version: status.current }), restart: false };
  return { text: t('Version {version} is the latest. Switchboard checks again every five minutes.', { version: status.current }), restart: false };
}

/**
 * The notice after an official sign-in is saved (SB-62). An identity already saved was signed in
 * again in place — same account, label and pool — so it is never announced as added.
 */
export function signInNotice(account: { label: string; pool: string; signed_in_again?: boolean; login_cleanup?: 'done' | 'pending'; organization_group?: OrganizationGroupOffer }, providerLabel: string, projects: { pool: string; name: string }[] = []): string {
  // D-1: the group by its name or the default group, never a pool id alone.
  const where = inGroup(account.pool, projects);
  const head = account.signed_in_again
    ? t('{label} is signed in again in {provider} {where}.', { label: account.label, provider: providerLabel, where })
    : t('{label} added to {provider} {where}.', { label: account.label, provider: providerLabel, where });
  const text = account.login_cleanup === 'pending'
    ? `${head} ${t('Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.')}`
    : head;
  const note = organizationMoveNote(account.label, providerLabel, account.organization_group);
  return note ? `${text} ${note}` : text;
}
/**
 * SB-160 (B-4, SCN-077): the ordinary CLI's own account cannot join a group, so its offer is a
 * sentence instead of an action; empty otherwise. Draft copy until wave 4.
 */
export function organizationMoveNote(label: string, providerLabel: string, offer: OrganizationGroupOffer | undefined): string {
  return offer?.in_cli ? t('Switch {provider} to another account first to move {label} to group “{group}”.', { provider: providerLabel, label, group: offer.name }) : '';
}
/** SB-160 (B-4, SCN-077): the notice's one action for a seat that landed outside its organization's group; nothing moves until the person presses it. */
export function signInMove(account: { id: string; label: string; organization_group?: OrganizationGroupOffer }): { id: string; pool: string; label: string; done: string } | null {
  const offer = account.organization_group;
  if (!offer || offer.in_cli) return null;
  return { id: account.id, pool: offer.pool, label: t('Move it to “{group}”', { group: offer.name }), done: t('{label} moved to group “{name}”.', { label: account.label, name: offer.name }) };
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
/** One Kimi Code plan window (SB-81), as the backend reads `GET /usages`. */
export interface KimiWindow { name: string; used_percent: number; resets_at: number | null }
/** The plan windows of a Kimi Code account on one line: "5 hours 25% · resets in 3h 10m · 7 days 50%". */
export function kimiWindowsLine(windows: KimiWindow[], nowSeconds: number): string {
  return kimiWindowsLines(windows, nowSeconds).join(' · ');
}
/** The same windows, one per line (SB-142: an account row shows them stacked, not as one long line). */
export function kimiWindowsLines(windows: KimiWindow[], nowSeconds: number): string[] {
  const names: Record<string, string> = { '5h': t('5 hours'), '7d': t('7 days'), month: t('Month'), month_code: t('Month · code') };
  return windows.map((w) => {
    const name = names[w.name] ?? w.name;
    // A window whose reset has passed no longer holds the percent it showed (SB-100).
    if (w.resets_at && w.resets_at <= nowSeconds) return `${name} ${t('reset, refresh to see it')}`;
    const part = `${name} ${Math.round(w.used_percent)}%`;
    return w.resets_at ? `${part}, ${t('resets in {time}', { time: compactCountdown(w.resets_at, nowSeconds) })}` : part;
  });
}
/**
 * Provider terms notice (operator decision 2026-10-08, SCN-049). About shows it always; Accounts
 * shows it from the first start until the person dismisses it, so it is in sight while the first
 * account is added. A store that refuses to read or write counts as not dismissed: the notice
 * shows again next start rather than never (as the tour, SCN-035).
 */
export const TERMS_NOTICE_KEY = 'switchboard.terms-notice';
export interface NoticeStore { getItem(key: string): string | null; setItem(key: string, value: string): void }
export function termsNoticeText(): string {
  return t('Use Switchboard in line with the terms of Anthropic and OpenAI. Breaking a provider\'s terms can get your account blocked. You are responsible for how you use your accounts.');
}
export function termsNoticeDismissed(store: NoticeStore | null | undefined): boolean {
  try { return !!store?.getItem(TERMS_NOTICE_KEY); } catch { return false; }
}
/** Remembers the dismissal; false when the store refused it (the notice is hidden for this run only). */
export function dismissTermsNotice(store: NoticeStore | null | undefined): boolean {
  if (!store) return false;
  try { store.setItem(TERMS_NOTICE_KEY, '1'); return true; } catch { return false; }
}
/** How a screen shows the notice: a dismissible banner, a permanent About panel, or not at all. */
export function termsNoticeOn(page: string, dismissed: boolean): 'banner' | 'panel' | null {
  if (page === 'about') return 'panel';
  return page === 'accounts' && !dismissed ? 'banner' : null;
}

/**
 * Usage counts: nothing is sent before the person has answered the switch once in this app
 * (issue #128, operator decision 2026-10-08, Fabric ADR-0127; docs/ANALYTICS.md). The window asks
 * when the build can send (`available`), the shared switch is on and this app has no answer yet
 * (`disclosed === false`). A status that is missing, unreadable or from an older backend without
 * `disclosed` asks nothing: the backend still sends nothing until it is answered.
 */
export interface UsageStatus { available: boolean; enabled: boolean; disclosed?: boolean; sent_before?: boolean }
export function usageQuestion(status: UsageStatus | null | undefined): boolean {
  return !!status && status.available && status.enabled && status.disclosed === false;
}
/** What the question says about what has left so far: an install from 0.6.81 or earlier may already have sent counts. */
export function usageQuestionText(status: UsageStatus | null | undefined): string {
  return status?.sent_before
    ? t('An earlier version of Switchboard may already have sent these counts from this computer. Nothing more is sent until you answer.')
    : t('Nothing has been sent yet. Nothing is sent until you answer.');
}
/** What an answer changed, from the status the backend returned: an "on" given after another tool turned the shared switch off leaves it off and says so. */
export function usageAnswerText(wanted: boolean, status: UsageStatus | null | undefined): string {
  if (wanted && status && !status.enabled) return t('Usage counts stay off: another PassionCode.ai tool on this computer turned them off. Switch them on in About to count again.');
  return wanted ? t('Anonymous usage analytics are on.') : t('Anonymous usage analytics are off for every PassionCode.ai tool on this computer.');
}
/** The first-run tour waits while the usage question is open or its status is still being read. */
export function tourWaits(status: UsageStatus | null | undefined, statusFailed: boolean): boolean {
  if (statusFailed) return false;
  return status == null || usageQuestion(status);
}

/**
 * How an account launches (operator decision 2026-10-09): every session starts in a folder chosen
 * with the picker, and a box makes it isolated — in the account's own home. A managed session, through
 * the local proxy, is open only to the account selected in its pool while the proxy runs; otherwise
 * the session is isolated and the box stays ticked.
 */
export function launchChoice(selectedInPool: boolean, proxyRunning: boolean): { managed: boolean; isolatedByDefault: boolean } {
  const managed = selectedInPool && proxyRunning;
  return { managed, isolatedByDefault: !managed };
}
/** A picked folder joins the list once; the order the person picked them in is kept. */
export function withFolder(folders: string[], folder: string): string[] {
  return !folder || folders.includes(folder) ? folders : [...folders, folder];
}
/** SB-131: a folder changed with the picker takes the old one's place; one already listed is not added twice. */
export function replaceFolder(folders: string[], old: string, folder: string): string[] {
  if (!folder || !folders.includes(old)) return folders;
  if (folders.includes(folder)) return folders.filter((entry) => entry !== old);
  return folders.map((entry) => (entry === old ? folder : entry));
}
/** SB-122: where an account can move. The default pool first, then each project by name; `current` marks where it is, `blocked` a project the CLI's own account cannot join. An account left in a deleted project's pool has no current option. */
export interface MoveTarget { pool: string; name: string | null; current: boolean; blocked: boolean }
export function moveTargets(currentPool: string, projects: { pool: string; name: string }[], inCli: boolean): MoveTarget[] {
  const sorted = [...projects].sort((a, b) => a.name.localeCompare(b.name));
  return [{ pool: 'default', name: null }, ...sorted.map((p) => ({ pool: p.pool, name: p.name }))].map((target) => {
    const current = target.pool === currentPool;
    return { ...target, current, blocked: inCli && target.pool !== 'default' && !current };
  });
}
/** SB-135: a project's folders as Git sees them: each root with its worktrees and clones. */
export interface DerivedFolder { folder: string; kind: 'worktree' | 'clone'; branch: string | null }
export interface ProjectRoot { folder: string; branch: string | null; derived: DerivedFolder[]; remote?: string | null; name?: string }
export interface FolderRow { folder: string; depth: 0 | 1; kind: 'worktree' | 'clone' | null; branch: string | null; derived: number }
/** The rows of a folder list: each root, then (when `open`) its derived folders under it; a folder the tree does not know (just added, or the tree not read yet) is a root of its own, in its place. */
export function folderRows(folders: string[], roots: ProjectRoot[] | null, open = true): FolderRow[] {
  const listed = new Set(folders); const placed = new Set<string>(); const rows: FolderRow[] = [];
  const byRoot = new Map((roots ?? []).map((r) => [r.folder, r]));
  const under = new Map<string, string>(); for (const r of roots ?? []) for (const d of r.derived) under.set(d.folder, r.folder);
  for (const folder of folders) {
    if (placed.has(folder)) continue;
    const parent = under.get(folder);
    if (parent && listed.has(parent)) continue; // listed under its root
    const root = byRoot.get(folder); const derived = (root?.derived ?? []).filter((d) => listed.has(d.folder));
    rows.push({ folder, depth: 0, kind: null, branch: root?.branch ?? null, derived: derived.length }); placed.add(folder);
    for (const d of derived) { placed.add(d.folder); if (open) rows.push({ folder: d.folder, depth: 1, kind: d.kind, branch: d.branch, derived: 0 }); }
  }
  return rows;
}
/** A folder's own name (its last path segment), for a narrow list; the full path goes in the title. */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
/**
 * SB-146: the projects a group holds, by name: one per repository (its root folders, never a
 * worktree, a clone or a subfolder of a listed folder), two roots of one `origin` read once.
 */
export function groupProjects(folders: string[], roots: ProjectRoot[] | null): string[] {
  return groupProjectRows(folders, roots).map((p) => p.name).sort((a, b) => a.localeCompare(b));
}
/** SB-155: a project's name as in the person's folders (the main checkout's folder, from Git), else the folder's own. */
function projectNameOf(row: FolderRow, root: ProjectRoot | undefined): string {
  return root?.name || folderName(row.folder);
}
/** SB-155: two projects with one name read with their owner in brackets: `research-agent (prowl-ai)`. */
function uniqueNames<T extends { key: string; name: string }>(projects: T[]): T[] {
  const count = new Map<string, number>();
  for (const p of projects) count.set(p.name, (count.get(p.name) ?? 0) + 1);
  return projects.map((p) => {
    if ((count.get(p.name) ?? 0) < 2) return p;
    const owner = p.key.replace(/[\\/]+$/, '').split(/[\\/]/).slice(-2, -1)[0];
    return owner ? { ...p, name: `${p.name} (${owner})` } : p;
  });
}
/** SB-135: taking a root out of the list takes its worktrees and clones with it. */
export function withoutFolder(folders: string[], roots: ProjectRoot[] | null, folder: string): string[] {
  const gone = new Set([folder, ...((roots ?? []).find((r) => r.folder === folder)?.derived.map((d) => d.folder) ?? [])]);
  return folders.filter((f) => !gone.has(f));
}
/** SB-135 part 2: the Git repositories on disk with their projects (`repository_map`). */
export interface FoundRepository { folder: string; remote: string | null; branch: string | null; pool: string | null; project: string | null; place: 'folder' | 'inside' | null; groups?: { pool: string; name: string }[] }
export interface RepositoryMap { under: string[]; repositories: FoundRepository[]; truncated: boolean }
export type MapFilter = 'unassigned' | 'all';
export interface MapRow extends FoundRepository { name: string; action: 'assign' | 'move' | null }
/** The rows the disk map shows: the unassigned ones only, or all; a repository inside a project folder moves with that folder, so it has no action of its own. */
export function mapRows(map: RepositoryMap | null, filter: MapFilter): MapRow[] {
  return (map?.repositories ?? [])
    .filter((r) => filter === 'all' || !r.pool)
    .map((r) => ({ ...r, name: folderName(r.folder), action: !r.pool ? 'assign' as const : r.place === 'folder' ? 'move' as const : null }));
}
/** Where a repository can go: each project by name, the one it is in now marked (and not chosen again). */
export function rootTargets(currentPool: string | null, projects: { pool: string; name: string }[]): { pool: string; name: string; current: boolean }[] {
  return [...projects].sort((a, b) => a.name.localeCompare(b.name)).map((p) => ({ pool: p.pool, name: p.name, current: p.pool === currentPool }));
}
/** The folders the disk map looks under, remembered across restarts; none: the default (`~/DATA`, else home). */
export const MAP_FOLDERS_KEY = 'switchboard.map-folders';
export function storedMapFolders(store: Pick<Storage, 'getItem'> | null): string[] {
  try {
    const value: unknown = JSON.parse(store?.getItem(MAP_FOLDERS_KEY) ?? '[]');
    return Array.isArray(value) ? value.filter((f): f is string => typeof f === 'string' && f.trim() !== '').slice(0, 16) : [];
  } catch { return []; }
}
/** The last launch folder, remembered across restarts as a per-window convenience; anything but a non-empty string reads as none. */
export const LAST_FOLDER_KEY = 'switchboard.last-folder';
export function storedFolder(store: Pick<Storage, 'getItem'> | null): string {
  try { const value = store?.getItem(LAST_FOLDER_KEY); return typeof value === 'string' ? value.trim() : ''; } catch { return ''; }
}
/** SB-93: what the Agents panel says about setup — each agent's tools registered or not, and whether any agent has called Switchboard yet. */
export function agentSetupFacts(setup: Pick<AgentSetup, 'connected' | 'proved'>): { text: string; done: boolean }[] {
  const facts: { text: string; done: boolean }[] = [];
  if (setup.connected) {
    for (const [agent, on] of [[t('Claude Code'), setup.connected.claude_code], [t('Codex CLI'), setup.connected.codex]] as const) {
      facts.push({ text: on ? t('{agent}: Switchboard tools registered.', { agent }) : t('{agent}: Switchboard tools not registered.', { agent }), done: on });
    }
  }
  const client = setup.proved?.client;
  facts.push(setup.proved ? { text: client ? t('An agent has called Switchboard: {client}.', { client }) : t('An agent has called Switchboard.'), done: true } : { text: t('No agent has called Switchboard yet.'), done: false });
  return facts;
}
/** SB-65: a session's state and how it runs, as the Activity page names them. */
export function sessionStateLabel(state: SessionInfo['state']): string {
  return state === 'running' ? t('Running') : state === 'starting' ? t('Starting') : state === 'ended' ? t('Ended') : t('State unknown');
}
export function sessionHow(session: Pick<SessionInfo, 'mode' | 'in_place' | 'pool'>, projects: { pool: string; name: string }[] = []): string {
  const mode = session.mode === 'managed' ? t('Managed · follows the selection {where}', { where: inGroup(session.pool, projects) }) : t('Isolated');
  return session.in_place ? t('{mode} · in an embedded terminal', { mode }) : t('{mode} · in Terminal', { mode });
}
/** SB-98: what a limit row says — a hidden limit (refused while the usage figures showed room) first. */
export function limitTitle(limit: Pick<AccountLimit, 'source' | 'hidden'>): string {
  if (limit.hidden) return t('Hidden personal limit · refused although the usage figures show room');
  return limit.source === 'managed' ? t('Limit reached · a managed request was refused') : t('Limit reached · reported by Claude Code');
}
/** SB-98: the provider's reason for refusing more, in words; null when none was given. */
export function limitReasonText(reason: string | null | undefined): string | null {
  if (!reason) return null;
  switch (reason) {
    case 'org_spend_cap_reached': return t('The organisation\'s spend cap is reached, so extra usage is off until it resets.');
    case 'group_zero_credit_limit': return t('No credits are left for extra usage.');
    case 'org_level_disabled': return t('Extra usage is turned off for the organisation.');
    default: return t('Provider reason: {reason}', { reason });
  }
}
/** SB-99: a Kimi plan window at 100 % stops the account until it resets — the latest such reset, or 0 when one names none. */
export function kimiLimitUntil(windows: KimiWindow[] | undefined, nowSeconds: number): number | null {
  const full = (windows ?? []).filter((w) => w.used_percent >= 100 && !(w.resets_at && w.resets_at <= nowSeconds));
  if (!full.length) return null;
  return Math.max(0, ...full.map((w) => w.resets_at ?? 0));
}
/** SB-100: the ordinary `kimi` is not an account Switchboard launches or signs in: its own words. */
export function ordinaryKimiNote(status: { signed_in: boolean; error: string | null }): string | null {
  if (!status.signed_in) return t('Not signed in. Run kimi login in Terminal.');
  if (status.error === 'Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.') return t('Run kimi once in Terminal to renew its sign-in, then refresh.');
  return status.error ? t(status.error) : null;
}

/** SB-120: the order agents take over work in — the subscriptions-first preset (Claude Code,
 * Codex, Kimi Code, Hermes), then OpenCode. Tools-only agents follow, in the catalog's order. */
export const TAKEOVER_ORDER = ['claude-code', 'codex', 'kimi-code', 'hermes', 'opencode'];
const takeoverRank = (id: string) => { const i = TAKEOVER_ORDER.indexOf(id); return i < 0 ? TAKEOVER_ORDER.length : i; };
export function sortAgents<T extends Pick<AgentRow, 'id'>>(agents: T[]): T[] {
  return agents.map((agent, index) => ({ agent, index })).sort((a, b) => takeoverRank(a.agent.id) - takeoverRank(b.agent.id) || a.index - b.index).map((x) => x.agent);
}
/** The default-agent choices: Auto, then every enabled agent that can take over work. A saved
 * default that is no longer usable (uninstalled, say) stays listed and selected, marked, so the
 * person sees why `continue` refuses and can pick another. */
export function defaultAgentChoices(agents: Pick<AgentRow, 'id' | 'name' | 'enabled' | 'switchable' | 'installed'>[], current: string | null): { id: string | null; label: string; on: boolean }[] {
  const choices: { id: string | null; label: string; on: boolean }[] = [{ id: null, label: t('Auto'), on: current === null }];
  for (const agent of sortAgents(agents)) {
    if (agent.switchable && agent.enabled) choices.push({ id: agent.id, label: agent.name, on: current === agent.id });
    else if (agent.id === current) choices.push({ id: agent.id, label: agent.installed ? t('{agent} (disabled)', { agent: agent.name }) : t('{agent} (not installed)', { agent: agent.name }), on: true });
  }
  return choices;
}
/** A token count as OpenRouter shows context: 1.05M, 200K, 10M. */
export function tokenCount(n: number): string {
  if (n >= 1_000_000) return `${Number((n / 1_000_000).toFixed(2))}M`;
  return `${Math.round(n / 1000)}K`;
}
/** A recommended model's line: maker, context and the input price per million tokens. */
export function modelMeta(m: Pick<RecommendedModel, 'maker' | 'context' | 'input_usd_per_million'>): string {
  return t('{maker} · {context} context · ${price} per 1M input tokens', { maker: m.maker, context: tokenCount(m.context), price: m.input_usd_per_million.toFixed(2) });
}

/** SB-145: one row of the proxy's counts. */
export interface StatCounts { requests: number; ok: number; limited: number; failed: number; input_tokens: number; output_tokens: number; cache_read_tokens: number; cache_write_tokens: number }
export interface StatAccount extends StatCounts { account_id: string; label: string | null; provider: string | null; kind: string | null }
export interface StatGroup extends StatCounts { pool: string; name: string | null }
/** SB-147: a project (a repository with its checkouts, or a folder outside Git); `project` empty for requests whose folder is not known. */
export interface StatProject extends StatCounts { project: string; name: string | null; groups: { pool: string; name: string | null }[] }
export interface StatAgent extends StatCounts { agent: string }
/** SB-109: by whose seat requests ran: an organization's id, `personal`, or `unknown` (a removed account). */
export interface StatOrganization extends StatCounts { organization: string; name: string | null }
export interface StatRoute extends StatAccount { pool: string; name: string | null; project: string; project_name: string | null; agent: string }
export interface StatDay extends StatCounts { day: string; groups?: { pool: string; requests: number; tokens: number }[] }
export interface Statistics { days: number; from: string; to: string; since: string | null; total: StatCounts; groups: StatGroup[]; organizations?: StatOrganization[]; projects: StatProject[]; accounts: StatAccount[]; agents: StatAgent[]; routes: StatRoute[]; by_day: StatDay[] }
/** A count as people read it in a table: 950, 12.4K, 3.21M. */
export function compactCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Number((n / 1000).toFixed(1))}K`;
  if (n < 1_000_000_000) return `${Number((n / 1_000_000).toFixed(2))}M`;
  return `${Number((n / 1_000_000_000).toFixed(2))}B`;
}
/** Tokens a request spent: what was sent (new and from cache) and what came back. */
export function statTokens(c: StatCounts): number {
  return c.input_tokens + c.cache_read_tokens + c.cache_write_tokens + c.output_tokens;
}
/** The share of requests that ended in a limit or an error, as a whole percent; null with none. */
export function statTrouble(c: StatCounts): number | null {
  return c.requests ? Math.round(((c.limited + c.failed) * 100) / c.requests) : null;
}
/** VIS-23: limits and errors are an alert from 1 % of requests, measured exactly (statTrouble rounds). */
export function statAlert(c: StatCounts): boolean {
  return c.requests > 0 && (c.limited + c.failed) * 100 >= c.requests;
}
/** The trouble share as text: a few failures among many requests read as under 1%, never 0%. */
export function statTroubleText(c: StatCounts): string {
  const share = statTrouble(c);
  if (share === null) return '–';
  return share === 0 && c.limited + c.failed > 0 ? '<1%' : `${share}%`;
}
/** Coding agents' display names by the id the proxy or a session record gives them. */
const AGENT_NAMES: Record<string, string> = { 'claude-code': 'Claude Code', codex: 'Codex CLI', opencode: 'OpenCode', hermes: 'Hermes Agent', kilo: 'Kilo Code', goose: 'Goose', aider: 'Aider', crush: 'Crush', 'qwen-code': 'Qwen Code', kimi: 'Kimi Code', 'kimi-code': 'Kimi Code', pi: 'Pi' };
/** A coding agent's name as the proxy recorded it (from its user-agent). */
export function statAgentName(agent: string): string {
  const names: Record<string, string> = { ...AGENT_NAMES, 'anthropic-sdk': t('Anthropic SDK'), 'openai-sdk': t('OpenAI SDK') };
  return names[agent] ?? t('Other client');
}
/**
 * SB-201: the agent a launched session runs, by its display name: the record's catalog `agent`
 * (an agent through the proxy records the route's provider, `claude`), else its provider's own
 * CLI for an older record; an agent without a known name shows its id.
 */
export function sessionAgentName(session: { agent?: string | null; provider: string }): string {
  const id = session.agent ?? (session.provider === 'claude' ? 'claude-code' : session.provider);
  return AGENT_NAMES[id] ?? id;
}

/** SB-147: the share of input tokens read from the cache, as a whole percent; null with no input. */
export function statCacheShare(c: StatCounts): number | null {
  const input = c.input_tokens + c.cache_read_tokens + c.cache_write_tokens;
  return input ? Math.round((c.cache_read_tokens * 100) / input) : null;
}
/** SB-147: tokens per request; null with no requests. */
export function statTokensPerRequest(c: StatCounts): number | null {
  return c.requests ? Math.round(statTokens(c) / c.requests) : null;
}
export type ChartMetric = 'tokens' | 'requests';
export interface ChartSeries { key: string; name: string; values: number[] }
/**
 * SB-147: the daily chart: every day from `from` to `to` (a day without traffic is 0), one series
 * per group, the `top` busiest by name and the rest summed as "Other groups".
 */
export function chartSeries(stats: Pick<Statistics, 'from' | 'to' | 'by_day'>, metric: ChartMetric, names: (pool: string) => string, top = 5): { days: string[]; series: ChartSeries[] } {
  const days: string[] = [];
  const start = Date.parse(`${stats.from}T00:00:00Z`); const end = Date.parse(`${stats.to}T00:00:00Z`);
  if (Number.isFinite(start) && Number.isFinite(end)) for (let t = start; t <= end && days.length < 400; t += 86_400_000) days.push(new Date(t).toISOString().slice(0, 10));
  const index = new Map(days.map((d, i) => [d, i]));
  const totals = new Map<string, number>(); const perPool = new Map<string, number[]>();
  for (const row of stats.by_day) {
    const i = index.get(row.day); if (i === undefined) continue;
    for (const g of row.groups ?? []) {
      const v = metric === 'tokens' ? g.tokens : g.requests;
      totals.set(g.pool, (totals.get(g.pool) ?? 0) + v);
      const values = perPool.get(g.pool) ?? new Array(days.length).fill(0); values[i] += v; perPool.set(g.pool, values);
    }
  }
  const ranked = [...totals.entries()].filter(([, v]) => v > 0).sort((a, b) => b[1] - a[1]).map(([pool]) => pool);
  const series: ChartSeries[] = ranked.slice(0, top).map((pool) => ({ key: pool, name: names(pool), values: perPool.get(pool)! }));
  const rest = ranked.slice(top);
  if (rest.length) series.push({ key: '', name: t('Other groups'), values: days.map((_, i) => rest.reduce((sum, pool) => sum + perPool.get(pool)![i], 0)) });
  return { days, series };
}
/** A round axis maximum at or above `n`: 1, 2 or 5 times a power of ten. */
export function niceMax(n: number): number {
  if (n <= 0) return 1;
  const p = 10 ** Math.floor(Math.log10(n));
  for (const m of [1, 2, 5, 10]) if (m * p >= n) return m * p;
  return 10 * p;
}
/**
 * SB-147: a group's folders by project: one entry per repository (its folders, worktrees and
 * clones of one `origin`) or per folder outside Git, in the order the folders are listed.
 */
export function groupProjectRows(folders: string[], roots: ProjectRoot[] | null): { key: string; name: string; rows: FolderRow[] }[] {
  const byRoot = new Map((roots ?? []).map((r) => [r.folder, r]));
  const out: { key: string; name: string; rows: FolderRow[] }[] = [];
  let current: { key: string; name: string; rows: FolderRow[] } | null = null;
  for (const row of folderRows(folders, roots, true)) {
    if (row.depth === 1 && current) { current.rows.push(row); continue; }
    const root = byRoot.get(row.folder);
    // A subfolder of a listed folder belongs to that folder's project, never a project of its own.
    const holder = out.find((p) => p.rows.some((r) => r.depth === 0 && (row.folder.startsWith(`${r.folder}/`) || row.folder.startsWith(`${r.folder}\\`))));
    if (holder) { holder.rows.push({ ...row, depth: 1 }); current = holder; continue; }
    const key = root?.remote ?? row.folder;
    const found = out.find((p) => p.key === key);
    if (found) { found.rows.push(row); current = found; continue; }
    current = { key, name: projectNameOf(row, root), rows: [row] };
    out.push(current);
  }
  return uniqueNames(out);
}

/** SB-153: a statistics table's row: its name cells (text, or text with a note) and its counts. */
export interface StatRow { cells: (string | [string, string])[]; counts: StatCounts }
export interface StatSort { column: number; descending: boolean }
/**
 * SB-153: rows sorted by one column. Columns are the name cells first, then the counts:
 * compact tables show requests, tokens, limits and errors; full ones requests, sent, from cache,
 * received, limits and errors. Text sorts by the visible name, counts by the number; equal
 * rows keep their order (the busiest first). No sort keeps the order given.
 */
export function sortStatRows(rows: StatRow[], sort: StatSort | null, compact: boolean): StatRow[] {
  if (!sort) return rows;
  const names = rows[0]?.cells.length ?? 0;
  const text = (r: StatRow) => { const c = r.cells[sort.column]; return typeof c === 'string' ? c : c?.[0] ?? ''; };
  const counts: ((c: StatCounts) => number)[] = compact
    ? [(c) => c.requests, statTokens, (c) => c.limited + c.failed]
    : [(c) => c.requests, (c) => c.input_tokens + c.cache_write_tokens, (c) => c.cache_read_tokens, (c) => c.output_tokens, (c) => c.limited + c.failed];
  const pick = sort.column < names ? null : counts[sort.column - names];
  const indexed = rows.map((row, i) => ({ row, i }));
  indexed.sort((a, b) => {
    const order = pick ? pick(a.row.counts) - pick(b.row.counts) : text(a.row).localeCompare(text(b.row), undefined, { numeric: true, sensitivity: 'base' });
    return (sort.descending ? -order : order) || a.i - b.i;
  });
  return indexed.map((x) => x.row);
}
/** SB-153: the next sort after a click on a column: a new column starts descending for a count and ascending for a name; the same column flips. */
export function nextStatSort(current: StatSort | null, column: number, isCount: boolean): StatSort {
  if (current && current.column === column) return { column, descending: !current.descending };
  return { column, descending: isCount };
}

/**
 * VIS-03: the pool field's `pattern`. WebKit compiles `pattern` with the `v` flag, where a bare `-`
 * inside a class is a syntax error and the whole check is silently dropped; the hyphen is escaped.
 */
export const POOL_PATTERN = '[a-z0-9_\\-]+';
/** D-1, D-5: a pool as a label: its group's name, the default group, or a group without folders, never a bare id. */
export function poolName(pool: string, projects: { pool: string; name: string }[] = []): string {
  if (pool === 'default') return t('Default group');
  return projects.find((p) => p.pool === pool)?.name ?? t('Group {id} · no folders', { id: pool });
}
/** The same pool after "in", for sentences (Russian takes the locative: «в группе по умолчанию»). */
export function inGroup(pool: string, projects: { pool: string; name: string }[] = []): string {
  if (pool === 'default') return t('in the default group');
  const name = projects.find((p) => p.pool === pool)?.name;
  return name !== undefined ? t('in group “{name}”', { name }) : t('in group {id} (no folders)', { id: pool });
}
export interface PoolChoice { pool: string; name: string; group: boolean }
/**
 * UX-17: the pools a group field offers: Default first, then each group by name, then any other pool
 * an account already uses (and `current`), so a typo never makes a pool nothing else uses.
 */
export function poolChoices(accounts: Pick<Account, 'pool'>[], projects: { pool: string; name: string }[], current?: string): PoolChoice[] {
  const groups = [...projects].sort((a, b) => a.name.localeCompare(b.name)).map((p) => ({ pool: p.pool, name: p.name, group: true }));
  const known = new Set(['default', ...groups.map((g) => g.pool)]);
  const others = [...new Set([...accounts.map((a) => a.pool), ...(current ? [current] : [])])].filter((pool) => !known.has(pool)).sort().map((pool) => ({ pool, name: pool, group: false }));
  return [{ pool: 'default', name: t('Default group'), group: false }, ...groups, ...others];
}
export interface Rect { left: number; top: number; width: number; height: number }
/**
 * VIS-12: where the tour card goes: below its target, else above, else beside it; always inside the
 * window and never over the target. A step without a target keeps the bottom-right corner.
 */
export function tourPlacement(target: Rect | null, card: { width: number; height: number }, viewport: { width: number; height: number }, gap = 12): { left: number; top: number; side: 'below' | 'above' | 'left' | 'right' | 'corner' } {
  const clampX = (x: number) => Math.max(gap, Math.min(x, viewport.width - card.width - gap));
  const clampY = (y: number) => Math.max(gap, Math.min(y, viewport.height - card.height - gap));
  const corner = { left: clampX(viewport.width - card.width - 24), top: clampY(viewport.height - card.height - 24), side: 'corner' as const };
  if (!target) return corner;
  const below = target.top + target.height + gap;
  if (below + card.height <= viewport.height - gap) return { left: clampX(target.left), top: below, side: 'below' };
  const above = target.top - gap - card.height;
  if (above >= gap) return { left: clampX(target.left), top: above, side: 'above' };
  const right = target.left + target.width + gap;
  if (right + card.width <= viewport.width - gap) return { left: right, top: clampY(target.top), side: 'right' };
  const left = target.left - gap - card.width;
  if (left >= gap) return { left, top: clampY(target.top), side: 'left' };
  return corner;
}
/**
 * UX-18: what the empty Accounts page leads with. `add_current` only while a CLI is signed in to an
 * account (its In use now card then offers Add to Switchboard); `sign_in` when neither is.
 */
export function firstRunHint(current: { claude?: { status: string } | null; codex?: { status: string } | null } | null): 'reading' | 'add_current' | 'sign_in' {
  if (!current) return 'reading';
  return current.claude?.status === 'available' || current.codex?.status === 'available' ? 'add_current' : 'sign_in';
}
/**
 * UX-24: controls that change nothing stay usable while an action runs: navigation, Refresh, view
 * toggles, sorting, the tour, and retries of a read. Every other control waits for the action.
 */
export function usableWhileBusy(key: string): boolean {
  return /^(nav-|refresh$|dismiss-notice$|countdown-toggle$|org-filter-|quota-|chart-|stats-|stat-[a-z]+-sort-|folders-|disk-map-toggle$|review-rules$|open-api-keys$|tour-|[a-z-]+-retry$)/.test(key);
}

/** SB-183 (node A-7): one start file's line in *Terminals in organization folders*; `command` is
 * the line a person adds by hand (with *Copy*) when Switchboard could not write the file. */
export interface ShellHookFileView { text: string; command: string | null; problem: boolean }
export interface ShellHookView {
  state: 'reading' | 'failed' | 'on' | 'default_on' | 'off' | 'default_off' | 'inactive';
  line: string;
  files: ShellHookFileView[];
  /** The switch: Turn off while the hook is wanted, Turn on otherwise; disabled outside the installed app. */
  action: { label: string; on: boolean; disabled: boolean } | null;
  /** A block the person took out while the hook is on: *Add the block again* writes it back. */
  readd: boolean;
  note: string | null;
  error: string | null;
  retry: boolean;
}
/** The line a person adds to a start file Switchboard left alone. */
export function shellInitLine(shell: string): string | null {
  if (shell === 'zsh' || shell === 'bash') return `eval "$(switchboard shell init ${shell})"`;
  if (shell === 'fish') return 'switchboard shell init fish | source';
  if (shell === 'pwsh') return 'switchboard shell init pwsh | Out-String | Invoke-Expression';
  return null;
}
/** A start file's path as the person knows it: `~/...` under their home. */
export function homePath(path: string, home: string | null): string {
  if (home && (path === home || path.startsWith(home.endsWith('/') ? home : `${home}/`))) return `~${path.slice(home.replace(/\/$/, '').length)}`;
  return path;
}
function shellHookFile(file: { shell: string; path: string; state: string }, home: string | null): ShellHookFileView {
  const path = homePath(file.path, home);
  const shell = file.shell;
  switch (file.state) {
    case 'written': return { text: shell === 'fish' ? t('fish: its own file, added') : t('{shell}: {path}, added', { shell, path }), command: null, problem: false };
    case 'skipped_symlink': return { text: t('{path} is a link, so Switchboard did not change it. Add this line yourself:', { path }), command: shellInitLine(shell), problem: true };
    case 'skipped_zdotdir_unknown': return { text: t('zsh may read its start file from another folder (ZDOTDIR), so Switchboard created none. Add this line to the start file zsh reads:'), command: shellInitLine('zsh'), problem: true };
    case 'skipped_owner': return { text: t('{path} belongs to another user, so Switchboard did not change it.', { path }), command: null, problem: true };
    case 'unreadable': return { text: t('{path} could not be read, so Switchboard did not change it.', { path }), command: null, problem: true };
    case 'removed_by_person': return { text: t('You took the block out of {path}, so Switchboard does not add it back.', { path }), command: null, problem: true };
    case 'policy_blocked': return { text: t('PowerShell\'s execution policy stops profiles from loading, so the hook is inactive there. Switchboard does not change it. Whoever manages this computer can allow signed or local scripts for your user.'), command: null, problem: true };
    default: return { text: t('{shell}: {path}, {state}', { shell, path, state: file.state }), command: null, problem: true };
  }
}
/** What the Groups page's *Terminals in organization folders* section shows for a read (null while
 * reading) and the last read's or save's error (SCN-075). */
export function shellHookView(status: ShellHookStatus | null, error: string | null = null): ShellHookView {
  const none = { files: [], action: null, readd: false, note: null };
  if (!status) {
    if (error) return { ...none, state: 'failed', line: t('The terminal hook\'s state could not be read.'), error, retry: true };
    return { ...none, state: 'reading', line: t('Reading the terminal hook…'), error: null, retry: false };
  }
  const wanted = status.hook === 'on' || status.hook === 'default_on';
  const files = status.files.map((f) => shellHookFile(f, status.home));
  const active = status.files.some((f) => f.state === 'written');
  const orgs = status.organizations;
  let state: ShellHookView['state'] = status.hook;
  let line: string;
  if (wanted && !active) { state = 'inactive'; line = t('On, but no start file holds the hook yet, so terminals do not follow organization folders.'); }
  else if (status.hook === 'default_on') line = orgs.length === 1 ? t('On: new terminals in {organization}\'s folders work in its context.', { organization: orgs[0] }) : t('On: new terminals in organization folders work in their context.');
  else if (status.hook === 'on') line = t('On: new terminals in organization folders work in their context.');
  else if (status.hook === 'off') line = t('Off: new terminals keep your own settings.');
  else line = t('Off: no group belongs to an organization on this computer.');
  const action = { label: wanted ? t('Turn off') : t('Turn on'), on: !wanted, disabled: !status.can_change };
  const readd = wanted && status.can_change && status.files.some((f) => f.state === 'removed_by_person');
  const note = status.can_change ? null : t('Only the installed app changes terminal settings.');
  const map = status.map === 'ok' || status.map === 'missing' ? null : status.map === 'unknown_version' ? t('The folder map was written by another version of Switchboard, so no folder applies until it writes it again.') : t('The folder map could not be read, so no folder applies.');
  return { state, line, files, action, readd, note, error: error ?? map, retry: false };
}
/** The notice after *Turn off* / *Turn on* (SCN-075 steps 2 and 3). */
export function shellHookToggleNotice(on: boolean): string {
  return on ? t('The terminal hook is on. New terminals in organization folders work in their context.') : t('The terminal hook is off. New terminals keep your own settings; terminals already open switch back at their next change of folder.');
}
/** The first-time notice (SCN-074 step 1): the same words the system notification carries. */
export function shellHookNoticeText(notice: ShellHookNotice): { title: string; body: string } {
  return {
    title: t('Terminals now work in {organization}\'s context in its folders', { organization: notice.organization }),
    body: t('New terminals opened in {group}\'s folders run claude and codex on {organization}\'s accounts and history. Turn this off under Groups.', { group: notice.group, organization: notice.organization }),
  };
}
/** SCN-066: under *Codex in group folders*, while the terminal hook is on in an organization's folders.
 * Only organizations whose folder map rows carry a Codex account (`codex_organizations`): elsewhere `codex` keeps the person's sign-in. */
export function codexHookLine(status: ShellHookStatus | null): string | null {
  const orgs = status?.codex_organizations ?? [];
  if (!status || !(status.hook === 'on' || status.hook === 'default_on') || !status.files.some((f) => f.state === 'written') || !orgs.length) return null;
  return orgs.length === 1 ? t('In {organization}\'s folders, new terminals use its Codex accounts whether this is on or off.', { organization: orgs[0] }) : t('In organization folders, new terminals use their Codex accounts whether this is on or off.');
}
