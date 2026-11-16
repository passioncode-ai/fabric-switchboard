# Session lifecycle: never kill a waiting session, resume the same one, manage them on one page (2026-11-16)

The final spec for the operator's request of 2026-11-16, closed through `task-pipeline` in waves.
Board rows: [SB-396](../../evidence/backlog.md) (this lifecycle), [SB-395](../../evidence/backlog.md)
(the Sessions page). Evidence: the [audit](audit.md) (every claim with `file:line`). Entry point for
the next agent: the wave table at the end.

## What was asked (the operator's words)

> Лайфсайкл должен понимать, что сессия находится в режиме мониторинга. Возможно, нужно доработать
> для этого эджинг-контракт, либо еще какие-то вещи для нашего харнеса, чтобы не кикать сессии,
> которые остаются в режиме мониторинга. … если они доработали и мониторят, а непонятно, когда этот
> мониторинг придет, значит, надо, чтобы сессия умерла. восстановить именно ту, которая ждала, а не
> новую открыть. Вот этот механизм, чтобы контекст не терялся, обязательно должен быть проверен …
> проведи детальный аудит, вот life cycle сессии, как он сделан, документацию, типа проверь, типа
> каждую функцию … составь план рефакторинга, напиши спеку финальную на это все, и о том, чтобы
> добавить сюда элементы типа сам вот этот лайфсайкл сессии, экран сессиями, чтобы виден был, можно
> было их менеджить … 2-3 круга итерации качественного UI и выпускаем апдейт.

And, the same message, on the Apps & Soft group: «DZ, SingPlay и SoundEngine, они сейчас висят без
подписок … проверим … поднимутся ли они … чтобы они корректно поднялись и продолжились … можно было
из дефолтной … переносить в другую группу».

## What the live machine showed (this MacBook, 2026-11-16 16:20 local)

- Seven limit stops of Apps & Soft (dissy, riffly, sound-engine, sound-engine-research-runner) read
  `ready` in `switchboard recovery` (a probe on the backoff, SB-115), with both of the group's
  accounts at 100 % of the five-hour window (resets 0.5 h and 1 h later) and weekly room left.
- Fabric Dashboards, the host that continues limit stops (`resumeAfterLimit: true`), held them:
  `limit stop … waits (LC-17: disk)` and `(LC-17: pool)` in its `main.log`; LC-17 starts a session
  only with at least 1 GB free, and the disk swung between 0.1 and 3 GB (a growing Colima VM).
- Balancing (SB-352), the mechanism that lends an account from one group to another, was off (no
  group chosen). Turned on for `default`, `apps-soft` and `agents` on the operator's words. It lends
  only from a group that keeps two accounts with room; the default group had one
  (`sergeysheleg4`), every other account at 100 % of its week. LC-8 below.

## Findings (from the [audit](audit.md), section 4)

P1: D1 an unread process table closes every idle session; D2 the duty officer closes sessions
stopped on a limit; D3 the session host ends a hosted session after 30 idle minutes, under the
65-minute wake-up floor. P2: D4 to D15 (no floor for a duplicate close, nothing recorded to resume
an idle close, weekly stops dropped after a day, a claim that does not refuse a live session,
transcripts not carried to another account's home, the fallback opening a new session, the
Sessions page's double rows and two shapes of `ended`, long MCP calls read as stuck, sleep). P3:
D16 to D23 (doc drift, dead parameters).

## Requirements

- **LC-1** Nothing Switchboard runs closes a session whose own commands, limit stop or monitor it
  cannot rule out: an unread process table is "unknown", never "no commands" (D1).
- **LC-2** A session with a waiting or claimed limit stop is never closed by the duty officer (D2).
- **LC-3** One idle floor (65 minutes, the longest wake-up Claude Code allows plus a margin) for the
  duty officer, a duplicate close and the session host (D3, D4).
- **LC-4** `ended` has one shape (D11, D16).
- **LC-5** Every end Switchboard causes or sees is recorded so the same session can be resumed
  (`resumable.jsonl`: session, config folder, folder, launch, account, reason) (D5, D12, D15).
- **LC-6** Monitoring is a state: hooks on `ScheduleWakeup`, `CronCreate`, `CronDelete`, `Monitor`
  and `SessionEnd` write an honest `until` (audit 2.3, option 1; no contract change). MON-1 to MON-5.
- **LC-7** Continue resumes the same session (`--resume <id>` in its own config folder, or with its
  transcript carried to the chosen account's home), refused while it is live. RES-1 to RES-4.
- **LC-8** Moving an account from the default group to another on purpose stays one command
  (`switchboard accounts move <id> --pool <group>`, MCP `switchboard_account_move`); balancing lends
  by itself between the chosen groups. Whether the default group may lend its last account with room
  to a group that has none is the operator's call (asked, not assumed).
- **SP-1 to SP-5** The Sessions page ([audit](audit.md) section 6), SCN-098.

## Observed after the requirements were written

- 16:47 local: Fabric Dashboards continued dissy (`090b5f51`) and riffly (`08bb894b`) as the same
  sessions (`claude --resume <id>`, attempts 2 and 3), with their context. The resume path works
  when the disk admits a start.
- dissy's stops show 17 probe attempts in one day while its account read 100 % with a known reset:
  **LC-9** (wave D) a probe is not offered before a known reset of the account it would run on,
  unless the usage reading is stale (SB-115 keeps probing for readings that lie).

## Waves

Each wave: the gate on the Mac Studio (`./scripts/check.sh`), Linux clippy, a release, the update on
this MacBook and a check of what the wave claims on the released app.

| Wave | Content | State |
|---|---|---|
| A | LC-1 (D1), LC-2 (D2), LC-3 (D3, D4); docs: CONTRACTS duty section, AGENTS lifecycle rows | merged (#767, `2ea4dc09`) |
| B | LC-5, LC-7 (continue the same session), SP-2 one row per session, D11 read side, the Sessions page with Continue (SB-395) | merged (#768, `5ecc4ed2`); LC-4 (one written shape of `ended`) moves to wave D |
| C | LC-6 monitor hooks and acting on them (MON-1 to MON-3); MON-4 (revive a parked cron session at its next fire) moves to wave D | branch `lifecycle-c` |
| D | Recovery guards (D6, D7, D8), the fallback prefers `--resume` (D9), cleanups (D13, D17 to D23) | after C |
