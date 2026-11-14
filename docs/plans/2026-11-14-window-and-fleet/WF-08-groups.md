# WF-8 Groups page (SB-370, wave 2)

> Дальше группы внутри, интерфейсы в группах в разделе тоже сделать как-то читаемым, потому что
> сейчас это очень сложно, типа придумать, как этот весь UI должен быть переорганизован более удобно.

## Now

`renderProjects` (`src/main.ts:466`): `projectsSection` (790: a card per group with hint, account
count, organization badges, folders collapsed at 6, account chips, chain link, seat warnings, Edit,
Delete), then `machinesSection` (477), `balancingSection` (561), `codexRouteSection` (594),
`shellHookSection` (628), `diskMapSection` (681), `rulesSection` (933).

## Sub-modules and tasks

**8.1 Group card.** A card shows one line of state ("3 accounts · 2 with room · 12 repositories ·
chain: Claude Code → Codex"), its organization, and folds open to three blocks: *Accounts* (with the
account each session uses now), *Repositories* (the worktree tree), *Agents and keys* (chain, own
key, ceiling). Edit and Delete in one ⋯ menu.

**8.2 Page sections.** The page keeps groups and *Other machines* (until WF-12 gives machines their
own view); balancing, Codex route and terminal hook move to Settings (WF-1.1); the disk map becomes
*Find repositories on disk…* (a dialog); folder rules stay as a folding section.

## MCP

Unchanged; `switchboard_projects` already answers each block.

## Checks

Demo with 12 groups and 300 folders (the long case); SCN-034, SCN-063, SCN-090 updated;
design-tester pass.

## Depends on

WF-1.1, WF-1.2.
