import './tokens.css';
import './style.css';
import './window.css';
import switchboardMark from '../brand/passioncode/switchboard-mark.svg';
import { version } from '../package.json';
import { platformLabel } from './platform';
import { LOCALE_KEY, dateLocale, locale, parseLocaleChoice, plural, saveLocaleChoice, t, type LocaleChoice } from './i18n';
import { demo, native, nativeAdapter, onProjectSuggestions, onShellHook, onSpendChanged, safeError, reportFrontendReady, reportLanguage, type Operation } from './adapter';
import type { CardState } from './ui-logic';
import { statAlert, ceilingQuestion, ceilingWarning, ceilingLine, keyLimitLine, dollars, POOL_PATTERN, poolChoices, poolName, inGroup, tourPlacement, usableWhileBusy, APPEARANCE_KEY, attribution, groupAdmits, groupSeatsElsewhere, passioncodeLine, seatEndedText, groupSeatsEndedText, organizationReleasedAt, inOrganizationFilter, domainList, type AttributedBy, sortStatRows, nextStatSort, type StatRow, type StatSort, statCacheShare, statTokensPerRequest, chartSeries, niceMax, groupProjectRows, type ChartMetric, groupProjects, compactCount, statTokens, statTrouble, statTroubleText, statAgentName, type Statistics, EXPIRY_CHOICES, folderName, folderRows, withoutFolder, type ProjectRoot, moveTargets, mapRows, rootTargets, storedMapFolders, MAP_FOLDERS_KEY, type MapFilter, type MapRow, type RepositoryMap, sortAgents, defaultAgentChoices, modelMeta, ordinaryKimiNote, kimiLimitUntil, limitReasonText, limitTitle, agentSetupFacts, sessionHow, sessionAgentName, sessionStateLabel, kimiWindowsLine, kimiWindowsLines, openrouterCreditLine, eventAction, eventDetail, MutationClock, activeRules, autoSwitchPool, launchChoice, withFolder, replaceFolder, storedFolder, LAST_FOLDER_KEY, canProbe, canSwitchNative, accountReset, accountUsedPercent, cardState, compactCountdown, expiryFrom, failedNextCheck, featureWindowLabel, groupAccounts, isFeatureWindow, limitLabel, intervalWhile, loginOutcome, updateLine, monitorChecks, parseAppearance, primaryAction, projectName, quotaMaxAge, quotaOrder, resetCountdown, resolveTheme, ruleState, signInNotice, signInMove, organizationMoveNote, dismissTermsNotice, termsNoticeDismissed, termsNoticeOn, termsNoticeText, tourWaits, orgSetups, orgSetupAction, orgSetupShown, orgSetupStateLabel, orgSetupTitle, hiddenOrgSetups, ORG_SETUP_HIDDEN_KEY, accountsEmptyKey, type OrgSetup, usageAnswerText, usageQuestion, usageQuestionText, usageFreshness, windowReset, type Appearance, shellHookView, shellHookToggleNotice, shellHookNoticeText, codexHookLine, moveStep, accountsForStep, chainSummary, stepReasonText, handoverOutcomeText, handoverSkipped, shortWorkflow, TAKEOVER_ORDER } from './ui-logic';
import agentCatalog from '../catalog/agents.json';
import type { AnalyticsStatus, Ceilings, CeilingView, CeilingQuestionView, OpenrouterCredit, BackupInfo, PassioncodeStatus, Account, Adapter, AgentConnection, AgentInfo, AgentRow, AgentsOverview, OpenrouterModels, HermesModel, KimiAccounts, KimiRegion, KimiStatus, OpenrouterStatus, OpenrouterKeys, CodexRoute, ShellHookStatus, AgentSetup, AuthKind, BackupStatus, KimiAccount, CurrentAccounts, LoginItem, Project, Restored, ExternalIdentity, MonitorStatus, ProjectRule, Provider, RotationPolicy, RuntimeStatus, SessionInfo, Snapshot, UpdateStatus, Onboarding, Chains, ChainExecutor, ChainScope, ChainStepView } from './types';

const root = document.querySelector<HTMLDivElement>('#app')!;
const announcements = document.querySelector<HTMLDivElement>('#announcements')!;
// Appearance: System follows prefers-color-scheme: light; Dark/Light are explicit.
// Storage is a per-machine convenience; an unavailable store falls back to System.
const systemLight = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: light)') : null;
let appearance: Appearance = 'system';
let appearanceSaveFailed = false;
try { appearance = parseAppearance(localStorage.getItem(APPEARANCE_KEY)); } catch { appearance = 'system'; }
function applyTheme() { document.documentElement.dataset.theme = resolveTheme(appearance, !!systemLight?.matches); }
applyTheme();
systemLight?.addEventListener('change', applyTheme);
function setAppearance(value: Appearance) {
  appearance = value; applyTheme();
  try { localStorage.setItem(APPEARANCE_KEY, value); appearanceSaveFailed = false; } catch { appearanceSaveFailed = true; }
}
const clock = new MutationClock();
let lastRendered = '';
let dialogSequence = 0;
const REPOSITORY = 'https://github.com/passioncode-ai/fabric-switchboard';
let adapter: Adapter = nativeAdapter;
let snapshot: Snapshot | null = null;
let runtime: RuntimeStatus | null = null;
let currentAccounts: CurrentAccounts | null = null;
let monitor: MonitorStatus | null = null;
type Page = 'accounts' | 'tokens' | 'projects' | 'orgs' | 'agents' | 'activity' | 'stats' | 'about';
let page: Page = 'accounts';
/** First-run tour (operator request 2026-10-05): five steps, each on its screen, pointing at what to press. */
const TOUR_KEY = 'switchboard.tour';
const onWindows = () => runtime?.platform === 'windows';
/** SB-104: the tray icon sits in the notification area on Windows and Linux, in the menu bar on macOS. */
const trayInArea = () => runtime?.platform === 'windows' || runtime?.platform === 'linux';
const onLinux = () => runtime?.platform === 'linux';
const tour = (): { page: Page; target: string[]; title: string; text: string }[] => [
  { page: 'accounts', target: [], title: t('Welcome to Switchboard'), text: t('Switchboard keeps your Claude Code and Codex accounts on this computer and moves your work between them, so you can keep working on another account when one reaches its limit.') },
  { page: 'accounts', target: ['[data-focus="menu-add"]'], title: t('Add your accounts'), text: t('Press + Add account to sign in to another account. To save the account Claude Code already uses, press Add to Switchboard under In use now. Each row then shows how much is used and when it resets.') },
  { page: 'accounts', target: ['.rotation-bar'], title: t('Switch, by hand or automatically'), text: t('Switch moves Claude Code to another account. Turn on automatic switching, and Switchboard moves off an account that nears its limit to the one with the most left.') },
  { page: 'projects', target: ['[data-focus="new-project"]', '[data-focus="empty-new-project"]'], title: t('Give a group its own accounts'), text: t('Press + New group, add the group\'s repositories and choose its accounts. Sessions launched from those folders use only them, and other groups never switch to them.') },
  { page: 'agents', target: ['[data-focus="copy-onboard"]', '#connect-panel > h2'], title: t('Agents, and it keeps running'), text: trayInArea() ? t('Here, give your coding agent the setup prompt: it registers the tools and sets up the rest, leaving you only the sign-in. Closing the window keeps Switchboard working; open or quit it from its icon in the notification area.') : t('Here, give your coding agent the setup prompt: it registers the tools and sets up the rest, leaving you only the sign-in. Closing the window keeps Switchboard working; open or quit it from the menu-bar icon.') },
];
// Storage refused: the tour shows again next start rather than never (SCN-035).
let tourStep: number | null = (() => { try { return localStorage.getItem(TOUR_KEY) ? null : 0; } catch { return 0; } })();
function tourGo(step: number | null) {
  tourStep = step;
  if (step === null) { try { localStorage.setItem(TOUR_KEY, '1'); } catch { /* the tour shows again next start */ } render(); restoreFocus('page-title'); return; }
  page = tour()[step].page; if (page === 'agents' && !agentsView) void loadAgents(); render(); restoreFocus('tour-title'); placeTour(true);
}
/** The tour waits for the usage question (issue #128): one first-run card at a time, the question first. */
function tourShown(): boolean { return tourStep !== null && (native || demo) && !tourWaits(analyticsState, analyticsError); }
/** The first selector of a step that is on the page (UX-30: the setup prompt may still be reading). */
function tourTarget(scope: ParentNode, step: number): HTMLElement | null {
  for (const selector of tour()[step].target) { const found = scope.querySelector<HTMLElement>(selector); if (found) return found; }
  return null;
}
/** The step and selector the card was last placed beside: a target that appears late (Agents loads after its step shows) is scrolled to once. */
let tourFound: { step: number; selector: string } | null = null;
/** VIS-12: the card goes beside its target, never over it, and the target is scrolled into sight first. */
function placeTour(scroll = false) {
  const card = root.querySelector<HTMLElement>('.tour-card'); if (!card || tourStep === null) return;
  const target = tourTarget(root, tourStep); const selector = target ? tour()[tourStep].target.find((s) => target.matches(s)) ?? '' : '';
  const viewport = { width: window.innerWidth, height: window.innerHeight }; const size = { width: card.offsetWidth, height: card.offsetHeight };
  const measure = () => { const r = target?.getBoundingClientRect(); return r ? { left: r.left, top: r.top, width: r.width, height: r.height } : null; };
  // Centred first; when the card fits neither above nor below, the target goes to the top so the card fits under it.
  // Scrolling happens when the step changes, when its target first appears and when the target lies off screen;
  // never otherwise on a background render (unit tier G, round 2).
  const box = measure();
  const firstFound = target !== null && (tourFound?.step !== tourStep || tourFound.selector !== selector);
  const offScreen = box !== null && (box.top + box.height < 0 || box.top > viewport.height);
  const move = target !== null && (scroll || firstFound || offScreen);
  if (target && move) target.scrollIntoView({ block: 'center', inline: 'nearest' });
  let at = tourPlacement(measure(), size, viewport);
  if (target && move && at.side === 'corner') { target.scrollIntoView({ block: 'start', inline: 'nearest' }); at = tourPlacement(measure(), size, viewport); }
  tourFound = target ? { step: tourStep, selector } : null;
  card.classList.add('is-placed'); card.style.left = `${at.left}px`; card.style.top = `${at.top}px`; card.dataset.side = at.side;
}
window.addEventListener('resize', () => placeTour());
function tourCard(step: number) {
  const item = tour()[step]; const last = step === tour().length - 1;
  const card = el('section', 'tour-card'); card.setAttribute('role', 'dialog'); card.setAttribute('aria-modal', 'false'); card.setAttribute('aria-labelledby', 'tour-title');
  const count = el('p', 'eyebrow', t('Step {step} of {total}', { step: step + 1, total: tour().length }));
  const title = el('h2', '', item.title); title.id = 'tour-title'; title.tabIndex = -1; title.dataset.focus = 'tour-title';
  const dots = el('div', 'tour-dots'); dots.setAttribute('aria-hidden', 'true'); tour().forEach((_, i) => dots.append(el('span', i === step ? 'is-on' : '')));
  const actions = el('div', 'tour-actions');
  actions.append(button(t('Skip tour'), () => tourGo(null), 'text-button', 'tour-skip'));
  if (step > 0) actions.append(button(t('Back'), () => tourGo(step - 1), 'button', 'tour-back'));
  actions.append(button(last ? t('Start using Switchboard') : t('Next'), () => tourGo(last ? null : step + 1), 'button primary', 'tour-next'));
  card.append(count, title, el('p', '', item.text), dots, actions);
  return card;
}
/** Provider terms (operator decision 2026-10-08, SCN-049): a banner on Accounts until dismissed, always in About. */
const noticeStore = () => { try { return localStorage; } catch { return null; } };
/**
 * Usage counts (issue #128, operator decision 2026-10-08; docs/ANALYTICS.md, SCN-073): on by
 * default, but nothing is sent until the person answers once, here or in About. Both show what is
 * counted and the same switch; the choice waits in `usageChoice` until Continue.
 */
let usageChoice = true;
const USAGE_COUNTED = () => t('Switchboard counts installs, days of use and how many accounts are saved, by provider and type, to help PassionCode.ai improve its apps. It never sends account names, e-mail addresses, sign-ins, group names or what you do with your accounts. A random installation number, shared by the PassionCode.ai tools on this computer, lets one person using several of them count once.');
function usageSwitch(focus: string) {
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = usageChoice; box.dataset.focus = focus;
  box.addEventListener('change', () => { usageChoice = box.checked; });
  option.append(box, el('span', '', t('Share anonymous usage counts')));
  return option;
}
/** `shown` is the shared switch the question was drawn with: an "on" given after another tool changed it since is not written back (round 2). */
function answerUsage(focus: string, shown: boolean) {
  const wanted = usageChoice;
  // The answer reports what is now true: an "on" after another tool turned the switch off stays off (round 1).
  void mutate(async () => { analyticsState = await adapter.setAnalytics(wanted, shown); }, () => usageAnswerText(wanted, analyticsState), focus);
}
/** SB-202 (B-8, SCN-076): "Set up {organization} on this computer", worded from the checklist the runtime computed. */
function orgSetupCard(setup: OrgSetup) {
  const id = `org-setup-${setup.organization.id}`;
  const card = el('section', 'org-setup'); card.setAttribute('aria-labelledby', `${id}-title`);
  const title = el('h2', '', orgSetupTitle(setup)); title.id = `${id}-title`;
  const list = el('ol', 'org-setup-steps');
  for (const line of setup.lines) {
    const item = el('li', `org-setup-step is-${line.state}`);
    const text = el('div', 'org-setup-text'); text.append(el('strong', '', line.title), el('span', '', line.line));
    item.append(el('span', `org-setup-state is-${line.state}`, orgSetupStateLabel(line.state)), text); list.append(item);
  }
  card.append(title, list);
  if (onboardingError) card.append(el('p', 'form-note org-setup-error', t('The setup could not be read. Refresh to retry.')));
  const actions = el('div', 'org-setup-actions');
  const action = orgSetupAction(setup);
  if (action) actions.append(action.kind === 'group' ? button(action.label, () => projectDialog(undefined, action.organization), 'button primary', `${id}-group`) : button(action.label, () => void startLogin('claude', '', action.pool), 'button primary', `${id}-sign-in`));
  if (page === 'accounts') actions.append(button(t('Hide'), () => { orgSetupHidden = [...orgSetupHidden, setup.organization.id]; try { noticeStore()?.setItem(ORG_SETUP_HIDDEN_KEY, JSON.stringify(orgSetupHidden)); } catch { /* hidden for this window only */ } render(); restoreFocus('page-title'); }, 'text-button', `${id}-hide`));
  if (actions.childElementCount) card.append(actions);
  return card;
}
function usageQuestionCard() {
  const card = el('section', 'usage-question'); card.setAttribute('aria-labelledby', 'usage-question-title');
  const title = el('h2', '', t('Share anonymous usage counts?')); title.id = 'usage-question-title';
  const actions = el('div', 'usage-question-actions'); const shown = !!analyticsState?.enabled;
  actions.append(usageSwitch('usage-switch'), button(t('Continue'), () => answerUsage('page-title', shown), 'button primary', 'usage-continue'));
  card.append(title, el('p', '', USAGE_COUNTED()), el('p', 'usage-question-sent', usageQuestionText(analyticsState)), actions, el('p', 'form-note', t('This switch applies to every PassionCode.ai tool on this computer. You can change it later in About.')));
  return card;
}
let termsDismissed = termsNoticeDismissed(noticeStore());
function termsBanner(main: HTMLElement) {
  if (termsNoticeOn(page, termsDismissed) !== 'banner') return;
  const banner = el('div', 'status-item terms-notice'); banner.setAttribute('role', 'note'); banner.setAttribute('aria-label', t('Provider terms'));
  banner.append(el('span', '', termsNoticeText()), button(t('Dismiss'), () => { dismissTermsNotice(noticeStore()); termsDismissed = true; render(); restoreFocus('page-title'); }, 'text-button', 'dismiss-terms'));
  main.append(banner);
}
function termsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'terms-heading');
  const heading = el('h2', '', t('Provider terms')); heading.id = 'terms-heading';
  panel.append(heading, el('p', '', termsNoticeText()));
  return panel;
}
let agentSetup: AgentSetup | null = null;
let agentSetupError = false;
/** The OpenRouter key for agents (SB-79); null while it is read. */
let openrouter: OpenrouterStatus | null = null;
let openrouterError = '';
/** SB-144: the machine's key and each project's own key (metadata only). */
let openrouterKeys: OpenrouterKeys | null = null;
/** The ordinary Hermes's model and provider (SB-80); null while it is read. */
let hermes: HermesModel | null = null;
let hermesError = '';
/** Kimi Code subscription accounts (SB-81); null while they are read. */
let kimi: KimiAccounts | null = null;
let kimiError = '';
let openMenu: string | null = null;
/** The label and pool a sign-in started with travel with it, so Try again repeats the same sign-in (SB-62). */
/** SB-104: one Try again at a time — a double click never starts two sign-ins. */
let retrying = false;
let pendingLogin: { id: string; provider: Provider; label: string; pool: string; state: 'pending' | 'ended' | 'finishing'; error: string; cancelError?: string } | null = null;
const quotaOpen = new Set<string>();
let backupStatus: BackupStatus | null = null;
let loginItem: LoginItem | null = null;
let loginItemError = false;
let updateState: UpdateStatus | null = null;
let updateError = false;
let analyticsState: AnalyticsStatus | null = null;
/** SB-202 (B-8): the organization checklist (Tauri `onboard`), read only while an organization exists; the last read stays when one fails. */
let onboarding: Onboarding | null = null;
let onboardingError = false;
let orgSetupHidden = hiddenOrgSetups(noticeStore());
let analyticsError = false;
let backupError = false;
let busy = false;
let loading = true;
let loadError = '';
let runtimeError = false;
let notice = '';
let noticeError = false;
/** SB-160 (B-4): the notice's one action (a new seat's *Move it to {group}*); a new notice drops it. */
let noticeAction: { label: string; run: () => void } | null = null;
let lastWorkingDirectory = storedFolder(noticeStore());
/** The folder the next picker opens at; kept for the next start as a convenience only. */
function rememberFolder(folder: string) { lastWorkingDirectory = folder; try { noticeStore()?.setItem(LAST_FOLDER_KEY, folder); } catch { /* remembered for this window only */ } }
const usageErrors = new Map<string, string>();
document.documentElement.lang = locale();
// The tray menu follows the window's language; it starts in the system's.
if (native) void reportLanguage(locale()).catch(() => undefined);
const providerName = (provider: Provider) => provider === 'claude' ? t('Claude Code') : t('Codex CLI');
const kindName = (kind: AuthKind) => ({ api_key: t('API key'), setup_token: t('Setup token'), oauth: t('OAuth') })[kind];
const date = (seconds: number) => new Date(seconds * 1000).toLocaleString(dateLocale(), { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
const age = (seconds: number) => { const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60)); return minutes < 1 ? t('just now') : minutes < 60 ? t('{n}m ago', { n: minutes }) : minutes < 2880 ? t('{n}h ago', { n: Math.floor(minutes / 60) }) : t('{n}d ago', { n: Math.floor(minutes / 1440) }); };
const identityText = (identity?: ExternalIdentity | null) => identity?.email || identity?.account_id || t('Identity not reported');
const currentMatch = (account: Account) => {
  const current = currentAccounts?.[account.provider];
  if (!current || current.status !== 'available') return false;
  if (current.account_id === account.id) return true;
  const identity = account.external_identity;
  return !!identity?.account_id && identity.account_id === current.identity?.account_id && identity.organization_id === current.identity?.organization_id;
};
// The account the ordinary CLI is signed in to cannot join a group (PROJECT_ACCOUNT_IN_CLI): never prefill it.
const inCli = (account: Account) => currentMatch(account) || !!currentAccounts?.[account.provider]?.account_ids?.includes(account.id);
interface Context { current: CurrentAccounts | null; currentFailed: boolean; monitor: MonitorStatus | null }
/** UX-04: a read of the CLIs' sign-ins that failed (or ran out of time) is kept as failed, so In use now offers Retry instead of reading forever. */
let currentReadFailed = false;
async function readContext(): Promise<Context> {
  const [current, status] = await Promise.allSettled([adapter.currentAccounts(), adapter.monitorStatus()]);
  return { current: current.status === 'fulfilled' ? current.value : null, currentFailed: current.status === 'rejected', monitor: status.status === 'fulfilled' ? status.value : null };
}
function applyContext(context: Context) { currentAccounts = context.current; currentReadFailed = context.currentFailed; monitor = context.monitor; }
async function refreshContext() { applyContext(await readContext()); }
/** SB-202 (B-8): the checklist is read again with the account list; a machine without organizations reads nothing. */
function loadOnboarding() {
  if (!(snapshot?.organizations ?? []).length) { onboarding = null; onboardingError = false; return; }
  const stamp = clock.stamp();
  void adapter.onboard().then((value) => { if (clock.accepts(stamp)) { onboarding = value; onboardingError = false; } }, () => { onboardingError = true; }).finally(backgroundRender);
}
/** Background results apply only when no user mutation began after they were requested. */
function backgroundRender() {
  if (busy || loading || openMenu || document.querySelector('dialog')) return;
  const key = (document.activeElement as HTMLElement | null)?.dataset.focus;
  if (render({ background: true })) restoreFocus(key);
}
const selected = (account: Account) => snapshot?.routes[`${account.provider}:${account.pool}`] === account.id;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); node.className = className; if (text) node.textContent = text; return node;
}
function button(text: string, action: () => void, className = '', key = ''): HTMLButtonElement {
  const node = el('button', className, text); node.type = 'button';
  node.disabled = (loading && (key === 'refresh' || !usableWhileBusy(key))) || (busy && !usableWhileBusy(key));
  node.addEventListener('click', action); if (key) node.dataset.focus = key; return node;
}
function announce(text: string) { announcements.textContent = ''; requestAnimationFrame(() => { announcements.textContent = text; }); }
/** Notices may carry a backend's English refusal: it is translated here, where it is shown (L10N-04). */
function showNotice(text: string, error = false) { notice = t(text); noticeError = error; noticeAction = null; announce(notice); }
/** One way to change page: what each page reads on arrival, then focus on its title or on `focusKey`. */
function goTo(target: Page, focusKey?: string) {
  page = target; notice = '';
  if (target === 'activity') void loadSessions(); if (target === 'agents') { void loadAgents(); void loadChains(); void loadCeilings(); } if (target === 'projects') void loadChains(); if (target === 'tokens') { groupKeyStatus = null; void loadOpenrouter(); void loadCeilings(); } if (target === 'stats' && !stats) void loadStats(); if (target === 'orgs') void loadPassioncode();
  render();
  if (focusKey) { restoreFocus(focusKey); document.querySelector<HTMLElement>(`[data-focus="${focusKey}"]`)?.scrollIntoView({ block: 'start' }); }
  else document.querySelector<HTMLElement>('h1')?.focus();
}
function restoreFocus(key?: string) { if (key) document.querySelectorAll<HTMLElement>('[data-focus]').forEach((node) => { if (node.dataset.focus === key) node.focus(); }); }

async function reload() {
  if (!native && !demo) { loading = false; render(); return; }
  loading = true; loadError = ''; render();
  // A Refresh pressed while an action runs may read the list from before it: that list is not applied (UX-24).
  const listStamp = clock.stamp();
  const [data, status] = await Promise.allSettled([adapter.snapshot(), adapter.runtime()]);
  if (data.status === 'fulfilled') { if (clock.accepts(listStamp)) snapshot = data.value; }
  else { loadError = safeError(data.reason, 'read'); announce(t(loadError)); }
  if (status.status === 'fulfilled') { runtime = status.value; runtimeError = false; }
  else { runtime = null; runtimeError = true; }
  loading = false;
  // UX-02, UX-03: Refresh reads statistics and the Codex switch again, so their errors clear.
  codexRouteState = null; codexRouteError = '';
  shellHookState = null; shellHookError = '';
  if (page === 'stats') { stats = null; statsError = false; void loadStats(); }
  render();
  void adapter.agentSetup().then((value) => { agentSetup = value; agentSetupError = false; }, () => { agentSetupError = true; }).finally(backgroundRender);
  loadOnboarding();
  // Unit tier D: these reads apply only when no action began after this Refresh.
  const readStamp = clock.stamp();
  void loadOpenrouter(readStamp);
  void loadKimi(readStamp);
  void loadHermes();
  void loadSessions(readStamp);
  void restorePendingLogin();
  void loadBackups(readStamp);
  if (page === 'agents') void loadAgents();
  if (page === 'agents' || page === 'projects') void loadChains();
  if (page === 'agents' || page === 'tokens') { groupKeyStatus = null; void loadCeilings(); }
  if (page === 'orgs') void loadPassioncode();
  void adapter.analytics().then((value) => { if (clock.accepts(readStamp)) { analyticsState = value; analyticsError = false; } }, () => { analyticsError = true; }).finally(backgroundRender);
  void adapter.loginItem().then((value) => { if (clock.accepts(readStamp)) { loginItem = value; loginItemError = false; } }, () => { loginItemError = true; }).finally(backgroundRender);
  void loadUpdate();
  // OS credential prompts must not hold the entire workbench in its loading state.
  const stamp = clock.stamp();
  void readContext().then((context) => {
    // A mutation that started meanwhile has already read a newer context.
    if (clock.accepts(stamp)) { applyContext(context); backgroundRender(); }
    if (native && data.status === 'fulfilled' && status.status === 'fulfilled' && context.current && context.monitor) {
      void reportFrontendReady().catch(() => { /* Smoke runner owns the deadline. */ });
    }
  });
}
async function mutate(action: () => Promise<unknown>, success: string | (() => string), focusKey?: string, accountId?: string, operation: Operation = 'change') {
  busy = true; notice = ''; clock.begin(); render();
  try {
    await action();
    await refreshContext();
    if (accountId) usageErrors.delete(accountId);
    showNotice(restoredText || (typeof success === 'function' ? success() : success)); restoredText = '';
    try { snapshot = await adapter.snapshot(); loadError = ''; }
    catch { loadError = t('The action completed, but accounts could not be refreshed. Retry loading the account list.'); }
  } catch (error) { if (accountId) { try { snapshot = await adapter.snapshot(); } catch { /* Keep last snapshot. */ } } const text = safeError(error, operation); if (accountId) usageErrors.set(accountId, text); showNotice(text, true); }
  finally { clock.end(); busy = false; render(); restoreFocus(focusKey); loadOnboarding(); }
}

/** Returns false when a background render found nothing to change and left the DOM alone. */
// A pending official sign-in finishes on its own once Terminal reports completion; the poll
// exists only while one waits (lifecycle LC-08).
const loginPoll = intervalWhile(() => { void pollLogin(); }, 1500);
function render(options: { background?: boolean } = {}): boolean {
  loginPoll.sync(pendingLogin?.state === 'pending');
  const openDetails = new Set([...root.querySelectorAll<HTMLDetailsElement>('details[open]')].map((details) => details.querySelector<HTMLElement>('summary')?.dataset.focus));
  const shell = el('div', 'shell');
  const commit = () => {
    withTour();
    shell.querySelectorAll<HTMLDetailsElement>('details').forEach((details) => { details.open = openDetails.has(details.querySelector<HTMLElement>('summary')?.dataset.focus); });
    const markup = shell.outerHTML;
    if (options.background && markup === lastRendered) return false;
    lastRendered = markup; root.replaceChildren(shell); syncCountdown(); if (tourStep !== null) placeTour(); return true;
  };
  const withTour = () => {
    if (tourStep !== null && tourShown()) {
      tourTarget(shell, tourStep)?.classList.add('tour-target');
      shell.append(tourCard(tourStep));
    }
  };
  const sidebar = el('aside', 'sidebar');
  const brand = el('div', 'brand');
  const mark = el('img', 'brand-mark'); mark.src = switchboardMark; mark.alt = ''; mark.width = 40; mark.height = 40;
  const brandName = el('div', 'brand-name'); brandName.append(el('strong', '', t('Switchboard')), el('span', 'brand-family', t('by PassionCode')));
  brand.append(mark, brandName);
  sidebar.append(brand);
  const nav = el('nav', 'navigation'); nav.setAttribute('aria-label', t('Main navigation'));
  const liveRules = activeRules(snapshot?.rules, Date.now() / 1000).length;
  for (const [target, label] of [['accounts', t('Accounts')], ['tokens', t('API keys')], ['projects', t('Groups')], ['orgs', t('Organizations')], ['agents', t('Agents')], ['activity', t('Activity')], ['stats', t('Statistics')], ['about', t('About')]] as const) {
    const control = button('', () => { if (target === 'stats') { stats = null; statsError = false; } goTo(target); }, `nav-item ${page === target ? 'active' : ''}`, `nav-${target}`);
    const symbol = el('span', 'nav-icon'); symbol.setAttribute('aria-hidden', 'true'); symbol.append(navIcon(target));
    control.append(symbol, el('span', '', label));
    if (target === 'projects' && liveRules) { const count = el('span', 'nav-count', String(liveRules)); count.setAttribute('aria-label', plural(liveRules, { one: '{n} active rule', other: '{n} active rules' })); control.append(count); }
    if (page === target) control.setAttribute('aria-current', 'page'); nav.append(control);
  }
  sidebar.append(nav);
  const sidebarFoot = el('div', 'sidebar-foot');
  sidebarFoot.append(el('span', 'eyebrow', t('LOCAL WORKBENCH')), el('p', '', demo ? t('Synthetic session') : t('Your accounts. Your machine.')), el('span', 'version', `v${version} · ${demo ? t('Browser demo') : platformLabel(runtime?.platform)}`));
  sidebar.append(sidebarFoot); shell.append(sidebar);
  const main = el('main', 'main'); main.id = 'main'; main.setAttribute('aria-busy', String(busy || loading));
  if (demo) main.append(el('div', 'demo-banner', t('SYNTHETIC DEMO · No real accounts, vault, proxy, or terminal. Changes reset when you reload.')));
  const header = el('header', 'page-header');
  const heading = el('div'); const title = el('h1', '', { accounts: t('Accounts'), tokens: t('API keys'), projects: t('Groups'), orgs: t('Organizations'), agents: t('Agents'), activity: t('Activity'), stats: t('Statistics'), about: t('About Switchboard') }[page]); title.tabIndex = -1; title.dataset.focus = 'page-title';
  heading.append(el('p', 'eyebrow', t('FABRIC SWITCHBOARD')), title, el('p', 'subtitle', { accounts: t('Switch Claude Code, keep sign-ins fresh and watch quota.'), tokens: t('API keys agents run on: the machine\'s, and one per group.'), projects: t('Groups of repositories with accounts of their own; other groups never switch to them.'), orgs: t('Companies your accounts belong to, by your word, their team at the provider or their address.'), agents: t('Choose which coding agents take over work, and connect them to Switchboard.'), activity: t('Local account and session events.'), stats: t('Which accounts and agents work in which groups, and where tokens go.'), about: t('Deliberate account switching for coding sessions.') }[page]));
  header.append(heading);
  if (native || demo) {
    const actions = el('div', 'header-actions');
    actions.append(button(loading ? t('Loading…') : t('Refresh'), () => void reload(), 'button quiet', 'refresh'));
    // VIS-21, UX-01: while a page shows its empty state, that state carries the one primary action.
    const empty = (list?: unknown[]) => !!snapshot && !(list ?? []).length;
    if (page === 'accounts' && !empty(snapshot?.accounts)) actions.append(addMenu());
    if (page === 'projects' && !empty(snapshot?.projects)) actions.append(button(t('+ New group'), () => projectDialog(), 'button primary', 'new-project'));
    if (page === 'orgs' && !empty(snapshot?.organizations)) actions.append(button(t('+ New organization'), () => organizationDialog(null), 'button primary', 'org-add'));
    header.append(actions);
  }
  main.append(header);
  if (busy) { const progress = el('p', 'operation-progress', t('Working…')); progress.setAttribute('role', 'status'); main.append(progress); }
  if (!native && !demo) {
    main.append(emptyState(t('Open the native app'), t('Account storage and session launches are available in the Fabric Switchboard desktop app. This browser window has no access to your accounts.')));
    shell.append(main); return commit();
  }
  if (notice) { const alert = el('div', `notice toast ${noticeError ? 'error' : 'success'}${tourShown() ? ' beside-tour' : ''}`); alert.append(el('span', '', notice)); if (noticeAction) { const action = noticeAction; alert.append(button(action.label, () => action.run(), 'text-button', 'notice-action')); } alert.append(button(t('Dismiss'), () => { notice = ''; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, 'text-button', 'dismiss-notice')); main.append(alert); }
  loginBanner(main);
  if (page !== 'about' && usageQuestion(analyticsState)) main.append(usageQuestionCard());
  if (snapshot && (page === 'accounts' || page === 'orgs')) for (const setup of orgSetups(onboarding)) if (orgSetupShown({ page, organization: setup.organization.id, hidden: orgSetupHidden, usage: analyticsState, usageFailed: analyticsError, tourStep })) main.append(orgSetupCard(setup));
  if (page === 'about') renderAbout(main);
  else if (page === 'agents') renderAgents(main);
  else if (page === 'tokens') renderTokens(main);
  else if (page === 'stats') renderStats(main);
  else if (loadError && !snapshot) {
    const state = emptyState(page === 'activity' ? t('Unable to load activity') : page === 'projects' ? t('Unable to load groups') : page === 'orgs' ? t('Unable to load organizations') : t('Unable to load accounts'), t(loadError)); state.classList.add('error-state'); state.append(button(t('Retry'), () => void reload(), 'button primary', 'retry-load')); main.append(state);
  } else if (loading && !snapshot) {
    const state = emptyState(t('Loading Switchboard'), t('Reading account metadata from the native app…')); state.setAttribute('role', 'status'); main.append(state);
  } else if (snapshot) {
    // UX-06: an action that worked keeps the last list on screen; only the re-read failed.
    if (loadError) { const stale = el('div', 'notice error stale-list'); stale.setAttribute('role', 'status'); stale.append(el('span', '', t(loadError)), button(t('Retry'), () => void reload(), 'text-button', 'retry-load')); main.append(stale); }
    if (page === 'accounts') renderAccounts(main);
    else if (page === 'projects') renderProjects(main);
    else if (page === 'orgs') renderOrganizations(main);
    else renderActivity(main);
  }
  shell.append(main); return commit();
}
/** VIS-19: one icon set for the sidebar: 16 px, drawn with one 1.5 px stroke in the text colour. */
const NAV_ICONS: Record<Page, string> = {
  accounts: 'M5.5 7a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5ZM1.5 14c0-2.5 1.8-4 4-4s4 1.5 4 4M11 3.5h3.5M11 7h3.5M11 10.5h3.5',
  tokens: 'M6 10a3.5 3.5 0 1 1 2.6-1.2L14.5 14.7M12 12.2l1.4-1.4M10.4 10.6l1.2-1.2M4.2 6.5h.01',
  projects: 'M1.5 4.5V12a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1H8L6.5 3h-4a1 1 0 0 0-1 1.5Z',
  orgs: 'M2.5 14V3.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1V14M9.5 7h3a1 1 0 0 1 1 1v6M1.5 14h13M5 5.5h2M5 8h2M5 10.5h2',
  agents: 'M3 4.5h10a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5.5a1 1 0 0 1 1-1ZM5 7.5l1.5 1.5L5 10.5M8.5 10.5H11M8 4.5V2',
  activity: 'M1.5 8h3l2-5 3 10 2-5h3',
  stats: 'M2 14h12M4 14V9M7 14V5M10 14V8M13 14V3',
  about: 'M8 14.5a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13ZM8 7.2V11M8 5h.01',
};
function navIcon(target: Page) {
  const icon = svg('svg', { viewBox: '0 0 16 16', width: 16, height: 16, fill: 'none', stroke: 'currentColor', 'stroke-width': 1.5, 'stroke-linecap': 'round', 'stroke-linejoin': 'round', class: 'nav-svg' });
  icon.append(svg('path', { d: NAV_ICONS[target] })); return icon;
}
const ruleAccount = (rule: ProjectRule) => snapshot?.accounts.find((account) => account.id === rule.account_id);
const ruleTarget = (rule: ProjectRule) => rule.target === 'managed' ? t('Managed sessions') : t('Claude Code login (all claude sessions)');
/** Active rules stay in sight on Accounts, so an optional rule is never forgotten. */
function rulesStrip(main: HTMLElement) {
  const live = activeRules(snapshot?.rules, Date.now() / 1000);
  if (!live.length) return;
  const strip = el('div', 'status-item rules-strip'); strip.setAttribute('role', 'note'); strip.setAttribute('aria-label', t('Active folder rules'));
  const names = live.map((rule) => `${projectName(rule.path)} → ${ruleAccount(rule)?.label ?? t('missing account')}${rule.expires_at ? t(' until {date}', { date: date(rule.expires_at) }) : ''}`);
  strip.append(el('span', '', plural(live.length, { one: '{n} folder rule is active: {names}. Automatic switching still moves off an exhausted account.', other: '{n} folder rules are active: {names}. Automatic switching still moves off an exhausted account.' }, { names: names.join('; ') })), button(t('Review rules'), () => goTo('projects', 'rules-heading'), 'text-button', 'review-rules'));
  main.append(strip);
}
function renderProjects(main: HTMLElement) {
  main.append(projectsSection(), codexRouteSection(), shellHookSection(), diskMapSection(), rulesSection());
}
/** SB-143: Codex sessions in a project's folders use its Codex accounts, by one switch. */
let codexRouteState: CodexRoute | null = null; let codexRouteLoading = false; let codexRouteError = ''; let codexRouteKey = '';
/** What the section's answer depends on: the groups and where Codex accounts are (UX-03). */
const codexRouteInputs = () => JSON.stringify([(snapshot?.projects ?? []).map((p) => p.pool), (snapshot?.accounts ?? []).filter((a) => a.provider === 'codex').map((a) => a.pool)]);
function loadCodexRoute() {
  if (codexRouteLoading) return;
  codexRouteLoading = true; const key = codexRouteInputs();
  void adapter.codexRoute(null).then((r) => { codexRouteState = r; codexRouteError = ''; codexRouteKey = key; }, (e) => { codexRouteError = t(safeError(e, 'read')); codexRouteKey = key; }).finally(() => { codexRouteLoading = false; render(); });
}
function codexRouteSection() {
  const section = el('section', 'project-section'); section.setAttribute('aria-labelledby', 'codex-route-heading');
  const heading = el('h2', 'section-heading', t('Codex in group folders')); heading.id = 'codex-route-heading';
  section.append(heading, el('p', 'surface-note', t('Codex sessions started in a group\'s folders use that group\'s Codex accounts, like Claude Code sessions do. Switchboard adds one provider to Codex\'s own settings that goes through its local proxy; folders outside groups keep your own Codex sign-in. While Switchboard is not running, Codex uses your own sign-in everywhere.')));
  if (codexRouteState && codexRouteKey !== codexRouteInputs()) loadCodexRoute();
  if (!codexRouteState && !codexRouteError) { loadCodexRoute(); section.append(el('p', 'form-note', t('Reading Codex\'s settings…'))); return section; }
  if (codexRouteError) { section.append(el('p', 'form-error', codexRouteError)); if (!codexRouteState) { section.append(button(t('Retry'), () => { codexRouteError = ''; loadCodexRoute(); render(); }, 'button', 'codex-route-retry')); return section; } }
  const r = codexRouteState;
  if (r) {
    const line = r.on ? (r.applied ? t('On: Codex goes through Switchboard.') : t('On, but not in Codex\'s settings right now.')) : t('Off: Codex uses your own sign-in in every folder.');
    section.append(el('p', 'account-meta', line));
    if (r.projects.length) section.append(el('p', 'form-note', t('Groups with a Codex account: {names}.', { names: r.projects.join(', ') })));
    else section.append(el('p', 'form-note', t('No group has a Codex account yet. Move one into a group first.')));
    const hookLine = codexHookLine(shellHookState); if (hookLine) section.append(el('p', 'form-note', hookLine));
    if (r.conflict) section.append(el('p', 'form-error', t(r.conflict)));
    const act = button(r.on ? t('Turn off') : t('Turn on'), () => void mutate(async () => { codexRouteState = await adapter.codexRoute(!r.on); }, r.on ? t('Codex uses your own sign-in again.') : t('Codex sessions in group folders now use their groups\' Codex accounts. Start a new Codex session to pick it up.'), 'codex-route-toggle'), r.on || !r.projects.length ? 'button' : 'button primary', 'codex-route-toggle');
    act.disabled = !r.on && !!r.conflict;
    section.append(act);
  }
  return section;
}
/** SB-183 (node A-7): whether terminals follow organization folders (SCN-075), beside *Codex in group folders*. */
let shellHookState: ShellHookStatus | null = null; let shellHookLoading = false; let shellHookError = ''; let shellHookKey = '';
/** What the section's answer depends on: the groups, their organizations and folders. */
const shellHookInputs = () => JSON.stringify((snapshot?.projects ?? []).map((p) => [p.pool, p.organization ?? null, p.folders.length]));
function loadShellHook() {
  if (shellHookLoading) return;
  shellHookLoading = true; const key = shellHookInputs(); const stamp = clock.stamp();
  // A read issued before *Turn off* / *Turn on* may predate it: it is not applied over the change.
  void adapter.shellHookStatus().then((s) => { if (clock.accepts(stamp) || !shellHookState) { shellHookState = s; shellHookError = ''; } shellHookKey = key; }, (e) => { if (clock.accepts(stamp) || !shellHookState) shellHookError = t(safeError(e, 'read')); shellHookKey = key; }).finally(() => { shellHookLoading = false; render(); });
}
function setShellHook(on: boolean, focusKey: string) {
  void mutate(async () => { shellHookState = await adapter.setShellHook(on); }, shellHookToggleNotice(on), focusKey);
}
function shellHookSection() {
  const section = el('section', 'project-section'); section.setAttribute('aria-labelledby', 'shell-hook-heading');
  const heading = el('h2', 'section-heading', t('Terminals in organization folders')); heading.id = 'shell-hook-heading';
  section.append(heading, el('p', 'surface-note', t('In the folders of a group that belongs to an organization, new terminals run claude and codex in that organization\'s context: its accounts, history and settings. Leaving the folder gives your own settings back. Switchboard adds one marked block to each shell\'s start file and takes it out byte for byte when you turn this off.')));
  if (shellHookState && shellHookKey !== shellHookInputs()) loadShellHook();
  if (!shellHookState && !shellHookError) loadShellHook();
  const view = shellHookView(shellHookState, shellHookError || null);
  section.append(el('p', view.state === 'reading' ? 'form-note' : 'account-meta', view.line));
  if (view.error) section.append(el('p', 'form-error', view.error));
  if (view.retry) { section.append(button(t('Retry'), () => { shellHookError = ''; loadShellHook(); render(); }, 'button', 'shell-hook-retry')); return section; }
  if (view.files.length) {
    const list = el('ul', 'shell-hook-files'); list.setAttribute('aria-label', t('Start files'));
    view.files.forEach((file, index) => {
      const item = el('li', file.problem ? 'form-note shell-hook-problem' : 'form-note'); item.append(el('span', '', file.text));
      if (file.command) { const command = file.command; const line = el('div', 'policy-row'); line.append(el('code', 'address', command)); const copy = button(t('Copy'), () => copyText(copy, command, t('Terminal hook')), 'text-button', `shell-hook-copy-${index}`); line.append(copy); item.append(line); }
      list.append(item);
    });
    section.append(list);
  }
  if (view.note) section.append(el('p', 'form-note', view.note));
  // *Add the block again* saves the choice `on` as *Turn on* does (A-7 fix cycle 1, N3): said beside it.
  if (view.action && view.readd) section.append(el('p', 'form-note', t('Adding the block again saves your choice to keep the hook on, as Turn on does.')));
  if (view.action) {
    const action = view.action; const controls = el('div', 'org-card-actions');
    const toggle = button(action.label, () => setShellHook(action.on, 'shell-hook-toggle'), 'button', 'shell-hook-toggle'); toggle.disabled = action.disabled; controls.append(toggle);
    if (view.readd) controls.append(button(t('Add the block again'), () => setShellHook(true, 'shell-hook-readd'), 'button', 'shell-hook-readd'));
    section.append(controls);
  }
  return section;
}
/** SCN-074 step 1: the first time the installed app wrote the terminal hook, the window says so once, with the way to Groups; read while the window is visible, then acknowledged. */
let shellNoticeReading = false;
function checkShellHookNotice() {
  if (shellNoticeReading || document.hidden) return;
  shellNoticeReading = true;
  void adapter.shellHookNotice().then((found) => {
    if (!found || document.hidden) return;
    const { title, body } = shellHookNoticeText(found);
    showNotice(`${title}. ${body}`); noticeAction = { label: t('Open Groups'), run: () => { notice = ''; goTo('projects', 'shell-hook-toggle'); } };
    shellHookState = null; render();
    void adapter.acknowledgeShellHookNotice().catch(() => { /* shown again at the next look */ });
  }, () => { /* read again at the next look */ }).finally(() => { shellNoticeReading = false; });
}
/** SB-135 part 2: the Git repositories under the folders the person chose, each with its project or none; one moves or joins in place. Read only when the section is open. */
let mapOpen = false; let mapFilter: MapFilter = 'unassigned'; let diskMap: RepositoryMap | null = null; let mapLoading = false; let mapError = '';
let mapFolders = storedMapFolders(noticeStore());
function saveMapFolders() { try { noticeStore()?.setItem(MAP_FOLDERS_KEY, JSON.stringify(mapFolders)); } catch { /* kept for this window only */ } }
function loadMap() {
  if (mapLoading) return;
  mapLoading = true; mapError = ''; render();
  void adapter.repositoryMap(mapFolders).then((map) => { diskMap = map; }, (error) => { mapError = t(safeError(error, 'read')); }).finally(() => { mapLoading = false; render(); });
}
function diskMapSection() {
  const section = el('section', 'project-section disk-map'); section.setAttribute('aria-labelledby', 'disk-map-heading');
  const heading = el('h2', 'section-heading', t('Repositories on disk')); heading.id = 'disk-map-heading';
  const toggle = button(mapOpen ? t('Hide repositories') : t('Show repositories'), () => { mapOpen = !mapOpen; if (mapOpen && !diskMap) loadMap(); else render(); restoreFocus('disk-map-toggle'); }, 'text-button', 'disk-map-toggle');
  toggle.setAttribute('aria-expanded', String(mapOpen));
  section.append(heading, el('p', 'surface-note', t('Every Git repository under the folders you choose, with the groups it is in. Add one to a group, or move it with its worktrees and clones to another.')), toggle);
  if (!mapOpen) return section;
  // The folders looked under: chosen with the system picker, remembered; none means ~/DATA (else home).
  const where = el('div', 'folder-picker-list'); const list = el('ul', 'folder-list');
  const shown = mapFolders.length ? mapFolders : diskMap?.under ?? [];
  if (!shown.length) list.append(el('li', 'folder-path is-empty', t('~/DATA, or your home folder')));
  for (const folder of shown) {
    const item = el('li', 'folder-item'); const path = el('span', 'folder-path', folder); path.title = folder; item.append(path);
    if (mapFolders.length) {
      const remove = button('×', () => { mapFolders = mapFolders.filter((f) => f !== folder); saveMapFolders(); loadMap(); restoreFocus('disk-map-add'); }, 'icon-button', '');
      remove.setAttribute('aria-label', t('Stop looking under {folder}', { folder })); item.append(remove);
    }
    list.append(item);
  }
  const add = button(t('Add folder…'), () => void chooseFolder(mapFolders.at(-1) ?? diskMap?.under[0] ?? '', (text) => { mapError = text; render(); }).then((folder) => { if (folder && !mapFolders.includes(folder)) { mapFolders = [...mapFolders, folder]; saveMapFolders(); loadMap(); } }), 'button row-button', 'disk-map-add');
  const again = button(t('Look again'), () => loadMap(), 'text-button', 'disk-map-refresh'); again.disabled = mapLoading;
  where.append(list, add, again);
  section.append(groupField(t('Look under'), where));
  const filters = el('div', 'segmented'); filters.setAttribute('role', 'group'); filters.setAttribute('aria-label', t('Show'));
  const all = mapRows(diskMap, 'all'); const loose = all.filter((r) => !r.pool).length;
  for (const [value, label] of [['unassigned', t('Not in a group ({n})', { n: loose })], ['all', t('All ({n})', { n: all.length })]] as [MapFilter, string][]) {
    const choice = button(label, () => { mapFilter = value; render(); restoreFocus(`disk-map-${value}`); }, mapFilter === value ? 'segment is-on' : 'segment', `disk-map-${value}`);
    choice.setAttribute('aria-pressed', String(mapFilter === value)); filters.append(choice);
  }
  section.append(filters);
  if (mapError) section.append(el('p', 'form-error', mapError));
  if (mapLoading) { section.append(el('p', 'surface-note', t('Looking for repositories…'))); return section; }
  const rows = mapRows(diskMap, mapFilter);
  if (!rows.length) { section.append(el('p', 'surface-note', mapFilter === 'unassigned' ? t('Every repository here is in a group.') : t('No Git repositories here.'))); return section; }
  const listing = el('ul', 'project-folders repository-map'); listing.setAttribute('aria-label', t('Repositories'));
  for (const row of rows) {
    const item = el('li', 'account-meta repository-row');
    const text = el('div', 'repository-text'); const name = el('span', 'repository-name', row.name); name.title = row.folder;
    const groups = row.groups ?? (row.pool && row.project ? [{ pool: row.pool, name: row.project }] : []);
    const where = row.project === null ? t('Not in a group') : row.place === 'inside' ? t('Inside a folder of “{name}”', { name: row.project }) : groups.length > 1 ? t('Groups {names}', { names: groups.map((g) => t('“{name}”', { name: g.name })).join(', ') }) : t('Group “{name}”', { name: row.project });
    text.append(name, el('span', 'folder-note', ` · ${where}${row.branch ? ` · ${row.branch}` : ''}`), el('span', 'repository-path', row.folder));
    item.append(text);
    if (row.action) {
      const act = button(row.action === 'assign' ? t('Add to group…') : t('Move…'), () => moveRootDialog(row), 'text-button', `move-root-${row.folder}`);
      act.setAttribute('aria-label', row.action === 'assign' ? t('Add {name} to a group', { name: row.name }) : t('Move {name} to another group', { name: row.name }));
      item.append(act);
      // SB-146: a project may be in several groups.
      if (row.action === 'move' && groups.length < (snapshot?.projects?.length ?? 0)) {
        const also = button(t('Also add…'), () => moveRootDialog(row, true), 'text-button', `also-root-${row.folder}`);
        also.setAttribute('aria-label', t('Also add {name} to another group', { name: row.name }));
        item.append(also);
      }
    }
    listing.append(item);
  }
  section.append(listing);
  if (diskMap?.truncated) section.append(el('p', 'form-note', t('Only the first repositories found are listed. Choose a narrower folder to see the rest.')));
  return section;
}
function moveRootDialog(row: MapRow, also = false) {
  const joining = row.pool === null;
  const held = new Set(row.groups?.map((g) => g.pool) ?? (row.pool ? [row.pool] : []));
  const context = also
    ? openDialog(t('Also add to a group'), t('“{name}” stays in its groups and joins one more. Sessions in its folders use the first of its groups that has an account.', { name: row.name }))
    : openDialog(joining ? t('Add to a group') : t('Move to another group'), joining ? t('Choose the group “{name}” goes to. Its worktrees and clones join it by themselves.', { name: row.name }) : t('Choose where “{name}” goes. Its worktrees and clones go with it.', { name: row.name }));
  const group = el('fieldset', 'appearance-options move-options'); group.append(el('legend', 'sr-only', t('Group')));
  const targets = rootTargets(row.pool, snapshot?.projects ?? []).filter((x) => !also || !held.has(x.pool)).map((x) => also ? { ...x, current: false } : x);
  const radios: HTMLInputElement[] = [];
  for (const target of targets) {
    const option = el('label', 'appearance-option'); const radio = el('input'); radio.type = 'radio'; radio.name = 'move-root'; radio.value = target.pool; radio.checked = target.current; radio.required = true;
    const text = t('Group “{name}”', { name: target.name });
    option.append(radio, groupHint(target.current ? t('{target} (now)', { target: text }) : text, target.pool, false)); group.append(option); radios.push(radio);
  }
  context.body.append(group);
  if (!targets.length) context.body.append(el('p', 'form-note', t('There are no groups yet. Create one under Groups first.')));
  const confirm = submit(joining || also ? t('Add') : t('Move')); context.actions.append(confirm);
  const sync = () => { const chosen = radios.find((r) => r.checked); confirm.disabled = !chosen || chosen.value === row.pool; };
  radios.forEach((radio) => radio.addEventListener('change', sync)); sync();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const chosen = radios.find((r) => r.checked); if (!chosen || chosen.value === row.pool) return;
    const target = targets.find((x) => x.pool === chosen.value)!;
    const success = joining || also ? t('{name} added to group “{project}”.', { name: row.name, project: target.name }) : t('{name} moved to group “{project}”.', { name: row.name, project: target.name });
    void dialogSave(context, async () => { await adapter.moveRoot(row.folder, chosen.value, also, also || joining ? null : row.pool); diskMap = null; loadMap(); }, success);
  });
  (radios.find((r) => !r.checked) ?? radios[0])?.focus();
}
/** SB-146: a group's name; hovering or focusing it shows the projects it holds (repositories, never worktrees or subfolders). */
let hintSeq = 0;
function groupHint(text: string, pool: string, focusable = true) {
  wantRoots();
  const project = snapshot?.projects?.find((p) => p.pool === pool);
  const wrap = el('span', 'group-hint');
  wrap.append(el('span', 'group-hint-name', text));
  if (!project) return wrap;
  // UX-20: one tab stop only where there is something to show, none inside a radio's label; Escape closes the tip.
  if (focusable) wrap.tabIndex = 0;
  wrap.addEventListener('keydown', (event) => { if (event.key === 'Escape' && !wrap.classList.contains('is-dismissed')) { event.stopPropagation(); wrap.classList.add('is-dismissed'); } });
  wrap.addEventListener('pointerleave', () => wrap.classList.remove('is-dismissed')); wrap.addEventListener('blur', () => wrap.classList.remove('is-dismissed'));
  const names = groupProjects(project.folders, projectRoots?.[pool] ?? null);
  const tip = el('span', 'group-tip'); tip.setAttribute('role', 'tooltip'); tip.id = `group-tip-${++hintSeq}`;
  tip.append(el('strong', '', plural(names.length, { one: '{n} repository', other: '{n} repositories' })));
  const list = el('ul', 'group-tip-list');
  for (const name of names.slice(0, 40)) list.append(el('li', '', name));
  if (names.length > 40) list.append(el('li', 'group-tip-more', t('and {n} more', { n: names.length - 40 })));
  tip.append(list); wrap.append(tip); wrap.setAttribute('aria-describedby', tip.id);
  return wrap;
}
/** Projects (0.6): folders that reserve their own accounts (core `save_project`). */
function projectsSection() {
  const section = el('section', 'project-section'); section.setAttribute('aria-labelledby', 'projects-heading');
  const heading = el('h2', 'section-heading sr-only', t('Groups')); heading.id = 'projects-heading';
  section.append(heading, el('p', 'surface-note', t('A group holds one or more repositories with accounts of its own, and a repository may be in several groups. Sessions launched from its folders use only its accounts, automatic switching stays inside them, and no other group or the ordinary Claude Code switches to them.')));
  const projects = snapshot!.projects ?? [];
  if (!projects.length) {
    const empty = emptyState(t('No groups yet'), t('Create one when a client or a team should work only on its own subscriptions. Its accounts move into the group, and its folders stay on them.'));
    empty.append(button(t('+ New group'), () => projectDialog(), 'button primary', 'empty-new-project'));
    section.append(empty); return section;
  }
  const list = el('div', 'account-list'); list.setAttribute('aria-label', t('Groups'));
  for (const project of projects) {
    const accounts = snapshot!.accounts.filter((a) => a.pool === project.pool);
    const card = el('article', 'account-card project-card'); card.setAttribute('aria-label', `${project.name}, ${plural(accounts.length, { one: '{n} account', other: '{n} accounts' })}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    const name = el('h2', ''); name.append(groupHint(project.name, project.pool));
    title.append(name, el('span', 'badge muted', `${plural(accounts.length, { one: '{n} account', other: '{n} accounts' })}`));
    // SB-109: the organization the group runs on; a personal group carries no badge.
    const groupOrg = project.organization ? snapshot!.organizations?.find((o) => o.id === project.organization) : undefined;
    if (groupOrg) { const badge = el('span', 'badge org-badge', groupOrg.name); badge.title = t('Runs only on accounts of {name}.', { name: groupOrg.name }); title.append(badge); }
    const folders = projectFolderList(project);
    const chips = el('p', 'account-meta', accounts.length ? accounts.map((a) => `${a.label} · ${providerName(a.provider)}`).join(', ') : t('No accounts yet, so sessions in these folders use your other accounts until you add one.'));
    details.append(title, folders, chips);
    // C-3 (SCN-081): which chain the group's tasks fall back along; the editor is on Agents.
    const fallback = groupChainLine(project.pool);
    if (fallback) { const line = el('p', 'account-meta chain-line'); line.append(button(fallback, () => goTo('agents', `chain-edit-project-${project.pool}`), 'text-button chain-link', `chain-link-${project.pool}`)); details.append(line); }
    // SB-170: every account of the group belongs elsewhere now, so its folders are refused.
    if (groupOrg && groupSeatsElsewhere(snapshot!, groupOrg.id, accounts)) details.append(el('p', 'surface-note seats-elsewhere', t('None of these accounts belongs to {name} any more, so sessions in this group\'s folders are refused. Add an account of {name}, or change the group\'s organization.', { name: groupOrg.name })));
    else { const ended = groupOrg ? groupSeatsEndedText(snapshot!, project, accounts, monitor?.sign_in_required) : null; if (ended) details.append(el('p', 'surface-note seats-ended', ended)); }
    const actions = el('div', 'management-actions');
    actions.append(button(t('Edit'), () => projectDialog(project), 'text-button', `edit-project-${project.pool}`), button(t('Delete'), () => deleteProjectDialog(project), 'text-button danger-text', `delete-project-${project.pool}`));
    card.append(details, actions); list.append(card);
  }
  section.append(list); return section;
}
/** Folders shown on a project's card before *Show all*: a project an agent works in gathers dozens (SB-131). */
const FOLDERS_SHOWN = 6;
const expandedProjects = new Set<string>();
/** SB-135: each project's folders as Git sees them (roots, and the worktrees and clones under each), read again when the folders change. */
let projectRoots: Record<string, ProjectRoot[]> | null = null; let rootsKey = ''; let rootsLoading = false;
function wantRoots() {
  const key = JSON.stringify((snapshot?.projects ?? []).map((p) => [p.pool, p.folders]));
  if (key === rootsKey || rootsLoading) return;
  rootsLoading = true;
  void adapter.projectRoots().then((roots) => { projectRoots = roots; rootsKey = key; }, () => { rootsKey = key; }).finally(() => { rootsLoading = false; render(); });
}
const derivedText = (kind: 'worktree' | 'clone' | null, branch: string | null) => [kind === 'clone' ? t('clone') : kind === 'worktree' ? t('worktree') : '', branch ?? ''].filter(Boolean).join(' · ');
function projectFolderList(project: Project) {
  wantRoots();
  const wrap = el('div', 'project-folder-block');
  const open = expandedProjects.has(project.pool);
  // SB-147: the group's projects, each with its folders (the repository, its worktrees and clones).
  const projects = groupProjectRows(project.folders, projectRoots?.[project.pool] ?? null);
  const shown = open ? projects : projects.slice(0, FOLDERS_SHOWN);
  const list = el('ul', 'group-projects'); list.setAttribute('aria-label', t('Repositories'));
  for (const entry of shown) {
    const item = el('li', 'group-project');
    const title = el('div', 'group-project-title');
    title.append(el('span', 'group-project-name', entry.name), el('span', 'folder-note', plural(entry.rows.length, { one: '{n} folder', other: '{n} folders' })));
    item.append(title);
    const folders = el('ul', 'project-folders');
    for (const row of entry.rows) {
      const line = el('li', row.depth ? 'account-meta derived-folder' : 'account-meta', row.depth ? folderName(row.folder) : row.folder); line.title = row.folder;
      const note = row.depth ? derivedText(row.kind, row.branch) : row.branch ?? '';
      if (note) line.append(el('span', 'folder-note', ` · ${note}`));
      folders.append(line);
    }
    item.append(folders); list.append(item);
  }
  wrap.append(list);
  if (projects.length > FOLDERS_SHOWN || open) {
    const label = open ? t('Show fewer repositories') : t('Show all repositories ({n})', { n: projects.length });
    const toggle = button(label, () => { if (open) expandedProjects.delete(project.pool); else expandedProjects.add(project.pool); render(); restoreFocus(`folders-${project.pool}`); }, 'text-button', `folders-${project.pool}`);
    toggle.setAttribute('aria-expanded', String(open));
    wrap.append(toggle);
  }
  return wrap;
}
function projectDialog(existing?: Project, organization?: string) {
  const context = openDialog(existing ? t('Edit {name}', { name: existing.name }) : t('New group'), t('Add the group\'s repositories (folders) and choose the accounts that belong to it. An account belongs to one group at a time; a repository may be in several groups.'));
  const name = input(existing?.name ?? ''); name.maxLength = 80; name.placeholder = t('e.g. Client Alpha');
  const folders = foldersPicker(context, existing?.folders ?? [], 'project-folders', existing ? projectRoots?.[existing.pool] ?? null : null);
  const picker = el('fieldset', 'project-accounts'); picker.append(el('legend', 'field-label', t('Accounts')));
  const usable = snapshot!.accounts;
  // SB-109 (SCN-070): with organizations, the group runs on one organization's accounts or on personal ones.
  const organizations = snapshot!.organizations ?? [];
  const orgPicker = organizations.length ? select([['personal', t('Personal accounts')], ...organizations.map((o): [string, string] => [o.id, o.name])]) : null;
  if (orgPicker) orgPicker.value = existing?.organization && organizations.some((o) => o.id === existing.organization) ? existing.organization : organization && organizations.some((o) => o.id === organization) ? organization : 'personal';
  // SB-202 (B-8): + Group for {organization} starts with its name and its seats already here.
  if (!existing && organization) name.value = organizations.find((o) => o.id === organization)?.name ?? '';
  const chosenOrg = () => (orgPicker && orgPicker.value !== 'personal' ? orgPicker.value : null);
  if (!usable.length) picker.append(el('p', 'form-note', t('No accounts yet. Add accounts first, or save the group now and add them later.')));
  const rows: { account: Account; box: HTMLInputElement; why: HTMLElement; text: string }[] = [];
  for (const account of usable) {
    const owner = projectOf(account.pool);
    const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.value = account.id; box.checked = existing ? account.pool === existing.pool : !!organization && attribution(snapshot!, account).organization === organization && !inCli(account);
    const note = owner && owner.pool !== existing?.pool ? t(' · in {name}, moves here', { name: owner.name }) : '';
    const why = el('span', '', `${account.label} · ${providerName(account.provider)}${note}`);
    option.append(box, why); picker.append(option);
    rows.push({ account, box, why, text: why.textContent ?? '' });
  }
  const syncAccounts = () => {
    const org = chosenOrg();
    for (const { account, box, why, text } of rows) {
      const fits = groupAdmits(snapshot!, org, account);
      box.disabled = !fits; if (!fits) box.checked = false;
      const own = attribution(snapshot!, account).organization;
      why.textContent = fits ? text : `${text} · ${own ? t('Belongs to {name}.', { name: organizations.find((o) => o.id === own)?.name ?? '' }) : t('Personal account.')}`;
    }
  };
  orgPicker?.addEventListener('change', syncAccounts); syncAccounts();
  const grid = el('div', 'form-grid'); grid.append(field(t('Group name'), name), groupField(t('Folders'), folders.node, t('The repositories and folders of this group; subfolders are included. Worktrees and clones of its repositories join it by themselves, and folders deleted from disk leave it. Add any other repository here, or from Repositories on disk.')));
  if (orgPicker) grid.append(field(t('Organization'), orgPicker, t('Sessions in this group\'s folders run only on this organization\'s accounts, or only on personal ones when you choose Personal accounts.')));
  context.body.append(grid, picker, el('p', 'form-note', t('Accounts you leave out move back to the default group.')));
  context.actions.append(submit(existing ? t('Save group') : t('Create group')));
  name.focus();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const accountIds = [...picker.querySelectorAll<HTMLInputElement>('input[type=checkbox]:checked')].map((box) => box.value);
    const list = folders.require(); if (!list) return;
    void dialogSave(context, () => adapter.saveProject({ pool: existing?.pool, name: name.value, folders: list, accountIds, baseFolders: existing?.folders, organization: orgPicker ? orgPicker.value : undefined }), existing ? t('Group “{name}” saved.', { name: name.value.trim() }) : t('Group “{name}” created. Launch its accounts from its folders.', { name: name.value.trim() }));
  });
}
function rulesSection() {
  const main = el('section', 'project-section'); main.setAttribute('aria-labelledby', 'rules-heading');
  const heading = el('div', 'section-heading-row'); const h = el('h2', 'section-heading', t('Folder rules')); h.id = 'rules-heading'; h.tabIndex = -1; h.dataset.focus = 'rules-heading';
  heading.append(h); if (snapshot!.accounts.length) heading.append(button(t('+ Add rule'), () => ruleDialog(), 'button quiet', 'add-rule'));
  main.append(heading);
  const rules = snapshot!.rules ?? [];
  main.append(el('p', 'surface-note', t('Folder rules are optional. With none, Switchboard follows your selection and automatic switching in every folder. A rule applies when an agent or switchboard project apply asks for it, only to that session, and never turns automatic switching off.')));
  if (!rules.length) {
    const empty = emptyState(t('No folder rules'), snapshot!.accounts.length ? t('Add a rule when one folder should start on a specific account, for example a client repository on its own subscription. Prefer an expiry.') : t('Add an account first; a rule names one of your accounts.'));
    if (snapshot!.accounts.length) empty.append(button(t('Add rule'), () => ruleDialog(), 'button primary', 'empty-add-rule'));
    main.append(empty); return main;
  }
  const now = Date.now() / 1000;
  const list = el('div', 'account-list'); list.setAttribute('aria-label', t('Folder rules'));
  for (const rule of rules) {
    const state = ruleState(rule, now); const account = ruleAccount(rule); const key = `${rule.provider}-${rule.path}`;
    const card = el('article', `account-card ${state === 'active' ? 'is-selected' : 'is-disabled'}`); card.setAttribute('aria-label', `${projectName(rule.path)}, ${state}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    title.append(el('h2', '', projectName(rule.path)), el('span', `badge ${state === 'active' ? 'current-badge' : 'muted'}`, { active: t('Active'), paused: t('Paused'), expired: t('Expired') }[state]));
    const path = el('p', 'account-meta', rule.path); path.title = rule.path;
    details.append(title, path, el('p', 'account-meta', `${providerName(rule.provider)} · ${account ? `${account.label} · ${poolName(account.pool, snapshot!.projects)}` : t('account missing')} · ${ruleTarget(rule)}`), el('p', 'usage-caption', rule.expires_at ? (state === 'expired' ? t('Expired {date}', { date: date(rule.expires_at) }) : t('Until {date}', { date: date(rule.expires_at) })) : t('No expiry')));
    const actions = el('div', 'management-actions');
    if (state === 'active') actions.append(button(t('Pause'), () => void mutate(() => adapter.setProjectRule({ path: rule.path, accountId: rule.account_id, target: rule.target, enabled: false, expiresAt: rule.expires_at }), t('Rule paused. Selection and automatic switching are unchanged.'), `pause-${key}`), 'text-button', `pause-${key}`));
    else actions.append(button(t('Resume…'), () => ruleDialog(rule), 'text-button', `resume-${key}`));
    actions.append(button(t('Edit'), () => ruleDialog(rule), 'text-button', `edit-${key}`), button(t('Remove'), () => removeRuleDialog(rule), 'text-button danger-text', `remove-${key}`));
    card.append(details, actions); list.append(card);
  }
  main.append(list);
  return main;
}
function ruleDialog(existing?: ProjectRule) {
  const context = openDialog(existing ? t('Edit folder rule') : t('Add folder rule'), t('Sessions in this folder and its subfolders start on the chosen account when the rule is applied. Automatic switching still runs.'));
  const path = folderPicker(context, existing?.path ?? lastWorkingDirectory, 'rule-folder', !!existing);
  const usable = snapshot!.accounts.filter((account) => account.enabled);
  const account = select(usable.map((item) => [item.id, `${item.label} · ${providerName(item.provider)} · ${poolName(item.pool, snapshot!.projects)}`]));
  if (existing) account.value = existing.account_id;
  const target = select([['managed', t('Managed sessions (switch from the next request)')], ['claude_cli', t('Claude Code login (changes every claude session)')]]);
  target.value = existing?.target ?? 'managed';
  const expiry = select(EXPIRY_CHOICES); expiry.value = existing && existing.expires_at === null ? '' : '8';
  const sync = () => { const chosen = usable.find((item) => item.id === account.value); const option = target.querySelector<HTMLOptionElement>('option[value="claude_cli"]')!; option.disabled = !(chosen?.provider === 'claude' && chosen.kind === 'oauth' && chosen.external_identity) || !!snapshot!.projects?.some((project) => project.pool === chosen?.pool); if (option.disabled && target.value === 'claude_cli') target.value = 'managed'; };
  account.addEventListener('change', sync); sync();
  const grid = el('div', 'form-grid'); grid.append(groupField(t('Folder'), path.node, t('Sessions in this folder and its subfolders.')), field(t('Account'), account), field(t('Applies to'), target), field(t('Keep the rule'), expiry, t('Pause or remove it any time on this screen.')));
  context.body.append(grid); context.actions.append(submit(existing ? t('Save rule') : t('Add rule')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const folder = path.require(); if (!folder) return;
    void dialogSave(context, () => adapter.setProjectRule({ path: folder, accountId: account.value, target: target.value as ProjectRule['target'], enabled: true, expiresAt: expiryFrom(expiry.value, Math.floor(Date.now() / 1000)) }), t('Rule saved and active. It applies when an agent or switchboard project apply asks.'));
  });
  if (existing) account.focus(); else path.focus();
}
/** UX-19: the button itself says Copied for two seconds, beside the announcement. */
function copyText(control: HTMLButtonElement, text: string, label: string) {
  if (!navigator.clipboard) { announce(t('Copy is unavailable here. Select the command text instead.')); return; }
  void navigator.clipboard.writeText(text).then(() => {
    announce(t('{label} command copied.', { label })); control.textContent = t('Copied'); control.classList.add('is-copied');
    window.setTimeout(() => { control.textContent = t('Copy'); control.classList.remove('is-copied'); }, 2000);
  }, () => announce(t('Copy is unavailable here. Select the command text instead.')));
}
function copyable(label: string, command: string, key: string) {
  const row = el('div', 'policy-row'); const content = el('div');
  content.append(el('strong', 'block', label), el('code', 'address', command));
  const copy = button(t('Copy'), () => copyText(copy, command, label), 'text-button', key);
  row.append(content, copy);
  return row;
}
let agentsView: AgentsOverview | null = null;
let agentsError = '';
async function loadAgents() {
  try { agentsView = await adapter.agentsOverview(); agentsError = ''; } catch (error) { agentsError = safeError(error, 'read'); }
  backgroundRender();
}
const catalogAgent = (id: string) => (agentCatalog as { agents: AgentInfo[] }).agents.find((a) => a.id === id);
/** SB-120: the Agents page — the default agent, what is installed and takes over work,
 * what can be installed, then the OpenRouter key and the tools registration. */
function renderAgents(main: HTMLElement) {
  ceilingBanners(main);
  if (agentsError && !agentsView) { const panel = el('section', 'about-panel'); panel.append(el('p', 'usage-error', t(agentsError)), button(t('Retry'), () => void loadAgents(), 'button', 'agents-retry')); main.append(panel); }
  else if (!agentsView) { const panel = el('section', 'about-panel'); panel.append(el('p', 'form-note', t('Looking for coding agents on this computer…'))); main.append(panel); }
  else main.append(defaultAgentPanel(agentsView), installedAgentsPanel(agentsView), chainsPanel(), availableAgentsPanel(agentsView));
  if (!agentsView) main.append(chainsPanel());
  main.append(tokensPointer(), connectPanel());
}
function defaultAgentPanel(view: AgentsOverview) {
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'default-agent-heading');
  const heading = el('h2', '', t('Default agent')); heading.id = 'default-agent-heading';
  section.append(heading, el('p', '', t('The agent that takes over a workflow when none is named: switchboard continue without an account hands it there, and Fabric starts with it unless a project sets its own. Fallback chains keep their own order.')));
  const chips = el('div', 'choice-chips'); chips.setAttribute('role', 'group'); chips.setAttribute('aria-label', t('Default agent'));
  for (const choice of defaultAgentChoices(view.agents, view.default)) {
    const key = `default-agent-${choice.id ?? 'auto'}`;
    const chip = button(choice.on ? `✓ ${choice.label}` : choice.label, () => { if (!choice.on) void mutate(async () => { await adapter.agentDefault(choice.id); agentsView = await adapter.agentsOverview(); }, choice.id ? t('{agent} is the default agent.', { agent: choice.label }) : t('The default agent is Auto.'), key); }, `choice-chip${choice.on ? ' is-on' : ''}`, key);
    chip.setAttribute('aria-pressed', String(choice.on)); chips.append(chip);
  }
  section.append(chips, el('p', 'form-note', t('Auto: switchboard continue asks for an account, and Fabric uses its own choice. Claude Code, Codex and Kimi Code continue on whichever of their accounts has room; Hermes and OpenCode on the OpenRouter key.')));
  return section;
}
function installedAgentsPanel(view: AgentsOverview) {
  const section = el('section', 'about-panel agents-panel'); section.setAttribute('aria-labelledby', 'installed-agents-heading');
  const head = el('div', 'agents-head'); const heading = el('h2', '', t('Installed')); heading.id = 'installed-agents-heading';
  // Agents that can take over work first, in chain order; tools-only agents after them.
  const installed = sortAgents(view.agents.filter((a) => a.installed));
  // SB-169: the page header's Refresh looks the agents up again (`reload`); one button, one meaning.
  head.append(heading, el('span', 'badge', plural(installed.length, { one: '{n} detected', other: '{n} detected' })));
  section.append(head, el('p', '', t('Enabled agents can take over a workflow whose account ran out: in fallback chains and with switchboard continue. Disable one to keep work away from it; switching accounts is not affected.')));
  const list = el('div', 'agent-list');
  for (const agent of installed) list.append(agentRow(agent));
  section.append(list);
  return section;
}
function agentRow(agent: AgentRow) {
  const row = el('div', `agent-row${agent.switchable && !agent.enabled ? ' is-off' : ''}`); row.dataset.agent = agent.id;
  const info = el('div', 'agent-info'); const title = el('div', 'agent-title');
  title.append(el('strong', '', agent.name));
  if (agent.switchable && agent.disabled) title.append(el('span', 'badge muted', t('Disabled')));
  info.append(title, el('code', 'agent-command', agent.command));
  if (agent.id === 'hermes' && hermes?.installed) info.append(el('span', 'form-note', `${t('Model: {model}', { model: hermes.model ?? t('not set') })} · ${t('Provider: {provider}', { provider: hermes.provider ?? t('not set') })}`));
  if (agent.id === 'hermes' && hermesError) info.append(el('span', 'usage-error', t(hermesError)));
  const controls = el('div', 'agent-controls');
  if (agent.switchable) {
    const seg = el('div', 'segmented'); seg.setAttribute('role', 'group'); seg.setAttribute('aria-label', t('{agent} takes over work', { agent: agent.name }));
    for (const [value, label] of [[true, t('Enabled')], [false, t('Disabled')]] as const) {
      const on = agent.enabled === value;
      const b = button(label, () => { if (!on) void mutate(async () => { await adapter.agentEnable(agent.id, value); agentsView = await adapter.agentsOverview(); }, value ? t('{agent} can take over work again.', { agent: agent.name }) : agent.default ? t('{agent} no longer takes over work. The default agent is Auto again.', { agent: agent.name }) : t('{agent} no longer takes over work.', { agent: agent.name }), `agent-${value ? 'on' : 'off'}-${agent.id}`); }, `segment${on ? ' is-on' : ''}`, `agent-${value ? 'on' : 'off'}-${agent.id}`);
      b.setAttribute('aria-pressed', String(on)); seg.append(b);
    }
    controls.append(seg);
    if (agent.default) controls.append(el('span', 'badge default-badge', `✓ ${t('Default')}`));
    else if (agent.enabled) controls.append(button(t('Set default'), () => void mutate(async () => { await adapter.agentDefault(agent.id); agentsView = await adapter.agentsOverview(); }, t('{agent} is the default agent.', { agent: agent.name }), `agent-default-${agent.id}`), 'text-button', `agent-default-${agent.id}`));
    else controls.append(el('span', 'default-slot'));
  } else controls.append(el('span', 'badge muted', t('Tools only')), el('span', 'default-slot'));
  const info2 = catalogAgent(agent.id);
  controls.append(info2 ? button(t('Set up'), () => void agentDialog(info2), 'button quiet', `agent-${agent.id}`) : el('span'));
  row.append(info, controls);
  const more = el('details', 'agent-more'); const summary = el('summary', '', t('Details')); summary.dataset.focus = `agent-more-${agent.id}`;
  more.append(summary);
  more.append(el('p', 'form-note', agent.switchable ? t('Switchboard can hand a workflow to it and launch it from Agents.') : t('It can use Switchboard\'s tools (switchboard mcp); Switchboard cannot hand it a workflow yet.')));
  if (agent.id === 'hermes') { const actions = el('div', 'management-actions'); actions.append(button(t('Change Hermes\'s model'), () => hermes ? hermesDialog() : void loadHermes(), 'button', 'hermes-change')); more.append(actions); }
  more.append(address(agent.url));
  const wrap = el('div', 'agent-item'); wrap.append(row, more);
  return wrap;
}
function availableAgentsPanel(view: AgentsOverview) {
  const section = el('section', 'about-panel agents-panel'); section.setAttribute('aria-labelledby', 'available-agents-heading');
  const missing = view.agents.filter((a) => !a.installed);
  const head = el('div', 'agents-head'); const heading = el('h2', '', t('Available to install')); heading.id = 'available-agents-heading';
  head.append(heading, el('span', 'badge', plural(missing.length, { one: '{n} agent', other: '{n} agents' })));
  section.append(head, el('p', '', t('Install one with its command, then press Refresh above: it moves to Installed.')));
  const details = el('details', 'agent-more available-list'); const summary = el('summary', '', t('Show all')); summary.dataset.focus = 'agents-available';
  details.append(summary);
  const list = el('div', 'agent-list');
  for (const agent of missing) {
    const row = el('div', 'agent-row'); const info = el('div', 'agent-info'); const title = el('div', 'agent-title');
    title.append(el('strong', '', agent.name)); if (agent.maker) title.append(el('span', 'form-note', agent.maker));
    info.append(title);
    if (agent.install) info.append(el('code', 'agent-command', agent.install)); else info.append(address(agent.url));
    const controls = el('div', 'agent-controls');
    if (agent.install) { const copy = button(t('Copy'), () => copyText(copy, agent.install!, agent.name), 'text-button', `install-${agent.id}`); controls.append(copy); }
    row.append(info, controls); list.append(row);
  }
  details.append(list); section.append(details, el('p', 'form-note', t('Sources for every agent: docs/research/agents-2026-10-05.md in the repository. CLI: switchboard agents status | enable | disable | default | launch.')));
  return section;
}
// C-3 (SB-71, SCN-080..082): the fallback chains, read with what each step would do now.
let chainsView: Chains | null = null;
let chainsError = '';
async function loadChains() {
  try { chainsView = await adapter.chains(); chainsError = ''; } catch (error) { chainsError = safeError(error, 'read'); }
  backgroundRender();
}
const chainAgentName = (id: string) => agentsView?.agents.find((a) => a.id === id)?.name ?? catalogAgent(id)?.name ?? id;
const chainAccountLabel = (id: string) => snapshot?.accounts.find((a) => a.id === id)?.label ?? kimi?.accounts.find((k) => k.account.id === id)?.account.label;
const scopeKey = (scope: ChainScope) => scope.kind === 'machine' ? 'machine' : scope.kind === 'project' ? `project:${scope.pool}` : `workflow:${scope.id}`;
const chainFor = (key: string) => chainsView?.chains.find((c) => scopeKey(c.scope) === key) ?? null;
const scopeLabel = (scope: ChainScope) => scope.kind === 'machine' ? t('This computer') : scope.kind === 'project' ? projectOf(scope.pool)?.name ?? scope.pool : t('Task {id}', { id: shortWorkflow(scope.id) });
/** The Groups card's line (SCN-081): the group's own chain, this computer's, or none. */
function groupChainLine(pool: string): string | null {
  if (!chainsView) return null;
  if (chainFor(`project:${pool}`)) return t('Fallback: own chain');
  return chainFor('machine') ? t('Fallback: this computer\'s chain') : t('Fallback: not set');
}
function chainsPanel() {
  const section = el('section', 'about-panel chains-panel'); section.id = 'chains-panel'; section.setAttribute('aria-labelledby', 'chains-heading');
  const heading = el('h2', '', t('Fallback chains')); heading.id = 'chains-heading';
  section.append(heading, el('p', '', t('When a task\'s accounts run out, Switchboard hands it to the next agent in its chain. Nothing is handed over until a chain is set.')));
  if (chainsError) {
    const failed = el('div', 'notice error stale-list'); failed.setAttribute('role', 'status');
    failed.append(el('span', '', `${t('Chains are unavailable. Refresh to retry.')}${chainsView ? ` ${t('The list may be out of date: the last read failed.')}` : ''}`), button(t('Retry'), () => void loadChains(), 'text-button', 'chains-retry'));
    section.append(failed);
  }
  if (!chainsView) { if (!chainsError) { const reading = el('p', 'form-note', t('Reading fallback chains…')); reading.setAttribute('role', 'status'); section.append(reading); } return section; }
  const view = chainsView;
  if (view.ceilings_unreadable) section.append(el('p', 'usage-error', t('Spending ceilings (spend.json) could not be read, so no paid agent is handed work until they are. Delete spend.json, then restore a backup to get the ceilings back, or leave it deleted to start over with no ceilings.')));
  if (!view.limits_known) section.append(el('p', 'form-note', t('Account limits are not known right now, so a step may read ready when its accounts have no room.')));
  const nothingSet = !view.chains.length;
  const list = el('div', 'chain-list');
  list.append(chainRow({ kind: 'machine' }, nothingSet));
  const projects = snapshot?.projects ?? [];
  for (const project of projects) list.append(chainRow({ kind: 'project', pool: project.pool }, false));
  for (const chain of view.chains) if (chain.scope.kind === 'workflow') list.append(chainRow(chain.scope, false));
  section.append(list);
  if (!projects.length) section.append(el('p', 'form-note', t('A group gets its own chain once you create one under Groups.')));
  section.append(lastHandoverLine(view));
  return section;
}
function chainRow(scope: ChainScope, primary: boolean) {
  const key = scopeKey(scope); const chain = chainFor(key); const label = scopeLabel(scope);
  const anchor = scope.kind === 'machine' ? 'machine' : scope.kind === 'project' ? `project-${scope.pool}` : `workflow-${scope.id}`;
  const row = el('div', 'chain-row'); row.id = `chain-row-${anchor}`; row.setAttribute('role', 'group'); row.setAttribute('aria-label', label);
  const head = el('div', 'chain-head'); const info = el('div', 'chain-info');
  info.append(el('strong', 'chain-scope', label));
  if (chain) {
    const steps = el('ol', 'chain-steps'); steps.setAttribute('aria-label', t('{scope}: {chain}', { scope: label, chain: chainSummary(chain.executors, chainAgentName, chainAccountLabel) }));
    for (const step of chain.executors) {
      const pin = step.key ? null : step.account_id ? chainAccountLabel(step.account_id) ?? step.account_id : null;
      const item = el('li', 'chain-step-chip'); const chip = el('span', 'chain-chip', chainAgentName(step.agent)); if (pin) chip.append(el('span', 'chain-pin', pin)); if (step.key) chip.append(el('span', 'chain-pin', t('key')));
      item.append(chip); steps.append(item);
    }
    info.append(steps);
  } else info.append(el('span', 'form-note', scope.kind === 'project' ? (chainFor('machine') ? t('Uses this computer\'s chain') : t('Uses this computer\'s chain, which is not set')) : t('Not set')));
  const actions = el('div', 'management-actions');
  if (scope.kind === 'machine' && !chain) actions.append(button(t('Use Subscriptions first'), () => void mutate(async () => { await adapter.setChain(scope, [], 'subscriptions-first'); chainsView = await adapter.chains(); }, t('This computer\'s chain is set.'), 'chain-edit-machine'), primary ? 'button primary' : 'button', 'chain-preset'));
  const editKey = `chain-edit-${anchor}`;
  if (chain || scope.kind === 'machine') actions.append(button(t('Edit…'), () => chainDialog(scope, chain?.executors ?? [], editKey), 'button', editKey));
  else actions.append(button(t('Set for this group…'), () => chainDialog(scope, [], editKey), 'button', editKey));
  if (chain) actions.append(button(t('Clear'), () => clearChainDialog(scope, scope.kind === 'workflow' ? 'chain-edit-machine' : editKey), 'text-button danger-text', `chain-clear-${anchor}`));
  head.append(info, actions); row.append(head);
  const readiness = chain ? chainsView?.readiness[key] ?? [] : [];
  if (readiness.length) {
    const lines = el('ul', 'chain-readiness'); lines.setAttribute('aria-label', t('Whether each step could take a task now'));
    for (const step of readiness) lines.append(readinessLine(step, scope.kind === 'project' ? label : undefined));
    row.append(lines);
  }
  return row;
}
function readinessLine(step: ChainStepView, groupName?: string) {
  const line = el('li', `chain-ready is-${step.state.replace('_', '-')}`);
  line.append(el('span', 'chain-ready-agent', step.name ?? chainAgentName(step.agent)), el('span', '', stepReasonText(step, groupName)));
  return line;
}
function lastHandoverLine(view: Chains) {
  const wrap = el('div', 'chain-last');
  const last = view.last_handover;
  if (!last) { wrap.append(el('p', 'form-note', t('No hand-over yet.'))); return wrap; }
  wrap.append(el('p', '', t('Last hand-over: {time}, task {task}: {outcome}.', { time: date(last.at), task: shortWorkflow(last.workflow), outcome: handoverOutcomeText(last.outcome) })));
  const skipped = handoverSkipped(last.skipped, chainAgentName);
  if (skipped.length) wrap.append(el('p', 'form-note', t('Passed over: {steps}', { steps: skipped.join('; ') })));
  return wrap;
}
const chainSavedText = (scope: ChainScope) => scope.kind === 'machine' ? t('This computer\'s chain is saved.') : scope.kind === 'project' ? t('The chain of {group} is saved.', { group: scopeLabel(scope) }) : t('The chain of task {id} is saved.', { id: shortWorkflow(scope.id) });
/** The editor (SCN-080): the steps in order, each an agent and an account, moved with buttons. */
function chainDialog(scope: ChainScope, initial: ChainExecutor[], returnFocus: string) {
  let steps: ChainExecutor[] = structuredClone(initial);
  const context = openDialog(t('Fallback chain: {scope}', { scope: scopeLabel(scope) }), t('Steps are tried in order: the first agent that can take the task gets it.'), returnFocus);
  const list = el('ol', 'chain-editor'); list.setAttribute('aria-label', t('Steps'));
  const add = button(t('+ Add agent'), () => { const used = new Set(steps.map((s) => s.agent)); steps = [...steps, { agent: TAKEOVER_ORDER.find((a) => !used.has(a)) ?? TAKEOVER_ORDER[0] }]; paint(`chain-step-${steps.length - 1}-agent`); }, 'button row-button', 'chain-add');
  const full = el('p', 'form-note', t('A chain holds at most 8 agents.'));
  const kimiAccounts = (kimi?.accounts ?? []).map((k) => k.account);
  const focusIn = (key?: string) => { if (key) context.form.querySelector<HTMLElement>(`[data-key="${key}"]`)?.focus(); };
  const paint = (focusKey?: string) => {
    list.replaceChildren();
    steps.forEach((step, index) => {
      const n = index + 1; const name = chainAgentName(step.agent);
      const item = el('li', 'chain-step'); item.append(el('span', 'chain-step-number', `${n}.`));
      const controls = el('div', 'chain-step-fields');
      if (step.key) {
        const fixed = el('div', 'chain-step-fixed'); fixed.append(el('strong', '', name), el('span', 'form-note', t('Pinned to key {name}: not automatic yet (waits for project grants)', { name: step.key })));
        controls.append(fixed);
      } else {
        const agents: [string, string][] = TAKEOVER_ORDER.map((id) => [id, chainAgentName(id)]);
        if (!TAKEOVER_ORDER.includes(step.agent)) agents.push([step.agent, t('{agent} (not automatic)', { agent: name })]);
        const agent = select(agents); agent.value = step.agent; agent.dataset.key = `chain-step-${index}-agent`; agent.setAttribute('aria-label', t('Agent for step {n}', { n }));
        agent.addEventListener('change', () => { steps[index] = { agent: agent.value }; paint(`chain-step-${index}-agent`); });
        const choices = accountsForStep(step.agent, snapshot?.accounts ?? [], kimiAccounts);
        const openrouterAgent = step.agent === 'hermes' || step.agent === 'opencode';
        const options: [string, string][] = [['', openrouterAgent ? t('The OpenRouter key') : t('Any account with room')], ...choices.map((c): [string, string] => [c.id, c.label])];
        if (step.account_id && !choices.some((c) => c.id === step.account_id)) options.push([step.account_id, chainAccountLabel(step.account_id) ?? step.account_id]);
        const account = select(options); account.value = step.account_id ?? ''; account.dataset.key = `chain-step-${index}-account`; account.setAttribute('aria-label', t('Account for step {n}', { n }));
        if (options.length === 1) { account.disabled = true; account.dataset.locked = 'true'; }
        account.addEventListener('change', () => { steps[index] = { agent: step.agent, ...(account.value ? { account_id: account.value } : {}) }; });
        controls.append(agent, account);
      }
      const moves = el('div', 'chain-step-moves');
      const move = (delta: number) => { steps = moveStep(steps, index, delta); const to = index + delta; paint(`chain-step-${to}-${(delta < 0 ? to > 0 : to < steps.length - 1) ? (delta < 0 ? 'up' : 'down') : (delta < 0 ? 'down' : 'up')}`); };
      const up = button(t('Move up'), () => move(-1), 'text-button', ''); up.dataset.key = `chain-step-${index}-up`; up.setAttribute('aria-label', t('Move up: step {n}, {agent}', { n, agent: name }));
      const down = button(t('Move down'), () => move(1), 'text-button', ''); down.dataset.key = `chain-step-${index}-down`; down.setAttribute('aria-label', t('Move down: step {n}, {agent}', { n, agent: name }));
      const remove = button(t('Remove'), () => { steps = steps.filter((_, i) => i !== index); paint(steps.length ? `chain-step-${Math.min(index, steps.length - 1)}-agent` : 'chain-add'); }, 'text-button danger-text', ''); remove.dataset.key = `chain-step-${index}-remove`; remove.setAttribute('aria-label', t('Remove: step {n}, {agent}', { n, agent: name }));
      if (index === 0) { up.disabled = true; up.dataset.locked = 'true'; }
      if (index === steps.length - 1) { down.disabled = true; down.dataset.locked = 'true'; }
      moves.append(up, down, remove);
      item.append(controls, moves); list.append(item);
    });
    if (!steps.length) list.append(el('li', 'chain-step is-empty form-note', t('No agents yet. Add the first one.')));
    const atMost = steps.length >= 8; add.disabled = atMost; add.dataset.locked = String(atMost); full.hidden = !atMost;
    add.dataset.key = 'chain-add';
    focusIn(focusKey);
  };
  context.body.append(list, add, full); context.actions.append(submit(t('Save chain')));
  paint(steps.length ? 'chain-step-0-agent' : 'chain-add');
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    if (!steps.length) { context.error.textContent = t('Add an agent, or clear the chain instead.'); add.focus(); return; }
    const executors = steps.map((s) => ({ agent: s.agent, ...(s.account_id ? { account_id: s.account_id } : {}), ...(s.key ? { key: s.key } : {}) }));
    void dialogSave(context, async () => { await adapter.setChain(scope, executors, null); chainsView = await adapter.chains(); }, chainSavedText(scope));
  });
}
/** Focus goes back to the row's Edit… (this computer's for a task row, which goes when cleared). */
function clearChainDialog(scope: ChainScope, returnFocus: string) {
  const question = scope.kind === 'machine' ? t('Clear this computer\'s chain?') : scope.kind === 'project' ? t('Clear the chain of {group}?', { group: scopeLabel(scope) }) : t('Clear the chain of task {id}?', { id: shortWorkflow(scope.id) });
  const consequence = scope.kind === 'machine' ? t('Nothing is handed over while no chain applies.') : scope.kind === 'project' ? t('Its tasks use this computer\'s chain again.') : t('It uses its group\'s or this computer\'s chain again.');
  const context = openDialog(question, consequence, returnFocus);
  const confirm = submit(t('Clear chain')); confirm.className = 'button danger'; context.actions.append(confirm);
  const cleared = scope.kind === 'machine' ? t('This computer\'s chain is cleared.') : scope.kind === 'project' ? t('The chain of {group} is cleared.', { group: scopeLabel(scope) }) : t('The chain of task {id} is cleared.', { id: shortWorkflow(scope.id) });
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { await adapter.setChain(scope, [], null); chainsView = await adapter.chains(); }, cleared); });
  context.actions.querySelector('button')?.focus();
}
/** Registering Switchboard's tools with coding agents (SB-93), shown after the agents. */
function connectPanel() {
  const setup = el('section', 'about-panel'); setup.id = 'connect-panel'; setup.append(el('h2', '', t('Connect an agent')));
  setup.append(el('p', '', t('Coding agents connect to switchboard mcp. They read who handles their requests and how much quota remains, switch the account of a managed session from the next request, and apply a folder rule when they start work in a folder. Changing the ordinary Claude Code login affects every claude session, so agents must pass global: true and should ask you first.')));
  if (agentSetupError || !agentSetup) { setup.append(el('p', 'form-note', agentSetupError ? t('Agent setup is unavailable. Refresh to retry.') : t('Reading agent setup…'))); return setup; }
  if (agentSetup.translocated) setup.append(el('p', 'form-note', runtime?.platform === 'linux' ? t('This Switchboard runs from an AppImage, whose files exist only while it runs. To connect agents, install the .deb package, which puts switchboard on your PATH.') : t('macOS is running Switchboard from a temporary copy of the download. Move Fabric Switchboard to Applications and open it from there before connecting agents; a path into the copy stops working when the app quits.')));
  if (agentSetup.agent_prompt) {
    setup.append(el('h3', '', t('Set up with your coding agent')), el('p', 'form-note', t('Give this to Claude Code or Codex. It does every setup step it can and tells you the ones that are yours: opening Switchboard and signing in.')), copyable(t('Prompt for your agent'), agentSetup.agent_prompt, 'copy-onboard'));
    const facts = el('ul', 'setup-facts');
    for (const fact of agentSetupFacts(agentSetup)) facts.append(el('li', fact.done ? 'is-done' : '', fact.text));
    setup.append(facts, el('h3', '', t('Or register the tools yourself')));
  }
  setup.append(el('p', 'form-note', agentSetup.cli_path ? t('Command-line tool: {path}', { path: agentSetup.cli_path }) : t('The switchboard command-line tool was not found on PATH.')));
  if (agentSetup.can_link && !agentSetup.linked_cli) setup.append(button(t('Link switchboard into ~/.local/bin'), () => void mutate(async () => { await adapter.linkCli(); agentSetup = await adapter.agentSetup(); }, t('The command-line tool is linked. Agents and plugins can now start switchboard mcp.'), 'link-cli'), 'button primary', 'link-cli'));
  setup.append(copyable(t('Claude Code'), agentSetup.commands.claude_code, 'copy-claude'), copyable(t('Codex CLI'), agentSetup.commands.codex, 'copy-codex'), copyable(t('Claude Code plugin (tools and the switching-accounts skill)'), agentSetup.commands.claude_plugin, 'copy-plugin'));
  setup.append(el('p', 'form-note', t('Sessions you launch from Switchboard get these tools automatically when the command-line tool is available. Isolated sessions get the read-only tools.')));
  return setup;
}
async function loadHermes() {
  try { hermes = await adapter.hermesModel(); hermesError = ''; } catch (error) { hermesError = safeError(error, 'read'); }
  backgroundRender();
}
function hermesDialog() {
  const current = hermes!;
  const context = openDialog(t('Change Hermes\'s model or provider'), t('Hermes uses them from its next start. Sessions already running keep theirs.'));
  const options = [...new Set([current.provider, ...current.providers].filter((p): p is string => !!p))];
  const provider = select(options.map((p) => [p, p])); if (current.provider) provider.value = current.provider;
  const model = input(current.model ?? ''); model.placeholder = 'moonshotai/kimi-k2';
  const grid = el('div', 'form-grid'); grid.append(field(t('Provider'), provider), field(t('Model'), model, t('The provider\'s model id; for OpenRouter, vendor/model as on openrouter.ai/models.')));
  context.body.append(grid);
  context.actions.append(submit(t('Save')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const nextProvider = provider.value !== current.provider ? provider.value : null;
    const nextModel = model.value.trim() !== (current.model ?? '') ? model.value.trim() : null;
    void dialogSave(context, async () => { hermes = await adapter.hermesSetModel(nextProvider, nextModel); }, t('Hermes settings saved.'));
  });
  model.focus();
}
/** Reads the key's metadata and asks OpenRouter what it may still spend; never the key. */
async function loadOpenrouter(stamp?: number) {
  try { const [status, keys] = await Promise.all([adapter.openrouterStatus(), adapter.openrouterKeys()]); if (stamp === undefined || clock.accepts(stamp)) { [openrouter, openrouterKeys] = [status, keys]; openrouterError = ''; groupKeyStatus = null; } } catch (error) { openrouterError = safeError(error, 'read'); }
  backgroundRender();
}
/** SB-144: the Tokens page: the key every folder uses, and each project's own. */
function renderTokens(main: HTMLElement) {
  if (!openrouter && !openrouterError) void loadOpenrouter();
  if (openrouterKeys && !groupKeyStatus) loadGroupKeys();
  ceilingBanners(main);
  main.append(el('p', 'surface-note', t('Agents that run on an API key (Hermes, OpenCode, Pi, Kimi Code CLI and others on OpenRouter) take the key of the group whose folder they start in. A folder outside every group, or a group without a key of its own, uses the machine\'s key. Nothing waits: each launch reads its key when it starts.')), openrouterPanel(), projectKeysPanel());
  const tasks = taskCeilingsPanel(); if (tasks) main.append(tasks);
}
/** On Agents, where the key used to be: one line to the Tokens page. */
function tokensPointer() {
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'tokens-pointer');
  const heading = el('h2', '', t('OpenRouter keys')); heading.id = 'tokens-pointer';
  section.append(heading, el('p', '', t('The machine\'s OpenRouter key and each group\'s own key are under API keys.')), button(t('Open API keys'), () => goTo('tokens'), 'button', 'open-api-keys'));
  return section;
}
/** SB-144: each project, with its own key or the machine's. */
function projectKeysPanel() {
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'project-keys-heading');
  const heading = el('h2', '', t('Group keys')); heading.id = 'project-keys-heading';
  section.append(heading, el('p', '', t('Give a group its own key, for example a client\'s, and agents started in its folders spend that key; the others keep the machine\'s.')));
  const projects = openrouterKeys?.projects ?? [];
  if (!openrouterKeys) { section.append(el('p', 'form-note', t('Reading the OpenRouter key…'))); return section; }
  if (!projects.length) { section.append(el('p', 'form-note', t('There are no groups yet. Create one under Groups, then give it a key here.'))); return section; }
  const list = el('ul', 'repository-map key-list'); list.setAttribute('aria-label', t('Group keys'));
  for (const project of projects) {
    const own = openrouterKeys.keys.find((k) => k.pool === project.pool);
    const item = el('li', 'account-meta repository-row'); const text = el('div', 'repository-text');
    text.append(el('span', 'repository-name', project.name), el('span', 'folder-note', ` · ${own ? (own.model ? t('Own key · {model}', { model: own.model }) : t('Own key')) : openrouter?.saved ? t('Uses the machine\'s key') : t('No key: agents here cannot start on OpenRouter')}`));
    item.append(text);
    const target = { pool: project.pool, name: project.name };
    const actions = el('div', 'management-actions');
    if (own) actions.append(button(t('Change model'), () => openrouterModelDialog(target), 'text-button', `key-model-${project.pool}`), button(t('Replace key'), () => openrouterKeyDialog(target), 'text-button', `key-replace-${project.pool}`), button(t('Remove key'), () => openrouterRemoveDialog(target), 'text-button danger-text', `key-remove-${project.pool}`));
    else actions.append(button(t('Add key…'), () => openrouterKeyDialog(target), 'text-button', `key-add-${project.pool}`));
    item.append(actions);
    item.append(groupCeilingLine(project.pool, !!own));
    list.append(item);
  }
  section.append(list);
  return section;
}
/* C-7 (SB-72, SCN-083, SCN-084): spending ceilings in the window. The window is the only surface
 * that may raise, clear or double a ceiling (C-Q1); the question has two answers of equal weight. */
let ceilingsView: Ceilings | null = null;
let ceilingsError = '';
async function loadCeilings() {
  const stamp = clock.stamp();
  try { const view = await adapter.ceilings(); if (clock.accepts(stamp)) { ceilingsView = view; ceilingsError = ''; } } catch (error) { ceilingsError = safeError(error, 'read'); }
  backgroundRender();
}
/** Each group key's status (its own limit at OpenRouter), read once per visit to API keys. */
let groupKeyStatus: Record<string, OpenrouterStatus | null> | null = null; let groupKeysLoading = false;
function loadGroupKeys() {
  if (groupKeysLoading || !openrouterKeys) return;
  groupKeysLoading = true;
  const pools = openrouterKeys.keys.map((k) => k.pool).filter((pool): pool is string => !!pool);
  void Promise.all(pools.map(async (pool) => [pool, await adapter.openrouterKeyStatus(pool).catch(() => null)] as const))
    .then((rows) => { groupKeyStatus = Object.fromEntries(rows); })
    .finally(() => { groupKeysLoading = false; backgroundRender(); });
}
const ceilingOf = (scope: ChainScope): CeilingView | null => ceilingsView?.ceilings.find((c) => scopeKey(c.scope) === scopeKey(scope)) ?? null;
const ceilingAnchor = (scope: ChainScope) => scope.kind === 'machine' ? 'machine' : scope.kind === 'project' ? `project-${scope.pool}` : `workflow-${scope.id}`;
/** One banner per open question, oldest first (SCN-084), on Agents and API keys. */
function ceilingBanners(main: HTMLElement) {
  if (ceilingsError) {
    const failed = el('div', 'notice error stale-list'); failed.setAttribute('role', 'status');
    // The core's own sentence for an unreadable file already says what failed; another error gets the lead.
    const unreadable = ceilingsError.startsWith('Spending ceilings (spend.json) could not be read');
    failed.append(el('span', '', unreadable ? t(ceilingsError) : `${t('Ceiling questions could not be read.')} ${t(ceilingsError)}`), button(t('Retry'), () => void loadCeilings(), 'text-button', 'ceilings-retry'));
    main.append(failed);
  }
  for (const q of ceilingsView?.questions ?? []) main.append(ceilingBanner(q));
}
function ceilingBanner(q: CeilingQuestionView) {
  const question = ceilingQuestion({ value: q.usd, double: q.double_usd, kind: q.scope.kind, scope: q.scope.kind === 'machine' ? undefined : q.scope.kind === 'workflow' ? shortWorkflow(q.scope.id) : scopeLabel(q.scope) });
  const banner = el('section', 'login-banner ceiling-banner'); const id = `ceiling-question-${q.id}`;
  banner.setAttribute('role', 'group'); banner.setAttribute('aria-labelledby', id);
  const copy = el('div', 'login-copy'); const lead = el('strong', '', question.text); lead.id = id;
  copy.append(lead, el('span', '', t('Reached {time}. No paid agent starts past it until you answer.', { time: date(q.reached_at) })));
  if (question.note) copy.append(el('span', '', question.note));
  const actions = el('div', 'login-actions ceiling-actions');
  for (const action of question.actions) {
    const control = button(action.label, () => void answerCeiling(q, action.answer), action.className, `ceiling-${action.answer}-${q.id}`);
    if (action.disabled) { control.disabled = true; control.dataset.locked = 'true'; }
    actions.append(control);
  }
  banner.append(copy, actions);
  return banner;
}
function answerCeiling(q: CeilingQuestionView, answer: 'keep' | 'double') {
  const usd = dollars(answer === 'double' ? q.double_usd ?? q.usd * 2 : q.usd);
  const group = scopeLabel(q.scope); const task = q.scope.kind === 'workflow' ? shortWorkflow(q.scope.id) : '';
  const success = answer === 'double'
    ? (q.scope.kind === 'machine' ? t('This computer\'s daily ceiling is now {usd}.', { usd }) : q.scope.kind === 'project' ? t('The daily ceiling of {group} is now {usd}.', { group, usd }) : t('The ceiling of task {id} is now {usd}.', { id: task, usd }))
    : (q.scope.kind === 'machine' ? t('This computer\'s daily ceiling stays {usd}. Paid agents wait until it resets.', { usd }) : q.scope.kind === 'project' ? t('The daily ceiling of {group} stays {usd}. Paid agents wait until it resets.', { group, usd }) : t('The ceiling of task {id} stays {usd}.', { id: task, usd }));
  void mutate(async () => { await adapter.answerCeiling(q.id, answer); ceilingsView = await adapter.ceilings(); }, success, 'page-title');
}
/** The ceiling of this computer (machine panel) or a group (its key row), beside the key's own limit. */
function ceilingBlock(scope: ChainScope, credit: OpenrouterCredit | null) {
  const block = el('div', 'ceiling-block'); const anchor = ceilingAnchor(scope);
  if (!ceilingsView) { block.append(el('p', 'form-note', ceilingsError ? t('Daily ceiling: not read') : t('Reading spending ceilings…'))); return block; }
  const ceiling = ceilingOf(scope);
  block.append(el('p', 'ceiling-line', ceilingLine(ceiling)));
  if (scope.kind === 'machine') block.append(el('p', 'form-note', t('Counts every OpenRouter key saved in Switchboard, added up per UTC day.')));
  else if (credit) block.append(el('p', 'form-note', keyLimitLine(credit)));
  const warning = ceiling?.active ? ceilingWarning(ceiling.usd, credit) : null;
  if (warning) block.append(el('p', 'ceiling-warning', warning));
  const actions = el('div', 'management-actions');
  const canSet = scope.kind === 'machine' || !!ceiling?.active || (scope.kind === 'project' && !!openrouterKeys?.keys.some((k) => k.pool === scope.pool));
  if (canSet) actions.append(button(t('Set ceiling…'), () => ceilingDialog(scope, ceiling, `ceiling-set-${anchor}`), scope.kind === 'machine' ? 'button' : 'text-button', `ceiling-set-${anchor}`));
  if (ceiling) actions.append(button(t('Clear'), () => clearCeilingDialog(scope, ceiling, canSet ? `ceiling-set-${anchor}` : 'page-title'), 'text-button danger-text', `ceiling-clear-${anchor}`));
  block.append(actions);
  return block;
}
/** A group key row's ceiling: its line with the key's own limit, or why it has none. */
function groupCeilingLine(pool: string, own: boolean) {
  if (own || ceilingOf({ kind: 'project', pool })) return ceilingBlock({ kind: 'project', pool }, own ? groupKeyStatus?.[pool]?.credit ?? null : null);
  const block = el('div', 'ceiling-block'); block.append(el('p', 'form-note', t('A daily ceiling needs the group\'s own key.'))); return block;
}
/** Task ceilings (set from the CLI or MCP): listed read-only, cleared only here. */
function taskCeilingsPanel() {
  const tasks = (ceilingsView?.ceilings ?? []).filter((c) => c.scope.kind === 'workflow');
  if (!tasks.length) return null;
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'task-ceilings-heading');
  const heading = el('h2', '', t('Task ceilings')); heading.id = 'task-ceilings-heading';
  section.append(heading, el('p', '', t('Counted on the key since the first paid hand-over; other work on the same key counts too.')), el('p', 'form-note', t('Set from the command line: switchboard continue --ceiling <usd>.')));
  const list = el('ul', 'repository-map key-list'); list.setAttribute('aria-label', t('Task ceilings'));
  for (const c of tasks) {
    const item = el('li', 'account-meta repository-row'); const text = el('div', 'repository-text');
    text.append(el('span', 'repository-name', scopeLabel(c.scope)), el('span', 'folder-note', ` · ${dollars(c.usd)}`));
    const anchor = ceilingAnchor(c.scope);
    item.append(text, button(t('Clear'), () => clearCeilingDialog(c.scope, c, 'page-title'), 'text-button danger-text', `ceiling-clear-${anchor}`));
    list.append(item);
  }
  section.append(list);
  return section;
}
function ceilingDialog(scope: ChainScope, current: CeilingView | null, returnFocus: string) {
  const name = scopeLabel(scope);
  const context = openDialog(scope.kind === 'machine' ? t('Daily ceiling for this computer') : t('Daily ceiling for {group}', { group: name }), scope.kind === 'machine' ? t('Counts every OpenRouter key saved in Switchboard, added up per UTC day.') : t('Counts this group\'s own key, per UTC day.'), returnFocus);
  const amount = input(current ? current.usd.toFixed(2) : '', 'number'); amount.min = '0.01'; amount.max = '1000'; amount.step = '0.01'; amount.inputMode = 'decimal';
  const grid = el('div', 'form-grid');
  grid.append(field(t('Dollars per UTC day'), amount, t('From $0.01 to $1,000. When it is reached, no paid agent starts until you answer.')));
  context.body.append(grid); context.actions.append(submit(t('Save ceiling')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const value = Number(amount.value);
    if (!Number.isFinite(value) || value < 0.01 || value > 1000) { context.error.textContent = t('Enter an amount from $0.01 to $1,000.'); amount.focus(); return; }
    const usd = dollars(Math.round(value * 100) / 100);
    void dialogSave(context, async () => { await adapter.setCeiling(scope, Math.round(value * 100) / 100); ceilingsView = await adapter.ceilings(); }, scope.kind === 'machine' ? t('Daily ceiling for this computer: {usd}.', { usd }) : t('Daily ceiling for {group}: {usd}.', { group: name, usd }));
  });
  amount.focus();
}
function clearCeilingDialog(scope: ChainScope, ceiling: CeilingView, returnFocus: string) {
  const name = scopeLabel(scope); const usd = dollars(ceiling.usd);
  const question = scope.kind === 'machine' ? t('Remove this computer\'s daily ceiling of {usd}?', { usd }) : scope.kind === 'project' ? t('Remove the daily ceiling of {group}, {usd}?', { group: name, usd }) : t('Remove the ceiling of task {id}, {usd}?', { id: shortWorkflow(scope.id), usd });
  const context = openDialog(question, t('Paid agents are then bound only by the key\'s own limit.'), returnFocus);
  const confirm = submit(t('Remove ceiling')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { await adapter.setCeiling(scope, null); ceilingsView = await adapter.ceilings(); }, t('Ceiling removed.')); });
  context.actions.querySelector('button')?.focus();
}
const openrouterAgents = () => (agentCatalog as { agents: AgentInfo[] }).agents.filter((agent) => agent.openrouter);
/** SB-79: one OpenRouter key that agents launched from Switchboard run on (SCN-045). */
function openrouterPanel() {
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'openrouter-heading');
  const heading = el('h2', '', t('Machine key · OpenRouter')); heading.id = 'openrouter-heading';
  const names = openrouterAgents().map((agent) => agent.name).join(', ');
  section.append(heading, el('p', '', t('Save one OpenRouter key, and these agents launch on it with the model you choose: {agents}.', { agents: names }) + ' ' + (onWindows() ? t('The key stays in Windows Credential Manager; an agent receives it only when it starts.') : onLinux() ? t('The key stays in the Secret Service keyring; an agent receives it only when it starts.') : t('The key stays in the Keychain; an agent receives it only when it starts.'))));
  if (openrouterError && !openrouter) { section.append(el('p', 'usage-error', t(openrouterError)), button(t('Retry'), () => void loadOpenrouter(), 'text-button', 'openrouter-retry')); return section; }
  if (!openrouter) { section.append(el('p', 'form-note', t('Reading the OpenRouter key…'))); return section; }
  if (!openrouter.saved) {
    section.append(el('p', 'form-note', t('No key is saved.')), button(t('Add OpenRouter key'), () => openrouterKeyDialog(), 'button primary', 'openrouter-add'));
    // This computer's ceiling adds up every saved key, so it is shown while a group's key is saved too.
    if (openrouterKeys?.keys.length || ceilingOf({ kind: 'machine' })) { const ceiling = ceilingBlock({ kind: 'machine' }, null); ceiling.classList.add('is-machine'); section.append(ceiling); }
    return section;
  }
  const facts = el('div', 'openrouter-facts');
  facts.append(el('p', '', openrouter.model ? t('Default model: {model}', { model: openrouter.model }) : t('No default model: each launch names one.')));
  if (openrouter.credit) facts.append(el('p', '', openrouterCreditLine(openrouter.credit)));
  else if (openrouter.credit_error) facts.append(el('p', 'usage-error', t(openrouter.credit_error)));
  section.append(facts);
  const actions = el('div', 'management-actions');
  actions.append(button(t('Change model'), () => openrouterModelDialog(), 'button', 'openrouter-model'), button(t('Replace key'), () => openrouterKeyDialog(), 'button', 'openrouter-replace'), button(t('Remove key'), () => openrouterRemoveDialog(), 'button danger', 'openrouter-remove'));
  section.append(actions, el('p', 'form-note', t('Launch an agent on the key from Agents: its Set up button under Installed. CLI: switchboard agents launch <agent> --openrouter.')));
  // C-7: this computer's spending ceiling, set apart from the key's own actions.
  const ceiling = ceilingBlock({ kind: 'machine' }, openrouter.credit); ceiling.classList.add('is-machine'); section.append(ceiling);
  return section;
}
/** Saves (or replaces) the key; it travels to the vault once and is never shown again. */
function openrouterKeyDialog(target?: { pool: string; name: string }) {
  const own = target ? openrouterKeys?.keys.find((k) => k.pool === target.pool) : null;
  const replacing = target ? !!own : !!openrouter?.saved;
  const context = openDialog(target ? (replacing ? t('Replace the key of “{name}”', { name: target.name }) : t('A key for “{name}”', { name: target.name })) : replacing ? t('Replace the OpenRouter key') : t('Add an OpenRouter key'), t('Create a key on openrouter.ai/settings/keys; a spending limit on it caps what agents can spend.') + (target ? ' ' + t('Agents started in this group\'s folders run on it.') : ''));
  const key = input('', 'password'); key.placeholder = 'sk-or-…';
  const picker = modelPicker((target ? own?.model : openrouter?.model) ?? '', 'key-model'); const model = picker.value;
  const grid = el('div', 'form-grid');
  grid.append(field(t('Key'), key, t('Starts with sk-or-. Switchboard never shows it again.')));
  context.body.append(grid, el('h3', 'pool-heading', t('Default model')), picker.node);
  context.actions.append(submit(replacing ? t('Replace key') : t('Save key')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const value = key.value.trim(); key.value = '';
    void dialogSave(context, async () => { await adapter.openrouterSave(value, model.value.trim() || null, target?.pool ?? null); await loadOpenrouter(); }, target ? t('Key saved for “{name}”. Agents started in its folders run on it.', { name: target.name }) : t('OpenRouter key saved. Agents launched on it receive it when they start.'));
  });
  key.focus();
}
let openrouterModelList: OpenrouterModels | null = null;
/** SB-119: an OpenRouter model field — the recommended models first, every tool-capable
 * OpenRouter model as suggestions while typing, and any id typed by hand. */
function modelPicker(initial: string, key: string) {
  const value = input(initial); value.required = false; value.placeholder = 'xiaomi/mimo-v2.6-pro'; value.setAttribute('list', `${key}-models`); value.autocomplete = 'off';
  const list = el('datalist'); list.id = `${key}-models`;
  const cards = el('div', 'model-cards'); cards.setAttribute('role', 'group'); cards.setAttribute('aria-label', t('Recommended models'));
  const note = el('p', 'form-note', '');
  // A failed read is shown in this picker only and never kept: the next picker asks again.
  let data = openrouterModelList;
  const paint = () => {
    const recommended = data?.recommended.length ? data.recommended : (agentCatalog as unknown as { openrouter_recommended: OpenrouterModels['recommended'] }).openrouter_recommended;
    cards.replaceChildren(...recommended.map((m) => {
      const on = value.value.trim() === m.id;
      const card = button('', () => { value.value = m.id; paint(); }, `model-card${on ? ' is-on' : ''}`, `${key}-model-${m.id}`);
      card.setAttribute('aria-pressed', String(on));
      card.append(el('strong', '', m.name), el('span', 'form-note', modelMeta(m)), el('code', 'agent-command', m.id));
      return card;
    }));
    list.replaceChildren(...(data?.models ?? []).map((m) => { const o = el('option'); o.value = m.id; if (m.name) o.label = m.name; return o; }));
    note.textContent = !data ? t('Reading OpenRouter\'s models…') : data.available ? t('{n} OpenRouter models that can call tools: type to search, or enter any id.', { n: data.models.length }) : t('OpenRouter\'s list is unavailable now; the recommended models and typed ids still work.');
  };
  value.addEventListener('input', paint);
  paint();
  if (!data) void adapter.openrouterModels().then((m) => { if (m.available) openrouterModelList = m; data = m; paint(); }, () => { data = { recommended: [], models: [], available: false }; paint(); });
  const node = el('div', 'model-picker');
  node.append(el('strong', 'block', t('Recommended models')), cards, field(t('Model'), value, ''), list, note);
  return { node, value };
}
function openrouterModelDialog(target?: { pool: string; name: string }) {
  const context = openDialog(target ? t('Default model of “{name}”', { name: target.name }) : t('Default model'), t('The OpenRouter model agents start on unless a launch names another.'));
  const picker = modelPicker((target ? openrouterKeys?.keys.find((k) => k.pool === target.pool)?.model : openrouter?.model) ?? '', 'default-model');
  context.body.append(picker.node);
  context.actions.append(submit(t('Save model')));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); const model = picker.value.value.trim(); void dialogSave(context, async () => { await adapter.openrouterModel(model, target?.pool ?? null); await loadOpenrouter(); agentsView = await adapter.agentsOverview().catch(() => agentsView); }, t('Agents launch on {model}.', { model })); });
  picker.value.focus();
}
function openrouterRemoveDialog(target?: { pool: string; name: string }) {
  const context = openDialog(target ? t('Remove the key of “{name}”?', { name: target.name }) : t('Remove the OpenRouter key?'), (onWindows() ? t('Switchboard deletes it from Windows Credential Manager.') : onLinux() ? t('Switchboard deletes it from the Secret Service keyring.') : t('Switchboard deletes it from the Keychain.')) + ' ' + t('Agents already running keep working; new launches need a key again.'));
  const confirm = submit(t('Remove key')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { await adapter.openrouterRemove(target?.pool ?? null); await loadOpenrouter(); }, target ? t('The key of “{name}” is removed; its agents use the machine\'s key again.', { name: target.name }) : t('OpenRouter key removed.')); });
  context.actions.querySelector('button')?.focus();
}
async function agentDialog(agent: AgentInfo) {
  const context = openDialog(t('Set up {agent}', { agent: agent.name }), agent.level === 'mcp' ? t('This agent can use Switchboard\'s tools; its model requests go to its own service.') : t('Register Switchboard\'s tools, then point the agent at Switchboard\'s proxy for one group.'));
  const pools = [...new Set(snapshot!.accounts.map((a) => a.pool))];
  const pool = select((pools.length ? pools : ['default']).map((p) => [p, poolName(p, snapshot!.projects)]));
  const out = el('div', 'agent-setup');
  const show = async () => {
    out.replaceChildren(el('p', 'form-note', t('Reading the setup…')));
    let info: AgentConnection;
    try { info = await adapter.agentConnect(agent.id, pool.value); } catch (error) { out.replaceChildren(el('p', 'usage-error', t(safeError(error, 'read')))); return; }
    out.replaceChildren();
    if (info.mcp.add_command) out.append(copyable(t('Register the tools'), info.mcp.add_command, `mcp-${agent.id}`));
    if (info.mcp.config_snippet) out.append(copyable(t('Or add to {place}', { place: info.mcp.config_path ?? t('its config') }), info.mcp.config_snippet, `mcp-file-${agent.id}`));
    if (!info.mcp.supported) out.append(el('p', 'form-note', t('This agent has no MCP support.')));
    if (info.anthropic) {
      const env = Object.entries(info.anthropic.env).map(([k, v]) => `export ${k}="${v}"`).join('\n');
      if (env) out.append(copyable(t('Environment'), env, `env-${agent.id}`));
      if (info.anthropic.config_snippet) out.append(copyable(t('Provider in its config'), info.anthropic.config_snippet, `cfg-${agent.id}`));
      else if (!env) out.append(copyable(t('Anthropic-compatible endpoint'), info.anthropic.base_url, `base-${agent.id}`));
    }
    if (info.openai) out.append(copyable(t('OpenAI-compatible endpoint'), info.openai.base_url, `oai-${agent.id}`));
    if (info.key_command) out.append(copyable(t('Key (prints it; nothing to store)'), info.key_command, `key-${agent.id}`));
    for (const line of [info.requires, info.warning, info.notes]) if (line) out.append(el('p', 'form-note', line));
    if (info.launch) out.append(copyable(t('Launch from the command line'), info.launch, `launch-cmd-${agent.id}`));
  };
  pool.addEventListener('change', () => void show());
  const grid = el('div', 'form-grid'); grid.append(field(t('Group'), pool, t('The group whose selected API-key account this agent uses.')));
  context.body.append(grid, out);
  const launches = agent.level !== 'mcp' && !!agent.binary;
  const dir = folderPicker(context, lastWorkingDirectory, `agent-folder-${agent.id}`);
  const folder = () => dir.require();
  const viaKey = agent.openrouter ? openrouterLaunch(agent, context, folder) : null;
  // The operator's choice (2026-10-08): agents with a recipe start on the OpenRouter key by
  // default; one whose recipe says `default: false` (Kimi Code, on its subscription) keeps it second.
  const keyFirst = !!viaKey?.run && agent.openrouter?.default !== false;
  if (launches || viaKey) context.body.append(groupField(t('Launch in folder'), dir.node, launches && !keyFirst ? t('Starts the agent in Terminal in this folder, on the group\'s API-key account.') : t('Starts the agent in Terminal in this folder.')));
  if (viaKey) context.body.append(viaKey.part);
  const throughProxy = () => { const value = folder(); if (value) void dialogSave(context, () => adapter.launchAgent(agent.id, pool.value, value), t('Terminal launch requested for {agent}.', { agent: agent.name }), 'launch'); };
  if (keyFirst) {
    if (launches) context.actions.append(button(t('Launch through the proxy'), throughProxy, 'button', `proxy-launch-${agent.id}`));
    context.actions.append(submit(t('Launch on OpenRouter')));
    context.form.addEventListener('submit', (event) => { event.preventDefault(); viaKey!.run!(); });
  } else {
    if (viaKey?.run) viaKey.part.append(button(t('Launch on OpenRouter'), viaKey.run, 'button', `openrouter-launch-${agent.id}`));
    if (launches) {
      context.actions.append(submit(t('Launch {agent}', { agent: agent.name })));
      context.form.addEventListener('submit', (event) => { event.preventDefault(); throughProxy(); });
    }
  }
  void show();
}
/** SB-79: an agent with an OpenRouter recipe launches on the saved key, with this launch's model.
 * `run` is null while no key is saved. */
function openrouterLaunch(agent: AgentInfo, context: DialogContext, folder: () => string | null): { part: HTMLElement; run: (() => void) | null } {
  const part = el('div', 'agent-setup');
  part.append(el('h3', 'pool-heading', t('On the OpenRouter key')));
  if (!openrouter?.saved) {
    part.append(el('p', 'form-note', t('Save an OpenRouter key under API keys first; then {agent} launches on it.', { agent: agent.name })), button(t('Open API keys'), () => { context.close(); goTo('tokens'); }, 'button', `open-api-keys-${agent.id}`));
    return { part, run: null };
  }
  const picksItself = !agent.openrouter?.model_flag?.length && !agent.openrouter?.model_env;
  const picker = modelPicker(openrouter.model ?? '', `launch-${agent.id}`); const model = picker.value;
  if (picksItself) part.append(el('p', 'form-note', t('{agent} picks its model in its own interface; the key comes from Switchboard.', { agent: agent.name })));
  else part.append(el('p', 'form-note', openrouter.model ? t('Model for this launch. Default: {model}.', { model: openrouter.model }) : t('Model for this launch. No default model is saved: name one.')), picker.node);
  if (agent.openrouter?.notes) part.append(el('p', 'form-note', agent.openrouter.notes));
  const run = () => {
    const value = folder(); if (!value) return;
    void dialogSave(context, () => adapter.launchOnOpenrouter(agent.id, picksItself ? null : model.value.trim() || null, value), t('Terminal launch requested for {agent} on OpenRouter.', { agent: agent.name }), 'launch');
  };
  return { part, run };
}
function emptyState(title: string, description: string) { const section = el('section', 'empty-state'); section.append(el('div', 'empty-symbol', '◇'), el('h2', '', title), el('p', '', description)); return section; }
// ── Accounts (0.5): one click to add, compact grouped rows, row menus instead of dialogs ──
const signInRequired = (account: Account) => !!monitor?.sign_in_required?.includes(account.id);
const accountLimit = (account: Account) => monitor?.limited?.find((entry) => entry.account_id === account.id && entry.until > Date.now() / 1000);
// #region quota-countdown — docs: docs/runs/2026-10-04-quota-review/README.md
let countdownPaused = false;
/** SB-158: the Accounts filter: `all`, `personal` or an organization's id. */
let orgFilter = 'all';
const pausedCountdowns = new Map<string, string>();
/** A countdown node's text: the compact card form ("2h 5m") or the long disclosure form. */
const countdownText = (node: HTMLElement, now: number) => node.dataset.format === 'compact' ? compactCountdown(Number(node.dataset.countdown), now) : resetCountdown(Number(node.dataset.countdown), now);
const countdownKey = (node: HTMLElement) => `${node.dataset.countdown}|${node.dataset.format ?? 'long'}`;
/** A tick touches text only: no focus movement, list reordering or provider request. */
const countdownPoll = intervalWhile(() => {
  const now = Date.now() / 1000;
  root.querySelectorAll<HTMLElement>('[data-countdown]').forEach(node => { node.textContent = countdownText(node, now); });
}, 60_000);
function syncCountdown() {
  countdownPoll.sync(!countdownPaused && !document.hidden && page === 'accounts' && !!root.querySelector('[data-countdown]'));
}
document.addEventListener('visibilitychange', () => {
  if (!document.hidden && !countdownPaused) root.querySelectorAll<HTMLElement>('[data-countdown]').forEach(node => { node.textContent = countdownText(node, Date.now() / 1000); });
  syncCountdown();
});
/** `text` words the time as one sentence per language (L10N-02), never a label joined to a date. */
function resetDisplay(until: number, text = (when: string) => t('Resets {date}', { date: when })) {
  if (resetCountdown(until, Date.now() / 1000) === t('Time unavailable')) return el('span', 'usage-unknown', t('Time unavailable'));
  const wrapper = el('span', 'reset-display');
  const time = el('time', '', text(date(until))); time.dateTime = new Date(until * 1000).toISOString();
  time.title = new Date(until * 1000).toLocaleString(dateLocale(), { timeZoneName: 'short' });
  const remaining = el('span', 'reset-remaining', countdownPaused ? pausedCountdowns.get(`${until}|long`) ?? t('Countdown paused') : resetCountdown(until, Date.now() / 1000));
  remaining.dataset.countdown = String(until);
  // Informational timer: screen readers can inspect it without minute-by-minute announcements.
  remaining.setAttribute('aria-live', 'off'); wrapper.append(time, remaining); return wrapper;
}
/** The project that reserves `pool`, if any. */
const projectOf = (pool: string) => snapshot?.projects?.find((p) => p.pool === pool);
const quotaContext = () => ({ nowSeconds: Date.now() / 1000, policies: snapshot?.policies, limits: monitor?.limited, signInRequired: monitor?.sign_in_required });
// #endregion quota-countdown
/** A menu is part of the render: `openMenu` names the one that is open. */
function menu(key: string, label: string, items: ([string, () => void] | [string, () => void, string])[], triggerClass = 'icon-button', text = '⋯') {
  const wrapper = el('div', 'menu-root');
  const trigger = button(text, () => { openMenu = openMenu === key ? null : key; render(); if (openMenu === key) document.querySelector<HTMLElement>(`[data-menu="${key}"] [role="menuitem"]`)?.focus(); else restoreFocus(`menu-${key}`); }, triggerClass, `menu-${key}`);
  trigger.setAttribute('aria-haspopup', 'menu'); trigger.setAttribute('aria-expanded', String(openMenu === key)); trigger.setAttribute('aria-label', label);
  wrapper.append(trigger);
  if (openMenu === key) {
    const list = el('div', 'menu'); list.setAttribute('role', 'menu'); list.setAttribute('aria-label', label); list.dataset.menu = key;
    items.forEach(([text, action, tone], index) => {
      // VIS-27: a line sets a destructive item apart from the ordinary ones above it.
      if (tone?.includes('danger') && index > 0) { const line = el('div', 'menu-separator'); line.setAttribute('role', 'separator'); list.append(line); }
      // UX-05: focus returns to the trigger before the action, so a dialog it opens gives focus back there.
      const item = button(text, () => { openMenu = null; render(); restoreFocus(`menu-${key}`); action(); }, `menu-item ${tone ?? ''}`); item.setAttribute('role', 'menuitem'); item.tabIndex = -1; list.append(item);
    });
    list.addEventListener('keydown', (event) => {
      const entries = [...list.querySelectorAll<HTMLElement>('[role="menuitem"]')]; const index = entries.indexOf(document.activeElement as HTMLElement);
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); entries[(index + (event.key === 'ArrowDown' ? 1 : entries.length - 1)) % entries.length]?.focus(); }
      else if (event.key === 'Home') { event.preventDefault(); entries[0]?.focus(); }
      else if (event.key === 'End') { event.preventDefault(); entries[entries.length - 1]?.focus(); }
    });
    wrapper.append(list);
  }
  return wrapper;
}
function addMenu(className = 'button primary') {
  return menu('add', t('Add account'), [
    [t('Sign in to another Claude account'), () => void startLogin('claude')],
    [t('Sign in to another Codex account'), () => void startLogin('codex')],
    [t('Import from Claude Swap'), () => void importSwap()],
    [t('Add an API key or token…'), () => addDialog()],
  ], className, t('+ Add account'));
}
/** Official sign-in without a form: Terminal opens, completion is detected, the email names the account. */
async function startLogin(provider: Provider, label = '', pool = 'default') {
  if (pendingLogin) { showNotice(t('A sign-in is already in progress. Finish it in Terminal or cancel it first.'), true); render(); return; }
  busy = true; notice = ''; render();
  try {
    const result = await adapter.beginLogin({ provider, label, pool });
    pendingLogin = { id: result.login_id, provider, label, pool, state: 'pending', error: '' };
    announce(t('Sign in to {provider} in the Terminal window that opened. Switchboard adds the account when you finish.', { provider: providerName(provider) }));
  } catch (error) { showNotice(safeError(error, 'sign_in'), true); }
  finally { busy = false; render(); restoreFocus('login-cancel'); }
}
/** SB-65: a sign-in the owner holds — begun before this window opened, or taken back after a restart — shows its banner again and finishes as usual. */
async function restorePendingLogin() {
  if (pendingLogin) return;
  try {
    const { logins } = await adapter.pendingLogins();
    const login = logins.at(-1);
    if (!login || pendingLogin) return;
    pendingLogin = { id: login.login_id, provider: login.provider, label: login.label, pool: login.pool, state: login.state === 'ended' ? 'ended' : 'pending', error: '' };
    render();
  } catch { /* The banner only returns when the owner can say which sign-in it holds. */ }
}
async function pollLogin() {
  const login = pendingLogin;
  // A failed finish waits for Retry instead of repeating under the owner lock every tick.
  if (!login || login.state !== 'pending' || login.error || busy) return;
  let state: 'pending' | 'complete' | 'ended';
  try { state = (await adapter.loginStatus(login.id)).state; }
  catch (error) {
    // The owner restarted and forgot this sign-in: nothing can finish it any more.
    if (loginOutcome(safeError(error)) === 'forgotten' && pendingLogin === login) { login.state = 'ended'; render(); }
    return;
  }
  if (pendingLogin !== login || login.state !== 'pending') return;
  if (state === 'ended') { login.state = 'ended'; render(); announce(t('Sign-in ended in Terminal without adding an account.')); return; }
  if (state !== 'complete') return;
  login.state = 'finishing'; render();
  busy = true; clock.begin();
  try {
    const account = await adapter.finishLogin(login.id);
    // Saved either way; a pending cleanup is reported, never a failed sign-in (SB-42).
    await loginSaved(signInNotice(account, providerName(account.provider), snapshot?.projects));
    // SB-160 (SCN-077): a seat outside its organization's one group is offered it; nothing moves until pressed.
    const move = signInMove(account);
    if (move) noticeAction = { label: move.label, run: () => void mutate(() => adapter.moveAccount(move.id, move.pool), move.done, 'notice-action') };
  } catch (error) {
    const text = safeError(error, 'sign_in'); const outcome = loginOutcome(text);
    // The owner saved the account and released the sign-in; only its staging folder is left.
    // Defensive: finish_login reports a pending cleanup in its receipt today; an older owner
    // answered with this refusal instead, and the account is saved either way.
    if (outcome === 'saved_with_cleanup') await loginSaved(t('Account added to {provider}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.', { provider: providerName(login.provider) }));
    else if (outcome === 'forgotten') login.state = 'ended';
    else { login.state = 'pending'; login.error = text; }
  }
  finally { clock.end(); busy = false; render(); }
}
/** The account is saved: the banner closes, then the list and the current CLI accounts refresh. */
async function loginSaved(text: string) {
  pendingLogin = null;
  await refreshContext();
  try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = t('The account was added, but the list could not be refreshed. Retry loading accounts.'); }
  showNotice(text);
  loadOnboarding();
}
async function cancelLogin() {
  const login = pendingLogin; if (!login) return;
  login.cancelError = ''; busy = true; render();
  try { await adapter.cancelLogin(login.id); pendingLogin = null; showNotice(t('Sign-in cancelled. No account was added.')); }
  catch (error) {
    const text = safeError(error);
    // The owner restarted and no longer knows this sign-in: there is nothing left to cancel.
    if (loginOutcome(text) === 'forgotten') { if (pendingLogin === login) pendingLogin = null; showNotice(t('Sign-in closed. Switchboard had already ended it; close its Terminal window if it is still open.')); }
    // A refused cancel is not a failed finish: the poll keeps watching Terminal (SB-83).
    else login.cancelError = text;
  }
  finally { busy = false; render(); }
}
function loginBanner(main: HTMLElement) {
  const login = pendingLogin; if (!login) return;
  const banner = el('section', `login-banner ${login.state === 'ended' || login.error ? 'is-error' : ''}`); banner.setAttribute('role', 'status');
  const text = el('div', 'login-copy');
  if (login.state === 'ended') text.append(el('strong', '', t('Sign-in ended without an account')), el('span', '', t('The Terminal session closed before the sign-in finished. Start again when you are ready.')));
  else if (login.state === 'finishing') text.append(el('strong', '', t('Adding the account…')), el('span', '', t('Reading the new sign-in from its private home.')));
  else text.append(el('strong', '', t('Signing in to {provider}', { provider: providerName(login.provider) })), el('span', '', t('Finish in the Terminal window. If a browser opens, complete that step first. Switchboard adds the account on its own.')));
  if (login.error) text.append(el('span', 'usage-error', t(login.error)));
  if (login.cancelError) text.append(el('span', 'usage-error', t(login.cancelError)));
  const actions = el('div', 'login-actions');
  if (login.error && login.state === 'pending') actions.append(button(t('Retry'), () => { login.error = ''; render(); void pollLogin(); }, 'button', 'login-retry-finish'));
  if (login.state === 'ended') actions.append(button(t('Try again'), () => { if (retrying) return; retrying = true; const { provider, label, pool } = login; void adapter.cancelLogin(login.id).then(() => true, (error) => loginOutcome(safeError(error)) === 'forgotten' || (showNotice(safeError(error), true), render(), false)).then((cleared) => { if (!cleared) return; pendingLogin = null; void startLogin(provider, label, pool); }).finally(() => { retrying = false; }); }, 'button', 'login-retry'));
  actions.append(button(login.state === 'ended' ? t('Dismiss') : t('Cancel'), () => void cancelLogin(), 'button quiet', 'login-cancel'));
  banner.append(text, actions); main.append(banner);
}
async function importSwap() {
  busy = true; notice = ''; clock.begin(); render();
  try {
    const result = await adapter.importClaudeSwap('default');
    await refreshContext();
    try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = t('Import completed, but accounts could not be refreshed. Refresh to load the imported accounts.'); }
    const parts = [plural(result.imported.length, { one: '{n} Claude Swap account imported or updated in the default group', other: '{n} Claude Swap accounts imported or updated in the default group' })];
    if (result.skipped) parts.push(t('{n} skipped', { n: result.skipped }));
    if (result.failed) parts.push(t('{n} could not be read; check them in Claude Swap and import again', { n: result.failed }));
    if (result.claude_swap_running) parts.push(t('Claude Swap is still running, so it keeps renewing those accounts and Switchboard follows its newest sign-ins'));
    showNotice(`${parts.join(' · ')}.`, result.failed > 0);
  } catch (error) { showNotice(safeError(error), true); }
  finally { clock.end(); busy = false; render(); restoreFocus('menu-add'); }
}
function switchNative(account: Account) {
  void mutate(() => adapter.activateNative(account.id), demo ? t('Synthetic switch: Claude Code now uses {label}. No local credentials were read or written.', { label: account.label }) : t('Claude Code now uses {label}. New claude sessions start on it; a running session may need a restart.', { label: account.label }), `primary-${account.id}`);
}
function renderAccounts(main: HTMLElement) {
  // VIS-13: the provider terms and the active folder rules share one compact block above the accounts.
  const status = el('section', 'status-stack'); status.setAttribute('aria-label', t('Notices'));
  termsBanner(status);
  rulesStrip(status);
  if (status.childElementCount) main.append(status);
  renderCurrent(main);
  renderPolicies(main);
  const accounts = snapshot!.accounts;
  if (!accounts.length) {
    // A reinstall with a backup this computer can open: one press brings everything back.
    const backup = backupStatus?.backups.filter((b) => b.openable && b.accounts > 0).sort((a, b) => b.created_at - a.created_at)[0];
    // VIS-21: an organization card above with its own action is the screen's one primary; the page's buttons go quiet.
    const cardLeads = orgSetups(onboarding).some((setup) => !!orgSetupAction(setup) && orgSetupShown({ page, organization: setup.organization.id, hidden: orgSetupHidden, usage: analyticsState, usageFailed: analyticsError, tourStep }));
    // SB-202 (B-8): an organization's group with no seat yet leads with the seat.
    const lead = accountsEmptyKey({ backup: !!backup, onboarding, current: currentAccounts });
    const empty = emptyState(t('Start with one account'), lead.lead === 'org_seat' ? lead.line : backup ? plural(backup.accounts, { one: 'A backup from {date} with {n} account is on this computer. Restore it to get your accounts, groups and settings back, or add an account.', other: 'A backup from {date} with {n} accounts is on this computer. Restore it to get your accounts, groups and settings back, or add an account.' }, { date: date(backup.created_at) }) : lead.lead === 'sign_in' ? t('Neither Claude Code nor Codex CLI is signed in on this computer. Press + Add account to sign in: Terminal opens the official sign-in, and Switchboard adds the account when you finish.') : t('Add the account Claude Code or Codex CLI already uses with Add to Switchboard above, or sign in to another account with + Add account. Nothing opens until you choose.'));
    if (backup) empty.append(button(t('Restore from backup'), () => void mutate(async () => { const result = await adapter.restoreBackup(backup.file); backupStatus = await adapter.backups(); showRestored(result); }, t('Backup restored.'), 'empty-restore'), 'button primary', 'empty-restore'));
    if (lead.lead === 'org_seat') { const pool = lead.pool; empty.append(button(lead.label, () => void startLogin('claude', '', pool), cardLeads ? 'button quiet' : 'button primary', 'empty-org-sign-in'), addMenu('button quiet')); main.append(empty, kimiSection()); return; }
    empty.append(addMenu(cardLeads ? 'button quiet' : 'button primary')); main.append(empty, kimiSection()); return;
  }
  const orgBar = organizationsBar(accounts); if (orgBar) main.append(orgBar);
  const shown = accounts.filter((a) => inOrganizationFilter(snapshot!, a, orgFilter));
  const sorting = el('div', 'account-sort-note');
  sorting.append(el('span', '', t('Within each group: remaining quota first, then shortest wait. Unknown usage follows.')), button(countdownPaused ? t('Resume countdown') : t('Pause countdown'), () => {
    if (!countdownPaused) { pausedCountdowns.clear(); root.querySelectorAll<HTMLElement>('[data-countdown]').forEach(node => pausedCountdowns.set(countdownKey(node), node.textContent ?? '')); }
    countdownPaused = !countdownPaused; render(); restoreFocus('countdown-toggle');
  }, 'text-button', 'countdown-toggle'));
  main.append(sorting);
  if (!shown.length) main.append(el('p', 'form-note', t('No account here yet.')));
  for (const group of groupAccounts(shown, quotaContext())) {
    const section = el('section', 'account-group'); section.setAttribute('aria-label', t('{provider} accounts', { provider: providerName(group.provider) }));
    const heading = el('div', 'group-heading');
    heading.append(el('h2', '', providerName(group.provider)), el('span', 'count', `${plural(group.count, { one: '{n} account', other: '{n} accounts' })}`));
    // SB-142: the ordinary CLI's login is one per machine; it is named here, not as a pool's mark.
    const inCli = accounts.find((a) => a.provider === group.provider && currentMatch(a));
    if (inCli) heading.append(el('span', 'group-note', group.provider === 'claude' ? t('Ordinary Claude Code: {label}', { label: inCli.label }) : t('Ordinary Codex CLI: {label}', { label: inCli.label })));
    section.append(heading);
    const anyProject = (snapshot!.projects ?? []).length > 0;
    for (const pool of group.pools) {
      const owner = projectOf(pool.pool);
      if (group.pools.length > 1 || owner || anyProject) {
        const poolHead = el('div', 'pool-heading');
        const poolTitle = el('h3', ''); poolTitle.append(owner ? groupHint(t('Group · {name}', { name: owner.name }), owner.pool) : el('span', '', pool.pool === 'default' ? t('Default group') : t('Group {id} · no folders', { id: pool.pool })));
        poolHead.append(poolTitle);
        // SB-142: which account this pool's sessions use, so every pool reads the same way.
        const routed = snapshot!.routes[`${group.provider}:${pool.pool}`];
        const chosen = pool.accounts.find((a) => a.id === routed);
        poolHead.append(el('span', 'group-note', chosen ? (owner ? t('Sessions in its folders use {label}', { label: chosen.label }) : t('Managed sessions use {label}', { label: chosen.label })) : t('Nothing selected here yet')));
        section.append(poolHead);
      }
      const list = el('div', 'account-list'); list.setAttribute('role', 'list');
      pool.accounts.forEach((account) => list.append(accountRow(account)));
      section.append(list);
    }
    main.append(section);
  }
  main.append(el('p', 'surface-note', t('Switch changes the account of the ordinary Claude Code on this computer. Select chooses the account for managed sessions launched from Switchboard; a response already in progress keeps its account.')));
  main.append(kimiSection());
}
let kimiLoadedAt = 0;
async function loadKimi(stamp?: number) {
  kimiLoadedAt = Date.now();
  try { const value = await adapter.kimiAccounts(); if (stamp === undefined || clock.accepts(stamp)) { kimi = value; kimiError = ''; } } catch (error) { kimiError = safeError(error, 'read'); }
  backgroundRender();
}
/** The state line of a Kimi Code home: its plan windows, or why there are none. */
function kimiStatusLine(status: KimiStatus) {
  if (status.error) return el('span', status.signed_in ? 'usage-sub is-stale' : 'usage-error', t(status.error));
  const now = Math.floor(Date.now() / 1000);
  const line = kimiWindowsLine(status.windows ?? [], now);
  // SB-100: how old the figures are, so a figure from an hour ago never reads as current.
  const checked = status.checked_at ? ` · ${t('checked {time} ago', { time: compactCountdown(now, Math.min(status.checked_at, now)) })}` : '';
  return el('span', 'usage-sub', (line || t('Usage unknown')) + checked);
}
/** SB-81: Kimi Code subscription accounts, each signed in by the official kimi in its own folder. */
function kimiSection() {
  const section = el('section', 'account-group'); section.setAttribute('aria-labelledby', 'kimi-heading');
  const heading = el('div', 'group-heading'); const title = el('h2', '', 'Kimi Code'); title.id = 'kimi-heading';
  heading.append(title);
  if (kimi) heading.append(el('span', 'count', plural(kimi.accounts.length, { one: '{n} account', other: '{n} accounts' })));
  section.append(heading);
  if (kimiError && !kimi) { section.append(el('p', 'usage-error', t(kimiError)), button(t('Retry'), () => void loadKimi(), 'text-button', 'kimi-retry')); return section; }
  if (!kimi) { section.append(el('p', 'form-note', t('Reading Kimi Code accounts…'))); return section; }
  // SB-100: a failed refresh is said even while the last figures stay on screen.
  if (kimiError) section.append(el('p', 'usage-error', t('Kimi Code could not be refreshed: {error}', { error: t(kimiError) })));
  const current = el('p', 'form-note');
  if (kimi.current?.signed_in && !kimi.current.error) current.append(t('Ordinary kimi: {name}', { name: [kimi.current.nickname, kimi.current.tier].filter(Boolean).join(' · ') || t('signed in') }), ' · ', kimiStatusLine(kimi.current));
  else if (kimi.current) { current.append(t('Ordinary kimi: '), el('span', kimi.current.signed_in ? 'usage-sub is-stale' : 'usage-error', ordinaryKimiNote(kimi.current) ?? '')); }
  else current.append(kimi.installed ? t('Ordinary kimi: not signed in.') : t('Kimi Code is not installed. Install it from kimi.com/code, then add an account.'));
  section.append(current);
  const list = el('div', 'account-list'); list.setAttribute('role', 'list');
  for (const { account, status } of kimi.accounts) list.append(kimiRow(account, status));
  if (kimi.accounts.length) section.append(list);
  const footer = el('div', 'management-actions');
  if (kimi.installed) footer.append(button(t('Add Kimi Code account'), () => kimiAddDialog(), 'button row-button', 'kimi-add'));
  footer.append(button(t('Refresh'), () => { kimi = null; render(); void loadKimi(); }, 'button row-button quiet', 'kimi-refresh'));
  section.append(footer, el('p', 'surface-note', t('Each Kimi Code account signs in with the official kimi in its own folder and stays there; Switchboard never copies or renews its login. Usage comes from Kimi at each refresh.')));
  return section;
}
/**
 * UX-28: after Sign in again, the Kimi list is read every few seconds for up to five minutes, and
 * stops as soon as the account reads signed in; the row then updates without a Refresh.
 */
let kimiWatch: number | null = null;
function watchKimiSignIn(id: string) {
  if (kimiWatch !== null) window.clearInterval(kimiWatch);
  // Only a row that cannot be used is watched, so "signed in" is a change, not a fresher check (unit tier F).
  const until = Date.now() + 5 * 60_000;
  kimiWatch = window.setInterval(() => {
    if (Date.now() > until) { window.clearInterval(kimiWatch!); kimiWatch = null; return; }
    void adapter.kimiAccounts().then((next) => {
      const row = next.accounts.find((a) => a.account.id === id);
      if (!row || (row.status.signed_in && !row.status.error)) { window.clearInterval(kimiWatch!); kimiWatch = null; kimi = next; kimiLoadedAt = Date.now(); if (row) showNotice(t('{label} is signed in again.', { label: row.account.label })); backgroundRender(); }
    }, () => { /* the next look retries */ });
  }, 3000);
}
/** A Kimi Code account in the same row as Claude and Codex accounts: a small primary action and a menu. */
function kimiRow(account: KimiAccount, status: KimiStatus) {
  const signedOut = !status.signed_in;
  const row = el('article', `account-row kimi-account ${signedOut ? 'is-disabled' : ''}`); row.setAttribute('role', 'listitem');
  row.setAttribute('aria-label', t('{label}, Kimi Code', { label: account.label }));
  const limitedUntil = signedOut ? null : kimiLimitUntil(status.windows, Math.floor(Date.now() / 1000));
  row.dataset.state = signedOut ? 'sign_in' : limitedUntil !== null ? 'blocked' : status.error ? 'stale' : 'available';
  const icon = el('span', 'provider-icon kimi', 'K'); icon.setAttribute('aria-hidden', 'true');
  const name = el('div', 'row-name'); const title = el('div', 'row-title'); const labelNode = el('strong', '', account.label); labelNode.title = account.label; title.append(labelNode);
  if (signedOut) title.append(el('span', 'badge danger-badge', t('Sign in again')));
  else if (limitedUntil !== null) title.append(el('span', 'badge danger-badge', limitedUntil > 0 ? t('Limit reached · resets in {time}', { time: compactCountdown(limitedUntil, Math.floor(Date.now() / 1000)) }) : t('Limit reached')));
  name.append(title, el('span', 'row-sub', [account.tier, account.region === 'global' ? 'kimi.ai' : 'kimi.com'].filter(Boolean).join(' · ')));
  const usage = el('div', 'usage-cell kimi-usage');
  // SB-142: one plan window per line, so a narrow column never wraps them into a block.
  const nowSeconds = Math.floor(Date.now() / 1000);
  const lines = status.error ? [] : kimiWindowsLines(status.windows ?? [], nowSeconds);
  if (lines.length) {
    for (const line of lines) { const item = el('span', 'usage-sub usage-line', line); item.title = line; usage.append(item); }
    if (status.checked_at) usage.append(el('span', 'usage-sub usage-line', t('checked {time} ago', { time: compactCountdown(nowSeconds, Math.min(status.checked_at, nowSeconds)) })));
  } else usage.append(kimiStatusLine(status));
  const unusable = signedOut || !!status.error;
  const relogin = () => void mutate(async () => { await adapter.kimiSignInAgain(account.id); if (unusable) watchKimiSignIn(account.id); }, unusable ? t('Sign in to {label} in Terminal. The row updates when you finish.', { label: account.label }) : t('Sign in to {label} in Terminal, then refresh.', { label: account.label }), `kimi-primary-${account.id}`, undefined, 'sign_in');
  const primary = el('div', 'row-primary');
  primary.append(signedOut ? button(t('Sign in'), relogin, 'button row-button', `kimi-primary-${account.id}`) : button(t('Launch…'), () => kimiLaunchDialog(account.id, account.label), 'button row-button', `kimi-primary-${account.id}`));
  const items: ([string, () => void] | [string, () => void, string])[] = [];
  if (!signedOut) items.push([t('Sign in again'), relogin]);
  items.push([t('Remove…'), () => kimiRemoveDialog(account.id, account.label), 'danger-text']);
  row.append(icon, name, usage, primary, menu(`kimi-${account.id}`, t('More actions for {label}', { label: account.label }), items));
  return row;
}
/** Signs a new account in with `kimi login` in Terminal and saves it once Terminal reports success. */
function kimiAddDialog() {
  const context = openDialog(t('Add a Kimi Code account'), t('Terminal opens the official kimi login: confirm the code in your browser, signed in to the account to add. Switchboard saves it when Terminal says you are logged in.'));
  const label = input(''); label.required = false; label.placeholder = t('Kimi\'s nickname for the account');
  const region = select([['mainland-cn', t('kimi.com (mainland China)')], ['global', t('kimi.ai (global)')]]);
  const grid = el('div', 'form-grid'); grid.append(field(t('Label'), label, t('Optional.')), field(t('Service'), region, t('Where the membership was bought.')));
  context.body.append(grid);
  const startButton = submit(t('Open sign-in')); context.actions.append(startButton);
  let loginId: string | null = null; let finishing = false; let completed = false;
  const waiting = el('p', 'form-note', t('Waiting for the sign-in in Terminal…'));
  // SB-104: a sign-in that ended, or a save that failed, leaves the dialog usable — never a dead end.
  const finish = () => {
    finishing = true; let saved = false;
    void dialogSave(context, async () => { const { account } = await adapter.kimiLoginFinish(loginId!); saved = true; loginId = null; await loadKimi(); return account; }, t('Kimi Code account saved.'))
      .then(() => { if (saved) return; finishing = false; startButton.disabled = false; startButton.textContent = t('Save the account'); });
  };
  const poll = window.setInterval(() => {
    if (!loginId || finishing || completed) return;
    void adapter.kimiLoginStatus(loginId).then(({ state }) => {
      if (state === 'ended') { loginId = null; waiting.remove(); startButton.disabled = false; context.error.textContent = t('This Kimi Code sign-in is no longer waiting. Start it again.'); return; }
      if (state !== 'complete') return;
      completed = true; window.clearInterval(poll); finish();
    }, () => { /* the next look retries */ });
  }, 1500);
  context.beforeCancel(async () => { finishing = true; try { if (loginId) await adapter.kimiLoginCancel(loginId); } catch (error) { finishing = false; throw error; } window.clearInterval(poll); });
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    if (completed && loginId && !finishing) { finish(); return; }
    if (loginId) return;
    context.error.textContent = ''; context.setBusy(true);
    adapter.kimiLoginBegin(label.value.trim(), region.value as KimiRegion).then(({ login_id }) => {
      loginId = login_id; context.setBusy(false); startButton.disabled = true;
      context.body.append(waiting);
    }, (error) => { context.setBusy(false); context.error.textContent = t(safeError(error)); });
  });
}
function kimiLaunchDialog(id: string, accountLabel: string) {
  const context = openDialog(t('Launch {label}', { label: accountLabel }), t('Starts the official kimi on this account in Terminal, in the folder you choose. Kimi Code always runs in this account’s own folder.'));
  const folder = folderPicker(context, lastWorkingDirectory, `kimi-folder-${id}`);
  context.body.append(groupField(t('Folder'), folder.node));
  context.actions.append(submit(t('Launch')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault(); const value = folder.require(); if (!value) return;
    void dialogSave(context, () => adapter.kimiLaunch(id, value), t('Terminal launch requested for {agent}.', { agent: accountLabel }), 'launch');
  });
  folder.focus();
}
function kimiRemoveDialog(id: string, accountLabel: string) {
  const context = openDialog(t('Remove {label}?', { label: accountLabel }), t('Switchboard forgets the account and deletes its login folder. The Kimi membership itself is not touched; sign in again to add it back.'));
  const confirm = submit(t('Remove account')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { await adapter.kimiRemove(id); await loadKimi(); }, t('Kimi Code account removed.')); });
  context.actions.querySelector('button')?.focus();
}
function accountRow(account: Account) {
  const current = currentMatch(account); const active = selected(account); const signIn = signInRequired(account);
  const row = el('article', `account-row ${current ? 'is-current' : ''} ${active ? 'is-selected' : ''} ${!account.enabled ? 'is-disabled' : ''}`); row.setAttribute('role', 'listitem');
  row.setAttribute('aria-label', t('{label}, {provider}, {group}', { label: account.label, provider: providerName(account.provider), group: poolName(account.pool, snapshot?.projects) }) + (current ? t(', in use by the CLI') : '') + (active ? t(', selected for managed sessions') : ''));
  const state = cardState(account, quotaContext()); row.dataset.state = state;
  const icon = el('span', `provider-icon ${account.provider}`, account.provider === 'claude' ? '✳' : '◎'); icon.setAttribute('aria-hidden', 'true'); icon.title = CARD_STATE[state];
  const name = el('div', 'row-name');
  const labelNode = el('strong', '', account.label); labelNode.title = account.label;
  const title = el('div', 'row-title'); title.append(labelNode);
  if (current) title.append(el('span', 'badge current-badge', account.provider === 'claude' ? t('In Claude Code') : t('In Codex CLI')));
  if (signIn) title.append(el('span', 'badge danger-badge', t('Sign in again')));
  const elsewhere = snapshot?.elsewhere?.[account.id] !== undefined;
  if (elsewhere) { const badge = el('span', 'badge muted', t('On another machine')); badge.title = t('Marked as used on another machine: this computer never renews it or switches to it, by hand or automatically.'); title.append(badge); }
  else if (!account.enabled) title.append(el('span', 'badge muted', t('Disabled')));
  const org = organizationOf(account);
  if (org) { const badge = el('span', 'badge org-badge', org.name); badge.title = org.why; title.append(badge); }
  const email = account.external_identity?.email;
  const subText = [email && email !== account.label ? email : '', account.kind === 'oauth' ? '' : kindName(account.kind)].filter(Boolean).join(' · ') || (account.kind === 'oauth' ? t('OAuth') : '');
  const sub = el('span', 'row-sub', subText); sub.title = subText; name.append(title, sub);
  const project = !!projectOf(account.pool);
  row.append(icon, name, usageCell(account), primaryButton(account, { current, selected: active, signIn, project }), rowMenu(account, { current, selected: active, signIn, project }));
  if (quotaOpen.has(account.id) && account.usage) row.append(quotaDetails(account));
  // SB-160 (B-6, SCN-078): a seat that ended names its organization instead of the generic sentence.
  const ended = seatEndedText(snapshot!, account, monitor?.sign_in_required);
  if (ended) row.append(el('p', 'row-error seat-ended', ended));
  if (usageErrors.has(account.id) && !(ended && usageErrors.get(account.id) === "This account's sign-in has ended. Sign in to it again.")) row.append(el('p', 'row-error', t(usageErrors.get(account.id)!)));
  return row;
}
function primaryButton(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const slot = el('div', 'row-primary'); const key = `primary-${account.id}`;
  const action = primaryAction(account, state);
  const status = (text: string) => { const label = el('span', 'row-state', text); label.tabIndex = -1; label.dataset.focus = key; return label; };
  if (action === 'in_use') slot.append(status(t('✓ In use')));
  else if (action === 'selected') slot.append(status(t('✓ Selected')));
  else if (action === 'switch') { const control = button(t('Switch'), () => switchNative(account), 'button row-button', key); control.title = t('Make this the Claude Code account on this computer'); slot.append(control); }
  else if (action === 'select') slot.append(button(t('Select'), () => void mutate(() => adapter.select(account), t('{label} selected for the next managed request {where}.', { label: account.label, where: inGroup(account.pool, snapshot?.projects) }), key), 'button row-button', key));
  else if (action === 'sign_in') slot.append(button(t('Sign in'), () => void startLogin(account.provider, account.label, account.pool), 'button row-button', key));
  else if (snapshot?.elsewhere?.[account.id] !== undefined) slot.append(button(t('Use here'), () => void mutate(() => adapter.markElsewhere(account.id, false), t('{label} is used on this computer again.', { label: account.label }), key), 'button row-button quiet', key));
  else slot.append(button(t('Enable'), () => void mutate(() => adapter.update(account.id, account.label, true), t('{label} enabled.', { label: account.label }), key), 'button row-button quiet', key));
  return slot;
}
function rowMenu(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const items: ([string, () => void] | [string, () => void, string])[] = [];
  if (account.enabled && canSwitchNative(account) && !state.project && !state.selected) items.push([t('Select for managed sessions'), () => void mutate(() => adapter.select(account), t('{label} selected for the next managed request {where}.', { label: account.label, where: inGroup(account.pool, snapshot?.projects) }), `menu-${account.id}`)]);
  if (account.enabled && canProbe(account)) items.push([t('Check usage'), () => void mutate(() => adapter.probe(account.id), t('Usage observation updated.'), `menu-${account.id}`, account.id)]);
  if (account.enabled) items.push([t('Launch…'), () => launchDialog(account)]);
  if (account.kind === 'oauth' && !state.signIn) items.push([t('Sign in again'), () => void startLogin(account.provider, account.label, account.pool)]);
  items.push([t('Move to another group…'), () => moveDialog(account, state.current)]);
  if (snapshot?.organizations?.length) items.push([t('Organization…'), () => assignDialog(account)]);
  // SB-175 (SB-168 D-2): an account signed in on another machine too must not be renewed from here.
  if (snapshot?.elsewhere?.[account.id] === undefined) items.push([t('Used on another machine'), () => void mutate(() => adapter.markElsewhere(account.id, true), t('{label} is marked as used on another machine. This computer no longer renews it or switches to it, by hand or automatically.', { label: account.label }), `menu-${account.id}`)]);
  items.push([t('Rename or disable…'), () => editDialog(account)], [t('Remove…'), () => removeDialog(account), 'danger-text']);
  return menu(account.id, t('More actions for {label}', { label: account.label }), items);
}
/** What the status mark and the screen reader say for each card state. */
const CARD_STATE: Record<CardState, string> = { available: t('Available'), low: t('Running low'), blocked: t('Limit reached'), stale: t('Stale'), failed: t('Check failed'), unknown: t('Usage unknown'), sign_in: t('Sign in again'), disabled: t('Disabled'), no_quota: t('No quota check') };
/** "{lead} 2h 5m · 5 Oct, 21:03": a compact wait that the minute timer keeps current. */
function compactWait(lead: string, until: number, estimated = false) {
  const wrapper = el('span', 'usage-wait');
  const remaining = el('span', 'usage-countdown', countdownPaused ? pausedCountdowns.get(`${until}|compact`) ?? t('paused') : compactCountdown(until, Date.now() / 1000));
  remaining.dataset.countdown = String(until); remaining.dataset.format = 'compact'; remaining.setAttribute('aria-live', 'off');
  const time = el('time', '', date(until)); time.dateTime = new Date(until * 1000).toISOString();
  time.title = new Date(until * 1000).toLocaleString(dateLocale(), { timeZoneName: 'short' });
  wrapper.append(`${lead} `, remaining, ' · ', time);
  if (estimated) wrapper.append(el('span', 'usage-estimate', t(' · estimated')));
  return wrapper;
}
/** Two lines in every state (compact list): use and freshness, then the one time that matters. */
function usageCell(account: Account) {
  const observation = account.usage; const now = Date.now() / 1000; const limit = accountLimit(account);
  const context = quotaContext(); const state = cardState(account, context); const health = account.usage_health;
  const used = observation ? accountUsedPercent(observation) : null;
  const freshness = observation ? usageFreshness(observation, quotaMaxAge(account, snapshot?.policies), now) : null;
  // Line one: the meter with the share used, and how fresh it is.
  const top = el('span', 'usage-line');
  if (observation && used !== null) {
    const meter = el('progress', `usage-meter is-${state}`); meter.max = 100; meter.value = used; meter.setAttribute('aria-hidden', 'true');
    top.append(meter, el('strong', 'usage-pct', `${Math.round(used)}%`));
    const ageNode = el('span', `usage-age${state === 'stale' || state === 'failed' ? ' is-warn' : ''}`, age(observation.observed_at));
    ageNode.title = t('Checked {date}', { date: date(observation.observed_at) }); top.append(ageNode);
  } else {
    top.append(el('span', 'usage-unknown', account.kind === 'api_key' ? t('API billing') : account.kind === 'setup_token' ? t('No quota check') : t('Usage unknown')));
  }
  // Line two: the time that decides what happens next.
  const sub = el('span', `usage-sub is-${state}`);
  const order = quotaOrder(account, context);
  if (state === 'blocked') {
    const estimated = !!limit && order.until === limit.until && limitLabel(limit) !== t('Limit resets');
    if (order.until) sub.append(compactWait(estimated ? t('Retry in') : t('Back in'), order.until, estimated)); else sub.append(t('Reset time unavailable'));
    if (limit) sub.title = limit.source === 'managed' ? t('A managed request was refused with a rate limit.') : t('Claude Code reported a usage or spend limit. The retry hold may be estimated.');
  } else if (state === 'failed') {
    const next = failedNextCheck(account); sub.append(next !== null ? t('Check failed · next {date}', { date: date(next) }) : t('Check failed · check usage to retry'));
  } else if (state === 'stale') {
    const reset = observation ? accountReset(observation) : null;
    sub.append(freshness?.resetPassed ? t('Reset since check · awaiting a new one') : reset ? t('Stale · reported reset {date}', { date: date(reset) }) : t('Stale · awaiting a new check'));
  } else if (state === 'available' || state === 'low') {
    const reset = observation ? accountReset(observation) : null;
    if (reset) sub.append(compactWait(t('Resets in'), reset)); else sub.append(t('Reset time unavailable'));
  } else if (state === 'unknown') sub.append(monitorChecks(account) ? (health ? t('Next check {date}', { date: date(health.next_check_at) }) : t('Waiting for the first check')) : t('Not checked automatically'));
  else if (state === 'sign_in') sub.append(t('Sign in to check usage'));
  else if (state === 'disabled') sub.append(t('Not checked while disabled'));
  // SCN-021: API-key and setup-token rows have no subscription quota to check (SB-67).
  else if (state === 'no_quota') sub.append(t('Not checked automatically · Quota checks need OAuth'));
  const label = `${CARD_STATE[state]}${used !== null ? t(', {percent}% used', { percent: Math.round(used) }) : ''}. ${sub.textContent ?? ''}`;
  if (!observation) { const cell = el('div', 'row-usage'); cell.setAttribute('role', 'group'); cell.setAttribute('aria-label', label); cell.append(top, sub); return cell; }
  const control = button('', () => { if (quotaOpen.has(account.id)) quotaOpen.delete(account.id); else quotaOpen.add(account.id); render(); restoreFocus(`quota-${account.id}`); }, 'row-usage', `quota-${account.id}`);
  control.setAttribute('aria-expanded', String(quotaOpen.has(account.id)));
  control.setAttribute('aria-label', `${label} ${t('Show quota windows.')}`);
  control.append(top, sub);
  return control;
}
function quotaDetails(account: Account) {
  const observation = account.usage!; const now = Date.now() / 1000; const health = account.usage_health;
  const details = el('div', 'row-details');
  const windows = observation.windows?.length ? observation.windows : [{ name: t('Highest reported window'), used_percent: observation.used_percent, resets_at: observation.resets_at }];
  const limit = accountLimit(account);
  if (limit) {
    // The hold is not a quota reset; the windows below keep their own reset times.
    const row = el('div', 'quota-window'); row.append(el('strong', '', limitTitle(limit)), resetDisplay(limit.until, (when) => (limitLabel(limit) === t('Limit resets') ? t('Limit resets {date}', { date: when }) : t('Retry hold until {date}', { date: when }))));
    const why = limitReasonText(limit.reason); if (why) row.append(el('p', 'form-note', why));
    details.append(row);
  }
  for (const window of windows) {
    const reset = windowReset(window.resets_at, now);
    const name = 'name' in window && isFeatureWindow(window) ? featureWindowLabel(window.name) : window.name;
    const row = el('div', 'quota-window'); row.append(el('strong', '', reset ? t('{name} · usage unknown since reset', { name }) : t('{name} · {percent}% used', { name, percent: Math.round(window.used_percent) })));
    if (reset) row.append(el('span', 'usage-caption', t('Reset {date} · {percent}% was used before', { date: date(window.resets_at!), percent: Math.round(window.used_percent) })));
    else if (window.resets_at) row.append(resetDisplay(window.resets_at));
    else row.append(el('span', 'usage-caption', t('Reset time unavailable')));
    details.append(row);
  }
  details.append(el('p', 'usage-caption', t('{source} · Observed {date}', { source: observation.source, date: date(observation.observed_at) })));
  if (health?.status === 'failed') details.append(el('p', 'usage-error', t('Last quota check failed. Not eligible for automatic switching until a check succeeds.')));
  if (monitorChecks(account) && health) details.append(el('span', 'usage-caption', t('Checked {age} · Next check {date}', { age: age(health.checked_at), date: date(health.next_check_at) })));
  else if (!monitorChecks(account)) details.append(el('span', 'usage-caption', canProbe(account) ? t('Not checked automatically while disabled') : t('Not checked automatically · Quota checks need OAuth')));
  return details;
}
/**
 * UX-17, D-1: the group an account or a setting belongs to, chosen from the groups that exist (by
 * name), Default, and pools accounts already use; a new id is typed only after choosing "New id…".
 */
function poolField(current: string, locked = false) {
  const choices = poolChoices(snapshot?.accounts ?? [], snapshot?.projects ?? [], current);
  const picker = select([...choices.map((c): [string, string] => [c.pool, c.group || c.pool === 'default' ? c.name : t('Group {id} · no folders', { id: c.pool })]), ...(locked ? [] : [['__new', t('New id…')] as [string, string]])]);
  picker.value = current;
  const typed = input(''); typed.maxLength = 32; typed.pattern = POOL_PATTERN; typed.required = false; typed.placeholder = t('e.g. client-a'); typed.title = t('Use lowercase letters, numbers, hyphens, or underscores.');
  const typedField = field(t('Group id'), typed, t('Accounts in one group switch among themselves. Use an existing group id, or a new one to start a group: lowercase letters, numbers, hyphens or underscores.')); typedField.hidden = true;
  // Unit tier A: a hidden id field is disabled and emptied, so a typed id that does not fit never blocks the form.
  const sync = () => { const fresh = picker.value === '__new'; typedField.hidden = !fresh; typed.required = fresh; typed.disabled = !fresh; typed.dataset.locked = String(!fresh); if (!fresh) typed.value = ''; else typed.focus(); };
  sync();
  picker.addEventListener('change', sync);
  if (locked) { picker.disabled = true; picker.dataset.locked = 'true'; }
  const node = el('div', 'pool-field'); node.append(field(t('Group'), picker, t('The default group works in every folder outside other groups.')), typedField);
  return { node, picker, value: () => (picker.value === '__new' ? typed.value.trim() : picker.value), onChange: (fn: () => void) => { picker.addEventListener('change', fn); typed.addEventListener('change', fn); } };
}
function addDialog() {
  const context = openDialog(t('Add an API key or token'), t('For accounts without an official sign-in. To add a Claude Code or Codex login, use + Add account → Sign in, or add the account the CLI already uses.'));
  const provider = select([['claude', t('Claude Code')], ['codex', t('Codex CLI')]]);
  const method = select([['api_key', t('API key')], ['setup_token', t('Claude setup token')], ['oauth', t('OAuth JSON')]]);
  const label = input(); label.maxLength = 80; label.placeholder = t('e.g. Studio');
  const pool = poolField('default');
  const grid = el('div', 'form-grid'); grid.append(field(t('Provider'), provider), field(t('Credential type'), method), field(t('Account label'), label), pool.node);
  const credentialSlot = el('div'); context.actions.append(submit(t('Add account'))); context.body.append(grid, credentialSlot);
  let secret: HTMLInputElement | HTMLTextAreaElement | null = null;
  const update = () => {
    if (secret) secret.value = ''; credentialSlot.replaceChildren(); secret = null;
    const setup = method.querySelector<HTMLOptionElement>('option[value="setup_token"]')!; setup.disabled = provider.value !== 'claude';
    if (setup.disabled && method.value === 'setup_token') method.value = 'api_key';
    if (method.value === 'oauth') { const textarea = el('textarea', 'secret-json'); textarea.rows = 5; textarea.required = true; textarea.spellcheck = false; textarea.autocomplete = 'off'; textarea.placeholder = t('Paste credential JSON'); secret = textarea; }
    else { secret = input('', 'password'); secret.autocomplete = 'new-password'; secret.placeholder = t('Enter credential'); }
    credentialSlot.append(field(method.value === 'oauth' ? t('OAuth credential JSON') : t('Credential'), secret, demo ? t('Use synthetic input only. Nothing is stored after reload.') : t('Sent only to native credential storage. Cleared on submit or cancel.')));
    if (method.value === 'oauth') credentialSlot.append(el('p', 'form-note', t('Imported OAuth is a snapshot. Sign in again when it expires. Imported identity is not independently verified.')));
  };
  provider.addEventListener('change', update); method.addEventListener('change', update); update();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault(); if (!context.form.reportValidity()) return;
    if (!label.value.trim()) { label.setCustomValidity(t('Enter an account label.')); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; }
    const value = secret!.value; secret!.value = '';
    void dialogSave(context, () => adapter.add({ provider: provider.value as Provider, label: label.value.trim(), pool: pool.value(), kind: method.value as AuthKind, secret: value }), t('Account added. Select it when you are ready.'));
  });
  provider.focus();
}
function renderCurrent(main: HTMLElement) {
  const section = el('section', 'current-section'); section.setAttribute('aria-label', t('Accounts the CLIs use now'));
  const heading = el('div', 'section-heading');
  const proxy = el('span', 'proxy-status'); const dot = el('span', `status-dot ${runtimeError ? 'unavailable' : ''}`); dot.setAttribute('aria-hidden', 'true');
  proxy.append(dot, el('span', '', demo ? t('Demo · no live requests') : runtimeError ? t('Local proxy status unavailable') : t('Local proxy {address}', { address: runtime?.proxy_address ?? t('starting…') })));
  const inUse = el('h2', '', t('In use now')); inUse.tabIndex = -1; inUse.dataset.focus = 'current-heading';
  heading.append(inUse, proxy);
  section.append(heading);
  const grid = el('div', 'current-grid');
  for (const provider of ['claude', 'codex'] as const) {
    const current = currentAccounts?.[provider]; const card = el('div', 'current-card');
    const copy = el('div', 'current-copy'); copy.append(el('span', 'current-provider', providerName(provider)));
    if (current?.status === 'available') {
      copy.append(el('strong', 'current-identity', identityText(current.identity)));
      const matches = snapshot!.accounts.filter((account) => account.provider === provider && currentMatch(account));
      card.append(copy);
      if (matches.length) card.append(el('span', 'current-state', t('✓ Saved as {labels}', { labels: matches.map((account) => account.label).join(', ') })));
      else card.append(button(t('Add to Switchboard'), () => { let captured: Awaited<ReturnType<typeof adapter.captureCurrent>> | undefined; void mutate(async () => { captured = await adapter.captureCurrent({ provider, pool: 'default' }); }, () => [t('{identity} added to {provider} {where}. Switchboard keeps its sign-in up to date.', { where: inGroup('default'), identity: identityText(current.identity), provider: providerName(provider) }), captured ? organizationMoveNote(captured.label, providerName(provider), captured.organization_group) : ''].filter(Boolean).join(' '), `capture-${provider}`); }, 'button primary row-button', `capture-${provider}`));
    } else {
      const failed = current?.status === 'unavailable' || (!current && currentReadFailed);
      copy.append(el('strong', 'current-identity muted-text', current?.status === 'missing' ? t('Not signed in') : failed ? t('Could not read the sign-in') : t('Reading…')));
      card.append(copy);
      if (failed) card.append(button(t('Retry'), () => {
        currentReadFailed = false; render(); const stamp = clock.stamp();
        void readContext().then((context) => { if (clock.accepts(stamp)) applyContext(context); render(); restoreFocus(currentReadFailed ? `retry-current-${provider}` : 'current-heading'); });
      }, 'button quiet row-button', `retry-current-${provider}`));
    }
    grid.append(card);
  }
  section.append(grid); main.append(section);
}
const policyTarget = (target: RotationPolicy['target']) => target === 'managed' ? t('Managed route') : t('Claude Code account');
function renderPolicies(main: HTMLElement) {
  const policies = snapshot!.policies ?? []; const enabled = policies.filter((policy) => policy.enabled);
  const panel = el('section', 'rotation-bar'); panel.setAttribute('aria-label', t('Automatic switching'));
  // SB-104: without the monitor's answer no row can say it is limited or signed out — say so instead of showing them all as available.
  if (!monitor && (native || demo)) panel.append(el('p', 'usage-error', t('Limits and sign-in states are unavailable right now. Refresh to retry.')));
  const copy = el('div', 'rotation-copy');
  copy.append(el('strong', '', enabled.length ? t('Automatic switching is on') : t('Automatic switching is off')));
  if (enabled.length) for (const policy of enabled) {
    const decision = monitor?.decisions?.find((entry) => entry.provider === policy.provider && entry.pool === policy.pool && entry.target === policy.target);
    copy.append(el('span', 'usage-caption', `${providerName(policy.provider)} · ${poolName(policy.pool, snapshot!.projects)} · ${policyTarget(policy.target)} · ${t('at {percent}% used', { percent: policy.threshold_percent })}${decision ? ` · ${decisionText(decision.reason)}` : ''}`));
  } else copy.append(el('span', 'usage-caption', monitor ? t('When the account in use nears its limit, Switchboard can move to the saved account with the most quota left.') : t('Quota monitor status unavailable. Refresh to retry.')));
  const swapHeld = monitor?.claude_swap_accounts ?? 0;
  if (swapHeld) copy.append(el('span', 'usage-caption', t('Claude Swap is running and renews {n} of these accounts. Switchboard takes their newest sign-ins from it and does not renew them itself.', { n: swapHeld })));
  if (monitor?.renewal_blocked) copy.append(el('span', 'usage-caption', t('Claude is refusing sign-in renewals for every account right now. Saved accounts keep their last sign-in and are not renewed; Switchboard tries again within the hour. If this stays, update Switchboard.')));
  if (demo) copy.append(el('span', 'usage-caption', t('Demo policies are editable fixtures; no switching runs here.')));
  const actions = el('div', 'rotation-actions');
  const pool = autoSwitchPool(snapshot!.accounts, policies, (snapshot!.projects ?? []).map((p) => p.pool));
  if (pool) actions.append(button(t('Turn on for Claude Code'), () => { const saved = policies.find((policy) => policy.provider === 'claude' && policy.pool === pool && policy.target === 'claude_cli'); void mutate(() => adapter.setPolicy({ provider: 'claude', pool, target: 'claude_cli', enabled: true, threshold_percent: saved?.threshold_percent ?? 90, hysteresis_percent: saved?.hysteresis_percent ?? 10, cooldown_seconds: saved?.cooldown_seconds ?? 1800, max_age_seconds: saved?.max_age_seconds ?? 300, last_switched_at: saved?.last_switched_at ?? null }), t('Automatic switching is on for Claude Code {where}. It moves at {threshold}% used to an account with at least {headroom} points more headroom.', { where: inGroup(pool, snapshot!.projects), threshold: saved?.threshold_percent ?? 90, headroom: saved?.hysteresis_percent ?? 10 }), 'menu-policies'); }, 'button primary row-button', 'auto-on'));
  for (const policy of enabled) actions.append(button(enabled.length > 1 ? t('Stop {where}', { where: inGroup(policy.pool, snapshot!.projects) }) : t('Stop'), () => void mutate(() => adapter.setPolicy({ ...policy, enabled: false }), t('Automatic switching stopped for {provider} {where}.', { provider: providerName(policy.provider), where: inGroup(policy.pool, snapshot!.projects) }), 'menu-policies'), 'button quiet row-button', `stop-${policy.provider}-${policy.pool}-${policy.target}`));
  actions.append(menu('policies', t('Automatic switching settings'), [...policies.map((policy): [string, () => void] => [t('Edit {provider} {where} ({target})…', { provider: providerName(policy.provider), where: inGroup(policy.pool, snapshot!.projects), target: policyTarget(policy.target) }), () => policyDialog(policy)]), [t('New automatic switching…'), () => policyDialog()]], 'icon-button', '⚙'));
  panel.append(copy, actions); main.append(panel);
}
/** SB-145/SB-147: the proxy's counts for the chosen range (7, 30 or 90 days). */
let stats: Statistics | null = null;
let statsError = false;
let statsDays = 30;
let chartMetric: ChartMetric = 'tokens';
async function loadStats() {
  keySpend = null; const days = statsDays;
  let value: Statistics | null = null; let failed = false;
  try { value = await adapter.statistics(days); } catch { failed = true; }
  // Unit tier C: an answer for a range the person has left is dropped.
  if (days !== statsDays) return;
  stats = value; statsError = failed;
  // Unit tier B: the render keeps focus where it was (the page title on arrival, Retry after a retry).
  const key = (document.activeElement as HTMLElement | null)?.dataset.focus;
  render(); restoreFocus(key);
  // A Retry that worked is gone from the page: focus goes to the page title, never to the body.
  if (page === 'stats' && (!document.activeElement || document.activeElement === document.body)) restoreFocus('page-title');
}
function statTile(label: string, value: string, note: string) {
  const tile = el('div', 'stat-tile'); tile.append(el('span', 'stat-label', label), el('strong', 'stat-value', value), el('span', 'stat-note', note)); return tile;
}
const SVG = 'http://www.w3.org/2000/svg';
function svg<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>) {
  const node = document.createElementNS(SVG, tag); for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v)); return node;
}
/** SB-147: tokens or requests per day, stacked by group; each bar names its day and values on hover. */
function statChart(s: Statistics) {
  const section = el('section', 'about-panel stat-panel stat-chart'); section.setAttribute('aria-labelledby', 'stat-chart-heading');
  const head = el('div', 'stat-chart-head');
  const heading = el('h2', '', chartMetric === 'tokens' ? t('Tokens per day') : t('Requests per day')); heading.id = 'stat-chart-heading';
  const toggle = el('div', 'segmented'); toggle.setAttribute('role', 'group'); toggle.setAttribute('aria-label', t('Show'));
  for (const [value, label] of [['tokens', t('Tokens')], ['requests', t('Requests')]] as [ChartMetric, string][]) {
    const b = button(label, () => { chartMetric = value; render(); restoreFocus(`chart-${value}`); }, `segment${chartMetric === value ? ' is-on' : ''}`, `chart-${value}`);
    b.setAttribute('aria-pressed', String(chartMetric === value)); toggle.append(b);
  }
  head.append(heading, toggle); section.append(head);
  const names = (pool: string) => poolName(pool, s.groups.filter((g): g is { pool: string; name: string } & typeof g => g.name !== null));
  const { days, series } = chartSeries(s, chartMetric, names);
  if (!series.length) { section.append(el('p', 'form-note', t('No traffic in this period.'))); return section; }
  const totals = days.map((_, i) => series.reduce((sum, x) => sum + x.values[i], 0));
  const max = niceMax(Math.max(...totals));
  const W = 900, H = 220, L = 52, B = 26, T = 8, plotW = W - L - 8, plotH = H - B - T;
  const chart = svg('svg', { viewBox: `0 0 ${W} ${H}`, class: 'stat-chart-svg', role: 'img', 'aria-label': t('{metric} per day by group', { metric: chartMetric === 'tokens' ? t('Tokens') : t('Requests') }) });
  for (const f of [0, 0.5, 1]) {
    const y = T + plotH - plotH * f;
    chart.append(svg('line', { x1: L, x2: W - 8, y1: y, y2: y, class: 'chart-grid' }));
    const label = svg('text', { x: L - 8, y: y + 4, class: 'chart-axis', 'text-anchor': 'end' }); label.textContent = compactCount(max * f); chart.append(label);
  }
  const slot = plotW / days.length; const barW = Math.min(28, Math.max(2, slot * 0.7));
  const every = Math.max(1, Math.ceil(days.length / 8));
  days.forEach((day, i) => {
    const x = L + i * slot + (slot - barW) / 2; let y = T + plotH;
    const group = svg('g', {});
    const tip = svg('title', {}); tip.textContent = [day, ...series.filter((x) => x.values[i]).map((x) => `${x.name}: ${compactCount(x.values[i])}`)].join('\n'); group.append(tip);
    series.forEach((ser, si) => {
      const h = (ser.values[i] / max) * plotH; if (h <= 0) return;
      y -= h; group.append(svg('rect', { x, y, width: barW, height: Math.max(h, 1), class: `chart-bar chart-s${si % 6}`, rx: 1.5 }));
    });
    chart.append(group);
    // Counted from the last day, so today always has its label and none run together.
    if ((days.length - 1 - i) % every === 0) { const label = svg('text', { x: x + barW / 2, y: H - 8, class: 'chart-axis', 'text-anchor': 'middle' }); label.textContent = day.slice(5); chart.append(label); }
  });
  section.append(chart);
  const legend = el('ul', 'chart-legend'); legend.setAttribute('aria-label', t('Groups'));
  series.forEach((x, si) => { const item = el('li', ''); item.append(el('span', `chart-swatch chart-s${si % 6}`), el('span', '', `${x.name} · ${compactCount(x.values.reduce((a, b) => a + b, 0))}`)); legend.append(item); });
  section.append(legend);
  return section;
}
/** SB-153: each statistics table's sort, by table id; kept while the window is open. */
const statSorts: Record<string, StatSort> = {};
/** One table: a name column (or several) and the counts every row has; a click on a column title sorts by it. */
function statTable(id: string, title: string, names: string[], rows: StatRow[], compact = false) {
  const section = el('section', 'about-panel stat-panel'); section.setAttribute('aria-labelledby', id);
  const heading = el('h2', '', title); heading.id = id; section.append(heading);
  const wrap = el('div', 'stat-scroll'); const table = el('table', 'stat-table');
  const head = el('tr');
  const sort = statSorts[id] ?? null;
  const countNames = compact ? [t('Requests'), t('Tokens'), t('Limits and errors')] : [t('Requests'), t('Sent'), t('From cache'), t('Received'), t('Limits and errors')];
  [...names, ...countNames].forEach((name, column) => {
    const isCount = column >= names.length;
    const th = el('th', isCount ? 'num' : ''); th.setAttribute('scope', 'col');
    const on = sort?.column === column;
    th.setAttribute('aria-sort', on ? (sort!.descending ? 'descending' : 'ascending') : 'none');
    const b = button(`${name}${on ? (sort!.descending ? ' ↓' : ' ↑') : ''}`, () => { statSorts[id] = nextStatSort(sort, column, isCount); render(); restoreFocus(`${id}-sort-${column}`); }, `sort-button${on ? ' is-on' : ''}`, `${id}-sort-${column}`);
    b.setAttribute('aria-label', t('Sort by {column}', { column: name }));
    th.append(b); head.append(th);
  });
  const thead = el('thead'); thead.append(head); table.append(thead);
  const body = el('tbody');
  for (const row of sortStatRows(rows, sort, compact)) {
    const tr = el('tr');
    row.cells.forEach((cell, i) => {
      const td = el(i === 0 ? 'th' : 'td', '', typeof cell === 'string' ? cell : cell[0]);
      if (typeof cell !== 'string' && cell[1]) td.append(el('span', 'cell-note', cell[1]));
      tr.append(td);
    });
    tr.firstElementChild?.setAttribute('scope', 'row');
    const c = row.counts; const trouble = statTrouble(c);
    const values = compact ? [compactCount(c.requests), compactCount(statTokens(c))] : [compactCount(c.requests), compactCount(c.input_tokens + c.cache_write_tokens), compactCount(c.cache_read_tokens), compactCount(c.output_tokens)];
    for (const v of values) tr.append(el('td', 'num', v));
    tr.append(el('td', `num${c.limited + c.failed ? ' is-trouble' : ''}${statAlert(c) ? ' is-alert' : ''}`, trouble === null ? '–' : `${compactCount(c.limited + c.failed)} · ${statTroubleText(c)}`));
    body.append(tr);
  }
  table.append(body); wrap.append(table); section.append(wrap);
  return section;
}
/** SB-156: what each saved OpenRouter key has spent (the machine's and each group's), as OpenRouter reports it. */
let keySpend: { name: string; status: OpenrouterStatus | null; error: boolean }[] | null = null; let keySpendLoading = false;
function loadKeySpend() {
  if (keySpendLoading) return;
  keySpendLoading = true;
  void (async () => {
    try {
      const keys = await adapter.openrouterKeys();
      keySpend = await Promise.all(keys.keys.map(async (k) => {
        const name = k.pool === null ? t('Every folder (the machine’s key)') : t('Group “{name}”', { name: k.project ?? k.pool });
        try { return { name, status: await adapter.openrouterKeyStatus(k.pool), error: false }; } catch { return { name, status: null, error: true }; }
      }));
    } catch { keySpend = []; }
    keySpendLoading = false; backgroundRender();
  })();
}
function statKeysPanel() {
  const section = el('section', 'about-panel stat-panel'); section.setAttribute('aria-labelledby', 'stat-keys');
  const heading = el('h2', '', t('API keys')); heading.id = 'stat-keys';
  section.append(heading, el('p', 'form-note', t('Agents on OpenRouter spend the key of the group whose folder they start in, else the machine’s. OpenRouter reports the spend per key.')));
  if (!keySpend) { loadKeySpend(); section.append(el('p', 'form-note', t('Reading the keys…'))); return section; }
  if (!keySpend.length) { section.append(el('p', 'form-note', t('No OpenRouter key is saved. Add one under API keys.'))); return section; }
  const table = el('table', 'stat-table'); const head = el('tr');
  for (const [name, cls] of [[t('Key'), ''], [t('Model'), ''], [t('Today'), 'num'], [t('In all'), 'num'], [t('Limit'), 'num']]) { const th = el('th', cls, name); th.setAttribute('scope', 'col'); head.append(th); }
  const thead = el('thead'); thead.append(head); table.append(thead);
  const body = el('tbody'); const money = (v: number | null | undefined) => (v === null || v === undefined || !Number.isFinite(v)) ? '–' : `$${v.toFixed(2)}`;
  for (const k of keySpend) {
    const tr = el('tr'); const th = el('th', '', k.name); th.setAttribute('scope', 'row'); tr.append(th);
    const c = k.status?.credit ?? null;
    tr.append(el('td', '', k.status?.model ?? '–'));
    if (k.error || (!c && k.status?.credit_error)) { const td = el('td', 'usage-error', k.error ? t('Not read') : t(k.status!.credit_error!)); td.colSpan = 3; tr.append(td); }
    else tr.append(el('td', 'num', money(c?.usage_daily)), el('td', 'num', money(c?.usage)), el('td', 'num', money(c?.limit)));
    body.append(tr);
  }
  table.append(body); const wrap = el('div', 'stat-scroll'); wrap.append(table); section.append(wrap);
  return section;
}
function renderStats(main: HTMLElement) {
  if (!stats && !statsError) void loadStats();
  const range = el('div', 'segmented'); range.setAttribute('role', 'group'); range.setAttribute('aria-label', t('Period'));
  for (const days of [7, 30, 90]) {
    const choice = button(plural(days, { one: '{n} day', other: '{n} days' }), () => { statsDays = days; stats = null; void loadStats(); render(); restoreFocus(`stats-${days}`); }, `segment${statsDays === days ? ' is-on' : ''}`, `stats-${days}`);
    choice.setAttribute('aria-pressed', String(statsDays === days)); range.append(choice);
  }
  const page = el('div', 'stat-page');
  const bar = el('div', 'stat-bar'); bar.append(range); page.append(bar); main.append(page);
  if (statsError) { page.append(el('p', 'usage-error', t('Statistics are unavailable. Refresh to retry.')), button(t('Retry'), () => { statsError = false; void loadStats(); render(); restoreFocus('stats-retry'); }, 'button', 'stats-retry')); return; }
  if (!stats) { page.append(el('p', 'form-note', t('Reading statistics…'))); return; }
  const s = stats;
  if (!s.total.requests) {
    page.append(emptyState(t('Nothing counted yet'), t('Requests that Claude Code, Codex and other agents send through Switchboard appear here: by group, account and agent, with the tokens the provider reports.')));
    page.append(statKeysPanel());
  } else {
    const cache = statCacheShare(s.total); const perRequest = statTokensPerRequest(s.total);
    const tiles = el('div', 'stat-tiles');
    tiles.append(
      statTile(t('Requests'), compactCount(s.total.requests), t('{count} answered', { count: compactCount(s.total.ok) })),
      statTile(t('Tokens'), compactCount(statTokens(s.total)), t('{sent} sent · {received} received', { sent: compactCount(s.total.input_tokens + s.total.cache_write_tokens), received: compactCount(s.total.output_tokens) })),
      statTile(t('From cache'), cache === null ? '–' : `${cache}%`, t('{count} tokens read from the cache', { count: compactCount(s.total.cache_read_tokens) })),
      statTile(t('Per request'), perRequest === null ? '–' : compactCount(perRequest), t('tokens on average')),
      statTile(t('Limits and errors'), compactCount(s.total.limited + s.total.failed), statTrouble(s.total) === null ? '–' : t('{percent} of requests', { percent: statTroubleText(s.total) })),
    );
    page.append(tiles, statChart(s));
    const groupName = (pool: string, name: string | null) => name ?? poolName(pool);
    const projectName = (name: string | null) => name ?? t('No folder recorded');
    const accountName = (label: string | null) => label ?? t('Removed account');
    const grid = el('div', 'stat-grid');
    grid.append(
      ...(s.organizations?.length ? [statTable('stat-organizations', t('By organization'), [t('Organization')], s.organizations.map((r) => ({ cells: [r.organization === 'personal' ? t('Personal') : r.organization === 'unknown' ? t('Removed account') : r.name ?? t('Removed organization')], counts: r })), true)] : []),
      statTable('stat-groups', t('By group'), [t('Group')], s.groups.map((r) => ({ cells: [groupName(r.pool, r.name)], counts: r })), true),
      statTable('stat-projects', t('By repository'), [t('Repository')], s.projects.map((r) => ({ cells: [[projectName(r.name), r.groups.map((g) => groupName(g.pool, g.name)).join(', ')]], counts: r })), true),
      statTable('stat-accounts', t('By account'), [t('Account')], s.accounts.map((r) => ({ cells: [`${accountName(r.label)}${r.provider ? ` · ${providerName(r.provider as Provider)}` : ''}`], counts: r })), true),
      statTable('stat-agents', t('By agent'), [t('Agent')], s.agents.map((r) => ({ cells: [statAgentName(r.agent)], counts: r })), true),
    );
    page.append(grid, statKeysPanel(),
      statTable('stat-routes', t('Who works where'), [t('Repository'), t('Group'), t('Account'), t('Agent')], s.routes.map((r) => ({ cells: [projectName(r.project_name), groupName(r.pool, r.name), accountName(r.label), statAgentName(r.agent)], counts: r }))),
      statTable('stat-days', t('By day'), [t('Day')], s.by_day.map((r) => ({ cells: [r.day], counts: r }))),
    );
  }
  page.append(el('p', 'surface-note', t('What is counted where: Claude Code and Codex sessions that go through Switchboard (managed launches, group folders, the Codex switch) and agents on an API-key account through its proxy are counted above, each request placed in the repository of the group folder it came from; “No folder recorded” holds requests counted before 0.6.53, when the folder was not kept, and requests that named no group folder. Agents on OpenRouter (Hermes, OpenCode, Pi and others) go to OpenRouter directly: their spend is under API keys, per key. Kimi Code runs on its own subscription: its usage is on its card under Accounts. A session on its own sign-in outside Switchboard is not counted. Token counts are the ones the provider reports; counts only, never a prompt, an answer or a path; kept for 90 days.') + (s.since ? ` ${t('Counted since {date}.', { date: s.since })}` : '')));
}
let sessionList: SessionInfo[] | null = null;
let sessionsError = false;
async function loadSessions(stamp?: number) {
  try { const value = (await adapter.sessions()).sessions; if (stamp === undefined || clock.accepts(stamp)) { sessionList = value; sessionsError = false; } } catch { sessionsError = true; }
  backgroundRender();
}
/** SB-65: the sessions Switchboard launched that start or run, read from their records (SB-95). */
function sessionsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'sessions-heading');
  const heading = el('h2', '', t('Sessions')); heading.id = 'sessions-heading'; panel.append(heading);
  if (sessionsError && !sessionList) { panel.append(el('p', 'usage-error', t('Sessions are unavailable. Refresh to retry.'))); return panel; }
  if (sessionsError) panel.append(el('p', 'usage-error', t('The list may be out of date: the last read failed.')));
  if (!sessionList) { panel.append(el('p', 'form-note', t('Reading sessions…'))); return panel; }
  if (!sessionList.length) { panel.append(el('p', 'form-note', t('No session Switchboard launched is running. Sessions started outside Switchboard are not listed.'))); return panel; }
  const list = el('ul', 'session-list');
  for (const session of sessionList) {
    const row = el('li', 'session-row'); const title = el('div', 'event-title');
    const name = el('span', 'session-name');
    name.append(el('strong', '', `${sessionAgentName(session)} · ${session.label ?? session.account_id}`));
    // SB-110/SB-111: an organization's context shows as its badge beside the name; personal shows nothing.
    const contextOrg = session.context?.startsWith('org:') ? snapshot?.organizations?.find((o) => `org:${o.id}` === session.context) : undefined;
    if (contextOrg) { const badge = el('span', 'badge org-badge', contextOrg.name); badge.title = t("Runs in {name}'s context.", { name: contextOrg.name }); name.append(' ', badge); }
    title.append(name, el('span', `session-state is-${session.state}`, sessionStateLabel(session.state)));
    const facts = el('p', 'form-note', session.started_at ? t('{how} · started {date}', { how: sessionHow(session, snapshot?.projects), date: date(session.started_at) }) : sessionHow(session, snapshot?.projects));
    row.append(title, el('code', 'address', session.working_directory), facts); list.append(row);
  }
  panel.append(list);
  return panel;
}
function renderActivity(main: HTMLElement) {
  main.append(sessionsPanel());
  if (!snapshot!.events.length) { main.append(emptyState(t('No activity yet'), t('Adding accounts, changing routes, and launching sessions will appear here. Credentials and prompts are never part of this journal.'))); return; }
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'events-heading');
  const heading = el('h2', '', t('Events')); heading.id = 'events-heading'; panel.append(heading);
  const list = el('ol', 'event-list');
  for (const event of [...snapshot!.events].reverse()) {
    const row = el('li', 'event-row'); const title = el('div', 'event-title');
    const account = snapshot!.accounts.find((item) => item.id === event.account_id);
    title.append(el('strong', '', eventAction(event.action)), el('time', '', date(event.at)));
    row.append(title, el('p', '', eventDetail(event.detail)));
    if (event.account_id) row.append(el('span', 'event-id', `${account ? `${account.label} · ` : ''}${event.account_id}`)); list.append(row);
  }
  panel.append(list, el('p', 'surface-note', t('A bounded local journal. A terminal launch is not proof of a successful provider request.')));
  main.append(panel);
}
function renderAbout(main: HTMLElement) {
  const section = el('section', 'about-panel');
  section.append(el('h2', '', t('A local account workbench')), el('p', '', t('Switchboard stores credentials in the native vault and manages private homes for Claude Code and Codex CLI. Account labels and imported identities are user claims until a provider confirms them.')));
  const definitions = el('dl', 'definitions');
  for (const [term, description] of [
    [t('Isolated launch'), t('A new official CLI session using this account’s private home. Account changes affect new launches.')],
    [t('Managed launch'), t('A session routed through the local HTTP/SSE proxy. Select an account for each provider and group; the next request uses that selection. A response already in progress keeps its identity.')],
    [t('Groups'), t('Repositories with accounts of their own, such as a client\'s. Accounts in no other group are in the default group, which works in every other folder. Selection never silently falls back to another group.')],
    [t('Usage'), t('A timestamped provider observation. Unknown, stale, and unavailable are distinct from measured zero. API billing is separate from subscription quota.')],
    [t('Build'), demo ? t('Synthetic browser demo; it runs without the native app.') : t('{platform} build. Native platform is reported by the running app.', { platform: platformLabel(runtime?.platform) })],
    [t('Compatibility'), t('macOS, Windows and Linux builds. Live provider acceptance is a separate check on each platform. Existing external CLI sessions and Codex Desktop are not controlled.')],
    [t('Imported OAuth'), t('A snapshot of a credential you pasted. After you sign in again elsewhere, add the account the CLI uses again. Switchboard renews inactive Claude accounts; the account Claude Code uses is renewed by Claude Code.')],
    [t('Switch'), t('Switch changes the account of the ordinary Claude Code. New claude sessions start on it; a session already running may keep its account until it restarts.')],
    [t('Automatic switching'), t('Off by default for each provider, group and target. Uses fresh quota observations, a threshold, a minimum improvement and a cooldown. No eligible account means the current account stays selected.')],
  ]) { definitions.append(el('dt', '', term), el('dd', '', description)); }
  section.append(definitions); const tour = el('section', 'about-panel'); tour.append(el('h2', '', t('Tour')), el('p', '', t('Five short steps: what Switchboard does and where to press.')), button(t('Show the tour again'), () => tourGo(0), 'button', 'tour-again'));
  main.append(section, termsPanel(), tour, residencyPanel(), backupsPanel(), analyticsPanel(), appearancePanel(), languagePanel(), productPanel());
}
async function loadBackups(stamp?: number) {
  try { const value = await adapter.backups(); if (stamp === undefined || clock.accepts(stamp)) { backupStatus = value; backupError = false; } } catch { backupError = true; }
  // Restored on its own at start (a reinstall): say so once per backup.
  const auto = backupStatus?.restored_at_start;
  if (auto) {
    const key = `switchboard.restored.${auto.file}`; let shown = false;
    try { shown = !!localStorage.getItem(key); localStorage.setItem(key, '1'); } catch { /* shown this run only */ }
    if (!shown) { showRestored(auto.restored); showNotice(t('Switchboard found your backup from {date} and restored it: {what}', { date: date(auto.created_at), what: restoredText })); restoredText = ''; try { snapshot = await adapter.snapshot(); } catch { /* the next refresh shows them */ } }
  }
  backgroundRender();
}
/** SB-28: closing the window keeps Switchboard working; it opens at login in the background. */
function residencyPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'residency-heading');
  const heading = el('h2', '', t('Running in the background')); heading.id = 'residency-heading';
  panel.append(heading, el('p', '', (trayInArea() ? t('Closing the window keeps Switchboard running, so quota checks, automatic switching, sign-in renewal and backups continue. To stop it, choose Quit Switchboard from its icon in the notification area.') : t('Closing the window keeps Switchboard running, so quota checks, automatic switching, sign-in renewal and backups continue. To stop it, choose Quit Switchboard from its menu-bar icon or the app menu.'))));
  if (loginItemError) { panel.append(el('p', 'form-note', t('The login setting is unavailable. Refresh to retry.'))); updateControls(panel); return panel; }
  if (!loginItem) { panel.append(el('p', 'form-note', t('Reading the login setting…'))); updateControls(panel); return panel; }
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = loginItem.enabled; box.disabled = !loginItem.available; box.dataset.focus = 'login-item';
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { loginItem = await adapter.setLoginItem(wanted); }, wanted ? t('Switchboard will open at login, in the background.') : t('Switchboard will no longer open at login.'), 'login-item'); });
  option.append(box, el('span', '', t('Open at login, in the background')));
  panel.append(option, el('p', 'form-note', loginItem.available ? (trayInArea() ? t('Starts without a window; open it from its icon in the notification area.') : t('Starts without a window; open it from the menu-bar icon.')) : t('Available in the installed app.')));
  updateControls(panel);
  return panel;
}
/** SB-104: a manual check runs in the background — it may download and install for minutes — and
 * says how it ended; the rest of the window stays usable meanwhile. */
async function checkUpdatesNow() {
  if (updateState) { updateState = { ...updateState, state: 'checking' }; render(); restoreFocus('update-check'); }
  try { updateState = await adapter.checkForUpdates(); }
  catch (error) { showNotice(safeError(error), true); }
  await loadUpdate();
  if (updateState) showNotice(updateLine(updateState, date).text, updateState.state === 'failed');
  backgroundRender();
}
/** SB-55: the backend checks, downloads and installs on its own; the window only reads the state, once a minute while a check can still change it. */
async function loadUpdate() {
  try { updateState = await adapter.updateStatus(); updateError = false; } catch { updateError = true; }
  updatePoll.sync(!!updateState?.available && updateState.enabled && updateState.state !== 'ready');
  backgroundRender();
}
const updatePoll = intervalWhile(() => { void loadUpdate(); }, 60_000);
function updateControls(panel: HTMLElement) {
  if (updateError) { panel.append(el('p', 'form-note', t('The update setting is unavailable. Refresh to retry.'))); return; }
  if (!updateState) { panel.append(el('p', 'form-note', t('Reading the update setting…'))); return; }
  const status = updateState;
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = status.enabled; box.disabled = !status.available; box.dataset.focus = 'auto-update';
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { updateState = await adapter.setAutoUpdate(wanted); await loadUpdate(); }, wanted ? t('Switchboard will install new versions on its own.') : t('Automatic updates are off. Switchboard will not check for new versions.'), 'auto-update'); });
  option.append(box, el('span', '', t('Install updates automatically')));
  const line = updateLine(status, date);
  panel.append(option, el('p', 'form-note', line.text));
  if (line.restart) panel.append(button(t('Restart to update'), () => void mutate(async () => { updateState = await adapter.restartToUpdate(); }, t('Restarting Switchboard with the new version…'), 'update-restart'), 'button', 'update-restart'));
  // LC-16: the person can always ask, with automatic updates off too.
  else if (status.available && status.state !== 'checking' && status.state !== 'downloading') panel.append(button(t('Check for updates'), () => void checkUpdatesNow(), 'button', 'update-check'));
}
/** Anonymous usage analytics (docs/ANALYTICS.md): what is sent, and the switch every PassionCode.ai tool shares. */
function analyticsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'analytics-heading');
  const heading = el('h2', '', t('Usage analytics')); heading.id = 'analytics-heading';
  panel.append(heading, el('p', '', USAGE_COUNTED()));
  if (analyticsError) { panel.append(el('p', 'form-note', t('The analytics setting is unavailable. Refresh to retry.'))); return panel; }
  if (!analyticsState) { panel.append(el('p', 'form-note', t('Reading the analytics setting…'))); return panel; }
  // Issue #128: not answered yet. The same choice as the first-run question; Continue answers it.
  if (usageQuestion(analyticsState)) {
    const actions = el('div', 'usage-question-actions'); const shown = analyticsState.enabled; actions.append(usageSwitch('analytics'), button(t('Continue'), () => answerUsage('analytics', shown), 'button', 'usage-continue-about'));
    panel.append(el('p', 'usage-question-sent', usageQuestionText(analyticsState)), actions, el('p', 'form-note', t('This switch applies to every PassionCode.ai tool on this computer.')));
    return panel;
  }
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = analyticsState.enabled; box.disabled = !analyticsState.available; box.dataset.focus = 'analytics';
  // A deliberate choice: it always sets the shared switch (round 2), and the notice says what is now true.
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { analyticsState = await adapter.setAnalytics(wanted); }, () => usageAnswerText(wanted, analyticsState), 'analytics'); });
  option.append(box, el('span', '', t('Share anonymous usage counts')));
  panel.append(option, el('p', 'form-note', analyticsState.available ? t('This switch applies to every PassionCode.ai tool on this computer.') : t('Only installed release builds send analytics; this build sends nothing.')));
  return panel;
}
function backupsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'backups-heading');
  const heading = el('h2', '', t('Backups')); heading.id = 'backups-heading'; panel.append(heading);
  if (backupError || !backupStatus) { panel.append(el('p', 'form-note', backupError ? t('Backup status is unavailable. Refresh to retry.') : t('Reading backups…'))); return panel; }
  const status = backupStatus;
  panel.append(el('p', '', status.enabled ? t('Switchboard saves an encrypted copy of your accounts after every change and once a day, and keeps the newest ten.') : t('Automatic backups run in the desktop app.')));
  if (status.directory) panel.append(el('code', 'address', status.directory));
  panel.append(el('p', 'form-note', (onWindows() ? t('The key is protected by Windows for your user account. These backups restore after reinstalling Switchboard on this computer; they cannot be opened on another computer or by another Windows user.') : onLinux() ? t('The key stays in this computer’s Secret Service keyring. These backups restore after reinstalling Switchboard here; they cannot be opened on another computer or after the keyring is erased.') : t('The key stays in this Mac’s Keychain. These backups restore after reinstalling Switchboard on this Mac; they cannot be opened on another Mac or after the Keychain is erased.'))));
  if (status.last_error) panel.append(el('p', 'usage-error', t('The last automatic backup failed: {error}', { error: t(safeError(status.last_error)) })));
  const list = el('div', 'backup-list');
  if (!status.backups.length) list.append(el('p', 'form-note', t('No backups yet.')));
  // SB-104: a backup this computer cannot open is listed, never offered to restore.
  for (const backup of status.backups.slice(0, 5)) {
    const row = el('div', 'policy-row'); const copy = el('div');
    copy.append(el('strong', 'block', date(backup.created_at)), el('span', 'usage-caption', `${plural(backup.accounts, { one: '{n} account', other: '{n} accounts' })}`));
    if (backup.openable === false) { copy.append(el('span', 'usage-caption', ` · ${t('made on another computer')}`)); row.append(copy); list.append(row); continue; }
    row.append(copy, button(t('Restore…'), () => restoreDialog(backup), 'text-button', `restore-${backup.file}`));
    list.append(row);
  }
  panel.append(list, button(t('Back up now'), () => void mutate(async () => { await adapter.backupNow(); backupStatus = await adapter.backups(); }, t('Backup saved.'), 'backup-now'), 'button', 'backup-now'));
  return panel;
}
/** UX-22: restoring names the backup's date and what returns, and runs only on a second press. */
function restoreDialog(backup: BackupInfo) {
  const context = openDialog(t('Restore the backup from {date}?', { date: date(backup.created_at) }), plural(backup.accounts, { one: 'It holds {n} account. Accounts, groups, rules and settings in it come back, including any you removed since; accounts already here stay as they are.', other: 'It holds {n} accounts. Accounts, groups, rules and settings in it come back, including any you removed since; accounts already here stay as they are.' }));
  const confirm = submit(t('Restore')); context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { const result = await adapter.restoreBackup(backup.file); backupStatus = await adapter.backups(); showRestored(result); }, t('Backup restored.')); });
  context.actions.querySelector('button')?.focus();
}
let restoredText = '';
function showRestored(result: Restored) {
  const extra = [result.projects ? `${plural(result.projects, { one: '{n} group', other: '{n} groups' })}` : '', result.rules ? `${plural(result.rules, { one: '{n} rule', other: '{n} rules' })}` : '', result.settings ? t('settings') : ''].filter(Boolean);
  restoredText = `${plural(result.added, { one: '{n} account restored', other: '{n} accounts restored' })}${extra.length ? t(', with {what}', { what: extra.join(', ') }) : ''} · ${t('{n} already here', { n: result.skipped })}${result.failed ? ` · ${t('{n} could not be restored', { n: result.failed })}` : ''}.`;
}
/** Language (L10N-01): the system's by default; a choice here reloads the window in that language. */
function languagePanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'language-heading');
  const heading = el('h2', '', t('Language')); heading.id = 'language-heading';
  const group = el('fieldset', 'appearance-options'); group.append(el('legend', 'sr-only', t('Language')));
  let chosen: LocaleChoice = 'system';
  try { chosen = parseLocaleChoice(localStorage.getItem(LOCALE_KEY)); } catch { chosen = 'system'; }
  for (const [value, text] of [['system', t('Same as system')], ['en', 'English'], ['ru', 'Русский']] as const) {
    const option = el('label', 'appearance-option'); const radio = el('input'); radio.type = 'radio'; radio.name = 'language'; radio.value = value; radio.checked = chosen === value; radio.dataset.focus = `language-${value}`;
    radio.addEventListener('change', () => {
      if (!radio.checked) return;
      if (saveLocaleChoice(value)) location.reload();
      else { showNotice(t('The language could not be saved on this machine.'), true); render(); }
    });
    option.append(radio, el('span', '', text)); group.append(option);
  }
  panel.append(heading, group, el('p', 'form-note', t('Same as system follows the language of your operating system. Changing the language reloads the window.')));
  return panel;
}
function appearancePanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'appearance-heading');
  const heading = el('h2', '', t('Appearance')); heading.id = 'appearance-heading';
  const group = el('fieldset', 'appearance-options'); group.append(el('legend', 'sr-only', t('Appearance')));
  for (const [value, text] of [['system', t('System')], ['dark', t('Dark')], ['light', t('Light')]] as const) {
    const option = el('label', 'appearance-option'); const radio = el('input'); radio.type = 'radio'; radio.name = 'appearance'; radio.value = value; radio.checked = appearance === value; radio.dataset.focus = `appearance-${value}`;
    radio.addEventListener('change', () => { if (radio.checked) { setAppearance(value); render(); restoreFocus(`appearance-${value}`); announce(appearanceSaveFailed ? t('{theme} appearance applied. It could not be saved and resets when Switchboard restarts.', { theme: text }) : t('{theme} appearance applied.', { theme: text })); } });
    option.append(radio, el('span', '', text)); group.append(option);
  }
  panel.append(heading, group, el('p', 'form-note', appearanceSaveFailed ? t('This choice could not be saved on this machine. It applies until Switchboard restarts.') : t('System follows the light or dark setting of your operating system. The choice is saved on this machine.')));
  return panel;
}
/** Tauri has no URL opener here, so native builds show a selectable address; the browser demo links it. */
function address(url: string) {
  if (native) { const text = el('code', 'address', url); return text; }
  const link = el('a', 'address', url); link.href = url; link.target = '_blank'; link.rel = 'noopener noreferrer'; link.dataset.focus = `link-${url}`; return link;
}
function productPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'product-heading');
  const heading = el('h2', '', t('Version and license')); heading.id = 'product-heading';
  const definitions = el('dl', 'definitions');
  const row = (term: string, ...content: (Node | string)[]) => { const dd = el('dd'); dd.append(...content); definitions.append(el('dt', '', term), dd); };
  row(t('Version'), `Fabric Switchboard ${version}`);
  row(t('License'), el('span', 'block', t('Open source under the GNU AGPL-3.0; a commercial license is available at passioncode.ai/business.')), address('https://passioncode.ai/business/'), address(`${REPOSITORY}/blob/main/LICENSE`));
  row(t('Third-party'), el('span', 'block', t('The app includes third-party components under their own licenses.')), address(`${REPOSITORY}/blob/main/THIRD_PARTY_NOTICES.md`));
  row(t('Toolkit'), el('span', 'block', t('Part of the PassionCode.ai toolkit.')), address('https://passioncode.ai/switchboard/'));
  panel.append(heading, definitions);
  if (native) panel.append(el('p', 'form-note', t('Addresses are selectable text. Copy one into your browser to open it.')));
  return panel;
}

interface DialogContext { dialog: HTMLDialogElement; form: HTMLFormElement; body: HTMLElement; actions: HTMLElement; error: HTMLElement; trigger: string | undefined; close: () => void; setBusy: (value: boolean) => void; beforeCancel: (handler: () => Promise<void>) => void }
/** `returnFocus` carries the original trigger when one dialog hands over to another. */
function openDialog(title: string, intro: string, returnFocus?: string): DialogContext {
  const trigger = returnFocus ?? (document.activeElement as HTMLElement | null)?.dataset.focus;
  const id = `dialog-${++dialogSequence}`;
  const dialog = el('dialog', 'dialog'); dialog.setAttribute('aria-labelledby', `${id}-title`); dialog.setAttribute('aria-describedby', `${id}-description`);
  const form = el('form'); const header = el('div', 'dialog-header');
  const heading = el('h2', '', title); heading.id = `${id}-title`; const description = el('p', '', intro); description.id = `${id}-description`; header.append(heading, description);
  const body = el('div', 'dialog-body'); const error = el('p', 'dialog-error'); error.setAttribute('role', 'alert');
  const actions = el('div', 'dialog-actions'); let pending = false;
  let cancelHandler: (() => Promise<void>) | undefined;
  const close = () => { if (!pending) dialog.close(); };
  const requestCancel = async () => {
    if (pending) return;
    if (!cancelHandler) { close(); return; }
    error.textContent = ''; setBusy(true);
    try { await cancelHandler(); setBusy(false); close(); }
    catch (failure) { error.textContent = t(safeError(failure)); setBusy(false); error.tabIndex = -1; error.focus(); }
  };
  const cancel = button(t('Cancel'), () => void requestCancel(), 'button'); actions.append(cancel);
  form.append(header, body, error, actions); dialog.append(form); document.body.append(dialog);
  dialog.addEventListener('cancel', (event) => { event.preventDefault(); void requestCancel(); });
  dialog.addEventListener('close', () => { form.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input,textarea').forEach((input) => { input.value = ''; }); dialog.remove(); if (!document.querySelector('dialog[open]')) restoreFocus(trigger); });
  dialog.showModal();
  const setBusy = (value: boolean) => { pending = value; form.setAttribute('aria-busy', String(value)); form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement | HTMLTextAreaElement>('input, button, select, textarea').forEach((node) => { node.disabled = value || node.dataset.locked === 'true'; }); };
  return { dialog, form, body, actions, error, trigger, close, setBusy, beforeCancel: (handler) => { cancelHandler = handler; } };
}
/** A labelled group for a control that is not one form element (a folder picker). */
function groupField(label: string, control: HTMLElement, help = '') {
  const wrapper = el('div', 'field'); wrapper.setAttribute('role', 'group');
  const caption = el('span', 'field-label', label); caption.id = `field-${++dialogSequence}`; wrapper.setAttribute('aria-labelledby', caption.id);
  wrapper.append(caption, control); if (help) wrapper.append(el('span', 'field-help', help)); return wrapper;
}
/** Opens the system folder picker at `start`; reports a failure through `report`, never as a typed path. */
async function chooseFolder(start: string, report: (text: string) => void): Promise<string | null> {
  try { const folder = await adapter.pickFolder(start || lastWorkingDirectory || undefined); if (folder) rememberFolder(folder); return folder; }
  catch (error) { report(t(safeError(error))); return null; }
}
/**
 * One folder, chosen with the system picker and never typed (operator decision 2026-10-09, every
 * PassionCode.ai product). `value()` is the chosen folder; `require()` returns it or, with none chosen,
 * says so in the dialog and focuses the button.
 */
function folderPicker(context: DialogContext, initial: string, key: string, locked = false) {
  let value = initial;
  const node = el('div', 'folder-picker'); const shown = el('span', 'folder-path');
  const choose = button('', () => void chooseFolder(value, (text) => { context.error.textContent = text; }).then((folder) => { if (folder) { value = folder; context.error.textContent = ''; paint(); } }), 'button row-button', key);
  const paint = () => { shown.textContent = value || t('No folder chosen'); shown.title = value; node.classList.toggle('is-empty', !value); choose.textContent = value ? t('Change…') : t('Choose folder…'); };
  if (locked) { choose.disabled = true; choose.dataset.locked = 'true'; choose.hidden = true; }
  node.append(shown, choose); paint();
  return {
    node, focus: () => choose.focus(), value: () => value,
    require: () => { if (value) return value; context.error.textContent = t('Choose a folder.'); choose.focus(); return null; },
  };
}
/** Several folders, each added or changed with the system picker and removable; never typed. */
function foldersPicker(context: DialogContext, initial: string[], key: string, roots: ProjectRoot[] | null = null) {
  let folders = [...initial];
  const node = el('div', 'folder-picker-list'); const list = el('ul', 'folder-list');
  const add = button(t('Add folder…'), () => void chooseFolder(folders.at(-1) ?? '', (text) => { context.error.textContent = text; }).then((folder) => { if (folder) { folders = withFolder(folders, folder); context.error.textContent = ''; paint(); } }), 'button row-button', `${key}-add`);
  const paint = () => {
    list.replaceChildren();
    if (!folders.length) list.append(el('li', 'folder-path is-empty', t('No folder chosen')));
    folderRows(folders, roots).forEach((row) => {
      const folder = row.folder;
      const item = el('li', row.depth ? 'folder-item derived-folder' : 'folder-item'); const path = el('span', 'folder-path', folderName(folder)); path.title = folder;
      // SB-135: a worktree or clone sits under its root, and goes when the root is removed.
      if (row.depth) path.append(el('span', 'folder-note', ` · ${derivedText(row.kind, row.branch)}`));
      const change = button(t('Change…'), () => void chooseFolder(folder, (text) => { context.error.textContent = text; }).then((chosen) => { if (chosen && chosen !== folder) { folders = replaceFolder(folders, folder, chosen); context.error.textContent = ''; paint(); } }), 'text-button', '');
      change.setAttribute('aria-label', t('Change {folder}', { folder }));
      const remove = button('×', () => { folders = withoutFolder(folders, roots, folder); paint(); add.focus(); }, 'icon-button', '');
      remove.setAttribute('aria-label', row.derived ? t('Remove {folder} with its {n} worktrees and clones', { folder, n: row.derived }) : t('Remove {folder}', { folder })); item.append(path, change, remove); list.append(item);
    });
  };
  node.append(list, add); paint();
  return { node, focus: () => add.focus(), value: () => folders, require: () => { if (folders.length) return folders; context.error.textContent = t('Choose a folder.'); add.focus(); return null; } };
}
function field(label: string, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement, help = '') {
  const wrapper = el('label', 'field'); wrapper.append(el('span', 'field-label', label), input); if (help) wrapper.append(el('span', 'field-help', help)); return wrapper;
}
function input(value = '', type = 'text') { const node = el('input'); node.type = type; node.value = value; node.required = true; node.autocomplete = 'off'; node.spellcheck = false; return node; }
function select(options: [string, string][]) { const node = el('select'); options.forEach(([value, text]) => { const option = el('option', '', text); option.value = value; node.append(option); }); return node; }
function submit(text: string) { const node = el('button', 'button primary', text); node.type = 'submit'; return node; }
async function dialogSave(context: DialogContext, action: () => Promise<unknown>, success: string, operation: Operation = 'change') {
  context.error.textContent = ''; context.setBusy(true); clock.begin();
  try { await action(); await refreshContext(); showNotice(restoredText || success); restoredText = ''; try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = t('The action completed, but the list could not be refreshed. Retry loading accounts.'); } render(); context.setBusy(false); context.close(); loadOnboarding(); }
  catch (error) { context.error.textContent = t(safeError(error, operation)); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); }
  finally { clock.end(); }
}
/**
 * Launch any account in a folder chosen with the picker (operator decision 2026-10-09). The box
 * runs it isolated, in the account's own home; cleared, it goes through the local proxy as a managed
 * session — open only to the pool's selected account while the proxy runs (`launchChoice`).
 */
function launchDialog(account: Account) {
  const choice = launchChoice(selected(account), !runtimeError);
  const context = openDialog(t('Launch {label}', { label: account.label }), t('{provider} · {group}', { provider: providerName(account.provider), group: poolName(account.pool, snapshot?.projects) }));
  const folder = folderPicker(context, lastWorkingDirectory, `launch-folder-${account.id}`);
  const isolated = input('', 'checkbox'); isolated.required = false; isolated.checked = choice.isolatedByDefault;
  if (!choice.managed) { isolated.disabled = true; isolated.dataset.locked = 'true'; }
  const box = el('label', 'checkbox-field'); box.append(isolated, el('span', '', t('Isolated session')));
  const note = el('p', 'form-note');
  const explain = () => { note.textContent = demo ? t('Synthetic launch only. The demo does not open a terminal.') : isolated.checked ? t('Runs in this account’s own home and keeps this account; automatic switching does not move it. Existing clients are not changed.') : t('Goes through the local proxy: new requests use the selected account of its group, and automatic switching can move it. In-progress responses keep their account.'); };
  isolated.addEventListener('change', explain); explain();
  context.body.append(groupField(t('Folder'), folder.node), box);
  if (!choice.managed) context.body.append(el('p', 'form-note', runtimeError ? t('The local proxy is not running, so this session is isolated.') : t('Only the account selected {where} launches through the proxy, so this session is isolated.', { where: inGroup(account.pool, snapshot?.projects) })));
  context.body.append(note);
  context.actions.append(submit(t('Launch')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const workingDirectory = folder.require(); if (!workingDirectory) return;
    const mode = isolated.checked ? 'isolated' : 'managed';
    let tools = true;
    const noTools = t(' Agents in this session have no Switchboard tools: link the command-line tool under Agents, then launch again.');
    void dialogSave(context, async () => { const result = await adapter.launch(account.id, mode, workingDirectory); tools = result.agent_tools !== false; }, demo ? mode === 'managed' ? t('Synthetic managed launch recorded. No terminal was opened.') : t('Synthetic isolated launch recorded. No terminal was opened.') : mode === 'managed' ? t('Terminal launch requested in your folder through the local proxy. Selection takes effect on the next request.') : t('Terminal launch requested in your folder with this account’s private home. Provider acceptance is not yet observed.'), 'launch').then(() => { if (!tools && !noticeError) { showNotice(notice + noTools); render(); } });
  });
  folder.focus();
}
function editDialog(account: Account) {
  const context = openDialog(t('Edit account'), t('{provider} · {group}. To change credentials, add the account again; to change the group, use Move to another group.', { provider: providerName(account.provider), group: poolName(account.pool, snapshot?.projects) }));
  const label = input(account.label); label.maxLength = 80; const enabled = input('', 'checkbox'); enabled.required = false; enabled.checked = account.enabled;
  const enabledLabel = el('label', 'checkbox-field'); enabledLabel.append(enabled, el('span', '', t('Enabled for selection and launch')));
  context.body.append(field(t('Account label'), label), enabledLabel, el('p', 'form-note', t('Disabling a selected account clears its route. Managed requests in its group fail until you select another account.')));
  context.actions.append(submit(t('Save changes')));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); if (!label.value.trim()) { label.setCustomValidity(t('Enter an account label.')); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; } void dialogSave(context, () => adapter.update(account.id, label.value.trim(), enabled.checked), enabled.checked ? t('Account updated.') : t('Account disabled. Any selection for this account has been cleared.')); }); label.focus();
}
/** SB-122: the account card's ⋯ → *Move to another pool…*: the default pool or a project, the current one marked (SCN-062). */
function moveDialog(account: Account, inCli: boolean) {
  const context = openDialog(t('Move to another group'), t('Choose where “{label}” is used. A group’s accounts work only in its folders; the default group works in every other folder.', { label: account.label }));
  const group = el('fieldset', 'appearance-options move-options'); group.append(el('legend', 'sr-only', t('Group')));
  const targets = moveTargets(account.pool, snapshot?.projects ?? [], inCli);
  const radios: HTMLInputElement[] = [];
  for (const target of targets) {
    const option = el('label', 'appearance-option'); const radio = el('input'); radio.type = 'radio'; radio.name = 'move-pool'; radio.value = target.pool; radio.checked = target.current; radio.disabled = target.blocked; radio.required = true;
    const text = target.name === null ? t('Default group') : t('Group “{name}”', { name: target.name });
    const label = target.current ? t('{target} (now)', { target: text }) : text;
    option.append(radio, target.name === null ? el('span', '', label) : groupHint(label, target.pool, false)); group.append(option); radios.push(radio);
  }
  context.body.append(group);
  if (inCli) context.body.append(el('p', 'form-note', t('This account is signed in to Claude Code or Codex, which every folder uses, so it cannot join a group. Switch the CLI to another account first.')));
  if (!snapshot?.projects?.length) context.body.append(el('p', 'form-note', t('There are no groups yet. Create one under Groups, then move accounts into it.')));
  const confirm = submit(t('Move')); context.actions.append(confirm);
  const sync = () => { const chosen = radios.find((r) => r.checked); confirm.disabled = !chosen || chosen.value === account.pool; };
  radios.forEach((radio) => radio.addEventListener('change', sync)); sync();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const chosen = radios.find((r) => r.checked); if (!chosen || chosen.value === account.pool) return;
    const target = targets.find((x) => x.pool === chosen.value)!;
    const success = target.name === null ? t('{label} moved to the default group.', { label: account.label }) : t('{label} moved to group “{name}”.', { label: account.label, name: target.name });
    void dialogSave(context, () => adapter.moveAccount(account.id, chosen.value), success);
  });
  (radios.find((r) => r.checked && !r.disabled) ?? radios.find((r) => !r.disabled))?.focus();
}
/** SB-158: the organization an account belongs to, with why, for its badge. */
function organizationOf(account: Account): { name: string; why: string } | null {
  const at = attribution(snapshot!, account);
  const org = at.organization ? snapshot!.organizations?.find((o) => o.id === at.organization) : undefined;
  if (!org) return null;
  return { name: org.name, why: byText(at.by, org.name) };
}
function byShort(by: AttributedBy | null) {
  return by === 'person' ? t('placed by you') : by === 'provider' ? t('by its team') : t('by its domain');
}
function byText(by: AttributedBy | null, name: string) {
  if (by === 'person') return t('You placed it in {name}.', { name });
  if (by === 'provider') return t('Its team at the provider belongs to {name}.', { name });
  if (by === 'domain') return t('Its address is on a domain of {name}.', { name });
  return t('Personal');
}
/** SB-158: the filter and the way to organizations, above the accounts; one quiet line while there are none. */
function organizationsBar(accounts: Account[]): HTMLElement | null {
  const organizations = snapshot!.organizations ?? [];
  const bar = el('div', 'org-bar');
  if (!organizations.length) { orgFilter = 'all'; return null; }
  if (orgFilter !== 'all' && orgFilter !== 'personal' && !organizations.some((o) => o.id === orgFilter)) orgFilter = 'all';
  const count = (filter: string) => accounts.filter((a) => inOrganizationFilter(snapshot!, a, filter)).length;
  const choices: [string, string][] = [['all', t('All')], ...organizations.map((o): [string, string] => [o.id, o.name]), ['personal', t('Personal')]];
  const group = el('div', 'segmented org-filter'); group.setAttribute('role', 'group'); group.setAttribute('aria-label', t('Show accounts of'));
  for (const [value, text] of choices) {
    const chip = button(`${text} · ${count(value)}`, () => { orgFilter = value; render(); restoreFocus(`org-filter-${value}`); }, `segment ${orgFilter === value ? 'is-on' : ''}`, `org-filter-${value}`);
    chip.setAttribute('aria-pressed', String(orgFilter === value)); group.append(chip);
  }
  bar.append(group);
  return bar;
}
/** SB-159 (SCN-071): the PassionCode.ai connection, read when the page opens. */
let passioncodeView: PassioncodeStatus | null = null; let passioncodeWaiting = false; let passioncodeError = '';
async function loadPassioncode() {
  try { passioncodeView = await adapter.passioncode('status'); passioncodeError = ''; } catch (error) { passioncodeError = safeError(error, 'read'); }
  backgroundRender();
}
/** UX-21: the person pressed Cancel: a plain notice, not an error. */
const PASSIONCODE_CANCELLED = 'Connecting PassionCode.ai was cancelled. Nothing changed.';
async function passioncodeAct(action: 'connect' | 'read' | 'disconnect') {
  if (action === 'connect') { passioncodeWaiting = true; render(); }
  try {
    passioncodeView = await adapter.passioncode(action); passioncodeError = '';
    snapshot = await adapter.snapshot();
    if (action === 'connect') showNotice(t('Connected to PassionCode.ai: {organizations}.', { organizations: plural(passioncodeView.organizations ?? 0, { one: '{n} organization', other: '{n} organizations' }) }));
    else if (action === 'disconnect') showNotice(t('PassionCode.ai disconnected. The organizations stay here.'));
  } catch (error) { const text = safeError(error); showNotice(text, text !== PASSIONCODE_CANCELLED); }
  finally { passioncodeWaiting = false; render(); restoreFocus(`passioncode-${action}`); }
}
function passioncodePanel() {
  const panel = el('section', 'org-card passioncode-panel'); panel.setAttribute('aria-label', 'PassionCode.ai');
  const head = el('div', 'org-card-head'); const titles = el('div', 'org-card-title');
  const status = passioncodeView;
  const line = passioncodeLine(status);
  const values: Record<string, string> = Object.fromEntries(Object.entries(line.values ?? {}).map(([k, v]) => [k, k === 'date' ? date(Number(v)) : String(v)]));
  if (line.values?.count !== undefined) values.organizations = plural(Number(line.values.count), { one: '{n} organization', other: '{n} organizations' });
  const lineText = passioncodeWaiting ? t('Waiting for PassionCode.ai in your browser…') : passioncodeError ? t(passioncodeError) : t(line.key, values);
  const lineNode = el('span', 'row-sub wraps', lineText); lineNode.title = lineText;
  titles.append(el('h2', '', 'PassionCode.ai'), lineNode);
  const actions = el('div', 'org-card-actions');
  if (passioncodeWaiting) actions.append(button(t('Cancel'), () => { void adapter.passioncode('cancel').catch((error) => { showNotice(safeError(error), true); render(); }); }, 'button row-button quiet', 'passioncode-cancel'));
  else if (!status?.connected || status.state === 'sign_in_again') actions.append(button(t('Connect PassionCode.ai…'), () => void passioncodeAct('connect'), 'button row-button', 'passioncode-connect'));
  else {
    actions.append(button(t('Read now'), () => void passioncodeAct('read'), 'button row-button quiet', 'passioncode-read'));
    actions.append(menu('passioncode', t('More actions for {name}', { name: 'PassionCode.ai' }), [[t('Disconnect'), () => void passioncodeAct('disconnect'), 'danger-text']]));
  }
  if (passioncodeWaiting) actions.querySelectorAll('button').forEach((b) => { b.disabled = false; });
  head.append(titles, actions); panel.append(head);
  return panel;
}
/** SB-158: the Organizations page: what makes an account a company's, then each organization with its domains and the accounts it holds. */
function renderOrganizations(main: HTMLElement) {
  const organizations = snapshot!.organizations ?? [];
  main.append(passioncodePanel());
  if (!organizations.length) {
    const empty = emptyState(t('No organizations yet'), t('Add the companies you work for with their e-mail domains. Accounts on those domains get the company’s badge on Accounts, and you can place any account yourself. Without organizations nothing changes.'));
    empty.append(button(t('+ New organization'), () => organizationDialog(null), 'button primary', 'org-empty-add'));
    main.append(empty);
    return;
  }
  main.append(el('p', 'surface-note org-rules', t('An account belongs to an organization when you place it there, when its team at the provider is the organization’s, or when its address is on one of the organization’s domains. Placing one seat of a Claude Team or ChatGPT workspace brings the team’s other seats along.')));
  const list = el('div', 'org-cards');
  for (const o of organizations) {
    const members = snapshot!.accounts.filter((a) => attribution(snapshot!, a).organization === o.id);
    const card = el('section', 'org-card'); card.setAttribute('aria-label', o.name);
    const head = el('div', 'org-card-head');
    const titles = el('div', 'org-card-title'); const heading = el('h2', '', o.name);
    if (o.vesta_id) { const mark = el('span', 'badge org-badge', 'PassionCode.ai'); mark.title = t('Its name and domains come from PassionCode.ai.'); heading.append(' ', mark); }
    titles.append(heading, el('span', 'row-sub', (o.domains ?? []).join(', ') || t('No domain')));
    const actions = el('div', 'org-card-actions');
    actions.append(button(t('Edit…'), () => organizationDialog(o.id), 'button row-button quiet', `org-edit-${o.id}`));
    actions.append(menu(`org-${o.id}`, t('More actions for {name}', { name: o.name }), [[t('Remove…'), () => organizationRemoveDialog(o.id), 'danger-text']]));
    head.append(titles, actions);
    card.append(head);
    // SB-160 (B-7, SCN-079): left on PassionCode.ai; kept here, and the person decides what to remove.
    const releasedAt = organizationReleasedAt(passioncodeView, o);
    if (releasedAt !== null) card.append(el('p', 'surface-note org-released', t('Not in your PassionCode.ai organizations since {date}. Its groups still refuse other accounts. Remove it when you no longer need it.', { date: date(releasedAt) })));
    if (!members.length) card.append(el('p', 'form-note', t('No account belongs to it yet. Place one from its ⋯ menu on Accounts.')));
    else {
      const rows = el('ul', 'org-members');
      for (const a of members) {
        const at = attribution(snapshot!, a);
        const row = el('li', 'org-member');
        const why = el('span', 'row-sub', `${providerName(a.provider)} · ${byShort(at.by)}`); why.title = byText(at.by, o.name);
        const memberName = el('span', 'org-member-name', a.label); memberName.title = a.label;
        row.append(el('span', `provider-dot ${a.provider}`), memberName, why);
        rows.append(row);
      }
      card.append(el('span', 'eyebrow', plural(members.length, { one: '{n} account', other: '{n} accounts' })), rows);
    }
    list.append(card);
  }
  main.append(list);
}
/** SB-158: add an organization (`id` null) or change one: its name and the domains it owns. */
function organizationDialog(id: string | null) {
  const current = id ? snapshot!.organizations?.find((o) => o.id === id) : undefined;
  const context = openDialog(current ? t('Edit {name}', { name: current.name }) : t('Add organization'), t('Accounts with an address on these domains belong to it. You can always place an account yourself.'));
  const name = input(current?.name ?? ''); name.maxLength = 80; name.placeholder = t('Company name');
  const domains = input((current?.domains ?? []).join(', ')); domains.required = false; domains.placeholder = 'example.com';
  const grid = el('div', 'form-grid'); grid.append(field(t('Name'), name), field(t('Domains'), domains, t('Optional. Separate several with commas.')));
  context.body.append(grid);
  context.actions.append(submit(current ? t('Save') : t('Add')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const list = domainList(domains.value);
    void dialogSave(context, () => current ? adapter.organizationEdit(current.id, name.value.trim(), list) : adapter.organizationAdd(name.value.trim(), list), current ? t('{name} saved.', { name: name.value.trim() }) : t('{name} added.', { name: name.value.trim() }));
  });
  name.focus();
}
function organizationRemoveDialog(id: string) {
  const o = snapshot!.organizations?.find((x) => x.id === id); if (!o) return;
  const context = openDialog(t('Remove {name}?', { name: o.name }), t('Its accounts stay where they are; they are placed again by their team or domain, or become personal. Nothing at the provider changes.'));
  const confirm = submit(t('Remove')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, () => adapter.organizationRemove(id), t('{name} removed.', { name: o.name })); });
  confirm.focus();
}
/** SB-158: the account card's ⋯ → *Organization…*: automatic, personal, or one organization. */
function assignDialog(account: Account) {
  const choice = snapshot!.memberships?.[account.id] ?? 'auto';
  const automatic = attribution({ organizations: snapshot!.organizations }, account);
  const autoName = automatic.organization ? snapshot!.organizations?.find((o) => o.id === automatic.organization)?.name : null;
  const context = openDialog(t('Organization of {label}', { label: account.label }), t('Automatic follows its team at the provider, then its address’s domain.'));
  const picker = select([['auto', autoName ? t('Automatic: {name}', { name: autoName }) : t('Automatic: personal accounts')], ...(snapshot!.organizations ?? []).map((o): [string, string] => [o.id, o.name]), ['personal', t('Personal accounts')]]);
  picker.value = choice;
  context.body.append(field(t('Belongs to'), picker));
  context.actions.append(submit(t('Save')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    void dialogSave(context, () => adapter.organizationAssign(account.id, picker.value === 'auto' ? null : picker.value), t('Saved for {label}.', { label: account.label }));
  });
  picker.focus();
}
function removeDialog(account: Account) {
  const active = selected(account);
  const context = openDialog(t('Remove account?'), t('Remove “{label}” from Switchboard and delete its stored credential. This cannot be undone.', { label: account.label }));
  context.body.append(el('p', 'form-note', active ? t('This account is selected for managed sessions in its group. Removing it disables it first, which clears that selection: managed requests there fail until you select another account.') : t('Existing external clients are not signed out. Managed account metadata and its vault entry will be removed.')));
  const confirm = submit(active ? t('Disable and remove') : t('Remove account')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    // Unit tier E: a removal that fails after the disable says the account is now disabled, and the list shows it.
    let disabledOnly: unknown = null;
    void dialogSave(context, async () => { if (active) await adapter.update(account.id, account.label, false); try { await adapter.remove(account.id); } catch (failure) { if (active) { disabledOnly = failure; try { snapshot = await adapter.snapshot(); render(); } catch { /* the next refresh shows it */ } } throw failure; } }, t('Account removed.'))
      .then(() => { if (disabledOnly) context.error.textContent = t('{label} is disabled now but was not removed: {reason}', { label: account.label, reason: t(safeError(disabledOnly)) }); });
  });
  context.actions.querySelector('button')?.focus();
}

/** SB-69: deleting a project asks first, like removing an account (SCN-042). */
function deleteProjectDialog(project: Project) {
  const context = openDialog(t('Delete group?'), t('Delete “{name}”. Its accounts stay saved and are no longer reserved for it; sessions in its folders use the usual selection again.', { name: project.name }));
  const confirm = submit(t('Delete group')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, () => adapter.removeProject(project.pool), t('Group “{name}” deleted. Its accounts stay saved and are no longer reserved for it.', { name: project.name })); });
  context.actions.querySelector('button')?.focus();
}
/** SB-69: removing a project rule asks first; pausing it is the reversible choice (SCN-042). */
function removeRuleDialog(rule: ProjectRule) {
  const context = openDialog(t('Remove rule?'), t('Remove the {provider} rule for {path}. Sessions there use the usual selection again. To stop it for a while, pause it instead.', { provider: providerName(rule.provider), path: rule.path }));
  const confirm = submit(t('Remove rule')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, () => adapter.removeProjectRule(rule.path, rule.provider), t('Rule removed.')); });
  context.actions.querySelector('button')?.focus();
}

function decisionText(reason: string) {
  const labels: Record<string, string> = {
    disabled: t('Automatic switching is off.'), cooldown: t('Waiting for the cooldown to end.'), below_threshold: t('Current usage is below the threshold.'),
    no_eligible_account: t('No eligible account. Holding the current account.'),
    stale_usage: t('Waiting for fresh usage. Holding the current account.'), usage_unavailable: t('Usage unavailable. Holding the current account.'),
    current_unavailable: t('Current account unavailable. Holding the current account.'), limit_reached: t('The account in use hit a provider limit. An unlimited account is available; a switch is not yet confirmed.'), limit_no_eligible_account: t('The account in use hit a provider limit, and no other account is free. Holding it.'), switched_on_limit: t('Switched after the account in use hit a provider limit.'), threshold_reached: t('Threshold reached. An eligible account is available; a switch is not yet confirmed.'), switched: t('The monitor recorded a switch.'), switch_failed: t('The switch failed. The current account stays selected; check Activity.'), activation_failed: t('Switching Claude Code failed. Check which account it uses, then switch by hand.'), claude_swap_switching: t('Claude Swap is switching Claude Code automatically, so Switchboard does not. Turn off automatic switching in one of them; manual switches still work.'),
  };
  return labels[reason] || t('The monitor has evaluated this policy. Check Activity for recorded changes.');
}
function policyDialog(existing?: RotationPolicy) {
  const context = openDialog(t('Automatic switching'), t('Choose the provider and group it watches. It moves only to a fresh account in the same group; with none, the current account stays.'));
  const provider = select([['claude', t('Claude Code')], ['codex', t('Codex CLI')]]);
  const pool = poolField(existing?.pool || 'default', !!existing);
  const target = select([['managed', t('Managed route')], ['claude_cli', t('Claude Code account')]]);
  const enabled = input('', 'checkbox'); enabled.required = false;
  const number = (value: number, min: number, max: number) => { const control = input(String(value), 'number'); control.min = String(min); control.max = String(max); control.step = '1'; return control; };
  const threshold = number(90, 0.1, 100); const hysteresis = number(10, 0, 100); threshold.step = 'any'; hysteresis.step = 'any'; const cooldown = number(1800, 0, 604800); const freshness = number(300, 1, 86400);
  const populate = (policy?: RotationPolicy) => {
    enabled.checked = policy?.enabled ?? false; threshold.value = String(policy?.threshold_percent ?? 90); hysteresis.value = String(policy?.hysteresis_percent ?? 10);
    cooldown.value = String(policy?.cooldown_seconds ?? 1800); freshness.value = String(policy?.max_age_seconds ?? 300);
  };
  const sync = () => {
    const nativeOption = target.querySelector<HTMLOptionElement>('option[value="claude_cli"]')!; nativeOption.disabled = provider.value !== 'claude';
    if (nativeOption.disabled && target.value === 'claude_cli') target.value = 'managed';
    populate(snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value() && policy.target === target.value));
  };
  provider.value = existing?.provider ?? 'claude'; target.value = existing?.target ?? 'managed'; sync(); populate(existing);
  if (existing) for (const control of [provider, target]) { control.disabled = true; control.dataset.locked = 'true'; }
  else { provider.addEventListener('change', sync); target.addEventListener('change', sync); pool.onChange(sync); }
  const grid = el('div', 'form-grid'); grid.append(field(t('Provider'), provider), pool.node, field(t('Target'), target), field(t('Switch at usage (%)'), threshold), field(t('Headroom below threshold (points)'), hysteresis, t('The candidate must stay this many points below the switching threshold.')), field(t('Cooldown (seconds)'), cooldown), field(t('Maximum usage age (seconds)'), freshness));
  const toggle = el('label', 'checkbox-field'); toggle.append(enabled, el('span', '', t('Turn on automatic switching')));
  context.body.append(grid, toggle, el('p', 'form-note', t('Switching Claude Code automatically needs OAuth accounts with a known owner. In managed sessions it takes effect from the next request; a response already in progress keeps its account.')));
  context.actions.append(submit(t('Save')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    hysteresis.setCustomValidity(Number(hysteresis.value) >= Number(threshold.value) ? t('Headroom must be less than the usage threshold.') : '');
    if (!context.form.reportValidity()) return;
    const previous = snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value() && policy.target === target.value);
    const policy: RotationPolicy = { provider: provider.value as Provider, pool: pool.value(), target: target.value as RotationPolicy['target'], enabled: enabled.checked, threshold_percent: Number(threshold.value), hysteresis_percent: Number(hysteresis.value), cooldown_seconds: Number(cooldown.value), max_age_seconds: Number(freshness.value), last_switched_at: previous?.last_switched_at ?? null };
    void dialogSave(context, () => adapter.setPolicy(policy), policy.enabled ? t('Automatic switching saved and turned on.') : t('Automatic switching saved; it is off.'));
  });
  hysteresis.addEventListener('input', () => hysteresis.setCustomValidity('')); threshold.addEventListener('input', () => hysteresis.setCustomValidity(''));
  (existing ? threshold : provider).focus();
}

render();
if (demo) { const { createDemoAdapter } = await import('./demo'); adapter = createDemoAdapter(); }
// SB-183 (node A-7): a first-time notice that waited (the window was closed) shows once the list is read.
void reload().then(() => checkShellHookNotice());
// Read monitor metadata on a bounded cadence. Quota probing belongs to the runtime.
let refreshing = false;
// Menus close on Escape and on a click anywhere outside them.
document.addEventListener('keydown', (event) => { if (event.key === 'Escape' && !openMenu && tourShown() && !document.querySelector('dialog[open]')) { tourGo(null); return; } if (event.key === 'Escape' && openMenu) { const key = openMenu; openMenu = null; render(); restoreFocus(`menu-${key}`); } });
document.addEventListener('click', (event) => { if (openMenu && !(event.target as HTMLElement | null)?.closest('.menu-root')) { openMenu = null; render(); } });
function backgroundRefresh() {
  if ((!native && !demo) || refreshing || openMenu || document.hidden || document.querySelector('dialog') || busy || loading) return;
  refreshing = true; const stamp = clock.stamp();
  // Issue #128 round 1: another PassionCode.ai tool may have turned the shared switch off since
  // the usage question was drawn; a fresh status drops a question that no longer applies.
  void adapter.analytics().then((value) => { if (clock.accepts(stamp)) { analyticsState = value; analyticsError = false; backgroundRender(); } }, () => { /* the next pass reads it again */ });
  void Promise.all([adapter.snapshot(), readContext()]).then(([data, context]) => {
    // A read issued before a user mutation may predate it; applying it would revert the change.
    if (!clock.accepts(stamp)) return;
    // A-7 fix cycle 1 (N7): a change made elsewhere (`switchboard shell on|off`, MCP
    // `switchboard_shell_hook_set`) reaches the Groups page at the next minute's read.
    if (page === 'projects') shellHookKey = '';
    snapshot = data; applyContext(context); backgroundRender();
    // B-8 seam read: the checklist lists sessions (SB-104); read it only where its card is drawn.
    if (page === 'accounts' || page === 'orgs') loadOnboarding();
    // SB-104: the session list is read only while it is on screen.
    if (page === 'activity') void loadSessions();
    // C-7: the ceilings are a local file read, only while a page that shows them is on screen.
    if (page === 'agents' || page === 'tokens') void loadCeilings();
    // SB-100: Kimi's figures refresh in the background too, every five minutes.
    if (Date.now() - kimiLoadedAt > 5 * 60_000) void loadKimi();
  }).catch(() => { /* Explicit Refresh reports a failed metadata read. */ }).finally(() => { refreshing = false; });
}
setInterval(backgroundRefresh, 60_000);
// SB-117/SB-131: the owner says when it added a folder to a project or took a deleted one out;
// the project list is read again at once.
onProjectSuggestions(() => backgroundRefresh());
// SB-183 (node A-7): the owner wrote the terminal hook for the first time; the window says so once.
onShellHook(() => { shellHookState = null; checkShellHookNotice(); backgroundRefresh(); });
// C-7 (SB-72): spend.json changed (a ceiling reached, set or answered); the questions are read again at once.
onSpendChanged(() => { void loadCeilings(); });
// A window shown after hours in the background (SB-30) reads fresh metadata at once rather
// than at the next minute.
document.addEventListener('visibilitychange', () => { if (!document.hidden) backgroundRefresh(); });
// SB-183 (node A-7): a first-time notice that waited while the window was hidden shows when it is shown.
document.addEventListener('visibilitychange', () => { if (!document.hidden) checkShellHookNotice(); });
