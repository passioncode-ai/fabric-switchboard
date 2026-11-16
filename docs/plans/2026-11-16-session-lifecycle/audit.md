# Session lifecycle audit: Fabric Switchboard

Date: 2026-11-16. Checkout read: `.claude/worktrees/agent-a3dff6e86b720f53e`, branch `sessions-view`
(base `origin/main` 2eb3d226; the session that ordered this audit committed `072ab065 wip SB-395 sessions page` and kept
editing `session_rows.rs`, `src/main.ts`, `src/demo.ts` while this audit ran, so `session_rows.rs`
line numbers are those of the working tree at 16:40 local). Read only; nothing was edited.

Operator's ask (2026-11-16, paraphrased): a session in a monitoring mode (finished its work, now
waiting: `ScheduleWakeup` of at most 3600 s, a `Monitor`, a background watcher, `CronCreate`) is never
kicked; a monitoring session with no known end may be allowed to die, but later the SAME session is
restored (`--resume <session id>` with its context), never a new one; a session stopped on a usage
limit comes back as the same session when an account frees.

Verdict in one paragraph: the duty officer's own rules are careful and well tested, but the
lifecycle has four independent killers (duty officer, session host idle close, dispatch timeout,
duplicate close) that do not share one notion of "monitoring", none of them reads the limit-stop
registry, and nothing in Switchboard records what is needed to resume a closed session later, or
ever resumes one by itself. A monitoring session is protected only by a wall-clock 65-minute floor
in one of the four killers. The resume-same-session requirement is met only for the duty officer's
`stuck` restart and, outside this repository, by Fabric Dashboards for limit stops.

---

## 1. The lifecycle as built

### 1.1 Who observes what

| Reader | Reads | Evidence |
|---|---|---|
| Process scan | this user's process table in-process (`proc_listpids`), Claude Code and Codex processes | `machine_sessions.rs:444-460`, `agent_of` `:32-51` |
| Session files | `sessions/<pid>.json` in `~/.claude`, `homes/*`, `runtimes/*`, `shell/*`: `pid`, `sessionId`, `cwd`, `kind`, `status`, `statusUpdatedAt` (fallback `updatedAt`, then process start), `startedAt` | `machine_sessions.rs:183-207`, `:308-319`; `duty.rs:210-241` |
| `waitingFor` | written by Claude Code 2.1.299 (plan README:64-65) but **parsed nowhere** (grep of `crates`, `src`, `src-tauri`) | `duty.rs:225-228` reads only `status`, `statusUpdatedAt`, `updatedAt` |
| Process tree | `ps -axww -o pid=,ppid=,etime=,time=,command=`: CPU of the session's tree, its "commands" = direct children whose command line names `/.claude/shell-snapshots/` | `duty.rs:312-326`, `tree_of` `:334-359` |
| Transcript stamps | per session: `config/projects/<folder>/<session>.jsonl` and its subagents; per folder (watch): newest `.jsonl` of the folder in any home | `machine_sessions.rs:211-236`, `:62-73`, `:426-437` |
| Launch records | `launches/<id>.json`: `provider_session_id`, `transcript`, `last_turn_at`, `turns`, `ended` (set by hooks, hosts, dispatch) | `launches.rs:904-974`, `end_once` `:424-449` |
| Home markers | `.session-pid`, `.session.json`, `.launch-pending` per home | `launch.rs:3583-3611`, `session_running` `:1483-1529` |
| Limit stops | `recovery.json`: event, session, account, pool, directory, state, attempt, kind | `recovery.rs:62-82` |

Claude Code statuses the code knows: `busy`, `shell`, `idle`, `waiting` (`duty.rs:6-9`, `:178-199`);
`machine_sessions.rs:90` documents only `busy`, `idle` (drift, P3). An unknown status is never acted
on (`duty.rs:198`), a safe default.

### 1.2 States and transitions

| State | Meaning in code | Entered by | Left by | Reads |
|---|---|---|---|---|
| launched / `starting` | `.launch-pending` in the home, reservation not expired | `launch` writes record and marker | the script runs and writes `.session-pid` | `launch.rs:3583-3599` |
| `running` (home) / live row | marker pid alive and not reused; or any Claude/Codex process | the agent process | process exit | `launch.rs:1483-1529`; `machine_sessions.rs:350-391` |
| working / quiet (row) | folder transcripts written within 30 min (folder-level, not per session) | time | time | `machine_sessions.rs:15-16`, `:365-374`; `session_rows.rs:100-104` |
| busy, shell | Claude Code says it is answering or in a shell command | Claude Code | Claude Code | `duty.rs:189` |
| `stuck` (duty) | busy/shell, status and transcript still for `stuck_minutes` (45), tree CPU < 2 s since last check, no own command or one 3x older | duty `decide` | restart: SIGTERM/SIGKILL, then Terminal `claude --resume <id>` with a continue prompt | `duty.rs:187-196`, `act_on` `:606-642` |
| `stuck` (watch finding) | a **dispatched** launch whose folder is quiet >= 30 min | `session_watch::findings` | folder written again | `session_watch.rs:109-118` |
| `idle` | Claude Code's turn ended | Claude Code | next turn | `duty.rs:181-186` |
| waiting on own wake-up / monitor | **not represented**. A `ScheduleWakeup` wait reads `idle`; a `Monitor` or background Bash reads `idle` + `commands > 0` only if its shell is a direct child with a snapshot path | none | none | plan README:64-74; `duty.rs:13-17`, `:353` |
| `waiting` | a question or permission for the person; never acted on | Claude Code | the person | `duty.rs:197`, `:533` |
| closed by duty (`idle`) | idle >= max(idle_minutes, 65 min) and no own command | duty `check` every 300 s | none: process gone; a line in `duty.jsonl` | `duty.rs:181-186`, `:541-603` |
| closed by duty (`duplicate`) | a finished copy (idle, no command) of a session another process also holds | duty `newer_copies` | none | `duty.rs:392-414`, `:172-174` |
| closed by host (`idle`) | hosted session, no viewer and no PTY in/out for 30 min | `session_host` idle thread | `SessionEnded` writes `ended {kind: idle}` | `session_host.rs:51-53`, `:444-458`; `lib.rs:5913-5928` |
| `timed_out` | dispatched run past its deadline (default 60 min wall clock) | `dispatch::sweep` | `ended {kind: timed_out}` | `dispatch.rs:534-566` |
| stopped on a limit | `StopFailure` hook (`rate_limit`, `billing_error`) | plugin hook -> `RecoveryHook` -> `register_stop`; the launch's own hook sets `ended: "limit"` | see below | `plugins/switchboard/hooks/hooks.json`; `lib.rs` RecoveryHook; `recovery.rs:322-381`; `launches.rs:962-965` |
| stop `waiting` | registered, no claim | `register_stop` | `claim`, `continued` (Stop hook), horizon, `finish(cancelled)` | `recovery.rs:322-381` |
| stop ready (`capacity` / `probe`) | an account with fresh room under threshold-less-hysteresis, or backoff 2/5/10/15 min passed | `readiness` | `claim` | `recovery.rs:542-591`, `:383-441` |
| stop `claimed` | a host chose an account; **Switchboard launches nothing** | host -> `RecoveryClaim` | host `finish`, Stop hook, next `StopFailure` (marks it resumed), horizon | `recovery.rs:595-621`, `:5`; CLI.md "No command here sends a prompt or restarts a client" |
| stop `resumed` | host report after a claim, or the session's own `Stop` hook | `finish(true)`, `continued` | terminal | `recovery.rs:623-672` |
| stop `cancelled` | host report, or a waiting/claimed stop older than 24 h | `finish(false)`, `prune` | terminal | `recovery.rs:120-128`, `:22-24` |
| `ended` (launch) | `ended` set: object `{kind, at, exit_code?}` or the strings `"limit"`, `"not_started"` | hosts, in-place exit, dispatch, hooks | terminal (`end_once` sets once) | `launches.rs:424-449`, `:962-965`, `:819-826` |
| `gone` | launch with no live process, no `ended`, >= 10 min old | `session_watch::findings` | none | `session_watch.rs:120-126`, `:174-180` |

### 1.3 Who moves a session

- **Claude Code** itself: `status`, `statusUpdatedAt`, transcript writes, hooks `SessionStart`, `Stop`,
  `StopFailure` (plugin `hooks.json`; launch homes add start/turn/limit hooks, `launches.rs:904-974`).
  No `SessionEnd`, `PreToolUse` or `PostToolUse` hook is installed.
- **Duty officer** (`duty::tick` from the watch thread, at most every 300 s, at most 3 acts):
  closes `idle` and `duplicate`, restarts `stuck` (`duty.rs:541-603`).
- **Session watch** (`session_watch::tick`, once a minute on its own thread): calls the duty officer
  first, then writes findings `duplicate`, `stuck`, `gone` (`session_watch.rs:354-413`). Acts on nothing
  itself.
- **Session host** (`switchboard session-host`, one per hosted session; every Terminal launch since
  H-1b goes through it, `launch.rs:765-772`, `:705-727`): ends a session after 30 min idle with no
  viewer, or on a viewer's `END` (`session_host.rs:287-304`, `:697-718`).
- **Dispatch sweep** (owner's 30 s pass): stops a run past its deadline (`dispatch.rs:534-566`).
- **Recovery host** (Fabric Dashboards, outside this repo): reads `recovery`, claims, relaunches, reports
  `finish`. Switchboard "does not ... restart a client" (CLI.md, Capacity recovery).
- **Automatic fallback** (SB-73): on a limit, hands an Observatory workflow to a NEW session with a
  continuation prompt; the old session is never stopped (`fallback.rs:1-8`, `continuation.rs:607-630`).
- **Person**: `close_copy` (`duty.rs:731-762`), `sessions end` (`session_host::end`), SB-395 `attach`,
  `end` (`lib.rs` `SessionAttach`, `SessionEnd`).

---

## 2. Monitoring mode

### 2.1 How the code tells monitoring from finished

It does not, except by two proxies:

1. **Own commands**: an `idle` session with a direct-child shell whose command line contains
   `/.claude/shell-snapshots/` counts as working (`duty.rs:181-183`, `:353`). This covers a background
   Bash (`run_in_background`) whose shell stays alive. It misses a watcher the command detached
   (`nohup ... &`, `setsid`, `disown`: the shell exits, the watcher is reparented to launchd, `ppid != root`)
   and any tool that spawns its process without the snapshot path. Whether Claude Code's `Monitor` tool
   spawns through a snapshot shell is **not verified** by any test or measurement in the repo.
2. **Wall-clock floor**: an `idle` session is closed only after `WAKE_UP_FLOOR` = 3600 + 300 s since
   `statusUpdatedAt` (`duty.rs:39-42`, `:182`). This assumes the wake-up fires within 60 min of the turn
   ending, measured on the same clock.

`CronCreate` is named as a known limit, not handled (`CONTRACTS.md:3338-3340`, plan README:73-74).
`waitingFor` is written by Claude Code and ignored.

### 2.2 Every way a monitoring session can still be closed

| # | Path | Condition | Protects monitoring? | Evidence |
|---|---|---|---|---|
| K1 | duty `idle` | idle >= 65 min, no own command | `ScheduleWakeup` <= 60 min only, on wall clock; not a cron gap > 65 min, not a detached watcher | `duty.rs:181-186` |
| K2 | duty `idle` when `ps` fails | `process_table()` returns None, `commands` stays 0 for everyone | **no**: every idle > 65 min session is closed, background commands or not | `duty.rs:235`, `:246-251`, `:310-326`; comment at `:310-311` claims the opposite |
| K3 | duty `duplicate` | a copy idle with no command while another copy exists | **no floor at all**: a copy waiting on its own `ScheduleWakeup` is closed at the next check; when all copies are idle the oldest stays, even if the newer one holds the wake-up | `duty.rs:392-414`, `:172-174` |
| K4 | duty `stuck` | busy/shell, no transcript write 45 min, < 2 s CPU, no own command | a session blocked in a long MCP tool call (MCP servers are not "commands") reads stalled and is restarted | `duty.rs:187-196`, `:9-11` |
| K5 | max age | removed in 0.7.96 (`max_age_hours` unused) | yes | `duty.rs:62-66`, DEC-0021 amended 0.7.96 |
| K6 | session host idle close | hosted, no viewer, no PTY output or input for 30 min | **no**: 30 min is below the 65-min floor; a session whose window was closed while it waits on a wake-up dies at 30 min (assuming its TUI writes nothing while idle: not measured) | `session_host.rs:51-53`, `:444-458`; `hosted.rs:160-176` passes no `--idle-minutes`, so the default 30 of `main.rs:379-382` applies |
| K7 | dispatch timeout | `claude -p` run past `timeout_minutes` (default 60), wall clock, regardless of activity | no | `dispatch.rs:556-566` |
| K8 | capacity cap | refuses new starts only; quiet sessions do not count | yes, kills nothing | `capacity.rs:1-8` |
| K9 | Fabric Dashboards LC-18 idle close | outside this repo; Dashboards consoles are skipped by the duty officer (`host == "dashboards"`) and Dashboards owns their idle close | not in this repo; interface: `duty::host_of` `duty.rs:145-158`, `decide` `:169`; the session host copies LC-18 (`session_host.rs:51-53`) | |
| K10 | duty `idle` on a session stopped on a limit | a session whose turn ended on `StopFailure` reads idle; the duty officer never reads `recovery.json` | **no**: a limit-stopped session in Terminal/Orca is closed after 65 min, its stop stays `waiting` until the 24 h horizon cancels it | `duty.rs:168-200` (no recovery input); `recovery.rs:120-128` |
| K11 | sleep and wake | wall-clock `now - statusUpdatedAt` includes the time the Mac slept; a wake-up timer in the session very likely runs on a monotonic clock that does not advance in sleep | **risk, unverified**: after a long sleep the duty check can run before the session's own timer fires | `duty.rs:175`, `:546` |

### 2.3 Signal needed

Nothing Claude Code writes today names a pending wake-up (plan README:64-65). Options, in order of
cost:

1. **Switchboard plugin hooks, no contract change.** Add `PreToolUse`/`PostToolUse` hooks with matcher
   `ScheduleWakeup|CronCreate|CronDelete|Monitor` to `plugins/switchboard/hooks/hooks.json` (launch
   homes already carry `plugins` and `hooks`, `carry.rs:11-19`). The hook writes
   `<data>/monitors/<session>.json`: `{session, config, cwd, kind, set_at, until | schedule, tool_use_id}`.
   `until` = `set_at + delaySeconds` for a wake-up, the `timeout` for a `Monitor`, null with the cron
   expression for `CronCreate` (a next fire can be computed). This is an honest end the duty officer and
   the session host can read. A `SessionEnd` hook in the same file records every end with Claude Code's
   own reason and the config folder, which closes K-gaps in section 3.
2. **Fabric agent contract change.** `fabric_heartbeat` already has `phase: waiting` with
   `waitingOn {kind: question | grant | continuation}`; a kind `wakeup` with `until` would let any
   agent declare a monitor across runtimes (Codex included). This is a change to fabric-agent-contract
   and needs a board request; it cannot be done in this repo alone.
3. **Upstream**: a `waitingFor` value (or a new field) for a scheduled wake-up in `sessions/<pid>.json`.
   Not ours; file with Anthropic only if 1 proves insufficient.

---

## 3. Resume of the same session

| Path | Records session id + folder + config for a later `--resume`? | Relaunches? | Context kept? | Evidence |
|---|---|---|---|---|
| duty `stuck` restart | in the script only | yes: `claude --resume <id> <CONTINUE_PROMPT>` in the session's own `CLAUDE_CONFIG_DIR`, folder, home exports minus `SWITCHBOARD_LAUNCH_ID` | **yes** (same id, same config so the transcript is found) | `duty.rs:419-475`, `:606-642` |
| duty `idle` close | `duty.jsonl`: `session`, `folder`, `pid`, `host`; **no config folder, no launch id, no account** | never | the transcript stays on disk, but for a session in a Switchboard home (`homes/<acct>`, `runtimes/claude-<pool>`, `shell/*`) the journal does not say which config folder holds it, so `cd folder && claude --resume <id>` from `~/.claude` finds nothing | `duty.rs:587-590` |
| duty `duplicate` close | same journal line | never needed (another copy keeps the id) but a wake-up set in the closed copy is lost | partly | `duty.rs:392-414` |
| host `idle` close | launch record: `provider_session_id`, `transcript`, `ended {kind: idle}` | never | data is there; no surface offers it | `launches.rs:962-965`, `lib.rs:5913-5928` |
| person `sessions end` | recorded as `exited` (reason `ended` folded into `exited`) | never | n/a | `lib.rs:5921-5924`, `session_host.rs:504-507` |
| limit stop | `recovery.json`: session, account, pool, directory; status adds `launch_id`, `transcript`, `live` | **only by a host** (Fabric Dashboards): `claim` then the host's own `--resume`; Switchboard starts nothing | same id if the host resumes with `--resume` in a config folder that holds the transcript | `recovery.rs:5`, `:595-621`, `:687-741`; CLI.md Capacity recovery |
| limit stop, resumed on another account | the claim may pick any candidate account (`candidates`, `probe_accounts`) | host | **at risk**: transcripts are per config folder and `projects` is not carried into homes (`carry.rs:11-19`); resuming a session from `homes/<A>` in `homes/<B>` (isolated mode) or from `~/.claude` in `runtimes/claude-<pool>` will not find the transcript, so `--resume` fails or the host starts fresh | `recovery.rs:407-440`, `:502-526` |
| limit stop older than 24 h | cancelled by `prune` | never | **lost**: a weekly limit reset is beyond the horizon | `recovery.rs:22-24`, `:120-128` |
| automatic fallback (SB-73) | Observatory checkpoint, not the session | yes, a **new** session with a continuation prompt | **no** (new context, checkpoint only); old session left running | `fallback.rs:1-8`; `continuation.rs:607-630` |
| dispatch `timed_out` | launch record | never | n/a | `dispatch.rs:556-566` |

The only code path that relaunches with `--resume` inside Switchboard is `duty::act_on` for `stuck`
(`duty.rs:620-641`). `launch --in-place -- --resume <id>` exists for hosts (`launch.rs:2775-2880`,
`extra_valid` `:2826`). Context can be lost by: K10 followed by no host, the 24 h horizon, the account
switch across config folders, the automatic fallback, and every idle close (no surface ever offers the
resume). The SB-395 Sessions page cannot offer it either: ended rows carry no `session_id`
(`session_watch.rs:181-186` omits `provider_session_id`; `session_rows.rs:129-139`).

---

## 4. Defects and risks

| ID | Sev | Where | What | Why it matters |
|---|---|---|---|---|
| D1 | P1 | `duty.rs:235`, `:246-251`, `:310-326`, `:181-183`, `:397` | When `ps` cannot be read, `commands` stays 0 for every session, so every idle > 65 min session and every idle copy is closed although background commands run. The comment at `:310-311` says the opposite | The operator's rule "never before it finished" breaks exactly under load (fork failure with dozens of sessions). Fix: `observe` returns `commands: Option`, `decide` and `newer_copies` act on `idle` only with `Some(0)` |
| D2 | P1 | `duty.rs:168-200` | The duty officer never reads `recovery.json`: a session stopped on a limit reads `idle` and is closed after 65 min; nothing outside Dashboards resumes it, and its stop is cancelled at 24 h | Directly contradicts "limit stops come back as the same session". Fix: skip a session with a `waiting`/`claimed` stop (`recovery::session_has_live_stop`, `recovery.rs:678-685`, already lock-free) |
| D3 | P1 | `session_host.rs:51-53`, `:444-458`; `hosted.rs:160-176`; `main.rs:379-382` | Every hosted session (all Switchboard Terminal launches since H-1b) is ended after 30 min with no viewer and no PTY traffic, below the 65-min wake-up floor and with no own-command or recovery check | A monitoring session whose window the person closed dies at 30 min. Its end is recorded (`idle`) but never resumed. Needs a measurement of PTY output while idle |
| D4 | P2 | `duty.rs:392-414`, `:172-174` | `newer_copies` closes a finished copy with no time floor; when all copies are idle the oldest stays even if the newer one holds a pending wake-up | Loses a monitor loop; the name `newer_copies`/`newer_copy` no longer says what it computes |
| D5 | P2 | `duty.rs:587-590` | The idle-close journal line has no `config` (the session's `CLAUDE_CONFIG_DIR`), no launch id, no account | A later resume of the same session cannot be built for any session in a Switchboard home |
| D6 | P2 | `recovery.rs:22-24`, `:120-128` | Waiting or claimed stops are cancelled after 24 h (Dashboards' horizon) | A weekly-limit stop never resumes; the session is forgotten |
| D7 | P2 | `recovery.rs:595-621`, `:715-732` | `claim` does not refuse while the session is still live in a process; `status` only reports `live` and trusts the host | SB-376 recorded two processes on one worktree; the guard belongs at the claim |
| D8 | P2 | `carry.rs:11-19`; `recovery.rs:407-440`, `:502-526` | Transcripts are per config folder and not carried; a claim can pick an account whose home does not hold the transcript | `--resume` on another account in isolated mode cannot find the session: context lost or a fresh session |
| D9 | P2 | `continuation.rs:607-630`; `fallback.rs:1-8` | The automatic fallback continues a limited workflow as a new session with a checkpoint prompt, never `--resume`, even Claude to Claude | The operator asked for the same session, not a new one |
| D10 | P2 | `session_rows.rs:81-120` with `:143-167` | A limit-stopped session still alive in its terminal appears twice: a live row (`running`) and a stop row (`waiting` or `needs_you`); `recovery.status` gives `live` and the builder ignores it | Two rows for one session; the person cannot tell which to act on |
| D11 | P2 | `launches.rs:962-965`, `:819-826` vs `:424-449`; `session_rows.rs:127`, `:136` | `ended` has two shapes: an object `{kind, at}` and the strings `"limit"`, `"not_started"`. Readers of `ended.kind` (`session_rows`, `dispatch::status` `dispatch.rs:494-499`) read the strings as plain "ended" with no time; `end_once` refuses a later real exit because `ended` is already set | Sessions page says "ended" for a limit stop; a session that resumed by itself after a limit stays "limit" forever |
| D12 | P2 | `session_rows.rs:122-141`, `session_watch.rs:181-186` | Ended and gone rows carry no `session_id`/`transcript`/config and no act; `ready_to_continue` rows carry no `continue` act; claimed stops vanish (`:147`) | The page cannot offer resume-same-session, which is the operator's core ask |
| D13 | P2 | `duty.rs:187-196` | `stuck` treats a session blocked in a long MCP tool call as stalled (MCP servers are not commands; an idle network wait uses no CPU) | A legitimate long tool call (research, media orders) is killed and restarted at 45 min |
| D14 | P2 (risk) | `duty.rs:175`, `:546` | Wall clock vs the session's timer across sleep (K11) | Possible close just before a wake-up after the Mac wakes; measure before relying on the floor |
| D15 | P2 | `duty.rs:606-642`, `session_watch.rs:120-126` | A duty close records no `ended` on the launch: a plain (non-hosted) launch reads `gone` in Needs attention for a day; a hosted one reads `exited` 143. A duty restart starts a session with no launch record (`SWITCHBOARD_LAUNCH_ID` dropped, `duty.rs:436-440`), so it is unmanaged, unhosted and unattachable | The view lies about why a session ended and loses the resumed one |
| D16 | P3 | `lib.rs:5921-5924` | A person's `END` (reason `ended`) is recorded as `exited` | SL-4 promises `ended` by a person (plan 2026-11-13, SL-4) |
| D17 | P3 | `duty.rs:48-50` | Orphan doc comment of a removed constant fused onto `CONTINUE_PROMPT` | Doc drift |
| D18 | P3 | `session_watch.rs:10-11` vs `:386-388` | Module doc says "Nothing here stops or starts a session"; `watch` calls `duty::tick`, which does | Doc drift |
| D19 | P3 | `duty.rs:449`, `:162`, `:195` | `resume_script` takes an unused `root` (`let _ = root`); `Act::Restart { prompt }` is always `true` | Dead parameters |
| D20 | P3 | `duty.rs:785-801`, `:565` | `stop` sleeps up to 10.5 s per act on the watch thread; with `RUNNING` set the one-minute watch is skipped meanwhile (up to ~32 s) | Delays findings; acceptable but undocumented |
| D21 | P3 | `duty.rs:80-86`, CONTRACTS:3355 | `waiting_minutes`, `max_age_hours` still validated and shown; journal reasons `old`, `waiting` still listed in the contract | Settings and docs describe rules that no longer exist |
| D22 | P3 | `session_watch.rs:109-118`; `session_rows.rs:100-104`; `machine_sessions.rs:365-374` | `quiet` and the watch's `stuck` are folder-level: another session writing in the same folder hides a stall, two sessions in one folder share one value | Misleading row state |
| D23 | P3 | `machine_sessions.rs:90` | Documents statuses `busy`, `idle` only; code uses `shell`, `waiting` too | Doc drift |

---

## 5. Quality review per function

### duty.rs

| Function | Naming | Tests | Missing edge cases / drift |
|---|---|---|---|
| `Settings`, `valid`, `settings`, `save` (`:57-113`) | fine; two dead fields | `settings_default_on_reject_out_of_range_and_round_trip` | unused fields still validated (D21) |
| `host_of` (`:145-158`) | fine | `the_host_is_read_from_the_ancestors_and_dashboards_wins` | iTerm untested; walk depth 8 may stop before the host (`machine_sessions.rs:244`) |
| `decide` (`:168-200`) | `newer_copy` parameter means "finished copy" | `only_a_finished_or_a_stalled_session_is_acted_on` (thorough) | no recovery input (D2); `commands` unknown vs 0 (D1); long MCP call (D13); clock moved back (`since` negative) is safe but untested |
| `observe` (`:203-254`) | fine | none directly | `ps` failure path (D1); a session file whose `cwd` was later changed by `cd` |
| `parse_table`, `clock_seconds` (`:266-308`) | fine | `the_process_table_gives_a_sessions_commands_and_its_trees_cpu` | fractional `etime` parts; locale already forced |
| `tree_of` (`:334-359`) | fine | same test | detached watchers (ppid 1) not counted; `Monitor` tool spawn path unverified |
| `compare_cpu` (`:368-386`) | fine | `cpu_is_compared_across_checks_and_a_status_read_keeps_no_sample` | pid reuse between checks (low) |
| `newer_copies` (`:392-414`) | misleading name | `only_a_finished_copy_of_a_session_is_closed` | no floor (D4) |
| `resume_script` (`:419-475`) | fine; unused `root` | `a_resume_script_quotes_every_word_and_keeps_a_homes_exports` | no launch id, so not hosted, not recorded (D15) |
| `journal`, `append`, `restarts` (`:477-503`) | fine | `a_session_restarted_three_times_in_six_hours_is_counted` | read-modify-write without a lock (owner and CLI `duty run` can race; low) |
| `status` (`:506-539`) | fine | none | no test of its shape |
| `tick`, `check` (`:544-603`) | fine | none (`check` is untested end to end) | no injectable observer, so the act loop and `MAX_ACTIONS` are untested |
| `act_on` (`:606-642`) | fine | none | restart outcome branches untested |
| `quiet_copy`, `older_copy`, `suggestion`, `copies`, `chosen_copy`, `close_copy` (`:657-783`) | fine | `a_person_closes_the_copy_quiet_longest_and_never_a_working_one`, `when_both_copies_work_the_one_waiting_longest_on_its_command_is_offered` | `chosen_copy` lets a person close a `waiting` copy (by design, warn in UI) |
| `stop` (`:787-801`) | fine | none | blocks the watch thread (D20); kills only the pid, not its group |

### session_watch.rs

| Function | Tests | Notes |
|---|---|---|
| `findings` (`:74-131`) | `a_copy_of_a_live_session_a_quiet_board_run_and_a_vanished_launch_are_found` | `stuck` only for dispatched runs and folder-level (D22); a duty-closed launch becomes `gone` (D15) |
| `assemble` (`:135-211`) | `the_tree_joins_live_sessions_to_their_launches_and_counts_them` | launches drop `provider_session_id`, `transcript` (D12) |
| `read_cached`, `usable`, `invalidate`, `read_fresh` (`:222-252`) | `a_kept_read_answers_only_the_same_folder_for_fifteen_seconds` | fine |
| `tree`, `hook_state`, `context` (`:263-315`) | `the_context_block_counts_an_organizations_sessions_and_who_launched_them` | fine |
| `last`, `forget` (`:318-346`) | covered in the findings test | fine |
| `tick`, `watch`, `describe_duplicates`, `key`, `last_keys` (`:354-444`) | none (`tick` returns early under `cfg!(test)`) | log-once logic and the duty call untested; module doc drift (D18) |

### recovery.rs (readiness, claim, status)

| Function | Tests | Notes |
|---|---|---|
| `readiness` (`:542-591`) | `a_stop_without_capacity_is_probed_on_the_backoff_not_at_the_reset`, `a_billing_stop_is_never_probed`, `a_stop_follows_its_account_into_another_group_and_says_why_it_waits`, `an_account_near_its_limit_is_not_capacity_for_a_stopped_session`, `restrictions_hidden_holds_and_stale_observations_win_over_percentages`, `changed_project_permissions_and_duplicate_identity_refuse_capacity` | computes `candidates` before the state check (harmless); does not consider whether the target account's config folder holds the transcript (D8) |
| `claim` (`:595-621`) | `a_notification_is_claimed_once_and_never_replayed_after_restart`, `a_probe_claim_is_checked_by_the_same_rule_and_taken_once`, `simultaneous_host_claims_are_serialized_by_the_owner` | no `live` refusal (D7) |
| `status` (`:687-741`) | `an_open_stop_names_its_launch_and_its_transcript_in_a_managed_home`, `a_session_is_live_only_while_its_pid_is_a_claude_code_process` (machine_sessions) | no config folder for the transcript's home beyond the path |
| `register_stop`, `continued`, `finish`, `prune` | `repeated_stops_of_one_session_raise_the_attempt`, `a_stop_the_session_resumed_by_itself_does_not_raise_the_next_attempt`, `a_session_that_goes_on_by_itself_settles_its_stop`, `a_stop_past_the_hosts_horizon_reads_and_is_written_as_cancelled`, `a_host_finishing_a_stop_settled_meanwhile_is_answered`, retention tests | horizon (D6) |

### session_rows.rs (SB-395, work in progress)

| Function | Tests | Notes |
|---|---|---|
| `build` (`:59-187`) | `every_session_lands_in_one_group_with_the_acts_it_has`, `a_recovery_that_could_not_be_read_is_named_not_guessed` | live and stop rows not merged by `session_id` (D10); `ended` string shapes (D11); no `continue` act, no `session_id` on ended rows, claimed stops dropped (D12); no `monitoring` why; duty-closed sessions never appear |
| `there`, `attach_script` (`:191-226`) | `another_machines_answer_reads_as_its_heading_says` | `there` classifies by English error text (fragile across versions, P3) |
| `agent_of` (`:40-47`, uncommitted) | asserted in the main test | fine |

Untested combinations worth a fixture: a live session with a waiting stop; a launch with `ended: "limit"`;
`ended: "not_started"`; a claimed stop; a tree with `degraded` process table.

---

## 6. Refactoring plan

Each step is one PR, reversible, behind the existing gate (`./scripts/check.sh`), with docs in the same
change (CONTRACTS duty section, AGENTS Lifecycle rows, DEC-0021 amendment, SCN-096).

1. **Fail safe on an unread process table (D1).** `Seen.commands: Option<usize>`; `decide` closes `idle`
   and `newer_copies` counts a copy finished only with `Some(0)`. Test: a `Seen` with `commands: None`
   and idle 120 min is never closed.
2. **Never close a session with a live limit stop (D2).** `decide` takes a `HashSet<session>` from
   `recovery::current` (waiting or claimed); such a session is left with reason `limit_stop` in `due`
   for the view. Test in duty and a fixture in `session_rows`.
3. **One floor for every idle killer (D3, D4).** A shared `lifecycle::idle_floor()` (65 min) used by
   `decide`, `newer_copies` (a copy must be idle past the floor) and the session host (`DEFAULT_IDLE`
   raised to the floor, and the host skips closing while its session has a live stop or a monitor).
4. **Normalise `ended` (D11, D16).** Write `{kind: "limit", at}` and `{kind: "not_started", at}`; readers
   accept both shapes for 90 days; `end_once` lets a real exit replace a `limit` end (keep the limit in
   `stopped_on_limit_at`); `SessionEnded` keeps `ended` as its own kind.
5. **Resume ledger (D5, D12, D15).** `<data>/resumable.jsonl` (0600, ids and paths only): one line per
   end Switchboard caused or saw, with `{session, config, cwd, launch_id?, account?, pool?, reason, at,
   monitor?}`. Written by duty close/restart, host idle/end, dispatch timeout and the new `SessionEnd`
   hook. Duty records `ended` on the launch it closed.
6. **Monitor signal (section 2.3, option 1).** Plugin `PreToolUse`/`PostToolUse` hooks for
   `ScheduleWakeup|CronCreate|CronDelete|Monitor` and `SessionEnd`, a `switchboard monitor hook` command
   (bounded like the other hooks: 1.5 s, silent, `|| true`) writing `<data>/monitors/<session>.json`.
   Read-only first: the view shows `monitoring until T`; no killer changes yet.
7. **Monitoring state with an honest end (spec below).** Duty and session host read the monitor file:
   a session `monitoring` with `until` in the future is never closed; `until` passed by more than the
   grace is `idle` again; an open-ended monitor may be closed after the operator's limit and gets a
   `revive` entry.
8. **Resume-same-session act (spec below).** `Operation::SessionContinue {session_id}` builds the
   resume from the ledger through the existing `launch_here` with `extra = ["--resume", id]` in the
   session's own config folder; refused while `live`.
9. **Recovery guards (D6, D7, D8).** `claim` refuses while `live`; candidates prefer accounts whose
   config folder holds the transcript, or the launch copies the transcript file into the target home's
   `projects/<folder>/` before `--resume` (one file, 0600, never a credential); the horizon becomes
   per-kind (keep 24 h for probes, keep a capacity wait until the provider's reset plus a day, max 8 days).
10. **Fallback prefers resume (D9).** When the chain's next step is the same provider and the executor's
    session id and transcript are known, continue with `--resume <id>` plus the continuation prompt
    instead of a fresh session; the fresh session stays for cross-agent steps.
11. **Sessions page (SB-395) on the new data (spec below).**
12. **Cleanups (D13, D17 to D23).** Rename `newer_copies` to `finished_copies`; drop the dead
    parameters; fix module docs; per-session quiet for the watch's `stuck`; long MCP call allowance
    (status `busy` with an MCP server child holding an open request counts as working when the transcript
    shows a pending `tool_use`, or raise `stuck` for `busy` with no shell to 3x).

### Spec: monitoring state with an honest end

- **MON-1** A session is `monitoring` when a monitor record exists for its session id and (a) a wake-up
  `until` is in the future, (b) a `Monitor` tool's process or timeout has not ended, (c) a cron job is
  registered and not deleted, or (d) one of its own commands runs (today's rule).
- **MON-2** `monitoring` with a known end is never closed by the duty officer, the session host, or a
  duplicate close, until `until + 10 min` (sleep-safe: `until` is compared after adding the wall time the
  machine slept, read from the host's sleep record or by comparing monotonic and wall clocks).
- **MON-3** `monitoring` with no known end (cron, an unbounded `Monitor`) may be closed only after the
  person's `open_monitor_hours` (default 6, operator to confirm) with no turn; the close writes a
  `resumable` line with `reason: monitor_parked` and, for a cron, the expression and the next fire.
- **MON-4** A parked session is revived as the same session (`--resume <id>` in its config folder) at
  the cron's next fire, or on the person's *Continue*; the revive prompt names what Switchboard closed
  and asks the session to recreate its schedule.
- **MON-5** The duty status, Sessions rows and `switchboard duty` name `monitoring` sessions with their
  `until` or `open`, and `waiting_for_person` stays separate.

### Spec: resume the same session

- **RES-1** Every end Switchboard causes or observes writes a `resumable` line (step 5).
- **RES-2** `SessionContinue {session_id, account?}`: refused when a live process holds the id
  (`machine_sessions::live_sessions`); refused when the transcript is gone; else launches
  `claude --resume <id>` in the recorded `cwd` with `CLAUDE_CONFIG_DIR` = recorded `config` (or copies the
  transcript into the chosen account's home first), through a session host, with a new launch record
  whose `continued_from` names the old launch and `provider_session_id` the same id.
- **RES-3** A limit stop that is ready is continued by Switchboard itself when no host claimed it within
  N minutes and the person turned on *continue stopped sessions here* (off by default; the claim/finish
  protocol is used so Dashboards and Switchboard never both resume).
- **RES-4** Never a fresh session where a resume is possible; the fallback uses a fresh one only across
  agents or when the transcript is unavailable, and says so in the launch record.

### Spec: the Sessions page (SB-395)

- **SP-1** Groups in order: **needs you** (duplicate, stuck, gone, ready to continue, monitor overdue),
  **running** (working, quiet), **waiting** (monitoring until T, monitoring open, limit with `probe_at`),
  **ended** (with kind: idle, exited, timed out, closed by duty, parked monitor, limit).
- **SP-2** One row per session id: a live process and its stop or monitor merge into one row (the live
  row carries `stop` and `monitor` blocks).
- **SP-3** Acts per row: `close_copy` (duplicate, the offered pid), `open_here` (attach a hosted session,
  here or `--target`), `end` (hosted), `continue` (ready stop, parked or ended session with a transcript:
  `SessionContinue`), `dismiss` (gone). Each act exists as an MCP tool (DEC-0019).
- **SP-4** Rows carry `session_id`, `config` (relative to the data folder, or `person`), `launch_id`,
  account label, group, `since`, `until`; never text a session wrote.
- **SP-5** Ended rows cover the last 7 days from the `resumable` ledger, not only the launch records of
  the last day.

---

## 7. Docs to update with the fixes

CONTRACTS "Session duty officer" (journal reasons, ps failure, recovery skip, floor shared), AGENTS
Lifecycle rows for the duty officer and the session host (idle 30 min vs 65), DEC-0021 amendment (new
DEC for monitoring), SCN-096 (its "never ended" claim is false today: K2, K3, K6, K10), SCN-095, the
SB-114 packet (claim refuses while live, horizon per kind), plan
`docs/plans/2026-11-16-sessions-and-keys/README.md` S-1 row (gap partly closed only).
