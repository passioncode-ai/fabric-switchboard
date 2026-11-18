# 08. Who starts, watches and continues coding sessions (operator's order, 2026-11-18)

## The order

«проведи аудит того, как организована вот эта работа … удобно ли для нас это запускать этих
агентов, как мне запускать их, чтобы я мог запустить какой-нибудь папки … по подписке или другого,
чтобы это все работало и также из дашборда, из фабрика. Нужно сделать также мониторинг загрузки,
чтобы у нас обсерватория мониторила все эти … сессии. Синхронизировать внутри нашей системы
проектов, кто вообще следит за сессиями, кому что делегирует … покажи, как есть сейчас и как
должно быть, напиши спеку и план последовательный.»

## As it is (measured 2026-11-18)

### Who starts a session today

| # | Entry point | How it starts | Goes through Switchboard? |
|---|---|---|---|
| 1 | The person types `claude` in a terminal | the ordinary sign-in; in a group's folder its `.claude/settings.local.json` routes it through the proxy | only by the folder's route file |
| 2 | Switchboard window or `switchboard launch` | Terminal window, `--in-place`, `--headless`, `--hosted` | yes |
| 3 | Fabric Dashboards console, this Mac | `switchboard launch --in-place --hosted --surface dashboards` | yes |
| 4 | Fabric Dashboards console placed on another machine | `claude` over the ssh tunnel, with no Switchboard there | **no**: the folder in no group opened the sign-in screen (board request `51c3dcca`) |
| 5 | Fabric dispatcher (board requests) | `switchboard dispatch` (`claude -p`), board share of the pool | yes |
| 6 | Personal communicator (`communicator_session_start`) | `switchboard launch`, isolated | yes |
| 7 | Workspace pulse and other scripts | `switchboard launch` from bash | yes |
| 8 | Switchboard itself | duty officer restarts, recovery after a limit, automatic fallback hand-over | yes |
| 9 | Dashboards' temporary `ai.passioncode.resume-after-boot` | `sessions continue` after a reboot | yes, until SB-450 replaces it |
| 10 | Subagents inside a session (the Agent tool) | the parent's process and account | invisible as sessions; they spend the parent's account |

### Who watches

| Watcher | What it reads | What it does |
|---|---|---|
| Switchboard (owner) | process table, `sessions/<pid>.json`, launch records, proxy counts | admission (`capacity.json`: cap 40, board share 10), duty officer (5 min), recovery, rotation, failover, balancing and the feed |
| Project Observatory (`observatory_sessions`) | processes and Switchboard's answer | counts managed and unmanaged, finding `session.unmanaged` |
| Fabric Dashboards | Switchboard's `sessions --rows`, its consoles | Sessions page, consoles, its own resume-after-boot agent |
| Fabric | the board | dispatches requests; refuses below 3 GB free disk |
| studio-agent (Mac Studio) | its host's load | defers its own launches when the host is critical |

### What is wrong (evidence of 2026-11-18)

1. **A worktree is served by its main checkout's group** (SB-452). Claude Code reads
   `settings.local.json` from the repository's main checkout, so a launch on the pay-per-token group
   in `sound-engine-audit` ran on Apps & Soft subscriptions; the ceiling counted nothing.
2. **Launches lack the login `PATH`** (SB-455): `PATH=~/.cargo/bin:/usr/bin:/bin`, every hook that
   runs `node` fails.
3. **A remote console bypasses Switchboard** (row 4 above).
4. **A reboot ends every session** and nothing continues them (SB-450); a launch record can name
   another session's id (SB-451).
5. **Under load the owner answers late**: `control_slow` 557 times in a day, the CLI exited 1 while
   the act happened later (fixed by #831 with status 4; reading without the owner is SB-446).
6. **Observatory misclassifies**: of 9 Claude Code processes at 11:10 UTC it called 7 unmanaged,
   this session included, which runs through its folder's route; `account_group` is null on every
   row.
7. **No one sees load and money together**: CPU load 70 to 450, swap 21 GB, 1.6 GB free disk in the
   morning, 9 of 19 sessions on a limit stop at 12:00 UTC, every subscription of a group spent, and
   no single page that says so or stops new work.
8. **Four things continue sessions** (duty officer, recovery, fallback, Dashboards' agent) with no
   one owner; launching a known job takes a long command (SB-454).

## As it should be

### One owner per concern

| Concern | Owner | Others |
|---|---|---|
| Starting any coding session, here or on another machine | **Switchboard** (`launch`, `run <saved launch>`, `dispatch`, `launch --target`) | Dashboards, Fabric, communicator and scripts only call it |
| Which account and group serves it | Switchboard, from the launch's own route, never a file of another checkout | none |
| Admission: pool cap, board share, host load, free disk, money | Switchboard, one gate for every entry point | studio-agent's deferral becomes the gate's input on that host |
| Continuing: limit stop, stall, reboot, fallback | Switchboard (one journal: `duty.jsonl`) | Dashboards' agent retires |
| Watching and history: sessions, load, spend, per host | **Project Observatory** reads Switchboard's records and each host's load | alerts go to the operator through the communicator |
| The screens | **Fabric Dashboards**: Sessions page, consoles, Run | reads Switchboard and Observatory, starts nothing itself |
| Work to do | **Fabric** board and dispatcher | starts work only through `switchboard dispatch` |

### How the operator starts a session

- **In any folder, on the folder's group** (subscription or key, whichever group holds the folder):
  `switchboard launch --provider claude --mode managed --working-directory <folder>`; with
  `--in-place --hosted` it outlives the window.
- **On another machine**: the same with `--target <machine>`; the folder's group there decides.
- **A known job with one action**: a saved launch (section 07): Sessions page, menu-bar icon,
  `switchboard run <name>`, MCP. Until it ships, a `.command` file per job.
- **From Fabric Dashboards**: New session and Run call the commands above; a remote console uses
  `--target`.
- **For the board**: Fabric dispatches; nothing else is needed.

### Load and session monitoring (Observatory)

- One record per session: host, launch id, entry point (surface), group, account, mode, state
  (working, quiet, limit stop, waiting), last turn, and spend for pay-per-token groups.
- One record per host every minute: CPU load per core, memory pressure and swap, free disk, live
  sessions, pool use and board share, subscriptions with room per group.
- Findings with an owner and a fix: host critical, disk under 5 GB, a group with no account with
  room, a launch served by a group other than its own, a session outside Switchboard, a stalled
  session; the communicator notifies the operator once per finding.
- The same figures in Dashboards' Sessions page and in Switchboard's `capacity`.

## Spec

1. **S1, route of a launch** (SB-452): every launch carries its group's route at the highest
   precedence (`--settings`), a worktree is judged by its own folder, and the proxy refuses a request
   whose group differs from its launch's group for a pay-per-token group; mismatches are counted.
2. **S2, environment** (SB-455): every session gets the login `PATH` the owner resolved at start.
3. **S3, remote launches** (Dashboards, board `51c3dcca`): a console on another machine runs
   `switchboard launch --target`; a folder in no group there answers with a sentence, not a sign-in
   screen.
4. **S4, continuity** (SB-450, SB-451): a reboot's sessions continue hosted, once, journalled; the
   launch record's session id comes from the session itself. Dashboards' agent retires.
5. **S5, one admission gate**: `capacity` adds host load and free disk to the pool cap and the board
   share; a start above them waits (`at_capacity`, with the reason) for every entry point; the
   studio-agent's `load.json` is read as the host's input (SB-422).
6. **S6, saved launches** (SB-454, section 07).
7. **S7, sessions and load feed for Observatory**: Switchboard writes `sessions-feed.json` (0600,
   per session and per host, no text a session wrote) and answers it read-only without the owner
   (SB-446); Observatory reads it, fixes the managed classification (a folder-route session is
   managed) and fills `account_group`, keeps history, raises the findings above.
8. **S8, one continuation owner**: duty officer, recovery and fallback share one journal and one
   rule per session; nothing else continues sessions.
9. **S9, subagents and money**: a session's subagents are counted with it (its account and its
   spend); a pay-per-token session's spend is visible live in the Sessions row.

## Plan, in order

| Step | What | Owner | Rows |
|---|---|---|---|
| 1 | Route of a launch and pay-per-token guard | Switchboard | SB-452, SB-453 |
| 2 | Login `PATH` for every launch | Switchboard | SB-455 |
| 3 | Reboot continuation and the session id | Switchboard; Dashboards retires its agent | SB-450, SB-451 |
| 4 | Remote consoles through `launch --target` | Fabric Dashboards | board `51c3dcca` |
| 5 | One admission gate with host load and disk | Switchboard; studio-agent feeds it | SB-422, SB-461 |
| 6 | Sessions and load feed, read without the owner | Switchboard | SB-446, SB-462 |
| 7 | Observatory reads the feed: classification, `account_group`, history, findings, alerts | Project Observatory | board request |
| 8 | Saved launches | Switchboard; Dashboards adds Run | SB-454 |
| 9 | One continuation owner | Switchboard | SB-463 |
| 10 | Subagents and live spend in the Sessions row | Switchboard, Dashboards | SB-464 |

Steps 1 to 3 are release 0.7.118; 4 and 7 run in parallel in their owners' repositories; 5, 6, 8
are 0.7.119; 9 and 10 follow.
