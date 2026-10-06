# Switchboard MCP tools — reference

Server `switchboard`, started as `switchboard mcp` (stdio, newline-delimited JSON-RPC 2.0,
MCP `2025-06-18`, also `2025-03-26` and `2024-11-05`). Every result carries
`structuredContent` and the same JSON as one text block. A refused call returns
`isError: true` with one fixed sentence, never a stack trace or a provider body. No tool
accepts or returns a credential; login and launch are not exposed.

`switchboard mcp --read-only` lists only the four read tools. A session Switchboard
launches in isolated mode gets that set.

## Contents

- [Session detection](#session-detection) — how the server knows which session asks
- [Read tools](#read-tools) — status, accounts, usage, project context
- [Write tools](#write-tools) — switch, project set, remove, apply
- [Refusals](#refusals) — fixed error texts and their remedies

## Session detection

The server works out which session asks from its own environment, first match wins:

1. `SWITCHBOARD_SESSION` = `managed:<provider>:<pool>` or `isolated:<provider>:<account id>`,
   written by a Switchboard launch.
2. `ANTHROPIC_BASE_URL` = `http://<running proxy address>/claude/<pool>` — only when the
   address is the proxy that is running now, so a stale variable cannot claim a pool.
3. `CODEX_HOME` = the app's `runtimes/codex-<pool>` folder.
4. Otherwise `native`; `provider` is `claude` inside Claude Code, else null (both providers).

## Read tools

### `switchboard_status` — no arguments

| Field | Meaning |
|---|---|
| `runtime.online`, `runtime.platform`, `runtime.note` | whether the desktop app or `switchboard serve` owns the proxy; offline means managed sessions cannot route and rotation is paused |
| `session` | `{mode: "managed", provider, pool}`, `{mode: "isolated", provider, account_id}` or `{mode: "native", provider}` |
| `current_cli_accounts` | the accounts signed in to the ordinary CLIs now, or null when unreadable |
| `routes` | `"<provider>:<pool>" → {account_id, label}` — who handles the next request |
| `rules.active`, `rules.total`, `rules.for_this_folder` | active rules (`path`, `provider`, `target`, `state`, `expires_at` in RFC 3339, `account`), the count of all rules, and the resolution for the server's folder |
| `rotation[]` | `provider`, `pool`, `target`, `enabled`, `threshold_percent`, `last_decision` |
| `limited[]` | accounts held after a provider limit error: `account_id`, `until` (when it may be tried again, Unix seconds), `resets_at` (the provider's own reset, or null when `until` is an estimated hold; a reset more than seven days out is held seven days, so `until` can be earlier), `kind` (`quota` when Claude Code named a subscription window — see `limit_type` — else `unknown`), `limit_type`, `confidence` (`attributed` for a managed request, `inferred` for a Claude Code transcript), `scope` (`unknown`: a shared organisation budget is not ruled out), `source`, `observed_at`, `event_id`. A hold is not a reset; never present `until` as one when `resets_at` is null |

### `switchboard_accounts` — `provider?: "claude" | "codex"`

`accounts[]`: `id`, `label`, `provider`, `kind` (`oauth`, `api_key`, `setup_token`), `pool`,
`enabled`, `selected_for_next_request`, `signed_in_cli`, `lowest_remaining_percent`,
`usage_fresh`.

### `switchboard_usage` — `account_id?`, `refresh?: bool` (default false)

`accounts[]` per account: `id`, `label`, `provider`, `pool`, `health`, `checked_at`,
`next_check_at`, `known`, `fresh`, `lowest_remaining_percent`, `windows[]`
(`name`, `scope`, `used_percent`, `remaining_percent`, `resets_at`), `observed_at`, `age_seconds`,
`source`, `reason`. Times are RFC 3339.

- `known: false` — no observation yet, or an API-key account (quota is reported for OAuth
  subscription accounts only). `lowest_remaining_percent` is then null: unknown, not zero.
- `scope: feature` marks a window that limits one metered feature of a Codex account (named
  `feature_<feature>_primary|secondary`); `scope: account` windows — including `spend_limit`,
  `workspace_credits`, `workspace_usage_limit`, `limit_reached` — limit the whole account.
  `lowest_remaining_percent` covers account windows only, and is null with `known: true` when
  the provider reported nothing but feature limits: unknown, not zero.
- `fresh` is false when the observation is older than the pool's freshest enabled rotation
  policy allows (900 seconds without one), when an account window's reset time has passed, or when
  the last check failed.
- `refresh: true` needs one `account_id` and asks the provider. For an inactive Claude account
  whose token has expired, Switchboard first renews that sign-in itself (0.5); the account the
  ordinary Claude Code uses is never renewed, and no credential reaches the agent. Within 60 seconds of the
  last check it answers with the earlier observation and `note` = "Checked less than a
  minute ago; showing that observation." A read-only server (`switchboard mcp --read-only`)
  never asks the provider: it answers with the stored observation and `note` = "This server is
  read-only: showing the stored observation without asking the provider."

### `switchboard_project_context` — `path?` (absolute folder)

`{path, project, rules[]}`: `project` is the project the folder belongs to (`name`, `pool`,
`folders`, reserved `accounts`), or null; `rules` has one entry per provider: `effective` (the rule in force, or null),
`nearest` (the nearest rule in any state, including paused or expired) and
`managed_route_in_effect` (whether the rule's account is already selected in its pool).
A rule view carries `path`, `provider`, `target` (`managed` or `claude_cli`), `enabled`,
`state` (`active`, `paused`, `expired`), `created_at` and `expires_at` in Unix seconds,
and `account` (`id`, `label`, `provider`, `pool`, `enabled`).

The folder defaults to `CLAUDE_PROJECT_DIR` when set, else the folder the server started
in. Pass `path` whenever the work moved to another folder.

## Write tools

Annotations: `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: true`.

### `switchboard_switch` — `account_id`, `target?`, `global?`

| target | Result `action` | Notes |
|---|---|---|
| `session` (default) | `selected` | the calling session must be managed, same provider, same pool |
| `route` | `selected` | selects the account in its own pool for every managed session of that pool |
| `claude_cli` | `activated` | requires `global: true`; changes the ordinary Claude Code login machine-wide |

Result: `{action, account: {id, label, pool}, message}`. A response already streaming
keeps its account; a manual choice starts the rotation cooldown.

### `switchboard_project_set` — `account_id`, `path?`, `target?`, `enabled?`, `expires_in_hours?`

Only when the operator asks. `target` is `managed` (default) or `claude_cli`;
`expires_in_hours` is 1 to 720. The folder must exist and cannot be inside Switchboard's
own data folder. Result: `{rule, message}`; the message reminds that the rule is visible
in the app under Projects and can be paused there.

### `switchboard_project_remove` — `provider`, `path?`

Removes the rule saved for exactly that folder and provider (not a parent folder's rule).
A folder that no longer exists can still have its rule removed. Result: `{removed}`.

### `switchboard_project_apply` — `path?`, `global?`

Result: `{path, session, results[]}`, one entry per provider this session can use, each
`{provider, action, account, message}`. Actions: `selected`, `activated`,
`already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool`,
`other_session`, `needs_global`, `failed`. Nothing changes without a rule in force.

| Rule target × session | Outcome |
|---|---|
| `managed` × managed, same pool | `selected`, or `already_in_effect` when already selected |
| `managed` × managed, other pool | `other_pool` |
| `managed` × native | `other_session` |
| `claude_cli` × managed | `other_session` |
| `claude_cli` × native | `already_in_effect`, `needs_global` without `global: true`, else `activated` |
| any × isolated | `other_session` |
| rule's account removed, or the change refused | `failed` |

## Refusals

Fixed texts the server returns with `isError: true`; relay them and offer the remedy:

| Text begins | Remedy |
|---|---|
| "This changes the Claude Code login for every ordinary claude session…" | ask the operator, then call again with `global: true` |
| "This session uses another provider." | choose an account of the session's provider |
| "This session routes the … pool; that account is in …" | choose an account in the session's pool, or launch a managed session in the other pool |
| "This session is isolated to one account…" | launch a managed session to switch inside a session |
| "This session does not run through Switchboard's proxy…" | `target: "route"` to prepare a pool, or `claude_cli` with `global: true` after the operator's OK |
| "No account with id …" | call `switchboard_accounts` for current ids |
| "Pass an absolute project folder." | pass an absolute `path` |
| "expires_in_hours must be 1 to 720." | pick an expiry in range |
| "Choose an existing project folder." | the folder is missing or inside Switchboard's data folder |
| "Name one account_id to refresh." | `refresh` needs one `account_id` |
