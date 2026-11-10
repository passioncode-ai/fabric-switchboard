import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Adapter } from './types';
import { readWithDeadline, READ_TIMEOUT } from './read-deadline';
import { matchFormatted } from './formatted-errors';
import personErrors from '../crates/switchboard-runtime/src/person-errors.json';
export const reportFrontendReady = () => invoke('frontend_ready');
/** The interface language, for the native menu-bar / notification-area menu (L10N-01). */
export const reportLanguage = (locale: string) => invoke('set_language', { locale });

export const native = isTauri();
/** SB-117: the owner found, added or declined a project folder; the window reads the list again. */
export function onProjectSuggestions(callback: () => void) {
  if (!native) return;
  void import('@tauri-apps/api/event').then(({ listen }) => listen('project-suggestions', () => callback())).catch(() => { /* the minute refresh still reads them */ });
}
/** SB-183 (node A-7): the owner wrote the terminal hook for the first time; the window shows its notice. */
export function onShellHook(callback: () => void) {
  if (!native) return;
  void import('@tauri-apps/api/event').then(({ listen }) => listen('shell-hook', () => callback())).catch(() => { /* read again when the window is shown */ });
}
/** SB-72 (C-7): spend.json changed (a ceiling set or answered, a question opened); the window reads the ceilings again. */
export function onSpendChanged(callback: () => void) {
  if (!native) return;
  void import('@tauri-apps/api/event').then(({ listen }) => listen('spend-changed', () => callback())).catch(() => { /* read again with the minute's refresh */ });
}
export const demo = !native && new URLSearchParams(location.search).get('demo') === '1';
/** SB-192 (UX-27): `?demo=1&empty=1` shows a first run: the tour and the terms notice ignore what this browser stored. */
export const demoFirstRun = demo && new URLSearchParams(location.search).get('empty') === '1';
export const nativeAdapter: Adapter = {
  snapshot: () => readWithDeadline(invoke('snapshot')),
  currentAccounts: () => readWithDeadline(invoke('current_accounts')),
  captureCurrent: (input) => invoke('capture_current', { ...input }),
  importClaudeSwap: (pool) => invoke('import_claude_swap', { pool }),
  activateNative: (id, force) => invoke('activate_native', { id, force: force ?? false }),
  setPolicy: (policy) => invoke('set_policy', { policy }),
  monitorStatus: () => readWithDeadline(invoke('monitor_status')),
  sessions: () => readWithDeadline(invoke('sessions')),
  runtime: () => readWithDeadline(invoke('runtime_status')),
  add: (input) => invoke('add_account', { ...input }),
  update: (id, label, enabled) => invoke('update_account', { id, label, enabled }),
  markElsewhere: (id, on, location) => invoke('mark_elsewhere', { id, on, location: location ?? null }),
  assignAccount: (id, to) => invoke('assign_account', { id, to }),
  sendAccount: (id, to, pool) => invoke('send_account', { id, to, pool }),
  recallAccount: (id, from) => invoke('recall_account', { id, from }),
  handOverAccount: (id, to, name) => invoke('hand_over_account', { id, to, name }),
  openHandover: () => invoke('open_handover'),
  receiveHandover: (record, signer, apply) => invoke('receive_handover', { record, signer, apply }),
  remove: (id) => invoke('remove_account', { id }),
  select: ({ id, provider, pool }, force) => invoke('select_account', { id, provider, pool, force: force ?? false }),
  launch: (id, mode, workingDirectory) => invoke('launch_account', { id, mode, workingDirectory }),
  pickFolder: (start) => invoke('pick_folder', { start: start ?? null }),
  pickFolders: (start) => invoke('pick_folders', { start: start ?? null }),
  beginLogin: (input) => invoke('begin_login', { ...input }),
  loginStatus: (loginId) => readWithDeadline(invoke('login_status', { loginId })),
  pendingLogins: () => readWithDeadline(invoke('pending_logins')),
  finishLogin: (loginId) => invoke('finish_login', { loginId }),
  cancelLogin: (loginId) => invoke('cancel_login', { loginId }),
  probe: (id) => invoke('probe_usage', { id }),
  setProjectRule: (input) => invoke('set_project_rule', { ...input }),
  removeProjectRule: (path, provider) => invoke('remove_project_rule', { path, provider }),
  agentSetup: () => readWithDeadline(invoke('agent_setup')),
  backups: () => readWithDeadline(invoke('backups')),
  backupNow: () => invoke('backup_now'),
  restoreBackup: (file) => invoke('restore_backup', { file }),
  linkCli: () => invoke('link_cli'),
  saveProject: (input) => invoke('save_project', { pool: input.pool ?? null, name: input.name, folders: input.folders, accountIds: input.accountIds, baseFolders: input.baseFolders ?? null, organization: input.organization ?? null, organizations: input.organizations ?? null }),
  removeProject: (pool) => invoke('remove_project', { pool }),
  projectRoots: () => invoke('project_roots'),
  moveRoot: (folder, pool, also, from) => invoke('move_root', { folder, pool, also: also ?? false, from: from ?? null }),
  codexRoute: (enabled) => invoke('codex_route', { enabled: enabled ?? null }),
  repositoryMap: (under) => readWithDeadline(invoke('repository_map', { under }), 60_000),
  statistics: (days) => invoke('statistics', { days }),
  moveAccount: (id, pool) => invoke('move_account', { id, pool }),
  organizationAdd: (name, domains) => invoke('organization_add', { name, domains }),
  githubOwners: () => invoke('github_owners'),
  githubRepositories: (owner) => invoke('github_repositories', { owner }),
  githubClone: (owner, name, into) => invoke('github_clone', { owner, name, into }),
  projectSources: (pool, sources) => invoke('project_sources', { pool, sources }),
  organizationEdit: (id, name, domains) => invoke('organization_edit', { id, name, domains }),
  organizationRemove: (id) => invoke('organization_remove', { id }),
  organizationAssign: (account, choice, organizations) => invoke('organization_assign', { account, choice, organizations: organizations ?? null }),
  passioncode: (action) => invoke('passioncode', { action }),
  projectSuggestions: () => readWithDeadline(invoke('project_suggestions')),
  acceptProjectSuggestion: (folder) => invoke('accept_project_suggestion', { folder }),
  dismissProjectSuggestion: (folder) => invoke('dismiss_project_suggestion', { folder }),
  agentConnect: (agent, pool) => readWithDeadline(invoke('agent_connect', { agent, pool })),
  launchAgent: (agent, pool, workingDirectory) => invoke('launch_agent', { agent, pool, workingDirectory }),
  // Not under the read deadline: the status asks OpenRouter itself (10 s timeout in the backend).
  openrouterStatus: () => invoke('openrouter_status'),
  openrouterKeyStatus: (pool) => readWithDeadline(invoke('openrouter_status', { pool })),
  openrouterSave: (key, model, pool) => invoke('openrouter_save', { key, model, pool: pool ?? null }),
  openrouterModel: (model, pool) => invoke('openrouter_model', { model, pool: pool ?? null }),
  openrouterRemove: (pool) => invoke('openrouter_remove', { pool: pool ?? null }),
  openrouterKeys: () => readWithDeadline(invoke('openrouter_keys')),
  hermesModel: () => readWithDeadline(invoke('hermes_model'), 20_000),
  agentsOverview: () => readWithDeadline(invoke('agents_overview'), 20_000),
  chains: () => readWithDeadline(invoke('chains'), 20_000),
  setChain: (scope, executors, preset) => invoke('set_chain', { scope, executors, preset: preset ?? null }),
  ceilings: () => readWithDeadline(invoke('ceilings')),
  setCeiling: (scope, usd) => invoke('set_ceiling', { scope, usd }),
  answerCeiling: (id, answer) => invoke('answer_ceiling', { id, answer }),
  agentEnable: (agent, enabled) => invoke('agent_enable', { agent, enabled }),
  agentDefault: (agent) => invoke('agent_default', { agent }),
  openrouterModels: () => readWithDeadline(invoke('openrouter_models'), 20_000),
  hermesSetModel: (provider, model) => invoke('hermes_set_model', { provider, model }),
  // Not under the read deadline: each account asks Kimi itself (8 s timeout in the backend).
  kimiAccounts: () => invoke('kimi_accounts'),
  kimiLoginBegin: (label, region) => invoke('kimi_login_begin', { label, region }),
  kimiLoginStatus: (id) => readWithDeadline(invoke('kimi_login_status', { id })),
  kimiLoginFinish: (id) => invoke('kimi_login_finish', { id }),
  kimiLoginCancel: (id) => invoke('kimi_login_cancel', { id }),
  kimiSignInAgain: (id) => invoke('kimi_sign_in_again', { id }),
  kimiLaunch: (id, workingDirectory) => invoke('kimi_launch', { id, workingDirectory }),
  kimiRemove: (id) => invoke('kimi_remove', { id }),
  launchOnOpenrouter: (agent, model, workingDirectory) => invoke('launch_agent', { agent, pool: '', workingDirectory, via: 'openrouter', model }),
  loginItem: () => readWithDeadline(invoke('login_item')),
  setLoginItem: (enabled) => invoke('set_login_item', { enabled }),
  shellHookNotice: () => readWithDeadline(invoke('shell_hook_notice')),
  acknowledgeShellHookNotice: async () => { await invoke('acknowledge_shell_hook_notice'); },
  shellHookStatus: () => readWithDeadline(invoke('shell_hook_status')),
  onboard: () => readWithDeadline(invoke('onboard')),
  setShellHook: (on) => invoke('set_shell_hook', { on }),
  analytics: () => readWithDeadline(invoke('analytics_status')),
  setAnalytics: (enabled) => invoke('set_analytics', { enabled }),
  analyticsNoticeSeen: () => invoke('analytics_notice_seen'),
  updateStatus: () => readWithDeadline(invoke('update_status')),
  setAutoUpdate: (enabled) => invoke('set_auto_update', { enabled }),
  restartToUpdate: () => invoke('restart_to_update'),
  checkForUpdates: () => invoke('check_for_updates'),
};

// Only exact, fixed backend vocabulary is surfaced. Unknown failures never print
// raw provider output, credentials, filesystem paths, or serialized error objects.
const safeErrors = new Set([
  'Only an account you sign in to can be handed over. Give an API key through your vault.',
  'This is not a Switchboard hand-over file (switchboard-handover/1).',
  'This hand-over file is not signed by the key you named, or it changed after it was signed.',
  'Say where it works in at most 80 characters, on one line.',
  // SB-322: an account given to an agent, and the agent's name.
  'This account is given to an agent, so Switchboard does not use it. Take it back, or use it anyway.',
  'Name the agent in at most 120 characters, on one line.',
  'A group can belong to at most 16 organizations.',
  // The CLI node's new backend sentences (N2 of 0.6.82), shown as they are; the older ones are mapped below.
  'No OpenRouter key is saved. Add one on the API keys page, or run switchboard agents openrouter set.',
  'That group id names no group. Choose a group, or none for the key every folder uses.',
  'Choose a model for this launch, or set the key\'s default model on the API keys page or with switchboard agents openrouter model.',
  'This account belongs to a group. Launch it from one of the group\'s folders.',
  'This folder belongs to a group. Launch one of the group\'s accounts.',
  // The permission mode of the sessions Switchboard starts (SB-298).
  'Choose bypass or ask.',
  'Group not found.',
  // Moving a sign-in to another machine (SB-283).
  "Name the other machine as ssh knows it: letters, digits, dots, hyphens, @ and : only.",
  "Give the other machine's switchboard as an absolute path without quotes.",
  "Give the other machine's data folder as an absolute path without quotes.",
  "This is not the sign-in that was handed over. Cancel this sign-in, then sign in with the account the record names.",
  "The file picker could not open. Try again.",
  "The record could not be saved there, so the account stays on this computer.",
  "This file could not be read.",
  "Nothing was saved. The account stays on this computer.",
  "Only a Claude or Codex subscription sign-in moves this way. Give an API key through your vault.",
  "This account is already used elsewhere or turned off here.",
  "Select another account for its group before moving this one.",
  "The other machine did not save the sign-in. Nothing changed here; check ssh and its Switchboard.",
  "The other machine already holds this sign-in in that group. Remove its old copy there, then move it again.",
  "The other machine saved the sign-in but the provider did not answer it there. It was removed there and stays here.",
  // Receiving a hand-over (SB-315).
  "Name the giver's published key (switchboard settings signer on their computer) to start this sign-in.",
  "This computer already holds this sign-in. There is nothing to receive.",
  // Using a moved sign-in here again by a mark (SB-315).
  "This account's sign-in was moved to another machine. Call it back to use it here, so only one machine renews it.",
  // Safe moves and call backs between machines (SB-312).
  "The ordinary Claude Code or Codex here is signed in to this account. Sign it in to another account first.",
  "This sign-in is also saved in another group here. Remove or move that copy first, so only one machine renews it.",
  "The other machine's Switchboard did not answer over ssh. Nothing changed; check ssh and that Switchboard runs there.",
  "The other machine has no group with that id. Name one of its groups, or default.",
  "This machine renewed the sign-in while it moved, so the copy sent was no longer valid. It was removed there and stays here; move it again.",
  "The other machine's answer was lost. This machine no longer uses the account; look at the other machine's accounts and call it back or remove its copy there.",
  "The copy on the other machine could not be removed. This machine no longer uses the account; remove that copy there, then mark the account used here.",
  "This account was renewed here after it moved, so its copy there is no longer valid. Remove that copy there instead of calling it back.",
  "This account was not given back by a call back here, so its mark stays. Nothing changed.",
  "This is the only account its group uses on this machine. Add another account to that group before calling this one back.",
  // Where a data folder keeps its sign-ins (SB-287).
  "The sign-in store's choice (vault.json) could not be read. Delete vault.json to use this computer's own store, or choose again with switchboard vault use.",
  "This computer's own sign-in store works here, so Switchboard keeps sign-ins in it. A file store is only for a machine with no graphical login.",
  // The terminal hook (SB-183, node A-7).
  'Only the installed app changes terminal settings.',
  'The terminal hook\'s choice could not be saved in Switchboard\'s data folder.',
  // Organizations (SB-158).
  'Organization not found.',
  // Placing an account whose provider team a group would then refuse (SB-313).
  "A group this account or its provider team serves would then refuse it. Move it out of that group first, or change the group's organization.",
  // SB-323: `personal` named after an organization.
  "Name personal first, then the organizations it also works for: an account's personal context can only be its first.",
  'Another organization already has this name.',
  'Another organization already has this domain.',
  'A domain is a name such as example.com.',
  'Name the organization in 1 to 80 characters.',
  'An organization has at most 16 domains.',
  'At most 32 organizations.',
  // PassionCode.ai (SB-159).
  'PassionCode.ai was not connected: access was denied.',
  'PassionCode.ai did not answer in the browser within 5 minutes. Nothing changed.',
  'Connecting PassionCode.ai was cancelled. Nothing changed.',
  'PassionCode.ai could not be connected. Try again.',
  'PassionCode.ai is not connected.',
  'PassionCode.ai is already waiting for your browser.',
  'The PassionCode.ai connection could not be read. Disconnect and connect again.',
  'The browser could not be opened.',
  // A group's organization (SB-109).
  "This account belongs to another organization than this group. Move it, or change the group's organization.",
  "This account is in use on another machine. Use one of this machine's accounts, or clear the mark if it moved here.",
  "An account chosen for this group belongs to another organization. Choose accounts of the group's organization, or change it.",
  'This group still holds accounts that belong elsewhere. Move them out first.',
  // Project suggestions (SB-117).
  'No such suggestion. Read the suggestions again.',
  'This folder is gone. Read the suggestions again.',
  'Too many folders wait for an answer. Answer some in Switchboard first.',
  // Hermes's model and provider (SB-80): each names the fix.
  'Hermes is not installed. Install it from hermes-agent.nousresearch.com, then try again.',
  'Agent settings (agents-settings.json) could not be read. Delete agents-settings.json, then restore a backup to get the settings back, or leave it deleted to start over with every agent enabled and no default.',
  'No such agent in the catalog. Run switchboard agents list.',
  'Switchboard cannot hand work to this agent yet, so it cannot be the default.',
  'This agent is disabled for taking over work. Enable it on the Agents page, then try again.',
  'This agent is not installed on this computer. Install it, then press Refresh.',
  'OpenCode is not installed. Install it from opencode.ai, then try again.',
  'Name the account or agent to continue on (--account), or choose a default agent on the Agents page.',
  'The default agent has no account that can take this workflow now. Name one with --account, or choose another default agent.',
  'Hermes did not answer. Run hermes config check, then try again.',
  'A Hermes provider is an id such as openrouter, anthropic or nous (lowercase letters, digits and - . _ :).',
  'A model is an id such as moonshotai/kimi-k2 or claude-sonnet-4.5 (letters, digits and / . _ : -).',
  'Name a provider, a model or both.',
  // Kimi Code accounts (SB-81): each names the fix.
  'This Kimi Code account is already saved.',
  'This sign-in is already saved as an account.',
  'The sign-in folder could not be removed.',
  'Region is mainland-cn (kimi.com) or global (kimi.ai).',
  'Kimi Code account not found. Refresh the list and choose another.',
  'This account is signed out. Sign in to it again.',
  'A Kimi Code session still runs on this account. Close it in Terminal first.',
  'Kimi offers no usage figures for this account.',
  'Kimi is limiting requests right now. Switchboard asks again shortly.',
  'Kimi answered with something Switchboard cannot read. Try again later.',
  'Kimi returned no usage figures for this account.',
  'Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.',
  'Kimi did not answer. Try again in a minute.',
  'Kimi refused this sign-in. Sign in to the account again.',
  'Kimi Code is not installed. Install it from kimi.com/code, then try again.',
  'This Kimi Code sign-in is no longer waiting. Start it again.',
  'Finish the Kimi Code sign-in in Terminal first.',
  'The Kimi Code sign-in did not save a login. Start it again and complete it in the browser.',
  // Agent keys (SB-79): each names the fix.
  'That is not an OpenRouter key: it starts with sk-or-.',
  'A model is an OpenRouter id such as moonshotai/kimi-k2 (letters, digits and / . _ : -).',
  'OpenRouter refused this key. Check it on openrouter.ai/settings/keys, then save it again.',
  'OpenRouter did not answer. The key is saved; its balance shows when OpenRouter answers.',
  'This agent cannot launch on the OpenRouter key. `switchboard agents list` marks the ones that can.',
  'The switchboard command was not found. Install it from Agents, then launch again.',
  // Fallback chains (SB-71): each names the fix.
  'Name the agent by its catalog id (switchboard agents list).',
  'Give an executor an account or a key, not both.',
  'Name the key as its Observatory vault name: project/env/NAME.',
  'Claude Code and Codex run on saved accounts, not on a key.',
  'Another agent runs on an API-key account or a paid key, never on a subscription sign-in.',
  'Not a workflow id: it looks like wf_ and 16 hex digits.',
  'The same agent and account appear twice in the chain.',
  'Give a preset or a list of agents, not both.',
  'Unknown preset.',
  'Claude Code runs on a Claude account.',
  'Codex runs on a Codex account.',
  'A Kimi Code account runs only Kimi Code.',
  "This account's Kimi MCP settings (mcp.json) cannot be read, so Switchboard did not change them. Fix or remove that file, then try again.",
  'A chain holds at most 8 agents.',
  // Spending ceilings (SB-72, C-4): each names the fix.
  'Spending ceilings (spend.json) could not be read, so no paid agent is handed work until they are. Delete spend.json, then restore a backup to get the ceilings back, or leave it deleted to start over with no ceilings.',
  'Enter an amount from $0.01 to $1,000.',
  'Give this group its own key under API keys first.',
  'That group id names no group.',
  'Doubling a ceiling is answered in the Switchboard window.',
  // C-Q8: a paid start by hand (a launch from the window) refused by the spend gate.
  'This OpenRouter key has less than $0.50 left of its own limit, so no paid agent starts on it.',
  'A spending ceiling is reached, so no paid agent starts. Its question waits in the Switchboard window.',
  'A spending ceiling covers this start and its spend could not be read, so no paid agent starts.',
  // SB-213: the start's folder moved to another group (another key) after its gate read the key.
  'This folder moved to another group while its spend was checked, so no paid agent starts. Start it again.',
  'Doubling would pass $1,000, the most a ceiling may be. Set a ceiling of up to $1,000 instead.',
  'No open question with that id: it was answered, or its ceiling changed or was removed.',
  'At most 192 task ceilings. Clear one in the Switchboard window first.',
  'At most 63 group ceilings. Clear one in the Switchboard window first.',
  'The spending ceilings file is full. Clear a task ceiling in the Switchboard window, then try again.',
  // Launch in place (SB-75).
  'Launching in place runs on macOS and Linux for now.',
  'Agent arguments are at most 16 plain values of up to 1024 characters each.',
  'Select an account for this provider first (switchboard accounts select).',
  'Unknown agent. `switchboard agents list` names them.',
  'This agent is set up in its own config file; `switchboard agents connect` shows how.',
  'A folder sits inside a folder of another group. Add the repository itself to that group instead.',
  'That group does not hold this repository. Choose one of the groups it is in.',
  'Home folder unavailable. Choose a folder.',
  'Repositories could not be listed.',
  'Codex\'s config.toml could not be read or would not stay valid. Nothing was changed.',
  'Usage analytics are available in the installed app only.',
  'Could not save the analytics choice.',
  'Opening at login is available in the installed app only.',
  'Automatic updates work in the installed app only.',
  'Move Fabric Switchboard to the Applications folder to receive updates.',
  'Automatic updates are available on macOS, Windows and Linux.',
  'This copy was installed from the .deb package. Install the newer .deb from the releases page to update it.',
  'No update is ready to install yet.',
  'Could not check for updates. Switchboard tries again within the hour.',
  'Could not download the update. Switchboard tries again within the hour.',
  'The downloaded update did not pass its signature check and was discarded. Switchboard tries again within the hour.',
  'The update needs an administrator password to replace Switchboard in this folder. It was not installed.',
  'Could not install the update. Switchboard tries again within the hour.',
  'Installing the update did not finish. Quit and reopen Switchboard to try again.',
  'This version needs a step by a person before it installs; its release notes say what to do. Switchboard keeps the current version.',
  'Could not save the choice in Switchboard\'s data folder.',
  'Could not add Switchboard to the login items. Check the system\'s login item settings.',
  'Could not remove Switchboard from the login items. Check the system\'s login item settings.',
  READ_TIMEOUT,
  'Claude account lock was lost after credential write. Sign in through Claude before retrying.',
  'Provider CLI not found. Install the official CLI and retry.',
  'Sign-in is not complete. Finish in Terminal, then try again.',
  'Usage unavailable for this credential type.',
  'Provider rejected the credential. Sign in again.',
  'Storage unavailable. Check native credential storage access and retry.',
  'This account is disabled. Enable it before continuing.',
  'Enter valid credential JSON for the selected provider.',
  'Enter a credential before adding the account.',
  'This credential type is not supported by this provider.',
  'Close the existing session before changing its home.',
  'Select another account before removing this one.',
  'Switchboard could not end the sign-in in Terminal. Close its Terminal window, then cancel again.',
  'The sign-in is still opening in Terminal. Cancel again in a moment.',
  'The folder picker could not open. Try again.',
  'No terminal app was found. Install one, such as GNOME Terminal, Konsole or xterm, or set TERMINAL to yours, then retry.',
  'Linux credential storage is unavailable. Start a Secret Service keyring (GNOME Keyring, KWallet or KeePassXC) and unlock it, then retry.',
  'The Linux keyring is locked. Unlock it, then retry.',
  'Terminal launch is currently supported on macOS, Windows and Linux only.',
  'Usage unavailable. Check sign-in or try again after the next scheduled check.',
  'Current CLI account changed. Refresh before switching.',
  'Keychain unavailable. Unlock it and allow access, then retry.',
  'Current Claude account changed. Refresh the account list, then retry.',
  'Current Claude credential is unavailable; activation was cancelled.',
  'Current Claude account changed during activation. Try again.',
  'Claude account lock was lost after credential write. Sign in through Claude before retrying.',
  'Claude activation failed; previous account restored.',
  'Claude activation failed and rollback needs attention. Sign in through Claude before retrying.',
  'Claude is updating its account. Wait for login or refresh to finish, then retry.',
  // Safety-critical: the rollback could not confirm which Claude account is active. Shown verbatim.
  'Claude rollback lost its account lock. Check the current Claude sign-in before retrying.',
  'Claude account lock is unavailable. Check permissions of the Claude config directory.',
  'Choose an expiry in the future.',
  'This build has no bundled command-line tool.',
  '~/.local/bin/switchboard already points elsewhere. Remove it first.',
  '~/.local/bin/switchboard already exists and is not a link. Remove it first.',
  'Could not create ~/.local/bin.',
  'Keep switchboard.exe from the download folder, or add it to PATH.',
  'Finish an existing sign-in before starting another.',
  "This account's sign-in has ended. Sign in to it again.",
  "This session runs in a Switchboard home or a terminal in an organization's folder. Run this from another terminal.",
  "Switchboard has not stored this account's renewed sign-in yet. Retry in a minute.",
  'Claude Code is signed in to an account Switchboard has not saved; switching would sign it out. Add it first (In use now → Add to Switchboard, or switchboard accounts capture --provider claude).',
  'The Claude Code sign-in does not match the account named in its settings. Sign in again in Claude Code (claude /login), then retry.',
  'Switchboard could not confirm which account Claude Code is signed in to. Check the connection, or use Claude Code once, then retry.',
  'Sign-in not found. Start again.',
  'Claude Swap is updating this account. Retry in a moment.',
  'Switchboard could not start its account service. Retry; if it keeps failing, quit and reopen Switchboard.',
  'Keychain is locked or does not let Switchboard read this sign-in without asking. Unlock it and retry, or add the account with official sign-in.',
  'Keychain unavailable. Unlock it, then retry.',
  "Switchboard's account store is in use by another Switchboard (a second window, 'switchboard serve' or a command still running). Close it, then retry.",
  "Switchboard is renewing this account's sign-in. Retry in a moment.",
  'Usage checks are rate limited by the provider. Switchboard waits before the next one.',
  'This backup was made with another key and cannot be opened on this machine.',
  'Choose a backup from the backup folder.',
  'Backup not found. Refresh the list and choose another.',
  'This file is not a Switchboard backup.',
  'This backup is damaged and cannot be restored.',
  'Backups are written by the desktop app or switchboard serve.',
  'Backups are restored by the desktop app or switchboard serve.',
  'Backups belong to the default data folder; this data folder has none.',
  'Give a new label, an enabled state, or both.',
  'Backups are not available on this platform.',
  'Backup storage unavailable. Check the backup folder and Keychain access.',
  // Keychain migration and access outcomes (core keychain.rs ACTIONABLE): each names its own recovery.
  'This account was saved by an earlier Switchboard. Open the Fabric Switchboard app once so it can move the account to shared storage, then retry.',
  'Keychain access was not allowed, so this account stays where an earlier Switchboard saved it. Restart Fabric Switchboard and choose Allow when macOS asks.',
  'Keychain did not let this copy of Switchboard read the account without asking. Unlock the login keychain, or use Fabric Switchboard from Applications, then retry.',
  'This development build reads only accounts saved by the same build and never asks Keychain for access. Add the account again in this build, or use the signed app.',
  'The account could not be moved to shared storage and stays where it was. Retry, or restart Fabric Switchboard.',
  'Part of this account is stored where only the Fabric Switchboard app can remove it. Remove the account in the app.',
  // Sign-in, launch and session-home outcomes (runtime launch.rs).
  'Enter a label of up to 80 characters.',
  'Sign-in is not complete or its private file is unsafe. Finish in Terminal, then try again.',
  'Sign-in credential unavailable. Check Keychain access and finish login.',
  'Account home cleanup failed. Close its sessions and retry.',
  'Choose isolated or managed launch.',
  'Enable the account before launch.',
  'Usage unavailable. Check the account and retry later.',
]);
/**
 * SB-189 (CPY-03): the sentence a person reads for each backend refusal is the runtime's, shared with
 * the CLI and the MCP server (`crates/switchboard-runtime/src/person-errors.json`); the window shows
 * the same one. `safeErrors` above lists the backend sentences already written for a person.
 */
const coreErrors: Record<string, string> = personErrors.sentences;
/**
 * UX-08: a refusal outside the vocabulary above is never shown raw. The sentence names the kind of
 * operation that failed and its next step; `scripts/check-error-vocabulary.mjs` keeps every backend
 * sentence in the vocabulary, so this is the rare path.
 */
export type Operation = 'change' | 'read' | 'launch' | 'sign_in';
const fallbackErrors: Record<Operation, string> = {
  change: 'Switchboard could not finish this change. Try again; if it fails again, restart Switchboard.',
  read: 'Switchboard could not read this. Press Refresh to try again; if it fails again, restart Switchboard.',
  launch: 'The session did not start. Launch it again; if it fails again, restart Switchboard.',
  sign_in: 'The sign-in could not continue. Try again; if it fails again, restart Switchboard.',
};
export function safeError(error: unknown, operation: Operation = 'change'): string {
  const candidate = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
  if (safeErrors.has(candidate) || matchFormatted(candidate)) return candidate;
  if (Object.hasOwn(coreErrors, candidate)) return coreErrors[candidate];
  // A sentence the window already rewrote (a refusal shown, then handed back through safeError) stays as it is.
  if (Object.values(coreErrors).includes(candidate) || Object.values(fallbackErrors).includes(candidate)) return candidate;
  return fallbackErrors[operation];
}
