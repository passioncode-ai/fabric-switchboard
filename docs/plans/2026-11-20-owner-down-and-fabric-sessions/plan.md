# The owner comes back by itself, sessions outlive it, and Switchboard's sessions are Fabric's

Board request `798aee09` from org-index (2026-11-20, the operator's words in org-index session
`11fa0559`: «вот такое висит в сессия надо разобраться и исправить баги»; «Если есть фабрик, то
оно автоматически становится фабрик. А если нет фабрик, то это да, в свитчборд … Если свитчборда
вообще не будет, как они будут работать … менеджер, который у нас общий модуль»). Rows: SB-588 to
SB-593 in [the backlog](../../evidence/backlog.md). Serves North Star §6 (work continues) and the
lifecycle contract LC-01.

## 1. What happened on 2026-11-20 (evidence)

Read on this machine at 18:40Z from the owner's log
(`~/Library/Logs/Fabric Switchboard/switchboard.log`) and `launchctl`:

| Fact | Evidence |
|---|---|
| The owner stopped at 16:49:15Z through its drain, which overran its 8 s deadline | `{"event":"owner_stopped","millis":8053,"outcome":"abandoned"}` |
| It was a quit, not a signal | no `stop_requested` line before it (a signal writes one: `src-tauri/src/main.rs:2195`) |
| The process ended with status 0, so launchd did not start it again | `launchctl print gui/<uid>/ai.passioncode.fabric-switchboard`: `runs = 1`, `last exit code = 0`; the job's `KeepAlive {SuccessfulExit: false}` restarts only a failure (SB-254, `src-tauri/src/residency.rs:81`) |
| The next owner started at 18:01:11Z, 4316 s later, outside launchd | `owner_started pid 88370`; `launchctl` still shows the job `not running`, the process's parent is pid 1 (opened through LaunchServices) |
| The watchdog saw it and could only report | `watchdog --once` every 300 s; an owner whose last run ended clean reads `stopped`, code `owner_stopped`, severity `warn` (`crates/switchboard-runtime/src/watchdog.rs:44`, `:114-117`); it "never acts" (`:1-14`) |
| 9 of the last 10 stops overran the drain | 10 `owner_stopped` lines in the current log: 9 `abandoned` (8053 to 8840 ms), 1 `drained` (an update restart, 97 ms) |
| Who restarted the others is not recorded | the other 8 abandoned stops came back after 1 to 225 s; nothing in the log says who quit or who started it |

So three defects, not one: a quit that nobody follows with a start leaves every bound folder
without its route; the log cannot say who quit; the drain is regularly cut at its deadline.

## 2. Decisions

- **D1. A quit that is not the person's own is followed by a start.** The person's Quit (tray,
  app menu, Cmd-Q, About's restart) stays final, as SB-28 and SB-254 decided. Every other end
  (an Apple Event `quit` from another program, a signal, a drain cut by its hard exit) ends with a
  non-zero status, so the LaunchAgent's `KeepAlive {SuccessfulExit: false}` starts it again within
  its 30 s throttle. Safest reversible choice: it reuses the job that exists, changes no plist, and
  a person's Quit behaves as today.
- **D2. The watchdog acts once on a stop that is not the person's.** It starts the owner
  (`open -g -a "Fabric Switchboard" --args --background`, the same start the login item makes)
  when the owner has not answered for 2 minutes and the last run's `owner-run.json` names an end
  that is not the person's Quit, at most once per stop. It still never kills anything.
- **D3. A bound folder does not keep a dead route.** When the owner stays down past the watchdog's
  start (10 minutes), the watchdog takes Switchboard's own lines out of the folders' route
  (`claude-bindings.json` already holds the digest of exactly what was written, SB-118), so a new
  session in that folder runs on its own sign-in; the owner's next bindings pass writes them back.
  Whether a running Claude Code session picks up the removal without a restart is measured first;
  if it does not, the row says so and the running sessions are listed in Needs attention.
- **D4. The two-sided Fabric part waits for Fabric's paired task.** Switchboard keeps being the
  layer of accounts and launches. When Fabric is installed and answers, a session Switchboard
  starts is started as a session of its Fabric project through the session manager module both
  read (`crates/fabric-sessions-core`, SB-397), carrying Fabric's launch marker; when Fabric is
  absent, nothing changes. The marker's issuer, value and the hub call (ADR-0134) are Fabric's to
  name; the module change is shared.

## 3. Rows and dates

| Row | What | Done by |
|---|---|---|
| SB-588 | D1: the end's origin recorded (`stop_requested` gains `origin`: `person_quit`, `apple_event`, `signal`, `hard_exit`, `update`) and `owner-run.json` keeps it; every end but the person's Quit and an update's relaunch exits non-zero; test that a person's Quit exits 0 and the others do not | 2026-11-22 (PR); in the next release after it |
| SB-589 | D2: the watchdog starts the owner once per stop that is not the person's, after 2 minutes down; job interval kept at 300 s, plus a check at the job's own start | 2026-11-23 (PR) |
| SB-590 | `agent-health/1` and `watchdog.json`: `owner_down` and `owner_stopped` carry `down_seconds` and `bound_folders` (how many folders route to the proxy now), and `owner_stopped` becomes `fail` when its end was not the person's Quit, so the Dashboards observer opens an episode | 2026-11-22 (PR, with SB-588) |
| SB-591 | The drain overran 8 s in 9 of 10 stops: find which step holds it (the stop's own timing per step in the log) and bring it under the deadline | 2026-11-24 (finding and fix or a dated next step) |
| SB-592 | D3: a bound folder does not keep a dead route; measured first whether a running session takes the removal | 2026-11-24 (measurement); the change after it |
| SB-593 | D4: Switchboard's launches are Fabric sessions when Fabric answers, through `fabric-sessions-core`; waits on Fabric's paired task for the marker and hub call | plan agreed on the board by 2026-11-24; code dated once Fabric names the marker |

Each row ships through the gate (`./scripts/check.sh`) and its PR; a release carries them. A row
whose date passes gets a new state with a new date in the board thread of `798aee09`.
