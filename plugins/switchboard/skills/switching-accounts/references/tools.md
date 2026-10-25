# Switchboard MCP tools: reference

Server `switchboard`, started as `switchboard mcp` (stdio, newline-delimited JSON-RPC 2.0,
MCP `2025-06-18`, also `2025-03-26` and `2024-11-05`). Every result carries
`structuredContent` and the same JSON as one text block. A refused call returns
`isError: true` with one fixed sentence, never a stack trace or a provider body. No tool
accepts or returns a credential. Launch is not exposed; sign-in is begun here
(`switchboard_login`, `switchboard_kimi_login`) and the person signs in.

Words: the person reads **group** for what tool names and fields call a project, and a
group's id is its `pool` (`default` is the default group). **Automatic switching** is what tool
names and fields call rotation. Say group and automatic switching to the person.

`switchboard mcp --read-only` lists only the 23 tools that read (every tool under
[Read tools](#read-tools)) and none of the 22 that write. A session Switchboard launches in
isolated mode gets that set. The server lists 45 tools; each has a section below.

## Contents

- [Session detection](#session-detection): how the server knows which session asks
- [Read tools](#read-tools): status, accounts, usage, groups, statistics, chains, agents
- [Write tools](#write-tools): switch, groups, folder rules, automatic switching, sign-in
- [Refusals](#refusals): fixed error texts and their remedies

## Session detection

The server works out which session asks from its own environment, first match wins:

1. `SWITCHBOARD_SESSION` = `managed:<provider>:<pool>` or `isolated:<provider>:<account id>`,
   written by a Switchboard launch.
2. `ANTHROPIC_BASE_URL` = `http://<running proxy address>/claude/<pool>`, only when the
   address is the proxy that is running now, so a stale variable cannot claim a pool.
3. `CODEX_HOME` = the app's `runtimes/codex-<pool>` folder.
4. Otherwise `native`; `provider` is `claude` inside Claude Code, else null (both providers).

## Read tools

Annotations: `readOnlyHint: true`.

### `switchboard_status`: no arguments

| Field | Meaning |
|---|---|
| `runtime.online`, `runtime.platform`, `runtime.note` | whether the desktop app or `switchboard serve` owns the proxy; offline means managed sessions cannot route and automatic switching is paused |
| `session` | `{mode: "managed", provider, pool}`, `{mode: "isolated", provider, account_id}` or `{mode: "native", provider}` |
| `current_cli_accounts` | the accounts signed in to the ordinary CLIs now, or null when unreadable |
| `routes` | `"<provider>:<pool>" → {account_id, label}`: who handles the next request in each group |
| `rules.active`, `rules.total`, `rules.for_this_folder` | active rules (`path`, `provider`, `target`, `state`, `expires_at` in RFC 3339, `account`), the count of all rules, and the resolution for the server's folder |
| `rotation[]` | automatic switching per group: `provider`, `pool`, `target`, `enabled`, `threshold_percent`, `last_decision` |
| `limited[]` | accounts held after a provider limit error: `account_id`, `until` (when it may be tried again, Unix seconds), `resets_at` (the provider's own reset, or null when `until` is an estimated hold; a reset more than seven days out is held seven days, so `until` can be earlier), `kind` (`quota` when Claude Code named a subscription window (see `limit_type`), else `unknown`), `limit_type`, `confidence` (`attributed` for a managed request, `inferred` for a Claude Code transcript), `reason` (why the provider would not let the account go on, from Claude Code's own report: `org_spend_cap_reached`, `group_zero_credit_limit`, `org_level_disabled`…; null when not said), `hidden` (true when the provider refused while its usage figures from the last ten minutes still showed room: a spend or credit cap those figures do not report), `scope` (`unknown`: a shared organisation budget is not ruled out), `source`, `observed_at`, `event_id`. A hold is not a reset; never present `until` as one when `resets_at` is null |

### `switchboard_accounts`: `provider?: "claude" | "codex"`

`accounts[]`: `id`, `label`, `provider`, `kind` (`oauth`, `api_key`, `setup_token`), `pool` (the group id),
`enabled`, `selected_for_next_request`, `signed_in_cli`, `lowest_remaining_percent`,
`usage_fresh`.

### `switchboard_usage`: `account_id?`, `refresh?: bool` (default false)

`accounts[]` per account: `id`, `label`, `provider`, `pool`, `health`, `checked_at`,
`next_check_at`, `known`, `fresh`, `lowest_remaining_percent`, `windows[]`
(`name`, `scope`, `used_percent`, `remaining_percent`, `resets_at`), `observed_at`, `age_seconds`,
`source`, `reason`. Times are RFC 3339.

- `known: false`: no observation yet, or an API-key account (quota is reported for OAuth
  subscription accounts only). `lowest_remaining_percent` is then null: unknown, not zero.
- `scope: feature` marks a window that limits one metered feature of a Codex account (named
  `feature_<feature>_primary|secondary`); `scope: account` windows (including `spend_limit`,
  `workspace_credits`, `workspace_usage_limit`, `limit_reached`) limit the whole account.
  `lowest_remaining_percent` covers account windows only, and is null with `known: true` when
  the provider reported nothing but feature limits: unknown, not zero.
- `fresh` is false when the observation is older than the group's freshest enabled automatic switching
  setting allows (900 seconds without one), when an account window's reset time has passed, or when
  the last check failed.
- `refresh: true` needs one `account_id` and asks the provider. For an inactive Claude account
  whose token has expired, Switchboard first renews that sign-in itself (0.5); the account the
  ordinary Claude Code uses is never renewed, and no credential reaches the agent. Within 60 seconds of the
  last check it answers with the earlier observation and `note` = "Checked less than a
  minute ago; showing that observation." A read-only server (`switchboard mcp --read-only`)
  never asks the provider: it answers with the stored observation and `note` = "This server is
  read-only: showing the stored observation without asking the provider."

### `switchboard_sessions`: `all?` (boolean)

`{sessions[]}`: the Claude Code, Codex and Kimi Code sessions Switchboard launched: `account_id`,
`label`, `provider`, `pool` (the group id), `context` (`personal` or `org:<id>`, fixed at the session's start;
absent before 0.6.80 and on Kimi Code, Hermes and OpenCode sessions), `context_name` (the organization's name), `mode`, `working_directory`,
`in_place`, `launched_at`, `started_at`, `continues`, `home` and `state` (`starting`,
`running`, `ended`, `unknown`).
Without `all`, only starting and running ones. A session started outside Switchboard is not
listed.

### `switchboard_onboarding`: `path?` (absolute folder), `host?` (`claude` or `codex`)

Where setting Switchboard up stands: `{complete, host, folder, steps[]}`, steps in order
`owner`, `accounts`, `project` (the folder's group), `mcp`, `proving_call`, each `{id, state, summary, next}`.
`state` is `done`, `todo` (run `next.command`, or call `next.tool`), `needs_person` (tell
the person `next.person`) or `optional`. Reads only; the same as
`switchboard onboard --status --json`. The guide is `switchboard onboard`.

### `switchboard_project_context`: `path?` (absolute folder)

`{path, project, rules[]}`: `project` is the group the folder belongs to (`name`, `pool` as its id,
`folders`, reserved `accounts`), or null; `rules` has one entry per provider: `effective` (the rule in force, or null),
`nearest` (the nearest rule in any state, including paused or expired) and
`managed_route_in_effect` (whether the rule's account is already selected in its group).
A rule view carries `path`, `provider`, `target` (`managed` or `claude_cli`), `enabled`,
`state` (`active`, `paused`, `expired`), `created_at` and `expires_at` in Unix seconds,
and `account` (`id`, `label`, `provider`, `pool`, `enabled`).

The folder defaults to `CLAUDE_PROJECT_DIR` when set, else the folder the server started
in. Pass `path` whenever the work moved to another folder.

### `switchboard_projects`: no arguments

`{projects[], rules[]}`: every group with its id (`pool`), `name`, `folders`, `roots` (each root
folder with its branch and the worktrees and clones under it), `sources`, reserved `accounts`
and `organization` / `organization_name`; then every folder rule with its `state`.

### `switchboard_repository_map`: `under?` (absolute folders, at most 16)

The Git repositories under those folders (default `~/DATA`, else the home folder; three levels,
never into a repository, a hidden folder or dependencies), each with its group or none.

### `switchboard_statistics`: `days?` (1 to 90, default 30)

What the proxy relayed: requests, outcomes (`ok`, `limited`, `failed`) and the provider's token
counts by group, project, account, coding agent and day. Managed traffic only; counts, never a
prompt or path. Out of range: "Statistics cover 1 to 90 days."

### `switchboard_events`: `limit?` (1 to 500, default 50)

`{events[]}`, newest first: fixed codes for accounts, switches, groups, automatic switching and
sign-ins. No credential, prompt or provider text.

### `switchboard_organizations`: no arguments

The organizations (domains, account count), every account with its organization (null:
personal) and `by` (`person`, `provider` or `domain`), and `passioncode`: the PassionCode.ai
connection's state. Empty until the operator adds one.

### `switchboard_rotation`: no arguments

`{policies[], monitor}`: automatic switching per provider, group (`pool`) and target (`enabled`,
`threshold_percent`, `hysteresis_percent` as headroom, `cooldown_seconds`, `max_age_seconds`)
and the background monitor's last decisions; with the app stopped, `monitor.running: false`.

### `switchboard_backups`: no arguments

The backup folder, whether backups run, the newest backups (file, time, account count) and the
last error. Never their contents.

### `switchboard_recovery`: no arguments

Sessions that stopped on a limit and the accounts ready to continue them: each stop's `mode`
(`capacity` or `probe`) and `probe_at`, and for a waiting stop that is not ready its `reason`
(`billing`, `account_removed`, `no_account`, `backoff`).

### `switchboard_handovers`: no arguments

`{enabled, handovers[]}`: whether hand-overs open in Fabric's console, and those waiting for a
console, oldest first.

### `switchboard_chain_get`: `workflow_id?`, `pool?`

`{chains[], effective, presets[]}`. A chain is `{scope, executors[], updated_at}`: `scope` is
`{kind: "machine"}`, `{kind: "project", pool}` (a group) or `{kind: "workflow", id}`; each executor is
`{agent, account_id?, key?}`: a catalog agent id, optionally pinned to an account or to a paid
key by its Observatory vault name (`project/env/NAME`; never a value). `effective` is the chain
that applies (the workflow's own, else the group's, else the machine's) or null: **no chain
applies until the operator sets one**. `presets` lists ready-made orders
(`subscriptions-first`: claude-code → codex → kimi-code → hermes).

### `switchboard_openrouter_status`: `pool?`

`{service, saved, saved_at, model, credit, credit_error}` for the OpenRouter key agents run on.
`credit` is what the key may still spend, asked of OpenRouter at this call: `label`, `limit`,
`limit_remaining`, `limit_reset`, `usage`, `usage_daily`, `is_free_tier`. A refused key reports
`credit_error` and stays saved; OpenRouter not answering does the same with its own text. The
With `pool`, that group's own key; `keys` lists every saved key with its group and `projects`
the groups with or without one. The key's value is never returned: an agent that runs on it gets it from its launch environment,
not from this server.

### `switchboard_openrouter_models`: no arguments

OpenRouter's public model list (kept six hours), recommended first: `id`, `name`, maker, context
and price per million tokens. No key is used.

### `switchboard_hermes_status`: no arguments

`{installed, configured, model, provider, base_url, api_mode, providers, error}` for the
operator's ordinary Hermes Agent, read with `hermes config get model --json`. Only those four
keys are kept (`base_url` without query or credentials); `providers` lists the common provider
ids the app offers. `error` names a Hermes that did not answer.

### `switchboard_agents`: no arguments

`{schema, revision, updated_at, default, default_ready, installed, agents: [...], recommended_models}`. `default` is the machine's default
agent, `null` for *Auto*; Fabric uses it unless a group sets its own. Each agent row: `id`,
`name`, `maker`, `url`, `install` (its install command), `installed` (found on this computer
now), `switchable` (Switchboard can hand it a workflow: Claude Code, Codex, Kimi Code, Hermes,
OpenCode), `enabled` (switchable, installed and not disabled by the operator), `disabled`,
`default`, `openrouter` and `mcp` (what it supports), `path` (the program found) and `command`
/ `argv` (what Switchboard runs for it, never a key). `revision` changes whenever the answer does. `recommended_models`: `[{id, name, maker, context,
input_usd_per_million, url}]`. Enabling, disabling and the default are the operator's (the
Agents page, or `switchboard agents enable|disable|default`).

### `switchboard_kimi_accounts`: no arguments

`{accounts: [{account, status}], current, installed}`. `account`: `id`, `label`, `region`
(`mainland-cn` or `global`), `added_at`, `nickname`, `tier` (the membership level). `status`:
`signed_in`, `nickname`, `tier`, `windows` (`[{name: "5h" | "7d" | "month" | "month_code",
used_percent, resets_at}]`, asked of Kimi at this call) and `error` when Kimi refused the login,
did not answer, or the login waits for Kimi to renew it (launching the account once does).
`current` is the ordinary `kimi` (`~/.kimi-code`) the same way, shown and never switched. No
token is returned or handed to any agent; signing in, launching and removing accounts are the
operator's (the app, or `switchboard kimi login|launch|remove`).

### `switchboard_project_suggestions`: no arguments

Folders still joining a group (retried by themselves, never asked) and those added or taken out
as deleted in the last day, each with its group and why (worktree, clone, worked in).

### `switchboard_project_bindings`: no arguments

Each group folder whose Claude Code sessions are routed to the group's accounts (its
`.claude/settings.local.json`), with its state. Never a capability value.

## Write tools

Annotations: `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: true`.

### `switchboard_switch`: `account_id`, `target?`, `global?`

| target | Result `action` | Notes |
|---|---|---|
| `session` (default) | `selected` | the calling session must be managed, same provider, same group |
| `route` | `selected` | selects the account in its own group for every managed session of that group |
| `claude_cli` | `activated` | requires `global: true`; changes the ordinary Claude Code login machine-wide |

Result: `{action, account: {id, label, pool}, message}`. A response already streaming
keeps its account; a manual choice starts the automatic switching cooldown.

### `switchboard_project_set` (a folder rule): `account_id`, `path?`, `target?`, `enabled?`, `expires_in_hours?`

Only when the operator asks. `target` is `managed` (default) or `claude_cli`;
`expires_in_hours` is 1 to 720. The folder must exist and cannot be inside Switchboard's
own data folder. Result: `{rule, message}`; the message reminds that the rule is visible
in the app under Groups and can be paused there.

### `switchboard_project_remove`: `provider`, `path?`

Removes the rule saved for exactly that folder and provider (not a parent folder's rule).
A folder that no longer exists can still have its rule removed. Result: `{removed}`.

### `switchboard_project_apply`: `path?`, `global?`

Result: `{path, session, results[]}`, one entry per provider this session can use, each
`{provider, action, account, message}`. Actions: `selected`, `activated`,
`already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool`,
`other_session`, `needs_global`, `failed`. Nothing changes without a rule in force.

| Rule target × session | Outcome |
|---|---|
| `managed` × managed, same group | `selected`, or `already_in_effect` when already selected |
| `managed` × managed, other group | `other_pool` |
| `managed` × native | `other_session` |
| `claude_cli` × managed | `other_session` |
| `claude_cli` × native | `already_in_effect`, `needs_global` without `global: true`, else `activated` |
| any × isolated | `other_session` |
| rule's account removed, or the change refused | `failed` |

### `switchboard_project_edit`: `pool`, `name?`, `organization?`, `add_accounts?`, `remove_accounts?`, `add_folders?`, `remove_folders?`

Only when the operator asks. Changes a group by delta; anything not named stays. A name the
group does not hold is refused; `organization` is an id, a name or `personal`. Result:
`{project, message}` ("Group saved. It is visible in the Switchboard app under Groups.").

### `switchboard_project_move_root`: `folder`, `pool`, `also?`, `from?`

Only when the operator asks. Moves a repository with its worktrees and clones into the group
`pool`, or with `also` adds it to one more group; `from` names the group it leaves. `also` with
`from` is refused ("Name either also or from, not both.").

### `switchboard_codex_route`: `enabled?`

Without `enabled`, the state (`on`, `applied`, `conflict` and the groups with a Codex account);
`true` or `false` changes whether Codex in group folders uses the group's accounts, only when the
operator asks.

### `switchboard_project_suggestion`: `action`, `folder`

Only when the operator asks. `accept` adds a waiting folder to its group now; `dismiss` keeps it
out for good. `folder` as `switchboard_project_suggestions` lists it.

### `switchboard_project_sources`: `pool`, `add?`, `remove?`

The repository owners (`github.com/example`) whose new repositories join the group by
themselves. Without `add` or `remove`, reads them; changing them only when the operator asks.

### `switchboard_account_update`: `account_id`, `label?`, `enabled?`, `elsewhere?`

Only when the operator asks. Renames, enables or disables an account; `elsewhere: true`
disables it here because another machine uses it. At least one field ("Name a label, enabled
or elsewhere to change.").

### `switchboard_account_move`: `account_id`, `pool`

Only when the operator asks. Moves an account into the group `pool`, or back with `default`;
the account the ordinary Claude Code or Codex uses cannot join a group.

### `switchboard_organization_set`: `action`, `id?`, `name?`, `domains?`, `account_id?`, `organization?`

Only when the operator asks. `action` is `add` (name, domains), `edit` (id; name and/or domains,
which replace the list), `remove` (id), `assign` (account_id, organization: an id, `personal` or
`auto`) or `passioncode_read` (read the PassionCode.ai organizations again; connecting is the
person's).

### `switchboard_rotation_set`: `provider`, `pool?`, `target?`, `enabled?`, `threshold_percent?`, `hysteresis_percent?`, `cooldown_seconds?`, `max_age_seconds?`

Only when the operator asks. Changes automatic switching for one provider, group and target
(`managed` or `claude_cli`); every field left out keeps its saved value.

### `switchboard_backup_now`: no arguments

Writes an encrypted backup now. Restoring stays the person's (the app, or
`switchboard backup restore`).

### `switchboard_agent_set`: `agent?`, `enabled?`, `default?`

Only when the operator asks. `enabled` (with `agent`) lets Switchboard hand work to that agent
or keeps it away; `default` sets the machine's default agent (an id, or null for Auto).

### `switchboard_login`: `action`, `login_id?`, `provider?`, `label?`, `pool?`

Only when the operator asks. `begin` (provider, label, pool as the group id) opens Terminal on
the official sign-in; the person signs in. `status`, `finish` and `cancel` take the `login_id`
`begin` returned. No password or key passes through this tool.

### `switchboard_kimi_login`: `action`, `id?`, `label?`, `region?`

Only when the operator asks. `begin` (label, region `mainland-cn` or `global`) starts the
official Kimi Code sign-in, finished by the person in the browser; `status`, `finish`, `cancel`
and `again` (sign a saved account in again) take `id`.

### `switchboard_kimi_rename`: `id`, `label`

Only when the operator asks. Renames a saved Kimi Code account.

### `switchboard_handovers_set`: `on`

Only when the operator asks. `true`: hand-overs open in Fabric's console; `false`: straight in
Terminal.

### `switchboard_chain_set`: `scope?`, `executors?` | `preset?`

Only when the operator asks. `scope` is `machine` (default), `project:<group id>` or
`workflow:<wf_id>`; `executors` in order, first tried first, at most 8; an empty list clears the
scope. An agent must load MCP servers and take a prompt without a person (catalog `mcp.supported`
and `headless`), else *“… cannot take over a workflow …”*. Claude Code and Codex run on their
provider's accounts; any other agent on an API-key account or a paid key, never on a
subscription sign-in. Returns `{chain}` (null when cleared).

### `switchboard_hermes_set`: `provider?`, `model?`

Only when the operator asks. Runs Hermes's own `hermes config set model.provider` (first) and
`model.default`, then returns the status. Ids are checked before Hermes runs: a provider is
lowercase letters, digits and `- . _ :`; a model is letters, digits and `/ . _ : -`. Changing the
provider makes Hermes drop an endpoint that belonged to the previous one; the new provider needs
its own key inside Hermes, which this server never sets.

### `switchboard_openrouter_model`: `model`, `pool?`

Only when the operator asks. `model` is an OpenRouter id (`vendor/model`, for example
`moonshotai/kimi-k2` or `anthropic/claude-sonnet-4.5`); agents launched on the operator's key
start on it; with `pool`, the model of that group's own key. Refuses a malformed id. Saving or removing the key itself is the operator's own
step (the app, or `switchboard agents openrouter set|remove`), never this server.

## Refusals

Fixed texts the server returns with `isError: true`; relay them and offer the remedy:

| Text begins | Remedy |
|---|---|
| "This changes the Claude Code login for every ordinary claude session…" | ask the operator, then call again with `global: true` |
| "This session uses another provider." | choose an account of the session's provider |
| "This session routes group …; that account is in group …" | choose an account in the session's group, or launch a managed session in the other group |
| "This session is isolated to one account…" | launch a managed session to switch inside a session |
| "This session does not run through Switchboard's proxy…" | `target: "route"` to prepare a group, or `claude_cli` with `global: true` after the operator's OK |
| "No account with id …" | call `switchboard_accounts` for current ids |
| "Pass an absolute folder." | pass an absolute `path` |
| "expires_in_hours must be 1 to 720." | pick an expiry in range |
| "Choose an existing project folder." | the folder is missing or inside Switchboard's data folder |
| "Name one account_id to refresh." | `refresh` needs one `account_id` |
| "Group not found." | call `switchboard_projects` for current group ids |
| "Statistics cover 1 to 90 days." | pass `days` from 1 to 90 |
