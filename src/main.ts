import './tokens.css';
import './style.css';
import switchboardMark from '../brand/passioncode/switchboard-mark.svg';
import { version } from '../package.json';
import { isAbsoluteProjectPath, platformLabel, projectPathExample } from './platform';
import { demo, native, nativeAdapter, safeError, reportFrontendReady } from './adapter';
import type { CardState } from './ui-logic';
import { APPEARANCE_KEY, EXPIRY_CHOICES, MutationClock, activeRules, autoSwitchPool, canProbe, canSwitchNative, accountReset, accountUsedPercent, cardState, compactCountdown, expiryFrom, failedNextCheck, featureWindowLabel, groupAccounts, isFeatureWindow, limitLabel, intervalWhile, loginOutcome, monitorChecks, parseAppearance, primaryAction, projectName, quotaMaxAge, quotaOrder, resetCountdown, resolveTheme, ruleState, usageFreshness, windowReset, type Appearance } from './ui-logic';
import type { Account, Adapter, AgentSetup, AuthKind, BackupStatus, CurrentAccounts, LoginItem, Project, ExternalIdentity, MonitorStatus, ProjectRule, Provider, RotationPolicy, RuntimeStatus, Snapshot } from './types';

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
const TOUR: { page: Page; target: string | null; title: string; text: string }[] = [
  { page: 'accounts', target: null, title: 'Welcome to Switchboard', text: 'Switchboard keeps your Claude Code and Codex accounts on this computer and moves your work between them, so a usage limit doesn\'t stop a session.' },
  { page: 'accounts', target: '[data-focus="menu-add"]', title: 'Add your accounts', text: 'Press + Add account to sign in to another account, or to save the one Claude Code already uses. Each row then shows how much is used and when it resets.' },
  { page: 'accounts', target: '.rotation-bar', title: 'Switch, by hand or automatically', text: 'Switch moves Claude Code to another account. Turn on automatic switching, and Switchboard moves off an account that nears its limit to the one with the most left.' },
  { page: 'projects', target: '[data-focus="new-project"]', title: 'Give a project its own accounts', text: 'Press + New project, add the project\'s folders and choose its accounts. Sessions launched from those folders use only them, and other projects never switch to them.' },
  { page: 'agents', target: null, title: 'Agents, and it keeps running', text: 'Here, connect Claude Code and Codex agents so they can read usage and switch accounts. Closing the window keeps Switchboard working; open or quit it from the menu-bar icon.' },
];
let tourStep: number | null = (() => { try { return localStorage.getItem(TOUR_KEY) ? null : 0; } catch { return null; } })();
function tourGo(step: number | null) {
  tourStep = step;
  if (step === null) { try { localStorage.setItem(TOUR_KEY, '1'); } catch { /* the tour shows again next start */ } render(); restoreFocus('page-title'); return; }
  page = TOUR[step].page; render(); restoreFocus('tour-title');
}
function tourCard(step: number) {
  const item = TOUR[step]; const last = step === TOUR.length - 1;
  const card = el('section', 'tour-card'); card.setAttribute('role', 'dialog'); card.setAttribute('aria-modal', 'false'); card.setAttribute('aria-labelledby', 'tour-title');
  const count = el('p', 'eyebrow', `Step ${step + 1} of ${TOUR.length}`);
  const title = el('h2', '', item.title); title.id = 'tour-title'; title.tabIndex = -1; title.dataset.focus = 'tour-title';
  const dots = el('div', 'tour-dots'); dots.setAttribute('aria-hidden', 'true'); TOUR.forEach((_, i) => dots.append(el('span', i === step ? 'is-on' : '')));
  const actions = el('div', 'tour-actions');
  actions.append(button('Skip tour', () => tourGo(null), 'text-button', 'tour-skip'));
  if (step > 0) actions.append(button('Back', () => tourGo(step - 1), 'button', 'tour-back'));
  actions.append(button(last ? 'Start using Switchboard' : 'Next', () => tourGo(last ? null : step + 1), 'button primary', 'tour-next'));
  card.append(count, title, el('p', '', item.text), dots, actions);
  return card;
}
let agentSetup: AgentSetup | null = null;
let agentSetupError = false;
let openMenu: string | null = null;
let pendingLogin: { id: string; provider: Provider; state: 'pending' | 'ended' | 'finishing'; error: string } | null = null;
const quotaOpen = new Set<string>();
let backupStatus: BackupStatus | null = null;
let loginItem: LoginItem | null = null;
let loginItemError = false;
let analyticsState: LoginItem | null = null;
let analyticsError = false;
let backupError = false;
let busy = false;
let loading = true;
let loadError = '';
let runtimeError = false;
let notice = '';
let noticeError = false;
let lastWorkingDirectory = '';
const usageErrors = new Map<string, string>();
const providerName = (provider: Provider) => provider === 'claude' ? 'Claude Code' : 'Codex CLI';
const kindName = (kind: AuthKind) => ({ api_key: 'API key', setup_token: 'Setup token', oauth: 'OAuth' })[kind];
const date = (seconds: number) => new Date(seconds * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
const age = (seconds: number) => { const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60)); return minutes < 1 ? 'just now' : minutes < 60 ? `${minutes}m ago` : minutes < 2880 ? `${Math.floor(minutes / 60)}h ago` : `${Math.floor(minutes / 1440)}d ago`; };
const identityText = (identity?: ExternalIdentity | null) => identity?.email || identity?.account_id || 'Identity not reported';
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
function showNotice(text: string, error = false) { notice = text; noticeError = error; announce(text); }
function restoreFocus(key?: string) { if (key) document.querySelectorAll<HTMLElement>('[data-focus]').forEach((node) => { if (node.dataset.focus === key) node.focus(); }); }

async function reload() {
  if (!native && !demo) { loading = false; render(); return; }
  loading = true; loadError = ''; render();
  const [data, status] = await Promise.allSettled([adapter.snapshot(), adapter.runtime()]);
  if (data.status === 'fulfilled') snapshot = data.value;
  else { loadError = safeError(data.reason); announce(loadError); }
  if (status.status === 'fulfilled') { runtime = status.value; runtimeError = false; }
  else { runtime = null; runtimeError = true; }
  loading = false; render();
  void adapter.agentSetup().then((value) => { agentSetup = value; agentSetupError = false; }, () => { agentSetupError = true; }).finally(backgroundRender);
  void loadBackups();
  void adapter.analytics().then((value) => { analyticsState = value; analyticsError = false; }, () => { analyticsError = true; }).finally(backgroundRender);
  void adapter.loginItem().then((value) => { loginItem = value; loginItemError = false; }, () => { loginItemError = true; }).finally(backgroundRender);
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
    catch { loadError = 'The action completed, but accounts could not be refreshed. Retry loading the account list.'; }
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
      const target = TOUR[tourStep].target; if (target) shell.querySelector(target)?.classList.add('tour-target');
      shell.append(tourCard(tourStep));
    }
  };
  const sidebar = el('aside', 'sidebar');
  const brand = el('div', 'brand');
  const mark = el('img', 'brand-mark'); mark.src = switchboardMark; mark.alt = ''; mark.width = 40; mark.height = 40;
  const brandName = el('div', 'brand-name'); brandName.append(el('strong', '', 'Switchboard'), el('span', 'brand-family', 'by PassionCode'));
  brand.append(mark, brandName);
  sidebar.append(brand);
  const nav = el('nav', 'navigation'); nav.setAttribute('aria-label', 'Main navigation');
  const liveRules = activeRules(snapshot?.rules, Date.now() / 1000).length;
  for (const [target, label, icon] of [['accounts', 'Accounts', '▦'], ['projects', 'Projects', '◫'], ['agents', 'Agents', '⌁'], ['activity', 'Activity', '≋'], ['about', 'About', '○']] as const) {
    const control = button('', () => { page = target; notice = ''; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, `nav-item ${page === target ? 'active' : ''}`, `nav-${target}`);
    const symbol = el('span', 'nav-icon', icon); symbol.setAttribute('aria-hidden', 'true');
    control.append(symbol, el('span', '', label));
    if (target === 'projects' && liveRules) { const count = el('span', 'nav-count', String(liveRules)); count.setAttribute('aria-label', `${liveRules} active ${liveRules === 1 ? 'rule' : 'rules'}`); control.append(count); }
    if (page === target) control.setAttribute('aria-current', 'page'); nav.append(control);
  }
  sidebar.append(nav);
  const sidebarFoot = el('div', 'sidebar-foot');
  sidebarFoot.append(el('span', 'eyebrow', 'LOCAL WORKBENCH'), el('p', '', demo ? 'Synthetic session' : 'Your accounts. Your machine.'), el('span', 'version', `v${version} · ${demo ? 'Browser demo' : platformLabel(runtime?.platform)}`));
  sidebar.append(sidebarFoot); shell.append(sidebar);
  const main = el('main', 'main'); main.id = 'main'; main.setAttribute('aria-busy', String(busy || loading));
  if (demo) main.append(el('div', 'demo-banner', 'SYNTHETIC DEMO · No real accounts, vault, proxy, or terminal. Changes reset when you reload.'));
  const header = el('header', 'page-header');
  const heading = el('div'); const title = el('h1', '', { accounts: 'Accounts', projects: 'Projects', agents: 'Agents', activity: 'Activity', about: 'About Switchboard' }[page]); title.tabIndex = -1; title.dataset.focus = 'page-title';
  heading.append(el('p', 'eyebrow', 'FABRIC SWITCHBOARD'), title, el('p', 'subtitle', { accounts: 'Switch Claude Code, keep sign-ins fresh and watch quota.', projects: 'Give a project its own accounts, and keep other projects off them.', agents: 'Let coding agents read usage and switch accounts.', activity: 'Local account and session events.', about: 'Deliberate account switching for coding sessions.' }[page]));
  header.append(heading);
  if (native || demo) {
    const actions = el('div', 'header-actions');
    actions.append(button(loading ? 'Loading…' : 'Refresh', () => void reload(), 'button quiet', 'refresh'));
    if (page === 'accounts') actions.append(addMenu());
    if (page === 'projects') actions.append(button('+ New project', () => projectDialog(), 'button primary', 'new-project'));
    header.append(actions);
  }
  main.append(header);
  if (busy) { const progress = el('p', 'operation-progress', 'Working…'); progress.setAttribute('role', 'status'); main.append(progress); }
  if (!native && !demo) {
    main.append(emptyState('Open the native app', 'Account storage and session launches are available in the Fabric Switchboard desktop app. This browser window has no access to your accounts.'));
    shell.append(main); return commit();
  }
  if (notice) { const alert = el('div', `notice ${noticeError ? 'error' : 'success'}`); alert.append(el('span', '', notice), button('Dismiss', () => { notice = ''; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, 'text-button', 'dismiss-notice')); main.append(alert); }
  loginBanner(main);
  if (page === 'about') renderAbout(main);
  else if (page === 'agents') renderAgents(main);
  else if (loadError) {
    const state = emptyState('Unable to load accounts', loadError); state.classList.add('error-state'); state.append(button('Retry', () => void reload(), 'button primary', 'retry-load')); main.append(state);
  } else if (loading && !snapshot) {
    const state = emptyState('Loading your workbench', 'Reading account metadata from the native app…'); state.setAttribute('role', 'status'); main.append(state);
  } else if (snapshot) {
    if (page === 'accounts') renderAccounts(main);
    else if (page === 'projects') renderProjects(main);
    else renderActivity(main);
  }
  shell.append(main); return commit();
}
const ruleAccount = (rule: ProjectRule) => snapshot?.accounts.find((account) => account.id === rule.account_id);
const ruleTarget = (rule: ProjectRule) => rule.target === 'managed' ? 'Managed sessions' : 'Claude Code login (all claude sessions)';
/** Active rules stay in sight on Accounts, so an optional rule is never forgotten. */
function rulesStrip(main: HTMLElement) {
  const live = activeRules(snapshot?.rules, Date.now() / 1000);
  if (!live.length) return;
  const strip = el('section', 'notice rules-strip'); strip.setAttribute('aria-label', 'Active project rules');
  const names = live.map((rule) => `${projectName(rule.path)} → ${ruleAccount(rule)?.label ?? 'missing account'}${rule.expires_at ? ` until ${date(rule.expires_at)}` : ''}`);
  strip.append(el('span', '', `${live.length} project ${live.length === 1 ? 'rule is' : 'rules are'} active: ${names.join('; ')}. Rotation still moves off an exhausted account.`), button('Review rules', () => { page = 'projects'; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, 'text-button', 'review-rules'));
  main.append(strip);
}
function renderProjects(main: HTMLElement) {
  main.append(projectsSection(), rulesSection());
}
/** Projects (0.6): folders that reserve their own accounts (core `save_project`). */
function projectsSection() {
  const section = el('section', 'project-section'); section.setAttribute('aria-labelledby', 'projects-heading');
  const heading = el('h2', 'section-heading', 'Projects'); heading.id = 'projects-heading';
  section.append(heading, el('p', 'surface-note', 'A project is one or more folders, such as related repositories, with accounts of its own. Sessions launched from its folders use only its accounts, rotation stays inside them, and no other project or the ordinary Claude Code switches to them.'));
  const projects = snapshot!.projects ?? [];
  if (!projects.length) {
    const empty = emptyState('No projects yet', 'Create one when a client or a team should work only on its own subscriptions. Its accounts move into the project, and its folders stay on them.');
    empty.append(button('New project', () => projectDialog(), 'button primary', 'empty-new-project'));
    section.append(empty); return section;
  }
  const list = el('div', 'account-list'); list.setAttribute('aria-label', 'Projects');
  for (const project of projects) {
    const accounts = snapshot!.accounts.filter((a) => a.pool === project.pool);
    const card = el('article', 'account-card project-card'); card.setAttribute('aria-label', `${project.name}, ${accounts.length} ${accounts.length === 1 ? 'account' : 'accounts'}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    title.append(el('h2', '', project.name), el('span', 'badge muted', `${accounts.length} ${accounts.length === 1 ? 'account' : 'accounts'}`));
    const folders = el('ul', 'project-folders'); for (const folder of project.folders) { const item = el('li', 'account-meta', folder); item.title = folder; folders.append(item); }
    const chips = el('p', 'account-meta', accounts.length ? accounts.map((a) => `${a.label} · ${providerName(a.provider)}`).join(', ') : 'No accounts yet — sessions in these folders use your other accounts until you add one.');
    details.append(title, folders, chips);
    const actions = el('div', 'management-actions');
    actions.append(button('Edit', () => projectDialog(project), 'text-button', `edit-project-${project.pool}`), button('Delete', () => void mutate(() => adapter.removeProject(project.pool), `Project “${project.name}” deleted. Its accounts stay in the pool ${project.pool}, no longer reserved.`, 'new-project'), 'text-button danger-text', `delete-project-${project.pool}`));
    card.append(details, actions); list.append(card);
  }
  section.append(list); return section;
}
function projectDialog(existing?: Project) {
  const context = openDialog(existing ? `Edit ${existing.name}` : 'New project', 'Add the project\'s folders and choose the accounts that belong to it. An account belongs to one project at a time.');
  const name = input(existing?.name ?? ''); name.maxLength = 80; name.placeholder = 'e.g. Client Alpha';
  const folders = el('textarea'); folders.rows = 3; folders.required = true; folders.spellcheck = false; folders.value = (existing?.folders ?? (lastWorkingDirectory ? [lastWorkingDirectory] : [])).join('\n'); folders.placeholder = projectPathExample(runtime?.platform);
  const picker = el('fieldset', 'project-accounts'); picker.append(el('legend', 'field-label', 'Accounts'));
  const usable = snapshot!.accounts;
  if (!usable.length) picker.append(el('p', 'form-note', 'No accounts yet. Add accounts first, or save the project now and add them later.'));
  for (const account of usable) {
    const owner = projectOf(account.pool);
    const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.value = account.id; box.checked = !!existing && account.pool === existing.pool;
    const note = owner && owner.pool !== existing?.pool ? ` · in ${owner.name}, moves here` : '';
    option.append(box, el('span', '', `${account.label} · ${providerName(account.provider)}${note}`)); picker.append(option);
  }
  const grid = el('div', 'form-grid'); grid.append(field('Project name', name), field('Folders', folders, 'One absolute path per line: the repositories and folders of this project. Subfolders are included.'));
  context.body.append(grid, picker, el('p', 'form-note', 'Accounts you leave out move back to the default pool.'));
  context.actions.append(submit(existing ? 'Save project' : 'Create project'));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const accountIds = [...picker.querySelectorAll<HTMLInputElement>('input[type=checkbox]:checked')].map((box) => box.value);
    const list = folders.value.split('\n').map((line) => line.trim()).filter(Boolean);
    void dialogSave(context, () => adapter.saveProject({ pool: existing?.pool, name: name.value, folders: list, accountIds }), existing ? `Project “${name.value.trim()}” saved.` : `Project “${name.value.trim()}” created. Launch its accounts from its folders.`);
  });
}
function rulesSection() {
  const main = el('section', 'project-section'); main.setAttribute('aria-labelledby', 'rules-heading');
  const heading = el('div', 'section-heading-row'); const h = el('h2', 'section-heading', 'Project rules'); h.id = 'rules-heading';
  heading.append(h); if (snapshot!.accounts.length) heading.append(button('+ Add rule', () => ruleDialog(), 'button quiet', 'add-rule'));
  main.append(heading);
  const rules = snapshot!.rules ?? [];
  main.append(el('p', 'surface-note', 'Rules are optional. With none, Switchboard follows your selection and rotation in every project. A rule applies when an agent or switchboard project apply asks for it, only to that session, and never turns rotation off.'));
  if (!rules.length) {
    const empty = emptyState('No project rules', snapshot!.accounts.length ? 'Add a rule when one project should start on a specific account, for example a client project on its own subscription. Prefer an expiry.' : 'Add an account first; a rule names one of your accounts.');
    if (snapshot!.accounts.length) empty.append(button('Add rule', () => ruleDialog(), 'button primary', 'empty-add-rule'));
    main.append(empty); return main;
  }
  const now = Date.now() / 1000;
  const list = el('div', 'account-list'); list.setAttribute('aria-label', 'Project rules');
  for (const rule of rules) {
    const state = ruleState(rule, now); const account = ruleAccount(rule); const key = `${rule.provider}-${rule.path}`;
    const card = el('article', `account-card ${state === 'active' ? 'is-selected' : 'is-disabled'}`); card.setAttribute('aria-label', `${projectName(rule.path)}, ${state}`);
    const details = el('div', 'account-details'); const title = el('div', 'account-title');
    title.append(el('h2', '', projectName(rule.path)), el('span', `badge ${state === 'active' ? 'current-badge' : 'muted'}`, { active: 'Active', paused: 'Paused', expired: 'Expired' }[state]));
    const path = el('p', 'account-meta', rule.path); path.title = rule.path;
    details.append(title, path, el('p', 'account-meta', `${providerName(rule.provider)} · ${account ? `${account.label} · ${account.pool}` : 'account missing'} · ${ruleTarget(rule)}`), el('p', 'usage-caption', rule.expires_at ? `${state === 'expired' ? 'Expired' : 'Until'} ${date(rule.expires_at)}` : 'No expiry'));
    const actions = el('div', 'management-actions');
    if (state === 'active') actions.append(button('Pause', () => void mutate(() => adapter.setProjectRule({ path: rule.path, accountId: rule.account_id, target: rule.target, enabled: false, expiresAt: rule.expires_at }), 'Rule paused. Selection and rotation are unchanged.', `pause-${key}`), 'text-button', `pause-${key}`));
    else actions.append(button('Resume…', () => ruleDialog(rule), 'text-button', `resume-${key}`));
    actions.append(button('Edit', () => ruleDialog(rule), 'text-button', `edit-${key}`), button('Remove', () => void mutate(() => adapter.removeProjectRule(rule.path, rule.provider), 'Rule removed.', 'add-rule'), 'text-button danger-text', `remove-${key}`));
    card.append(details, actions); list.append(card);
  }
  main.append(list);
  return main;
}
function ruleDialog(existing?: ProjectRule) {
  const context = openDialog(existing ? 'Edit project rule' : 'Add project rule', 'Sessions in this folder and its subfolders start on the chosen account when the rule is applied. Rotation still runs.');
  const path = input(existing?.path ?? lastWorkingDirectory); path.placeholder = projectPathExample(runtime?.platform);
  if (existing) { path.disabled = true; path.dataset.locked = 'true'; }
  const usable = snapshot!.accounts.filter((account) => account.enabled);
  const account = select(usable.map((item) => [item.id, `${item.label} · ${providerName(item.provider)} · ${item.pool}`]));
  if (existing) account.value = existing.account_id;
  const target = select([['managed', 'Managed sessions (switch from the next request)'], ['claude_cli', 'Claude Code login (changes every claude session)']]);
  target.value = existing?.target ?? 'managed';
  const expiry = select(EXPIRY_CHOICES); expiry.value = existing && existing.expires_at === null ? '' : '8';
  const sync = () => { const chosen = usable.find((item) => item.id === account.value); const option = target.querySelector<HTMLOptionElement>('option[value="claude_cli"]')!; option.disabled = !(chosen?.provider === 'claude' && chosen.kind === 'oauth' && chosen.external_identity); if (option.disabled && target.value === 'claude_cli') target.value = 'managed'; };
  account.addEventListener('change', sync); sync();
  const grid = el('div', 'form-grid'); grid.append(field('Project folder', path, 'An absolute path to an existing folder.'), field('Account', account), field('Applies to', target), field('Keep the rule', expiry, 'Pause or remove it any time on this screen.'));
  context.body.append(grid); context.actions.append(submit(existing ? 'Save rule' : 'Add rule'));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const folder = path.value.trim();
    if (!isAbsoluteProjectPath(folder, runtime?.platform)) { path.setCustomValidity(`Enter an absolute folder, for example ${projectPathExample(runtime?.platform)}.`); path.reportValidity(); path.addEventListener('input', () => path.setCustomValidity(''), { once: true }); return; }
    void dialogSave(context, () => adapter.setProjectRule({ path: folder, accountId: account.value, target: target.value as ProjectRule['target'], enabled: true, expiresAt: expiryFrom(expiry.value, Math.floor(Date.now() / 1000)) }), 'Rule saved and active. It applies when an agent or switchboard project apply asks.');
  });
  (existing ? account : path).focus();
}
function copyable(label: string, command: string, key: string) {
  const row = el('div', 'policy-row'); const content = el('div');
  content.append(el('strong', 'block', label), el('code', 'address', command));
  row.append(content, button('Copy', () => { void navigator.clipboard?.writeText(command).then(() => announce(`${label} command copied.`), () => announce('Copy is unavailable here. Select the command text instead.')); }, 'text-button', key));
  return row;
}
function renderAgents(main: HTMLElement) {
  const panel = el('section', 'about-panel');
  panel.append(el('h2', '', 'What agents can do'), el('p', '', 'Coding agents connect to switchboard mcp. They read who handles their requests and how much quota remains, switch the account of a managed session from the next request, and apply a project rule when they start work in a project. Changing the ordinary Claude Code login affects every claude session, so agents must pass global: true and should ask you first.'));
  panel.append(el('p', 'form-note', 'Sessions you launch from Switchboard get these tools automatically when the command-line tool is available. Isolated sessions get the read-only tools.'));
  main.append(panel);
  const setup = el('section', 'about-panel'); setup.append(el('h2', '', 'Connect an agent'));
  if (agentSetupError || !agentSetup) setup.append(el('p', 'form-note', agentSetupError ? 'Agent setup is unavailable. Refresh to retry.' : 'Reading agent setup…'));
  else {
    if (agentSetup.translocated) setup.append(el('p', 'form-note', 'macOS is running Switchboard from a temporary copy of the download. Move Fabric Switchboard to Applications and open it from there before connecting agents; a path into the copy stops working when the app quits.'));
    setup.append(el('p', 'form-note', agentSetup.cli_path ? `Command-line tool: ${agentSetup.cli_path}` : 'The switchboard command-line tool was not found on PATH.'));
    if (agentSetup.can_link && !agentSetup.linked_cli) setup.append(button('Link switchboard into ~/.local/bin', () => void mutate(async () => { await adapter.linkCli(); agentSetup = await adapter.agentSetup(); }, 'The command-line tool is linked. Agents and plugins can now start switchboard mcp.', 'link-cli'), 'button primary', 'link-cli'));
    setup.append(copyable('Claude Code', agentSetup.commands.claude_code, 'copy-claude'), copyable('Codex CLI', agentSetup.commands.codex, 'copy-codex'), copyable('Claude Code plugin (tools and the switching-accounts skill)', agentSetup.commands.claude_plugin, 'copy-plugin'));
  }
  main.append(setup);
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
function resetDisplay(until: number, label = 'Resets') {
  if (resetCountdown(until, Date.now() / 1000) === 'Time unavailable') return el('span', 'usage-unknown', 'Time unavailable');
  const wrapper = el('span', 'reset-display');
  const time = el('time', '', `${label} ${date(until)}`); time.dateTime = new Date(until * 1000).toISOString();
  time.title = new Date(until * 1000).toLocaleString(undefined, { timeZoneName: 'short' });
  const remaining = el('span', 'reset-remaining', countdownPaused ? pausedCountdowns.get(`${until}|long`) ?? 'Countdown paused' : resetCountdown(until, Date.now() / 1000));
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
      const item = button(text, () => { openMenu = null; action(); }, `menu-item ${tone ?? ''}`); item.setAttribute('role', 'menuitem'); item.tabIndex = -1; list.append(item);
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
  return menu('add', 'Add account', [
    ['Sign in to another Claude account', () => void startLogin('claude')],
    ['Sign in to another Codex account', () => void startLogin('codex')],
    ['Import from Claude Swap', () => void importSwap()],
    ['Add an API key or token…', () => addDialog()],
  ], 'button primary', '+ Add account');
}
/** Official sign-in without a form: Terminal opens, completion is detected, the email names the account. */
async function startLogin(provider: Provider, label = '', pool = 'default') {
  if (pendingLogin) { showNotice('A sign-in is already in progress. Finish it in Terminal or cancel it first.', true); render(); return; }
  busy = true; notice = ''; render();
  try {
    const result = await adapter.beginLogin({ provider, label, pool });
    pendingLogin = { id: result.login_id, provider, state: 'pending', error: '' };
    announce(`Sign in to ${providerName(provider)} in the Terminal window that opened. Switchboard adds the account when you finish.`);
  } catch (error) { showNotice(safeError(error), true); }
  finally { busy = false; render(); restoreFocus('login-cancel'); }
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
  if (state === 'ended') { login.state = 'ended'; render(); announce('Sign-in ended in Terminal without adding an account.'); return; }
  if (state !== 'complete') return;
  login.state = 'finishing'; render();
  busy = true; clock.begin();
  try {
    const account = await adapter.finishLogin(login.id);
    // Saved either way; a pending cleanup is reported, never a failed sign-in (SB-42).
    await loginSaved(account.login_cleanup === 'pending'
      ? `${account.label} added to ${providerName(account.provider)} · ${account.pool}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.`
      : `${account.label} added to ${providerName(account.provider)} · ${account.pool}.`);
  } catch (error) {
    const text = safeError(error); const outcome = loginOutcome(text);
    // The owner saved the account and released the sign-in; only its staging folder is left.
    if (outcome === 'saved_with_cleanup') await loginSaved(`Account added to ${providerName(login.provider)}. Switchboard could not remove its temporary sign-in folder yet and retries before the next sign-in.`);
    else if (outcome === 'forgotten') login.state = 'ended';
    else { login.state = 'pending'; login.error = text; }
  }
  finally { clock.end(); busy = false; render(); }
}
/** The account is saved: the banner closes, then the list and the current CLI accounts refresh. */
async function loginSaved(text: string) {
  pendingLogin = null;
  await refreshContext();
  try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'The account was added, but the list could not be refreshed. Retry loading accounts.'; }
  showNotice(text);
}
async function cancelLogin() {
  const login = pendingLogin; if (!login) return;
  busy = true; render();
  try { await adapter.cancelLogin(login.id); pendingLogin = null; showNotice('Sign-in cancelled. No account was added.'); }
  catch (error) {
    const text = safeError(error);
    // The owner restarted and no longer knows this sign-in: there is nothing left to cancel.
    if (loginOutcome(text) === 'forgotten') { if (pendingLogin === login) pendingLogin = null; showNotice('Sign-in closed. Switchboard had already ended it; close its Terminal window if it is still open.'); }
    else login.error = text;
  }
  finally { busy = false; render(); }
}
function loginBanner(main: HTMLElement) {
  const login = pendingLogin; if (!login) return;
  const banner = el('section', `login-banner ${login.state === 'ended' || login.error ? 'is-error' : ''}`); banner.setAttribute('role', 'status');
  const text = el('div', 'login-copy');
  if (login.state === 'ended') text.append(el('strong', '', 'Sign-in ended without an account'), el('span', '', 'The Terminal session closed before the sign-in finished. Start again when you are ready.'));
  else if (login.state === 'finishing') text.append(el('strong', '', 'Adding the account…'), el('span', '', 'Reading the new sign-in from its private home.'));
  else text.append(el('strong', '', `Signing in to ${providerName(login.provider)}`), el('span', '', 'Finish in the Terminal window. If a browser opens, complete that step first. Switchboard adds the account on its own.'));
  if (login.error) text.append(el('span', 'usage-error', login.error));
  const actions = el('div', 'login-actions');
  if (login.error && login.state === 'pending') actions.append(button('Retry', () => { login.error = ''; render(); void pollLogin(); }, 'button', 'login-retry-finish'));
  if (login.state === 'ended') actions.append(button('Try again', () => { const provider = login.provider; void adapter.cancelLogin(login.id).catch(() => undefined).finally(() => { pendingLogin = null; void startLogin(provider); }); }, 'button', 'login-retry'));
  actions.append(button(login.state === 'ended' ? 'Dismiss' : 'Cancel', () => void cancelLogin(), 'button quiet', 'login-cancel'));
  banner.append(text, actions); main.append(banner);
}
async function importSwap() {
  busy = true; notice = ''; clock.begin(); render();
  try {
    const result = await adapter.importClaudeSwap('default');
    await refreshContext();
    try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'Import completed, but accounts could not be refreshed. Refresh to load the imported accounts.'; }
    const parts = [`${result.imported.length} Claude Swap ${result.imported.length === 1 ? 'profile' : 'profiles'} imported or updated in the default pool`];
    if (result.skipped) parts.push(`${result.skipped} skipped`);
    if (result.failed) parts.push(`${result.failed} could not be read — check them in Claude Swap and import again`);
    if (result.claude_swap_running) parts.push('Claude Swap is still running, so it keeps renewing those accounts and Switchboard follows its newest sign-ins');
    showNotice(`${parts.join(' · ')}.`, result.failed > 0);
  } catch (error) { showNotice(safeError(error), true); }
  finally { clock.end(); busy = false; render(); restoreFocus('menu-add'); }
}
function switchNative(account: Account) {
  void mutate(() => adapter.activateNative(account.id), demo ? `Synthetic switch: Claude Code now uses ${account.label}. No local credentials were read or written.` : `Claude Code now uses ${account.label}. New claude sessions start on it; a running session may need a restart.`, `primary-${account.id}`);
}
function renderAccounts(main: HTMLElement) {
  rulesStrip(main);
  renderCurrent(main);
  renderPolicies(main);
  const accounts = snapshot!.accounts;
  if (!accounts.length) {
    const empty = emptyState('Start with one account', 'Add the account Claude Code or Codex CLI already uses with one click above, or sign in to another account. Nothing opens until you choose.');
    empty.append(addMenu()); main.append(empty); return;
  }
  const sorting = el('div', 'account-sort-note');
  sorting.append(el('span', '', 'Within each pool: remaining quota first, then shortest wait. Unknown usage follows.'), button(countdownPaused ? 'Resume countdown' : 'Pause countdown', () => {
    if (!countdownPaused) { pausedCountdowns.clear(); root.querySelectorAll<HTMLElement>('[data-countdown]').forEach(node => pausedCountdowns.set(countdownKey(node), node.textContent ?? '')); }
    countdownPaused = !countdownPaused; render(); restoreFocus('countdown-toggle');
  }, 'button quiet', 'countdown-toggle'));
  main.append(sorting);
  for (const group of groupAccounts(accounts, quotaContext())) {
    const section = el('section', 'account-group'); section.setAttribute('aria-label', `${providerName(group.provider)} accounts`);
    const heading = el('div', 'group-heading');
    heading.append(el('h2', '', providerName(group.provider)), el('span', 'count', `${group.count} ${group.count === 1 ? 'account' : 'accounts'}`));
    section.append(heading);
    for (const pool of group.pools) {
      const owner = projectOf(pool.pool);
      if (group.pools.length > 1 || owner) section.append(el('h3', 'pool-heading', owner ? `Project · ${owner.name}` : `Pool · ${pool.pool}`));
      const list = el('div', 'account-list'); list.setAttribute('role', 'list');
      pool.accounts.forEach((account) => list.append(accountRow(account)));
      section.append(list);
    }
    main.append(section);
  }
  main.append(el('p', 'surface-note', 'Switch changes the account of the ordinary Claude Code on this Mac. Select chooses the account for managed sessions launched from Switchboard; a response already in progress keeps its account.'));
}
function accountRow(account: Account) {
  const current = currentMatch(account); const active = selected(account); const signIn = signInRequired(account);
  const row = el('article', `account-row ${current ? 'is-current' : ''} ${active ? 'is-selected' : ''} ${!account.enabled ? 'is-disabled' : ''}`); row.setAttribute('role', 'listitem');
  row.setAttribute('aria-label', `${account.label}, ${providerName(account.provider)}, ${account.pool} pool${current ? ', in use by the CLI' : ''}`);
  const state = cardState(account, quotaContext()); row.dataset.state = state;
  const icon = el('span', `provider-icon ${account.provider}`, account.provider === 'claude' ? '✳' : '◎'); icon.setAttribute('aria-hidden', 'true'); icon.title = CARD_STATE[state];
  const name = el('div', 'row-name');
  const title = el('div', 'row-title'); title.append(el('strong', '', account.label));
  if (current) title.append(el('span', 'badge current-badge', account.provider === 'claude' ? 'In Claude Code' : 'In Codex CLI'));
  if (active) title.append(el('span', 'badge selected-badge', 'Next managed request'));
  if (signIn) title.append(el('span', 'badge danger-badge', 'Sign in again'));
  if (!account.enabled) title.append(el('span', 'badge muted', 'Disabled'));
  const email = account.external_identity?.email;
  name.append(title, el('span', 'row-sub', [email && email !== account.label ? email : '', account.kind === 'oauth' ? '' : kindName(account.kind)].filter(Boolean).join(' · ') || (account.kind === 'oauth' ? 'OAuth' : '')));
  const project = !!projectOf(account.pool);
  row.append(icon, name, usageCell(account), primaryButton(account, { current, selected: active, signIn, project }), rowMenu(account, { current, selected: active, signIn, project }));
  if (quotaOpen.has(account.id) && account.usage) row.append(quotaDetails(account));
  if (usageErrors.has(account.id)) row.append(el('p', 'row-error', usageErrors.get(account.id)!));
  return row;
}
function primaryButton(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const slot = el('div', 'row-primary'); const key = `primary-${account.id}`;
  const action = primaryAction(account, state);
  const status = (text: string) => { const label = el('span', 'row-state', text); label.tabIndex = -1; label.dataset.focus = key; return label; };
  if (action === 'in_use') slot.append(status('✓ In use'));
  else if (action === 'selected') slot.append(status('✓ Selected'));
  else if (action === 'switch') { const control = button('Switch', () => switchNative(account), 'button row-button', key); control.title = 'Make this the Claude Code account on this Mac'; slot.append(control); }
  else if (action === 'select') slot.append(button('Select', () => void mutate(() => adapter.select(account), `${account.label} selected for the next managed request in ${account.pool}.`, key), 'button row-button', key));
  else if (action === 'sign_in') slot.append(button('Sign in', () => void startLogin(account.provider, account.label, account.pool), 'button row-button', key));
  else slot.append(button('Enable', () => void mutate(() => adapter.update(account.id, account.label, true), `${account.label} enabled.`, key), 'button row-button quiet', key));
  return slot;
}
function rowMenu(account: Account, state: { current: boolean; selected: boolean; signIn: boolean; project: boolean }) {
  const items: ([string, () => void] | [string, () => void, string])[] = [];
  if (account.enabled && canSwitchNative(account) && !state.project && !state.selected) items.push(['Select for managed sessions', () => void mutate(() => adapter.select(account), `${account.label} selected for the next managed request in ${account.pool}.`, `menu-${account.id}`)]);
  if (account.enabled && canProbe(account)) items.push(['Check usage', () => void mutate(() => adapter.probe(account.id), 'Usage observation updated.', `menu-${account.id}`, account.id)]);
  if (account.enabled) items.push(['Launch isolated…', () => launchDialog(account, 'isolated')]);
  if (account.enabled && state.selected && !runtimeError) items.push(['Launch managed…', () => launchDialog(account, 'managed')]);
  if (account.kind === 'oauth' && !state.signIn) items.push(['Sign in again', () => void startLogin(account.provider, account.label, account.pool)]);
  items.push(['Rename or disable…', () => editDialog(account)], ['Remove…', () => removeDialog(account), 'danger-text']);
  return menu(account.id, `More actions for ${account.label}`, items);
}
/** What the status mark and the screen reader say for each card state. */
const CARD_STATE: Record<CardState, string> = { available: 'Available', low: 'Running low', blocked: 'Limit reached', stale: 'Stale', failed: 'Check failed', unknown: 'Usage unknown', sign_in: 'Sign in again', disabled: 'Disabled', no_quota: 'No quota check' };
/** "{lead} 2h 5m · 5 Oct, 21:03": a compact wait that the minute timer keeps current. */
function compactWait(lead: string, until: number, estimated = false) {
  const wrapper = el('span', 'usage-wait');
  const remaining = el('span', 'usage-countdown', countdownPaused ? pausedCountdowns.get(`${until}|compact`) ?? 'paused' : compactCountdown(until, Date.now() / 1000));
  remaining.dataset.countdown = String(until); remaining.dataset.format = 'compact'; remaining.setAttribute('aria-live', 'off');
  const time = el('time', '', date(until)); time.dateTime = new Date(until * 1000).toISOString();
  time.title = new Date(until * 1000).toLocaleString(undefined, { timeZoneName: 'short' });
  wrapper.append(`${lead} `, remaining, ' · ', time);
  if (estimated) wrapper.append(el('span', 'usage-estimate', ' · estimated'));
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
    ageNode.title = `Checked ${date(observation.observed_at)}`; top.append(ageNode);
  } else {
    top.append(el('span', 'usage-unknown', account.kind === 'api_key' ? 'API billing' : account.kind === 'setup_token' ? 'No quota check' : 'Usage unknown'));
  }
  // Line two: the time that decides what happens next.
  const sub = el('span', `usage-sub is-${state}`);
  const order = quotaOrder(account, context);
  if (state === 'blocked') {
    const estimated = !!limit && order.until === limit.until && limitLabel(limit) !== 'Limit resets';
    if (order.until) sub.append(compactWait(estimated ? 'Retry in' : 'Back in', order.until, estimated)); else sub.append('Reset time unavailable');
    if (limit) sub.title = limit.source === 'managed' ? 'A managed request was refused with a rate limit.' : 'Claude Code reported a usage or spend limit. The retry hold may be estimated.';
  } else if (state === 'failed') {
    const next = failedNextCheck(account); sub.append(next !== null ? `Check failed · next ${date(next)}` : 'Check failed · check usage to retry');
  } else if (state === 'stale') {
    const reset = observation ? accountReset(observation) : null;
    sub.append(freshness?.resetPassed ? 'Reset since check · awaiting a new one' : reset ? `Stale · reported reset ${date(reset)}` : 'Stale · awaiting a new check');
  } else if (state === 'available' || state === 'low') {
    const reset = observation ? accountReset(observation) : null;
    if (reset) sub.append(compactWait('Resets in', reset)); else sub.append('Reset time unavailable');
  } else if (state === 'unknown') sub.append(monitorChecks(account) ? (health ? `Next check ${date(health.next_check_at)}` : 'Waiting for the first check') : 'Not checked automatically');
  else if (state === 'sign_in') sub.append('Sign in to check usage');
  else if (state === 'disabled') sub.append('Not checked while disabled');
  const label = `${CARD_STATE[state]}${used !== null ? `, ${Math.round(used)}% used` : ''}. ${sub.textContent ?? ''}`;
  if (!observation) { const cell = el('div', 'row-usage'); cell.setAttribute('role', 'group'); cell.setAttribute('aria-label', label); cell.append(top, sub); return cell; }
  const control = button('', () => { if (quotaOpen.has(account.id)) quotaOpen.delete(account.id); else quotaOpen.add(account.id); render(); restoreFocus(`quota-${account.id}`); }, 'row-usage', `quota-${account.id}`);
  control.setAttribute('aria-expanded', String(quotaOpen.has(account.id)));
  control.setAttribute('aria-label', `${label} Show quota windows.`);
  control.append(top, sub);
  return control;
}
function quotaDetails(account: Account) {
  const observation = account.usage!; const now = Date.now() / 1000; const health = account.usage_health;
  const details = el('div', 'row-details');
  const windows = observation.windows?.length ? observation.windows : [{ name: 'Highest reported window', used_percent: observation.used_percent, resets_at: observation.resets_at }];
  const limit = accountLimit(account);
  if (limit) {
    // The hold is not a quota reset; the windows below keep their own reset times.
    const row = el('div', 'quota-window'); row.append(el('strong', '', `Limit reached · ${limit.source === 'managed' ? 'a managed request was refused' : 'reported by Claude Code'}`), resetDisplay(limit.until, limitLabel(limit)));
    details.append(row);
  }
  for (const window of windows) {
    const reset = windowReset(window.resets_at, now);
    const name = 'name' in window && isFeatureWindow(window) ? featureWindowLabel(window.name) : window.name;
    const row = el('div', 'quota-window'); row.append(el('strong', '', reset ? `${name} · usage unknown since reset` : `${name} · ${Math.round(window.used_percent)}% used`));
    if (reset) row.append(el('span', 'usage-caption', `Reset ${date(window.resets_at!)} · ${Math.round(window.used_percent)}% was used before`));
    else if (window.resets_at) row.append(resetDisplay(window.resets_at));
    else row.append(el('span', 'usage-caption', 'Reset time unavailable'));
    details.append(row);
  }
  details.append(el('p', 'usage-caption', `${observation.source} · Observed ${date(observation.observed_at)}`));
  if (health?.status === 'failed') details.append(el('p', 'usage-error', 'Last quota check failed. Not eligible for automatic switching until a check succeeds.'));
  if (monitorChecks(account) && health) details.append(el('span', 'usage-caption', `Checked ${age(health.checked_at)} · Next check ${date(health.next_check_at)}`));
  else if (!monitorChecks(account)) details.append(el('span', 'usage-caption', canProbe(account) ? 'Not checked automatically while disabled' : 'Not checked automatically · Quota checks need OAuth'));
  return details;
}
function addDialog() {
  const context = openDialog('Add an API key or token', 'For accounts without an official sign-in. To add a Claude Code or Codex login, use + Add account → Sign in, or add the account the CLI already uses.');
  const provider = select([['claude', 'Claude Code'], ['codex', 'Codex CLI']]);
  const method = select([['api_key', 'API key'], ['setup_token', 'Claude setup token'], ['oauth', 'OAuth JSON']]);
  const label = input(); label.maxLength = 80; label.placeholder = 'e.g. Studio';
  const pool = input('default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+'; pool.title = 'Use lowercase letters, numbers, hyphens, or underscores.';
  const grid = el('div', 'form-grid'); grid.append(field('Provider', provider), field('Credential type', method), field('Account label', label), field('Pool', pool, 'A boundary for routing, such as work or personal.'));
  const credentialSlot = el('div'); context.actions.append(submit('Add account')); context.body.append(grid, credentialSlot);
  let secret: HTMLInputElement | HTMLTextAreaElement | null = null;
  const update = () => {
    if (secret) secret.value = ''; credentialSlot.replaceChildren(); secret = null;
    const setup = method.querySelector<HTMLOptionElement>('option[value="setup_token"]')!; setup.disabled = provider.value !== 'claude';
    if (setup.disabled && method.value === 'setup_token') method.value = 'api_key';
    if (method.value === 'oauth') { const textarea = el('textarea', 'secret-json'); textarea.rows = 5; textarea.required = true; textarea.spellcheck = false; textarea.autocomplete = 'off'; textarea.placeholder = 'Paste credential JSON'; secret = textarea; }
    else { secret = input('', 'password'); secret.autocomplete = 'new-password'; secret.placeholder = 'Enter credential'; }
    credentialSlot.append(field(method.value === 'oauth' ? 'OAuth credential JSON' : 'Credential', secret, demo ? 'Use synthetic input only. Nothing is stored after reload.' : 'Sent only to native credential storage. Cleared on submit or cancel.'));
    if (method.value === 'oauth') credentialSlot.append(el('p', 'form-note', 'Imported OAuth is a snapshot. Sign in again when it expires. Imported identity is not independently verified.'));
  };
  provider.addEventListener('change', update); method.addEventListener('change', update); update();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault(); if (!context.form.reportValidity()) return;
    if (!label.value.trim()) { label.setCustomValidity('Enter an account label.'); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; }
    const value = secret!.value; secret!.value = '';
    void dialogSave(context, () => adapter.add({ provider: provider.value as Provider, label: label.value.trim(), pool: pool.value.trim(), kind: method.value as AuthKind, secret: value }), 'Account added. Select it when you are ready.');
  });
  provider.focus();
}
function renderCurrent(main: HTMLElement) {
  const section = el('section', 'current-section'); section.setAttribute('aria-label', 'Accounts the CLIs use now');
  const heading = el('div', 'section-heading');
  const proxy = el('span', 'proxy-status'); const dot = el('span', `status-dot ${runtimeError ? 'unavailable' : ''}`); dot.setAttribute('aria-hidden', 'true');
  proxy.append(dot, el('span', '', demo ? 'Demo · no live requests' : runtimeError ? 'Local proxy status unavailable' : `Local proxy ${runtime?.proxy_address ?? 'starting…'}`));
  heading.append(el('h2', '', 'In use now'), proxy);
  section.append(heading);
  const grid = el('div', 'current-grid');
  for (const provider of ['claude', 'codex'] as const) {
    const current = currentAccounts?.[provider]; const card = el('div', 'current-card');
    const copy = el('div', 'current-copy'); copy.append(el('span', 'current-provider', providerName(provider)));
    if (current?.status === 'available') {
      copy.append(el('strong', 'current-identity', identityText(current.identity)));
      const matches = snapshot!.accounts.filter((account) => account.provider === provider && currentMatch(account));
      card.append(copy);
      if (matches.length) card.append(el('span', 'current-state', `✓ Saved as ${matches.map((account) => account.label).join(', ')}`));
      else card.append(button('Add to Switchboard', () => void mutate(() => adapter.captureCurrent({ provider, pool: 'default' }), `${identityText(current.identity)} added to ${providerName(provider)} · default. Switchboard keeps its sign-in up to date.`, `capture-${provider}`), 'button primary row-button', `capture-${provider}`));
    } else {
      copy.append(el('strong', 'current-identity muted-text', current?.status === 'missing' ? 'Not signed in' : current ? 'Could not read the sign-in' : 'Reading…'));
      card.append(copy);
      if (current?.status === 'unavailable') card.append(button('Retry', () => void reload(), 'button quiet row-button', `retry-current-${provider}`));
    }
    grid.append(card);
  }
  section.append(grid); main.append(section);
}
const policyTarget = (target: RotationPolicy['target']) => target === 'managed' ? 'Managed route' : 'Claude Code account';
function renderPolicies(main: HTMLElement) {
  const policies = snapshot!.policies ?? []; const enabled = policies.filter((policy) => policy.enabled);
  const panel = el('section', 'rotation-bar'); panel.setAttribute('aria-label', 'Automatic switching');
  const copy = el('div', 'rotation-copy');
  copy.append(el('strong', '', enabled.length ? 'Automatic switching is on' : 'Automatic switching is off'));
  if (enabled.length) for (const policy of enabled) {
    const decision = monitor?.decisions?.find((entry) => entry.provider === policy.provider && entry.pool === policy.pool && entry.target === policy.target);
    copy.append(el('span', 'usage-caption', `${providerName(policy.provider)} · ${policy.pool} · ${policyTarget(policy.target)} · at ${policy.threshold_percent}% used${decision ? ` · ${decisionText(decision.reason)}` : ''}`));
  } else copy.append(el('span', 'usage-caption', monitor ? 'When the account in use nears its limit, Switchboard can move to the saved account with the most quota left.' : 'Quota monitor status unavailable. Refresh to retry.'));
  const swapHeld = monitor?.claude_swap_accounts ?? 0;
  if (swapHeld) copy.append(el('span', 'usage-caption', `Claude Swap is running and renews ${swapHeld} of these ${swapHeld === 1 ? 'account' : 'accounts'}. Switchboard takes their newest sign-ins from it and does not renew them itself.`));
  if (monitor?.renewal_blocked) copy.append(el('span', 'usage-caption', 'Claude is refusing sign-in renewals for every account right now. Saved accounts keep their last sign-in and are not renewed; Switchboard tries again within the hour. If this stays, update Switchboard.'));
  if (demo) copy.append(el('span', 'usage-caption', 'Demo policies are editable fixtures; no switching runs here.'));
  const actions = el('div', 'rotation-actions');
  const pool = autoSwitchPool(snapshot!.accounts, policies, (snapshot!.projects ?? []).map((p) => p.pool));
  if (pool) actions.append(button('Turn on for Claude Code', () => { const saved = policies.find((policy) => policy.provider === 'claude' && policy.pool === pool && policy.target === 'claude_cli'); void mutate(() => adapter.setPolicy({ provider: 'claude', pool, target: 'claude_cli', enabled: true, threshold_percent: saved?.threshold_percent ?? 90, hysteresis_percent: saved?.hysteresis_percent ?? 10, cooldown_seconds: saved?.cooldown_seconds ?? 1800, max_age_seconds: saved?.max_age_seconds ?? 300, last_switched_at: saved?.last_switched_at ?? null }), `Automatic switching is on for Claude Code in ${pool}. It moves at 90% used to an account with at least 10 points more headroom.`, 'menu-policies'); }, 'button primary row-button', 'auto-on'));
  for (const policy of enabled) actions.append(button(enabled.length > 1 ? `Stop ${policy.pool}` : 'Stop', () => void mutate(() => adapter.setPolicy({ ...policy, enabled: false }), 'Automatic switching stopped for this provider, pool and target.', 'policies'), 'button quiet row-button', `stop-${policy.provider}-${policy.pool}-${policy.target}`));
  actions.append(menu('policies', 'Automatic switching settings', [...policies.map((policy): [string, () => void] => [`Edit ${providerName(policy.provider)} · ${policy.pool} · ${policyTarget(policy.target)}…`, () => policyDialog(policy)]), ['New policy…', () => policyDialog()]], 'icon-button', '⚙'));
  panel.append(copy, actions); main.append(panel);
}
function renderActivity(main: HTMLElement) {
  if (!snapshot!.events.length) { main.append(emptyState('No activity yet', 'Adding accounts, changing routes, and launching sessions will appear here. Credentials and prompts are never part of this journal.')); return; }
  const list = el('ol', 'event-list');
  for (const event of [...snapshot!.events].reverse()) {
    const row = el('li', 'event-row'); const title = el('div', 'event-title');
    const account = snapshot!.accounts.find((item) => item.id === event.account_id);
    title.append(el('strong', '', event.action.replaceAll('.', ' · ').replaceAll('_', ' ')), el('time', '', date(event.at)));
    row.append(title, el('p', '', event.detail));
    if (event.account_id) row.append(el('span', 'event-id', `${account ? `${account.label} · ` : ''}${event.account_id}`)); list.append(row);
  }
  main.append(list, el('p', 'surface-note', 'A bounded local journal. A terminal launch is not proof of a successful provider request.'));
}
function renderAbout(main: HTMLElement) {
  const section = el('section', 'about-panel');
  section.append(el('h2', '', 'A local account workbench'), el('p', '', 'Switchboard stores credentials in the native vault and manages private homes for Claude Code and Codex CLI. Account labels and imported identities are user claims until a provider confirms them.'));
  const definitions = el('dl', 'definitions');
  for (const [term, description] of [
    ['Isolated launch', 'A new official CLI session using this account’s private home. Account changes affect new launches.'],
    ['Managed launch', 'A session routed through the local HTTP/SSE proxy. Select an account for each provider and pool; the next request uses that selection. A response already in progress keeps its identity.'],
    ['Pools', 'Explicit routing boundaries, such as work and personal. Selection never silently falls back to a different pool.'],
    ['Usage', 'A timestamped provider observation. Unknown, stale, and unavailable are distinct from measured zero. API billing is separate from subscription quota.'],
    ['Build', demo ? 'Synthetic browser demo; no native platform is connected.' : `${platformLabel(runtime?.platform)} build. Native platform is reported by the running app.`],
    ['Compatibility', 'macOS and Windows builds. Live provider acceptance is a separate check on each platform. Existing external CLI sessions and Codex Desktop are not controlled.'],
    ['Imported OAuth', 'A captured credential snapshot. Capture the current CLI account again after reauthentication. Switchboard does not run competing OAuth refresh grants.'],
    ['Native Claude activation', 'An explicit update of the local Claude Code account. CLI reload timing is not a guarantee that a running session has changed account.'],
    ['Automatic rotation', 'Off by default for each provider, pool and target. Uses fresh quota observations, a threshold, a minimum improvement and a cooldown. No eligible account means the current account stays selected.'],
  ]) { definitions.append(el('dt', '', term), el('dd', '', description)); }
  section.append(definitions); const tour = el('section', 'about-panel'); tour.append(el('h2', '', 'Tour'), el('p', '', 'Five short steps: what Switchboard does and where to press.'), button('Show the tour again', () => tourGo(0), 'button', 'tour-again'));
  main.append(section, tour, residencyPanel(), backupsPanel(), analyticsPanel(), appearancePanel(), productPanel());
}
async function loadBackups() {
  try { backupStatus = await adapter.backups(); backupError = false; } catch { backupError = true; }
  backgroundRender();
}
/** SB-28: closing the window keeps Switchboard working; it opens at login in the background. */
function residencyPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'residency-heading');
  const heading = el('h2', '', 'Running in the background'); heading.id = 'residency-heading';
  panel.append(heading, el('p', '', 'Closing the window keeps Switchboard running, so quota checks, automatic switching, sign-in renewal and backups continue. To stop it, choose Quit Switchboard from its menu-bar icon or the app menu.'));
  if (loginItemError) { panel.append(el('p', 'form-note', 'The login setting is unavailable. Refresh to retry.')); return panel; }
  if (!loginItem) { panel.append(el('p', 'form-note', 'Reading the login setting…')); return panel; }
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = loginItem.enabled; box.disabled = !loginItem.available; box.dataset.focus = 'login-item';
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { loginItem = await adapter.setLoginItem(wanted); }, wanted ? 'Switchboard will open at login, in the background.' : 'Switchboard will no longer open at login.', 'login-item'); });
  option.append(box, el('span', '', 'Open at login, in the background'));
  panel.append(option, el('p', 'form-note', loginItem.available ? 'Starts without a window; open it from the menu-bar icon.' : 'Available in the installed app.'));
  return panel;
}
/** Anonymous usage analytics (docs/ANALYTICS.md): what is sent, and the switch every PassionCode app shares. */
function analyticsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'analytics-heading');
  const heading = el('h2', '', 'Usage analytics'); heading.id = 'analytics-heading';
  panel.append(heading, el('p', '', 'Switchboard counts installs, days of use and how many accounts are connected, by provider and type, to help PassionCode improve its apps. It never sends account names, e-mail addresses, sign-ins, pool names or what you do with your accounts. A random installation number, shared by the PassionCode apps on this computer, lets one person using several of them count once.'));
  if (analyticsError) { panel.append(el('p', 'form-note', 'The analytics setting is unavailable. Refresh to retry.')); return panel; }
  if (!analyticsState) { panel.append(el('p', 'form-note', 'Reading the analytics setting…')); return panel; }
  const option = el('label', 'appearance-option login-option'); const box = el('input'); box.type = 'checkbox'; box.checked = analyticsState.enabled; box.disabled = !analyticsState.available; box.dataset.focus = 'analytics';
  box.addEventListener('change', () => { const wanted = box.checked; void mutate(async () => { analyticsState = await adapter.setAnalytics(wanted); }, wanted ? 'Anonymous usage analytics are on.' : 'Anonymous usage analytics are off for every PassionCode app on this computer.', 'analytics'); });
  option.append(box, el('span', '', 'Share anonymous usage counts'));
  panel.append(option, el('p', 'form-note', analyticsState.available ? 'This switch applies to every PassionCode app on this computer.' : 'Only installed release builds send analytics; this build sends nothing.'));
  return panel;
}
function backupsPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'backups-heading');
  const heading = el('h2', '', 'Backups'); heading.id = 'backups-heading'; panel.append(heading);
  if (backupError || !backupStatus) { panel.append(el('p', 'form-note', backupError ? 'Backup status is unavailable. Refresh to retry.' : 'Reading backups…')); return panel; }
  const status = backupStatus;
  panel.append(el('p', '', status.enabled ? 'Switchboard saves an encrypted copy of your accounts after every change and once a day, and keeps the newest ten.' : 'Automatic backups run in the desktop app.'));
  if (status.directory) panel.append(el('code', 'address', status.directory));
  panel.append(el('p', 'form-note', 'The key stays in this Mac’s Keychain. These backups restore after reinstalling Switchboard on this Mac; they cannot be opened on another Mac or after the Keychain is erased.'));
  if (status.last_error) panel.append(el('p', 'usage-error', `The last automatic backup failed: ${safeError(status.last_error)}`));
  const list = el('div', 'backup-list');
  if (!status.backups.length) list.append(el('p', 'form-note', 'No backups yet.'));
  for (const backup of status.backups.slice(0, 5)) {
    const row = el('div', 'policy-row'); const copy = el('div');
    copy.append(el('strong', 'block', date(backup.created_at)), el('span', 'usage-caption', `${backup.accounts} ${backup.accounts === 1 ? 'account' : 'accounts'}`));
    row.append(copy, button('Restore', () => void mutate(async () => { const result = await adapter.restoreBackup(backup.file); backupStatus = await adapter.backups(); showRestored(result); }, 'Backup restored.', `restore-${backup.file}`), 'text-button', `restore-${backup.file}`));
    list.append(row);
  }
  panel.append(list, button('Back up now', () => void mutate(async () => { await adapter.backupNow(); backupStatus = await adapter.backups(); }, 'Backup saved.', 'backup-now'), 'button', 'backup-now'));
  return panel;
}
let restoredText = '';
function showRestored(result: { added: number; skipped: number; failed: number }) {
  restoredText = `${result.added} ${result.added === 1 ? 'account' : 'accounts'} restored · ${result.skipped} already here${result.failed ? ` · ${result.failed} could not be restored` : ''}.`;
}
function appearancePanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'appearance-heading');
  const heading = el('h2', '', 'Appearance'); heading.id = 'appearance-heading';
  const group = el('fieldset', 'appearance-options'); group.append(el('legend', 'sr-only', 'Appearance'));
  for (const [value, text] of [['system', 'System'], ['dark', 'Dark'], ['light', 'Light']] as const) {
    const option = el('label', 'appearance-option'); const radio = el('input'); radio.type = 'radio'; radio.name = 'appearance'; radio.value = value; radio.checked = appearance === value; radio.dataset.focus = `appearance-${value}`;
    radio.addEventListener('change', () => { if (radio.checked) { setAppearance(value); render(); restoreFocus(`appearance-${value}`); announce(appearanceSaveFailed ? `${text} appearance applied. It could not be saved and resets when Switchboard restarts.` : `${text} appearance applied.`); } });
    option.append(radio, el('span', '', text)); group.append(option);
  }
  panel.append(heading, group, el('p', 'form-note', appearanceSaveFailed ? 'This choice could not be saved on this machine. It applies until Switchboard restarts.' : 'System follows the light or dark setting of your operating system. The choice is saved on this machine.'));
  return panel;
}
/** Tauri has no URL opener here, so native builds show a selectable address; the browser demo links it. */
function address(url: string) {
  if (native) { const text = el('code', 'address', url); return text; }
  const link = el('a', 'address', url); link.href = url; link.target = '_blank'; link.rel = 'noopener noreferrer'; link.dataset.focus = `link-${url}`; return link;
}
function productPanel() {
  const panel = el('section', 'about-panel'); panel.setAttribute('aria-labelledby', 'product-heading');
  const heading = el('h2', '', 'Version and license'); heading.id = 'product-heading';
  const definitions = el('dl', 'definitions');
  const row = (term: string, ...content: (Node | string)[]) => { const dd = el('dd'); dd.append(...content); definitions.append(el('dt', '', term), dd); };
  row('Version', `Fabric Switchboard ${version}`);
  row('License', el('span', 'block', 'Open source under the GNU AGPL-3.0; a commercial license is available — contact@passioncode.ai.'), address(`${REPOSITORY}/blob/main/LICENSE`));
  row('Third-party', el('span', 'block', 'The app includes third-party components under their own licenses.'), address(`${REPOSITORY}/blob/main/THIRD_PARTY_NOTICES.md`));
  row('Toolkit', el('span', 'block', 'Part of the PassionCode.ai toolkit.'), address('https://passioncode.ai/switchboard/'));
  panel.append(heading, definitions);
  if (native) panel.append(el('p', 'form-note', 'Addresses are selectable text. Copy one into your browser to open it.'));
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
    catch (failure) { error.textContent = safeError(failure); setBusy(false); error.tabIndex = -1; error.focus(); }
  };
  const cancel = button('Cancel', () => void requestCancel(), 'button'); actions.append(cancel);
  form.append(header, body, error, actions); dialog.append(form); document.body.append(dialog);
  dialog.addEventListener('cancel', (event) => { event.preventDefault(); void requestCancel(); });
  dialog.addEventListener('close', () => { form.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input,textarea').forEach((input) => { input.value = ''; }); dialog.remove(); if (!document.querySelector('dialog[open]')) restoreFocus(trigger); });
  dialog.showModal();
  const setBusy = (value: boolean) => { pending = value; form.setAttribute('aria-busy', String(value)); form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement | HTMLTextAreaElement>('input, button, select, textarea').forEach((node) => { node.disabled = value || node.dataset.locked === 'true'; }); };
  return { dialog, form, body, actions, error, trigger, close, setBusy, beforeCancel: (handler) => { cancelHandler = handler; } };
}
function field(label: string, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement, help = '') {
  const wrapper = el('label', 'field'); wrapper.append(el('span', 'field-label', label), input); if (help) wrapper.append(el('span', 'field-help', help)); return wrapper;
}
function input(value = '', type = 'text') { const node = el('input'); node.type = type; node.value = value; node.required = true; node.autocomplete = 'off'; node.spellcheck = false; return node; }
function select(options: [string, string][]) { const node = el('select'); options.forEach(([value, text]) => { const option = el('option', '', text); option.value = value; node.append(option); }); return node; }
function submit(text: string) { const node = el('button', 'button primary', text); node.type = 'submit'; return node; }
async function dialogSave(context: DialogContext, action: () => Promise<unknown>, success: string) {
  context.error.textContent = ''; context.setBusy(true); clock.begin();
  try { await action(); await refreshContext(); showNotice(success); try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'The action completed, but the list could not be refreshed. Retry loading accounts.'; } render(); context.setBusy(false); context.close(); }
  catch (error) { context.error.textContent = safeError(error); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); }
  finally { clock.end(); }
}
function launchDialog(account: Account, mode: 'isolated' | 'managed') {
  const context = openDialog(`Launch ${mode}`, `${account.label} · ${providerName(account.provider)} · ${account.pool} pool`);
  const directory = input(lastWorkingDirectory); directory.placeholder = projectPathExample(runtime?.platform);
  context.body.append(field('Project directory', directory, 'Enter an absolute path to an existing folder. The native app checks that the folder exists before launching. Account credentials stay in their separate managed home.'));
  context.body.append(el('p', 'form-note', demo ? 'Synthetic launch only. The demo validates an absolute path but does not inspect your filesystem or open a terminal.' : mode === 'managed' ? 'New requests will use the selected account in this pool. In-progress responses keep their account.' : 'A new official CLI session will use this account’s private home. Existing clients are not changed.'));
  context.actions.append(submit('Launch'));
  directory.addEventListener('input', () => directory.setCustomValidity(''));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    const workingDirectory = directory.value.trim();
    if (!isAbsoluteProjectPath(workingDirectory, runtime?.platform)) { directory.setCustomValidity(`Enter an absolute project directory, for example ${projectPathExample(runtime?.platform)}.`); directory.reportValidity(); return; }
    let tools = true;
    const noTools = ' Agents in this session have no Switchboard tools: link the command-line tool under Agents, then launch again.';
    void dialogSave(context, async () => { const result = await adapter.launch(account.id, mode, workingDirectory); tools = result.agent_tools !== false; lastWorkingDirectory = workingDirectory; }, demo ? `Synthetic ${mode} launch recorded. No terminal was opened.` : mode === 'managed' ? 'Terminal launch requested in your project through the local proxy. Selection takes effect on the next request.' : 'Terminal launch requested in your project with this account’s private home. Provider acceptance is not yet observed.').then(() => { if (!tools && !noticeError) { showNotice(notice + noTools); render(); } });
  });
  directory.focus();
}
function editDialog(account: Account) {
  const context = openDialog('Edit account', `${providerName(account.provider)} · ${account.pool} pool. To change credentials or pool, add a new account.`);
  const label = input(account.label); label.maxLength = 80; const enabled = input('', 'checkbox'); enabled.required = false; enabled.checked = account.enabled;
  const enabledLabel = el('label', 'checkbox-field'); enabledLabel.append(enabled, el('span', '', 'Enabled for selection and launch'));
  context.body.append(field('Account label', label), enabledLabel, el('p', 'form-note', 'Disabling a selected account clears its route. Managed requests in that pool will fail until you select another account.'));
  context.actions.append(submit('Save changes'));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); if (!label.value.trim()) { label.setCustomValidity('Enter an account label.'); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; } void dialogSave(context, () => adapter.update(account.id, label.value.trim(), enabled.checked), enabled.checked ? 'Account updated.' : 'Account disabled. Any selection for this account has been cleared.'); }); label.focus();
}
function removeDialog(account: Account) {
  const active = selected(account);
  const context = openDialog('Remove account?', `Remove “${account.label}” from Switchboard and delete its stored credential. This cannot be undone.`);
  context.body.append(el('p', 'form-note', active ? 'This account is selected. Select another account in the same pool, or disable this one in Edit, before removing it.' : 'Existing external clients are not signed out. Managed account metadata and its vault entry will be removed.'));
  const confirm = submit('Remove account'); confirm.className = 'button danger'; confirm.disabled = active; context.actions.append(confirm);
  context.form.addEventListener('submit', (event) => { event.preventDefault(); if (!active) void dialogSave(context, () => adapter.remove(account.id), 'Account removed.'); });
  context.actions.querySelector('button')?.focus();
}

function decisionText(reason: string) {
  const labels: Record<string, string> = {
    disabled: 'Rotation is off.', cooldown: 'Waiting for the cooldown to end.', below_threshold: 'Current usage is below the threshold.',
    no_eligible_account: 'No eligible account. Holding the current account.',
    stale_usage: 'Waiting for fresh usage. Holding the current account.', usage_unavailable: 'Usage unavailable. Holding the current account.',
    current_unavailable: 'Current account unavailable. Holding the current account.', limit_reached: 'The account in use hit a provider limit. An unlimited account is available; a switch is not yet confirmed.', limit_no_eligible_account: 'The account in use hit a provider limit, and no other account is free. Holding it.', switched_on_limit: 'Switched after the account in use hit a provider limit.', threshold_reached: 'Threshold reached. An eligible account is available; a switch is not yet confirmed.', switched: 'The monitor recorded a switch.', switch_failed: 'The switch failed. The current account stays selected; check Activity.', activation_failed: 'Native activation failed. Check the current CLI identity and retry manually.', claude_swap_switching: 'Claude Swap is switching Claude Code automatically, so Switchboard does not. Turn off automatic switching in one of them; manual switches still work.',
  };
  return labels[reason] || 'The monitor has evaluated this policy. Check Activity for recorded changes.';
}
function policyDialog(existing?: RotationPolicy) {
  const context = openDialog('Automatic rotation', 'Configure one provider, pool and target. Fresh eligible accounts stay within this boundary; no eligible account means hold.');
  const provider = select([['claude', 'Claude Code'], ['codex', 'Codex CLI']]);
  const pool = input(existing?.pool || 'default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+';
  const target = select([['managed', 'Managed route'], ['claude_cli', 'Claude Code account']]);
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
  const grid = el('div', 'form-grid'); grid.append(field('Provider', provider), field('Pool', pool), field('Target', target), field('Switch at usage (%)', threshold), field('Headroom below threshold (points)', hysteresis, 'The candidate must stay this many points below the switching threshold.'), field('Cooldown (seconds)', cooldown), field('Maximum usage age (seconds)', freshness));
  const toggle = el('label', 'checkbox-field'); toggle.append(enabled, el('span', '', 'Enable automatic rotation'));
  context.body.append(grid, toggle, el('p', 'form-note', 'Claude Code rotation requires OAuth accounts with an external identity. Managed rotation affects the next request. A response already in progress keeps its account.'));
  context.actions.append(submit('Save policy'));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    hysteresis.setCustomValidity(Number(hysteresis.value) >= Number(threshold.value) ? 'Headroom must be less than the usage threshold.' : '');
    if (!context.form.reportValidity()) return;
    const previous = snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value.trim() && policy.target === target.value);
    const policy: RotationPolicy = { provider: provider.value as Provider, pool: pool.value.trim(), target: target.value as RotationPolicy['target'], enabled: enabled.checked, threshold_percent: Number(threshold.value), hysteresis_percent: Number(hysteresis.value), cooldown_seconds: Number(cooldown.value), max_age_seconds: Number(freshness.value), last_switched_at: previous?.last_switched_at ?? null };
    void dialogSave(context, () => adapter.setPolicy(policy), `Rotation policy saved${policy.enabled ? ' and enabled' : '; rotation is off'}.`);
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
  }).catch(() => { /* Explicit Refresh reports a failed metadata read. */ }).finally(() => { refreshing = false; });
}
setInterval(backgroundRefresh, 60_000);
// A window shown after hours in the background (SB-30) reads fresh metadata at once rather
// than at the next minute.
document.addEventListener('visibilitychange', () => { if (!document.hidden) backgroundRefresh(); });
