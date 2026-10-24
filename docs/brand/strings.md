Contract: brand-contract v1
# Interface strings
Status: implemented, operator outcome unobserved
Canonical actions: + Add account (its menu: Sign in to another Claude account, Sign in to another Codex account, Import from Claude Swap, Add an API key or token…), Add to Switchboard, Switch, Select, Launch… (with the *Isolated session* box), Check usage, Sign in again, Rename or disable…, Remove…, Cancel. They serve SCN-001..013, SCN-018..022 and SCN-032.
Source: [interface](../../src/main.ts), [window logic](../../src/ui-logic.ts), [safe backend vocabulary](../../src/adapter.ts), [backend sentences with a name](../../src/formatted-errors.ts), [update texts](../../src-tauri/src/updates.rs), [Russian dictionary](../../src/locales/ru.ts), [explicit demo messages](../../src/demo.ts).
Review decisions: selected means next request, launch requested is not authenticated success, usage unavailable is not zero, cancellation may require closing the official sign-in process first. Draft voice selected under autonomous design authorization; no operator voice calibration claimed.

CLI text uses the same verbs and failure distinctions (SCN-014..016). Native platform labels come from runtime metadata rather than hardcoded macOS. No Windows build is labelled provider-verified (SCN-017).

## How completeness is checked

Resynced 2026-10-24 against 0.6.81 (commit `b85aad1`), SB-66 and CPY-12 of the 0.6.82 iteration.
The window shows 1242 interface strings (`node scripts/check-locale.mjs` → "1242 interface
strings, all in Russian."). This file does not copy all of them: the English source of each is
its own key, collected from `t('…')` and `plural(…)` calls, `src/adapter.ts`'s verbatim and mapped
backend sentences, `src/formatted-errors.ts` and `src-tauri/src/updates.rs`, and every one has a
Russian entry in [src/locales/ru.ts](../../src/locales/ru.ts). `node scripts/check-locale.mjs --list`
prints the full list; the gate fails on a key without a Russian entry, a changed placeholder, a
plural without three forms or an em dash in either language.

What this file registers is each surface the window has, its scenarios, the strings that carry
its decisions (titles, actions, states, the sentences a person acts on) in English and, for the
navigation and the main actions, Russian, and the strings that were retired. A surface missing
here, or a string listed here that `--list` no longer prints, is drift: the registry is resynced
in the change that adds or retires it.

Vocabulary of 0.6.82 ([terminology](terminology.md)): a sentence says **group** («группа»), never
pool; the feature is **automatic switching** («автоматическое переключение», short form
«автопереключение»); the page of keys is **API keys** («API-ключи»). Strings below marked
*(0.6.81)* still carry the older word in 0.6.81 and change to the target in the window copy of
0.6.82 (node N5); the registry is then resynced to the shipped text.

## Registry by surface

### Shell and navigation (SCN-013, SCN-023, SCN-024, SCN-043)

| English | Russian |
|---|---|
| Switchboard · by PassionCode | Switchboard · by PassionCode |
| Accounts | Аккаунты |
| Tokens *(0.6.81)* → API keys | Токены *(0.6.81)* → API-ключи |
| Groups | Группы |
| Organizations | Организации |
| Agents | Агенты |
| Activity | Журнал |
| Statistics | Статистика |
| About / About Switchboard | О программе / О Switchboard |
| Refresh | Обновить |
| Retry | Повторить |
| Dismiss | Закрыть |
| Cancel | Отмена |

- Page subtitles: Accounts “Switch Claude Code, keep sign-ins fresh and watch quota.”; API keys “API keys agents run on: the machine's, and one per group.”; Groups “Groups of projects with accounts of their own; other groups never switch to them.”; Organizations “Companies your accounts belong to, by your word, their team at the provider or their address.”; Agents “Choose which coding agents take over work, and connect them to Switchboard.”; Activity “Local account and session events.”; Statistics “Which accounts and agents work in which groups, and where tokens go.”; About “Deliberate account switching for coding sessions.”
- Sidebar foot “LOCAL WORKBENCH”, “Your accounts. Your machine.”, then the version and platform; demo “SYNTHETIC DEMO · No real accounts, vault, proxy, or terminal. Changes reset when you reload.”, “Synthetic session”, “Browser demo”.
- Loading and failure: “Loading…”, “Working…”, “Loading your workbench”, “Reading account metadata from the native app…”, “Unable to load accounts”, “Unable to load groups”, “Unable to load activity”, “Open the native app”; startup timeout “The native app did not respond. Close and reopen Switchboard, then retry.” ([read deadline](../../src/read-deadline.ts)).
- Folder pickers (SB-86): “Choose folder…”, “Change…”, “No folder chosen”, “Add folder…”, “Remove {folder}”, “Choose a folder.”; backend “The folder picker could not open. Try again.”
- Copy buttons: “Copy”, “{label} command copied.”, “Copy is unavailable here. Select the command text instead.”

### First-run tour (SCN-035)

Five steps: “Welcome to Switchboard”, “Add your accounts”, “Switch, by hand or automatically”, “Give a group its own accounts”, “Agents, and it keeps running”; “Step {step} of {total}”, “Skip tour”, “Back”, “Next”, “Start using Switchboard”. About “Tour” · “Show the tour again”.

### Provider terms (SCN-049)

Banner on Accounts until dismissed, and About “Provider terms”: “Use Switchboard in line with the terms of Anthropic and OpenAI. Breaking a provider's terms can get your account blocked. You are responsible for how you use your accounts.”; action “Dismiss”. README and README.ru carry the same three claims.

### Accounts (SCN-001..010, SCN-018..022, SCN-025, SCN-028..032, SCN-062, SCN-072)

| English | Russian |
|---|---|
| + Add account | + Добавить аккаунт |
| Sign in to another Claude account / Codex account | Войти в другой аккаунт Claude / Codex |
| Import from Claude Swap | Импортировать из Claude Swap |
| Add an API key or token… | Добавить API-ключ или токен… |
| In use now · Add to Switchboard | Сейчас используется · Добавить в Switchboard |
| Switch | Переключить |
| Select | Выбрать |
| Select for managed sessions | Выбрать для управляемых сессий |
| Check usage | Проверить использование |
| Launch… · Isolated session | Запустить… · Изолированная сессия |
| Sign in again | Войти снова |
| Move to another pool… *(0.6.81)* | Перенести в другой пул… *(0.6.81)* |
| Organization… | Организация… |
| Used on another machine · Use here | Используется на другой машине · Использовать здесь |
| Rename or disable… | Переименовать или отключить… |
| Remove… | Удалить… |

- Card states (`CARD_STATE`): “Available”, “Running low”, “Limit reached”, “Stale”, “Check failed”, “Usage unknown”, “Sign in again”, “Disabled”, “No quota check”; badges “In Claude Code”, “In Codex CLI”, “On another machine”, the organization's name; primary slot “✓ In use”, “✓ Selected”.
- Quota cell and countdown (SB-39/40/41, SCN-032): “Resets {date}”, “Resets in”, “Back in”, “Retry in”, “ · estimated”, “Limit resets {date}”, “Retry hold until {date}”, “Next check {date}”, “Check failed · next {date}”, “Check failed · check usage to retry”, “Stale · reported reset {date}”, “Reset since check · awaiting a new one”, “Waiting for the first check”, “Not checked automatically · Quota checks need OAuth”, “Not checked while disabled”, “API billing”, “Reset time unavailable”, “Pause countdown” / “Resume countdown” / “Countdown paused”; quota windows “{name} · {percent}% used”, “{name} · usage unknown since reset”, “Reset {date} · {percent}% was used before”, “{feature} · {window} feature limit”; remaining time “{time} remaining”, “<1m remaining”, “Due · awaiting check” ([ui-logic](../../src/ui-logic.ts)).
- List headings: “Accounts the CLIs use now”, “{provider} accounts”, “Ordinary Claude Code: {label}”, “Ordinary Codex CLI: {label}”, “Group · {name}”, “Default pool” *(0.6.81)*, “Sessions in its folders use {label}”, “Managed sessions use {label}”, “Nothing selected here yet”, “Within each pool: remaining quota first, then shortest wait. Unknown usage follows.” *(0.6.81)*.
- Sign-in banner: “Signing in to {provider}”, “Finish in the Terminal window. If a browser opens, complete that step first. Switchboard adds the account on its own.”, “Adding the account…”, “Sign-in ended without an account”, “Sign-in cancelled. No account was added.”, “A sign-in is already in progress. Finish it in Terminal or cancel it first.”
- Results: “Claude Code now uses {label}. New claude sessions start on it; a running session may need a restart.” (never “switched live”), “{label} selected for the next managed request in {pool}.” *(0.6.81)*, “Account added. Select it when you are ready.”, “{identity} added to {provider} · default. Switchboard keeps its sign-in up to date.”, “{label} moved to group “{name}”.”, “{label} is marked as used on another machine. This computer no longer renews, rotates to or switches to it.” (SB-175).
- Automatic switching bar and dialog (SCN-022, SCN-029): “Automatic switching”, “Automatic switching is on” / “… is off”, “at {percent}% used”, “Turn on for Claude Code”, “Stop”, “Automatic switching settings”, “New policy…”, “Save policy”, “Turn on automatic switching”, “Switch at usage (%)”, “Headroom below threshold (points)”, “Cooldown (seconds)”, “Maximum usage age (seconds)”, “Automatic switching saved and turned on.”, “Automatic switching saved; it is off.”; decisions “Current usage is below the threshold.”, “No eligible account. Holding the current account.”, “Threshold reached. An eligible account is available; a switch is not yet confirmed.”, “Claude Swap is switching Claude Code automatically, so Switchboard does not. …”; renewal “Claude is refusing sign-in renewals for every account right now. …”.
- Launch dialog (SB-86): “Launch {label}”, “Folder”, “Isolated session”, notes “Runs in this account’s own home and keeps this account; automatic switching does not move it. Existing clients are not changed.” and “Goes through the local proxy: new requests use the selected account of this pool, and automatic switching can move it. In-progress responses keep their account.” *(0.6.81)*, “Launch”, results “Terminal launch requested in your project through the local proxy. Selection takes effect on the next request.”, “Terminal launch requested in your project with this account’s private home. Provider acceptance is not yet observed.”
- Add an API key or token: “Provider”, “Credential type”, “Account label”, “Pool” *(0.6.81)*, “API key”, “Claude setup token”, “OAuth JSON”, “Sent only to native credential storage. Cleared on submit or cancel.”, “Imported OAuth is a snapshot. Sign in again when it expires. Imported identity is not independently verified.”
- Remove: “Remove account?”, “Remove “{label}” from Switchboard and delete its stored credential. This cannot be undone.”, “Remove account”, “Account removed.”
- Active rules strip (SCN-025): “{n} project rules are active: {names}. Rotation still moves off an exhausted account.” *(0.6.81: “Rotation” changes to automatic switching)*, “Review rules”.

### Kimi Code accounts (SCN-047)

“Add Kimi Code account”, “Ordinary kimi: {name}”, “Ordinary kimi: not signed in.”, “Limit reached · resets in {time}”, “checked {time} ago”, “Open sign-in”, “Waiting for the sign-in in Terminal…”, “Kimi Code account saved.”, “kimi.com (mainland China)”, “kimi.ai (global)”, “Kimi offers no usage figures for this account.”, “Kimi is limiting requests right now. Switchboard asks again shortly.”, “A Kimi Code session still runs on this account. Close it in Terminal first.”

### Groups (SCN-034, SCN-042, SCN-055, SCN-056, SCN-061, SCN-063, SCN-068, SCN-070)

- “+ New group” («+ Новая группа»), “New group”, “No groups yet”, note “A group holds one or more projects (repository folders) with accounts of its own, and a project may be in several groups. Sessions launched from its folders use only its accounts, rotation stays inside them, and no other group or the ordinary Claude Code switches to them.” *(0.6.81: “rotation”)*.
- Card: “{n} accounts”, “{n} projects”, “Projects”, “Show all projects ({n})”, “Show fewer projects”, “Runs only on accounts of {name}.”, “Edit”, “Delete”, “No accounts yet, so sessions in these folders use your other accounts until you add one.”
- Dialog: “Group name”, “Folders”, “Accounts”, “Organization”, “Accounts you leave out move back to the default pool.” *(0.6.81)*, “Create group”, “Save group”, “Group “{name}” created. Launch its accounts from its folders.”, “Delete group?”, “Delete group”.
- Project rules (SCN-025): “Project rules”, “+ Add rule”, “No project rules”, “Active”, “Paused”, “Expired”, “Until {date}”, “No expiry”, “Pause”, “Resume…”, “Managed sessions (switch from the next request)”, “Claude Code login (changes every claude session)”, “Save rule”, “Remove rule?”, “Rule paused. Selection and rotation are unchanged.” *(0.6.81)*.
- Folder notifications (native, [suggestions.rs](../../src-tauri/src/suggestions.rs)): “{folder} is now part of {project}.”

### Codex in group folders (SCN-066)

“Codex in group folders”, “On: Codex goes through Switchboard.”, “On, but not in Codex's settings right now.”, “Off: Codex uses your own sign-in in every folder.”, “Groups with a Codex account: {names}.”, “No group has a Codex account yet. Move one into a group first.”, “Turn on”, “Turn off”, “Codex uses your own sign-in again.”

### Repositories on disk (SCN-064)

“Repositories on disk”, “Show repositories” / “Hide repositories”, “Every Git repository under the folders you choose, with the groups it is in. Add one to a group, or move it with its worktrees and clones to another.”, “Look under”, “~/DATA, or your home folder”, “Look again”, “Not in a group ({n})”, “All ({n})”, “Every repository here is in a group.”, “No Git repositories here.”, “Add to group…”, “Move…”, “Also add…”, “Choose where “{name}” goes. Its worktrees and clones go with it.”, “{name} added to group “{project}”.”, “{name} moved to group “{project}”.”

### Organizations (SCN-069, SCN-070, SCN-071)

“+ New organization” («+ Новая организация»), “No organizations yet”, “Show accounts of”, “All”, “Personal”, “Company name”, “Domains”, “Optional. Separate several with commas.”, attribution “placed by you”, “by its team”, “by its domain”, “You placed it in {name}.”, “Its team at the provider belongs to {name}.”, “Its address is on a domain of {name}.”, “Organization of {label}”, “Automatic follows its team at the provider, then its address’s domain.”, “Remove {name}?”, “Its accounts stay where they are; they are placed again by their team or domain, or become personal. Nothing at the provider changes.”; PassionCode.ai (SB-159): “Connect PassionCode.ai…”, “Waiting for PassionCode.ai in your browser…”, “Connected to PassionCode.ai: {organizations}.”, “Read now”, “Disconnect”, “PassionCode.ai disconnected. The organizations stay here.”, “Its name and domains come from PassionCode.ai.”, “PassionCode.ai did not answer in the browser within 5 minutes. Nothing changed.”

### Agents (SCN-026, SCN-036, SCN-046, SCN-048, SCN-050, SCN-057..060)

“Default agent”, “Installed”, “{n} detected”, “Enabled” / “Disabled”, “Default”, “Set default”, “Tools only”, “Set up”, “Details”, “Available to install”, “{agent} takes over work”, “{agent} no longer takes over work.”, “The default agent is Auto.”, “Connect an agent”, “Set up with your coding agent”, “Prompt for your agent”, “Or register the tools yourself”, “Link switchboard into ~/.local/bin”, “The switchboard command-line tool was not found on PATH.”, “Change Hermes's model or provider”, “Hermes settings saved.”, “Set up {agent}”, “Launch in folder”, “Launch on OpenRouter”, “On the OpenRouter key”, “Terminal launch requested for {agent}.”, “Terminal launch requested for {agent} on OpenRouter.”

### API keys (SB-144, SCN-045, SCN-065; nav and title “Tokens” in 0.6.81)

“OpenRouter keys”, “Machine key · OpenRouter”, “Group keys”, “Give a group its own key, for example a client's, and agents started in its folders spend that key; the others keep the machine's.”, “Own key · {model}”, “Uses the machine's key”, “No key: agents here cannot start on OpenRouter”, “Add key…”, “Change model”, “Replace key”, “Remove key”, “Add an OpenRouter key”, “Create a key on openrouter.ai/settings/keys; a spending limit on it caps what agents can spend.”, “Starts with sk-or-. Switchboard never shows it again.”, “Save key”, “Recommended models”, “{n} OpenRouter models that can call tools: type to search, or enter any id.”, “OpenRouter's list is unavailable now; the recommended models and typed ids still work.”, platform notes “The key stays in the Keychain; an agent receives it only when it starts.” (Windows Credential Manager and Secret Service variants); pointer from Agents “The machine's OpenRouter key and each group's own key are under Tokens.” and “Open Tokens” *(0.6.81: “API keys”)*.

### Statistics (SB-145, SB-147, SB-157, SCN-067)

“Requests per day”, “Tokens per day”, “{metric} per day by group”, “Show”, “Period”, “{n} days”, tiles “Sent”, “Received”, “From cache”, “Per request”, “tokens on average”, “Limits and errors”; tables “By group”, “By project”, “By account”, “By agent”, “By organization”, “By day”, “Who works where”, “No folder recorded”, “Removed account”, “Removed organization”; empty and failure “Nothing counted yet”, “No traffic in this period.”, “Statistics are unavailable. Refresh to retry.”, “Reading statistics…”; keys spend “API keys”, “Today”, “In all”, “Limit”, “Not read”, “No OpenRouter key is saved. Add one under Tokens.” *(0.6.81)*; “Counted since {date}.” Tokens here are counted usage only ([terminology](terminology.md)).

### Activity (SCN-011, SCN-051)

“Sessions”, “Running”, “Starting”, “Ended”, “State unknown”, “{how} · started {date}”, “Runs in {name}'s context.” (SB-110 phase 1, SB-111), “No session Switchboard launched is running. Sessions started outside Switchboard are not listed.”, “Sessions are unavailable. Refresh to retry.”, “The list may be out of date: the last read failed.”, “Events”, “A bounded local journal. A terminal launch is not proof of a successful provider request.”, “No activity yet”. Journal names ([ui-logic](../../src/ui-logic.ts)): “Account added”, “Usage check”, “Managed request”, “Proxy”, “Project rule”, “Fallback chain”, “Agent key”, “Hand-over”, “Sign-in”; outcomes in lower case (“done”, “observed”, “rate limited”, “HTTP {status}”…).

### About (SCN-030, SCN-033, SCN-037, SCN-041, SCN-044)

- “Running in the background”, “Open at login, in the background”, “Starts without a window; open it from the menu-bar icon.” (Windows and Linux: “… from its icon in the notification area.”); tray “Open Switchboard”, “Quit Switchboard”.
- Updates (SB-55, SB-77, texts from [updates.rs](../../src-tauri/src/updates.rs)): “Install updates automatically”, “Check for updates”, “Checking for updates…”, “Downloading version {version}…”, “Version {version} is the latest. Switchboard checks again every five minutes.”, “Version {version}. Automatic updates are off.”, “Version {version} is ready. It starts on its own when Switchboard is idle in the background or opens again, or restart now.”, “Restart to update”, “This version needs a step by a person before it installs; its release notes say what to do. Switchboard keeps the current version.”, “Automatic updates work in the installed app only.”
- Analytics (SCN-044): “Usage analytics”, “Share anonymous usage counts”, “Switchboard counts installs, days of use and how many accounts are saved, by provider and type, to help PassionCode.ai improve its apps. …”, “This switch applies to every PassionCode.ai tool on this computer.”
- Backups (SCN-030): “Backups”, “Back up now”, “Backup saved.”, “Restore”, “Backup restored.”, “No backups yet.”, “made on another computer”, “{n} accounts restored”, “{n} already here”, “{n} could not be restored”; platform notes on where the key stays (Keychain, Windows, Secret Service).
- Language (SCN-041): “Language”, “Same as system”, “English”, “Русский”, “The language could not be saved on this machine.” Appearance: “Appearance”, “System”, “Dark”, “Light”, “{theme} appearance applied.” Version and license: “Version and license”, “Version”, “License”, “Third-party”, “Toolkit”, “Part of the PassionCode.ai toolkit.”, and the license line “Open source under the GNU AGPL-3.0; a commercial license is available at passioncode.ai/business.”

### Backend sentences (B-04, L10N-04)

About 320 of the 1242 keys are backend sentences the window shows verbatim (`safeErrors`) or maps (`coreErrors`, the generic fallback “The operation could not be completed…”), plus the templates with a name in [formatted-errors.ts](../../src/formatted-errors.ts). Each names the cause and the next action; none carries a raw provider response or a secret. Examples: “Finish an existing sign-in before starting another.”, “This account's sign-in has ended. Sign in to it again.”, “Another Switchboard window owns the account storage. Close that instance, then retry.”, “Linux credential storage is unavailable. Start a Secret Service keyring (GNOME Keyring, KWallet or KeePassXC) and unlock it, then retry.”, “Backups belong to the default data folder; this data folder has none.”

## Retired strings

No longer shown by the window (absent from `node scripts/check-locale.mjs --list` at `b85aad1`); listed so a doc or test that quotes one is caught:

- Projects as the page name (0.6, replaced by Groups, SB-146): “+ New project”, “Give a project its own accounts, and keep other projects off them.”, “Project name”, “Create project”, “Save project”, “No projects yet”.
- Tour: “Step {n} of 5” (now “Step {step} of {total}”).
- Sign-in banner: “Signing in to Claude Code” (now “Signing in to {provider}”); “Finish sign-in” (the window finishes on its own); “Finish or close sign-in in Terminal before cancelling.” and its Kimi variant (SB-83).
- Updates: “Version {current} is the latest. …” (now `{version}`), “Update check finished.”
- Backups: “{n} accounts restored · {k} already here” (now “{n} accounts restored”, “{n} already here”).
- Quota: “Limit reached · until {time}”; badge “Next managed request”.
- Row menu: “Launch isolated…”, “Launch managed…” (one “Launch…” with the *Isolated session* box, SB-86).
- 0.3 actions: “Capture current account”, “Import profiles”, “Activate in Claude Code”, “Configure rotation”, “Stop rotation”.
- SessionStart line before 0.6.51 (SB-146, commit `7c02122` removed it): “Switchboard: this Git folder is now the project “{project}”; …” (see the CLI section below).

## Rules the strings keep

- Selected means the next request; a launch requested is not authenticated success; usage unavailable is never zero; “Claude Code now uses …” never says the running session switched live.
- One form per kind of state in Russian: loading «Загрузка …», waiting «Ожидание …» / «Ожидается …»; Switchboard is named in the third person, never «мы»; Terminal is «Терминал»; “Refresh to retry” is «Нажмите «Обновить», чтобы повторить».
- An identity is «владелец аккаунта», a credential «учётные данные»; account-name placeholders take a noun for agreement («Аккаунт {label} включён»).
- No em dash in either language (operator rule 2026-10-18; checked by `scripts/check-locale.mjs`).
Humanization: on, own operational-language pass at each resync; no decorative or promotional wording.

## Recovery CLI and console hand-overs, 2026-10-13

Surface: product-ui (CLI). Status: proposed. SCN-053 and SCN-054.
`recovery.rs` owns invalid/conflicting stop, unreadable registry and no-longer-ready
errors; `handovers.rs` owns invalid id, unavailable offer and failed queue writes.
The CLI uses "register", "claim" and "finish" for distinct host actions; it never
calls a claim "resumed". No new renderer strings or locale keys.
Humanization: on, own operational-language pass; no decorative or promotional wording.

## SessionStart lines (CLI), since 0.6.32 (SB-121)

Surface: product-ui (CLI SessionStart). Status: implemented. SCN-055, SCN-056.
“Switchboard: this folder was added to the project "{project}"; its sessions use the project's accounts from their next start.” and “Switchboard: this folder looks like part of the project "{project}" but could not be added yet; Switchboard tries again by itself.” ([main.rs](../../crates/switchboard-cli/src/main.rs), the SessionStart hook). The line names the effect only; under the 0.6.82 vocabulary “project” here means the group (node N2 owns the wording).
Humanization: on, own operational-language pass; no decorative or promotional wording.
