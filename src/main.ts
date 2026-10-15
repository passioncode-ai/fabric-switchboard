import './tokens.css';
import './style.css';
import switchboardMark from '../brand/passioncode/switchboard-mark.svg';
import { version } from '../package.json';
import { platformLabel } from './platform';
import { LOCALE_KEY, dateLocale, locale, parseLocaleChoice, plural, saveLocaleChoice, t, type LocaleChoice } from './i18n';
import { demo, native, nativeAdapter, onProjectSuggestions, safeError, reportFrontendReady, reportLanguage } from './adapter';
import type { CardState } from './ui-logic';
import { APPEARANCE_KEY, EXPIRY_CHOICES, sortAgents, defaultAgentChoices, modelMeta, ordinaryKimiNote, kimiLimitUntil, limitReasonText, limitTitle, agentSetupFacts, sessionHow, sessionStateLabel, kimiWindowsLine, openrouterCreditLine, eventAction, eventDetail, MutationClock, activeRules, autoSwitchPool, launchChoice, withFolder, replaceFolder, storedFolder, LAST_FOLDER_KEY, canProbe, canSwitchNative, accountReset, accountUsedPercent, cardState, compactCountdown, expiryFrom, failedNextCheck, featureWindowLabel, groupAccounts, isFeatureWindow, limitLabel, intervalWhile, loginOutcome, updateLine, monitorChecks, parseAppearance, primaryAction, projectName, quotaMaxAge, quotaOrder, resetCountdown, resolveTheme, ruleState, signInNotice, dismissTermsNotice, termsNoticeDismissed, termsNoticeOn, termsNoticeText, usageFreshness, windowReset, type Appearance } from './ui-logic';
import agentCatalog from '../catalog/agents.json';
import type { Account, Adapter, AgentConnection, AgentInfo, AgentRow, AgentsOverview, OpenrouterModels, HermesModel, KimiAccounts, KimiRegion, KimiStatus, OpenrouterStatus, AgentSetup, AuthKind, BackupStatus, KimiAccount, CurrentAccounts, LoginItem, Project, Restored, ExternalIdentity, MonitorStatus, ProjectRule, Provider, RotationPolicy, RuntimeStatus, SessionInfo, Snapshot, UpdateStatus } from './types';

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
type Page = 'accounts' | 'projects' | 'agents' | 'activity' | 'about';
let page: Page = 'accounts';
/** First-run tour (operator request 2026-10-05): five steps, each on its screen, pointing at what to press. */
const TOUR_KEY = 'switchboard.tour';
const onWindows = () => runtime?.platform === 'windows';
/** SB-104: the tray icon sits in the notification area on Windows and Linux, in the menu bar on macOS. */
const trayInArea = () => runtime?.platform === 'windows' || runtime?.platform === 'linux';
const onLinux = () => runtime?.platform === 'linux';
const tour = (): { page: Page; target: string | null; title: string; text: string }[] => [
  { page: 'accounts', target: null, title: t('Welcome to Switchboard'), text: t('Switchboard keeps your Claude Code and Codex accounts on this computer and moves your work between them, so you can keep working on another account when one reaches its limit.') },
  { page: 'accounts', target: '[data-focus="menu-add"]', title: t('Add your accounts'), text: t('Press + Add account to sign in to another account. To save the account Claude Code already uses, press Add to Switchboard under In use now. Each row then shows how much is used and when it resets.') },
  { page: 'accounts', target: '.rotation-bar', title: t('Switch, by hand or automatically'), text: t('Switch moves Claude Code to another account. Turn on automatic switching, and Switchboard moves off an account that nears its limit to the one with the most left.') },
  { page: 'projects', target: '[data-focus="new-project"]', title: t('Give a project its own accounts'), text: t('Press + New project, add the project\'s folders and choose its accounts. Sessions launched from those folders use only them, and other projects never switch to them.') },
  { page: 'agents', target: '[data-focus="copy-onboard"]', title: t('Agents, and it keeps running'), text: trayInArea() ? t('Here, give your coding agent the setup prompt: it registers the tools and sets up the rest, leaving you only the sign-in. Closing the window keeps Switchboard working; open or quit it from its icon in the notification area.') : t('Here, give your coding agent the setup prompt: it registers the tools and sets up the rest, leaving you only the sign-in. Closing the window keeps Switchboard working; open or quit it from the menu-bar icon.') },
];
// Storage refused: the tour shows again next start rather than never (SCN-035).
let tourStep: number | null = (() => { try { return localStorage.getItem(TOUR_KEY) ? null : 0; } catch { return 0; } })();
function tourGo(step: number | null) {
  tourStep = step;
  if (step === null) { try { localStorage.setItem(TOUR_KEY, '1'); } catch { /* the tour shows again next start */ } render(); restoreFocus('page-title'); return; }
  page = tour()[step].page; if (page === 'agents' && !agentsView) void loadAgents(); render(); restoreFocus('tour-title');
}
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
let termsDismissed = termsNoticeDismissed(noticeStore());
function termsBanner(main: HTMLElement) {
  if (termsNoticeOn(page, termsDismissed) !== 'banner') return;
  const banner = el('section', 'notice terms-notice'); banner.setAttribute('aria-label', t('Provider terms'));
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
let analyticsState: LoginItem | null = null;
let analyticsError = false;
let backupError = false;
let busy = false;
let loading = true;
let loadError = '';
let runtimeError = false;
let notice = '';
let noticeError = false;
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
interface Context { current: CurrentAccounts | null; monitor: MonitorStatus | null }
async function readContext(): Promise<Context> {
  const [current, status] = await Promise.allSettled([adapter.currentAccounts(), adapter.monitorStatus()]);
  return { current: current.status === 'fulfilled' ? current.value : null, monitor: status.status === 'fulfilled' ? status.value : null };
}
function applyContext(context: Context) { currentAccounts = context.current; monitor = context.monitor; }
async function refreshContext() { applyContext(await readContext()); }
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
  const node = el('button', className, text); node.type = 'button'; node.disabled = busy || loading;
  node.addEventListener('click', action); if (key) node.dataset.focus = key; return node;
}
function announce(text: string) { announcements.textContent = ''; requestAnimationFrame(() => { announcements.textContent = text; }); }
/** Notices may carry a backend's English refusal: it is translated here, where it is shown (L10N-04). */
function showNotice(text: string, error = false) { notice = t(text); noticeError = error; announce(notice); }
function restoreFocus(key?: string) { if (key) document.querySelectorAll<HTMLElement>('[data-focus]').forEach((node) => { if (node.dataset.focus === key) node.focus(); }); }

async function reload() {
  if (!native && !demo) { loading = false; render(); return; }
  loading = true; loadError = ''; render();
  const [data, status] = await Promise.allSettled([adapter.snapshot(), adapter.runtime()]);
  if (data.status === 'fulfilled') snapshot = data.value;
  else { loadError = safeError(data.reason); announce(t(loadError)); }
  if (status.status === 'fulfilled') { runtime = status.value; runtimeError = false; }
  else { runtime = null; runtimeError = true; }
  loading = false; render();
  void adapter.agentSetup().then((value) => { agentSetup = value; agentSetupError = false; }, () => { agentSetupError = true; }).finally(backgroundRender);
  void loadOpenrouter();
  void loadKimi();
  void loadHermes();
  void loadSessions();
  void restorePendingLogin();
  void loadBackups();
  if (page === 'agents') void loadAgents();
  void adapter.analytics().then((value) => { analyticsState = value; analyticsError = false; }, () => { analyticsError = true; }).finally(backgroundRender);
  void adapter.loginItem().then((value) => { loginItem = value; loginItemError = false; }, () => { loginItemError = true; }).finally(backgroundRender);
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
async function mutate(action: () => Promise<unknown>, success: string, focusKey?: string, accountId?: string) {
  busy = true; notice = ''; clock.begin(); render();
  try {
    await action();
    await refreshContext();
    if (accountId) usageErrors.delete(accountId);
    showNotice(restoredText || success); restoredText = '';
    try { snapshot = await adapter.snapshot(); loadError = ''; }
    catch { loadError = t('The action completed, but accounts could not be refreshed. Retry loading the account list.'); }
  } catch (error) { if (accountId) { try { snapshot = await adapter.snapshot(); } catch { /* Keep last snapshot. */ } } const text = safeError(error); if (accountId) usageErrors.set(accountId, text); showNotice(text, true); }
  finally { clock.end(); busy = false; render(); restoreFocus(focusKey); }
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
    lastRendered = markup; root.replaceChildren(shell); syncCountdown(); return true;
  };
  const withTour = () => {
    if (tourStep !== null && (native || demo)) {
      const target = tour()[tourStep].target; if (target) shell.querySelector(target)?.classList.add('tour-target');
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
  for (const [target, label, icon] of [['accounts', t('Accounts'), '▦'], ['projects', t('Projects'), '◫'], ['agents', t('Agents'), '⌁'], ['activity', t('Activity'), '≋'], ['about', t('About'), '○']] as const) {
    const control = button('', () => { page = target; notice = ''; if (target === 'activity') void loadSessions(); if (target === 'agents') void loadAgents(); render(); document.querySelector<HTMLElement>('h1')?.focus(); }, `nav-item ${page === target ? 'active' : ''}`, `nav-${target}`);
    const symbol = el('span', 'nav-icon', icon); symbol.setAttribute('aria-hidden', 'true');
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
  const heading = el('div'); const title = el('h1', '', { accounts: t('Accounts'), projects: t('Projects'), agents: t('Agents'), activity: t('Activity'), about: t('About Switchboard') }[page]); title.tabIndex = -1; title.dataset.focus = 'page-title';
  heading.append(el('p', 'eyebrow', t('FABRIC SWITCHBOARD')), title, el('p', 'subtitle', { accounts: t('Switch Claude Code, keep sign-ins fresh and watch quota.'), projects: t('Give a project its own accounts, and keep other projects off them.'), agents: t('Choose which coding agents take over work, and connect them to Switchboard.'), activity: t('Local account and session events.'), about: t('Deliberate account switching for coding sessions.') }[page]));
  header.append(heading);
  if (native || demo) {
    const actions = el('div', 'header-actions');
    actions.append(button(loading ? t('Loading…') : t('Refresh'), () => void reload(), 'button quiet', 'refresh'));
    if (page === 'accounts') actions.append(addMenu());
    if (page === 'projects') actions.append(button(t('+ New project'), () => projectDialog(), 'button primary', 'new-project'));
    header.append(actions);
  }
  main.append(header);
  if (busy) { const progress = el('p', 'operation-progress', t('Working…')); progress.setAttribute('role', 'status'); main.append(progress); }
  if (!native && !demo) {
    main.append(emptyState(t('Open the native app'), t('Account storage and session launches are available in the Fabric Switchboard desktop app. This browser window has no access to your accounts.')));
    shell.append(main); return commit();
  }
  if (notice) { const alert = el('div', `notice ${noticeError ? 'error' : 'success'}`); alert.append(el('span', '', notice), button(t('Dismiss'), () => { notice = ''; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, 'text-button', 'dismiss-notice')); main.append(alert); }
  loginBanner(main);
  if (page === 'about') renderAbout(main);
  else if (page === 'agents') renderAgents(main);
  else if (loadError) {
    const state = emptyState(page === 'activity' ? t('Unable to load activity') : page === 'projects' ? t('Unable to load projects') : t('Unable to load accounts'), t(loadError)); state.classList.add('error-state'); state.append(button(t('Retry'), () => void reload(), 'button primary', 'retry-load')); main.append(state);
  } else if (loading && !snapshot) {
    const state = emptyState(t('Loading your workbench'), t('Reading account metadata from the native app…')); state.setAttribute('role', 'status'); main.append(state);
  } else if (snapshot) {
    if (page === 'accounts') renderAccounts(main);
    else if (page === 'projects') renderProjects(main);
    else renderActivity(main);
  }
  shell.append(main); return commit();
}
const ruleAccount = (rule: ProjectRule) => snapshot?.accounts.find((account) => account.id === rule.account_id);
const ruleTarget = (rule: ProjectRule) => rule.target === 'managed' ? t('Managed sessions') : t('Claude Code login (all claude sessions)');
/** Active rules stay in sight on Accounts, so an optional rule is never forgotten. */
function rulesStrip(main: HTMLElement) {
  const live = activeRules(snapshot?.rules, Date.now() / 1000);
  if (!live.length) return;
  const strip = el('section', 'notice rules-strip'); strip.setAttribute('aria-label', t('Active project rules'));
  const names = live.map((rule) => `${projectName(rule.path)} → ${ruleAccount(rule)?.label ?? t('missing account')}${rule.expires_at ? t(' until {date}', { date: date(rule.expires_at) }) : ''}`);
  strip.append(el('span', '', plural(live.length, { one: '{n} project rule is active: {names}. Rotation still moves off an exhausted account.', other: '{n} project rules are active: {names}. Rotation still moves off an exhausted account.' }, { names: names.join('; ') })), button(t('Review rules'), () => { page = 'projects'; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, 'text-button', 'review-rules'));
  main.append(strip);
}
function renderProjects(main: HTMLElement) {
  main.append(projectsSection(), rulesSection());
}
/** Projects (0.6): folders that reserve their own accounts (core `save_project`). */
function projectsSection() {
  const section = el('section', 'project-section'); section.setAttribute('aria-labelledby', 'projects-heading');
  const heading = el('h2', 'section-heading', t('Projects')); heading.id = 'projects-heading';
  section.append(heading, el('p', 'surface-note', t('A project is one or more folders, such as related repositories, with accounts of its own. Sessions launched from its folders use only its accounts, rotation stays inside them, and no other project or the ordinary Claude Code switches to them.')));
  const projects = snapshot!.projects ?? [];
  if (!projects.length) {
    const empty = emptyState(t('No projects yet'), t('Create one when a client or a team should work only on its own subscriptions. Its accounts move into the project, and its folders stay on them.'));
    empty.append(button(t('New project'), () => projectDialog(), 'button primary', 'empty-new-project'));
    section.append(empty); return section;
  }
  const list = el('div', 'account-list'); list.setAttribute('aria-label', t('Projects'));
  for (const project of projects) {
    const accounts = snapshot!.accounts.filter((a) => a.pool === project.pool);
    const card = el('article', 'account-card project-card'); card.setAttribute('aria-label', `${project.name}, ${plural(accounts.length, { one: '{n} account', other: '{n} accounts' })}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    title.append(el('h2', '', project.name), el('span', 'badge muted', `${plural(accounts.length, { one: '{n} account', other: '{n} accounts' })}`));
    const folders = projectFolderList(project);
    const chips = el('p', 'account-meta', accounts.length ? accounts.map((a) => `${a.label} · ${providerName(a.provider)}`).join(', ') : t('No accounts yet — sessions in these folders use your other accounts until you add one.'));
    details.append(title, folders, chips);
    const actions = el('div', 'management-actions');
    actions.append(button(t('Edit'), () => projectDialog(project), 'text-button', `edit-project-${project.pool}`), button(t('Delete'), () => deleteProjectDialog(project), 'text-button danger-text', `delete-project-${project.pool}`));
    card.append(details, actions); list.append(card);
  }
  section.append(list); return section;
}
/** Folders shown on a project's card before *Show all*: a project an agent works in gathers dozens (SB-131). */
const FOLDERS_SHOWN = 6;
const expandedProjects = new Set<string>();
function projectFolderList(project: Project) {
  const wrap = el('div', 'project-folder-block');
  const folders = el('ul', 'project-folders');
  const open = expandedProjects.has(project.pool);
  const shown = open ? project.folders : project.folders.slice(0, FOLDERS_SHOWN);
  for (const folder of shown) { const item = el('li', 'account-meta', folder); item.title = folder; folders.append(item); }
  wrap.append(folders);
  if (project.folders.length > FOLDERS_SHOWN) {
    const label = open ? t('Show fewer folders') : t('Show all folders ({n})', { n: project.folders.length });
    const toggle = button(label, () => { if (open) expandedProjects.delete(project.pool); else expandedProjects.add(project.pool); render(); restoreFocus(`folders-${project.pool}`); }, 'text-button', `folders-${project.pool}`);
    toggle.setAttribute('aria-expanded', String(open));
    wrap.append(toggle);
  }
  return wrap;
}
function projectDialog(existing?: Project) {
  const context = openDialog(existing ? t('Edit {name}', { name: existing.name }) : t('New project'), t('Add the project\'s folders and choose the accounts that belong to it. An account belongs to one project at a time.'));
  const name = input(existing?.name ?? ''); name.maxLength = 80; name.placeholder = t('e.g. Client Alpha');
  const folders = foldersPicker(context, existing?.folders ?? (lastWorkingDirectory ? [lastWorkingDirectory] : []), 'project-folders');
  const picker = el('fieldset', 'project-accounts'); picker.append(el('legend', 'field-label', t('Accounts')));
  const usable = snapshot!.accounts;
  if (!usable.length) picker.append(el('p', 'form-note', t('No accounts yet. Add accounts first, or save the project now and add them later.')));
  for (const account of usable) {
    const owner = projectOf(account.pool);
    const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.value = account.id; box.checked = !!existing && account.pool === existing.pool;
    const note = owner && owner.pool !== existing?.pool ? t(' · in {name}, moves here', { name: owner.name }) : '';
    option.append(box, el('span', '', `${account.label} · ${providerName(account.provider)}${note}`)); picker.append(option);
  }
  const grid = el('div', 'form-grid'); grid.append(field(t('Project name'), name), groupField(t('Folders'), folders.node, t('The repositories and folders of this project. Subfolders are included. Repositories its agents work in join it by themselves, and folders deleted from disk leave it.')));
  context.body.append(grid, picker, el('p', 'form-note', t('Accounts you leave out move back to the default pool.')));
  context.actions.append(submit(existing ? t('Save project') : t('Create project')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const accountIds = [...picker.querySelectorAll<HTMLInputElement>('input[type=checkbox]:checked')].map((box) => box.value);
    const list = folders.require(); if (!list) return;
    void dialogSave(context, () => adapter.saveProject({ pool: existing?.pool, name: name.value, folders: list, accountIds, baseFolders: existing?.folders }), existing ? t('Project “{name}” saved.', { name: name.value.trim() }) : t('Project “{name}” created. Launch its accounts from its folders.', { name: name.value.trim() }));
  });
}
function rulesSection() {
  const main = el('section', 'project-section'); main.setAttribute('aria-labelledby', 'rules-heading');
  const heading = el('div', 'section-heading-row'); const h = el('h2', 'section-heading', t('Project rules')); h.id = 'rules-heading';
  heading.append(h); if (snapshot!.accounts.length) heading.append(button(t('+ Add rule'), () => ruleDialog(), 'button quiet', 'add-rule'));
  main.append(heading);
  const rules = snapshot!.rules ?? [];
  main.append(el('p', 'surface-note', t('Rules are optional. With none, Switchboard follows your selection and rotation in every project. A rule applies when an agent or switchboard project apply asks for it, only to that session, and never turns rotation off.')));
  if (!rules.length) {
    const empty = emptyState(t('No project rules'), snapshot!.accounts.length ? t('Add a rule when one project should start on a specific account, for example a client project on its own subscription. Prefer an expiry.') : t('Add an account first; a rule names one of your accounts.'));
    if (snapshot!.accounts.length) empty.append(button(t('Add rule'), () => ruleDialog(), 'button primary', 'empty-add-rule'));
    main.append(empty); return main;
  }
  const now = Date.now() / 1000;
  const list = el('div', 'account-list'); list.setAttribute('aria-label', t('Project rules'));
  for (const rule of rules) {
    const state = ruleState(rule, now); const account = ruleAccount(rule); const key = `${rule.provider}-${rule.path}`;
    const card = el('article', `account-card ${state === 'active' ? 'is-selected' : 'is-disabled'}`); card.setAttribute('aria-label', `${projectName(rule.path)}, ${state}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    title.append(el('h2', '', projectName(rule.path)), el('span', `badge ${state === 'active' ? 'current-badge' : 'muted'}`, { active: t('Active'), paused: t('Paused'), expired: t('Expired') }[state]));
    const path = el('p', 'account-meta', rule.path); path.title = rule.path;
    details.append(title, path, el('p', 'account-meta', `${providerName(rule.provider)} · ${account ? `${account.label} · ${account.pool}` : t('account missing')} · ${ruleTarget(rule)}`), el('p', 'usage-caption', rule.expires_at ? (state === 'expired' ? t('Expired {date}', { date: date(rule.expires_at) }) : t('Until {date}', { date: date(rule.expires_at) })) : t('No expiry')));
    const actions = el('div', 'management-actions');
    if (state === 'active') actions.append(button(t('Pause'), () => void mutate(() => adapter.setProjectRule({ path: rule.path, accountId: rule.account_id, target: rule.target, enabled: false, expiresAt: rule.expires_at }), t('Rule paused. Selection and rotation are unchanged.'), `pause-${key}`), 'text-button', `pause-${key}`));
    else actions.append(button(t('Resume…'), () => ruleDialog(rule), 'text-button', `resume-${key}`));
    actions.append(button(t('Edit'), () => ruleDialog(rule), 'text-button', `edit-${key}`), button(t('Remove'), () => removeRuleDialog(rule), 'text-button danger-text', `remove-${key}`));
    card.append(details, actions); list.append(card);
  }
  main.append(list);
  return main;
}
function ruleDialog(existing?: ProjectRule) {
  const context = openDialog(existing ? t('Edit project rule') : t('Add project rule'), t('Sessions in this folder and its subfolders start on the chosen account when the rule is applied. Rotation still runs.'));
  const path = folderPicker(context, existing?.path ?? lastWorkingDirectory, 'rule-folder', !!existing);
  const usable = snapshot!.accounts.filter((account) => account.enabled);
  const account = select(usable.map((item) => [item.id, `${item.label} · ${providerName(item.provider)} · ${item.pool}`]));
  if (existing) account.value = existing.account_id;
  const target = select([['managed', t('Managed sessions (switch from the next request)')], ['claude_cli', t('Claude Code login (changes every claude session)')]]);
  target.value = existing?.target ?? 'managed';
  const expiry = select(EXPIRY_CHOICES); expiry.value = existing && existing.expires_at === null ? '' : '8';
  const sync = () => { const chosen = usable.find((item) => item.id === account.value); const option = target.querySelector<HTMLOptionElement>('option[value="claude_cli"]')!; option.disabled = !(chosen?.provider === 'claude' && chosen.kind === 'oauth' && chosen.external_identity) || !!snapshot!.projects?.some((project) => project.pool === chosen?.pool); if (option.disabled && target.value === 'claude_cli') target.value = 'managed'; };
  account.addEventListener('change', sync); sync();
  const grid = el('div', 'form-grid'); grid.append(groupField(t('Project folder'), path.node, t('Sessions in this folder and its subfolders.')), field(t('Account'), account), field(t('Applies to'), target), field(t('Keep the rule'), expiry, t('Pause or remove it any time on this screen.')));
  context.body.append(grid); context.actions.append(submit(existing ? t('Save rule') : t('Add rule')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const folder = path.require(); if (!folder) return;
    void dialogSave(context, () => adapter.setProjectRule({ path: folder, accountId: account.value, target: target.value as ProjectRule['target'], enabled: true, expiresAt: expiryFrom(expiry.value, Math.floor(Date.now() / 1000)) }), t('Rule saved and active. It applies when an agent or switchboard project apply asks.'));
  });
  if (existing) account.focus(); else path.focus();
}
function copyable(label: string, command: string, key: string) {
  const row = el('div', 'policy-row'); const content = el('div');
  content.append(el('strong', 'block', label), el('code', 'address', command));
  row.append(content, button(t('Copy'), () => { void navigator.clipboard?.writeText(command).then(() => announce(t('{label} command copied.', { label })), () => announce(t('Copy is unavailable here. Select the command text instead.'))); }, 'text-button', key));
  return row;
}
let agentsView: AgentsOverview | null = null;
let agentsError = '';
async function loadAgents() {
  try { agentsView = await adapter.agentsOverview(); agentsError = ''; } catch (error) { agentsError = safeError(error); }
  backgroundRender();
}
const catalogAgent = (id: string) => (agentCatalog as { agents: AgentInfo[] }).agents.find((a) => a.id === id);
/** SB-120: the Agents page — the default agent, what is installed and takes over work,
 * what can be installed, then the OpenRouter key and the tools registration. */
function renderAgents(main: HTMLElement) {
  if (agentsError && !agentsView) { const panel = el('section', 'about-panel'); panel.append(el('p', 'usage-error', t(agentsError)), button(t('Retry'), () => void loadAgents(), 'button', 'agents-retry')); main.append(panel); }
  else if (!agentsView) { const panel = el('section', 'about-panel'); panel.append(el('p', 'form-note', t('Looking for coding agents on this computer…'))); main.append(panel); }
  else main.append(defaultAgentPanel(agentsView), installedAgentsPanel(agentsView), availableAgentsPanel(agentsView));
  main.append(openrouterPanel(), connectPanel());
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
  head.append(heading, el('span', 'badge', plural(installed.length, { one: '{n} detected', other: '{n} detected' })), button(t('Refresh'), () => { agentsView = null; render(); void loadAgents(); }, 'button quiet', 'agents-refresh'));
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
    if (agent.install) controls.append(button(t('Copy'), () => { void navigator.clipboard?.writeText(agent.install!).then(() => announce(t('{label} command copied.', { label: agent.name })), () => announce(t('Copy is unavailable here. Select the command text instead.'))); }, 'text-button', `install-${agent.id}`));
    row.append(info, controls); list.append(row);
  }
  details.append(list); section.append(details, el('p', 'form-note', t('Sources for every agent: docs/research/agents-2026-10-05.md in the repository. CLI: switchboard agents status | enable | disable | default | launch.')));
  return section;
}
/** Registering Switchboard's tools with coding agents (SB-93), shown after the agents. */
function connectPanel() {
  const setup = el('section', 'about-panel'); setup.append(el('h2', '', t('Connect an agent')));
  setup.append(el('p', '', t('Coding agents connect to switchboard mcp. They read who handles their requests and how much quota remains, switch the account of a managed session from the next request, and apply a project rule when they start work in a project. Changing the ordinary Claude Code login affects every claude session, so agents must pass global: true and should ask you first.')));
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
  try { hermes = await adapter.hermesModel(); hermesError = ''; } catch (error) { hermesError = safeError(error); }
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
async function loadOpenrouter() {
  try { openrouter = await adapter.openrouterStatus(); openrouterError = ''; } catch (error) { openrouterError = safeError(error); }
  backgroundRender();
}
const openrouterAgents = () => (agentCatalog as { agents: AgentInfo[] }).agents.filter((agent) => agent.openrouter);
/** SB-79: one OpenRouter key that agents launched from Switchboard run on (SCN-045). */
function openrouterPanel() {
  const section = el('section', 'about-panel'); section.setAttribute('aria-labelledby', 'openrouter-heading');
  const heading = el('h2', '', t('OpenRouter key for agents')); heading.id = 'openrouter-heading';
  const names = openrouterAgents().map((agent) => agent.name).join(', ');
  section.append(heading, el('p', '', t('Save one OpenRouter key, and these agents launch on it with the model you choose: {agents}.', { agents: names }) + ' ' + (onWindows() ? t('The key stays in Windows Credential Manager; an agent receives it only when it starts.') : onLinux() ? t('The key stays in the Secret Service keyring; an agent receives it only when it starts.') : t('The key stays in the Keychain; an agent receives it only when it starts.'))));
  if (openrouterError && !openrouter) { section.append(el('p', 'usage-error', t(openrouterError)), button(t('Retry'), () => void loadOpenrouter(), 'text-button', 'openrouter-retry')); return section; }
  if (!openrouter) { section.append(el('p', 'form-note', t('Reading the OpenRouter key…'))); return section; }
  if (!openrouter.saved) {
    section.append(el('p', 'form-note', t('No key is saved.')), button(t('Add OpenRouter key'), () => openrouterKeyDialog(), 'button primary', 'openrouter-add'));
    return section;
  }
  const facts = el('div', 'openrouter-facts');
  facts.append(el('p', '', openrouter.model ? t('Default model: {model}', { model: openrouter.model }) : t('No default model: each launch names one.')));
  if (openrouter.credit) facts.append(el('p', '', openrouterCreditLine(openrouter.credit)));
  else if (openrouter.credit_error) facts.append(el('p', 'usage-error', t(openrouter.credit_error)));
  section.append(facts);
  const actions = el('div', 'management-actions');
  actions.append(button(t('Change model'), () => openrouterModelDialog(), 'button', 'openrouter-model'), button(t('Replace key'), () => openrouterKeyDialog(), 'button', 'openrouter-replace'), button(t('Remove key'), () => openrouterRemoveDialog(), 'button danger', 'openrouter-remove'));
  section.append(actions, el('p', 'form-note', t('Launch an agent on the key from its Set up button in Installed, above. CLI: switchboard agents launch <agent> --openrouter.')));
  return section;
}
/** Saves (or replaces) the key; it travels to the vault once and is never shown again. */
function openrouterKeyDialog() {
  const replacing = !!openrouter?.saved;
  const context = openDialog(replacing ? t('Replace the OpenRouter key') : t('Add an OpenRouter key'), t('Create a key on openrouter.ai/settings/keys; a spending limit on it caps what agents can spend.'));
  const key = input('', 'password'); key.placeholder = 'sk-or-…';
  const picker = modelPicker(openrouter?.model ?? '', 'key-model'); const model = picker.value;
  const grid = el('div', 'form-grid');
  grid.append(field(t('Key'), key, t('Starts with sk-or-. Switchboard never shows it again.')));
  context.body.append(grid, el('h3', 'pool-heading', t('Default model')), picker.node);
  context.actions.append(submit(replacing ? t('Replace key') : t('Save key')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const value = key.value.trim(); key.value = '';
    void dialogSave(context, async () => { await adapter.openrouterSave(value, model.value.trim() || null); await loadOpenrouter(); }, t('OpenRouter key saved. Agents launched on it receive it when they start.'));
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
function openrouterModelDialog() {
  const context = openDialog(t('Default model'), t('The OpenRouter model agents start on unless a launch names another.'));
  const picker = modelPicker(openrouter?.model ?? '', 'default-model');
  context.body.append(picker.node);
  context.actions.append(submit(t('Save model')));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); const model = picker.value.value.trim(); void dialogSave(context, async () => { await adapter.openrouterModel(model); await loadOpenrouter(); agentsView = await adapter.agentsOverview().catch(() => agentsView); }, t('Agents launch on {model}.', { model })); });
  picker.value.focus();
}
function openrouterRemoveDialog() {
  const context = openDialog(t('Remove the OpenRouter key?'), (onWindows() ? t('Switchboard deletes it from Windows Credential Manager.') : onLinux() ? t('Switchboard deletes it from the Secret Service keyring.') : t('Switchboard deletes it from the Keychain.')) + ' ' + t('Agents already running keep working; new launches need a key again.'));
  const confirm = submit(t('Remove key')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, async () => { await adapter.openrouterRemove(); await loadOpenrouter(); }, t('OpenRouter key removed.')); });
  context.actions.querySelector('button')?.focus();
}
async function agentDialog(agent: AgentInfo) {
  const context = openDialog(t('Set up {agent}', { agent: agent.name }), agent.level === 'mcp' ? t('This agent can use Switchboard\'s tools; its model requests go to its own service.') : t('Register Switchboard\'s tools, then point the agent at Switchboard\'s proxy for one pool.'));
  const pools = [...new Set(snapshot!.accounts.map((a) => a.pool))];
  const pool = select((pools.length ? pools : ['default']).map((p) => [p, projectOf(p) ? t('{pool} (project {name})', { pool: p, name: projectOf(p)!.name }) : p]));
  const out = el('div', 'agent-setup');
  const show = async () => {
    out.replaceChildren(el('p', 'form-note', t('Reading the setup…')));
    let info: AgentConnection;
    try { info = await adapter.agentConnect(agent.id, pool.value); } catch (error) { out.replaceChildren(el('p', 'usage-error', t(safeError(error)))); return; }
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
  const grid = el('div', 'form-grid'); grid.append(field(t('Pool'), pool, t('The pool whose selected API-key account this agent will use.')));
  context.body.append(grid, out);
  const launches = agent.level !== 'mcp' && !!agent.binary;
  const dir = folderPicker(context, lastWorkingDirectory, `agent-folder-${agent.id}`);
  const folder = () => dir.require();
  const viaKey = agent.openrouter ? openrouterLaunch(agent, context, folder) : null;
  // The operator's choice (2026-10-08): agents with a recipe start on the OpenRouter key by
  // default; one whose recipe says `default: false` (Kimi Code, on its subscription) keeps it second.
  const keyFirst = !!viaKey?.run && agent.openrouter?.default !== false;
  if (launches || viaKey) context.body.append(groupField(t('Launch in folder'), dir.node, launches && !keyFirst ? t('Starts the agent in Terminal in this folder, on the pool\'s API-key account.') : t('Starts the agent in Terminal in this folder.')));
  if (viaKey) context.body.append(viaKey.part);
  const throughProxy = () => { const value = folder(); if (value) void dialogSave(context, () => adapter.launchAgent(agent.id, pool.value, value), t('Terminal launch requested for {agent}.', { agent: agent.name })); };
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
  if (!openrouter?.saved) { part.append(el('p', 'form-note', t('Save an OpenRouter key on the Agents screen first; then {agent} launches on it.', { agent: agent.name }))); return { part, run: null }; }
  const picksItself = !agent.openrouter?.model_flag?.length && !agent.openrouter?.model_env;
  const picker = modelPicker(openrouter.model ?? '', `launch-${agent.id}`); const model = picker.value;
  if (picksItself) part.append(el('p', 'form-note', t('{agent} picks its model in its own interface; the key comes from Switchboard.', { agent: agent.name })));
  else part.append(el('p', 'form-note', openrouter.model ? t('Model for this launch. Default: {model}.', { model: openrouter.model }) : t('Model for this launch. No default model is saved: name one.')), picker.node);
  if (agent.openrouter?.notes) part.append(el('p', 'form-note', agent.openrouter.notes));
  const run = () => {
    const value = folder(); if (!value) return;
    void dialogSave(context, () => adapter.launchOnOpenrouter(agent.id, picksItself ? null : model.value.trim() || null, value), t('Terminal launch requested for {agent} on OpenRouter.', { agent: agent.name }));
  };
  return { part, run };
}
function emptyState(title: string, description: string) { const section = el('section', 'empty-state'); section.append(el('div', 'empty-symbol', '◇'), el('h2', '', title), el('p', '', description)); return section; }
// ── Accounts (0.5): one click to add, compact grouped rows, row menus instead of dialogs ──
const signInRequired = (account: Account) => !!monitor?.sign_in_required?.includes(account.id);
const accountLimit = (account: Account) => monitor?.limited?.find((entry) => entry.account_id === account.id && entry.until > Date.now() / 1000);
// #region quota-countdown — docs: docs/runs/2026-10-04-quota-review/README.md
let countdownPaused = false;
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
    for (const [text, action, tone] of items) {
      const item = button(text, () => { openMenu = null; render(); action(); }, `menu-item ${tone ?? ''}`); item.setAttribute('role', 'menuitem'); item.tabIndex = -1; list.append(item);
    }
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
function addMenu() {
  return menu('add', t('Add account'), [
    [t('Sign in to another Claude account'), () => void startLogin('claude')],
    [t('Sign in to another Codex account'), () => void startLogin('codex')],
    [t('Import from Claude Swap'), () => void importSwap()],
    [t('Add an API key or token…'), () => addDialog()],
  ], 'button primary', t('+ Add account'));
}
/** Official sign-in without a form: Terminal opens, completion is detected, the email names the account. */
async function startLogin(provider: Provider, label = '', pool = 'default') {
  if (pendingLogin) { showNotice(t('A sign-in is already in progress. Finish it in Terminal or cancel it first.'), true); render(); return; }
  busy = true; notice = ''; render();
  try {
    const result = await adapter.beginLogin({ provider, label, pool });
    pendingLogin = { id: result.login_id, provider, label, pool, state: 'pending', error: '' };
    announce(t('Sign in to {provider} in the Terminal window that opened. Switchboard adds the account when you finish.', { provider: providerName(provider) }));
  } catch (error) { showNotice(safeError(error), true); }
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
    await loginSaved(signInNotice(account, providerName(account.provider)));
  } catch (error) {
    const text = safeError(error); const outcome = loginOutcome(text);
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
    const parts = [plural(result.imported.length, { one: '{n} Claude Swap profile imported or updated in the default pool', other: '{n} Claude Swap profiles imported or updated in the default pool' })];
    if (result.skipped) parts.push(t('{n} skipped', { n: result.skipped }));
    if (result.failed) parts.push(t('{n} could not be read — check them in Claude Swap and import again', { n: result.failed }));
    if (result.claude_swap_running) parts.push(t('Claude Swap is still running, so it keeps renewing those accounts and Switchboard follows its newest sign-ins'));
    showNotice(`${parts.join(' · ')}.`, result.failed > 0);
  } catch (error) { showNotice(safeError(error), true); }
  finally { clock.end(); busy = false; render(); restoreFocus('menu-add'); }
}
function switchNative(account: Account) {
  void mutate(() => adapter.activateNative(account.id), demo ? t('Synthetic switch: Claude Code now uses {label}. No local credentials were read or written.', { label: account.label }) : t('Claude Code now uses {label}. New claude sessions start on it; a running session may need a restart.', { label: account.label }), `primary-${account.id}`);
}
function renderAccounts(main: HTMLElement) {
  termsBanner(main);
  rulesStrip(main);
  renderCurrent(main);
  renderPolicies(main);
  const accounts = snapshot!.accounts;
  if (!accounts.length) {
    // A reinstall with a backup this computer can open: one press brings everything back.
    const backup = backupStatus?.backups.filter((b) => b.openable && b.accounts > 0).sort((a, b) => b.created_at - a.created_at)[0];
    const empty = emptyState(t('Start with one account'), backup ? plural(backup.accounts, { one: 'A backup from {date} with {n} account is on this computer. Restore it to get your accounts, projects and settings back, or add an account.', other: 'A backup from {date} with {n} accounts is on this computer. Restore it to get your accounts, projects and settings back, or add an account.' }, { date: date(backup.created_at) }) : t('Add the account Claude Code or Codex CLI already uses with one click above, or sign in to another account. Nothing opens until you choose.'));
    if (backup) empty.append(button(t('Restore from backup'), () => void mutate(async () => { const result = await adapter.restoreBackup(backup.file); backupStatus = await adapter.backups(); showRestored(result); }, t('Backup restored.'), 'empty-restore'), 'button primary', 'empty-restore'));
    empty.append(addMenu()); main.append(empty, kimiSection()); return;
  }
  const sorting = el('div', 'account-sort-note');
  sorting.append(el('span', '', t('Within each pool: remaining quota first, then shortest wait. Unknown usage follows.')), button(countdownPaused ? t('Resume countdown') : t('Pause countdown'), () => {
    if (!countdownPaused) { pausedCountdowns.clear(); root.querySelectorAll<HTMLElement>('[data-countdown]').forEach(node => pausedCountdowns.set(countdownKey(node), node.textContent ?? '')); }
    countdownPaused = !countdownPaused; render(); restoreFocus('countdown-toggle');
  }, 'button quiet', 'countdown-toggle'));
  main.append(sorting);
  for (const group of groupAccounts(accounts, quotaContext())) {
    const section = el('section', 'account-group'); section.setAttribute('aria-label', t('{provider} accounts', { provider: providerName(group.provider) }));
    const heading = el('div', 'group-heading');
    heading.append(el('h2', '', providerName(group.provider)), el('span', 'count', `${plural(group.count, { one: '{n} account', other: '{n} accounts' })}`));
    section.append(heading);
    for (const pool of group.pools) {
      const owner = projectOf(pool.pool);
      if (group.pools.length > 1 || owner) section.append(el('h3', 'pool-heading', owner ? t('Project · {name}', { name: owner.name }) : t('Pool · {pool}', { pool: pool.pool })));
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
async function loadKimi() {
  kimiLoadedAt = Date.now();
  try { kimi = await adapter.kimiAccounts(); kimiError = ''; } catch (error) { kimiError = safeError(error); }
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
/** A Kimi Code account in the same row as Claude and Codex accounts: a small primary action and a menu. */
function kimiRow(account: KimiAccount, status: KimiStatus) {
  const signedOut = !status.signed_in;
  const row = el('article', `account-row kimi-account ${signedOut ? 'is-disabled' : ''}`); row.setAttribute('role', 'listitem');
  row.setAttribute('aria-label', t('{label}, Kimi Code', { label: account.label }));
  const limitedUntil = signedOut ? null : kimiLimitUntil(status.windows, Math.floor(Date.now() / 1000));
  row.dataset.state = signedOut ? 'sign_in' : limitedUntil !== null ? 'blocked' : status.error ? 'stale' : 'available';
  const icon = el('span', 'provider-icon kimi', 'K'); icon.setAttribute('aria-hidden', 'true');
  const name = el('div', 'row-name'); const title = el('div', 'row-title'); title.append(el('strong', '', account.label));
  if (signedOut) title.append(el('span', 'badge danger-badge', t('Sign in again')));
  else if (limitedUntil !== null) title.append(el('span', 'badge danger-badge', limitedUntil > 0 ? t('Limit reached · resets in {time}', { time: compactCountdown(limitedUntil, Math.floor(Date.now() / 1000)) }) : t('Limit reached')));
  name.append(title, el('span', 'row-sub', [account.tier, account.region === 'global' ? 'kimi.ai' : 'kimi.com'].filter(Boolean).join(' · ')));
  const usage = el('div', 'usage-cell kimi-usage'); usage.append(kimiStatusLine(status));
  const relogin = () => void mutate(() => adapter.kimiSignInAgain(account.id), t('Sign in to {label} in Terminal, then refresh.', { label: account.label }), `kimi-primary-${account.id}`);
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
    void dialogSave(context, () => adapter.kimiLaunch(id, value), t('Terminal launch requested for {agent}.', { agent: accountLabel }));
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
  row.setAttribute('aria-label', t('{label}, {provider}, {pool} pool', { label: account.label, provider: providerName(account.provider), pool: account.pool }) + (current ? t(', in use by the CLI') : ''));
  const state = cardState(account, quotaContext()); row.dataset.state = state;
  const icon = el('span', `provider-icon ${account.provider}`, account.provider === 'claude' ? '✳' : '◎'); icon.setAttribute('aria-hidden', 'true'); icon.title = CARD_STATE[state];
  const name = el('div', 'row-name');
  const title = el('div', 'row-title'); title.append(el('strong', '', account.label));
  if (current) title.append(el('span', 'badge current-badge', account.provider === 'claude' ? t('In Claude Code') : t('In Codex CLI')));
  if (active) title.append(el('span', 'badge selected-badge', t('Next managed request')));
  if (signIn) title.append(el('span', 'badge danger-badge', t('Sign in again')));
  if (!account.enabled) title.append(el('span', 'badge muted', t('Disabled')));
  const email = account.external_identity?.email;
  name.append(title, el('span', 'row-sub', [email && email !== account.label ? email : '', account.kind === 'oauth' ? '' : kindName(account.kind)].filter(Boolean).join(' · ') || (account.kind === 'oauth' ? t('OAuth') : '')));
  const project = !!projectOf(account.pool);
  row.append(icon, name, usageCell(account), primaryButton(account, { current, selected: active, signIn, project }), rowMenu(account, { current, selected: active, signIn, project }));
  if (quotaOpen.has(account.id) && account.usage) row.append(quotaDetails(account));
  if (usageErrors.has(account.id)) row.append(el('p', 'row-error', t(usageErrors.get(account.id)!)));
  return row;
}
function primaryButton(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const slot = el('div', 'row-primary'); const key = `primary-${account.id}`;
  const action = primaryAction(account, state);
  const status = (text: string) => { const label = el('span', 'row-state', text); label.tabIndex = -1; label.dataset.focus = key; return label; };
  if (action === 'in_use') slot.append(status(t('✓ In use')));
  else if (action === 'selected') slot.append(status(t('✓ Selected')));
  else if (action === 'switch') { const control = button(t('Switch'), () => switchNative(account), 'button row-button', key); control.title = t('Make this the Claude Code account on this computer'); slot.append(control); }
  else if (action === 'select') slot.append(button(t('Select'), () => void mutate(() => adapter.select(account), t('{label} selected for the next managed request in {pool}.', { label: account.label, pool: account.pool }), key), 'button row-button', key));
  else if (action === 'sign_in') slot.append(button(t('Sign in'), () => void startLogin(account.provider, account.label, account.pool), 'button row-button', key));
  else slot.append(button(t('Enable'), () => void mutate(() => adapter.update(account.id, account.label, true), t('{label} enabled.', { label: account.label }), key), 'button row-button quiet', key));
  return slot;
}
function rowMenu(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const items: ([string, () => void] | [string, () => void, string])[] = [];
  if (account.enabled && canSwitchNative(account) && !state.project && !state.selected) items.push([t('Select for managed sessions'), () => void mutate(() => adapter.select(account), t('{label} selected for the next managed request in {pool}.', { label: account.label, pool: account.pool }), `menu-${account.id}`)]);
  if (account.enabled && canProbe(account)) items.push([t('Check usage'), () => void mutate(() => adapter.probe(account.id), t('Usage observation updated.'), `menu-${account.id}`, account.id)]);
  if (account.enabled) items.push([t('Launch…'), () => launchDialog(account)]);
  if (account.kind === 'oauth' && !state.signIn) items.push([t('Sign in again'), () => void startLogin(account.provider, account.label, account.pool)]);
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
function addDialog() {
  const context = openDialog(t('Add an API key or token'), t('For accounts without an official sign-in. To add a Claude Code or Codex login, use + Add account → Sign in, or add the account the CLI already uses.'));
  const provider = select([['claude', t('Claude Code')], ['codex', t('Codex CLI')]]);
  const method = select([['api_key', t('API key')], ['setup_token', t('Claude setup token')], ['oauth', t('OAuth JSON')]]);
  const label = input(); label.maxLength = 80; label.placeholder = t('e.g. Studio');
  const pool = input('default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+'; pool.title = t('Use lowercase letters, numbers, hyphens, or underscores.');
  const grid = el('div', 'form-grid'); grid.append(field(t('Provider'), provider), field(t('Credential type'), method), field(t('Account label'), label), field(t('Pool'), pool, t('A boundary for routing, such as work or personal.')));
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
    void dialogSave(context, () => adapter.add({ provider: provider.value as Provider, label: label.value.trim(), pool: pool.value.trim(), kind: method.value as AuthKind, secret: value }), t('Account added. Select it when you are ready.'));
  });
  provider.focus();
}
function renderCurrent(main: HTMLElement) {
  const section = el('section', 'current-section'); section.setAttribute('aria-label', t('Accounts the CLIs use now'));
  const heading = el('div', 'section-heading');
  const proxy = el('span', 'proxy-status'); const dot = el('span', `status-dot ${runtimeError ? 'unavailable' : ''}`); dot.setAttribute('aria-hidden', 'true');
  proxy.append(dot, el('span', '', demo ? t('Demo · no live requests') : runtimeError ? t('Local proxy status unavailable') : t('Local proxy {address}', { address: runtime?.proxy_address ?? t('starting…') })));
  heading.append(el('h2', '', t('In use now')), proxy);
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
      else card.append(button(t('Add to Switchboard'), () => void mutate(() => adapter.captureCurrent({ provider, pool: 'default' }), t('{identity} added to {provider} · default. Switchboard keeps its sign-in up to date.', { identity: identityText(current.identity), provider: providerName(provider) }), `capture-${provider}`), 'button primary row-button', `capture-${provider}`));
    } else {
      copy.append(el('strong', 'current-identity muted-text', current?.status === 'missing' ? t('Not signed in') : current ? t('Could not read the sign-in') : t('Reading…')));
      card.append(copy);
      if (current?.status === 'unavailable') card.append(button(t('Retry'), () => void reload(), 'button quiet row-button', `retry-current-${provider}`));
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
    copy.append(el('span', 'usage-caption', `${providerName(policy.provider)} · ${policy.pool} · ${policyTarget(policy.target)} · ${t('at {percent}% used', { percent: policy.threshold_percent })}${decision ? ` · ${decisionText(decision.reason)}` : ''}`));
  } else copy.append(el('span', 'usage-caption', monitor ? t('When the account in use nears its limit, Switchboard can move to the saved account with the most quota left.') : t('Quota monitor status unavailable. Refresh to retry.')));
  const swapHeld = monitor?.claude_swap_accounts ?? 0;
  if (swapHeld) copy.append(el('span', 'usage-caption', t('Claude Swap is running and renews {n} of these accounts. Switchboard takes their newest sign-ins from it and does not renew them itself.', { n: swapHeld })));
  if (monitor?.renewal_blocked) copy.append(el('span', 'usage-caption', t('Claude is refusing sign-in renewals for every account right now. Saved accounts keep their last sign-in and are not renewed; Switchboard tries again within the hour. If this stays, update Switchboard.')));
  if (demo) copy.append(el('span', 'usage-caption', t('Demo policies are editable fixtures; no switching runs here.')));
  const actions = el('div', 'rotation-actions');
  const pool = autoSwitchPool(snapshot!.accounts, policies, (snapshot!.projects ?? []).map((p) => p.pool));
  if (pool) actions.append(button(t('Turn on for Claude Code'), () => { const saved = policies.find((policy) => policy.provider === 'claude' && policy.pool === pool && policy.target === 'claude_cli'); void mutate(() => adapter.setPolicy({ provider: 'claude', pool, target: 'claude_cli', enabled: true, threshold_percent: saved?.threshold_percent ?? 90, hysteresis_percent: saved?.hysteresis_percent ?? 10, cooldown_seconds: saved?.cooldown_seconds ?? 1800, max_age_seconds: saved?.max_age_seconds ?? 300, last_switched_at: saved?.last_switched_at ?? null }), t('Automatic switching is on for Claude Code in {pool}. It moves at {threshold}% used to an account with at least {headroom} points more headroom.', { pool, threshold: saved?.threshold_percent ?? 90, headroom: saved?.hysteresis_percent ?? 10 }), 'menu-policies'); }, 'button primary row-button', 'auto-on'));
  for (const policy of enabled) actions.append(button(enabled.length > 1 ? t('Stop {pool}', { pool: policy.pool }) : t('Stop'), () => void mutate(() => adapter.setPolicy({ ...policy, enabled: false }), t('Automatic switching stopped for this provider, pool and target.'), 'menu-policies'), 'button quiet row-button', `stop-${policy.provider}-${policy.pool}-${policy.target}`));
  actions.append(menu('policies', t('Automatic switching settings'), [...policies.map((policy): [string, () => void] => [t('Edit {provider} · {pool} · {target}…', { provider: providerName(policy.provider), pool: policy.pool, target: policyTarget(policy.target) }), () => policyDialog(policy)]), [t('New policy…'), () => policyDialog()]], 'icon-button', '⚙'));
  panel.append(copy, actions); main.append(panel);
}
let sessionList: SessionInfo[] | null = null;
let sessionsError = false;
async function loadSessions() {
  try { sessionList = (await adapter.sessions()).sessions; sessionsError = false; } catch { sessionsError = true; }
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
    title.append(el('strong', '', `${session.provider === 'kimi' ? t('Kimi Code') : session.provider === 'hermes' ? t('Hermes Agent') : session.provider === 'opencode' ? 'OpenCode' : providerName(session.provider)} · ${session.label ?? session.account_id}`), el('span', `session-state is-${session.state}`, sessionStateLabel(session.state)));
    const facts = el('p', 'form-note', session.started_at ? t('{how} · started {date}', { how: sessionHow(session), date: date(session.started_at) }) : sessionHow(session));
    row.append(title, el('code', 'address', session.working_directory), facts); list.append(row);
  }
  panel.append(list);
  return panel;
}
function renderActivity(main: HTMLElement) {
  main.append(sessionsPanel());
  if (!snapshot!.events.length) { main.append(emptyState(t('No activity yet'), t('Adding accounts, changing routes, and launching sessions will appear here. Credentials and prompts are never part of this journal.'))); return; }
  const list = el('ol', 'event-list');
  for (const event of [...snapshot!.events].reverse()) {
    const row = el('li', 'event-row'); const title = el('div', 'event-title');
    const account = snapshot!.accounts.find((item) => item.id === event.account_id);
    title.append(el('strong', '', eventAction(event.action)), el('time', '', date(event.at)));
    row.append(title, el('p', '', eventDetail(event.detail)));
    if (event.account_id) row.append(el('span', 'event-id', `${account ? `${account.label} · ` : ''}${event.account_id}`)); list.append(row);
  }
  main.append(list, el('p', 'surface-note', t('A bounded local journal. A terminal launch is not proof of a successful provider request.')));
}
function renderAbout(main: HTMLElement) {
  const section = el('section', 'about-panel');
  section.append(el('h2', '', t('A local account workbench')), el('p', '', t('Switchboard stores credentials in the native vault and manages private homes for Claude Code and Codex CLI. Account labels and imported identities are user claims until a provider confirms them.')));
  const definitions = el('dl', 'definitions');
  for (const [term, description] of [
    [t('Isolated launch'), t('A new official CLI session using this account’s private home. Account changes affect new launches.')],
    [t('Managed launch'), t('A session routed through the local HTTP/SSE proxy. Select an account for each provider and pool; the next request uses that selection. A response already in progress keeps its identity.')],
    [t('Pools'), t('Explicit routing boundaries, such as work and personal. Selection never silently falls back to a different pool.')],
    [t('Usage'), t('A timestamped provider observation. Unknown, stale, and unavailable are distinct from measured zero. API billing is separate from subscription quota.')],
    [t('Build'), demo ? t('Synthetic browser demo; it runs without the native app.') : t('{platform} build. Native platform is reported by the running app.', { platform: platformLabel(runtime?.platform) })],
    [t('Compatibility'), t('macOS, Windows and Linux builds. Live provider acceptance is a separate check on each platform. Existing external CLI sessions and Codex Desktop are not controlled.')],
    [t('Imported OAuth'), t('A captured credential snapshot. After you sign in again elsewhere, add the account the CLI uses again. Switchboard renews inactive Claude accounts; the account Claude Code uses is renewed by Claude Code.')],
    [t('Switch'), t('Switch changes the account of the ordinary Claude Code. New claude sessions start on it; a session already running may keep its account until it restarts.')],
    [t('Automatic switching'), t('Off by default for each provider, pool and target. Uses fresh quota observations, a threshold, a minimum improvement and a cooldown. No eligible account means the current account stays selected.')],
  ]) { definitions.append(el('dt', '', term), el('dd', '', description)); }
  section.append(definitions); const tour = el('section', 'about-panel'); tour.append(el('h2', '', t('Tour')), el('p', '', t('Five short steps: what Switchboard does and where to press.')), button(t('Show the tour again'), () => tourGo(0), 'button', 'tour-again'));
  main.append(section, termsPanel(), tour, residencyPanel(), backupsPanel(), analyticsPanel(), appearancePanel(), languagePanel(), productPanel());
}
async function loadBackups() {
  try { backupStatus = await adapter.backups(); backupError = false; } catch { backupError = true; }
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
  if (updateState) showNotice(updateLine(updateState).text, updateState.state === 'failed');
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
  const line = updateLine(status);
  panel.append(option, el('p', 'form-note', line.text));
  if (line.restart) panel.append(button(t('Restart to update'), () => void mutate(async () => { updateState = await adapter.restartToUpdate(); }, t('Restarting Switchboard with the new version…'), 'update-restart'), 'button', 'update-restart'));
  // LC-16: the person can always ask, with automatic updates off too.
  else if (status.available && status.state !== 'checking' && status.state !== 'downloading') panel.append(button(t('Check for updates'), () => void checkUpdatesNow(), 'button', 'update-check'));
}
/** Anonymous usage analytics (docs/ANALYTICS.md): what is sent, and the switch every PassionCode.ai tool shares. */
function analyticsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'analytics-heading');
  const heading = el('h2', '', t('Usage analytics')); heading.id = 'analytics-heading';
  panel.append(heading, el('p', '', t('Switchboard counts installs, days of use and how many accounts are saved, by provider and type, to help PassionCode.ai improve its apps. It never sends account names, e-mail addresses, sign-ins, pool names or what you do with your accounts. A random installation number, shared by the PassionCode.ai tools on this computer, lets one person using several of them count once.')));
  if (analyticsError) { panel.append(el('p', 'form-note', t('The analytics setting is unavailable. Refresh to retry.'))); return panel; }
  if (!analyticsState) { panel.append(el('p', 'form-note', t('Reading the analytics setting…'))); return panel; }
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = analyticsState.enabled; box.disabled = !analyticsState.available; box.dataset.focus = 'analytics';
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { analyticsState = await adapter.setAnalytics(wanted); }, wanted ? t('Anonymous usage analytics are on.') : t('Anonymous usage analytics are off for every PassionCode.ai tool on this computer.'), 'analytics'); });
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
    row.append(copy, button(t('Restore'), () => void mutate(async () => { const result = await adapter.restoreBackup(backup.file); backupStatus = await adapter.backups(); showRestored(result); }, t('Backup restored.'), `restore-${backup.file}`), 'text-button', `restore-${backup.file}`));
    list.append(row);
  }
  panel.append(list, button(t('Back up now'), () => void mutate(async () => { await adapter.backupNow(); backupStatus = await adapter.backups(); }, t('Backup saved.'), 'backup-now'), 'button', 'backup-now'));
  return panel;
}
let restoredText = '';
function showRestored(result: Restored) {
  const extra = [result.projects ? `${plural(result.projects, { one: '{n} project', other: '{n} projects' })}` : '', result.rules ? `${plural(result.rules, { one: '{n} rule', other: '{n} rules' })}` : '', result.settings ? t('settings') : ''].filter(Boolean);
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
function foldersPicker(context: DialogContext, initial: string[], key: string) {
  let folders = [...initial];
  const node = el('div', 'folder-picker-list'); const list = el('ul', 'folder-list');
  const add = button(t('Add folder…'), () => void chooseFolder(folders.at(-1) ?? '', (text) => { context.error.textContent = text; }).then((folder) => { if (folder) { folders = withFolder(folders, folder); context.error.textContent = ''; paint(); } }), 'button row-button', `${key}-add`);
  const paint = () => {
    list.replaceChildren();
    if (!folders.length) list.append(el('li', 'folder-path is-empty', t('No folder chosen')));
    folders.forEach((folder) => {
      const item = el('li', 'folder-item'); const path = el('span', 'folder-path', folder); path.title = folder;
      const change = button(t('Change…'), () => void chooseFolder(folder, (text) => { context.error.textContent = text; }).then((chosen) => { if (chosen && chosen !== folder) { folders = replaceFolder(folders, folder, chosen); context.error.textContent = ''; paint(); } }), 'text-button', '');
      change.setAttribute('aria-label', t('Change {folder}', { folder }));
      const remove = button('×', () => { folders = folders.filter((entry) => entry !== folder); paint(); add.focus(); }, 'icon-button', '');
      remove.setAttribute('aria-label', t('Remove {folder}', { folder })); item.append(path, change, remove); list.append(item);
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
async function dialogSave(context: DialogContext, action: () => Promise<unknown>, success: string) {
  context.error.textContent = ''; context.setBusy(true); clock.begin();
  try { await action(); await refreshContext(); showNotice(success); try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = t('The action completed, but the list could not be refreshed. Retry loading accounts.'); } render(); context.setBusy(false); context.close(); }
  catch (error) { context.error.textContent = t(safeError(error)); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); }
  finally { clock.end(); }
}
/**
 * Launch any account in a folder chosen with the picker (operator decision 2026-10-09). The box
 * runs it isolated, in the account's own home; cleared, it goes through the local proxy as a managed
 * session — open only to the pool's selected account while the proxy runs (`launchChoice`).
 */
function launchDialog(account: Account) {
  const choice = launchChoice(selected(account), !runtimeError);
  const context = openDialog(t('Launch {label}', { label: account.label }), t('{provider} · {pool} pool', { provider: providerName(account.provider), pool: account.pool }));
  const folder = folderPicker(context, lastWorkingDirectory, `launch-folder-${account.id}`);
  const isolated = input('', 'checkbox'); isolated.required = false; isolated.checked = choice.isolatedByDefault;
  if (!choice.managed) { isolated.disabled = true; isolated.dataset.locked = 'true'; }
  const box = el('label', 'checkbox-field'); box.append(isolated, el('span', '', t('Isolated session')));
  const note = el('p', 'form-note');
  const explain = () => { note.textContent = demo ? t('Synthetic launch only. The demo does not open a terminal.') : isolated.checked ? t('Runs in this account’s own home and keeps this account; automatic switching does not move it. Existing clients are not changed.') : t('Goes through the local proxy: new requests use the selected account of this pool, and automatic switching can move it. In-progress responses keep their account.'); };
  isolated.addEventListener('change', explain); explain();
  context.body.append(groupField(t('Folder'), folder.node), box);
  if (!choice.managed) context.body.append(el('p', 'form-note', runtimeError ? t('The local proxy is not running, so this session is isolated.') : t('Only the selected account of {pool} launches through the proxy, so this session is isolated.', { pool: account.pool })));
  context.body.append(note);
  context.actions.append(submit(t('Launch')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const workingDirectory = folder.require(); if (!workingDirectory) return;
    const mode = isolated.checked ? 'isolated' : 'managed';
    let tools = true;
    const noTools = t(' Agents in this session have no Switchboard tools: link the command-line tool under Agents, then launch again.');
    void dialogSave(context, async () => { const result = await adapter.launch(account.id, mode, workingDirectory); tools = result.agent_tools !== false; }, demo ? mode === 'managed' ? t('Synthetic managed launch recorded. No terminal was opened.') : t('Synthetic isolated launch recorded. No terminal was opened.') : mode === 'managed' ? t('Terminal launch requested in your project through the local proxy. Selection takes effect on the next request.') : t('Terminal launch requested in your project with this account’s private home. Provider acceptance is not yet observed.')).then(() => { if (!tools && !noticeError) { showNotice(notice + noTools); render(); } });
  });
  folder.focus();
}
function editDialog(account: Account) {
  const context = openDialog(t('Edit account'), t('{provider} · {pool} pool. To change credentials or pool, add a new account.', { provider: providerName(account.provider), pool: account.pool }));
  const label = input(account.label); label.maxLength = 80; const enabled = input('', 'checkbox'); enabled.required = false; enabled.checked = account.enabled;
  const enabledLabel = el('label', 'checkbox-field'); enabledLabel.append(enabled, el('span', '', t('Enabled for selection and launch')));
  context.body.append(field(t('Account label'), label), enabledLabel, el('p', 'form-note', t('Disabling a selected account clears its route. Managed requests in that pool will fail until you select another account.')));
  context.actions.append(submit(t('Save changes')));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); if (!label.value.trim()) { label.setCustomValidity(t('Enter an account label.')); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; } void dialogSave(context, () => adapter.update(account.id, label.value.trim(), enabled.checked), enabled.checked ? t('Account updated.') : t('Account disabled. Any selection for this account has been cleared.')); }); label.focus();
}
function removeDialog(account: Account) {
  const active = selected(account);
  const context = openDialog(t('Remove account?'), t('Remove “{label}” from Switchboard and delete its stored credential. This cannot be undone.', { label: account.label }));
  context.body.append(el('p', 'form-note', active ? t('This account is selected. Select another account in the same pool, or disable this one in Edit, before removing it.') : t('Existing external clients are not signed out. Managed account metadata and its vault entry will be removed.')));
  const confirm = submit(t('Remove account')); confirm.className = 'button danger'; confirm.disabled = active; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); if (!active) void dialogSave(context, () => adapter.remove(account.id), t('Account removed.')); });
  context.actions.querySelector('button')?.focus();
}

/** SB-69: deleting a project asks first, like removing an account (SCN-042). */
function deleteProjectDialog(project: Project) {
  const context = openDialog(t('Delete project?'), t('Delete “{name}”. Its accounts stay in the pool {pool}, no longer reserved; sessions in its folders use the usual selection again.', { name: project.name, pool: project.pool }));
  const confirm = submit(t('Delete project')); confirm.className = 'button danger'; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, () => adapter.removeProject(project.pool), t('Project “{name}” deleted. Its accounts stay in the pool {pool}, no longer reserved.', { name: project.name, pool: project.pool })); });
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
    disabled: t('Rotation is off.'), cooldown: t('Waiting for the cooldown to end.'), below_threshold: t('Current usage is below the threshold.'),
    no_eligible_account: t('No eligible account. Holding the current account.'),
    stale_usage: t('Waiting for fresh usage. Holding the current account.'), usage_unavailable: t('Usage unavailable. Holding the current account.'),
    current_unavailable: t('Current account unavailable. Holding the current account.'), limit_reached: t('The account in use hit a provider limit. An unlimited account is available; a switch is not yet confirmed.'), limit_no_eligible_account: t('The account in use hit a provider limit, and no other account is free. Holding it.'), switched_on_limit: t('Switched after the account in use hit a provider limit.'), threshold_reached: t('Threshold reached. An eligible account is available; a switch is not yet confirmed.'), switched: t('The monitor recorded a switch.'), switch_failed: t('The switch failed. The current account stays selected; check Activity.'), activation_failed: t('Native activation failed. Check the current CLI identity and retry manually.'), claude_swap_switching: t('Claude Swap is switching Claude Code automatically, so Switchboard does not. Turn off automatic switching in one of them; manual switches still work.'),
  };
  return labels[reason] || t('The monitor has evaluated this policy. Check Activity for recorded changes.');
}
function policyDialog(existing?: RotationPolicy) {
  const context = openDialog(t('Automatic switching'), t('Configure one provider, pool and target. Fresh eligible accounts stay within this boundary; no eligible account means hold.'));
  const provider = select([['claude', t('Claude Code')], ['codex', t('Codex CLI')]]);
  const pool = input(existing?.pool || 'default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+';
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
    populate(snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value.trim() && policy.target === target.value));
  };
  provider.value = existing?.provider ?? 'claude'; target.value = existing?.target ?? 'managed'; sync(); populate(existing);
  if (existing) for (const control of [provider, pool, target]) { control.disabled = true; control.dataset.locked = 'true'; }
  else { provider.addEventListener('change', sync); target.addEventListener('change', sync); pool.addEventListener('change', sync); }
  const grid = el('div', 'form-grid'); grid.append(field(t('Provider'), provider), field(t('Pool'), pool), field(t('Target'), target), field(t('Switch at usage (%)'), threshold), field(t('Headroom below threshold (points)'), hysteresis, t('The candidate must stay this many points below the switching threshold.')), field(t('Cooldown (seconds)'), cooldown), field(t('Maximum usage age (seconds)'), freshness));
  const toggle = el('label', 'checkbox-field'); toggle.append(enabled, el('span', '', t('Turn on automatic switching')));
  context.body.append(grid, toggle, el('p', 'form-note', t('Claude Code rotation requires OAuth accounts with an external identity. Managed rotation affects the next request. A response already in progress keeps its account.')));
  context.actions.append(submit(t('Save policy')));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    hysteresis.setCustomValidity(Number(hysteresis.value) >= Number(threshold.value) ? t('Headroom must be less than the usage threshold.') : '');
    if (!context.form.reportValidity()) return;
    const previous = snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value.trim() && policy.target === target.value);
    const policy: RotationPolicy = { provider: provider.value as Provider, pool: pool.value.trim(), target: target.value as RotationPolicy['target'], enabled: enabled.checked, threshold_percent: Number(threshold.value), hysteresis_percent: Number(hysteresis.value), cooldown_seconds: Number(cooldown.value), max_age_seconds: Number(freshness.value), last_switched_at: previous?.last_switched_at ?? null };
    void dialogSave(context, () => adapter.setPolicy(policy), policy.enabled ? t('Automatic switching saved and turned on.') : t('Automatic switching saved; it is off.'));
  });
  hysteresis.addEventListener('input', () => hysteresis.setCustomValidity('')); threshold.addEventListener('input', () => hysteresis.setCustomValidity(''));
  (existing ? threshold : provider).focus();
}

render();
if (demo) { const { createDemoAdapter } = await import('./demo'); adapter = createDemoAdapter(); }
void reload();
// Read monitor metadata on a bounded cadence. Quota probing belongs to the runtime.
let refreshing = false;
// Menus close on Escape and on a click anywhere outside them.
document.addEventListener('keydown', (event) => { if (event.key === 'Escape' && !openMenu && tourStep !== null && !document.querySelector('dialog[open]')) { tourGo(null); return; } if (event.key === 'Escape' && openMenu) { const key = openMenu; openMenu = null; render(); restoreFocus(`menu-${key}`); } });
document.addEventListener('click', (event) => { if (openMenu && !(event.target as HTMLElement | null)?.closest('.menu-root')) { openMenu = null; render(); } });
function backgroundRefresh() {
  if ((!native && !demo) || refreshing || openMenu || document.hidden || document.querySelector('dialog') || busy || loading) return;
  refreshing = true; const stamp = clock.stamp();
  void Promise.all([adapter.snapshot(), readContext()]).then(([data, context]) => {
    // A read issued before a user mutation may predate it; applying it would revert the change.
    if (!clock.accepts(stamp)) return;
    snapshot = data; applyContext(context); backgroundRender();
    // SB-104: the session list is read only while it is on screen.
    if (page === 'activity') void loadSessions();
    // SB-100: Kimi's figures refresh in the background too, every five minutes.
    if (Date.now() - kimiLoadedAt > 5 * 60_000) void loadKimi();
  }).catch(() => { /* Explicit Refresh reports a failed metadata read. */ }).finally(() => { refreshing = false; });
}
setInterval(backgroundRefresh, 60_000);
// SB-117/SB-131: the owner says when it added a folder to a project or took a deleted one out;
// the project list is read again at once.
onProjectSuggestions(() => backgroundRefresh());
// A window shown after hours in the background (SB-30) reads fresh metadata at once rather
// than at the next minute.
document.addEventListener('visibilitychange', () => { if (!document.hidden) backgroundRefresh(); });
