# Dossier A: context binding for plain sessions and third-party launches

One dossier per cluster of inventory items, written in parallel with the others, read-only on
every repository: the dossier is the only write. **A dossier is a design, not a plan**: it does not
cut waves. Evidence for every claim: `file:line @a9ff8fa`, or a command and its output. No secret
values.

Written 2026-10-26 against: `passioncode-ai/fabric-switchboard` @ `a9ff8fa` (worktree
`/Users/sshlg/DATA/_worktrees/sb-0.7.0`, branch `iteration/0.7.0`). Other repositories
(fabric-workspace, passioncode-vesta, fabric-agent-contract) are cited only as the SB-110 design
cites them (`docs/research/session-home-per-context-2026-10-24.md:8-17`); none was read here.

## Contents

- 1. Items
- 2. As-is
- 3. To-be
- 4. Options considered
- 5. Contracts touched
- 6. Nodes
- 7. Deletes
- 8. Questions
- 9. Out of scope / program

## 1. Items

| id | title (short) | still holds? (yes / no + evidence / changed) | severity |
|---|---|---|---|
| SB-183 | `switchboard shell init zsh\|bash\|fish` (+ PowerShell): plain `claude` and `codex` in an organization's group folders get that group's home and `SWITCHBOARD_CONTEXT`, restored on leaving | **yes, changed.** No `shell` command exists: `grep -n "Shell" crates/switchboard-cli/src/main.rs` prints nothing, and `grep -rln "shell init\|shell_hook" crates/ src/ src-tauri/src` prints nothing. Changed twice since the design: "opt-in" (`docs/research/session-home-per-context-2026-10-24.md:54`, H-3) became **on by default on a machine holding an organization group, off by one command** (D-3, `docs/reports/2026-10-25-audit-and-plan/README.md:248`); PowerShell joins zsh, bash and fish (`docs/evidence/backlog.md:193`, entry 2026-10-26; X-16). Due 2026-11-14 (`docs/evidence/backlog.md:193`). | P2 (backlog), but it is item 1 of release 0.7.0 and the gap that keeps Vesta wave 2 on launched sessions only (`docs/reports/2026-10-25-audit-and-plan/README.md:136`, `:168-169`) |
| SB-201 | Kimi Code, Hermes and OpenCode sessions record their context (`SWITCHBOARD_CONTEXT`, badge, `switchboard sessions`) as Claude Code and Codex do since 0.6.80 | **yes.** Kimi's launch record has no `context` and its env no `SWITCHBOARD_CONTEXT` (`crates/switchboard-runtime/src/kimi.rs:768-793`; continuation `:921-946`); Hermes continuation record and env likewise (`crates/switchboard-runtime/src/launch.rs:1773-1819`); OpenCode continuation likewise (`launch.rs:1928-1971`). Direct OpenRouter launches (`launch.rs:2058-2152`) and proxy agent launches (`launch.rs:1560-1604`) write no session record at all, so they are not in the list. | P2 (`docs/evidence/backlog.md:211`) |
| SB-110 / SB-111 (context only) | phase 1 and the badge, the base both items build on | **closed**, done in 0.6.80 (`docs/evidence/backlog.md:117-118`); read here only as the as-is. Not reopened. | n/a |

## 2. As-is

**A launched session already records its context (SB-110 phase 1).**
- `Snapshot::session_context(account, folder)` gives `personal` or `org:<id>`: the group of the
  account's pool, else the group holding the folder, else the account's own organization; a group
  and an account that disagree give `personal` (`crates/switchboard-core/src/organizations.rs:195-210`).
  Its doc comment shares lines with `context_admits`'s: `organizations.rs:187-189` describe
  `context_admits` (line 211), which has no comment of its own.
- `launch_full` puts it in the env as `SWITCHBOARD_CONTEXT` and in `.session.json` as `context`
  (`crates/switchboard-runtime/src/launch.rs:2452-2459`, `:2569-2584`); the inherited value is
  scrubbed like `SWITCHBOARD_SESSION` (`launch.rs:48-50`, used by the script header `launch.rs:353`).
- `Operation::Sessions` adds `context_name` from the record, for any provider
  (`crates/switchboard-runtime/src/lib.rs:4285-4291`); the CLI prints it after the label
  (`crates/switchboard-cli/src/main.rs:2015-2019`); the window shows the badge from
  `session.context` for any row (`src/main.ts:1864-1866`), and `SessionInfo.provider` already
  allows `'kimi' | 'hermes' | 'opencode'` (`src/types.ts:20`). So a record that carries `context`
  gets the badge and the CLI label with no window change.
- `launch::sessions` scans `homes`, `runtimes`, `kimi/sessions`, `runtimes/hermes-continue`,
  `runtimes/opencode-continue` and lists only folders holding a `.session.json`
  (`launch.rs:2599-2622`). CONTRACTS still says Kimi sessions are "not covered yet"
  (`docs/CONTRACTS.md:595-596`), which is stale: Kimi writes a record (`kimi.rs:780-793`).

**Third-party launches carry no context (SB-201).**
- Kimi: env is `KIMI_CODE_HOME` only, record `{account_id, provider: kimi, pool: default, ...}` with
  no `context` (`kimi.rs:768-793`; continuation `kimi.rs:921-946`). A `KimiAccount` has no
  organization attribution (`crates/switchboard-core/src/kimi_accounts.rs:17-29`), and the
  organization code never mentions Kimi (`grep -n -i kimi crates/switchboard-core/src/organizations.rs`
  prints nothing). The Kimi launch runs no `project_allows` check (`kimi.rs:754-800`), unlike Claude
  Code and Codex (`launch.rs:1523-1530`).
- Hermes and OpenCode continuations: records without `context`, env without
  `SWITCHBOARD_CONTEXT` (`launch.rs:1773-1819`, `:1928-1971`). Their key is chosen per folder:
  the group's own OpenRouter key, else the machine's (`crates/switchboard-core/src/agent_keys.rs:214-237`).
- Direct OpenRouter launches (`openrouter_session`, `launch.rs:2058-2152`) and proxy agent launches
  (`launch_agent`, `launch.rs:1560-1604`, env `SWITCHBOARD_SESSION` only) write no `.session.json`,
  although their homes sit under the scanned `runtimes/` and the script writes `.session-pid`
  (`launch.rs:370`).

**Plain `claude` in a group folder is already routed, but not re-homed (SB-118, SB-140, SB-133).**
- For each group with an enabled Claude account, every folder and every worktree inside it gets
  `.claude/settings.local.json` `env` = `ANTHROPIC_BASE_URL` (the pool's proxy route) and
  `ANTHROPIC_CUSTOM_HEADERS` = `X-Switchboard-Capability: <folder capability>`
  (`crates/switchboard-runtime/src/bindings.rs:188-247`, `:365-413`); the capability is per folder
  (`crates/switchboard-proxy/src/lib.rs:165-170`). No `ANTHROPIC_AUTH_TOKEN` is written (SB-140,
  `bindings.rs:20-25`, `:405`). A folder whose values are the person's is left alone
  (`bindings.rs:383-390`, `Applied::UserValues`).
- The proxy accepts a folder capability as a header for its pool and ignores the bearer then
  (`crates/switchboard-proxy/src/lib.rs:574-605`).
- So plain `claude` in a bound folder **already spends only the group's accounts**. What it does
  not change: the home stays the person's (`~/.claude` or their `CLAUDE_CONFIG_DIR`), with their
  sign-in, history and transcripts, auto-memory, user-scope MCP servers (`~/.claude.json`), plugins
  and settings; nothing tells the session or its hooks which context it is in. A settings file
  cannot move the home: `CLAUDE_CONFIG_DIR` is read before any settings file
  (`docs/research/session-home-per-context-2026-10-24.md:35-39`).

**Plain `codex` follows SB-143, off by default.** With `<data>/codex-route` = `on`, the owner
writes a marked provider block into the person's `<CODEX_HOME or ~/.codex>/config.toml` pointing
at `/codex/_auto/v1` with a machine capability (`crates/switchboard-runtime/src/codex_route.rs:1-21`,
`:97-123`); the proxy maps the request's `<cwd>` to a group with a Codex account, else forwards it
with the person's own sign-in (`switchboard-proxy/src/lib.rs:915-1000`, `:1055-1075`). The folder
map is set on every bindings pass whatever the switch (`crates/switchboard-runtime/src/lib.rs:950-952`,
`:1883`). On this machine no group has a Codex account (`docs/evidence/backlog.md:152`).

**The 2026-10-24 measurement (H-3).** Claude Code 2.1.299 with an empty `CLAUDE_CONFIG_DIR` and a
local stub: no credential gives "Not logged in"; `ANTHROPIC_AUTH_TOKEN` in the env, or in the
home's own `settings.json` `env` beside `ANTHROPIC_BASE_URL` and the capability header, runs with no
sign-in and sends `Authorization: Bearer` and `X-Switchboard-Capability`
(`docs/research/session-home-per-context-2026-10-24.md:54`). Not measured: the split this design
uses (token in the home's `settings.json`, route and header from the folder's
`settings.local.json`), and whether that token brings back SB-140's "claude.ai connectors are
disabled" notice (`docs/evidence/backlog.md:148`).

**Homes and what reads the person's folders.**
- Launched homes: `runtimes/<provider>-<pool>` (managed) and `homes/<account>` (isolated)
  (`launch.rs:2388-2400`). A managed Claude home's `settings.json` is rewritten at each launch
  from the carried settings (`launch.rs:2423-2437`); a managed Codex home's `config.toml` is
  rewritten with `env_key = "SWITCHBOARD_LOCAL_TOKEN"`, the proxy's session token
  (`launch.rs:2490-2493`).
- Carry links `CLAUDE.md`, skills, commands, agents, output styles, plugins, hooks and strips
  `env` and secret-shaped keys from `settings.json` (`docs/CONTRACTS.md:68`,
  `crates/switchboard-runtime/src/carry.rs:11`, `:50`, `:130`, `:150`); links are made on macOS and
  Linux only (`docs/CONTRACTS.md:68`).
- These read the **process's own** `CLAUDE_CONFIG_DIR` or `CODEX_HOME` as the person's:
  `carry::claude_user_dir` (`carry.rs:34-39`), `launch::user_scope` (`launch.rs:575-583`),
  `launch::board_server` (`launch.rs:232-236`), `limits::transcript_files` (`limits.rs:641-650`),
  `codex_route::config_path` (`codex_route.rs:51-56`), `agents.rs:57`, `main.rs:1776`, and the
  native sign-in context (`crates/switchboard-runtime/src/external.rs:243-279`), which refuses a home
  inside Switchboard's data folder with "This session runs in a Switchboard home..."
  (`external.rs:1877-1890`, used at `:1921`, `:1966`).

**Shell and setting precedents.** No shell integration exists. Close precedents: marker-line edits
of a person's file that come out byte-identical (`codex_route.rs:16-18`, `:71-90`, `:117-122`);
a per-machine choice file whose absence means the default and whose writer acts only on an
installed build (`src-tauri/src/residency.rs:15-55`, `at_start(choice, packaged)`); shell quoting
for POSIX and PowerShell (`launch.rs:259`, `:426`); uninstall's owned list and side effects
(`crates/switchboard-runtime/src/uninstall.rs:18-56`, `:244-262`; `docs/CONTRACTS.md:236-237`).
The window's Groups page holds the SB-143 section (`src/main.ts:396`).

**Scenarios.** SCN-051 (session list and badge, `docs/ux/scenarios.md:1073-1092`), SCN-056
(project folders on project accounts, `:1195-1213`), SCN-066 (Codex in group folders,
`:1378-1398`), SCN-070 (a group on its organization's accounts, `:1465-1487`). The last scenario id
is SCN-073 (`grep -oh "SCN-0[0-9][0-9]" -r docs/ | sort -u | tail -1`).

## 3. To-be

One design, two parts: **A** a terminal hook that moves plain sessions into the group's context
(SB-183), **B** context on every third-party launch (SB-201). Both use one core rule for a
context.

### 3.1 One context rule (answers SB-183 and SB-201)

`Snapshot::context_for(own: Option<&str>, pool: Option<&str>, folder: Option<&Path>) -> String`
in `organizations.rs`, where `own` is the organization of whatever pays (account, Kimi account,
key), `pool` the pool it serves, `folder` the working folder. Same rule as today: the group of the
pool, else the group of the folder, else `own`; disagreement gives `personal`; `personal` while no
organization exists. `session_context(account, folder)` becomes
`context_for(attribution(account).organization, Some(&account.pool), folder)`, so 0.6.80's
behaviour and its test `a_sessions_context_follows_its_group_else_its_account_and_the_more_private_wins`
(`organizations.rs:647`) stay. The misplaced comment lines (`organizations.rs:187-189`) move onto
`context_admits`.

### 3.2 Part B: third-party launches (SB-201)

| Launch | `own` | `pool` | env | record |
|---|---|---|---|---|
| Kimi launch and continuation (`kimi.rs:754`, `:894`) | none (a Kimi account has no organization) | none | add `SWITCHBOARD_CONTEXT` | add `context` |
| Hermes and OpenCode continuations (`launch.rs:1726`, `:1895`) | organization of the group whose key `agent_key_in` picked, none for the machine's key | that key's pool | add | add |
| Direct OpenRouter launch (`launch.rs:2058`) | as above | as above | add | **new** `.session.json` (`provider` = recipe id, `pool: openrouter`, `label` = model) |
| Proxy agent launch (`launch.rs:1560`) | `session_context(&account, folder)` | account's pool | add | **new** `.session.json` (`provider` = agent id) |

A Kimi session in an organization's folder therefore reads `personal` (the more private wins:
it runs on a personal subscription). That is truthful, not a refusal; whether Kimi should be
refused there as SB-109 refuses foreign Claude and Codex accounts is Q6. Nothing in the window
changes except that `SessionInfo.provider` widens to any agent id (`src/types.ts:20`), so the
new rows render; the badge code is generic (`src/main.ts:1864-1866`).

### 3.3 Part A: the terminal hook (SB-183)

**What it adds beyond SB-118.** SB-118 already sends a bound folder's plain `claude` to the
group's accounts. The hook adds, only in folders whose effective group (`Snapshot::project_for`,
`projects.rs:297-306`, personal groups first) is bound to an organization:
1. **The home.** `CLAUDE_CONFIG_DIR` and `CODEX_HOME` point at the group's shell homes, so the
   session's sign-in, transcripts, auto-memory, user-scope MCP servers, settings and plugins are
   the organization context's, not the person's personal ones; personal history never mixes
   with the company's (CX-26, the reason SB-110 exists).
2. **The context.** `SWITCHBOARD_CONTEXT=org:<id>`, for hooks, Observatory and Fabric (H-1's
   form).
3. **Codex in organization folders without the SB-143 switch.** A group with a Codex account gets
   its Codex routed by the shell home's own config, whatever `<data>/codex-route` says; the
   person's `~/.codex/config.toml` is never touched for this.
4. **No personal sign-in in the request at all.** Today the person's OAuth bearer still reaches
   the local proxy, which discards it (`switchboard-proxy/src/lib.rs:574-605`); a shell home has
   no personal sign-in to send.
A personal group, a folder in no group, a folder SB-118 skipped (`Applied::UserValues`) and a
session Switchboard launched (`SWITCHBOARD_SESSION` set) change nothing.

**Homes (H-2 kept: keyed by pool, never by context).** `<data>/shell/claude-<pool>` and
`<data>/shell/codex-<pool>`, written by the owner's bindings pass, only for pools of
organization-bound groups:
- Claude: carried links (as SB-137, `carry::link_into`), `.claude.json` from `carry::claude_state`
  (onboarding done, trust question kept), and `settings.json` = `carry::claude_settings(user,
  Some({"ANTHROPIC_AUTH_TOKEN": "switchboard-shell"}))`. The value is a fixed placeholder, not a
  secret: the folder's `settings.local.json` supplies the route and the capability header, and the
  proxy authenticates by the header alone. The Fabric board server goes into `.claude.json`
  `mcpServers` with the same SB-20 rule launches follow (`launch.rs:170-236`), because a plain
  `claude` takes no `--mcp-config`.
- Codex: `AGENTS.md` link and `config.toml` = provider `switchboard-auto` at
  `http://<proxy>/codex/_auto/v1`, `requires_openai_auth = false`, header `X-Switchboard-Capability`
  = the machine's auto capability (the value SB-143 already writes, `codex_route.rs:97-123`). A
  folder the proxy does not map goes on with no sign-in and is refused upstream: closed, never
  the person's account.
- Why not the launched managed homes (H-3's literal text): a managed Claude home's `settings.json`
  is rewritten at every launch and its credential arrives by env (`launch.rs:2423-2437`,
  `:2486-2488`), so a placeholder there races the launch's master token with unmeasured
  precedence; a managed Codex home names `env_key = "SWITCHBOARD_LOCAL_TOKEN"`
  (`launch.rs:2491-2492`), which a plain shell must never hold. Option table row 2; Q1.
- Rewritten when the proxy port moves (beside `repoint_managed`, `launch.rs:591`), when the auto
  capability rotates, when carried files change, and when a pool loses its organization (its
  shell homes stay on disk as history, like launched homes at uninstall, `uninstall.rs:57-71`).
- Written only when the bindings fingerprint moves (`lib.rs:946-981`), so the idle budget
  (`AGENTS.md` "Idle budget") is unchanged.

**The folder map.** The same pass writes `<data>/shell/map.json` (0600, schema-versioned):
`{version: 1, rows: [{folder, pool, context, claude: bool, codex: bool}]}` where `claude` is true
only for folders the binding record says are bound (`claude-bindings.json`, `bindings.rs:146`
per `docs/CONTRACTS.md:146`), `codex` only for pools with an enabled Codex account
(`codex_route::folders`, `codex_route.rs:26-48`). Folders and organization ids only; no
capability, no token.

**`switchboard shell env --shell <zsh|bash|fish|pwsh> --dir <path>`.** Reads `map.json` directly
(at most 1 MiB), never the store lock and never the owner's control port, because owner answers
stalled up to 38 s on 0.6.82 (`docs/reports/2026-10-25-audit-and-plan/README.md:157`). Longest
folder prefix wins. Prints one line: `none`, or `<pool>\t<context>\t<claude_home or ->\t<codex_home
or ->`. Also `none` when `<data>/shell-hook` reads `off`, the map is missing, unreadable or of an
unknown version. Always exit 0, never prints to stderr: a terminal must never break because of
Switchboard. Budget: 25 ms per call, measured in a test.

**The hook scripts** (`<data>/shell/hook.zsh|bash|fish|ps1`, written by the owner, also printed by
`switchboard shell init <shell>` for people who keep their own dotfiles). Each embeds the absolute
path of the bundled CLI (`launch::agent_cli`, `launch.rs:58`) and the data folder, quoted with the
existing quoting (`launch.rs:259`, `:426`). Triggers: zsh `add-zsh-hook chpwd` plus one call at
load; bash a `PROMPT_COMMAND` entry that acts when `$PWD` changed (string form: macOS ships bash
3.2); fish `--on-variable PWD`; PowerShell a wrapped `prompt` that acts when the location changed
(works in 5.1 and 7). One transition, the same in all four:
- Entering a target while inactive: save the person's `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
  `SWITCHBOARD_CONTEXT` (value or "was unset") in **shell-local, unexported** variables, then
  export the targets. Unexported, so `claude`, `codex` and the CLI started from that shell never
  see the person's saved values.
- Moving to another target: export the new ones; saved values stay.
- Leaving: restore exactly (set back, or unset), clear the shell-local state.
- A variable the person changed while active (current value is not what the hook set) is
  theirs: the hook stops managing that variable and never restores over it.
- `SWITCHBOARD_SESSION` set (a launched session's shell): do nothing.
- On load it exports `SWITCHBOARD_SHELL_HOOK=1`, a constant, so `switchboard shell status` can say
  whether the current terminal has the hook.
- The CLI path missing (app removed) or the answer not one of the forms: do nothing, print nothing.

**On by default (D-3), safely.**
- The choice: `<data>/shell-hook` = `on`, `off`, or absent (= default). Default means installed
  while the machine holds at least one group bound to an organization, removed when none remains.
  `on` installs even without one; `off` removes and never re-adds.
- Who writes: **the installed app's owner only** (as the login item, `residency.rs:36-55`): never
  `serve`, a development build, `--data-dir` or a test. A CLI-only person runs
  `switchboard shell on`. When: at owner start and when the presence of an organization group
  flips in a bindings pass. Never on a timer.
- Where, only for shells present on the machine: zsh `${ZDOTDIR:-$HOME}/.zshrc` (created if zsh
  is the login shell and none exists); bash `~/.bashrc` on Linux, and on macOS the first existing
  of `~/.bash_profile`, `~/.bash_login`, `~/.profile` (else `~/.bash_profile`), only when bash is
  the login shell or that file exists; fish its own file
  `~/.config/fish/conf.d/fabric-switchboard.fish` (no edit of the person's file), only when
  `~/.config/fish` exists; PowerShell `profile.ps1` under the Documents folder resolved through
  the OS known-folder (OneDrive-redirected Documents included) for each edition installed
  (`PowerShell\` for 7, `WindowsPowerShell\` for 5.1).
- What: one block between `# >>> Fabric Switchboard: shell hook (SB-183) >>>` and
  `# <<< Fabric Switchboard <<<`, holding one guarded line that sources the hook file only if it
  is readable (zsh and bash `[ -r '<file>' ] && . '<file>'`; PowerShell
  `if (Test-Path -LiteralPath '<file>') { . '<file>' }`), so an uninstalled app leaves a
  harmless line at worst. Nothing outside the markers changes; taking the block out gives the file
  back byte for byte (the SB-143 property, `codex_route.rs:117-118`).
- Never written: a symlinked rc file (dotfile managers), a file not owned by the user, a file not
  valid UTF-8, a file that changed between read and rename (compare size and mtime, retry next
  time). The file's mode is kept; the write is a same-directory rename.
- Never fought: a block the person removed by hand is recorded `removed_by_person` and not written
  back until `switchboard shell on`.
- Record: `<data>/shell-hook.json` (0600): per file, the shell, path, block digest, state
  (`written`, `removed_by_person`, `skipped_symlink`, `skipped_owner`, `unreadable`,
  `policy_blocked`), time. Codes only in the log (event `shell_hook`, LC-12); no analytics event,
  so `docs/ANALYTICS.md` is untouched.
- Windows: the execution policy is read, never changed (`powershell -NoProfile -NonInteractive
  -Command Get-ExecutionPolicy`, 5 s, killed and reaped, only when installing). `Restricted`
  or `AllSigned` means profiles do not load or the unsigned hook is refused: state
  `policy_blocked`, said in the window and `shell status`. Q4.
- Off by one command: `switchboard shell off` (and the window's switch, and MCP) writes `off`,
  removes every block and the fish file. Terminals already open keep the functions until closed,
  but their next directory change gets `none` (the CLI honours `off`) and restores the person's
  values. `switchboard uninstall` removes the blocks before the data folder goes, as it does for
  bindings (`uninstall.rs:244-248`), and adds `shell`, `shell-hook`, `shell-hook.json` to `OWNED`.
- Visible: the first time the owner writes a block, the app posts one notification naming the
  organization and saying new terminals pick it up, with the way to turn it off (scenario
  SCN-074 below). The Groups page gets a section beside *Codex in group folders*
  (`src/main.ts:396`) with the state and *Turn off* / *Turn on*; CLI `switchboard shell
  status|on|off|init <shell>|env`; MCP `switchboard_shell_hook` (read and set), for dashboard
  parity.

**Processes started in a hooked terminal see a shell home as `CLAUDE_CONFIG_DIR`.** Every place
that takes the process's own value for the person's folder (`carry.rs:34-39`, `launch.rs:232-236`,
`:575-583`, `limits.rs:641-650`, `codex_route.rs:51-56`, `agents.rs:57`, `main.rs:1776`) goes
through one helper, `person::claude_dir()` / `person::codex_dir()`: a value inside Switchboard's
data folder is never the person's, so the default location is used instead. This matters for
`serve` or an offline CLI started in such a terminal (carry would link from the shell home, the
limit scan would read the wrong transcripts, SB-143 would write into the shell home). The native
sign-in context (`external.rs:243-279`) keeps its refusal: an offline native operation from a
hooked terminal is refused as from any Switchboard home (`external.rs:1887`), which is the safe
answer; its text gains "or a terminal in an organization's folder" so the person knows why.

**Failure handling, in one place.** Owner down: the shell home still points at the proxy, as
SB-118's folder files already do today, so plain `claude` fails to connect in both cases; plain
`codex` in an organization folder fails too (with SB-143 it would reach OpenAI on the person's
sign-in when Switchboard is not running, `codex_route.rs:9-11`): closed, stated in the scenario.
Map stale: corrected at the next directory change after the pass (at most 30 s). Hook file
missing: guarded line does nothing.

## 4. Options considered

| Choice | Options | Chosen | Why |
|---|---|---|---|
| How a plain session gets its environment | (a) rc-file hook on directory change (zsh chpwd, bash PROMPT_COMMAND, fish PWD, PowerShell prompt); (b) direnv-like: an `.envrc` per group folder; (c) a `claude` / `codex` shim on `PATH` that sets the environment and execs the real binary | (a) | (b) needs direnv installed, a person's `direnv allow` per folder (a human step per folder) and a file in every repository; (c) still needs an rc edit to put the shim first on `PATH`, shadows the real binary (Claude Code's installer and `claude doctor` see two installs), must find and exec the right binary, and still misses the VS Code extension, which runs its own bundled binary. (a) needs one edit per shell, keeps the move visible and reversible (H-3), costs nothing between directory changes. What (a) misses (non-interactive shells, IDE-spawned sessions) stays routed by SB-118 and SB-143 as today; section 9 |
| Which home a plain session gets | (a) the launched managed home `runtimes/<provider>-<pool>` (H-3 text); (b) a shell home per pool `shell/<provider>-<pool>`; (c) a home per context `contexts/<id>` | (b) | (a) shares one `settings.json` that every managed launch rewrites (`launch.rs:2423-2437`) and whose credential arrives by env with unmeasured precedence against the placeholder; its Codex config needs the session token in the env (`launch.rs:2491-2492`), which a shell must never hold. (c) is H-2's rejected key. (b) keeps H-2 (keyed by pool), cost: `--resume` in a plain terminal does not list conversations of launched sessions of the same group (Q1) |
| How the shell home signs in | (a) placeholder `ANTHROPIC_AUTH_TOKEN` in the home's `settings.json`, route and header from the folder; (b) the hook exports the placeholder; (c) `apiKeyHelper` printing a constant; (d) the folder capability in the home | (a) | (b) leaks a bogus token into every SDK script run in that folder; (c) is also an auth source (same SB-140 notice) and needs a helper binary; (d) is impossible: a home serves many folders, a capability is per folder (`switchboard-proxy/src/lib.rs:165-170`). (a) is the measured H-3 shape; the split with the folder file is measured first (node A-0) |
| How Codex is routed from the shell home | (a) `_auto` provider with the machine capability, no sign-in in the home; (b) `/codex/<pool>` with the session token in the env; (c) `/codex/<pool>` with a folder capability from an env variable | (a) | (b) puts the master token in a shell; (c) exports a capability to every process in the terminal (SB-139 is exactly a capability printed into a transcript). (a) reuses SB-143's accepted threat and fails closed for an unmapped folder |
| How `shell env` answers | (a) read a published map file, no lock; (b) ask the owner over control; (c) open the store | (a) | (b) stalled up to 38 s on 0.6.82; (c) contends with the owner's `instance.lock` and takes the store's validation cost on every `cd` |
| Where the saved personal values live | (a) shell-local unexported variables; (b) exported `SWITCHBOARD_OWN_*` variables | (a) | (b) would reach `claude` and the CLI started in the terminal and let an agent in an organization's context resolve the person's own home (the native refusal `external.rs:1877-1890` would no longer hold) |
| Default-on mechanics (D-3) | (a) the installed app's owner writes the marked block when the first organization group exists, notifies once, removes it when none remains, `off` wins; (b) the installer writes it on every machine; (c) the window asks once (opt-in); (d) print the line and wait for the person | (a) | (b) edits rc files on machines with no organization, against D-3's condition; (c) and (d) are opt-in, which D-3 replaced. (a) follows the login item's proven rule (installed build only, choice file, `residency.rs:36-55`) and the SB-143 marker property |
| rc line content | (a) source a file Switchboard writes, guarded by a readability test; (b) `eval "$(switchboard shell init zsh)"` | (a) | (b) spawns the CLI at every shell start, needs `switchboard` on `PATH`, and prints an error in every new terminal once the app is gone |
| bash file on macOS | (a) `.bashrc` everywhere; (b) the login-shell file on macOS, `.bashrc` on Linux | (b) | Terminal.app starts login shells, which do not read `.bashrc` |

## 5. Contracts touched

| Contract | Owner (repository / session) | Change | Owner must agree first? |
|---|---|---|---|
| `docs/CONTRACTS.md` new section "Shell hook (SB-183)": files, map schema v1, `shell env` answer, block markers, choice file, record, uninstall | fabric-switchboard | new | no |
| `docs/CONTRACTS.md` "Session list" (`:574-598`): context on Kimi, Hermes, OpenCode, OpenRouter and proxy agent launches; their records | fabric-switchboard | additive | no |
| `.session.json` record: new writers (OpenRouter and proxy agent launches); `provider` may be any agent id | fabric-switchboard (readers: window `src/types.ts:20`, CLI `main.rs:2005-2030`, MCP `switchboard_sessions`) | additive | no |
| `SWITCHBOARD_CONTEXT` in a plain session's environment (form `personal` / `org:<id>`, H-1) | fabric-switchboard; readers Observatory, Fabric (fabric-agent-contract `contexts.md`, as cited at `docs/research/session-home-per-context-2026-10-24.md:45`) | additive: present for plain sessions in organization folders only | no (same form as 0.6.80); tell them on the integration thread |
| The person's rc files and PowerShell profiles | the person (D-3 is the operator's decision to write them by default) | new persistent edit, marked and reversible | operator decided (D-3, `docs/reports/2026-10-25-audit-and-plan/README.md:248`) |
| `uninstall::OWNED` and uninstall's side effects (`uninstall.rs:18-56`, `:244-262`) | fabric-switchboard | additive (`shell`, `shell-hook`, `shell-hook.json`; remove blocks) | no |
| `AGENTS.md` lifecycle table: Writes (rc files, shell homes, map), Child processes (PowerShell policy read) | fabric-switchboard | additive | no |
| CLI surface: `switchboard shell status\|on\|off\|init <shell>\|env` | fabric-switchboard | new | no |
| MCP tool `switchboard_shell_hook` (`crates/switchboard-cli/src/mcp.rs`) | fabric-switchboard; plugin skill `switchboard:switching-accounts` may mention it | new | no |
| Scenarios: SCN-074, SCN-075 (new), SCN-051, SCN-056, SCN-066 (amended) | fabric-switchboard `docs/ux/scenarios.md` via `super-ux` | new and amended | no; written before the window and CLI messages |
| Vesta rollout wording "SB-110 binds plain sessions by 2026-11-14" (as cited at `docs/research/session-home-per-context-2026-10-24.md:66-68`) | passioncode-vesta | wording: on by default, home and accounts bound, not git, SSH or `gh` (SB-184) | no; inform on the board |
| `docs/research/session-home-per-context-2026-10-24.md` H-3 | fabric-switchboard | amended: default-on (D-3), PowerShell, shell homes instead of managed homes | no |

## 6. Nodes

| id | repo | owns (files) | does | tests (★ = fails today) | risk | deps | one pass? |
|---|---|---|---|---|---|---|---|
| A-0 | fabric-switchboard | `docs/evidence/shell-home-measurement-2026-10.md` (new), `scripts/measure_shell_home.sh` (new) | Measures on the installed Claude Code and Codex with a local stub: placeholder token in the home plus route and header from the folder's `settings.local.json`; whether the SB-140 connectors notice appears; Codex with the `_auto` config and no `auth.json` from a mapped and an unmapped folder; `shell env` cold timing | ★ the record does not exist (`ls docs/evidence/shell-home-measurement-2026-10.md` fails) | R1 | none | yes |
| A-1 | fabric-switchboard | `crates/switchboard-runtime/src/shell/map.rs` (new), `crates/switchboard-cli/src/shell.rs` (new), `crates/switchboard-cli/src/main.rs` (the `Shell` command only) | Map schema, its reader, `shell env` and `shell status` | ★ `shell::map::tests::the_longest_folder_of_an_organization_group_answers_its_homes_and_context`; ★ CLI `shell_env_answers_without_the_owner_or_the_store_lock_within_25_ms`; ★ `shell_env_says_none_when_off_missing_unknown_version_or_outside`; ★ `shell_env_never_writes_to_stderr_and_exits_zero` | R2 | A-4 (writer) can land after; format defined here | yes |
| A-2 | fabric-switchboard | `crates/switchboard-runtime/src/shell/scripts.rs` (new), `shell/hook.zsh`, `hook.bash`, `hook.fish` (templates, new) | The three POSIX-family hook scripts and `shell init <shell>` | ★ `shell::scripts::tests::entering_and_leaving_restores_unset_and_set_values` (runs real `zsh` and `bash` from the test; fish when installed, else skipped with the reason); ★ `a_value_the_person_set_inside_is_never_restored_over`; ★ `a_launched_sessions_shell_is_never_touched`; ★ `the_saved_values_are_not_exported`; ★ `a_missing_cli_prints_nothing`; ★ `bash_3_2_syntax_is_accepted` (`bash -n` on macOS `/bin/bash`) | R3 | A-1 | no |
| A-3 | fabric-switchboard | `crates/switchboard-runtime/src/shell/install.rs` (new), `crates/switchboard-runtime/src/uninstall.rs`, the owner-start hunk of `crates/switchboard-runtime/src/lib.rs` | Choice file, default-on rule, marked block in, block out, record, notification event to the app, `shell on\|off`, uninstall removal | ★ `install::tests::the_block_goes_in_and_comes_out_leaving_the_file_byte_identical`; ★ `a_symlinked_foreign_owned_or_changed_rc_is_never_written`; ★ `a_block_the_person_removed_is_not_written_back`; ★ `default_on_only_on_an_installed_build_with_an_organization_group_and_off_wins`; ★ `the_last_organization_group_leaving_removes_the_default_block`; ★ uninstall `apply_removes_owned_things_and_keeps_the_persons_files` extended with an rc block; ★ `no_test_or_dev_build_touches_the_real_home` (home injected, as `uninstall::Places`) | R3 | A-2 | no |
| A-4 | fabric-switchboard | `crates/switchboard-runtime/src/shell/homes.rs` (new), the `bindings_pass` hunk of `lib.rs`, `repoint_managed` hunk of `launch.rs:591-620` | Shell homes for organization pools, `map.json`, repoint on port change and capability rotation, history kept when a pool leaves | ★ `homes::tests::an_organization_pools_shell_home_has_the_placeholder_carried_links_and_no_credential`; ★ `a_codex_shell_home_routes_to_auto_with_no_sign_in`; ★ `a_port_change_repoints_shell_homes`; ★ `a_personal_group_gets_no_shell_home_and_no_map_row`; ★ `an_idle_pass_writes_nothing` (extends `an_idle_hour_on_a_fake_clock_stays_inside_the_budget`) | R2 | A-0, A-1 | yes |
| A-5 | fabric-switchboard | `crates/switchboard-runtime/src/person.rs` (new); call sites `carry.rs:34-39`, `limits.rs:641-650`, `codex_route.rs:51-56`, `agents.rs:57`, `launch.rs:232-236`, `launch.rs:575-583`, `crates/switchboard-cli/src/main.rs:1776`; refusal text `external.rs:1887` | One helper for the person's own folders; a value inside the data folder is never theirs | ★ `person::tests::a_config_dir_inside_switchboards_data_is_never_the_persons_own`; ★ `carry::tests::a_launch_from_a_hooked_terminal_links_from_the_persons_folder`; ★ `codex_route::tests::the_route_is_never_written_into_a_shell_home` | R2 | none | yes |
| A-6 | fabric-switchboard | `crates/switchboard-proxy/src/lib.rs` (tests only) | Guards the proxy facts the design relies on | `a_placeholder_bearer_with_a_folder_header_reaches_its_pool` (passes today by `lib.rs:574-605`; a guard, not ★); `an_auto_request_from_an_unmapped_folder_carries_no_sign_in_upstream` (guard) | R1 | A-0 | yes |
| A-7 | fabric-switchboard | `src/main.ts` (Groups section), `src/adapter.ts`, `src/ui-logic.ts`, `src/locales/ru.ts`, `src/demo.ts`, `src-tauri/src` command and notification, `crates/switchboard-cli/src/mcp.rs` (`switchboard_shell_hook`), `docs/ux/scenarios.md` (SCN-074, SCN-075, amend SCN-051/056/066), `docs/ux/screens.md` | The window's switch, the first-write notification, MCP parity; scenarios first | ★ ui-logic `the_shell_hook_section_reads_every_state`; ★ MCP `handshake_lists_tools_and_rejects_malformed_messages` with `switchboard_shell_hook`; WebKit demo on `?demo=1`, both themes, 740 px | R2 | A-3; scenarios before code | yes |
| A-8 | fabric-switchboard | `crates/switchboard-runtime/src/shell/windows.rs` (new), `shell/hook.ps1` (template, new) | PowerShell hook, profile paths through the Documents known folder, execution policy read | ★ `windows::tests::profile_paths_follow_the_documents_folder_for_each_edition`; ★ `a_restricted_or_allsigned_policy_reads_as_policy_blocked`; ★ `the_ps1_transition_matches_the_posix_one` (runs `pwsh` when installed, else skipped with the reason); live check needs the Windows host (SB-02, `docs/evidence/backlog.md:9`) | R2 | A-2, A-3 | yes (live check after) |
| A-9 | fabric-switchboard | `docs/CONTRACTS.md`, `AGENTS.md` lifecycle rows, `docs/research/session-home-per-context-2026-10-24.md`, `docs/HANDOFF.md`, `docs/evidence/backlog.md` (under agent-sync lease) | Records the contracts of section 5 with file:line evidence | `./scripts/check.sh` docs checks; ★ none beyond the gate | R1 | A-1 to A-8, B-1, B-2 | yes |
| B-1 | fabric-switchboard | `crates/switchboard-core/src/organizations.rs` (`context_for`, comment move), `kimi.rs:754-800` and `:894-950`, `launch.rs:1726-1830` and `:1895-1978` | Context for Kimi, Hermes and OpenCode, env and record | ★ core `organizations::tests::a_session_without_an_account_takes_its_keys_group_else_personal`; ★ kimi `a_launch_needs_a_signed_in_home_and_writes_its_script_outside_it` (`kimi.rs:1618`) asserting `context` and `SWITCHBOARD_CONTEXT`; ★ `an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key` (`launch.rs:3878`) and `a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key` (`launch.rs:3970`) asserting both; `a_sessions_context_follows_its_group_else_its_account_and_the_more_private_wins` unchanged and green | R1 | none | yes |
| B-2 | fabric-switchboard | `launch.rs:1560-1604` and `:2058-2152`, `src/types.ts:20` (`provider` widened) | Context and a session record for direct OpenRouter and proxy agent launches, so they appear in Sessions with their badge | ★ `an_openrouter_launch_records_its_session_and_context`; ★ `a_proxy_agent_launch_records_its_context`; ★ `the_session_list_names_an_organizations_context` (`lib.rs:4849`) extended with an OpenRouter row; WebKit demo with an agent row | R2 | B-1 | yes |

Count by risk: R1 4 (A-0, A-6, A-9, B-1) · R2 6 (A-1, A-4, A-5, A-7, A-8, B-2) · R3 2 (A-2, A-3).

Shared files: `crates/switchboard-runtime/src/lib.rs` (A-3 owner start, A-4 bindings pass: land A-4
first), `launch.rs` (A-4 repoint, A-5 two helpers, B-1 continuations, B-2 launches: disjoint
hunks, land B-1 before B-2), `src/types.ts` (B-2 only), `crates/switchboard-cli/src/main.rs`
(A-1 command, A-5 line 1776).

## 7. Deletes

| Node | Removes |
|---|---|
| B-1 | The SB-110 comment lines sitting above the wrong function (`organizations.rs:187-189` move to `context_admits`); the duplicated rule inside `session_context` (`organizations.rs:195-210`), now a call to `context_for` |
| A-5 | Seven private copies of "env value, else the default folder" (`carry.rs:34-39`, `limits.rs:641-650`, `codex_route.rs:51-56`, `agents.rs:57`, `launch.rs:232-236`, `launch.rs:575-583`, `main.rs:1776`), replaced by `person::*` |
| A-9 | The stale line "Not covered yet: agents launched through `switchboard agents launch` and Kimi Code sessions" (`docs/CONTRACTS.md:595-596`): Kimi has a record since `kimi.rs:780-793`, agent launches get one in B-2. The word "opt-in" in H-3 (`docs/research/session-home-per-context-2026-10-24.md:54`) and in SB-183's title (`docs/evidence/backlog.md:193`), superseded by D-3. H-3's "the group's managed home", superseded by the shell homes (Q1). |
| none | No data is deleted on any machine: shell homes are kept as history when a pool leaves (as launched homes are, `uninstall.rs:57-71`); removed rc blocks leave the file byte-identical |

## 8. Questions

| id | question | options → consequence | recommendation | whose (operator / peer: name it) | blocks |
|---|---|---|---|---|---|
| Q1 | H-3 says plain sessions use "that group's managed home"; this design gives them their own shell home per pool. Accept the split? | (a) shell homes → no credential race with launches, safe Codex config; `claude --resume` in a terminal does not list conversations of launched sessions of the same group. (b) share the managed home → one history; needs A-0 to show env wins over `settings.json`, and a new Codex credential path for managed launches | (a) for 0.7.0; revisit after A-0 if the measured precedence makes (b) safe | operator (it amends a recorded design) | A-4 |
| Q2 | An organization's group with no Codex account: should plain `codex` in its folders stay on the person's own sign-in (today) or be closed? | (a) leave it, `shell status` and SCN-074 say so → organization work may run on a personal Codex account, as today. (b) close: point `CODEX_HOME` at a shell home that refuses → `codex` fails there with an upstream 401 until a Codex seat is added | (a) in 0.7.0 with the plain statement; (b) once the proxy can answer a clear "this group has no Codex account" error | operator | A-4 |
| Q3 | A shell rc file that is a symlink (dotfile managers such as stow or chezmoi) | (a) skip, state `skipped_symlink`, show `switchboard shell init <shell>` for the person's own dotfiles. (b) edit the link's target → writes into the person's dotfiles repository | (a) | operator | A-3 |
| Q4 | PowerShell's execution policy on the Appvillis pilot's Windows machines | (a) never change it; `policy_blocked` shown with what the person or their IT can do → the hook may be inactive on those machines. (b) set `RemoteSigned` for the user → Switchboard changes a security setting | (a); ask Appvillis IT which policy the pilot machines run | operator (Appvillis IT) | A-8 acceptance |
| Q5 | The SB-140 notice ("claude.ai connectors are disabled because ... another auth source is set") if A-0 shows it in a shell home | (a) accept: an organization's home has no claude.ai login, so no connectors are lost, and SCN-074 says so. (b) find another sign-in form (`apiKeyHelper` is also an auth source) | (a) | operator (SB-140 was the operator's complaint) | A-4, A-7 |
| Q6 | A Kimi account has no organization and its launch has no folder check. In an organization's folder, record `personal` (truthful) and launch, or refuse as SB-109 refuses foreign accounts? | (a) record `personal`, launch, file a board row for Kimi attribution and refusal → company work may run on a personal Kimi subscription, now visible. (b) refuse in organization folders now → a new refusal path in 0.7.0 | (a) | operator | B-1 |
| Q7 | Windows has no carried links (`docs/CONTRACTS.md:68`): an organization's shell home there lacks the person's `CLAUDE.md`, skills and hooks | (a) accept for 0.7.0, say so in SCN-074 → organization sessions on Windows start without them. (b) directory junctions and hard links (no admin needed) in A-8 → more Windows code before SB-02 is ever run | (a), with a board row for (b) after SB-02 | operator | A-8 |

## 9. Out of scope / program

| Item | Why later |
|---|---|
| SB-184: the context profile (`GIT_CONFIG_GLOBAL`, `SSH_AUTH_SOCK`, `GH_CONFIG_DIR`, MCP servers, plugins) applied at launch and in the hook | Owned by Project Observatory as OBS-236, engine 0.31.0, 2026-11-14; lands as a 0.7.x (`docs/evidence/backlog.md:194`, D-4). The hook's design leaves room: the profile's variables join the same save and restore |
| Sessions started without an interactive shell: `claude -p` from scripts and cron, the VS Code extension's bundled binary, Fabric Dashboards' console | No rc file runs there; SB-118 and SB-143 still route their accounts. A `PATH` shim was rejected (section 4) |
| `cmd.exe` and Git Bash on Windows | `cmd.exe` has no per-directory hook short of the registry's AutoRun; Git Bash needs Windows path handling in the bash script and its own acceptance after SB-02 |
| Re-reading the map in an open terminal without a directory change | One more process per prompt; a new group applies at the next `cd` or in a new terminal (stated in SCN-074) |
| A session-start line "this session runs in {org}'s context" | SCN-055 allows one line only when a folder joins a group (`docs/ux/scenarios.md:1163`); a new line is a scenario change of its own |
| Kimi organization attribution and refusal (Q6 option b) | Needs Kimi accounts in organizations' membership model first |
| Windows carried links (Q7 option b) | After SB-02's first Windows run |

Scenario ids proposed for `super-ux` to write **before** any window or CLI text (proposals; the run
assigns final ids under one lease, other dossiers may propose the same numbers):
- **SCN-074** "Plain `claude` and `codex` in an organization's folders run in its context": the
  first-write notification, new terminals pick it up, entering and leaving restore the person's
  own, what is not covered (open terminals until `cd`, non-interactive starts, Codex without a
  seat, Windows without carried files), owner down, CLI `shell status` lines.
- **SCN-075** "Turn the terminal hook off or on": Groups section states (on, off, default on,
  default with no organization, `skipped_symlink`, `removed_by_person`, `policy_blocked`), the
  window's switch, `switchboard shell on|off`, MCP `switchboard_shell_hook`, uninstall.
- Amend **SCN-051** (Kimi, Hermes, OpenCode and agent rows carry the badge), **SCN-056** (what
  the hook adds in organization folders), **SCN-066** (organization folders route Codex through
  the shell home whatever the switch).
