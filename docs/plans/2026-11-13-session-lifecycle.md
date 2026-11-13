# Coding-agent sessions: one lifecycle, checked by every project

Plan, 2026-11-13. Source: the operator in session `59745113`, 2026-11-13 ~02:50Z: «разберемся с
нашими сессиями, агентскими клод-кодами, чтобы они правильно запускались, с правильными
параметрами, с правильными задачами, правильно завершались, не зависали, контролировались
дополнительными службами надежными, чтобы их лайфсайкл трекать. И это на всех проектах: фабрика,
дашборды, свитчборд и обсерватория, везде, где у нас этот протокол … добавь это всем нашим проектам
PassionCode план, чтобы все, каждый, состоятельно себя проверили». The agents database's architect
was consulted first (closest template `agent-host`; patterns `resumable-jobs` and `sandbox-queue`).

## Where we stand (this MacBook, 2026-11-13 02:55Z)

- 32 Claude Code processes run; 28 of them started more than a day ago, all interactive.
- Switchboard's own record (`switchboard sessions --all`) knows 36 sessions it launched: 35 ended,
  1 running. The other 31 running processes were started outside Switchboard (Fabric's consoles,
  Terminal by hand) and appear in no lifecycle record of Switchboard's.
- Fabric's `fabric_agents_list` answered one enrolled session (this one); `ListAgents` named 30.
- Twelve sessions that stopped on a limit on 2026-11-12 stayed stopped for hours after capacity
  returned: Fabric Dashboards could not find their transcripts (fixed in Switchboard 0.7.70,
  SB-348); four more stopped while their windows had closed and nobody could resume them.
- An update check cut by a timed-out request left the app "downloading" for hours (SB-344,
  SB-351): a lifecycle with no owner for its end hangs.

So no single place answers «which agent sessions run now, on which task and account, since when,
and how each one ended», and an end that nobody records is indistinguishable from a hang.

## The lifecycle every session follows (SL-1 to SL-8)

| Step | What must hold | Who owns it |
|---|---|---|
| SL-1 Start | Every agent session is started by an owner, never by hand: Switchboard (`launch`, `launch --hosted`, `dispatch`), Fabric Dashboards' console, or Fabric's dispatcher. The start records the folder, group, account, agent and model, permission mode, `launch_id` and who asked (`launched_by`) | Switchboard (records), Dashboards and Fabric (call it) |
| SL-2 Task | A session starts with one named task: a board request, a task id, a workflow checkpoint (Observatory) or a dispatch request. The task id is in its launch record and its first act is to claim it (`fabric_work_claim`) | Fabric (board, claims), Observatory (workflows) |
| SL-3 Alive | A running session signs every turn (`fabric_session_observe` turn, Switchboard's turn hook, `fabric_heartbeat`). A session with no turn for its window (default 30 minutes while it holds a claim) is *stalled*: the owner nudges it once, then reports it | Fabric (nudges), Switchboard (turns) |
| SL-4 End | Every end is recorded with its kind: `exited`, `stopped`, `timed_out`, `idle`, `limit`, `ended` by a person. Claims are released at the end (`fabric_work_release`), a dispatched run has a time limit, a hosted session closes after its idle time, and no process outlives its record | Switchboard (records, hosts, dispatch limits), Fabric (claims) |
| SL-5 Limits | A session stopped on a limit is recorded (`switchboard recovery`) with its transcript, and resumed in place when its group has room again, never twice, never on an account without room (SB-345) | Switchboard (stops), Dashboards (resume) |
| SL-6 Survives | The session does not depend on the window or the app that showed it: it runs in a session host (`switchboard session-host`), any app attaches to it, and an update or a restart of an app loses nothing | Switchboard (hosts), Dashboards (attach) |
| SL-7 Supervised | Everything that watches sessions runs under a supervisor that brings it back (launchd or systemd through the lifecycle broker), and a watcher's own loop can never be left half way (a cut request, a dropped future) | each owner of a watcher; the lifecycle broker |
| SL-8 One view | One answer to «what runs now»: every running Claude Code or Codex process on the machine is either in an owner's record or listed as *unmanaged* with its folder and age. Fabric Dashboards shows it; Observatory keeps the history | Dashboards (view), Observatory (journal), Switchboard (the process scan) |

## What each project checks in itself

Each PassionCode project runs this self-check against its own sessions and products and answers on
the board thread of the announcement, naming each SL step as *holds*, *gap* (with the row it filed)
or *not mine*:

1. Who starts the agent sessions this project uses, with which parameters, and where the start is
   recorded (SL-1).
2. Whether each session carries a task id and claims it (SL-2).
3. Whether its sessions sign their turns, and what happens when one goes quiet (SL-3).
4. How each kind of end is recorded and whether claims, processes and hosts are released (SL-4).
5. How a session stopped on a limit comes back (SL-5).
6. What of the session dies with a window, a console or an app update (SL-6).
7. Which of its loops can be cut half way and leave a state nobody clears (SL-7): an awaited call
   whose caller may stop waiting, a flag set before work and cleared only after it.
8. Whether its sessions appear in one view (SL-8).

## Switchboard's own answers (2026-11-13)

| Step | Switchboard | Row |
|---|---|---|
| SL-1 | holds for what it launches (`launches/<id>.json`: folder, group, account, agent, `launched_by`); sessions started outside it are not seen | SB-353 (process scan, below) |
| SL-2 | partly: `continues` and `dispatch.request` carry a task for dispatched and continued runs; a launch by hand carries none | SB-353 |
| SL-3 | the turn hook records `last_turn_at` and `turns`; nothing marks a session stalled | SB-353 |
| SL-4 | holds for dispatched runs (time limit, `stopped`, `timed_out`) and hosted sessions (`idle`, `ended`); a Terminal session's end is known only by its process marker | SB-342 (hosts for every launch) |
| SL-5 | holds since 0.7.70 (transcript in `recovery`, SB-348) and 0.7.66 (room, SB-345) | done |
| SL-6 | holds for hosted sessions (SB-342, `attach`, `sessions end`); Terminal windows run through a host since 0.7.64 | done |
| SL-7 | the update loop: SB-344 (0.7.67) and SB-351 (0.7.71); `sessions --history` slow through the owner | SB-347 |
| SL-8 | `sessions` lists only Switchboard's launches | SB-353 |

**SB-353 (new):** a scan of the machine's Claude Code and Codex processes (pid, folder, age, last
transcript write) joined to the launch records, answering `switchboard sessions --unmanaged` and the
MCP tool, with a session marked *stalled* when it holds a claim and wrote nothing for 30 minutes.
Fabric Dashboards shows it; the journal goes to Observatory.

## Rollout

1. This plan in fabric-switchboard (here), and SB-353 on its board.
2. A board announcement to every PassionCode project with the eight questions, due 2026-11-15,
   and a request to fabric-workspace to make SL-1 to SL-8 part of `knowledge/lifecycle.md` next
   to LC-01 to LC-15 and of the unified plan.
3. Owners build their gaps in waves; the operator's 10-minute rounds check SL-8's view.
