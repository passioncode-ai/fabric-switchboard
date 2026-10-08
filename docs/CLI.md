# switchboard CLI

Reference for the `switchboard` CLI as of 0.6.5: the same account store, vault, request proxy and launcher the desktop app uses. The original [CLI release plan](CLI-RELEASE-PLAN.md) fixed the 0.2 behaviour; release records: [0.5.x–0.6.x](evidence/release-0.5.md), [0.2](evidence/release-0.2.md). This file is an operational contract, not a claim of real-provider acceptance.

## Install and first use

Use the `switchboard` executable beside the macOS app in the signed archive, or `switchboard.exe` from the Windows artifact. Building only the CLI from source:

```sh
cargo install --path crates/switchboard-cli --locked
switchboard --help
switchboard --json accounts list
```

Help/version do not open account storage. Default data root is shared with GUI: `~/Library/Application Support/ai.passioncode.fabric-switchboard` on macOS, `%LOCALAPPDATA%/ai.passioncode.fabric-switchboard` on Windows. `--data-dir ABSOLUTE_PATH` selects a deliberately separate metadata root. Credentials stay in the native vault by UUID; never mix metadata roots casually or treat this flag as account export.

## Account management

```sh
switchboard accounts list
switchboard accounts add --provider claude --kind api-key --label Work --pool work --secret-stdin < /private/path/to/credential
switchboard accounts select ACCOUNT_UUID --provider claude --pool work
switchboard accounts update ACCOUNT_UUID --label Work                  # or --enabled false, or both
switchboard accounts remove ACCOUNT_UUID
switchboard --json events
```

`--secret-stdin` is mandatory for add; a secret value is never accepted as an argument. Redirect from a private credential file or a trusted secret manager. Do not put tokens in shell history. Provider/kind are explicit; supported kind spellings are `api-key`, `setup-token` and `oauth`. OAuth input is provider credential JSON. UUID identifies an account; labels are display text. Removal has the same selected-account/active-home guards as the GUI.

Without a running owner, metadata operations acquire the Store’s exclusive lock and release it after the command. With GUI or `serve` running, they use the owner’s authenticated loopback control server. Both paths run core validation. The CLI must not convert an authentication failure into a local-write fallback.

## Running sessions and official sign-in

Keep the desktop open **or** run this in a separate terminal:

```sh
switchboard serve
```

Then use another terminal:

```sh
switchboard status
switchboard backup list
switchboard backup now
switchboard backup restore switchboard-backup-1790000000-<id>.json   # a name from backup list
switchboard login begin --provider codex --label Work --pool work
switchboard login status LOGIN_UUID
switchboard login finish LOGIN_UUID
switchboard accounts select ACCOUNT_UUID --provider codex --pool work
switchboard launch ACCOUNT_UUID --mode managed --working-directory /absolute/project/path
switchboard launch --provider claude --working-directory /absolute/project/path --in-place -- --resume SESSION_ID
switchboard continue wf_0123456789abcdef --account ACCOUNT_UUID --dir /absolute/checkout
```

Backups belong to the default data folder: with `--data-dir`, `backup list` shows none and `backup restore` refuses (“Backups belong to the default data folder; this data folder has none.”), so a scratch store never sees or restores the operator's accounts (SB-61, `a_scratch_data_folder_never_sees_the_real_backups`).

On Windows use an absolute drive or UNC project path, quoted if it contains spaces. CLI login/launch need a live owner because its proxy and pending-login registry must outlive the command. Missing owner returns an instruction to start GUI or serve. Managed launch requires that account selected in its provider/pool; isolated mode launches a private credential snapshot and is not changed by later selection.

`login begin` opens the installed official provider CLI in a private home; `--label` is optional and defaults to the email of the account that signs in; signing in to an account already saved updates it where it is (label and pool kept, every copy in every pool) instead of adding another (SB-62). `login status` answers `pending`, `complete` (ready to finish) or `ended` (the Terminal session exited without a successful sign-in) and reads no credential; the desktop app polls it and finishes on its own. Complete its login, then `finish` imports only that new credential into the OS vault and cleans staging. If the account is saved but its staging home cannot be removed, `finish` reports `Account saved; isolated login cleanup needs attention.`, releases the sign-in slot and retries the cleanup before the next `login begin` (`failed_login_cleanup_releases_the_slot_after_the_account_is_saved`). To cancel, close/finish that Terminal process, then `switchboard login cancel LOGIN_UUID`. It never kills an unrelated process or invokes global logout. App/server restart invalidates pending in-memory login IDs; recovery limits are in [operations](OPERATIONS.md).

`launch` opens the session in a new Terminal window. **`--in-place`** (0.6.9, SB-75) prepares exactly the same session — the private home, its tools and `switchboard mcp`, the scrubbed environment, the one-session-per-home marker — and runs it in the terminal the command was started in, for a host with its own console (Fabric Dashboards' panel); the agent's exit status is the command's, and the host never sees a token. It needs a terminal on stdin and stdout and, like every launch, a running owner; macOS and Linux for now. **`--provider claude|codex`** instead of an account id uses the account Switchboard would use for that folder: its project's selected account, else the account of the folder's rule in force, else the default pool's selected account. Arguments after `--` go to the agent after Switchboard's own, each quoted (at most 16, up to 1024 characters, no control characters) — for example `-- --resume <id>` for Claude Code, `-- resume <id>` for Codex; they need `--in-place`.

`continue` hands an [Observatory](CONTRACTS.md#workflow-continuation-sb-52-2026-10-05) workflow whose executor ran out of its limit to another account — of the same provider, or since 0.6.7 of the other one, Claude Code ↔ Codex, with the same checkpoint and constraints (SB-70) (`--mode isolated` by default, or `managed`). It reads the workflow, refuses before anything changes when it cannot be continued here (closed, no checkpoint, a handoff already waiting, a declared key not in the vault, a workflow that names no executor, the same account, a folder outside the workflow's checkouts), offers it with reason `limit` and opens the account's session in Terminal. That session accepts the handoff itself, through the Observatory tools Switchboard declares in its config. The engine refuses an offer while the executor wrote a checkpoint less than two minutes ago; Switchboard reports that sentence and launches nothing. Set `SWITCHBOARD_OBSERVATORY_BIN` when `project-observatory` is not in `~/.local/bin`, Homebrew or `PATH`. macOS only for now.

Select another same-pool account while the managed response streams: the current response keeps its snapshot; the next request sees the new account. `Ctrl-C` or `SIGTERM` in `serve` stops the owner within ten seconds. Since 0.5.4 the proxy keeps its port and token across restarts, so a managed client launched earlier works again once an owner is back. Only one owner may hold a data root.

## Uninstall

```sh
switchboard uninstall              # the plan; nothing changes
switchboard uninstall --yes        # remove credentials, the CLI link and the data folder's own entries
switchboard uninstall --keep-data --yes
switchboard uninstall --purge --yes    # also the session homes and the CLIs' history in them
```

Quit the app and `serve` first (the store is opened exclusively). What is and is not removed: [OPERATIONS → uninstall](OPERATIONS.md#corrupt-metadata-backup-and-uninstall) (SB-29).

## Usage, output and failures

```sh
switchboard usage
switchboard usage ACCOUNT_UUID
switchboard --json status
switchboard --json activity
```

Without an ID, usage reads cached observations; an ID requests a provider check, refused without a request while a wait the provider asked for lasts (SB-39; [ACCOUNTS-AND-ROTATION](ACCOUNTS-AND-ROTATION.md)). Human output prints times as RFC 3339 UTC, marks an observation older than 900 seconds as stale (the idle quota cadence plus slack, SB-48; rotation judges by its policy's `--max-age`) and shows the last check's health (`ok`, `failed`, `unavailable` or `not checked`) with the next check time; `events` prints RFC 3339 times too. `--json` output keeps Unix seconds (`human_usage_and_events_print_utc_times_and_health`). Unsupported/unknown quota is an error or unknown state, never zero. The human percentage is the account's own; a Codex feature limit is listed in `--json` and MCP (`scope: feature`) but does not count toward it (SB-40). API billing is not subscription utilization.

`--json` writes `{ "ok": true, "data": ... }` to stdout on success and `{ "ok": false, "error": "..." }` to stderr on failure. Exit status is 0 for success, 1 for an operational failure and 2 for invalid arguments. `--help` succeeds without touching the vault. Do not log stdin or treat parser diagnostics as a place to print credentials. Output never includes the control capability, provider token, credential JSON or provider request/response payload. The [CLI implementation and integration fixtures](../crates/switchboard-cli/src/main.rs) define this envelope.

The local `control.json` contains a capability and is private runtime state. It is not an API discovery file to share. It does not grant authority on LAN: clients require exact loopback host/port, no redirects/proxies, bounded bodies and a per-runtime bearer. Browser Origins are refused. Same-user malicious processes are outside the protection boundary, just as for the official CLIs they could inspect.

## Capture, import and automatic rotation

```sh
switchboard current
switchboard accounts capture --provider claude --pool default
switchboard accounts import-claude-swap --pool default
switchboard accounts activate ACCOUNT_UUID
switchboard rotation set --provider claude --pool default --target claude-cli --enabled true
switchboard rotation status
switchboard rotation set --provider claude --pool default --target claude-cli --enabled false
```

Capture reuses existing local authorization without another login. Import reads Claude Swap and reports imported/failed/skipped counts without credentials. Activate changes the ordinary Claude Code account with identity checks and compatible locks; only captured/imported Claude OAuth profiles qualify. Account selection for the proxy stays separate.

Rotation targets are `managed` and `claude-cli` (serialized `claude_cli`). `rotation set` changes only the flags it is given and keeps the saved values of the others, so `--enabled false` stops rotation without resetting custom settings. A new policy starts off, with `--threshold` 90, `--hysteresis` 10, `--cooldown` 1800 seconds and `--max-age` 300 seconds (`rotation_set_changes_only_the_given_flags`). The saved policy is read and the merged one written in two steps; a concurrent change between them is overwritten. Hysteresis is headroom below the threshold. Only one native Claude policy may be enabled across all pools because the ordinary native account is one shared target. Existing streams are never replayed. Configuration persists offline; status explicitly reports whether the owner/monitor runs. Full behavior and limitations: [account and rotation contract](ACCOUNTS-AND-ROTATION.md).

## For agents

Version 0.4 serves Switchboard to coding agents over the Model Context Protocol and adds optional project rules. The [tool reference](../plugins/switchboard/skills/switching-accounts/references/tools.md) and the [server source](../crates/switchboard-cli/src/mcp.rs) define the tools (PLAN-0.4's table is the 0.4 design, kept for history); the [plugin](../plugins/switchboard/README.md) and its [skill](../plugins/switchboard/skills/switching-accounts/SKILL.md) tell an agent how to use them.

```sh
switchboard mcp               # stdio server: status, accounts, usage, switch, project rules
switchboard mcp --read-only   # only the five read tools; usage refresh shows the stored observation
switchboard --data-dir /path/to/app-data mcp
```

`switchboard mcp` speaks newline-delimited JSON-RPC 2.0 on stdin/stdout (MCP `2025-06-18`, also `2025-03-26` and `2024-11-05`; clients asking for `2025-11-25` — MCP Inspector 2.9.0, the TypeScript SDK 1.32.0 and client 2.3.0, the Python SDK 2.3.0 — accept `2025-06-18` and work: [matrix](runs/2026-10-04-sb-43-mcp-matrix/README.md), `scripts/mcp_compat.py`); stdout carries protocol messages only. It uses the running owner's control channel when the desktop app or `serve` runs, else an offline store operation under the exclusive lock. No tool accepts or returns a credential, and login and launch are not exposed. `switchboard_switch` with `target: "claude_cli"` changes the ordinary Claude Code login for every `claude` session and is refused unless the call carries `global: true`. `--read-only` lists `switchboard_status`, `switchboard_accounts`, `switchboard_usage` and `switchboard_project_context` only. A managed session that Switchboard launches gets the server automatically when the CLI is found (Claude through `--mcp-config`, Codex through `[mcp_servers]` in its private config); an isolated session gets the read-only set.

Project rules are optional: a folder and its subfolders start on a chosen account, rules never stop rotation, and the desktop app lists them under Projects, where each can be paused.

```sh
switchboard project list                      # {projects, rules}: projects with folders and accounts, then rules
switchboard project save --name "Client Alpha" --folder /work/alpha-web --folder /work/alpha-api --account <id>
switchboard project save --pool client-alpha --name "Client Alpha" --folder /work/alpha-web   # update; omitted accounts go back to default
switchboard project delete --pool client-alpha  # its accounts stay in the pool, no longer reserved
switchboard project show --path /path/to/project
switchboard project set --path /path/to/project --account ACCOUNT_UUID --expires-in-hours 8
switchboard project set --path /path/to/project --account ACCOUNT_UUID --target claude-cli --paused
switchboard project remove --path /path/to/project --provider claude
switchboard project apply --path /path/to/project
switchboard project apply --path /path/to/project --global
```

`--path` defaults to the current folder and must be absolute. `--target` is `managed` (default) or `claude-cli`; `--expires-in-hours` accepts 1 to 720; `--paused` saves the rule switched off. `apply` detects the calling session (managed, isolated or native) and reports one action per provider: `selected`, `activated`, `already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool`, `other_session`, `needs_global` or `failed`. A `claude_cli` rule changes the ordinary Claude Code login only with `--global`.

Connecting an agent. The `switchboard` executable must be on `PATH`; on macOS the desktop app's Agents panel links the bundled CLI to `~/.local/bin/switchboard`, and on Windows the installer puts `switchboard.exe` beside the desktop app (since 0.6.0, SB-05), where the Agents panel finds it and prints the registration commands with its full path; to type `switchboard` in a shell, add that folder to `PATH`.

| Agent | Command |
|---|---|
| Claude Code, plugin (server plus the `switching-accounts` skill) | `claude plugin marketplace add passioncode-ai/fabric-switchboard`, then `claude plugin install switchboard@switchboard` |
| Claude Code, server only | `claude mcp add --scope user switchboard -- switchboard mcp` |
| PassionCode launcher, once the plugin is released as a member | `npx @passioncode-ai/passioncode@latest update` |
| Codex | `codex mcp add switchboard -- switchboard mcp` |

Restart the agent after installing; it loads servers and skills at session start. The plugin is validated by `python3 scripts/check_plugin.py`, `python3 -m unittest scripts/test_check_plugin.py` and `claude plugin validate ./plugins/switchboard --strict` plus `claude plugin validate . --strict`.

### Fallback chains

Which agents continue a workflow, in what order, when its executor's accounts run out (SB-71,
[XA-01](packets/cross-agent-continuation.md)). You set them per machine, per project (its pool) or
per task (one workflow); the narrowest set chain applies, and **none applies until you set one**.

```sh
switchboard chain set --preset subscriptions-first            # claude-code → codex → kimi-code → hermes
switchboard chain set --scope project:alpha-web codex claude-code
switchboard chain set --scope workflow:wf_0123456789abcdef codex@ACCOUNT_ID kimi-code#my-project/prod/OPENROUTER_API_KEY
switchboard chain list [--workflow wf_…] [--pool …]          # every chain, presets, the one that applies
switchboard chain clear --scope workflow:wf_0123456789abcdef
```

An agent is a catalog id (`switchboard agents list`) that loads MCP servers and takes a prompt
without a person; Aider, for example, does not and is refused. `@ACCOUNT_ID` pins an account,
`#project/env/NAME` a paid key by its Observatory vault name — never a key value. Without a pin
Switchboard chooses an account at the switch. Claude Code and Codex run on their own provider's
accounts; any other agent on an API-key account or a paid key, never on a subscription sign-in.
Removing an account or a project drops what pinned it. When a workflow's executor hits its limit,
the automatic fallback walks the chain that applies (0.6.8, SB-73: Claude Code and Codex so far;
other agents are skipped until SB-72); `switchboard continue` still hands a workflow over by hand.

### Other coding agents

Switchboard's catalog of 30 coding agents ([AGENT-SUPPORT.md](AGENT-SUPPORT.md)) says what each
can use: its MCP tools (`mcp`), the local proxy with an API-key account (`proxy`), or a launch
configured by environment alone (`launch`).

```sh
switchboard agents list                                # id, level (mcp|proxy|launch), name
switchboard agents connect hermes --pool work          # MCP registration and, when it can, proxy settings
switchboard agents key                                 # the agents' proxy key, for an agent's key command
switchboard agents launch goose --pool work --dir /abs/folder
```

The agent id comes from `agents list`. `agents key` prints a local proxy key, not a provider
credential: the proxy accepts it only for the pool's API-key account, never for a Claude
subscription. Keep it out of shared configuration files.

### Hermes's model and provider (SB-80)

```sh
switchboard agents hermes                       # model, provider and endpoint Hermes runs on
switchboard agents hermes set --provider openrouter --model moonshotai/kimi-k2
```

`set` runs Hermes's own `hermes config set`; the new provider needs its own key inside Hermes.

### Kimi Code accounts (SB-81)

Kimi Code subscription accounts, each signed in by the official `kimi` in a folder of its own.

```sh
switchboard kimi login [--label Work] [--region mainland-cn|global]   # kimi login in Terminal
switchboard kimi finish <id>          # once Terminal says you are logged in
switchboard kimi list                 # tier and 5-hour / 7-day / monthly usage, asked of Kimi
switchboard kimi launch <id> --dir ~/src/app
switchboard kimi relogin <id>         # when Kimi refused the saved login
switchboard kimi rename <id> "New name" ; switchboard kimi remove <id>
```

`list` also shows the ordinary `kimi`'s sign-in (`~/.kimi-code`), never changed. Switchboard
never copies or renews a Kimi login: when `list` says Kimi renews it on its next run, launch
the account once. Backups do not carry these logins.

### The OpenRouter key for agents (SB-79)

One OpenRouter key, saved once in the OS vault, for agents that can run on OpenRouter. The value
never touches a file, an argument or a log.

```sh
switchboard agents openrouter set --key-stdin [--model moonshotai/kimi-k2]   # key piped on stdin
switchboard agents openrouter status       # saved? default model, what the key may still spend
switchboard agents openrouter model anthropic/claude-sonnet-4.5
switchboard agents openrouter remove
switchboard agents key --service openrouter   # the value, for a session script's export only
```

`set` refuses interactive input: pipe the key (`< key.txt`, `pbpaste |`). `status` asks
OpenRouter itself (`GET /api/v1/key`) and shows `limit_remaining` and `usage_daily`; a refused
key stays saved and is named as refused. An agent over MCP gets the same status and sets the
model (`switchboard_openrouter_status`, `switchboard_openrouter_model`) — never the key.
Encrypted backups carry the key, so it returns after a reinstall.

Launch an agent on the key in Terminal (no owner needed; the pool does not apply):

```sh
switchboard agents launch hermes --openrouter --dir ~/src/app [--model moonshotai/kimi-k2]
```

`--model` defaults to the key's model. The session script reads the key from `switchboard agents
key --service openrouter` when it starts and exports it under the agent's own variable
(`OPENROUTER_API_KEY`, `KIMI_MODEL_API_KEY` for Kimi Code, `OPENAI_API_KEY` for Qwen Code).
Hermes, pi, omp and Qwen Code start in their own home under Switchboard's data folder, so a key
they saved themselves cannot win. Which agents launch this way, and their caveats:
[AGENT-SUPPORT.md](AGENT-SUPPORT.md) ("OpenRouter key" lines). OpenClaw and Cline cannot.
