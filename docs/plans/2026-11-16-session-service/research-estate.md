# Research: where coding-session lifecycle lives today, and what a separate always-on session service would inherit

Date: 2026-11-16. Read-only. Commits read (origin/main unless noted):

| Repository | Commit | Path on this Mac |
|---|---|---|
| passioncode-ai/fabric-dashboards | `4526d6a` | `~/DATA/fabric-dashboards` |
| passioncode-ai/fabric | `7be60d93` | `~/DATA/fabric` (working tree on `feat/board-search`, read through `git show origin/main:`) |
| passioncode-ai/fabric-switchboard | `8b33ff73` | `~/DATA/fabric-switchboard` |
| passioncode-ai/fabric-workspace | `43f02e4f` | `~/DATA/fabric-workspace` |
| passioncode-ai/fabric-agent-contract | `db0d8f15` | `~/DATA/fabric-agent-contract` |
| passioncode-ai/fabric-agent-adapter | `7d476bb1` | `~/DATA/fabric-agent-adapter` |
| passioncode-ai/passioncode (launcher 0.2.78) | `fa6dc89` | `~/DATA/passioncode` (working tree on another branch) |
| passioncode-ai/project-observatory-dashboard | `95fdf2cf` | `~/DATA/project-observatory-open-source` |
| ssheleg/sshlg-personal-os (private; the broker) | `7f5b8406` | `~/DATA/sshlg-personal-os` |
| fabric-agents-database (private) | `f39465f9` | `~/DATA/fabric-agents-database` |

Live reads on this MacBook (no service started or stopped): `~/Library/LaunchAgents/ai.passioncode.local-lifecycle.plist`,
the broker's `installation.json` and `registry.json` (no token files opened), and the service descriptors in
`~/Library/Application Support/ai.passioncode.fabric/services/`.

---

## 1. Session-lifecycle functions today: who runs them, and what an app quit or update does to them

### 1.1 Fabric Dashboards (Electron main process)

All of these run inside the Electron main process (`src/electron/console.ts`, class `ConsoleHost`, and `src/core/*`).
The AGENTS.md lifecycle table row for consoles is `AGENTS.md:155`.

| Function | Where | Process | On app quit / update |
|---|---|---|---|
| Console session (PTY) for an agent: the runtime CLI on `node-pty`, or `switchboard launch ... --in-place` | `src/core/consoles.ts:156` (`this.o.spawn` = node-pty), `src/electron/console.ts:56-59` | child of the Electron main process | **Dies.** `will-quit` (`src/electron/main.ts:1219-1256`) calls `consoles.endAllForQuit()` (`console.ts:1175-1191`), which `manager.stopAll()` (`consoles.ts:267-271`): SIGHUP to the group, SIGKILL 2 s later. Output kept in a 1 MB ring, written to `console-output/` (AGENTS.md:155, FD-124). |
| Hosted console (FD-211, 0.8.64): when Switchboard supports `launch --hosted` and `sessions end` (detected by `launchFeatures`, `src/core/switchboard.ts:78-97`), the console runs `switchboard launch ... --hosted` and is only a viewer | `console.ts:692-693` (`hostedKeys`), `console.ts:1199-1218` (`endConsole` → `switchboard sessions end <launch>`) | the session lives in Switchboard's `session-host` (own process, `setsid`) | **Survives.** Quit only detaches the view (`console.ts:1179-1180`); backlog `docs/backlog.md:199` (FD-211). |
| LC-17 admission before every start (disk ≥ 1 GB, swap, ≤ 6 consoles, or Switchboard's machine cap) | `src/core/admission.ts:1-40`, FD-261, FD-283 | main process | n/a (gate only) |
| LC-18 idle close: 30 min without input or output → saved (when, account) and closed; reopening continues | `console.ts:124` (`IDLE_CLOSE_MS`), `console.ts:246-248` (60 s interval), `console.ts:768-795` (`closeIdle`, `closeSaved`) | main process timer | Timer stops with the app; for hosted consoles Switchboard's host has its own idle close (65 min). |
| FD-275 "stuck" close: Claude Code busy 45 min with no transcript write | `src/core/stuck.ts:1-30` (`STUCK_MS`), `console.ts:776-783` (`closeStuck`) | main process, on the idle sweep's minute | Stops with the app. Switchboard's duty officer explicitly leaves Dashboards' consoles alone (Switchboard AGENTS.md:91), so with the app gone nobody watches them (but they died anyway, unless hosted). |
| Limit-stop recovery (FD-83/84): polls `switchboard --json recovery`, claims a ready stop, resumes with `claude --resume <session>` (or codex) in a console of its own, records `finish resumed/cancelled` | `src/core/recovery.ts:1-12`, `console.ts:875-930` (pass), `console.ts:1012-1060` (`resumeOnce`: `this.manager.start(key, {argv: [cli, ...resumeArgs(stop)]})` at about line 1057, a plain PTY, **not** `--hosted`) | main process, on the hand-overs' 15 s/60 s tick | Resumed session **dies** at quit like any non-hosted console. Quit waits up to 2.5 s for a continue in flight, then finishes still-claimed stops as `cancelled` (`console.ts:126-127`, `1183-1190`, AGENTS.md:149). While the app is not running nobody resumes limit stops (Switchboard owns no terminal for them: `recovery.ts:2-4`). |
| FD-262 guard: never a second copy of a conversation a live process holds | `console.ts:946-955` (`heldElsewhere`), `console.ts:1021-1024`, `1410-1411`; backlog `:250` | main process | n/a |
| Switchboard hand-overs (`switchboard handovers run <id>`) taken in a console | AGENTS.md:155, FD-62 | PTY under the main process | dies at quit; one not taken goes to Terminal on Switchboard's side after 2 min |
| Sessions mode: reads `sessions --all`, `--machine`, `attention`, `targets`, remote sessions over ssh | AGENTS.md:159, 203-205 | main process | stops; read-only |
| Auto-update hold: a running console blocks the automatic install | `src/electron/main.ts:492-500` (`busy = ownedCount() + consoles.manager.runningCount()`, retry every 60 s) | main process | Update installs only when no console runs (or on the person's Restart to update), so an update never kills a console by itself; a person's restart does. |
| FD-288 (open): limit continues held by LC-17 are only in `main.log` | `docs/backlog.md:276` | | |

So: Dashboards' own PTY consoles and its limit-resume consoles die with the Electron app; only consoles started with `--hosted` survive (FD-211). The duty functions (idle close, stuck close, limit resume, hand-over taking) all stop while the app is down.

### 1.2 Fabric (Electron main process; `apps/desktop`)

| Function | Where | Process | On quit / update |
|---|---|---|---|
| PTY sessions and coding-agent runners (Fabric's own terminals) | `apps/desktop/src/main/pty.ts:9` (node-pty); AGENTS.md:153 | children of Fabric's main process | "keep running until they end or the app quits; the app's drain on quit (8 s per attempt), then the 10 s hard deadline" (AGENTS.md:153). **Die** on a real quit. Fabric does not use Switchboard's session host (no `--hosted` / `attach` in `apps/desktop/src`). |
| Quit semantics since 0.3.18 (ADR-0192): with «Служба доски» on (macOS), the person's Quit only hides; the hub, board and terminals keep running. A signal (the lifecycle broker, `launchctl`, `kill`), logout, «Перезапустить Fabric» end it within 10 s | AGENTS.md:152; `docs/adr/0192-the-board-is-a-service-launchd-keeps.md:21-37` and Amendment 1 | launchd agent from the bundle (`Contents/Library/LaunchAgents/ai.passioncode.desktop.board.plist`) | A broker stop/restart or an update (a new bundle needs a restart) ends every terminal. ADR-0192's context: a broker idle-stopped Fabric 54 times in one day (the board and terminals went with it). |
| Updates | AGENTS.md:213: no update-feed stage until the updater exists (CO-246) | | manual; every install restarts the process and ends terminals |
| Board dispatcher (CO-409, CO-559): every 30 s reads board requests, starts a run with `switchboard --json dispatch ...` when a project has no live session; watchdog restarts a run whose session did not accept its hand-over in 5 min or whose workflow wrote nothing for 20 min; at most 3 runs per machine | AGENTS.md:174; `apps/desktop/src/main/boardDispatchWiring.ts:286` (`dispatch`), `:460` (`dispatch stop`), `:495` (`dispatch status`); `boardDispatchRun.ts` | Fabric main process; the **runs** are Switchboard owner's children (Switchboard AGENTS.md:86: "Like a Terminal session it outlives a quit owner, and the next owner enforces its limit") | Fabric quitting: "no new start once quitting" (AGENTS.md:174); runs already started continue (they belong to Switchboard and end by their own time limit); the 5/20 min watchdog and the 2 h workflow close stop until Fabric runs again; records persist in `userData/board-dispatches.json`. |
| Sweep of abandoned work (claims, SL-3 nudges), session hooks' turns | AGENTS.md:157 | Fabric main process, every 3 min | in memory; "starts empty after a restart" |
| fabric-mcp per coding session | AGENTS.md (fabric-mcp row) | stdio child of each coding agent | survives Fabric restarts: re-reads `hub.json` every 2 s and re-enrolls |
| Planned: hub in its own process (CO-545) | `docs/adr/0192...md:28`, carry-over spec `docs/evidence/specs/2026-08-16-software-fabric-carryover.md:560` | | open |

### 1.3 Switchboard (context; this repository)

Already the closest thing to the wanted service, but split across processes:

- **Session host** (`switchboard session-host`, SB-342 H-1): its own process (`setsid`), holds the PTY, 1 MiB output, socket `/tmp/switchboard-<uid>/<16 hex>.sock`, up to 8 viewers, idle close at 65 min, never while a limit stop waits. "not the owner, so an update or a quit of the app leaves it running" (AGENTS.md:85; CONTRACTS.md:85). Started by `switchboard launch --hosted` or `session-open` from a Terminal window (H-1b). No supervisor: one process per session, nothing restarts it.
- **Duty officer** (SB-384, DEC-0021): runs **in the owner** (app or `serve`), from the one-minute session watch's thread, every 300 s; leaves Dashboards' consoles alone (AGENTS.md:91). Stops with the owner.
- **Limit-stop records and readiness** (SB-114/115): in the owner's store (`recovery::readiness`, CONTRACTS.md:2008-2026); Switchboard owns no terminal for resuming, Dashboards does (`fabric-dashboards src/core/recovery.ts:2-4`).
- **Monitor of a session's own wake-ups** (SB-396 wave C): the plugin's `PreToolUse` hooks on `ScheduleWakeup`, `Monitor`, `CronCreate`, `CronDelete` and `SessionEnd` run `switchboard monitor hook`, which writes `<data>/monitors/<session>.json`; the duty officer, duplicate close and session host ask `holds` (`crates/switchboard-runtime/src/monitors.rs:1-17`).
- **Dispatched runs** (SB-247): children of the owner, outlive a quit owner (AGENTS.md:86).
- The plan already names the move: wave D of `docs/plans/2026-11-16-session-lifecycle/README.md:86`: "the rest moves into the standalone session service (SB-397)". SB-397 has no backlog row yet (only that mention).
- The org-level session rules SL-1..SL-8 already assign SL-6 "Survives" (session host, any app attaches, update loses nothing) and SL-7 "Supervised" (every watcher under launchd/systemd through the lifecycle broker) (`fabric-workspace knowledge/lifecycle.md:166-194`).

**Gap summary.** Today a session survives an app restart only if it runs in a Switchboard session host (FD-211 hosted consoles, Switchboard Terminal windows from H-1b). Dashboards' limit resumes (plain PTY), Dashboards' non-hosted consoles and every Fabric terminal die with their app. The watchers (duty officer, Dashboards' idle/stuck close, limit resume, Fabric's dispatcher watchdog and sweep) run inside app processes and pause while their app is down or updating; two of them overlap (Switchboard duty officer vs Dashboards FD-275, split by "outside Fabric Dashboards").

---

## 2. The local lifecycle broker

### What it is
A deterministic per-user macOS daemon that wakes, stops and restarts **enrolled** desktop apps and LaunchAgents for agents; no LLM, never takes commands, paths or arguments from a caller (`sshlg-personal-os apps/local-lifecycle/README.md:1-22`; module doc `docs/modules/local-lifecycle.md:1-60`). Six MCP tools: `lifecycle.list`, `.status`, `.ensure_running`, `.stop`, `.restart`, `.operation.get` (module doc `:101-149`).

### Repository and code
- Private personal repo `ssheleg/sshlg-personal-os`, `apps/local-lifecycle/` (`lifecycle.py` 2128 lines, `broker.py` 1450, `native.py` 921, `policy_source.py` 387, `native_app.swift`, vendored adapter kit 0.62.0 in `vendor/`). Constants: `ID = "local-lifecycle"`, `VERSION = "0.2.1"`, `PORT = 47831`, `LABEL = "ai.passioncode.local-lifecycle"` (`lifecycle.py:54-57`).
- Its descriptor declares `source.repository: https://github.com/ssheleg/sshlg-personal-os` (`lifecycle.py:1138-1160`; live descriptor `.../ai.passioncode.fabric/services/local-lifecycle.default.json`).
- **A public port exists**: Project Observatory's `node.lifecycle` (OBS-79), the same mechanics moved into the engine under the operator's standing permission: `observatory/engine/node/lifecycle/{broker,macos,service,supervisor,importer}.py` + `native_app.swift`, served on Observatory's local server `POST 127.0.0.1:47311/mcp`, callers by access binding (`service.py:1-40`; `docs/design/LIFECYCLE.md:1-10, 80-95`). Steps L1-L4 and the engine side of L5 (import) are built; the cutover (operator runs `full lifecycle import`, moves agents, uninstalls the private broker) is not done (`docs/backlog.md:108` OBS-79, `:278-279` OBS-250/251 open). This MacBook still runs the private broker (plist below).
- Agents database: template `host-operations`, variant `lifecycle-broker` (`templates/host-operations/TEMPLATE.md:9, 34, 271-276`; `architecture.md:25, 108-110`), reference code `code/lifecycle_broker.py:1-60`, patterns P10-P17, P20, P30 (`patterns.md:18-25, 28, 38`).

### Install path and mechanism
- Operator CLI from a repo checkout: `python3 apps/local-lifecycle/lifecycle.py install [--no-start]`, then `credential`, `enroll`, `bootstrap` (README.md:46-66). "Only the operator runs installation/enrollment" (README.md:64-66).
- `install()` (`lifecycle.py:1366-1470`): copies `RELEASE_FILES`, `vendor/`, `static/` into an immutable `~/.local/share/local-lifecycle/releases/<version>-<digest12>/` (files 0444, helper 0555), compiles `native-app` with `/usr/bin/swiftc`, verifies the build digest, writes the plist (`~/Library/LaunchAgents/ai.passioncode.local-lifecycle.plist`), the descriptor, and `installation.json` (receipt: release, previous release, plist hash, build digest, native hash); never re-enables a held-off service (LC-14); boots out the old job and waits for it; `switch_release` rolls back if the new release does not answer as itself; writes a stable entry shim `~/.local/share/local-lifecycle/lifecycle.py` (`lifecycle.py:1188-1212`); prunes to new + rollback + running (`:1217-1248`).
- Live plist: `KeepAlive true`, `RunAtLoad true`, `ExitTimeOut 75`, `ThrottleInterval 10`, `FABRIC_SERVICE_SUPERVISOR=launchd`, Homebrew `opt/` Python 3.14, release `0.2.1-24e3132d6e02`, data `~/Library/Application Support/local-lifecycle/default`, port 47831.
- Not a signed binary (LC-05 partial), runs on Homebrew Python (module doc `:478-480`).

### Update path
None automatic. An update is the operator re-running `install` from a newer checkout (module doc "Installed release 2026-11-09" table `:567-578`: `service stop` → `install --no-start` → `service start`). No feed, no LC-16 check. Observatory's port would inherit Observatory's own 5-minute signed update (`project-observatory-dashboard AGENTS.md:236-237, 268-272`).

### How products and agents find it
- Agents: MCP through the communicator agentgateway `127.0.0.1:47835`, route `/mcp/local-lifecycle`, shared caller `local-agents` (README.md:68-73; known gap "All agents share one caller", module doc `:483-485`). Direct loopback with a bearer from a credential file is allowed for approved clients.
- Hosts: the fabric-service/0.1 descriptor `local-lifecycle.default.json` (well-known at `127.0.0.1:47831`, `surfaces.mcp.auth: "own"`, module doc `:536-544`).
- Fabric Dashboards reads the broker's `registry.json` without tokens to refuse Start/Stop of a job the broker owns (`fabric-dashboards src/core/broker-targets.ts:1-15`, FD-270, AGENTS.md:194) and drives its own policy through a policy-source file the broker polls every 2 min (BL-1396, FD-232; module doc `:41-50`).
- Enrolled targets now (live `registry.json`): `dashboards.desktop` always_on + background (rev 6), `fabric.desktop` always_on + background (rev 35; the module doc's table still says on_demand, rev 11/12), `inbox.desktop` on_demand idle 15, `switchboard.desktop` on_demand + background.
- The launcher does not know it (no entry in `passioncode lib/catalog.js:23-80`).

### Could it host or install another always-on service?
- It does not install anything: "installs no target applications" (README.md:9); enrollment needs the operator CLI. It **can supervise** an existing LaunchAgent: enrolled launchd targets (`kind: launchd`, pinned plist, label, executable and file arguments), `kickstart`/`bootstrap`, `always_on` restart within a second with a crash circuit (2 starts/60 s → 5 min) and restart budget (3 failed starts/30 min → `gave_up`) (module doc `:200-240`, `:370-379`). So a session service could be an enrolled `launchd` target with `desiredState: always_on`, but its plist must be written by someone else.
- It is the private personal repo; an organization product cannot depend on it. The public successor is Observatory's `node.lifecycle`; the contract also defines `lifecycle.manager: "node"` (Observatory's node supervises child services; `fabric-agent-contract docs/specification/service.md:1047-1062`, DEC-0061), so "the node starts the service as its child" is a contract-level option, but the node path requires Observatory to be installed.

### Patterns worth reusing (with code)
- Single instance: process lock before token, store or recovery; held lock exits 75 (module doc `:380-384`; contract "One copy", `service.md:1078-1124`, supervisor marker and 300 s wait).
- Durable operations in SQLite (WAL, synchronous FULL), idempotent receipt per (caller, requestId) looked up before policy checks, `idempotency_conflict` (`:380-390`; template P11).
- Policy revision and grant revision on every effect; a stale one is `policy_changed`; registry re-read before each effect (`:295-306`).
- Leases: `ensure_running`/`restart` renew, `status` never; idle stop re-checked under the target lock (P12).
- The person comes first: only what the broker started, not taken over (activation > 20 s after launch), not frontmost with input in 60 s (`:200-232`; P13).
- Effect marker before the OS call, generations fence recovered work, restart resumes at its start half (`:390-405`; P16).
- Quit path, then SIGTERM, then owner-only SIGKILL; pid re-read before each action (P14).
- Crash circuit plus restart budget without time re-arm (P15).
- Immutable release folders, rollback release kept, entry shim outside `releases/`, `installation.json` receipt checked by `bootstrap` (`lifecycle.py:1188-1300`, 1366-1470).
- Stable interpreter path lint, minimal `PATH` (P20; module doc `:356-369`).
- Audit: `events.jsonl` (5000 lines, one sentence per event), JSON-lines `broker.log` 5 x 5 MB 0600; store pruning (receipts 24 h, terminal operations 7 days) (`AGENTS.md:22-27`).
- Bounded shutdown: SIGTERM stops admission, kills children, 8 s worker grace (`AGENTS.md:29-31`).

---

## 3. How products install launchd agents and helpers; shared install mechanisms

| Product | What it installs | How | Updates |
|---|---|---|---|
| Fabric Switchboard | LaunchAgent `ai.passioncode.fabric-switchboard` (`KeepAlive {SuccessfulExit:false}`) as its login item, written by the installed app on first start; `serve` under a supervisor | the app itself (this repo AGENTS.md lifecycle row "Desktop app") | Tauri updater, 5 min feed check; `serve` self-updates since SB-333 |
| Fabric Dashboards | login item through Electron (`app.setLoginItemSettings`, `src/core/loginitem.ts:1-25`); launchd verbs only for other services (`src/core/launchd.ts:1-20`, ADR-0002); detached helpers (`src/core/after-exit.ts`) | the app | Squirrel/ShipIt, 5 min (AGENTS.md:210) |
| Fabric | bundle LaunchAgent `Contents/Library/LaunchAgents/ai.passioncode.desktop.board.plist` registered as an agent service (ADR-0192); `ai.passioncode.fabric-workspace-sync` via `scripts/install-workspace-sync.sh` (operator's) | the app; a script | none yet (CO-246) |
| Project Observatory | `org.project-observatory.<sha16>.{tick,server,maintain,check}` plists | `tools/install_launchd.py install`, `serverd.py --install`, `full maintain ensure` (`project-observatory-dashboard AGENTS.md:234-237`) | its own 5 min signed check, `full update --apply` (AGENTS.md:268-272) |
| Local lifecycle broker | `ai.passioncode.local-lifecycle` | operator CLI `lifecycle.py install` | manual re-install |
| Services built with the adapter kit | launchd plist / systemd user unit / Task Scheduler task, release dirs, pruning | `fabric_supervisors.py` (`launchd_plist` :294, `launchd_install` :711, `prune_releases` :920; header :1-4 cites DEC-0032, LC-14) | per service |

The launcher (`passioncode`, 0.2.78):
- Installs the apps from verified releases (signed `SHA256SUMS.asc` with the bundled release key, team `KJ35UYYL22`, notarization, Gatekeeper) into `~/Applications` or an owned `/Applications` copy; reinstalls an outdated or damaged app only when it is not running (README.md:117-131; `lib/catalog.js:23-80`: switchboard, dashboards, observatory, inbox, fabric).
- Registers each app's MCP server for every agent present (README.md:85-115).
- Installs the skill set as an immutable release `~/.passioncode/releases/<version>` (keeps one for rollback), marketplace `~/.passioncode/current`, `~/.agents/skills` links (`family.json`; README.md:85-100); self-updates from Claude Code's SessionStart hook with a trust gate (README.md:182-200).
- Release trains pin app versions per organization and never downgrade (README.md:163-180).
- `doctor` reads service descriptors only for catalog products with `serviceIds` (today Observatory) and repairs a launchd service that does not answer with `launchctl kickstart`, never a kill (`lib/doctor.js:34-39, 513-545`).
- Owns no launchd job, port or resident process (`AGENTS.md:195-210`). It does not install the broker or any shared daemon.

**Is there a shared "install if missing, keep the newest version" mechanism?** No. What exists is per product: the launcher installs missing apps and replaces older ones (a cross-product installer, but of apps, not of a shared daemon, and only when someone runs it); each app installs its own login item; the adapter kit gives services a launchd/systemd/Task Scheduler installer and release pruning; the broker has an install/rollback/receipt layout; Dashboards' MCP relay hands a session to the newest installed copy (FD-109/FD-150, AGENTS.md:153). Nothing lets Switchboard, Dashboards or Fabric each carry a copy of a shared helper and install it when absent or older. The agents database's `installer` template (`templates/installer/TEMPLATE.md:1-45`, variant `cli-launcher`) explicitly delegates "starting, stopping or supervising apps and services" to the host-operations lifecycle broker, and pattern P30 "Verified update, safe point, rollback by layout" (`host-operations/patterns.md:38`, `code/release_update.py`) is the closest reusable piece.

---

## 4. What the contracts require of a new session service, and where it must be declared

### Lifecycle contract (`fabric-workspace knowledge/lifecycle.md`)
- LC-01 quit within 5/10 s, nothing starts after quit begins (`:23`). For a session service the drain must not kill hosted sessions (that is the point), so it must say so in its own AGENTS.md (the contract allows a documented deviation, `:4-5`).
- LC-02 every child has an owner, a deadline, a process group; no orphans with ppid 1 (`:24`). Session hosts are intentionally long-lived and detached today (`setsid`); the new service should own them as children or record them so none is an orphan (SL-4 "no process outlives its record").
- LC-03 scheduled work bounded, exclusive (flock), with a status record; journal-first (`:25`).
- LC-04 credentials silent; LC-21 no secret on argv (`:26`, `:43`).
- LC-05 stable signed identity; `ProgramArguments[0]` a stable signed executable, never a Homebrew/Cellar path; brokers pin by designated requirement (`:27`). The private broker itself does not meet this (Homebrew Python); a session service shipped by the products should be a signed binary (Switchboard's `switchboard` CLI in the bundle already is).
- LC-08 idle means idle, published idle budget (`:30`). LC-09 the footprint declared in AGENTS.md *Lifecycle* (`:31`).
- LC-10 per-session servers exit with their session and with their code (`:32`).
- LC-11 an update is a lifecycle event for every process; addresses other processes hold survive a restart; previous release kept (`:33`). Directly relevant: socket paths of session hosts must survive the service's update.
- LC-12 bounded files; LC-14 symmetric uninstall, operator's off switch wins; LC-15 builds prune (`:34-37`).
- LC-16 every installed copy updates itself: 90 s then every 5 min, verified, "never by killing a session" (`:38`, detail `:47-70`).
- LC-17 admission; LC-18 idle session saved, closed, resumable with stated idle time; LC-19 shared stateless tools run once under the lifecycle broker; LC-20 limits never stop work; LC-22 work survives a restart (journal before start, a restarted owner takes it back) (`:39-44`).
- LC-23 closing a window keeps the app running (`:45`).
- Broker section: agents never `open -a`/`pkill` products; policy per product; who may stop what (`:104-142`). Background launch `--background` (`:144-164`).
- SL-1..SL-8 for coding-agent sessions (`:166-194`): SL-4 End (every end recorded, hosted session closes after its idle time, no process outlives its record), SL-5 Limits (resume in place, never twice), **SL-6 Survives** (session host, any app attaches, update or restart loses nothing), **SL-7 Supervised** (every watcher under launchd/systemd through the lifecycle broker; no loop left half way), SL-8 One view.

### Agent contract (`fabric-agent-contract`)
- `fabric-service/0.1` (`docs/specification/service.md`): descriptor in the services folder (`~/Library/Application Support/ai.passioncode.fabric/services/<id>.<instance>.json`), well-known `/.well-known/fabric-service`, events feed, operator login, usage report (`:46-450`). Lifecycle table (`:1047-1062`): one copy (`service.lock`, exit 75, supervisor marker, ≤ 300 s wait), port claim (HA-1), supervisor per OS (`launchd` with `RunAtLoad`, `KeepAlive`, `ThrottleInterval 10`, `ExitTimeOut` above the drain; `systemd`; `task-scheduler`; `node`; `person`), install/stop/off/code/state/uninstall rules; code from an immutable release directory; directories per system (`:1064-1076`); settings backups (`:1126-1233`, DEC-0025). Several OS accounts on one computer (`:580-820`).
- `session-handoff/1` (DEC-0042, `docs/specification/session-handoff.md:1-25`): the folder a host writes before a session's first turn (Fabric and Dashboards both write one).
- `agent-health/1` (DEC-0179, `docs/specification/agent-health.md:1-30`, FAC-SEM-181): one reading per product; Dashboards will read it every 5 min as the machine's observer (FD-285, `fabric-dashboards docs/backlog.md:273`). A session service would publish one (areas `lifecycle`, `sessions`).
- ops digest (`ops.digest.hourly@1` / `daily@1`, DEC-0153): every product writes them (Switchboard, Fabric do).
- Semantic checks LC-19/LC-21 via DEC-0044 (`FAC-SEM-056`, `FAC-SEM-057`) (lifecycle.md:41, 43).
- No contract document defines a session-host protocol: Switchboard's socket protocol (HELLO/OUTPUT/INPUT/RESIZE/DETACH/END/EXIT/HISTORY frames, protocol 1) lives only in Switchboard `docs/CONTRACTS.md:85`. If apps from several repositories attach, it should become a contract document (agent-contract decision) rather than a Switchboard-internal format.

### Where a new shared service must be declared
- `fabric-workspace knowledge/products.md:17-37` (product table) and the "Install and drive over MCP" table (`:39-57`) if it is its own product; `org-index repositories.json` row (Switchboard AGENTS.md "Organisation").
- `fabric-workspace knowledge/roadmap.md`: an RM track or a row under RM-27 "Machine resources and continuity" (`:234`) and RM-19 (self-update, `:233`); the unified plan `docs/plans/2026-11-11-unified/README.md` (SL answers are its row 2.8, lifecycle.md:194).
- `fabric-workspace knowledge/lifecycle.md`: SL-6/SL-7 owners column, the broker section, and a statement of how the service deviates from LC-01/LC-02.
- Its own `AGENTS.md` *Lifecycle* table (LC-09) and every product that delegates to it (Switchboard, Dashboards, Fabric) updating theirs.
- A `fabric-service/0.1` descriptor and well-known document, plus a contract decision for the session-host protocol (and agent-health reading).
- The launcher's `lib/catalog.js` (if the launcher should install or doctor it) and its `doctor` services check.
- The agents database: consult the architect (`audit.py consult`); nearest template `host-operations` (lifecycle-broker variant) and `agent-host` (`templates/agent-host/playbook.md:241` already speaks of "the account service's session host").
- Switchboard: SB-397 needs a backlog row (only mentioned in `docs/plans/2026-11-16-session-lifecycle/README.md:86`); Fabric CO-545 (hub in its own process) is the analogous track there.

---

## 5. Observations for the design

1. The org already decided the lifecycle broker's mechanics move into a public product (Observatory `node.lifecycle`, OBS-79), and Switchboard's session host already gives SL-6 for hosted sessions. A separate session service would combine: Switchboard's session host and duty officer and limit records, Dashboards' limit-resume and idle/stuck close, and possibly Fabric's dispatcher watchdog.
2. "Installed separately by whichever product is installed first" has no existing mechanism. Building blocks: the adapter kit's supervisor installer (`fabric_supervisors.py`), the broker's release layout + receipt + entry shim + rollback (`lifecycle.py:1188-1470`), the launcher's signed-release verification, Observatory's `lifecycle.manager: node`. A version-compare-and-install-if-newer step would have to be added (each product bundles a signed copy, installs it into a shared versioned location under one lock, never downgrades, never kills live sessions, LC-11/LC-14/LC-16).
3. The supervisor of that service would be launchd directly (contract Supervisor row); the lifecycle broker supervises apps, it does not install services, and the private broker cannot be a dependency of an organization product.
4. Hazards seen: the broker restarting or idle-stopping Fabric kills its terminals (ADR-0192 context, 54 stops); `fabric.desktop` is now always_on (rev 35) while the module doc still says on_demand; Dashboards' auto-update is held by any running console (a session service would remove that hold); session hosts run from the app bundle path, so a bundle replaced under a running host keeps the old image, and the host protocol version (`protocol: 1`) must stay compatible across versions.
