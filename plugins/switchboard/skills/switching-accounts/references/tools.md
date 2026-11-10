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

`switchboard mcp --read-only` lists only the 24 tools that read (every tool under
[Read tools](#read-tools)) and none of the 24 that write. A session Switchboard launches in
isolated mode gets that set. The server lists 48 tools; each has a section below.

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

### `switchboard_accounts`: `provider?: "claude" | "codex"`, `machine?: "here" | "elsewhere"`

`accounts[]`: `id`, `label`, `provider`, `kind` (`oauth`, `api_key`, `setup_token`), `pool` (the group id),
`groups` (every group it serves, its own first), `enabled`, `selected_for_next_request`,
`signed_in_cli`, `lowest_remaining_percent`, `usage_fresh`, `organization`, `also_organizations`
(`{id, name}` of each organization it also works for), `elsewhere` (used on
another machine) and `location` (where, when said), `assigned_to` (the agent or service it is given
to, null when none: Switchboard keeps observing its usage but never uses it by itself). `machine`
lists only this machine's accounts or only those used elsewhere.

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

### `switchboard_sessions`: `all?` (boolean), `history?` (boolean), `launch_id?` (string)

`{sessions[]}`: the sessions Switchboard launched (Claude Code, Codex, Kimi Code, Hermes,
OpenCode and catalog agents): `account_id`, `label`, `agent` (the catalog id, such as
`claude-code`, `codex` or `goose`; null when unknown), `provider` (the provider the route serves:
`claude` for an agent through an API-key account's proxy), `pool` (the group id), `context` (`personal` or `org:<id>`, fixed at the session's start;
absent before 0.6.80, and on Kimi Code, Hermes and OpenCode sessions before 0.7.0), `context_name` (the organization's name), `mode`, `working_directory`,
`in_place`, `launched_at`, `started_at`, `continues`, `home` and `state` (`starting`,
`running`, `ended`, `unknown`).
Without `all`, only starting and running ones. A session started outside Switchboard is not
listed.

With `history` (SB-212): `{launches[], total}`, every launch of the last 90 days, newest first
(at most 500), one record per launch even when a later launch reused its home: the fields above
plus `launch_id`, `launched_by` (`surface`: `window`, `cli`, `mcp`, `fallback`, `recovery`,
`dashboards` or `fabric`, a run `switchboard dispatch` started for a board request, SB-247, with
`launched_by.dispatch` `{id, request, thread, project, workflow}`; `caller_launch_id`,
`caller_project`, `caller_program`, `by`, `parent`), `provider_session_id`, `transcript` (the path
of its transcript), `turns`, `last_turn_at`, `ended` (`limit` when it stopped on one; a dispatched
run's `{kind: exited | stopped | timed_out, exit_code?, at}`), `ended_at`, `handover` and `continued_as` (`{kind, id}`: the
launch or hand-over its work went to). With `launch_id`: `{launch, up, down}`, that launch, who
launched it up to the first launch, and what it launched or what took its work over.


### `switchboard_provenance`: `provider?` (`claude` or `codex`), `pool?` (string), `launch_id?` (string)

Which account serves a provider's group (the default group when `pool` is left out), or one
launch, since when and why (SB-47, Fabric COM-11): `provider`, `pool`, `account_ref` (the
account's handle `sb-…`, never its id), `identity_revision` (a hash of the provider's identity, or
null), `mode` (`managed` for a group, null for a launch), `changed_at` (Unix seconds; null when the
journal no longer holds the choice), `cause` (`manual`, `rotation`, `limit`, `failover` or
`unknown`), `replaced` (`{account_ref, limit}` of the account it replaced, `limit` the owner's
evidence `{kind, resets_at, confidence}` or null; null when none is known) and `session`
(`{launch_id, dispatch_id, workflow}` for a launch, else null). Give `provider` or `launch_id`.
Reads only; no credential.
### `switchboard_onboarding`: `path?` (absolute folder), `host?` (`claude` or `codex`)

Where setting Switchboard up stands: `{complete, degraded, host, folder, steps[]}`, steps in order
`owner`, `accounts`, `project` (the folder's group), `mcp`, `proving_call`, each `{id, state, summary, next}`.
Those five are all on a computer without organizations, plus one optional `organization` step
when PassionCode.ai is connected and lists none. With organizations, `organization` follows,
then `org_group`, `seat`, `hook` and `launch` for each, each carrying `organization: {id, name}`.
`state` is `done`, `todo` (run `next.command`, or call `next.tool`), `needs_person` (tell
the person `next.person`; run `next.command`, or `next.alternatives` in order (save the group, then bind it; the bind alone when a group for those folders already exists), once they agree) or
`optional`. `next.turn_on` and `next.turn_off` (the `hook` step) are the person's switches: run
one only when the person asks. `degraded` names reads that failed (`snapshot`, `sign_ins`,
`sessions`); the step each touches says so. Reads only; the same as
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

### `switchboard_app`: `action`, `enabled?`

What the installed app's About and tray do, answered by the running app only (elsewhere: "Only the
installed app answers this."). `updates` reads the update state, or with `enabled` turns *Install
updates automatically* on or off; `check_for_updates` checks now; `restart_to_update` installs a
downloaded update and restarts through the drain; `login_item` reads *Open at login*, or with
`enabled` sets it. Changing anything only when the operator asks.

### `switchboard_link_cli`

Only when the operator asks. Links this `switchboard` into `~/.local/bin`, as the window's *Link the
CLI* does; it never replaces a file that is not Switchboard's.

### `switchboard_analytics`: `enabled?`

Switchboard's own usage analytics (counts and kinds only), as About sets it: `enabled` true or false
turns them on or off, only when the operator asks; without it, the state (`available`, `enabled`,
`notice_pending`). Only the installed app sends them.

### `switchboard_github`: `owner?`

Through the person's own `gh`: without `owner`, the owners it sees (the person and their
organizations); with `owner`, that owner's repositories, each with where it is on this computer and
its group. Reads only.

### `switchboard_github_clone`: `owner`, `name`, `into` (an absolute folder that does not exist yet)

Only when the operator asks. Clones one repository with the person's `gh`, as the window does.

### `switchboard_statistics`: `days?` (1 to 90, default 30)

What the proxy relayed: requests, outcomes (`ok`, `limited`, `failed`) and the provider's token
counts by group, project, account, coding agent and day. Managed traffic only; counts, never a
prompt or path. Out of range: "Statistics cover 1 to 90 days."

### `switchboard_events`: `limit?` (1 to 500, default 50)

`{events[]}`, newest first: fixed codes for accounts, switches, groups, automatic switching and
sign-ins. No credential, prompt or provider text.

### `switchboard_organizations`: no arguments

The organizations (domains, account count), every account with its organization (null:
personal), `by` (`person` or `provider`) and `suggested` with `suggested_name`: the organization
its address's domain suggests (SB-285: a domain places no account; null when none, when the
operator already placed it, or when it holds that organization; the operator confirms with
`switchboard_organization_set` `assign`), and `passioncode`: the PassionCode.ai connection's
state. Empty until the operator adds one.

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

### `switchboard_chain_get`: `workflow_id?`, `pool?`, `folder?`

`{chains[], effective, presets[], readiness, last_handover, ceilings_unreadable, limits_known}`. A chain is `{scope, executors[], updated_at}`: `scope` is
`{kind: "machine"}`, `{kind: "project", pool}` (a group) or `{kind: "workflow", id}`; each executor is
`{agent, account_id?, key?}`: a catalog agent id, optionally pinned to an account or to a paid
key by its Observatory vault name (`project/env/NAME`; never a value). `effective` is the chain
that applies (the workflow's own, else the group's, else the machine's) or null: **no chain
applies until the operator sets one**. With `folder` (the workflow's checkout, an absolute
folder) the group is found as the automatic fallback finds it: the executor's group (`pool`) when
it holds the folder, else the one group holding it, else, when several do, the group its proxy route
and paid key use; a relative `folder` is refused. `presets` lists ready-made orders
(`subscriptions-first`: claude-code → codex → kimi-code → hermes; the automatic fallback never hands a Claude Code or Codex task to Kimi Code, Hermes or OpenCode, which ask for no approval, SB-82 D-6). `readiness` says per chain
what each step would do now; `limits_known` is false when no Switchboard owner runs, so readiness
cannot name a limited account; `last_handover` and `ceilings_unreadable` come from the spending
ceilings' file.

### `switchboard_ceilings`

`{ceilings[], questions[], last_handover, limits}`: the spending ceilings for paid fallback
agents (Hermes and OpenCode on an OpenRouter key, Claude Code or Codex on an API-key account).
A ceiling is `{scope, cents, usd, set_at, doubled_from, active, inactive_reason}`: `scope` as in
a chain; this computer's and a group's count per UTC day, a task's on the key since its first
paid hand-over; a group's ceiling whose own key was removed reads `active: false`,
`inactive_reason: "no_own_key"`. Each open question is `{id, scope, cents, usd, double_usd,
spent, reached_at}`: a paid step reached that ceiling at a hand-over and was skipped.
`last_handover` is `{at, workflow, outcome, skipped[{agent, reason}]}` or null; `limits` names
`min_usd`, `max_usd` and `min_remaining_usd` (a paid step is skipped on a key with less than
that left of its own limit). Before every paid hand-over the key's spend is read from
OpenRouter; a reached ceiling, a nearly spent key, or a ceiling whose spend cannot be read skips
the step. A spending ceilings file that cannot be read is refused (*"Spending ceilings
(spend.json) could not be read, …"*) and no paid agent is handed work until it reads again.

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

`{schema, revision, updated_at, default, default_ready, dispatch_agent, permissions, installed, agents: [...], recommended_models}`. `permissions` is `bypass` (the default: the Claude Code and Codex sessions Switchboard starts skip their permission prompts) or `ask`. `default` is the machine's default
agent, `null` for *Auto*; Fabric uses it unless a group sets its own. `dispatch_agent` is the agent a
board dispatch would start now: with *Auto*, Claude Code, else Codex, else `null`. Each agent row: `id`,
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

### `switchboard_switch`: `account_id`, `target?`, `global?`, `force?`

| target | Result `action` | Notes |
|---|---|---|
| `session` (default) | `selected` | the calling session must be managed, same provider, same group |
| `route` | `selected` | selects the account in its own group for every managed session of that group |
| `claude_cli` | `activated` | requires `global: true`; changes the ordinary Claude Code login machine-wide |

Result: `{action, account: {id, label, pool}, message}`. A response already streaming
keeps its account; a manual choice starts the automatic switching cooldown.
An account given to an agent (`assigned_to`) is refused ("This account is given to an agent, so
Switchboard does not use it. Take it back, or use it anyway.") unless `force: true`, which only the
operator's request to use it anyway justifies; the activity records it as `assignment override`.

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

### `switchboard_project_edit`: `pool`, `name?`, `organization?`, `organizations?`, `add_accounts?`, `remove_accounts?`, `add_folders?`, `remove_folders?`, `move_accounts?`

Only when the operator asks. Changes a group by delta; anything not named stays. A name the
group does not hold is refused; `organization` is an id, a name or `personal`; `organizations` is
the whole list (none, one or several, the first the group's own context) and wins. An account of
another group in `add_accounts` keeps its own group and serves this one too; `move_accounts`
moves it here instead; `remove_accounts` of a shared account only stops it serving here. Result:
`{project, message}` ("Group saved. It is visible in the Switchboard app under Groups.").

### `switchboard_project_create`: `name`, `folders`, `accounts?`, `organizations?`, `move_accounts?`

Only when the operator asks. Creates a group as the window's *New group* does: at least one
absolute folder, the accounts reserved for it, and its organizations (by id or name, the first its
own context; empty or left out: personal). An account at home in another group serves this one
too unless `move_accounts` moves it here. Refusals are those of a saved group (an account of
another organization, the ordinary CLI's account). Result: `{project, message}` ("Group created.
It is visible in the Switchboard app under Groups."); `project.pool` is the new group's id.

### `switchboard_project_move_root`: `folder`, `pool`, `also?`, `from?`

Only when the operator asks. Moves a repository with its worktrees and clones into the group
`pool`, or with `also` adds it to one more group; `from` names the group it leaves. `also` with
`from` is refused ("Name either also or from, not both.").

### `switchboard_codex_route`: `enabled?`

Without `enabled`, the state (`on`, `applied`, `conflict` and the groups with a Codex account);
`true` or `false` changes whether Codex in group folders uses the group's accounts, only when the
operator asks.

### `switchboard_shell_hook`: `path?`

Whether this computer's terminals follow organization folders, as `switchboard shell status`
reads it: `hook` (`on`, `off`, `default_on` because the folder map holds a folder of a group
bound to an organization, `default_off`), `choice`, `map`, each start file in `files` with its
`state` (`written`, `removed_by_person`, `skipped_symlink`, `skipped_owner`,
`skipped_zdotdir_unknown`, `unreadable`, `policy_blocked`: a PowerShell profile on Windows whose
execution policy, `Restricted` or `AllSigned`, keeps it from loading; Switchboard never changes
the policy; `constrained_language`: PowerShell on Windows runs in Constrained Language Mode,
where the hook cannot act), and in `here` the group, context and homes a terminal gets in
`path` (default the agent's folder). Reads files only.

### `switchboard_shell_hook_set`: `on`

Only when the operator asks. `false` takes Switchboard's block out of every start file byte for
byte and keeps it out; `true` writes it for every shell present. Answers what
`switchboard_shell_hook` reads. From another data folder it is refused and nothing changes.

### `switchboard_project_suggestion`: `action`, `folder`

Only when the operator asks. `accept` adds a waiting folder to its group now; `dismiss` keeps it
out for good. `folder` as `switchboard_project_suggestions` lists it.

### `switchboard_project_sources`: `pool`, `add?`, `remove?`

The repository owners (`github.com/example`) whose new repositories join the group by
themselves. Without `add` or `remove`, reads them; changing them only when the operator asks.

### `switchboard_account_update`: `account_id`, `label?`, `enabled?`, `elsewhere?`, `location?`, `assigned_to?`, `force?`

Only when the operator asks. Renames, enables or disables an account; `elsewhere: true`
disables it here because another machine uses it; `location` says where it works (a machine and
its user, or a colleague, at most 80 characters) and marks it so; an empty one clears it.
`assigned_to` gives the account to an agent or service that signs in with it now (its name, at
most 120 characters): unlike `elsewhere` it stays enabled, its sign-in is renewed and its usage
observed, but nothing selects it, switches to it or launches on it by itself; giving it clears
`elsewhere`; an empty value takes it back. At least one field ("Name a label, enabled, elsewhere,
location or assigned_to to change.").

### `switchboard_account_remove`: `account_id`

Only when the operator asks. Removes one account from Switchboard and deletes its stored sign-in or
key here, as the window's *Remove account* does; this cannot be undone. Refused while its group has
it selected (select another first). Clients signed in elsewhere are not signed out; an account moved
to another machine is called back or removed there with `switchboard_account_recall`.

### `switchboard_account_send`: `account_id`, `to`, `pool?`, `remote?`, `remote_data_dir?`, `decision?`

Only when the operator asks. Moves a Claude or Codex subscription sign-in to another machine over
ssh without a new sign-in there: it is marked here as used elsewhere first, saved there (the tokens
on stdin), checked there with a quota check, and both are undone on any failure. `to` is the machine
as ssh knows it, reachable without a prompt; `pool` its group there; `remote` that machine's
`switchboard` (default the macOS app's); `remote_data_dir` its data folder when it runs with
`--data-dir`, given to every command there. Every move is a line in `seats.jsonl`; `decision` (the
operator's words: `by: operator`, `instruction` verbatim, `at` RFC 3339, `source`) is recorded with it
as who ordered it.
The person's word on the account's organizations goes with it: an organization the other machine
lacks is added there by name and domains, then the copy is placed the same way; `organizations` in
the answer is null (no word said here), or `{carried, contexts}` with `reason` when a step there
failed (the move stands; place it there with `switchboard org assign`).

### `switchboard_targets`: `probe?`

The other machines this Switchboard knows, from accounts moved there and runs started there:
`machine` (as ssh names it), `program` and `data_dir` used there, `last_at`, `known_by`
(`moves`, `dispatch`). `probe: true` asks each one's Switchboard over ssh whether it answers now
and its version. A person's session on one starts in a terminal with
`switchboard launch --target MACHINE --provider claude --working-directory FOLDER --in-place`
(the folder is that machine's; its account comes from the folder's group there). Reads only.

### `switchboard_remote`: `action`, `on`, `remote?`, `remote_data_dir?`

Reads another machine's Switchboard over ssh (`on`, the machine as ssh knows it). `status`: its
accounts (label, group, on or off, used elsewhere, used percent, usage check; never a credential),
groups (accounts and folders each holds, organization), organizations, and whether it is connected
to PassionCode.ai. `check`: `findings` against the moves recorded here, each with `kind`
`missing_there`, `off_there`, `refused_there` (needs a sign-in there), `not_marked_here`,
`held_twice` (the same sign-in used on both machines: call one back or remove it) or `no_record`
(marked used elsewhere here with no location and no recorded move); an empty list is a clean check.
`remote` and `remote_data_dir` default to the newest move there. Reads only, on both machines.

### `switchboard_account_recall`: `account_id`, `from`, `remote_id?`, `remote?`, `remote_data_dir?`, `decision?`

Only when the operator asks. Takes back a Claude or Codex sign-in this machine moved with
`switchboard_account_send`: the other machine stops using it and gives its tokens back over ssh, the
provider is checked here (on failure it goes back there), then the copy there is removed and its
account list read. Answers `{recalled, removed_there, proved}`; `proved` is false while the list
there still names it. `remote_id`, `remote` and `remote_data_dir` default to the ones the
recorded move named, until a call back of it is recorded. Refused when the account was renewed here after it moved (its copy there is
spent then); the other machine's own reason is answered when it refuses, and a group that uses the
account there moves to another of its accounts first.

### `switchboard_seats`: `limit?`

Reads only. The newest lines of `seats.jsonl` (default 50, at most 1000), newest last: `at`, `event`
(`granted`, `handed_over`, `recalled`), `account` (the id here), `machine` (as ssh named it),
`pool`, `remote_account` (the id there), `sent` (a fingerprint of the sign-in sent, never the
token), `ordered_by` and the operator's `decision`. Read it before `switchboard_account_recall` to
find what is on another machine.

### `switchboard_account_hand_over`: `account_id`, `to`, `decision?`

Only when the operator asks. Answers a signed record of the account without its credential and
marks it here as used elsewhere, with `to` as where it works. The receiver passes the record to
`switchboard_account_receive` and signs in there. Hand a personal subscription only to the
operator's own machines unless the operator says otherwise. Recorded in `seats.jsonl` like a move,
with `decision` when given.

### `switchboard_account_receive`: `record`, `signer?`, `apply?`, `headless?`

Answers whose sign-in the record names, the group it lands in here (the record's pool when this
machine has that group, else `default`), whether the signature holds (`signer`: the giver's
published key) and whether this machine already holds that sign-in. `apply` starts the sign-in;
the person signs in. `apply` needs `signer` and is refused when this machine already holds it.

### `switchboard_account_move`: `account_id`, `pool`

Only when the operator asks. Moves an account into the group `pool`, or back with `default`;
the account the ordinary Claude Code or Codex uses cannot join a group. To let an account serve
one more group and keep its own, add it with `switchboard_project_edit` instead.

### `switchboard_organization_set`: `action`, `id?`, `name?`, `domains?`, `account_id?`, `organization?`

Only when the operator asks. `action` is `add` (name, domains), `edit` (id; name and/or domains,
which replace the list), `remove` (id), `assign` (account_id, organization: an id, `personal` or
`auto`; or `organizations`: every context of an account that works for several, the first its
own, the rest it also works for, empty to let the rules decide; `personal` can only be first:
`["personal", "Acme"]` keeps an account personal and working for Acme too; the answer's
`refused_by` names the groups that no longer use it; for an account moved to another machine,
`there` says whether its copy there took the same word), `passioncode_read` (read the
PassionCode.ai organizations again; connecting is the person's), `passioncode_cancel` (stop a
connection waiting for the browser) or `passioncode_disconnect` (delete the token; the
organizations stay as local ones).

### `switchboard_rotation_set`: `provider`, `pool?`, `target?`, `enabled?`, `threshold_percent?`, `hysteresis_percent?`, `cooldown_seconds?`, `max_age_seconds?`

Only when the operator asks. Changes automatic switching for one provider, group and target
(`managed` or `claude_cli`); every field left out keeps its saved value.

### `switchboard_backup_now`: no arguments

Writes an encrypted backup now.

### `switchboard_backup_restore`: `file`

Only when the operator asks. Restores one backup that `switchboard_backups` lists, by its file
name, as the window's *Restore* does: accounts, groups, rules, the person's words and settings in
it come back where this computer has none; accounts already here stay as they are and nothing is
deleted. An account whose group here does not take it goes to the default group
(`returned_to_default`). Answers the counts.

### `switchboard_project_delete`: `pool`, `decision?`

Only when the operator asks. Deletes a group as the window's *Delete group* does: its folders stop
routing to its accounts and its own accounts go to the default group (nothing is signed out). A
group with its own key or spending ceiling needs `decision`, the operator's words.

### `switchboard_settings_import`: `file`, `under?`, `apply?`, `clone?`, `signer?`

Reads a `switchboard-settings/1` file (`switchboard settings export` on another machine; `file`
is an absolute path here) and answers what it would add: groups found by their repositories under
`under` (absolute folders), organizations, agents' settings, rotation policies, teams and
ceilings. With `apply` it adds them, only when the operator asks, and never raises or clears a
ceiling; `clone` with `apply` first clones the groups' missing GitHub repositories with the
person's `gh`; `signer` is the organization's published key the file must be signed by.

### `switchboard_agent_set`: `agent?`, `enabled?`, `default?`, `permissions?`

Only when the operator asks. `enabled` (with `agent`) lets Switchboard hand work to that agent
or keeps it away; `default` sets the machine's default agent (an id, or null for Auto);
`permissions` (`bypass`, the default, or `ask`) chooses whether the Claude Code and Codex
sessions Switchboard starts bypass their permission prompts (SB-298).

### `switchboard_login`: `action`, `login_id?`, `provider?`, `label?`, `pool?`

Only when the operator asks. `begin` (provider, label, pool as the group id) opens Terminal on
the official sign-in; the person signs in. `status`, `finish` and `cancel` take the `login_id`
`begin` returned; `pending` lists every sign-in waiting, oldest first (`{logins: [{login_id,
provider, label, pool, state}]}`). No password or key passes through this tool.

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

### `switchboard_ceiling_set`: `scope?`, `usd`, `decision?`

Only when the operator asks. `scope` is `machine` (default), `project:<group id>` or
`workflow:<wf_id>`; `usd` is dollars from 0.01 to 1000 (per UTC day for this computer and a
group, since the first paid hand-over for a task). A first ceiling for a scope or a value no
higher than the current one is accepted; raising one, or clearing it with `usd: null`, takes
`decision`: the operator's own words (`{by: "operator", instruction, at, source}`: the words
verbatim, when, RFC 3339, and where they were said), recorded in `spend.json` (SB-299). Without
them it is refused (*"Raising or clearing a ceiling is done in the Switchboard window. …"*).
Returns `{ceiling}` (`scope`, `cents`, `set_at`).

### `switchboard_ceiling_answer`: `id`, `answer?`, `decision?`

Only when the operator asks. Answers an open question from `switchboard_ceilings` with `keep`
(the default): the ceiling stays as it is and the question closes. `double` takes `decision`,
the operator's own words as for `switchboard_ceiling_set`; without them it is refused
(*"Doubling a ceiling is answered in the Switchboard window."*). Returns `{answer, ceiling}`.

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

Fixed texts the server returns with `isError: true`, the same sentences the window shows a person (SB-189); relay them and offer the remedy:

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
| "Choose an existing folder." | the folder is missing or inside Switchboard's data folder |
| "Name one account_id to refresh." | `refresh` needs one `account_id` |
| "Group not found." | call `switchboard_projects` for current group ids |
| "Statistics cover 1 to 90 days." | pass `days` from 1 to 90 |
| "Raising or clearing a ceiling is done in the Switchboard window…" | set a first or a lower ceiling, or ask the operator to raise or clear it in the window |
| "Doubling a ceiling is answered in the Switchboard window." | keep it here, or ask the operator to double it in the window |
| "Give this group its own key under API keys first." | the operator saves the group's own OpenRouter key, then the ceiling |
| "No open question with that id…" | call `switchboard_ceilings` for the open questions |
| "Only Switchboard's own data folder can change your terminals' start files…" | run the server without `--data-dir`, or ask the operator to use the app's switch |
| "No installed switchboard command was found for the hook to run…" | install Fabric Switchboard, then turn it on again |
| "on is true or false." | `switchboard_shell_hook_set` takes `on`: `true` or `false` |
| "The terminal hook could not be written…" | Switchboard or its data folder sits at a path that is not absolute or not valid text: reinstall, then turn it on again |
| "The terminal hook's choice could not be saved in Switchboard's data folder." | the data folder is not writable; nothing changed, try again once it is |
| "Your home folder could not be found, so no start file was changed." | the server runs without a home folder (`HOME`); run it as the person's own user |
