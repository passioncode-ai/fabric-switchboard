# SB-118 — ordinary Claude Code in a project's folders uses the project's accounts

Operator, 2026-10-14, looking at the PassionCode project's two accounts: «выглядит так как
будто для проекта не стали эти подписки и тратятся основные».

## What was measured

- Every session on the Mac, this one included, ran the ordinary `claude`. That was signed in
  as the default-pool account, and `switchboard sessions` listed none launched by Switchboard.
  A project's reservation (SB-102) reaches only sessions Switchboard launches. Those have minimal
  homes (no skills, plugins, MCP or `CLAUDE.md`; SB-20, SB-110), so nobody used them.
- Claude Code reads `env` from the repository's `.claude/settings.local.json`. A probe server got
  `Bearer <value>` at `<ANTHROPIC_BASE_URL>/v1/messages` from `claude -p` started in a
  *subfolder*.
- `claude -p` with the real proxy route `/claude/passioncode` answered, and the pool rotated
  (vibecoder 91 % → contact@). The user's own configuration stays in force: only the route and
  the capability change.

## Decisions

| # | Decision | Why |
|---|---|---|
| D-1 | Every folder of a project whose pool has an enabled Claude account gets `env.ANTHROPIC_BASE_URL = http://<proxy>/claude/<pool>` and `env.ANTHROPIC_AUTH_TOKEN = pool_token(session, pool)` in `.claude/settings.local.json`. So do worktrees of its repositories that sit inside a project folder, since a worktree is its own repository root. | The only way to steer the ordinary `claude` per folder without losing the person's setup. |
| D-2 | The capability opens that pool's route only, and only as a bearer (`switchboard_proxy::pool_token`). It is derived from the session token, so it survives a restart. | The value lives in a working tree; if it leaks, it reaches one pool on loopback, not every account. |
| D-3 | Everything else in the file is the person's. Switchboard records folder, pool, URL and the **SHA-256** of the capability (`claude-bindings.json`, 0600). It removes only values still equal to what it wrote, and deletes the file only if it created it and nothing else is left. A file that is not a JSON object is never touched. | Never destroy a person's settings; never keep a second copy of the capability. |
| D-4 | The file is kept out of Git through the repository's own `info/exclude`, never a tracked `.gitignore`. A folder that is a subfolder of the repository gets its line relative to the working-tree root (`<rel>/.claude/settings.local.json`), since a pattern with a slash is anchored there. | No change to anyone's repository content. `info/exclude` cannot untrack a file that was already tracked before binding. |
| D-5 | The owner re-syncs only when a fingerprint moves: projects, folders, the pools' Claude accounts, the proxy address, and each project repository's worktree-list stamp. It does so under the owner transaction. `switchboard uninstall` removes every binding first. | Idle budget (LC-04) and a clean uninstall. |
| D-6 | A running session keeps the account it started with; a new session, a resumed one or a subagent picks the route up. | Claude Code reads settings at start. |

## Requirements

| Requirement | Verified by |
|---|---|
| REQ-1 The pool capability reaches its pool only, as a bearer only | proxy `a_pool_capability_reaches_its_pool_only` |
| REQ-2 Project folders get the route; the person's keys stay; 0600; excluded from Git (relative to the working-tree root for a nested folder); no rewrite when nothing changed; a new port rewrites; leaving removes only ours, per key | `bindings::tests::a_project_folder_gets_its_pool_route_and_keeps_the_persons_settings`, `bindings::tests::a_folder_nested_in_a_repository_gets_a_relative_exclude_line`, `bindings::tests::removal_is_per_key_a_value_the_person_edited_stays` |
| REQ-3 Values the person changed and broken files are never touched; a pre-existing `ANTHROPIC_*` value that is not Switchboard's skips the folder (`user_values_present`) and is reported; a failed re-apply keeps the record row; a pool without a Claude account binds nothing | `bindings::tests::values_the_person_changed_or_a_broken_file_are_never_touched`, `bindings::tests::values_that_are_not_switchboards_are_never_overwritten_but_own_values_are_replaced`, `bindings::tests::a_failed_re_apply_keeps_the_record_row_and_later_syncs_own_it_again` |
| REQ-4 Worktrees inside a project folder are bound; the exclude goes to the common git folder | `bindings::tests::a_worktree_inside_a_project_folder_is_bound_too` |
| REQ-5 `switchboard project bindings` lists them; uninstall removes them, and an unbind that fails surfaces in the plan's `failures` | `commands::project_bindings_are_listed_offline`; `uninstall::tests::bindings_are_unbound_before_the_data_folder_goes`, `uninstall::tests::a_binding_that_cannot_be_removed_is_a_plan_failure_not_a_stop` |
