import './tokens.css';
import './style.css';
import { demo, native, nativeAdapter, safeError } from './adapter';
import type { Account, Adapter, AuthKind, Provider, RuntimeStatus, Snapshot } from './types';

const root = document.querySelector<HTMLDivElement>('#app')!;
const announcements = document.querySelector<HTMLDivElement>('#announcements')!;
const theme = matchMedia('(prefers-color-scheme: dark)');
const setTheme = () => { document.documentElement.dataset.theme = theme.matches ? 'dark' : 'light'; };
theme.addEventListener('change', setTheme); setTheme();
let adapter: Adapter = nativeAdapter;
let snapshot: Snapshot | null = null;
let runtime: RuntimeStatus | null = null;
let page: 'accounts' | 'activity' | 'about' = 'accounts';
let filter: 'all' | Provider = 'all';
let busy = false;
let loading = true;
let loadError = '';
let runtimeError = false;
let notice = '';
let noticeError = false;
const usageErrors = new Map<string, string>();
const providerName = (provider: Provider) => provider === 'claude' ? 'Claude Code' : 'Codex CLI';
const kindName = (kind: AuthKind) => ({ api_key: 'API key', setup_token: 'Setup token', oauth: 'OAuth' })[kind];
const date = (seconds: number) => new Date(seconds * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
const age = (seconds: number) => { const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60)); return minutes < 1 ? 'just now' : minutes < 60 ? `${minutes}m ago` : `${Math.floor(minutes / 60)}h ago`; };
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
  else { loadError = 'Accounts could not be loaded. Check Keychain access and retry.'; announce(loadError); }
  if (status.status === 'fulfilled') { runtime = status.value; runtimeError = false; }
  else { runtime = null; runtimeError = true; }
  loading = false; render();
}
async function mutate(action: () => Promise<unknown>, success: string, focusKey?: string, accountId?: string) {
  busy = true; notice = ''; render();
  try {
    await action();
    if (accountId) usageErrors.delete(accountId);
    showNotice(success);
    try { snapshot = await adapter.snapshot(); loadError = ''; }
    catch { loadError = 'The action completed, but accounts could not be refreshed. Retry loading the account list.'; }
  } catch (error) { const text = safeError(error); if (accountId) usageErrors.set(accountId, text); showNotice(text, true); }
  finally { busy = false; render(); restoreFocus(focusKey); }
}

function render() {
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
  sidebarFoot.append(el('span', 'eyebrow', 'LOCAL WORKBENCH'), el('p', '', demo ? 'Synthetic session' : 'Your accounts. Your machine.'), el('span', 'version', 'v0.1 · macOS'));
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
    if (page === 'accounts') actions.append(button('+ Add account', () => addDialog(), 'button primary', 'add-account'));
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
}
function emptyState(title: string, description: string) { const section = el('section', 'empty-state'); section.append(el('div', 'empty-symbol', '◇'), el('h2', '', title), el('p', '', description)); return section; }
function renderAccounts(main: HTMLElement) {
  const status = el('section', 'runtime'); status.setAttribute('aria-label', 'Runtime status');
  const indicator = el('span', `status-dot ${runtimeError ? 'unavailable' : ''}`); indicator.setAttribute('aria-hidden', 'true');
  const text = el('div', 'runtime-copy');
  text.append(el('strong', '', demo ? 'Demo runtime' : runtimeError ? 'Proxy status unavailable' : 'Local proxy ready'), el('span', '', runtimeError ? 'Refresh to retry. Isolated launch is still available.' : `${runtime?.proxy_address ?? 'Checking…'} · ${demo ? 'No live requests' : 'HTTP / SSE'}`));
  status.append(indicator, text, el('span', 'runtime-caption', 'Selection applies to the next managed request.'));
  main.append(status);
  const toolbar = el('div', 'toolbar');
  const filters = el('div', 'filters'); filters.setAttribute('role', 'group'); filters.setAttribute('aria-label', 'Filter by provider');
  for (const [value, label] of [['all', 'All providers'], ['claude', 'Claude Code'], ['codex', 'Codex CLI']] as const) {
    const control = button(label, () => { filter = value; render(); restoreFocus(`filter-${value}`); }, `filter ${filter === value ? 'active' : ''}`, `filter-${value}`); control.setAttribute('aria-pressed', String(filter === value)); filters.append(control);
  }
  const accounts = snapshot!.accounts.filter((account) => filter === 'all' || account.provider === filter);
  toolbar.append(filters, el('span', 'count', `${accounts.length} ${accounts.length === 1 ? 'account' : 'accounts'}`)); main.append(toolbar);
  if (!accounts.length) {
    const empty = emptyState(snapshot!.accounts.length ? 'No accounts for this provider' : 'Start with one account', snapshot!.accounts.length ? 'Add an account here, or choose All providers to see your other accounts.' : 'Sign in with the official CLI or import a credential. Choose a pool to keep work and personal sessions separate.');
    empty.append(button('Add account', () => addDialog(), 'button primary', 'empty-add')); main.append(empty);
  } else {
    const list = el('div', 'account-list'); list.setAttribute('aria-label', 'Accounts');
    accounts.forEach((account) => list.append(accountCard(account))); main.append(list);
  }
  const footer = el('p', 'surface-note', 'Managed sessions follow your selection within the same provider and pool. In-progress responses keep their account. Existing external clients are not controlled.'); main.append(footer);
}
function accountCard(account: Account) {
  const active = selected(account);
  const card = el('article', `account-card ${active ? 'is-selected' : ''} ${!account.enabled ? 'is-disabled' : ''}`);
  card.setAttribute('aria-label', `${account.label}, ${providerName(account.provider)}, ${account.pool} pool`);
  const identity = el('div', 'account-identity');
  const icon = el('span', `provider-icon ${account.provider}`, account.provider === 'claude' ? '✳' : '◎'); icon.setAttribute('aria-hidden', 'true');
  const details = el('div', 'account-details'); const title = el('div', 'account-title');
  title.append(el('h2', '', account.label)); if (!account.enabled) title.append(el('span', 'badge muted', 'Disabled'));
  details.append(title, el('p', 'account-meta', `${providerName(account.provider)} · ${kindName(account.kind)} · ${account.pool}`));
  identity.append(icon, details); card.append(identity);
  const usage = el('div', 'usage');
  if (account.usage) {
    const observation = account.usage; const stale = Date.now() / 1000 - observation.observed_at > 300;
    const label = el('div', 'usage-label'); label.append(el('strong', '', `${Math.round(observation.used_percent)}% used`), el('span', stale ? 'stale' : '', `${stale ? 'Stale · ' : ''}${age(observation.observed_at)}`)); usage.append(label);
    const meter = el('progress', 'usage-meter'); meter.max = 100; meter.value = observation.used_percent; meter.setAttribute('aria-label', `Quota used for ${account.label}`); usage.append(meter);
    usage.title = `${observation.source} · Observed ${date(observation.observed_at)}${observation.resets_at ? ` · Resets ${date(observation.resets_at)}` : ''}`;
  } else usage.append(el('strong', 'usage-unknown', 'Usage unknown'), el('span', 'usage-caption', account.kind === 'api_key' ? 'API billing is separate' : 'No observation yet'));
  if (usageErrors.has(account.id)) usage.append(el('p', 'usage-error', usageErrors.get(account.id)!));
  card.append(usage);
  const route = el('div', 'route-control');
  if (active) { const badge = el('span', 'selected-label', '✓ Selected for next request'); badge.tabIndex = -1; badge.dataset.focus = `select-${account.id}`; route.append(badge); }
  else { const select = button('Select', () => void mutate(() => adapter.select(account), `${account.label} selected for the next managed request in ${account.pool}.`, `select-${account.id}`), 'button select-button', `select-${account.id}`); select.disabled ||= !account.enabled; route.append(select); }
  card.append(route);
  const actions = el('div', 'account-actions');
  const launches = el('div', 'launch-actions');
  const isolated = button('Launch isolated', () => void mutate(() => adapter.launch(account.id, 'isolated'), demo ? 'Synthetic isolated launch recorded. No terminal was opened.' : 'Terminal launch requested with this account’s private home. Provider acceptance is not yet observed.', `isolated-${account.id}`), 'text-button', `isolated-${account.id}`); isolated.disabled ||= !account.enabled;
  const managed = button('Launch managed', () => void mutate(() => adapter.launch(account.id, 'managed'), demo ? 'Synthetic managed launch recorded. No terminal was opened.' : 'Terminal launch requested through the local proxy. Selection takes effect on the next request.', `managed-${account.id}`), 'text-button', `managed-${account.id}`); managed.disabled ||= !account.enabled || !active || runtimeError; managed.title = !active ? 'Select this account before launching a managed session.' : 'Launch through the local proxy.';
  launches.append(isolated, managed);
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
    ['Compatibility', 'macOS first; Windows is planned. Live provider acceptance is a separate check. Existing external CLI sessions and Codex Desktop are not controlled.'],
    ['Imported OAuth', 'A captured credential snapshot. On expiry, sign in again. There is no background refresh-token exchange.'],
  ]) { definitions.append(el('dt', '', term), el('dd', '', description)); }
  section.append(definitions); main.append(section);
}

interface DialogContext { dialog: HTMLDialogElement; form: HTMLFormElement; body: HTMLElement; actions: HTMLElement; error: HTMLElement; close: () => void; setBusy: (value: boolean) => void }
function openDialog(title: string, intro: string): DialogContext {
  const trigger = (document.activeElement as HTMLElement | null)?.dataset.focus;
  const dialog = el('dialog', 'dialog'); dialog.setAttribute('aria-labelledby', 'dialog-title'); dialog.setAttribute('aria-describedby', 'dialog-description');
  const form = el('form'); const header = el('div', 'dialog-header');
  const heading = el('h2', '', title); heading.id = 'dialog-title'; const description = el('p', '', intro); description.id = 'dialog-description'; header.append(heading, description);
  const body = el('div', 'dialog-body'); const error = el('p', 'dialog-error'); error.setAttribute('role', 'alert');
  const actions = el('div', 'dialog-actions'); let pending = false;
  const close = () => { if (!pending) dialog.close(); };
  const cancel = button('Cancel', close, 'button'); actions.append(cancel);
  form.append(header, body, error, actions); dialog.append(form); document.body.append(dialog);
  dialog.addEventListener('cancel', (event) => { if (pending) event.preventDefault(); });
  dialog.addEventListener('close', () => { form.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input,textarea').forEach((input) => { input.value = ''; }); dialog.remove(); restoreFocus(trigger); });
  dialog.showModal();
  const setBusy = (value: boolean) => { pending = value; form.setAttribute('aria-busy', String(value)); form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement | HTMLTextAreaElement>('input, button, select, textarea').forEach((node) => { node.disabled = value; }); };
  return { dialog, form, body, actions, error, close, setBusy };
}
function field(label: string, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement, help = '') {
  const wrapper = el('label', 'field'); wrapper.append(el('span', 'field-label', label), input); if (help) wrapper.append(el('span', 'field-help', help)); return wrapper;
}
function input(value = '', type = 'text') { const node = el('input'); node.type = type; node.value = value; node.required = true; node.autocomplete = 'off'; node.spellcheck = false; return node; }
function select(options: [string, string][]) { const node = el('select'); options.forEach(([value, text]) => { const option = el('option', '', text); option.value = value; node.append(option); }); return node; }
function submit(text: string) { const node = el('button', 'button primary', text); node.type = 'submit'; return node; }
async function dialogSave(context: DialogContext, action: () => Promise<unknown>, success: string) {
  context.error.textContent = ''; context.setBusy(true);
  try { await action(); showNotice(success); try { snapshot = await adapter.snapshot(); loadError = ''; } catch { loadError = 'The action completed, but the list could not be refreshed. Retry loading accounts.'; } render(); context.setBusy(false); context.close(); }
  catch (error) { context.error.textContent = safeError(error); context.setBusy(false); context.error.tabIndex = -1; context.error.focus(); }
}
function addDialog() {
  const context = openDialog('Add account', 'Use an account you own or are authorized to use. Your existing client configuration stays separate.');
  const provider = select([['claude', 'Claude Code'], ['codex', 'Codex CLI']]);
  const method = select([['login', 'Official sign-in'], ['api_key', 'API key'], ['setup_token', 'Claude setup token'], ['oauth', 'Import OAuth JSON']]);
  const label = input(); label.maxLength = 80; label.placeholder = 'e.g. Studio';
  const pool = input('work'); pool.maxLength = 40; pool.pattern = '[a-z0-9_-]+'; pool.title = 'Use lowercase letters, numbers, hyphens, or underscores.';
  const grid = el('div', 'form-grid'); grid.append(field('Provider', provider), field('Authentication', method), field('Account label', label), field('Pool', pool, 'A boundary for routing, such as work or personal.'));
  const credentialSlot = el('div'); const submitButton = submit('Begin sign-in'); context.actions.append(submitButton); context.body.append(grid, credentialSlot);
  let secret: HTMLInputElement | HTMLTextAreaElement | null = null;
  let loginId: string | null = null;
  const update = () => {
    if (secret) secret.value = ''; credentialSlot.replaceChildren(); secret = null;
    const setup = method.querySelector<HTMLOptionElement>('option[value="setup_token"]')!; setup.disabled = provider.value !== 'claude';
    if (setup.disabled && method.value === 'setup_token') method.value = 'api_key';
    if (method.value === 'login') {
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
    if (!base.label) { label.setCustomValidity('Enter an account label.'); label.reportValidity(); label.addEventListener('input', () => label.setCustomValidity(''), { once: true }); return; }
    if (loginId) { void dialogSave(context, () => adapter.finishLogin(loginId!), demo ? 'Synthetic sign-in account added.' : 'Account captured from the private sign-in home. Select it when you are ready.').finally(() => { grid.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select').forEach((node) => { node.disabled = true; }); }); return; }
    if (method.value !== 'login') {
      const value = secret!.value; secret!.value = '';
      void dialogSave(context, () => adapter.add({ ...base, kind: method.value as AuthKind, secret: value }), 'Account added. Select it when you are ready.'); return;
    }
    context.error.textContent = ''; context.setBusy(true);
    void adapter.beginLogin(base).then((result) => {
      loginId = result.login_id; context.setBusy(false);
      grid.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select').forEach((node) => { node.disabled = true; });
      credentialSlot.replaceChildren(el('div', 'login-pending', demo ? 'Synthetic sign-in ready. Choose Finish sign-in to add this demo account.' : 'Complete sign-in in Terminal, then choose Finish sign-in. If the provider opens a browser, finish that step first.'));
      submitButton.textContent = 'Finish sign-in'; submitButton.focus();
    }).catch((error) => { context.error.textContent = safeError(error); context.setBusy(false); });
  });
  provider.focus();
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

render();
if (demo) { const { createDemoAdapter } = await import('./demo'); adapter = createDemoAdapter(); }
void reload();
// Refresh age labels only; never probe providers or read credentials implicitly.
setInterval(() => { if (!document.querySelector('dialog') && !busy && !loading && page === 'accounts') { const key = (document.activeElement as HTMLElement)?.dataset.focus; render(); restoreFocus(key); } }, 60_000);
