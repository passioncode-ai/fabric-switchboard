# SB-117 — a project offers its new folders itself

Operator request, 2026-10-14: «не должно быть лимита на проект и мы должны грамотно учитывать
текущие и будущие папки чтобы они сами добавлялись через апрув пользователя … уведомление
приходило и висело на экране с просьбой добавить … агенты когда делают новые ворктри …
и могут ещё новые проекты в эту экосистему добавлять … в харнесе все агенты знали что надо
трекать это и в свитчборде сразу показываем предложение». Base: `a5d00cc` (0.6.29).

## Decisions (taken by the agent under the operator's standing rule, recorded here)

| # | Decision | Why |
|---|---|---|
| D-1 | No limit per project. One storage bound stays: 1024 folders across all projects (`MAX_ALL_FOLDERS`, the old worst case 64 × 16). | The metadata file is capped at 2 MiB; 1024 × `MAX_PATH` (1 KiB) keeps the worst case at 1 MiB, as before. |
| D-2 | A folder joins only after a person says yes. Switchboard suggests and never adds on its own. A declined folder is never suggested again. | The operator asked for approval; a wrong folder would reserve accounts in someone else's repository. |
| D-3 | A folder is the project's if it is (a) a git worktree of a project repository, (b) a clone with the same `origin` as a project folder, or (c) a repository whose `origin` owner is one of the project's **sources** (`github.com/passioncode-ai`). Sources are set explicitly (`switchboard project source`), never guessed. | (a) and (b) are facts. (c) admits new repositories of an ecosystem, but an owner can hold unrelated repositories, so a person names it. |
| D-4 | Three discovery paths: the owner's background pass, Claude Code's `SessionStart` hook in the plugin (`switchboard project suggest --from-hook`), and `switchboard project suggest --folder` for any agent or person. | Agents need not remember the rule, and new folders appear even when no agent starts there. |
| D-5 | The background pass is stamp-driven (LC-04). It re-lists a project folder's parent directory only when that directory's modification time changed, and a repository's `.git/worktrees` only when its time changed. A folder classified "not ours" is remembered for the process lifetime. | Idle budget: an unchanged hour reads no directory twice. |
| D-6 | Signal: a system notification when a suggestion appears; the tray icon carries the count (macOS title next to the icon, Windows/Linux the tooltip) until it is answered; the window shows a banner above every page with **Add** and **Not this one**. | "Arrives and stays on the screen": a notification can vanish, the count and the banner stay. |
| D-7 | An ambiguous folder (two projects match equally) is not suggested. | Never guess between projects. |

## Requirements

| Requirement | Verified by |
|---|---|
| REQ-1 No per-project folder limit; at most 1024 folders across projects | core `a_project_has_no_folder_limit_and_all_projects_hold_1024` |
| REQ-2 Worktree, same-origin clone and ecosystem source are recognised; anything else, an ambiguous match or a folder already in a project is not | runtime `suggest::tests::worktrees_clones_and_ecosystem_repositories_are_recognised`, `…_other_folders_are_not` |
| REQ-3 Suggestions persist (0600, bounded 256), no duplicates; a dismissed one stays dismissed; accept adds the folder to the project and keeps its accounts | `suggest::tests::a_suggestion_is_recorded_once_accepted_or_dismissed_for_good` |
| REQ-4 The background pass finds a new sibling clone and a new worktree, and re-reads nothing while stamps are unchanged | `suggest::tests::the_scan_finds_new_siblings_and_worktrees_and_reads_nothing_unchanged` |
| REQ-5 CLI `project suggest --folder/--from-hook`, `project suggestions`, `project accept`, `project dismiss`, `project source`; the hook never fails a session | CLI `project_suggestions_work_from_the_cli_and_the_hook_never_fails` |
| REQ-6 The plugin's `SessionStart` hook reports the session folder; the skill tells agents to report worktrees and clones they create | plugin validate; review |
| REQ-7 Tray count, notification and window banner with Add / Not this one; nothing is added without a click | SCN-055; UI logic test; browser demo |

## Not in scope

Folders an agent works in that are not git repositories; removing folders that were deleted
from disk (a project keeps a missing folder, as today); Codex-side hooks (Codex has no
`SessionStart` hook; its agents use the CLI command from the skill).
