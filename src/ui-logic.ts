// Pure interface rules, kept free of DOM access so scripts/test-ui-logic.mjs can
// exercise them directly. main.ts owns rendering; these functions own decisions.
import type { Account, ProjectRule, Usage } from './types';

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
