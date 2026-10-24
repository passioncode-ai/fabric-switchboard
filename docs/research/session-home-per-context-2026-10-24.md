# A session home per context: what Switchboard builds, and what waits for a profile

Design, 2026-10-24, for SB-110 (RM-28 CX-SB-02). Status: decided here, built in the phases of
§5. Due 2026-10-31 (board thread `0d82422a`, reply `702e823e`).

## 1. The ask

CX-SB-02 as amended (fabric-workspace `docs/reports/2026-10-12-org-contexts/README.md:495`):
Switchboard owns `CLAUDE_CONFIG_DIR` and `CODEX_HOME` per context (`personal`, `org:<id>`) and
applies the launch environment; git identity (`GIT_CONFIG_GLOBAL`), `SSH_AUTH_SOCK`,
`GH_CONFIG_DIR`, MCP servers and plugins come from **the context's profile on the Observatory
node**, which Switchboard sets but never creates or owns. Around it: the context is fixed for the
session and derived from the folder and the account, the more private winning (CX-26,
`README.md:264`); changing context is a change of home, with no second sign-in (MC-57,
`scenarios.md:497-502`). The operator's rollout of 2026-10-23 counts on it: plain `claude` and
`codex` are not bound to a context until SB-110, so Appvillis work goes through Switchboard
(passioncode-vesta `docs/design/2026-10-23-rollout-estate-and-appvillis.md:60-63`, wave 2 `:104`).

## 2. What is true today

- **Homes are already one context each.** A launched session lives in `runtimes/<provider>-<pool>`
  (managed) or `homes/<account>` (isolated) (`crates/switchboard-runtime/src/launch.rs:2387-2398`).
  A pool belongs to one group and a group to one organization or none
  (`switchboard-core/src/projects.rs:220`); an account a group reserves is in that group's pool.
  Since SB-109, `Snapshot::context_admits` (`organizations.rs:190`) refuses another context's
  account at launch (`launch.rs:1528`), on the proxy and in rotation. So a launched home never
  mixes two contexts; what it lacks is the context written down.
- **No context is recorded on a launch.** `SWITCHBOARD_SESSION` is `managed:<provider>:<pool>`
  or `isolated:<provider>:<id>`, and `.session.json` (SB-95) names no context.
- **What a launched home carries is the operator's personal scope.** `carry.rs` links
  `CLAUDE.md`, skills, commands, agents, output styles, plugins and hooks from `~/.claude` into
  every Claude home and merges `settings.json` without credentials (`carry.rs:10-30`); SB-20's
  decision carries only the Fabric board among MCP servers (`launch.rs:231`). Git, SSH and `gh`
  are inherited from the person's shell: nothing sets `GIT_*`, `SSH_AUTH_SOCK` or `GH_CONFIG_DIR`.
- **Plain `claude` in a group folder** is routed per folder (`bindings.rs:515-560`: the proxy
  address and the folder's capability in `.claude/settings.local.json`), so its requests spend
  only that group's accounts, but its home, memory, MCP servers and plugins stay the person's
  ordinary ones. A settings file cannot move the home: `CLAUDE_CONFIG_DIR` is read before any
  settings file. Plain `codex` follows the SB-143 provider block (off by default).
- **The profile CX-SB-02 depends on does not exist.** Project Observatory has no per-context git,
  SSH, `gh`, MCP or plugin profile, no row for one and no tool or file Switchboard could read it
  from (`project-observatory-dashboard`: `SSH_AUTH_SOCK` only in the SSH-key inventory, `GH_CONFIG_DIR`
  nowhere; OE-01a, OE-01b and OE-06, due 2026-11-14, are transcript collection, event emission and
  managed-settings compilation, not this). The contract leaves "a context on a project, a session
  or an event" to the host (`fabric-agent-contract docs/specification/contexts.md:285`) and has no
  profile schema.

## 3. Decisions

| # | Decision | Why |
|---|---|---|
| H-1 | **The context is recorded at launch** in the contract's form (`personal`, `org:<id>`, `contexts.md:22-43`): in `.session.json`, in the session list (CLI, MCP, the window's Activity) and as `SWITCHBOARD_CONTEXT` in the session's environment. It is computed as today's admission computes it: the folder's group, else the account's organization; when they differ the more private wins (`projects.rs:297-306`). | Fabric, Observatory and the node read a session's context; today they would have to re-derive it. Recording it changes no behaviour. |
| H-2 | **Homes stay keyed by pool and account.** No `contexts/<id>/` tree. | Each home is already one context (§2); a second key would create two homes for one pool and break managed sessions that survive a restart on a stable home. |
| H-3 | **Plain sessions get a context home through an opt-in shell hook,** `switchboard shell init zsh|bash|fish`: on entering a group folder whose group belongs to an organization it exports `CLAUDE_CONFIG_DIR` and `CODEX_HOME` of that group's managed home and `SWITCHBOARD_CONTEXT`; on leaving it restores the person's own values. A personal group, or no group, changes nothing. | The only way to move a plain session's home is its environment at start (§2). A hook the person installs keeps the home move visible and reversible, as direnv does, and needs no root, MDM or second OS account (CX-25, isolation level 1). Risk to settle in phase 2, before the scenario is final: a managed home holds no sign-in (the proxy supplies the account), and plain `claude` may ask for one there even with the folder's route; if it does, the hook points at a home that carries a sign-in-free marker the CLI accepts, or the phase stops and says so. |
| H-4 | **The profile is applied when it exists, never invented.** The launch and the hook set `GIT_CONFIG_GLOBAL`, `SSH_AUTH_SOCK`, `GH_CONFIG_DIR`, and pass the profile's MCP servers and plugins, only from a profile the node publishes for that context. Until one exists they set none of these, and the session uses the person's own git, SSH and `gh`, as today. Switchboard never writes a git config, starts an SSH agent or signs in to `gh`. | CX-SB-02's amendment gives the profile to the node. A Switchboard-made profile would be a second source of truth for an identity Switchboard cannot verify. |
| H-5 | **What a context home carries.** A personal home carries the operator's user scope as today (SB-137). An organization's home carries the same until that organization's profile lists what it carries; then the profile's list replaces it. | Removing the operator's instructions, skills and hooks from organization sessions now would break working sessions (SB-137 was this exact complaint) with nothing to replace them. |
| H-6 | **First sign-in per home is already once.** An isolated home is per account and holds its sign-in; a managed home signs in nowhere (the proxy holds the account). MC-57's "no second sign-in" holds for Switchboard's part; `gh` and git sign-in are the profile owner's (MC-57 assigns them to Fabric). | No change needed. |

## 4. What this needs from others

- **A context profile and its owner.** CX-SB-02 names the Observatory node, but no row, schema or
  date exists (§2). Asked on the board (thread `12bd8b4c`): who owns the per-context profile
  (git identity file, SSH agent socket, `gh` config dir, MCP servers, plugins), its format and the
  surface Switchboard reads (an MCP tool of the node or a file under `node/orgs/<org.id>/`), and
  a date. H-4 waits on that answer; H-1 and H-3 do not.
- **The rollout's wording.** Vesta's rollout expects SB-110 to bind plain sessions by 2026-11-14.
  With H-3 it binds them for a person who installs the hook; without the profile it binds the
  home and the accounts, not git, SSH or `gh`.

## 5. Phases and rows

| Phase | What | Row | Date |
|---|---|---|---|
| 1 | H-1: context recorded on every launch, in the session list and `SWITCHBOARD_CONTEXT` | SB-110 | 2026-11-07 |
| 2 | H-3: `switchboard shell init` for plain `claude` and `codex` in an organization's group folders, with the scenario first | SB-183 | 2026-11-14 |
| 3 | H-4 and H-5: apply the node's context profile at launch and in the hook | SB-184 | 7 days after the profile's owner publishes it |

H-2 and H-6 need no work.
