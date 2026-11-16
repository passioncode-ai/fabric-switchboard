# Sessions that never hang, keys to choose from, and every task of the 2026-11-13 to 11-16 session (2026-11-16)

One spec for the operator's request of 2026-11-16 11:35 UTC, closed through `task-pipeline` in
waves. Board row: [SB-392](../../evidence/backlog.md). Entry point for the next agent: this file,
then the wave table at the end.

## What was asked (the operator's words)

> Во-первых, перенести звуковой движок в проект Apps… Вот дальше, что мы делаем, это мы
> валидируем, что у нас качественно работают грамотно сессии, не заставиваются. Смотрим, давайте
> проводим анализ, делаем рефакторинг, посмотрим все задачи, которые я вот здесь давал, чтобы они
> все были доделаны, что не доделаны. помещаем в одну большую спеку, по таск-пайплайну все
> закрываем, волнами ходим до конца и выпускаем. Ключи Antropica и личные уже создались и лежат уже
> в обсерватории, но мы их пока не используем, мы используем open router… для гермеса да, мы
> используем ключи. Это выбор оператора, можно и так и так добавить, чтобы мы могли выбрать на каких
> ключах запускаться, какие ключи у нас есть и на чем мы будем запускаться.

## Every task the operator gave in this session, and where it stands

Session `4c635d5e`, 2026-11-13 23:21 to 2026-11-16. "Done" means released and running on this
MacBook unless the row says otherwise.

| # | When (UTC) | Asked | Outcome | Evidence | State |
|---|---|---|---|---|---|
| 1 | 11-13 23:21 | Restore the last work, audit, collect bugs, finish to the last release, check against the board and the North Star | Audit after 0.7.76; SB-358, SB-359, SB-360 | PR #679 `9ac68e26`, release 0.7.79 (#682) | done |
| 2 | 11-14 16:40 | Review everything, one spec in modules, waves through `task-pipeline`, test, ship | [Window and fleet](../2026-11-14-window-and-fleet/README.md), WF-1 to WF-12 (SB-363 to SB-374) | 0.7.80 to 0.7.85 | partly: W-rows below |
| 3 | 11-14 17:14 to 11-15 07:18 | Screenshots of pages: layout, Statistics width, organizations checklist, sessions lifecycle visible, bug tracker, latency, board | [Quality waves](../2026-11-15-quality-waves/README.md), SB-379 Q1 to Q5, W3, W4 | 0.7.88 to 0.7.99; UI audit closed (#749) | partly: W4 items 4 and 9 |
| 4 | 11-15 14:47 | Terminal windows the duty officer opened do not close; working sessions were closed | Scoped and fixed, PATH, window closes on a signal (SB-384, DEC-0021 amended) | PR #736, 0.7.95 | done |
| 5 | 11-15 17:03 | Sessions must not hang and must not be killed before they finish; close only a finished one or a diagnosed stall | Finished = idle with no commands; stalled = busy, still, no CPU, no command (SB-384) | PR #738, 0.7.96; journal below | done, one gap (S-1) |
| 6 | 11-16 01:48 | «так чего не дашь кнопку сразу закрыть» on a duplicate session | *Close the quiet copy* in Needs attention (SB-388) | PR #744, 0.7.98 | done |
| 7 | 11-16 02:01 | claude14@sshlg.me came back | SB-386 confirmed | PR #750 | done |
| 8 | 11-16 02:03 | A duplicate hangs in Needs attention (go-to-market-strategy); check lifecycle bugs | The older working copy offered by pid; `--headless` records its end (SB-389) | PR #751, 0.7.100 | done |
| 9 | 11-16 03:03 | dissy, playsign, clawdbot in the Apps & Soft group | They already were | `switchboard_project_context` | done |
| 10 | 11-16 03:11 | Hermes on Appvillis's or the operator's own Anthropic key, as an option | `agents anthropic`, `agents launch --anthropic` (SB-390) | PR #754, 0.7.101 | done; waits on a key (K-0) |
| 11 | 11-16 11:35 | The sound engine in the Apps group | `~/DATA/sound-engine` added to Apps & Soft | `switchboard_project_context` | done |
| 12 | 11-16 11:35 | Validate sessions, analyse, refactor, this spec, key choice | This plan | below | in progress |

Open from row 2 (window and fleet), each with its own module file and board row:

| Module | Left | Waits on |
|---|---|---|
| WF-5 Switching (SB-367) | 5.2e borrow at once on exhaustion; 5.2f C8 streams' drain time; 5.3 shares on Statistics | nothing: wave 2 |
| WF-9 Organizations (SB-371) | connect, read, bind, leave verified end to end; cloud link | the PassionCode.ai cloud (Vesta) contract for leave |
| WF-10 Agents (SB-372) | 10.2 templates, 10.3 shared skills and plugins | the agents database's template format (fabric-agents-database) |
| WF-12 Fleet (SB-374) | own machines page; organization admin; routing between machines | the estate hub and access levels (SB-177, SB-291) |

Open from row 3 (quality waves): W4 item 4 (`current_accounts` waits behind fsync'd writes under the
store lock) and item 9's remainder (the session tree recomputed per call).

Other open rows this session filed or touched, with their owner: SB-385 (usage data, the contract's
DEC-0157), SB-387 (the `task` of a Fabric console run, asked of Fabric), SB-391 (re-pin folder
attribution to the contract's DEC-0177), SB-350 (the architect's judged audit), SB-353 (the *stalled*
mark from a held claim, needs Fabric's claims), SB-383 (needs the contract's level field CT-303).

## What the live sessions show (this MacBook, 2026-11-16 12:20 UTC, 0.7.101)

- `switchboard --json attention`: `{"count":0}`. Nothing waits for the person.
- `duty.jsonl` from 2026-11-15 14:04 to 2026-11-16 09:30: 37 finished sessions closed (`idle`), 3
  duplicate copies closed, 7 restarts for age, all 7 before 0.7.95 removed that rule. No restart for
  a stall happened. Four session ids were closed twice: each was resumed in between by its owner
  (Orca or Terminal), and closed again only after another idle hour.
- Every Claude Code session file (`~/.claude/sessions/<pid>.json`, Claude Code 2.1.299) carries
  `status`, `statusUpdatedAt`, `waitingFor` and no field naming a scheduled wake-up.

**Gap S-1.** A session that set itself a wake-up (`ScheduleWakeup`, at most 3600 s, the loop the
global wait rule asks agents to use) reads `idle` while it waits. With `idle_minutes` at 60 (this
MacBook's saved value and the default), the duty officer can close it in the minute before it wakes
and the loop is lost: an end before the work finished, exactly what the operator ruled out. Fix: an
idle session is finished only after the longest wake-up Claude Code allows plus a margin (65
minutes), whatever `idle_minutes` says; the setting's floor and the help say so. A recurring
`CronCreate` job lives only inside its session and cannot be seen from outside; it is named in the
contract as a limit, not guessed.

## Keys: what we hold and what we run on

Measured 2026-11-16 (names only, no value read or printed):

- Observatory (this MacBook and the Mac Studio): `appvillis/prod/ANTHROPIC_ADMIN_KEY` and
  `sshlg-personal-os/prod/ANTHROPIC_ADMIN_KEY`, both Admin keys (`sk-ant-admin`): they manage an
  organization and cannot call a model. No Anthropic API key (`sk-ant-api`) is stored anywhere.
- The Admin API (`GET /v1/organizations/api_keys`, active): Appvillis has no API key; the personal
  organization has three (two Claude Code keys, `nicegram-ai-agent`), whose values are never
  returned. The Admin API cannot create a key.
- OpenRouter: the machine key and each group's own key in Switchboard (SB-213), and
  `*/prod/OPENROUTER_API_KEY` slots in Observatory. OpenRouter stays the default, as the operator
  said.

**K-0, a human gate.** For Hermes to run on Anthropic, a person creates an API key in Claude Console
(https://platform.claude.com/settings/keys) for each organization and saves it in Observatory
(`vault.py put appvillis prod ANTHROPIC_API_KEY < file`). Then one command saves it into
Switchboard without anyone seeing it:
`use_secret.py run --vault-only --env prod appvillis ANTHROPIC_API_KEY -- sh -c 'printf %s "$ANTHROPIC_API_KEY" | switchboard agents anthropic set appvillis --key-stdin'`.

**K-1, one list of keys.** `switchboard agents keys [--folder F]`, MCP `switchboard_agent_keys`
and the launch dialog read the same list: every key an agent can start on here, its kind, name,
default model, whether it is the one this folder resolves to, and why a key cannot be used.
OpenRouter's key for the folder (its group's own, else this computer's) comes first and is the
default; Anthropic keys follow for the agents that take them (Hermes).

**K-2, one choice in the launch dialog.** An agent's launch dialog shows *Run on* with that list,
names the key a launch will spend (group or machine, not just "the OpenRouter key"), and starts
with the chosen one. No new way to launch: the same `launch_on_openrouter` and
`launch_on_anthropic`.

**K-3, an Admin key says what it is.** Saving an `sk-ant-admin` key is refused with its own words
(an Admin key manages an organization and cannot run a model; create an API key in Claude
Console) instead of the generic "not an Anthropic API key".

## Analysis and refactoring

- The launch dialog builds two independent key sections (`openrouterLaunch`, `anthropicLaunch`),
  each with its own button; K-2 makes it one picker and one button. Refactor in the same change.
- Branches: 64 remote branches; 45 belong to merged pull requests (`gh pr list`), 19 have none
  (`w4/*`, `w5/*` of three weeks ago, `release/0.7.3`, `sb-283-handover`). Merged ones are deleted;
  the others are archived under `refs/archive/2026-11-16/` first, then deleted (global cleanup rule).
- Backlog rows whose status contradicts merged work: SB-380, SB-381, SB-382 (merged in #718, #726,
  #727, rows say open), SB-390 (released 0.7.101, row says branch), SB-379 (W3 part 2 released in
  0.7.99), SB-384 (0.7.96 rules live), SB-342 (H-1b to H-1d released), SB-352 (waves 1 and 2
  released). Each gets its receipt.

## Waves

Each wave ends with the gate on the Mac Studio (`./scripts/check.sh`), Linux clippy in a container,
a release, the update on this MacBook and a check of what the wave claims on the released app.

| Wave | Content | State |
|---|---|---|
| 1 | S-1 wake-up floor; K-1, K-2, K-3; backlog hygiene; branch cleanup; this spec | in progress |
| 2 | W4 item 4 (reads off the store lock), item 9 (tree cached per change); WF-5 5.2e, 5.2f C8, 5.3 | next |
| 3 | SB-391 re-pin to DEC-0177 | after wave 2 |
| Waits | K-0 (a person creates API keys); WF-9, WF-10.2 and 10.3, WF-12; SB-385, SB-387 `task`, SB-353 *stalled*, SB-383, SB-350 | owners named above |
