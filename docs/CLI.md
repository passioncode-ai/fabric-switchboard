# switchboard CLI

Reference for the `switchboard` CLI as of 0.6.18: the same account store, vault, request proxy and launcher the desktop app uses. The original [CLI release plan](CLI-RELEASE-PLAN.md) fixed the 0.2 behaviour; release records: [0.5.x–0.6.x](evidence/release-0.5.md), [0.2](evidence/release-0.2.md). This file is an operational contract, not a claim of real-provider acceptance.

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
switchboard accounts move ACCOUNT_UUID --pool agents                  # a project's pool, or default (SB-122)
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

`continue` hands an [Observatory](CONTRACTS.md#workflow-continuation-sb-52-2026-10-05) workflow whose executor ran out of its limit to another account — of the same provider, or since 0.6.7 of the other one, Claude Code ↔ Codex, with the same checkpoint and constraints (SB-70) (`--mode isolated` by default, or `managed`). Since 0.6.31 `--account` also takes `opencode` — OpenCode on the saved OpenRouter key and its default model, with its own configuration for that session only (`OPENCODE_CONFIG_CONTENT`), the handoff as its first message (`opencode --prompt`) and never a change to your own OpenCode (SB-119); without `--account` the default agent continues (`switchboard agents default`, SB-120). Since 0.6.20 `--account` also takes a saved Kimi Code account's id (SB-99) or `hermes` — Hermes Agent on the saved OpenRouter key, on the model your Hermes is set to when it runs on OpenRouter (`switchboard agents hermes status`), else the key's default model (`switchboard agents openrouter model`); in a home Switchboard owns, its first turn the handoff (`hermes chat --query-file`, SB-107). The answer's `modelFrom` says which (`hermes` or `key`). With `--in-place` (0.6.25, SB-108) the continuing session runs in the calling terminal instead of a new Terminal window — the same session, home and first prompt — for an embedded console such as Fabric Dashboards'; it needs a terminal on stdin and stdout and refuses before anything is offered without one. It reads the workflow, refuses before anything changes when it cannot be continued here (closed, no checkpoint, a handoff already waiting, a declared key not in the vault, a workflow that names no executor, the same account, a folder outside the workflow's checkouts), offers it with reason `limit` and opens the account's session in Terminal. That session accepts the handoff itself, through the Observatory tools Switchboard declares in its config. The engine refuses an offer while the executor wrote a checkpoint less than two minutes ago; Switchboard reports that sentence and launches nothing. Set `SWITCHBOARD_OBSERVATORY_BIN` when `project-observatory` is not in `~/.local/bin`, Homebrew or `PATH`. macOS only for now.

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
switchboard mcp --read-only   # only the ten read tools; usage refresh shows the stored observation
switchboard --data-dir /path/to/app-data mcp
```

`switchboard mcp` speaks newline-delimited JSON-RPC 2.0 on stdin/stdout (MCP `2025-06-18`, also `2025-03-26` and `2024-11-05`; clients asking for `2025-11-25` — MCP Inspector 2.9.0, the TypeScript SDK 1.32.0 and client 2.3.0, the Python SDK 2.3.0 — accept `2025-06-18` and work: [matrix](runs/2026-10-04-sb-43-mcp-matrix/README.md), `scripts/mcp_compat.py`); stdout carries protocol messages only. It uses the running owner's control channel when the desktop app or `serve` runs, else an offline store operation under the exclusive lock. No tool accepts or returns a credential, and login and launch are not exposed. `switchboard_switch` with `target: "claude_cli"` changes the ordinary Claude Code login for every `claude` session and is refused unless the call carries `global: true`. `--read-only` lists the ten tools that only read (status, accounts, usage, `switchboard_sessions`, `switchboard_onboarding`, project context, chains, the OpenRouter key's status, Kimi accounts, Hermes's model) and none of the seven that write. A managed session that Switchboard launches gets the server automatically when the CLI is found (Claude through `--mcp-config`, Codex through `[mcp_servers]` in its private config); an isolated session gets the read-only set. Both also get the Fabric project board (`fabric`) when the operator registered it in that provider's user scope (SB-20).

A project reserves its accounts to its folders and their subfolders. A project has no folder limit; all projects together hold at most 1024 folders, and no folder may sit inside another project's (`a_project_has_no_folder_limit_and_all_projects_hold_1024`).

### Ordinary Claude Code in a project's folders uses the project's accounts (SB-118)

The ordinary `claude` is signed in once for every folder. So that sessions in a project's folders spend the project's accounts, Switchboard writes two keys into each folder's `.claude/settings.local.json`:

- `env.ANTHROPIC_BASE_URL`: the proxy route of the project's pool;
- `env.ANTHROPIC_CUSTOM_HEADERS`: `X-Switchboard-Capability: <capability>`, a capability for that pool only, sent as a header of its own (SB-140). It is not an auth source, so the session keeps its claude.ai login and the claude.ai connectors. Versions up to 0.6.37 wrote `ANTHROPIC_AUTH_TOKEN`, which turned the connectors off; the next sync replaces it.

This covers worktrees inside a project folder too. Everything else in the file stays as it was, and the file is kept out of Git through the repository's `info/exclude`. Your skills, plugins, MCP servers and `CLAUDE.md` stay in force. A session already running keeps its account until it restarts.

```sh
switchboard project bindings    # the folders bound, with their pool and route
```

Leaving a project removes only what Switchboard wrote, and `switchboard uninstall` removes every binding. Never print that file in a chat: its token opens the pool on this computer. See the [SB-118 packet](packets/sb-118-claude-bindings.md).

### A project's folders follow its work and the disk (SB-117, SB-121, SB-131)

Switchboard keeps a project's folder list by itself and never asks (SB-121, SB-131; operator
2026-10-14 and 2026-10-15). One system notification tells the person about each change.

A folder joins its existing project when it is:

- a git worktree of a project repository;
- a clone with the same `origin`;
- a repository of one of the project's sources (`host/owner`);
- a repository an agent working in the project changes (SB-131). An agent changes it when it edits or writes a file there (Claude Code `Edit`/`Write`), or commits, pushes, stages, merges, rebases, runs `git init` or makes a worktree there. Reading it does not count.

When no group matches, the repository stays in the default group: since SB-146 Switchboard never makes a group of its own for it (REQ-002 did, and scattered repositories that belonged together). Put it in a group with `project move-root`.

Where the signals come from:

- **Background pass.** It looks at the parent folder of every project folder and at every project repository's worktree list, re-reading either only when its modification time changed. It does not scan for unrelated new projects.
- **Recent Claude Code transcripts.** These are the sessions and their subagents of the last 15 minutes. They are read only where they grew, and only the paths are kept. Work in an unassigned session's Git root creates its project through the same path.
- **The plugin's `SessionStart` hook.** It reports the folder a Claude Code session starts in.

Folders that never join or create:

- a folder or project the person removed;
- a folder in a temporary or cache location (`/tmp`, `$TMPDIR`, `~/.cache`, `~/Library/Caches`);
- a folder that is not a git repository, the home folder or a filesystem root;
- a folder two projects match equally.

Codex sessions and sessions Switchboard launches isolated are not read yet (SB-132).

- **Tools' own folders.** A folder of a tool (`~/Library`, a hidden folder in the home folder such as `~/.claude`) never joins because an agent worked there. A worktree of a project repository kept there (`~/.codex/worktrees/…`) still joins as a worktree.
- **Shared repositories.** Suppose agents of two projects work in one repository, an organisation wiki for example. If it joined one project by work, it leaves that project and is never added by work again.
- **Effect of joining or creating.** For a provider with an enabled account in the project, accounts of other pools are refused there (SB-118). A provider with no enabled project account keeps its existing launch fallback; automatic `--provider` selection still requires the project's selected account.
- **When adding fails.** If the folder overlaps another project or the 1024-folder total is reached, nothing is asked. Switchboard tries again every ten minutes and logs `project_suggestion add: refused`.

A project folder deleted from disk leaves its project once it is still missing five minutes later (SCN-061).

- **Stays:** a folder on a drive that may only be unmounted (`/Volumes`, `/media`, `/mnt`), a folder behind a share that went away, and the project's last folder.
- **Renamed or moved:** a renamed or moved folder counts as deleted; add the new place with *Add folder…*.
- **Comes back:** a deleted folder that comes back is found again.

```sh
switchboard project suggest --folder /work/app-review   # attach to the group it belongs to (worktree, clone, worked in); a repository in no group stays in the default group (default: the current folder)
switchboard project suggestions                         # {suggestions: still being added, added: last 24 h, removed: taken out as deleted, last 24 h}
switchboard project accept /work/app-review             # add a folder still being added now
switchboard project dismiss /work/app-review            # never add it
switchboard project source --pool app --add github.com/example   # also adopt new repositories of this owner
```

`suggest --from-hook` reads Claude Code's hook input on stdin. It prints one line, which joins the session's context, only when the folder was added to its group, or an add could not finish yet, and it always exits 0. See the [SB-117 packet](packets/sb-117-project-suggestions.md), SCN-055 and SCN-061; `suggest::tests`, `activity::tests`, `an_unassigned_git_repository_stays_in_the_default_group_and_no_group_is_made`, `a_repository_may_be_in_several_groups_and_sessions_take_the_one_with_accounts`, `a_projects_new_folders_join_it_by_themselves_and_a_removed_one_stays_out`, `a_repository_a_projects_agent_changes_joins_it_and_a_shared_one_joins_neither`, `a_deleted_project_folder_leaves_its_project_once_it_stays_gone`, `an_unassigned_repository_stays_in_the_default_group_through_the_session_hook` and `project_suggestions_work_from_the_cli_and_the_hook_never_fails`.

Project rules are optional: a folder and its subfolders start on a chosen account, rules never stop rotation, and the desktop app lists them under Projects, where each can be paused.

```sh
switchboard project list                      # {projects, rules}: projects with folders, roots (each with its worktrees and clones) and accounts, then rules
switchboard project save --name "Client Alpha" --folder /work/alpha-web --folder /work/alpha-api --account <id>
switchboard project save --pool client-alpha --name "Client Alpha" --folder /work/alpha-web   # update; omitted accounts go back to default
switchboard project edit --pool client-alpha --add-account <id> --remove-folder /work/old   # SB-134: change by delta; the rest stays
switchboard project rotate-capability --pool client-alpha   # the folders' capability leaked: the old one is refused, a new one is written
switchboard project codex [on|off]     # SB-143: Codex sessions in project folders use the projects' Codex accounts (one provider in Codex's config.toml)
switchboard stats [--days 30]            # SB-145/SB-147: requests and the provider's token counts by group, project, account, agent and day (1-90 days)
switchboard project map [--under ~/DATA]   # SB-135: Git repositories on disk, each with its project or none (default ~/DATA, else home)
switchboard project move-root --folder /work/bot --pool client-beta   # SB-135: a repository with its worktrees and clones into another group, in one step
switchboard project move-root --folder /work/bot --pool tools --also   # SB-146: into one more group; it stays in its others (--from <group> picks the one it leaves)
switchboard project delete --pool client-alpha  # its accounts stay in the pool, no longer reserved
switchboard project show --path /path/to/project
switchboard project set --path /path/to/project --account ACCOUNT_UUID --expires-in-hours 8
switchboard project set --path /path/to/project --account ACCOUNT_UUID --target claude-cli --paused
switchboard project remove --path /path/to/project --provider claude
switchboard project apply --path /path/to/project
switchboard project apply --path /path/to/project --global
```

`--path` defaults to the current folder and must be absolute. `--target` is `managed` (default) or `claude-cli`; `--expires-in-hours` accepts 1 to 720; `--paused` saves the rule switched off. `apply` detects the calling session (managed, isolated or native) and reports one action per provider: `selected`, `activated`, `already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool`, `other_session`, `needs_global` or `failed`. A `claude_cli` rule changes the ordinary Claude Code login only with `--global`.

Connecting an agent. The `switchboard` executable must be on `PATH`; on macOS the desktop app's Agents panel links the bundled CLI to `~/.local/bin/switchboard`, and on Windows the installer puts `switchboard.exe` beside the desktop app (since 0.6.0, SB-05), where the Agents panel finds it and prints the registration commands with its full path; to type `switchboard` in a shell, add that folder to `PATH`. On Linux the .deb installs it as `/usr/bin/switchboard`, already on `PATH`; the AppImage's files exist only while it runs, so connect agents from a .deb install or the `-cli.tar.gz` ([INSTALL → Linux](INSTALL.md#linux)).

| Agent | Command |
|---|---|
| Claude Code, plugin (server plus the `switching-accounts` skill) | `claude plugin marketplace add passioncode-ai/fabric-switchboard`, then `claude plugin install switchboard@switchboard` |
| Claude Code, server only | `claude mcp add --scope user switchboard -- switchboard mcp` |
| PassionCode launcher, once the plugin is released as a member | `npx @passioncode-ai/passioncode@latest update` |
| Codex | `codex mcp add switchboard -- switchboard mcp` |

Restart the agent after installing; it loads servers and skills at session start. The plugin is validated by `python3 scripts/check_plugin.py`, `python3 -m unittest scripts/test_check_plugin.py` and `claude plugin validate ./plugins/switchboard --strict` plus `claude plugin validate . --strict`.

### Sessions (SB-95)

```sh
switchboard sessions          # Claude Code, Codex and Kimi Code sessions Switchboard launched that are starting or running
switchboard sessions --all    # also the ended ones (each home keeps its last launch's record)
switchboard --json sessions   # {sessions: [...]}
```

Each launch writes `.session.json` beside its `launch.command` (0600): `account_id`, `provider`, `pool`, `mode` (`managed`/`isolated`), `working_directory`, `in_place`, `launched_at` and `continues` (the workflow id a continuation picks up, or null) — no credential. The list adds `label`, `home` (`homes` or `runtimes`), `started_at` (when the script recorded its process) and `state`: `starting` (Terminal has not run the script yet), `running` (the recorded process is still that session — the pid alive and at least as old as its marker; on Windows, the pid with its creation time), `ended`, or `unknown` (the marker cannot be read). Only Switchboard's own homes are read (`homes`, `runtimes` and `kimi/sessions`) and no link is followed; a session started outside Switchboard is not listed. The MCP tool `switchboard_sessions` (`all?`) answers the same. A managed session follows its pool's selected account from the next request, so `account_id` is the account selected at launch.

### Setting up with a coding agent (SB-93)

`switchboard onboard` prints the guide a coding agent follows ([docs/AGENT-ONBOARDING.md](AGENT-ONBOARDING.md), built into the CLI). `switchboard onboard --status [--folder <absolute dir>] [--host claude|codex] [--json]` reads where setup stands and writes nothing — it creates no data folder:

```sh
switchboard onboard                         # the guide
switchboard onboard --status                # one line per step, with the command or the person's part
switchboard --json onboard --status --host codex --folder /work/app
```

The answer is `{complete, host, folder, steps[]}`; each step is `{id, state, summary, next}` in this order: `owner` (the app or `serve` answers), `accounts` (claude, codex and kimi accounts that are switched on), `project` (the folder's project; always optional), `mcp` (a `switchboard` server in the agent's config: Claude Code's `~/.claude.json` user or per-folder scope, the folder's `.mcp.json`, or the enabled Switchboard plugin; Codex's `config.toml` under `CODEX_HOME` or `~/.codex`) and `proving_call` (`mcp-proof.json` in the data folder, written by `switchboard mcp` after its first answered tool call: the client's name and the time). `state` is `done`, `todo` (`next.command` for the agent, `next.tool` for the proving call), `needs_person` (`next.person`, sometimes with a `next.command` the agent may run to start it) or `optional`. `--host` defaults to the agent the command runs inside (`CLAUDECODE=1`, or Codex's `CODEX_SANDBOX`/`CODEX_THREAD_ID`); unknown, any registration counts. The MCP tool `switchboard_onboarding` (`path?`, `host?`) answers the same.

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
accounts, Kimi Code on a saved Kimi Code account (`switchboard kimi`, SB-99), Hermes also on the
saved OpenRouter key (SB-107); any other agent on an API-key account or a paid key, never on a
subscription sign-in.
Removing an account or a project drops what pinned it. When a workflow's executor hits its limit,
the automatic fallback walks the chain that applies (0.6.8, SB-73: Claude Code and Codex;
Kimi Code on saved Kimi accounts since SB-99 — its first turn runs as `kimi -p`, without approvals,
then `kimi -c` resumes it only when that first turn succeeded; Hermes on the saved OpenRouter key since
SB-107; OpenCode on the same key and its default model since SB-119; an agent disabled on the Agents
page is skipped as `agent_disabled` (SB-120); other agents are skipped until SB-72); `switchboard continue` still hands a workflow over by hand.
`--mode managed` applies to Claude Code and Codex only; with Hermes, OpenCode or Kimi Code it is refused.

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

### Installed agents and the default agent (SB-119/SB-120)

```sh
switchboard agents status                 # what is installed, which take over work, the default, the command run
switchboard agents disable kimi-code      # keep an agent out of fallback chains and continue
switchboard agents enable kimi-code
switchboard agents default opencode       # the machine's default agent; `auto` clears it
switchboard agents models                 # the recommended OpenRouter models, then every model that can call tools
```

Installed means the agent's program is found the way launches find it: on `PATH`, in the usual
per-user folders (`~/.local/bin`, `~/.bun/bin`, …), Homebrew's, then the agents' own install
folders (`~/.opencode/bin`, `~/.kimi-code/bin`, `~/.hermes/bin`, …); version-manager shims are not
searched. Claude Code, Codex, Kimi Code, Hermes and OpenCode can take over a
workflow; each is enabled for taking over work until you disable it (account switching is not affected), and a disabled one is skipped in
fallback chains (`agent_disabled`) and refused by `continue`. The default agent is *Auto* or one
installed, enabled agent of those five; disabling it returns the machine to *Auto*.
`switchboard continue` without `--account` hands work to the default agent: OpenCode and Hermes on
the OpenRouter key, Claude Code, Codex and Kimi Code on whichever of their accounts has room now.
Fallback chains keep their own order. Fabric can read `default` (and `default_ready`) from
`switchboard --json agents status` unless a project sets its own. The choices are saved in
`agents-settings.json` (0600) and carried in encrypted backups.
`agents models` asks OpenRouter's public model list (no key, 10 s, kept 6 hours); when OpenRouter
does not answer, the five recommended models and any id you type still work. MCP:
`switchboard_agents` (read).

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
switchboard agents openrouter set --key-stdin --pool client-alpha   # SB-144: the key agents in that project's folders run on
switchboard agents openrouter list         # every saved key with its project, and the projects without one
switchboard agents key --service openrouter --dir /work/app   # the key of that folder's project, else the machine's (launch scripts call this)
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

## Capacity recovery (SB-114)

The running desktop app or `switchboard serve` owns the registry. The host that
knows a session stopped registers its exact binding; Switchboard does not infer
an instruction to resume from arbitrary transcript text.

```sh
switchboard recovery --json
switchboard recovery register limit-1 --session session-1 --account <id> --directory /work/project
switchboard recovery claim limit-1 --account <candidate-id>
switchboard recovery finish limit-1 resumed
# Or when the person cancelled the work:
switchboard recovery finish limit-1 cancelled
```

Status separates `limited_accounts`, `waiting_sessions`, `ready_sessions` and
`claimed_sessions`. `coverage: registered_sessions_only` and
`source: host_reported` are part of the contract: zero registered waits is not
proof that no external session is stopped. `limits` includes hidden-limit evidence.
Each ready row carries its stable stop event and compatible candidates. The host
polls status, notifies the session owner, claims immediately before acting, and
reports resumed only after observing that session continue. Child-agent state
belongs to the host's checkpoint. No command here sends a prompt or restarts a client.

Same-event registration and same-result finish are idempotent. Claim is deliberately
not replayable: after a lost response inspect the existing claim and reconcile the
host's receipt. The full private registry is bounded at 1024 records, including
completed receipts; reaching it requires reconciliation, never silent eviction.
See [SB-114 packet](packets/sb-114-capacity-recovery.md) and `recovery::tests`.

Recovery status separates `limited_accounts` (active holds) from
`awaiting_post_limit_probe_accounts` (ended holds awaiting a successful observation
made after the deadline). `limit_unavailable_accounts` includes both. A quota sample
made during a hidden hold cannot make the account ready when its timer ends.

### A limit stop is not a wait (SB-115)

A stop is ready in one of two modes, both in each status row as `mode`:

- `capacity`: a compatible account has a fresh successful quota observation (as above);
- `probe`: no such account, but the stop's backoff has passed. The backoff is 2, 5, 10, then
  15 minutes after the stop, by its `attempt`. Its `probe_at` says when. The provider's
  reset time is never waited for. `candidates` lists the accounts a probe may use, those
  without an active hold first.

A `billing` stop is never probed, and neither is a pool whose every compatible account is
held by a spend or credit cap: a payment is a person's step. A claim records its `mode` and
stays claim-once.

```sh
# Claude Code's StopFailure hook (shipped in the plugin) runs:
switchboard recovery hook < stop-failure.json
```

`hook` reads the hook's JSON on stdin and registers only `rate_limit` and
`billing_error` stops. The account is the one Switchboard charged that session's limit
to, else the ordinary Claude Code's. A second stop of a waiting session keeps its event,
one attempt later. A stop after a claim closes that claim as `resumed`, because the
session ran, and opens a new stop. It always exits 0: `registered: false` with
`reason` `malformed_input`, `not_a_limit` or `refused` (no owner, unknown account,
folder not allowed). The owner logs `recovery_hook` with a result code. The rule for
agents is fabric-workspace
[principle 10, item 6](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/principles.md#10-a-free-agent-takes-the-next-task).
See [SB-115 packet](packets/sb-115-limit-stop-resume.md); `recovery::tests` and
`recovery_hook_never_fails_the_session`.

## Console hand-overs (SB-108)

```sh
switchboard handovers --json
switchboard handovers set on
switchboard handovers run ho-0123456789ab
switchboard handovers set off
switchboard continue wf_0123456789abcdef --account opencode --dir ~/src/app --to-console
```

Off by default. With the setting on, automatic fallback prepares a continuation
and queues metadata for the host console; `continue … --to-console` (0.6.32) does the same for one
workflow and account a person names, and is refused before anything is offered while the setting
is off. `run` requires a terminal on stdin and
stdout and takes the record once. After 120 seconds without a taker, the monitor
requests a Terminal launch while the offer is live. Expired offers never launch.
The private queue stores no credential or prompt. Fabric Dashboards' consumer
(FD-62) is separate from this CLI producer; an available command is not evidence
that the installed console has consumed it. Checks: `handovers::tests` and
`hand_overs_are_listed_and_set_offline_and_run_only_in_a_terminal`.
