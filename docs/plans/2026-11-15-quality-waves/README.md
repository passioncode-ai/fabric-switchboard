# Quality waves, 2026-11-15

**Asked (operator, 2026-11-15).** Fix the window's layout (Statistics runs sideways and scrolls badly) and every page; make the lifecycle of agents' sessions visible (which chain, which ran, which are alive, how many where, how each was started, for which task) and watch it every minute so no session hangs or is abandoned; check the board and other PassionCode agents' deliveries; analyse interfaces, latency, performance and correctness; add a bug tracker on our own analytics; gather GitHub issues and the board into one plan and fix it in waves to the end. On Organizations: the setup checklist asks for a PassionCode.ai group although its accounts live on the Mac Studio, and making a group means picking repositories one by one instead of one folder.

**Inputs.** Screenshots of every page at 1280, 960 and 760 px in the demo (Playwright WebKit on the Mac Studio, `scripts` in the run notes below); the open backlog and GitHub issues (one open issue, #36, already done by SB-47; 78 rows with something left); the error and analytics pipeline as it stands (SB-248 `app_error` counts, 276 events from 8 installs in 14 days; gaps listed in W3); the operations log's slow calls (W4).

Each wave ends with the gate (`./scripts/check.sh` on the Mac Studio), Linux clippy, a reshoot of the pages it touched, a release, and the board rows updated.

## W1: what a person sees broken (now)

| Id | What | State |
|---|---|---|
| Q1 | Statistics: the chart's 1fr track grew to the SVG's 900 units, so the page ran 216 px past a 960 px window and 352 px past 760; the pinned header held the five tiles too, a third of a narrow window, and covered the tables on scroll | fixed on branch `layout` (`minmax(0, 1fr)`, tiles scroll, scroll margin 120 px); reshoot: no page wider than its window |
| Q2 | Every page: defects from the screenshot audit (overflow, wrapping, spacing, typography, copy, empty states) | in progress |
| Q3 | Organizations: an organization whose accounts all live on another machine reads so in the checklist (not "Needs you"); a group is made from one parent folder (every repository under it) or from the organization's repositories on this computer by GitHub owner, never one repository at a time | open |

## W2: sessions you can follow

| Id | What | State |
|---|---|---|
| Q4 | A Sessions view in the window: every live coding session (Switchboard's or not), the tree of launches (who started whom: `launched_by`, parent, dispatch, hand-over), how it started (window, CLI, MCP, Fabric, Dashboards, recovery), its task (workflow, board request), its group, account and machine, alive or ended; counts per group and machine; the session pool (SB-378) | open |
| Q5 | A one-minute watch: a session quiet past its limit while a task or dispatch holds it, a dispatch past its deadline, a launch whose process died with no end recorded, a resumed copy of a live session (SB-376); each listed in *Needs attention*, logged, and reported on the board once | open |

## W3: a bug tracker on our own analytics

`bug_seen` (fingerprint, kind, surface, class, code, location as line and column, uptime bucket; never text, paths or ids) beside the existing daily `app_error`; renderer `error`/`unhandledrejection`, IPC failures and timeouts, UI stalls; panics in every process, logged; a background pass that panics recorded and the loop kept alive; crashes found at the next start (`owner-run.json`, the macOS crash report's exception type only); local `bugs.jsonl` and `switchboard bugs` / MCP `switchboard_bugs`; the planted-identifier test extended. Design: research notes in this folder's run log.

## W4: latency and performance

From the operations log (slow control calls such as `recovery` 34 s and `launches` 22 s on 2026-11-14 under load) and the code paths behind them: the fixes ranked by the latency they remove.

## W5: the board and other agents

Backlog hygiene (rows whose status contradicts their body: SB-65, SB-174, SB-192, SB-352, SB-32; issue #36 closed against SB-47); WF-5.2e, 5.2f C8, 5.3; the live call back of claude6 (SB-377) once 0.7.86 runs on both machines; other agents' deliveries checked against what they claim (Fabric Dashboards FD-260 still resumed a live session on 0.8.88, board reply `36e058ab`; Fabric's queue CO-588 on the pool).
