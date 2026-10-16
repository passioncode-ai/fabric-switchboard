# SB-134 / SB-135 — MCP covers every feature; a project is a set of root folders

Filed 2026-10-16 on the operator's direct instruction («закинь свитчеру доработку»), from a session
outside a Fabric project, so the Fabric board could not carry it. Rows: [backlog](../evidence/backlog.md).

## SB-134 — `switchboard mcp` covers every feature the CLI has

Operator request, 2026-10-16: «сделать работающий нормально mcp по всем фичам».
Measured on switchboard 0.6.33 (installed CLI and app binary), tool list from
`crates/switchboard-cli/src/mcp.rs` on origin/main `6dc4d16`+1.

### Trigger

Moving an account into the `agents` pool had no MCP tool. The agent had to fall back
to `switchboard project save --pool agents` and re-send all 54 folders plus every
reserved account by hand, because `save` treats the listed set as the whole set
(an omitted account goes back to `default`). One typo in that list would silently
unreserve accounts or drop folders.

### Gap: CLI commands with no MCP tool

| Area | CLI has | MCP today |
|---|---|---|
| Projects / pools | `project list`, `save`, `delete`, `suggest`, `suggestions`, `accept`, `dismiss`, `bindings`, `source` | only `project_context` (one folder) and rule tools `project_set/remove/apply` |
| Accounts | `accounts update` (label, enable/disable), `remove`, `capture`, `activate`, `select --pool` | `accounts`, `switch` (covers select/activate) |
| Sign-in | `login begin/status/finish/cancel` | none (`onboarding` only names the step) |
| Rotation | `rotation status`, `rotation set` | read-only inside `status` |
| Activity | `events` | none |
| Launch / continue | `launch`, `continue` | none (`switchboard_continue` is SB-64/SB-74) |
| Recovery / hand-overs | `recovery …`, `handovers list/run/set` | none |
| Backups | `backup list/now/restore` | none |
| Kimi | `kimi login/finish/cancel/relogin/launch/rename/remove` | `kimi_accounts` (read) |
| Agents | `agents enable/disable/default/models/connect/key/openrouter/hermes/launch` | `agents` (read), `openrouter_*`, `hermes_*` |

### Ask

1. Pool membership as a delta, not a whole-set rewrite: e.g. `switchboard_project_accounts`
   `{pool, add:[account_id], remove:[account_id]}` and the same for folders; plus
   `switchboard_projects` (list) and project create/delete. Mirror the delta form in the CLI.
2. MCP tools for every remaining CLI feature in the table, read tools always,
   write tools hidden under `--read-only`, each write marked "only when the operator asks"
   as the existing write tools are. Secrets stay out: no tool takes a key/token value
   (`accounts add --secret-stdin` stays CLI-only; a sign-in tool starts the official
   login and returns its id, the person signs in).
3. A parity check in CI: every CLI subcommand is either mapped to an MCP tool or listed
   in an explicit exclusion with a reason, so the gap cannot reopen silently.
4. Docs in the same change: CLI.md / README MCP table (relates to SB-63 drift).

### Also observed on this machine

Two registrations serve the same tools: `switchboard` (user `~/.claude.json`,
app binary `mcp`) and the plugin `switchboard:switchboard`. Every tool appears twice
in a session. Onboarding/doctor should detect and name the duplicate.

### Acceptance

From a live Claude Code session, using MCP only: move an account between pools,
add/remove a project folder, enable/disable an account, set rotation, read events,
list/trigger a backup, start and finish a sign-in up to the human step —
each with a test name and a real-session receipt (extends SB-01 / SB-43).

---

## SB-135 — a project is a set of ROOT folders; everything derived joins by itself and shows as a tree

Operator request, 2026-10-16 (verbatim intent): папки на диске должны быть указаны в Switchboard —
какая к какому проекту относится, менять это в интерфейсе; все производные ворктри, ветки и модули
в отдельных папках, которые относятся к проекту, должны сами туда попадать; в UI видны рут-папки,
развернув рут, видно все вложенные ворктри/ветки, связанные с ним; проект — это несколько рут-папок.

Builds on SB-117 (suggestions), SB-118 (bindings), SB-131 (auto-join, done on a branch), SB-133
(moved folder). What is NOT covered by them:

1. **Root vs derived.** The project stores roots (one per repository/product checkout). A derived
   folder — `git worktree` of a root (`git worktree list`), a clone with the same origin, a module
   checkout under a root's sources — is attached to its root, not stored as a peer. Today the Agents
   project is a flat list of 61 folders mixing roots with `_worktrees/*`, `*-wt-*` and dated copies;
   five dead worktrees fell out of it during this session alone.
2. **Derived joins without a person** (operator: «должны сами туда попадать»), inherits the root's
   project, and leaves when its directory is gone (SB-131 semantics, extended to the root relation).
3. **Tree in the window.** Project → roots → expandable list of derived folders with their kind
   (worktree/clone/module), branch, and last activity. Add/remove/move a root with the system folder
   picker (global interface rule); moving a root moves its derived folders.
4. **Disk map.** A view of repositories found on disk (e.g. under the folders the person picks,
   default `~/DATA`) with their project or "unassigned", re-assignable in place. Today 233 entries under
   `~/DATA`; the operator assigned projects by hand in chat this session.
5. **MCP and CLI parity** for all of the above (see SB-134): list the tree, move a root between
   projects, list unassigned roots — delta operations, never whole-set rewrites.

Live example (2026-10-16): project `Apps & Soft` (pool `apps-soft`) was created by hand with 16
flat folders — roots `sms-activate-app-website`, `vitrual-phone-number-bot`, `longevity-app` (aion),
`screen-record-studio`, `screen-record-studio-Recordly`, `claude-code-data`, `privateclawd-docs`,
`aion-captures`, plus 8 worktrees the agent had to discover with `git worktree list` (e.g.
`.worktrees/be-*` of the bot, `aion-worktrees/AION-B`). In the target design only the roots are
entered; the 8 follow.

Acceptance: create a project with roots only; `git worktree add` under a root appears under that
root in the window and in MCP within one pass, with no question; deleting it removes it; moving a root
to another project moves its tree; an unassigned repository is visible and assignable.
