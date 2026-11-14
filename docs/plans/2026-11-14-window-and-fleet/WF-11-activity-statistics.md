# WF-11 Activity and Statistics (SB-373, wave 2)

> Activity вообще сложно смотреть, какая-то таблица. ... статистика уже получше ... но все равно ...
> длинные таблицы, очень долго листать, такого не должно быть. Такие таблицы должны быть либо там
> сворачиваемые, разворачиваемые, либо как-то по-другому навигация должна осуществляться

## Now

- Activity (`renderActivity`, `src/main.ts:2645`): `sessionsPanel` (2623), then one flat
  `ol.event-list` of every event, newest first, no filter, no paging.
- Statistics (`renderStats` 2572): period control, tiles, chart, then full tables (by organization,
  group, repository, account, agent; "who works where"; "by day"), sortable, each scrolling inside
  `stat-scroll`, none folding.

## Sub-modules and tasks

**11.1 Activity.** Events grouped by day (folding, today open), a filter row (kind: switches,
sign-ins, groups, moves, errors; account; search), 50 per day shown with *Show more*. Sessions stay on
top as a folding section with running ones first.

**11.2 Statistics.** Each table a folding section with its top 10 rows and *Show all N*; sticky
header; the period control and tiles stay pinned at the top; an index of sections at the top that
jumps to each.

## MCP

`switchboard_events` gains `kind`, `account_id` and `before` (paging) filters; the window uses the
same.

## Checks

Demo with 5,000 events and 300 repositories (the long case) renders under 100 ms per page change;
SCN-011, SCN-051, SCN-067 updated.

## Depends on

WF-1.2.
