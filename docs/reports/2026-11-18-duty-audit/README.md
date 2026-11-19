---
report: fabric-switchboard session duty officer audit, why working sessions were closed 2026-11-18
project: fabric-switchboard
date: 2026-11-18
source: operator 2026-11-18, session 4c635d5e («давай вообще проведем аудит, почему эти дежурные закрывают сессии, которые у нас работают, чтобы так вот не было, потому что периодически все сессии закрываются, дежурный работает неправильно»)
---

# Why the session duty officer closed sessions that were working

The duty officer (SB-384, [DEC-0021](../../DECISIONS.md#dec-0021-a-duty-officer-restarts-stalled-claude-code-sessions-and-closes-finished-ones))
looks at every live interactive Claude Code session every five minutes and closes one that
finished. The person saw sessions they were using disappear. This cut reads the duty officer's own
journal against the sessions' transcripts and launch records, names each cause and the change
that removes it.

## What was read

- The journal `duty.jsonl` in the app's data folder (`ai.passioncode.fabric-switchboard`): every
  close outcome line of the three days before 2026-11-18 17:25 UTC, 118 closes (115 `idle`, 3
  `duplicate`), and the whole journal (186 lines since 2026-11-15) for repeats.
- For each closed session: its transcript and its background agents' transcripts
  (`<session>/subagents/*.jsonl`) in every Claude config folder Switchboard knows (the person's own,
  `homes/*`, `runtimes/*`), by the `timestamp` of each line; and the launch records
  (`launches/*.json`) for launches the session started itself (`launched_by.caller_launch_id`).
- The code: `crates/switchboard-runtime/src/duty.rs` (`decide`, `check`, `observe`) at
  origin/main `54fe7a5f`.

The classification script lives in the session's scratch space, not in Git: it reads only times,
ids and line kinds, never a transcript's text.

## Classification of the 118 closes

A close is counted wrong when the evidence shows the session was still doing work at the moment
it was closed.

| Cause at the moment of the close | Closes | Wrong? |
|---|---:|---|
| Idle 65 minutes or more, nothing written, nothing of its own running | 77 | no, by the rule |
| Idle by its status file, but its own transcript written within the 65 minutes before | 35 | 3 yes (written 45 s, 2.5 min and 3.6 min before: the status file was stale); 32 were bookkeeping lines (`turn_duration`, `pr-link`) 61 to 65 minutes old, finished |
| Idle, no transcript found in any known config folder | 3 | unknown, so unsafe |
| Duplicate copy closed while the shared transcript was written | 3 | not decidable from outside (both copies write one transcript) |
| Background agents writing while the main session read `idle` | 0 | none in this window |
| A launch the session started itself still running | 0 | none in this window |

The person's view differs from the rule. Of the 118 closes, 35 were of sessions the person took
up again afterwards (the transcript grew after the close), and 33 of the 120 `idle` closes in the
journal were of a session already closed as `idle` before: the same session closed again after the
person resumed it (one session four times). The duty officer counted "waiting for the person with
an empty prompt" as finished; the person counted it as theirs.

101 of the 118 closes were sessions in the person's own terminal app (host `orca`), 9 in Terminal,
8 in other hosts.

### The Sound Engine audit (session `d4a341f6`)

Closed `idle` at 2026-11-18 11:45 UTC. Its last background agents reported at 10:39 and 10:40
(ten task notifications within 30 s), its last turn ended 65.5 minutes before the close, no
subagent transcript was written after, it had no launch of its own running and no wake-up set
(the 11 `ScheduleWakeup` mentions are tool listings, not calls). By the rule it was finished. It
was an orchestrator the person meant to keep: its next work was to start sessions on
subscriptions and to come back to them. The rule had no way to know, and a wait on sessions it
starts itself through `switchboard launch --hosted` is invisible to the process tree (each runs in
a session host of its own). Later the same day it ran with seven such launches; without SB-500 the
next quiet hour would have closed it again.

### Two claims checked

- **`idle_minutes` 60 against "never under 65".** `decide` takes `max(idle_minutes * 60,
  WAKE_UP_FLOOR)` with `WAKE_UP_FLOOR` 3900 s (`duty.rs:42`), so a setting of 60 closes after 65
  minutes. The claim holds; the setting reads lower than what it does, and `duty` answers the
  effective `idle_closes_after_minutes`.
- **`max_age_hours` unused, yet 7 restarts with reason `old`.** All 7 are from 2026-11-15 05:24 to
  06:01 UTC, an earlier version that restarted long-lived sessions; no code at origin/main writes
  `old` (`grep '"old"' crates/` finds only tests of other modules). The claim holds for the current
  code, but `switchboard duty --help` still said it "gives a long-lived one a fresh process at the
  end of a turn"; corrected in this change.

## Causes and fixes

| Row | Cause | Fix |
|---|---|---|
| SB-500 | `idle` judged by the status file alone: a stale status, background agents writing, a missing transcript and the session's own running launches all read as finished | `idle` now also needs its transcript and its background agents' transcripts quiet for the same 65 minutes (`transcript_quiet`), a transcript found (unknown is not quiet), and no launch of its own running (`session_watch::running_children`; unknown when the process table was not read) |
| SB-501 | A session the person took up again after an `idle` close was closed again at its next quiet hour | the journal is read: a session closed as `idle` before and running again is never closed as `idle` again (`closed_idle_before`); stuck and duplicate still apply |
| SB-502 | `switchboard duty --help` described the removed age restart | help text corrected |

Interim, until the release with these changes is installed: `idle_minutes` set to 240 on the
person's Mac (`switchboard duty set --idle-minutes 240`, 2026-11-18 17:30 UTC), so a session closes
after four quiet hours instead of 65 minutes.

Left as rows: a duplicate's copies share one transcript, so which copy works is read from the
status files only (SB-503); a session waiting for the person with nothing running and no
transcript written is still closed after `idle_minutes` the first time (the rule DEC-0021 keeps;
`switchboard monitor hold` keeps one open on purpose).

## Tests

`duty::tests::an_idle_session_whose_agents_or_launches_still_work_is_never_closed`,
`duty::tests::a_session_taken_up_again_after_an_idle_close_is_not_closed_as_idle_again`,
`duty::tests::own_launches_are_counted_per_session_and_unknown_without_the_table`,
`session_watch::tests::a_sessions_running_launches_of_its_own_are_counted_for_its_process`; the
existing decide tests now give a finished session a quiet transcript.
