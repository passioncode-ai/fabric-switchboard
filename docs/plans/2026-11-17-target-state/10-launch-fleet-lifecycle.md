# 10. Launch, fleet and lifecycle (operator's order, 2026-11-19)

## The order

«давай еще аудит как атрибуция происходит, чтобы правильно проекты к папкам матчить, дальше делаем
фичу, чтобы подписки пользовательские, которые на машинах организации, конвертнулись в наш борд
администратора подписок … проверить лайфсайкл, как работает, и вотчер за ним, и скрипты, чтобы в
случае отказа срабатывать и не подвисали сессии, перезапуски и продолжения, запуски всегда с полным
доступом по дефолту, если не ограничен намеренно, проверить, что все настраивается через MCP и нет
багов, важно, чтобы сессии не пропадали, не путались, не терялись, и важно обновления чекать,
автосвоп, чтобы грамотно работал и не ддосить клауд, и проверять по таймеру, когда надо, или по
запросу, и продумать какой-то смарт еще более режим, + хочу видеть кнопку запустить агента, выбрать
папку / агента / модель / оплату токен или подписка / может надо выпустить новый токен через
доступные ключи или взять текущий, минтер ключей … и менеджмент сессий с возможностью удаленного
менеджмента, например с макбука менеджить сессию на студии, запускать, ставить на паузу,
перезапускать, взаимодействовать», then «и дальше реализовать, волнами до конца, пока не будет все
готово, выпустить апдейт».

Six read-only audits on 2026-11-19 at origin/main `b6d33448` (0.7.120). Each claim below carries its
evidence; line numbers are at that commit.

## A. Which group a folder belongs to (attribution)

**Now.** Every place compares by path components (`Path::starts_with`), so `/a` never matches `/ab`.
Beyond that they differ:

| Where | Rule | Canonical? |
|---|---|---|
| `Snapshot::project_for` (`projects.rs:668-681`) | prefix; personal groups first; groups switched off last; first group with an enabled account of its own, any provider | no |
| `bindings::desired` (`bindings.rs:204-295`, writes `settings.local.json`) | personal first; first group with an enabled Claude account, shared ones counted | group folders no, worktrees yes |
| `account_for_folder` (`projects.rs:521-534`) | `project_for`, then the default group per provider | no |
| Launch `serving_pool` (`launch.rs:1769-1788`) | the chosen account's pool, else its first shared pool holding the folder | cwd yes |
| Shell map (`shell/map.rs:135-158`) | `project_for` for the row, longest prefix for the lookup | yes |
| Proxy statistics and the SB-452 check (`proxy/lib.rs:899-931`, `stats.rs:93-129`) | the folder capability, else a folder hint read from the request body | no |
| Joining a folder (`suggest.rs:302-320`) | DEC-0089 attribution; a tie is a conflict | git common dir |

**Gaps.**
- Two resolvers disagree for a folder in two groups: a group with only Codex or only shared Claude
  accounts is named by `project_for`, while `settings.local.json` routes to the other group (SB-511).
- The tie order is the group's **name**, not its creation, because every save sorts by name
  (`projects.rs:944`); the comments at `projects.rs:664` and `bindings.rs:292` say creation order.
  Renaming a group can move a shared folder (live: a folder in `agents` and `passioncode`). SB-511.
- Group folders are stored as typed (`save_project_with`, MCP `absolute_folders` check only that a
  path is absolute): a symlinked path, `/tmp` against `/private/tmp`, another letter case or a
  trailing slash never matches a canonical working folder (SB-512).
- 158 of 261 group folders are worktrees; nothing prunes merged ones, and saves fail at
  `MAX_ALL_FOLDERS = 1024` (`projects.rs:451`) (SB-513).
- The proxy's folder hint can be taken from message text and drifts when a Codex session changes
  folder (`stats.rs:93-121`), and it is computed twice (SB-483). "Project" names three things: a
  group (`pool`), a repository (statistics), and Fabric's own project on a dispatch, stored and
  never mapped. The chain scope `project:<pool>` looks like Observatory's `project:<slug>` (SB-514).

**Target.** One resolver `group_for(folder, provider)` used by bindings, launch, the proxy, the
shell map and the window; folders canonical on save and on lookup, with a one-time migration; the
tie broken by a stored order (creation, or the person's priority), never the name; worktree
folders leave their group when merged or removed, with the cap's distance shown; the folder hint
read only from the system prompt or environment block; chain scopes named `group:<pool>`, and a
dispatch records Fabric's project beside the group it resolved.

## B. Lifecycle, watchers and continuations

**Now.** Session states and their owners: a launch and its start hook (starting, running); the
capacity count (quiet after 30 minutes); hooks and `monitor hold` (waiting); the `StopFailure` hook
and recovery (limit stop); the duty officer every 5 minutes (idle close, stuck restart, at most 3
acts a check); the session host (hosted, no viewer and no output for 65 minutes); the session watch
(duplicate, gone); the dispatch pass (deadline).

**Measured (2026-11-17 to 2026-11-19, `duty.jsonl`).** 62 close lines were 40 closes and 22
duplicates; 0 failed restarts; 3 re-closes of a session the person took up again, all before
0.7.118 (SB-501); 0 idle closes since 0.7.118; 7 gone launches open, up to 19.7 hours old; 2
dispatched runs stopped at their deadline while still writing.

**Gaps.**
- **P0, the duty officer acted twice** (SB-505, PR #848): the intent was stamped `now + elapsed`
  and the outcome `now`, paired by `(session, at)`, so a check longer than a second left the act
  open and the next check finished it again: a second close line and Continue entry, a second
  resume window for a restart, restarts counted twice.
- Two continuations of one session can start: `continue_as` checks only for a live process
  (`resumable.rs:156`), and a Terminal takes seconds to write its session file. The `boot-resume`
  branch (PR #836) serialises them with a lock and a 120 s guard (SB-507).
- After a reboot or a crash, a session Switchboard never closed and never launched (one the person
  started in a terminal or an editor) cannot be continued: no resumable line, launch record or stop
  names it (`resumable.rs:105-130`). The duty officer's own closes do write resumable lines with the
  config and working folders (`duty.rs:776-779`), so the gap is the sessions nobody closed (SB-508).
- The session host closes a hosted session after 65 quiet minutes without the SB-500 checks (its
  own launches, subagent transcripts, running commands): `session_host.rs:618` (SB-506).
- A gone launch stays in Needs attention with no action; managed sessions keep the old proxy token
  after a port collision rotates it and get 401 until restarted; a dispatched run is stopped at 60
  minutes even while it writes (SB-509).
- Nothing outside the owner notices the owner itself stopping or an act left half done (SB-510).

**Target.** Every act journalled once (intent, act, outcome, one stamp); one continuation at a
time per session; a minute-by-minute ledger of every live Claude session (session, config folder,
working folder, host) so any session can be continued after a reboot or a crash; the host's idle
rule is the duty officer's rule; a gone launch with a transcript is offered for continuation once,
through admission; sessions on a rotated token restarted by a wave; a dispatched run extended
while its log grows. A small launchd job `switchboard watchdog --once` every 5 minutes, independent
of the owner, checks that the owner answers, that no intent is older than 10 minutes, that no gone
launch is older than 30 minutes and no duplicate older than 15; it reports through Needs attention,
an `agent-health/1` check and the operator's notification channel, and never acts itself. It is
Switchboard's first own launchd label (`ai.passioncode.fabric-switchboard.watchdog`): the same change
updates the Lifecycle table ("Launchd labels owned"), `switchboard uninstall` and the Linux and
Windows equivalents (a systemd user timer, a scheduled task).

## C. Full access by default

**Now.** The setting `permissions` in `agents-settings.json` is bypass when absent
(`agent_settings.rs:37-61`); `launch::bypass_args` (`launch.rs:3456-3498`) gives Claude Code
`--dangerously-skip-permissions` and Codex `approval_policy="never"` with `danger-full-access`,
and adds nothing when the caller named a mode. It reaches window and CLI launches, the fallback's
Claude Code and Codex hand-overs, local dispatch (its own `--permissions all`), the duty officer's
restarts, Continue and restart waves.

**Gaps (SB-515).**
- Kimi Code (`kimi.rs:795`, `:952`), Hermes (`launch.rs:2426`), OpenCode (`launch.rs:2577`) and
  catalog agents on a key (`launch.rs:2102-2128`) start in ask mode, while the fallback treats them
  as unattended (`fallback.rs:409`, `:906`): an unattended hand-over can wait on a prompt nobody
  answers.
- A remote dispatch records `--permissions` but never sends it (`lib.rs:5460`), and passes Codex
  `-p`, which Codex reads as `--profile` (`remote_dispatch.rs:157`).
- `SessionAccounts::resume_flags` in `fabric-sessions-core` is never called (`accounts.rs:40`).
- The window and MCP offer no per-launch restriction (SB-516, part of section E).

**Target.** One rule in one place: every agent Switchboard starts gets its own full-access flags
unless the launch (or the person's global setting) asks otherwise; a remote start sends the mode;
the launch dialog and `switchboard_run` carry a "restricted" switch.

## D. MCP: everything settable, no bugs

**Now.** 93 tools; a parity test covers CLI subcommands, not window buttons (`main.rs:6578-6892`).

**Gaps.**
- No MCP tool launches anything (`Not(LAUNCH)`, `main.rs:6718`), and `switchboard_launch_keys`
  points to `switchboard_launch_agent`, which does not exist (`mcp.rs:210`): section E closes it.
- Bugs (SB-517): read halves of several tools are hidden from a read-only server because a tool
  has one write flag (`capacity`, `duty`, `own_sign_in`, `app`, `analytics`, `balance`, `feed`,
  `switchboard_launch_keys`, and `switchboard_anthropic_keys`, flagged write at `mcp.rs:207`
  although its list half only reads); a write tool on a read-only server answers "Unknown tool." (`mcp.rs:1492`);
  schemas declare `additionalProperties:false`, enums and ranges, but nothing validates them;
  some refusals return as success (`remote_dispatch.rs:202`).
- Two doors (`search`, `execute`) instead of 93 tools: SB-478, with old names answering one release.
- Window actions without MCP: removing the OpenRouter key or a Kimi account (deletions, not secret
  entry), the language setting, connecting PassionCode.ai (sign-in in the browser stays the
  person's), board dispatch (SB-517).

## E. The Launch flow: folder, agent, model, billing, key, harness

**Now.** Three launch paths with different options: the account dialog (`launchDialog`,
`src/main.ts:4317`, Claude or Codex, Terminal only), the Agents page (`agentDialog`, `:2158`,
catalog agents on a key), Kimi rows (`kimiLaunchDialog`, `:2929`). The CLI has `launch` (account or
provider; hosted, headless, target; no model or key) and `agents launch` (group or key and model;
Terminal only). MCP has none. Key minting exists nowhere in Switchboard; Observatory mints
OpenRouter keys (`tools/openrouter.py issue`, value straight into the vault) and has no door for
Anthropic keys. A key launch first selects the key for the whole group (`keyWay === 'select'`),
which moves every other managed session of that group. A key launch needs the pay-per-token group
to hold the folder (`launch_keys.rs:11`). Saved launches ([07](07-saved-launches.md), SB-454) and
session profiles (SB-184) are designed, not built.

**Target (SB-520 to SB-522).** One **Launch** dialog (Sessions page, menu-bar icon, Agents row,
prefilled from a saved launch), one CLI verb and one MCP tool, all producing one operation:

1. **Folder** (required), with its group shown from the one resolver of section A and any other
   group holding it offered.
2. **Agent**: installed catalog agents, the default first.
3. **Billing**, by agent: only Claude Code and Codex run on a subscription sign-in (Claude Code on a
   Claude subscription, Codex on a ChatGPT one); every other agent runs on a key (the template's
   rule that an agent's capability never rides a subscription sign-in). The choices: the group's subscription route, one subscription account (isolated), the
   pay-per-token group's key, an OpenRouter key (the group's or the machine's), or a named
   Anthropic key (shown as unmetered until a ceiling counts it).
4. **Key**: reuse a saved key, take one Observatory holds (`switchboard_observatory_keys`), or
   **mint** an OpenRouter key: the account it comes from (personal projects from the personal
   account, agents from the Appvillis organization), the limit (default USD 100 a month), reset,
   expiry (90 days) and name; minted through Observatory's door with the value going straight into
   the vault, then added to the group through the existing pipe; the window's click is the
   operator's decision, an MCP call passes the operator's words.
5. **Model**, per billing kind.
6. **Harness**: a profile (default: everything the person carries today) with switches for MCP
   servers, plugins and full access.
7. **Run as**: hosted (default), Terminal or headless; on this machine or another one (section F).
8. **First message** (text or file), the fresh-branch step, parallel runs allowed or not.
9. **Money line**: the group's ceiling and today's spend before Run.

A plan step validates everything (the pay-per-token group and its ceiling, DEC-0023; keys by name
through Observatory, DEC-0024; the proxy's refusal of a crossing between a subscription group and
the pay-per-token group, SB-452; the folder outside the key's group with "add the folder to the
group" offered; unmetered spend, a reached ceiling, a live last
run offered for attach) and answers a digest; the run step runs exactly that plan. CLI:
`switchboard run --dir D --agent A --billing route|account:ID|tokens|openrouter[:pool]|anthropic:NAME
[--mint openrouter:limit=USD,reset=P] --model M --profile P --mode hosted|terminal|headless
[--target MACHINE] [--prompt-file F] [--save NAME] [--plan]`; MCP `switchboard_run` (plan, run);
both `launch` and `agents launch` become forms of it. A key launch never selects the key for the
whole group: it routes only its own session (its route file, SB-452).

## F. Remote session management

**Now.** From one machine over ssh to the operator's own user on another: list (`sessions --rows
--target`, and the window reads every machine), launch (`launch --target`), attach and type
(`attach --target` runs `ssh -t host switchboard attach <id>`; the window's *Open here*), end and
close a duplicate copy. Not remote: Continue (`lib.rs:460`; the window hides it,
`src/main.ts:3829`), restart waves, a session's output without attaching, one line typed without a
terminal (the session host's INPUT frame exists, `session_host.rs:39`, nothing sends it). No pause
exists anywhere. The remote session paths skip the machine proof SB-357 requires (`there()` goes
straight to `exec_ssh`).

**Target (SB-523 to SB-525).**
- Every session act takes `--target` (Continue, restart plan and apply, show, a new `sessions output
  <id> [--tail N]`, a new `sessions send <id> --text|--keys` through a short-lived viewer), each
  after the machine proof; the window's remote rows and MCP get the same acts.
- **Pause** holds the session at the proxy, not the process: requests of that launch get 503 with
  `Retry-After` until resumed, so the agent stays alive, tools keep running and nothing is spent;
  stored in `<data>/holds.json`, reachable remotely. An opt-in freeze (SIGSTOP) only for a hosted
  session, with SIGCONT on resume.
- **Restart one session** remotely: end, then continue as itself (`sessions restart <id> --target`),
  through the same continuation lock as section B.
- `sessions output` and `send` carry whatever a session printed or will read: output is redacted by
  the same scrubber as the logs before it leaves the machine, and over MCP it is opt-in per call.
- Pause by 503 reaches only proxied launches, and Claude Code gives up after its own retry budget:
  the pause is bounded (measured before it ships) and the window says when a paused session will
  stop; a longer pause is the freeze, or an end and a later Continue.
- On another person's device of an organization, a separate restricted grant (`ssh-serve
  --sessions --pool G`) for reads, output, end and attach of that group's launches only; typing
  behind its own switch and the device holder's consent on that device; output only when the
  holder allowed it; never launch or continue there.

## G. The organization's subscriptions board

**Now.** Organizations are labels (DEC-0022); machine groups hold the accounts of each other
machine, keyed by ssh identity (DEC-0013) and read every 15 minutes (DEC-0017); seats can be
delegated under a lease (DEC-0025); `org-seats/1` is exported (SB-381); `org-usage/1` is in PR #829.
Nothing marks a machine as an organization's, nothing turns an account into an organization's seat
with an owner, and the administrator sees only their own machines over ssh. The fleet view
(template section 26), the admin view (WF-12.2, SB-374) and devices of an organization
([02](02-groups-orgs-keys.md) capability 14) are designed, not built.

**Target (SB-526 to SB-528).**
1. **Custody** per account: `personal`, `org_seat` (the organization's, held on a device) or
   `guest` (DEC-0025); `machines[].organization` and its holder. Converting an account is one act
   (`switchboard accounts convert <id> --org O`, MCP and the window): `org assign` plus custody,
   recorded in `seats.jsonl` with who ordered it; the credential never moves.
2. **The board** (window page, MCP `switchboard_fleet`, CLI `fleet`): organization, then devices,
   then seats, with room, usage, health and custody, built first from the machine-group reads this
   machine already makes; stale after two read windows. Acts: move a seat between devices (recall
   and send as one journalled act), switch it off, call it back, remove a device (every seat called
   back, the device forgotten).
3. Invitations and `switchboard org join` writing the restricted key line itself; then the hub
   (SB-177) and access levels (SB-291) for several administrators: each machine publishes
   `org-fleet/1` (organization seats only, never a personal account or a credential).

## H. Automatic switching without hammering the provider

**Now (measured 2026-11-19, 32 enabled sign-ins in 5 groups).** One pass every 30 s checks at most
4 accounts (`monitor.rs:16-18`); since SB-310 every account in a group with switching on is
"active" at 180 s, so demand is about 640 checks an hour against a ceiling of 480: the queue is
always full, about 8 usage calls a minute from one address. A switch needs a figure no older than
300 s (`rotation.rs:609-631`), the real cadence is about 240 s plus stalls (`control_slow` up to
33 s), so most switches come after a refusal: 25 before the limit against 66 after it on
2026-11-18, 175 "no candidate" decisions. Response headers are read on every Claude answer
(`proxy/lib.rs:1065`, `:1144`) but never push the next check back (`core/lib.rs:2696-2704`); each
changed header rewrites `accounts.json` with fsync (1187 `metadata_write_slow`). A 429 holds one
account, never the provider (`usage_gate.rs:30`). An external sweep checks every account at :03
and :33; the window and CLI have no spacing (`runtime/lib.rs:1850`). Codex headers are not read.

**Target, the smart mode (SB-518, SB-519).**
- **Passive first.** A response's headers with both windows count as a check; while traffic flows
  the next active check is 180 s after the last header, and an account with no traffic falls to
  the idle cadence (600 s). Header observations are kept in memory and written at most once a
  minute.
- **The one move of a refused request (DEC-0002) is unchanged**: it uses the group's next usable
  account as today and asks for no check.
- **Active only when it decides something.** Before a switch, one fresh check of the chosen
  candidate (30 s budget); near the limit (95 %) only when the last header is older than 60 s; 30 s
  after a reset. Ranking accepts figures up to 900 s old; the switch itself needs the fresh one.
- **A budget per provider:** a token bucket of 2 checks a minute with a burst of 4 (about 120 an
  hour, a quarter of today), at most 2 in flight, ±15 % jitter on every schedule. A 429 on a usage
  check holds the whole provider for `max(Retry-After, 900 s)`, doubling on repeats up to an hour;
  900 s is today's floor per account (`usage_gate.rs:30`), kept because the provider answers
  `retry-after: 0` while it refuses every check.
- **People's checks** (window, CLI, MCP, ssh) are spaced 60 s per account and go through the same
  bucket; a sweep over all accounts takes its turn. The source of the :03/:33 sweep is found and
  routed through the bucket.
- **Across machines** one machine owns a seat's checks (the one holding it); others read its
  figures over the existing 15-minute read.
- The rotation log names the group; the event journal keeps switches apart from usage lines; Codex
  headers are read the same way.
- **A mode the person chooses** (CLI, MCP, window): `smart` (the default, above), `timer` (today's
  fixed cadence) or `on_demand` (only before a switch and when a person asks).
- **App updates** keep their cadence (DEC-0004, every 5 minutes, one small feed request): they never
  reach the provider.

## Waves (W10)

| Item | What | Rows | Order |
|---|---|---|---|
| W10.0 | Open PRs land: #838 PATH, #848 duty journal, #830 recovery filters, #836 boot resume, #829 org usage, #821 account off, #832 named users | SB-455, SB-505, SB-423, SB-450, SB-451, SB-418, SB-421 | first |
| W10.1 | Lifecycle: the duty journal (merged, #848), host idle rule, one continuation at a time, live-session ledger, gone and rotated-token continuations, dispatch extension, the watchdog job | SB-505 to SB-510, SB-529, SB-530 | 1 |
| W10.2 | Full access for every agent, remote mode sent, MCP bugs | SB-515, SB-517 | 2 |
| W10.3 | Smart usage checks and a provider budget | SB-518, SB-519 | 3 |
| W10.4 | Attribution: one resolver, canonical folders, stored tie order, worktree pruning | SB-511 to SB-514 | 4 |
| W10.5 | The Launch flow (`switchboard run`, `switchboard_run`, the dialog) with its restricted switch, key minting, harness profile; saved launches on top | SB-516, SB-520 to SB-522, SB-454 | 5 |
| W10.6 | Remote session acts and pause | SB-523, SB-524 | 6 |
| W10.7 | Custody, conversion and the subscriptions board; then invitations and the restricted session grant | SB-526, SB-527, SB-525, SB-528 | 7 |
| W10.8 | Two MCP doors | SB-478 | 8 |

Each item ends at the wave rules of the [README](README.md): the full gate on the Mac Studio, two
review rounds with no P0 or P1, a release.
