import './tokens.css';
import './style.css';
import { isAbsoluteProjectPath, platformLabel, projectPathExample } from './platform';
import { demo, native, nativeAdapter, safeError } from './adapter';
import type { Account, Adapter, AuthKind, CurrentAccounts, ExternalIdentity, MonitorStatus, Provider, RotationPolicy, RuntimeStatus, Snapshot } from './types';

const root = document.querySelector<HTMLDivElement>('#app')!;
const announcements = document.querySelector<HTMLDivElement>('#announcements')!;
const theme = matchMedia('(prefers-color-scheme: dark)');
const setTheme = () => { document.documentElement.dataset.theme = theme.matches ? 'dark' : 'light'; };
theme.addEventListener('change', setTheme); setTheme();
let adapter: Adapter = nativeAdapter;
let snapshot: Snapshot | null = null;
let runtime: RuntimeStatus | null = null;
let currentAccounts: CurrentAccounts | null = null;
let monitor: MonitorStatus | null = null;
let page: 'accounts' | 'activity' | 'about' = 'accounts';
let filter: 'all' | Provider = 'all';
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
const age = (seconds: number) => { const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60)); return minutes < 1 ? 'just now' : minutes < 60 ? `${minutes}m ago` : `${Math.floor(minutes / 60)}h ago`; };
const identityText = (identity?: ExternalIdentity | null) => identity?.email || identity?.account_id || 'Identity not reported';
const currentMatch = (account: Account) => {
  const current = currentAccounts?.[account.provider];
  if (!current || current.status !== 'available') return false;
  if (current.account_id === account.id) return true;
  const identity = account.external_identity;
  return !!identity?.account_id && identity.account_id === current.identity?.account_id && identity.organization_id === current.identity?.organization_id;
};
async function refreshContext() {
  const [current, status] = await Promise.allSettled([adapter.currentAccounts(), adapter.monitorStatus()]);
  currentAccounts = current.status === 'fulfilled' ? current.value : null;
  monitor = status.status === 'fulfilled' ? status.value : null;
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
  const [data, status] = await Promise.allSettled([adapter.snapshot(), adapter.runtime(), refreshContext()]);
  if (data.status === 'fulfilled') snapshot = data.value;
  else { loadError = safeError(data.reason); announce(loadError); }
  if (status.status === 'fulfilled') { runtime = status.value; runtimeError = false; }
  else { runtime = null; runtimeError = true; }
  loading = false; render();
}
async function mutate(action: () => Promise<unknown>, success: string, focusKey?: string, accountId?: string) {
  busy = true; notice = ''; render();
  try {
    await action();
    await refreshContext();
    if (accountId) usageErrors.delete(accountId);
    showNotice(success);
    try { snapshot = await adapter.snapshot(); loadError = ''; }
    catch { loadError = 'The action completed, but accounts could not be refreshed. Retry loading the account list.'; }
  } catch (error) { if (accountId) { try { snapshot = await adapter.snapshot(); } catch { /* Keep last snapshot. */ } } const text = safeError(error); if (accountId) usageErrors.set(accountId, text); showNotice(text, true); }
  finally { busy = false; render(); restoreFocus(focusKey); }
}

function render() {
  const openDetails = new Set([...root.querySelectorAll<HTMLDetailsElement>('details[open]')].map((details) => details.querySelector<HTMLElement>('summary')?.dataset.focus));
  root.replaceChildren();
  const shell = el('div', 'shell');
  const sidebar = el('aside', 'sidebar');
  const brand = el('div', 'brand'); brand.append(el('div', 'brand-mark', 'F'), el('div', '', 'FABRIC'));
  sidebar.append(brand, el('p', 'product-name', 'Switchboard'));
  const nav = el('nav', 'navigation'); nav.setAttribute('aria-label', 'Main navigation');
  for (const [target, label, icon] of [['accounts', 'Accounts', '▦'], ['activity', 'Activity', '≋'], ['about', 'About', '○']] as const) {
    const control = button('', () => { page = target; notice = ''; render(); document.querySelector<HTMLElement>('h1')?.focus(); }, `nav-item ${page === target ? 'active' : ''}`);
    const symbol = el('span', 'nav-icon', icon); symbol.setAttribute('aria-hidden', 'true');
    control.append(symbol, el('span', '', label)); if (page === target) control.setAttribute('aria-current', 'page'); nav.append(control);
  }
  sidebar.append(nav);
  const sidebarFoot = el('div', 'sidebar-foot');
  sidebarFoot.append(el('span', 'eyebrow', 'LOCAL WORKBENCH'), el('p', '', demo ? 'Synthetic session' : 'Your accounts. Your machine.'), el('span', 'version', `v0.3.0 · ${demo ? 'Browser demo' : platformLabel(runtime?.platform)}`));
  sidebar.append(sidebarFoot); shell.append(sidebar);
  const main = el('main', 'main'); main.id = 'main'; main.setAttribute('aria-busy', String(busy || loading));
  if (demo) main.append(el('div', 'demo-banner', 'SYNTHETIC DEMO · No real accounts, vault, proxy, or terminal. Changes reset when you reload.'));
  const header = el('header', 'page-header');
  const heading = el('div'); const title = el('h1', '', { accounts: 'Accounts', activity: 'Activity', about: 'About Switchboard' }[page]); title.tabIndex = -1;
  heading.append(el('p', 'eyebrow', 'FABRIC SWITCHBOARD'), title, el('p', 'subtitle', { accounts: 'Choose who handles the next request.', activity: 'Local account and session events.', about: 'Deliberate account switching for coding sessions.' }[page]));
  header.append(heading);
  if (native || demo) {
    const actions = el('div', 'header-actions');
    actions.append(button(loading ? 'Loading…' : 'Refresh', () => void reload(), 'button quiet', 'refresh'));
    if (page === 'accounts') actions.append(button('Import Claude Swap', () => importDialog(), 'button', 'import-swap'), button('+ Add account', () => addDialog(), 'button primary', 'add-account'));
    header.append(actions);
  }
  main.append(header);
  if (busy) { const progress = el('p', 'operation-progress', 'Working…'); progress.setAttribute('role', 'status'); main.append(progress); }
  if (!native && !demo) {
    main.append(emptyState('Open the native app', 'Account storage and session launches are available in the Fabric Switchboard desktop app. This browser window has no access to your accounts.'));
    shell.append(main); root.append(shell); return;
  }
  if (notice) { const alert = el('div', `notice ${noticeError ? 'error' : 'success'}`); alert.append(el('span', '', notice), button('Dismiss', () => { notice = ''; render(); }, 'text-button')); main.append(alert); }
  if (page === 'about') renderAbout(main);
  else if (loadError) {
    const state = emptyState('Unable to load accounts', loadError); state.classList.add('error-state'); state.append(button('Retry', () => void reload(), 'button primary')); main.append(state);
  } else if (loading && !snapshot) {
    const state = emptyState('Loading your workbench', 'Reading account metadata from the native app…'); state.setAttribute('role', 'status'); main.append(state);
  } else if (snapshot) {
    if (page === 'accounts') renderAccounts(main);
    else renderActivity(main);
  }
  shell.append(main); root.append(shell);
  root.querySelectorAll<HTMLDetailsElement>('details').forEach((details) => { details.open = openDetails.has(details.querySelector<HTMLElement>('summary')?.dataset.focus); });
}
function emptyState(title: string, description: string) { const section = el('section', 'empty-state'); section.append(el('div', 'empty-symbol', '◇'), el('h2', '', title), el('p', '', description)); return section; }
function renderAccounts(main: HTMLElement) {
  const status = el('section', 'runtime'); status.setAttribute('aria-label', 'Runtime status');
  const indicator = el('span', `status-dot ${runtimeError ? 'unavailable' : ''}`); indicator.setAttribute('aria-hidden', 'true');
  const text = el('div', 'runtime-copy');
  text.append(el('strong', '', demo ? 'Demo runtime' : runtimeError ? 'Proxy status unavailable' : 'Local proxy ready'), el('span', '', runtimeError ? 'Refresh to retry. Isolated launch is still available.' : `${runtime?.proxy_address ?? 'Checking…'} · ${demo ? 'No live requests' : 'HTTP / SSE'}`));
  status.append(indicator, text, el('span', 'runtime-caption', 'Selection applies to the next managed request.'));
  main.append(status);
  renderCurrent(main);
  renderPolicies(main);
  const toolbar = el('div', 'toolbar');
  const filters = el('div', 'filters'); filters.setAttribute('role', 'group'); filters.setAttribute('aria-label', 'Filter by provider');
  for (const [value, label] of [['all', 'All providers'], ['claude', 'Claude Code'], ['codex', 'Codex CLI']] as const) {
    const control = button(label, () => { filter = value; render(); restoreFocus(`filter-${value}`); }, `filter ${filter === value ? 'active' : ''}`, `filter-${value}`); control.setAttribute('aria-pressed', String(filter === value)); filters.append(control);
  }
  const accounts = snapshot!.accounts.filter((account) => filter === 'all' || account.provider === filter);
  toolbar.append(filters, el('span', 'count', `${accounts.length} ${accounts.length === 1 ? 'account' : 'accounts'}`)); main.append(toolbar);
  if (!accounts.length) {
    const empty = emptyState(snapshot!.accounts.length ? 'No accounts for this provider' : 'Start with one account', snapshot!.accounts.length ? 'Add an account here, or choose All providers to see your other accounts.' : 'Capture your current CLI account without another sign-in, or add a different account through the official CLI. Choose a pool to keep work and personal sessions separate.');
    empty.append(button('Add account', () => addDialog(), 'button primary', 'empty-add')); main.append(empty);
  } else {
    const list = el('div', 'account-list'); list.setAttribute('aria-label', 'Accounts');
    accounts.forEach((account) => list.append(accountCard(account))); main.append(list);
  }
  const footer = el('p', 'surface-note', 'Managed sessions follow your selection within the same provider and pool. In-progress responses keep their account. Native Claude activation is a separate action; session reload behavior depends on the CLI.'); main.append(footer);
}
function accountCard(account: Account) {
  const active = selected(account);
  const card = el('article', `account-card ${active ? 'is-selected' : ''} ${!account.enabled ? 'is-disabled' : ''} ${currentMatch(account) ? 'is-current' : ''}`);
  card.setAttribute('aria-label', `${account.label}, ${providerName(account.provider)}, ${account.pool} pool`);
  const identity = el('div', 'account-identity');
  const icon = el('span', `provider-icon ${account.provider}`, account.provider === 'claude' ? '✳' : '◎'); icon.setAttribute('aria-hidden', 'true');
  const details = el('div', 'account-details'); const title = el('div', 'account-title');
  title.append(el('h2', '', account.label)); if (currentMatch(account)) title.append(el('span', 'badge current-badge', 'Current CLI account')); if (!account.enabled) title.append(el('span', 'badge muted', 'Disabled'));
  details.append(title, el('p', 'account-meta', `${providerName(account.provider)} · ${kindName(account.kind)} · ${account.pool}`));
  if (account.external_identity) details.append(el('p', 'account-meta', identityText(account.external_identity)));
  identity.append(icon, details); card.append(identity);
  card.append(usagePanel(account));
  const route = el('div', 'route-control');
  if (active) { const badge = el('span', 'selected-label', '✓ Selected for next request'); badge.tabIndex = -1; badge.dataset.focus = `select-${account.id}`; route.append(badge); }
  else { const select = button('Select', () => void mutate(() => adapter.select(account), `${account.label} selected for the next managed request in ${account.pool}.`, `select-${account.id}`), 'button select-button', `select-${account.id}`); select.disabled ||= !account.enabled; route.append(select); }
  card.append(route);
  const actions = el('div', 'account-actions');
  const launches = el('div', 'launch-actions');
  const isolated = button('Launch isolated', () => launchDialog(account, 'isolated'), 'text-button', `isolated-${account.id}`); isolated.disabled ||= !account.enabled;
  const managed = button('Launch managed', () => launchDialog(account, 'managed'), 'text-button', `managed-${account.id}`); managed.disabled ||= !account.enabled || !active || runtimeError; managed.title = !active ? 'Select this account before launching a managed session.' : 'Launch through the local proxy.';
  launches.append(isolated, managed);
  if (account.provider === 'claude' && account.kind === 'oauth' && account.external_identity) {
    const activate = button('Activate in Claude Code', () => activateDialog(account), 'text-button', `activate-${account.id}`);
    activate.disabled ||= !account.enabled; launches.append(activate);
  }
  const management = el('div', 'management-actions');
  const probe = button('Check usage', () => void mutate(() => adapter.probe(account.id), 'Usage observation updated.', `usage-${account.id}`, account.id), 'text-button', `usage-${account.id}`); probe.disabled ||= !account.enabled;
  management.append(probe, button('Edit', () => editDialog(account), 'text-button', `edit-${account.id}`), button('Remove', () => removeDialog(account), 'text-button danger-text', `remove-${account.id}`));
  actions.append(launches, management); card.append(actions); return card;
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
  section.append(definitions); main.append(section);
}

interface DialogContext { dialog: HTMLDialogElement; form: HTMLFormElement; body: HTMLElement; actions: HTMLElement; error: HTMLElement; close: () => void; setBusy: (value: boolean) => void; beforeCancel: (handler: () => Promise<void>) => void }
function openDialog(title: string, intro: string): DialogContext {
  const trigger = (document.activeElement as HTMLElement | null)?.dataset.focus;
  const dialog = el('dialog', 'dialog'); dialog.setAttribute('aria-labelledby', 'dialog-title'); dialog.setAttribute('aria-describedby', 'dialog-description');
  const form = el('form'); const header = el('div', 'dialog-header');
  const heading = el('h2', '', title); heading.id = 'dialog-title'; const description = el('p', '', intro); description.id = 'dialog-description'; header.append(heading, description);
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
  dialog.addEventListener('close', () => { form.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input,textarea').forEach((input) => { input.value = ''; }); dialog.remove(); restoreFocus(trigger); });
  dialog.showModal();
  const setBusy = (value: boolean) => { pending = value; form.setAttribute('aria-busy', String(value)); form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement | HTMLTextAreaElement>('input, button, select, textarea').forEach((node) => { node.disabled = value || node.dataset.locked === 'true'; }); };
  return { dialog, form, body, actions, error, close, setBusy, beforeCancel: (handler) => { cancelHandler = handler; } };
}
function field(label: string, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement, help = '') {
  const wrapper = el('label', 'field'); wrapper.append(el('span', 'field-label', label), input); if (help) wrapper.append(el('span', 'field-help', help)); return wrapper;
}
function input(value = '', type = 'text') { const node = el('input'); node.type = type; node.value = value; node.required = true; node.autocomplete = 'off'; node.spellcheck = false; return node; }
function select(options: [string, string][]) { const node = el('select'); options.forEach(([value, text]) => { const option = el('option', '', text); option.value = value; node.append(option); }); return node; }
function submit(text: string) { const node = el('button', 'button primary', text); node.type = 'submit'; return node; }
async function dialogSave(context: DialogContext, action: () => Promise<unknown>, success: string) {
  context.error.textContent = ''; context.setBusy(true);
  try { await action(); await refreshContext(); showNotice(success); try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'The action completed, but the list could not be refreshed. Retry loading accounts.'; } render(); context.setBusy(false); context.close(); }
  catch (error) { context.error.textContent = safeError(error); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); }
}
function addDialog() {
  const context = openDialog('Add account', 'Capture an account already signed in to the CLI, or sign in with another account.');
  const provider = select([['claude', 'Claude Code'], ['codex', 'Codex CLI']]);
  const method = select([['capture', 'Capture current CLI account'], ['login', 'Official sign-in with another account'], ['api_key', 'API key'], ['setup_token', 'Claude setup token'], ['oauth', 'Import OAuth JSON']]);
  const label = input(); label.maxLength = 80; label.placeholder = 'e.g. Studio';
  const pool = input('default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+'; pool.title = 'Use lowercase letters, numbers, hyphens, or underscores.';
  const grid = el('div', 'form-grid'); grid.append(field('Provider', provider), field('Authentication', method), field('Account label', label), field('Pool', pool, 'A boundary for routing, such as work or personal.'));
  const credentialSlot = el('div'); const submitButton = submit('Capture current account'); context.actions.append(submitButton); context.body.append(grid, credentialSlot);
  let secret: HTMLInputElement | HTMLTextAreaElement | null = null;
  let loginId: string | null = null;
  context.beforeCancel(async () => {
    if (loginId) await adapter.cancelLogin(loginId);
  });
  const update = () => {
    if (secret) secret.value = ''; credentialSlot.replaceChildren(); secret = null;
    const setup = method.querySelector<HTMLOptionElement>('option[value="setup_token"]')!; setup.disabled = provider.value !== 'claude';
    if (setup.disabled && method.value === 'setup_token') method.value = 'api_key';
    label.required = method.value !== 'capture';
    if (method.value === 'capture') {
      const current = currentAccounts?.[provider.value as Provider];
      credentialSlot.append(el('p', 'form-note', current?.status === 'available' ? `Current CLI account: ${identityText(current.identity)}. A stored copy is added to this pool. Leave the label empty to use the source identity.` : 'Capture reads the CLI account on this machine. If none is signed in, choose official sign-in with another account.'));
      submitButton.textContent = 'Capture current account';
    } else if (method.value === 'login') {
      credentialSlot.append(el('p', 'form-note', demo ? 'Demo sign-in is synthetic. No provider or terminal will open.' : 'Begin sign-in opens the official CLI in a private home. Complete the provider’s flow, then return here to finish.'));
      submitButton.textContent = 'Begin sign-in';
    } else {
      if (method.value === 'oauth') { const textarea = el('textarea', 'secret-json'); textarea.rows = 5; textarea.required = true; textarea.spellcheck = false; textarea.autocomplete = 'off'; textarea.placeholder = 'Paste credential JSON'; secret = textarea; }
      else { secret = input('', 'password'); secret.autocomplete = 'new-password'; secret.placeholder = 'Enter credential'; }
      credentialSlot.append(field(method.value === 'oauth' ? 'OAuth credential JSON' : 'Credential', secret, demo ? 'Use synthetic input only. Nothing is stored after reload.' : 'Sent only to native credential storage. Cleared on submit or cancel.'));
      if (method.value === 'oauth') credentialSlot.append(el('p', 'form-note', 'Imported OAuth is a snapshot. Sign in again when it expires. Imported identity is not independently verified.'));
      submitButton.textContent = 'Add account';
    }
  };
  provider.addEventListener('change', update); method.addEventListener('change', update); update();
  context.form.addEventListener('submit', (event) => {
    event.preventDefault(); if (!context.form.reportValidity()) return;
    const base = { provider: provider.value as Provider, label: label.value.trim(), pool: pool.value.trim() };
    if (method.value === 'capture') { void dialogSave(context, () => adapter.captureCurrent({ provider: base.provider, pool: base.pool, ...(base.label ? { label: base.label } : {}) }), 'Current CLI account captured. Existing records in this pool are updated without enabling disabled accounts.'); return; }
    if (!base.label) { label.setCustomValidity('Enter an account label.'); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; }
    if (loginId) { void dialogSave(context, () => adapter.finishLogin(loginId!), demo ? 'Synthetic sign-in account added.' : 'Account captured from the private sign-in home. Select it when you are ready.').finally(() => { grid.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select').forEach((node) => { node.disabled = true; }); }); return; }
    if (method.value !== 'login') {
      const value = secret!.value; secret!.value = '';
      void dialogSave(context, () => adapter.add({ ...base, kind: method.value as AuthKind, secret: value }), 'Account added. Select it when you are ready.'); return;
    }
    context.error.textContent = ''; context.setBusy(true);
    void adapter.beginLogin(base).then((result) => {
      loginId = result.login_id; context.setBusy(false);
      grid.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select').forEach((node) => { node.disabled = true; node.dataset.locked = 'true'; });
      credentialSlot.replaceChildren(el('div', 'login-pending', demo ? 'Synthetic sign-in ready. Choose Finish sign-in to add this demo account.' : 'Complete sign-in in the terminal, then choose Finish sign-in. If the provider opens a browser, finish that step first.'));
      submitButton.textContent = 'Finish sign-in'; submitButton.focus();
    }).catch((error) => { context.error.textContent = safeError(error); context.setBusy(false); });
  });
  provider.focus();
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
    void dialogSave(context, async () => { await adapter.launch(account.id, mode, workingDirectory); lastWorkingDirectory = workingDirectory; }, demo ? `Synthetic ${mode} launch recorded. No terminal was opened.` : mode === 'managed' ? 'Terminal launch requested in your project through the local proxy. Selection takes effect on the next request.' : 'Terminal launch requested in your project with this account’s private home. Provider acceptance is not yet observed.');
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

function renderCurrent(main: HTMLElement) {
  const section = el('section', 'current-section'); section.setAttribute('aria-label', 'Current CLI accounts');
  const heading = el('div', 'section-heading');
  heading.append(el('h2', '', 'Current CLI accounts'), el('span', 'usage-caption', 'Observed local identity · managed selection is separate'));
  section.append(heading);
  const grid = el('div', 'current-grid');
  for (const provider of ['claude', 'codex'] as const) {
    const current = currentAccounts?.[provider]; const card = el('div', 'current-card');
    card.append(el('strong', '', providerName(provider)));
    if (current?.status === 'available') {
      card.append(el('p', 'current-identity', identityText(current.identity)));
      if (current.identity?.organization_id) card.append(el('span', 'usage-caption', `Organization: ${current.identity.organization_id}`));
      const matches = snapshot!.accounts.filter((account) => account.provider === provider && currentMatch(account));
      card.append(el('span', 'usage-caption', matches.length ? `Stored as ${matches.map((account) => `${account.label} · ${account.pool}`).join(', ')}` : 'Not matched to a stored account. Capture it to add it.'));
    } else card.append(el('p', 'current-identity', current?.status === 'missing' ? 'No signed-in CLI account found' : 'Current identity unavailable'), el('span', 'usage-caption', current?.status === 'missing' ? 'Use official sign-in to add an account.' : 'Refresh to retry reading the local CLI account.'));
    const capture = button('Capture current account', () => addDialogForProvider(provider), 'text-button', `capture-${provider}`);
    card.append(capture); grid.append(card);
  }
  section.append(grid); main.append(section);
}
function addDialogForProvider(provider: Provider) {
  addDialog();
  const control = document.querySelector<HTMLSelectElement>('dialog select');
  if (control) { control.value = provider; control.dispatchEvent(new Event('change')); }
}
function usagePanel(account: Account) {
  const section = el('div', 'usage'); const observation = account.usage; const health = account.usage_health;
  if (observation) {
    const policies = snapshot?.policies?.filter((policy) => policy.provider === account.provider && policy.pool === account.pool && policy.enabled) ?? [];
    const maxAge = policies.length ? Math.min(...policies.map((policy) => policy.max_age_seconds)) : 300;
    const stale = Date.now() / 1000 - observation.observed_at > maxAge;
    const label = el('div', 'usage-label'); label.append(el('strong', '', `${Math.round(observation.used_percent)}% used`), el('span', stale ? 'stale' : '', `${stale ? 'Stale' : 'Observed'} · ${age(observation.observed_at)}`)); section.append(label);
    const meter = el('progress', 'usage-meter'); meter.max = 100; meter.value = observation.used_percent; meter.setAttribute('aria-label', `Highest quota used for ${account.label}`); section.append(meter);
    const details = el('details', 'quota-details'); const summary = el('summary', '', 'Quota windows'); summary.dataset.focus = `quota-${account.id}`; details.append(summary);
    const windows = observation.windows?.length ? observation.windows : [{ name: 'Highest reported window', used_percent: observation.used_percent, resets_at: observation.resets_at }];
    for (const window of windows) {
      const row = el('div', 'quota-window'); row.append(el('strong', '', `${window.name} · ${Math.round(window.used_percent)}% used`), el('span', 'usage-caption', window.resets_at ? `Resets ${date(window.resets_at)}` : 'Reset time unavailable')); details.append(row);
    }
    details.append(el('p', 'usage-caption', `${observation.source} · Observed ${date(observation.observed_at)}`));
    if (stale) details.append(el('p', 'usage-caption', 'Too old for automatic rotation. Waiting for a fresh quota check.'));
    section.append(details);
  } else section.append(el('strong', 'usage-unknown', 'Usage unknown'), el('span', 'usage-caption', account.kind === 'api_key' ? 'API billing is separate' : 'No observation yet'));
  if (health) {
    if (health.status === 'failed') section.append(el('p', 'usage-error', 'Last quota check failed. Not eligible for automatic rotation.'));
    else if (health.status === 'unavailable') section.append(el('p', 'usage-caption', 'Quota unavailable for this account.'));
    section.append(el('span', 'usage-caption', `Checked ${age(health.checked_at)} · Next check ${date(health.next_check_at)}`));
  }
  if (usageErrors.has(account.id)) section.append(el('p', 'usage-error', usageErrors.get(account.id)!));
  return section;
}
const policyTarget = (target: RotationPolicy['target']) => target === 'managed' ? 'Managed route' : 'Claude Code account';
function renderPolicies(main: HTMLElement) {
  const details = el('details', 'policy-panel'); const policies = snapshot!.policies ?? [];
  const summary = el('summary', '', `Automatic rotation · ${policies.filter((policy) => policy.enabled).length} enabled`); summary.dataset.focus = 'policies'; details.append(summary);
  details.append(el('p', 'form-note', monitor ? `${monitor.running ? 'Quota monitor running' : 'Quota monitor stopped'} · Normal check interval ${monitor.interval_seconds} seconds. Failed checks back off. Rotation starts only when you enable a policy.` : 'Quota monitor status unavailable. Refresh to retry.'));
  if (demo) details.append(el('p', 'form-note', 'Demo policies are editable fixtures. No automatic switching or provider checks run here.'));
  if (!policies.length) details.append(el('p', 'form-note', 'No rotation policies saved. Configure a provider and pool to begin.'));
  for (const policy of policies) {
    const row = el('div', 'policy-row');
    const content = el('div'); content.append(el('strong', '', `${providerName(policy.provider)} · ${policy.pool} · ${policyTarget(policy.target)}`), el('p', 'usage-caption', `${policy.enabled ? 'Enabled' : 'Off'} · At ${policy.threshold_percent}% used · Improve by ${policy.hysteresis_percent} points · Cooldown ${policy.cooldown_seconds}s · Fresh within ${policy.max_age_seconds}s`));
    if (policy.last_switched_at) content.append(el('p', 'usage-caption', `Last switched ${date(policy.last_switched_at)}`));
    const decision = monitor?.decisions?.find((entry) => entry.provider === policy.provider && entry.pool === policy.pool && entry.target === policy.target);
    if (decision) content.append(el('p', 'usage-caption', decisionText(decision.reason)));
    const actions = el('div', 'policy-actions');
    actions.append(button('Edit policy', () => policyDialog(policy), 'text-button', `policy-${policy.provider}-${policy.pool}-${policy.target}`));
    if (policy.enabled) actions.append(button('Stop rotation', () => void mutate(() => adapter.setPolicy({ ...policy, enabled: false }), 'Automatic rotation stopped for this provider, pool and target.', 'policies'), 'text-button'));
    row.append(content, actions); details.append(row);
  }
  details.append(button('Configure rotation', () => policyDialog(), 'button', 'configure-policy')); main.append(details);
}
function decisionText(reason: string) {
  const labels: Record<string, string> = {
    disabled: 'Rotation is off.', cooldown: 'Waiting for the cooldown to end.', below_threshold: 'Current usage is below the threshold.',
    no_eligible_account: 'No eligible account. Holding the current account.',
    stale_usage: 'Waiting for fresh usage. Holding the current account.', usage_unavailable: 'Usage unavailable. Holding the current account.',
    current_unavailable: 'Current account unavailable. Holding the current account.', threshold_reached: 'Threshold reached. An eligible account is available; a switch is not yet confirmed.', switched: 'The monitor recorded a switch.', activation_failed: 'Native activation failed. Check the current CLI identity and retry manually.',
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
  const grid = el('div', 'form-grid'); grid.append(field('Provider', provider), field('Pool', pool), field('Target', target), field('Switch at usage (%)', threshold), field('Minimum improvement (points)', hysteresis, 'The candidate must have this much less usage.'), field('Cooldown (seconds)', cooldown), field('Maximum usage age (seconds)', freshness));
  const toggle = el('label', 'checkbox-field'); toggle.append(enabled, el('span', '', 'Enable automatic rotation'));
  context.body.append(grid, toggle, el('p', 'form-note', 'Claude Code rotation requires OAuth accounts with an external identity. Managed rotation affects the next request. A response already in progress keeps its account.'));
  context.actions.append(submit('Save policy'));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault();
    hysteresis.setCustomValidity(Number(hysteresis.value) >= Number(threshold.value) ? 'Minimum improvement must be less than the usage threshold.' : '');
    if (!context.form.reportValidity()) return;
    const previous = snapshot?.policies?.find((policy) => policy.provider === provider.value && policy.pool === pool.value.trim() && policy.target === target.value);
    const policy: RotationPolicy = { provider: provider.value as Provider, pool: pool.value.trim(), target: target.value as RotationPolicy['target'], enabled: enabled.checked, threshold_percent: Number(threshold.value), hysteresis_percent: Number(hysteresis.value), cooldown_seconds: Number(cooldown.value), max_age_seconds: Number(freshness.value), last_switched_at: previous?.last_switched_at ?? null };
    void dialogSave(context, () => adapter.setPolicy(policy), `Rotation policy saved${policy.enabled ? ' and enabled' : '; rotation is off'}.`);
  });
  hysteresis.addEventListener('input', () => hysteresis.setCustomValidity('')); threshold.addEventListener('input', () => hysteresis.setCustomValidity(''));
  (existing ? threshold : provider).focus();
}
function importDialog() {
  const context = openDialog('Import Claude Swap', 'Read Claude Swap profiles from their standard local location. Existing identities in this pool are updated; disabled accounts remain disabled.');
  const pool = input('default'); pool.maxLength = 32; pool.pattern = '[a-z0-9_-]+';
  context.body.append(field('Pool', pool, 'All imported profiles are placed in this routing pool.'));
  if (demo) context.body.append(el('p', 'form-note', 'Synthetic fixture: two profiles import, one fails, and one is skipped.'));
  context.actions.append(submit('Import profiles'));
  context.form.addEventListener('submit', (event) => {
    event.preventDefault(); if (!context.form.reportValidity()) return;
    context.setBusy(true); context.error.textContent = '';
    void adapter.importClaudeSwap(pool.value.trim()).then(async (result) => {
      await refreshContext();
      try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'Import completed, but accounts could not be refreshed. Refresh to load the imported accounts.'; }
      showNotice(`${result.imported.length} profiles imported or updated · ${result.skipped} skipped · ${result.failed} failed. ${result.failed ? 'Check the source profiles and retry; imported accounts remain available.' : 'Select an account when you are ready.'}`, result.failed > 0);
      render(); context.setBusy(false); context.close();
    }).catch((failure) => { context.error.textContent = safeError(failure); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); });
  }); pool.focus();
}
function activateDialog(account: Account) {
  const context = openDialog('Activate in Claude Code', `Use ${account.label} (${identityText(account.external_identity)}) as the local Claude Code account.`);
  context.body.append(el('p', 'form-note', 'This writes the native Claude Code account. Managed route selection stays separate. Running sessions may need to reload; activation alone is not evidence of a provider response.'));
  context.actions.append(submit('Activate in Claude Code'));
  context.form.addEventListener('submit', (event) => { event.preventDefault(); void dialogSave(context, () => adapter.activateNative(account.id), demo ? 'Synthetic Claude Code account changed. No local credentials were read or written.' : 'Claude Code account activated. Reload the CLI if needed; current local identity is shown above.'); });
}

render();
if (demo) { const { createDemoAdapter } = await import('./demo'); adapter = createDemoAdapter(); }
void reload();
// Read monitor metadata on a bounded cadence. Quota probing belongs to the runtime.
let refreshing = false;
setInterval(() => {
  if ((!native && !demo) || refreshing || document.hidden || document.querySelector('dialog') || busy || loading) return;
  refreshing = true;
  void Promise.all([adapter.snapshot(), refreshContext()]).then(([data]) => {
    if (document.querySelector('dialog') || busy || loading) return;
    snapshot = data; const key = (document.activeElement as HTMLElement)?.dataset.focus;
    render(); restoreFocus(key);
  }).catch(() => { /* Explicit Refresh reports a failed metadata read. */ }).finally(() => { refreshing = false; });
}, 60_000);
