# SB-342: one session host, so a session opens in any of our apps and ends cleanly

**State:** proposal, 2026-11-12; H-0 released in 0.7.60; H-1 built (see the decision below). Row: [SB-342](../evidence/backlog.md). Source: the operator, 2026-11-12 about 03:10 UTC,
this repository's session `59745113`: «я не могу нормально подключаться к сессиям, которые открыты например в свитчборде,
и не могу и открыть в дашборде, и вообще проблема с открытием сессий в разные приложения наши, медленно глючит или не
открывает вообще, а после завершения не закрывает и копит, а должен быть lifecycle manager для сессий и чтобы можно было
и открывать между приложениями». Needs the agreement of Fabric Dashboards and Fabric before code (board thread below).

## What happens today

Each app that starts a coding session owns its terminal, and no other app can reach it.

| Who starts it | Where the terminal lives | Can another app open it? | When it ends |
|---|---|---|---|
| Switchboard window, `switchboard launch` without `--in-place`, a hand-over nobody takes, the automatic fallback | a Terminal.app window running `launch.command` (`launch::start_terminal`, `crates/switchboard-runtime/src/launch.rs:672`) | no: the PTY is Terminal.app's | the window stays open after the CLI exits (Terminal's default profile keeps it), so windows pile up |
| Fabric Dashboards console (New session, Continue, a hand-over it takes, a limit continue) | its own PTY (`node-pty`, fabric-dashboards `AGENTS.md:125`), at most 6 | no: the PTY is the Dashboards process's | closed after 30 min idle (LC-18) or when the app quits |
| Fabric's own terminals | Fabric's PTY | no | Fabric's own rules |
| `switchboard dispatch` for a board request | no terminal: `claude -p` as the owner's child, output in `launches/logs/<id>.log` (SB-247) | nothing to open; FD-207 shows it as "running in the background" and SB-341 links its log | the run exits or the time limit stops it |

Switchboard's launch records already know every launch (SB-212: 328 records on 2026-11-12, `sessions --history`), its state
(`starting`, `running`, `ended`, read from the home's `.session.json`) and who started it, but they carry no way to reach
the terminal, because no Switchboard process holds one. Measured 2026-11-12 04:15 Warsaw on the operator's MacBook: one
launch `running` (a dispatch run), zero Terminal windows, zero Dashboards consoles.

## Proposal

**One per-user session host, owned by Switchboard's resident owner**, holds the PTY of every interactive session it
starts. Every app is a viewer that attaches to it by launch id; closing a viewer detaches, never kills.

1. **Host.** The owner (the app or `serve`, already resident, already recording launches) spawns each interactive session
   on a PTY it holds (`portable-pty` in Rust: macOS, Linux and Windows ConPTY), keeps a scrollback ring (1 MB, as
   Dashboards' console does), and serves attaches on a per-user socket beside `control.json` (Unix socket 0600, a named
   pipe on Windows), authenticated with the control token. A session outlives every viewer; it ends only when its
   process exits, the person ends it, or the owner's idle rule closes it.
2. **Attach.** `switchboard attach <launch_id>` puts the session in the current terminal (raw mode, resize, detach key).
   A Terminal.app window Switchboard opens runs `switchboard attach`, so closing it detaches, and the window closes by
   itself when the session ends. Fabric Dashboards and Fabric attach their xterm views to the same socket (a small
   framed protocol: output, input, resize, exit) instead of starting their own PTY, and keep their own PTY only for a
   session Switchboard cannot start (a runtime it does not know).
3. **Lifecycle.** The host is the one place that knows a session's state: `starting`, `running`, `idle` (no output
   and no viewer for N minutes), `ended` (exit status, time). On exit it writes `ended` to the launch record, tells every
   viewer (`exit` frame, so Dashboards and Terminal close their view), and reaps the process. Ended sessions leave the
   live list at once; their record and the last screen stay for the history (90 days, as today). An idle rule (the
   person's setting, default the 30 minutes Dashboards uses) saves and closes a session nobody watches; opening it
   again continues it with `--resume`.
4. **One list.** `switchboard sessions` (and MCP `switchboard_sessions`, the Dashboards Sessions mode, Fabric) shows the
   same live sessions with `attachable: true` and the viewers attached now, so any app can say «open here».
5. **Moving between apps.** «Open in Dashboards» on a Terminal session, «Open in Terminal» on a Dashboards one, and
   Fabric's view are all attaches to the same session; nothing restarts, nothing is resumed, the conversation is the
   same process.

Dispatch runs stay headless (`claude -p` reads no input) but get the same lifecycle and a read-only attach of their log.

## Decision in H-1: a host process per session, not a thread of the owner

The proposal put the host inside the resident owner. Building it showed why not: the installed
app restarts itself into every update (LC-16) and on a quit, and a PTY the owner holds would end
every session with it. Each hosted session therefore gets a small host process of its own
(`switchboard session-host`, in its own process session), as tmux and dtach do; the owner only
records the end it reports. The socket moved out of the data folder to `/tmp/switchboard-<uid>/`
(0700) because the data folder's path is too long for a Unix socket address on macOS; the data
folder keeps the descriptor (`sessions/<launch_id>.json`). The contract is in
[CONTRACTS](../CONTRACTS.md) (SB-342 H-1).

## Order of work

- **H-1 (Switchboard):** the host and `switchboard attach`, Terminal windows through it, lifecycle in the records,
  `attachable` in `sessions`; tests with a fake runtime; macOS and Linux first, Windows ConPTY next.
- **H-2 (Fabric Dashboards):** attach the console to the host for sessions Switchboard holds; Open in Terminal and Open in
  Dashboards as attaches; the Sessions mode from `switchboard sessions`.
- **H-3 (Fabric):** the same attach for Fabric's views of coding sessions.
- **H-4:** each app's own PTY path kept only for runtimes Switchboard does not start, then retired where none remain.

Until H-1 lands, a quick relief for the pile-up: the `launch.command` Switchboard writes closes its own Terminal window
when the session exits (H-0).
