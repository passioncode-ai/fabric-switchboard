# SB-115 — a limit stop is not a wait

Operator request, 2026-10-13, relayed by the nicegram-business session: «сделай такую
отметку в правилах для всех агентов… это свитчборд, пусть он продумает». Operator
decision in the fabric-switchboard session the same night: **hybrid** (below). The rule
for every agent lives in one place: fabric-workspace
[principle 10, item 6](https://github.com/passioncode-ai/fabric-workspace/blob/0bae3875/knowledge/principles.md#10-a-free-agent-takes-the-next-task).
This packet is Switchboard's part. Base: `b58477b` (0.6.26, SB-114 producer).

## Why the stop is usually stale

Tonight's evidence (nicegram-business): more than six workflow lanes stopped with
"resets 5am", "resets 9pm" and "resets Oct 17", and all resumed minutes later; a failed
lane and a lane silent for 30 minutes both resumed on restart. SB-98 explains it: when
the account a session works on is refused, Switchboard moves the pool to the next
account within a pass (30 s), so the session's stop names a reset that no longer
applies to it. Waiting for that reset idles the work for hours (SB-98's measurement:
264 stops in three days, median stall 77 minutes).

## The decision: hybrid readiness

| Situation | SB-114 (0.6.26) | SB-115 |
|---|---|---|
| A compatible account has a fresh successful quota observation (< 5 min, < 100 %, no hold) | ready at once | unchanged: ready at once, `mode: "capacity"` |
| No such account | waits, possibly until the reset | **probe** on a backoff from the stop: 2, 5, 10, then 15 minutes, `mode: "probe"`; the reset time is never waited for |
| The stop is a spend, credit or billing limit (the hook's `billing_error`, or every compatible account held by a spend-cap reason) | waits | never probed: a human gate (payment or top-up) |

The backoff step is the stop's `attempt`: 0 for the first stop of a session, one more
for each stop of the same session that follows a restart within 30 minutes. A probe is
a claim like any other: claim-once, persisted before it returns, never replayed after
an unknown outcome. Readiness by probe is computed by the same function that `claim`
checks (retro standing instruction 1).

## Registration from the stopped session itself

Claude Code fires `StopFailure` when a turn ends on an API error; its input carries
`session_id`, `transcript_path`, `cwd` and `error` (`rate_limit`, `billing_error`,
`overloaded`…). It is notification-only: it cannot continue the turn
([hooks reference](https://code.claude.com/docs/en/hooks.md), read 2026-10-13). The
plugin ships one hook:

```json
{"hooks": {"StopFailure": [{"matcher": "rate_limit|billing_error",
  "hooks": [{"type": "command", "command": "switchboard recovery hook", "timeout": 10}]}]}}
```

`switchboard recovery hook` reads that JSON on stdin and registers the stop with the
running owner:

- **event:** `cc-<session_id>-<unix seconds>`;
- **account:** the account Switchboard's limit evidence charged that session's limit to
  (`limits` session binding), else the account the ordinary Claude Code is signed in to.
  When neither resolves, nothing is registered;
- **folder:** `cwd`;
- **kind:** `billing` for `billing_error`, `limit` otherwise.

A repeated stop of the same session follows the session: a `waiting` row is bumped
(new `stopped_at`, `attempt + 1`); a `claimed` row is closed as `resumed`, since the
session ran and stopped again, and a new row starts at `attempt + 1`. The hook never fails the
session: every refusal (no owner, unknown account, folder not allowed, full registry)
exits 0 and is logged by code as `recovery_hook`.

Open questions are not invariants (standing instruction 7):

- whether a subscription session or usage limit arrives as `rate_limit` is not
  documented. The matcher includes it either way, and an unmatched error registers
  nothing, which is SB-114's behaviour;
- whether `StopFailure` fires for subagents inside a session is not documented. Lanes
  and subagents are restarted by their orchestrator under the principle, not by this
  hook.

## What Switchboard does not do

It owns no PTY and sends no prompt. It never runs `claude --resume` against a session
that may still be open (undocumented, two writers on one transcript), never kills a
client, and never replays a claimed request. The host acts on `ready` rows: Fabric
Dashboards' console (FD-62), Fabric's session host (SB-114 REQ-5), or the orchestrator
that reads `switchboard recovery`. Detecting a silent session (no transcript writes for
15–20 minutes) needs a host that knows the task is unfinished; it is not inferred from
transcripts here (follow-up, SB-116).

## Requirements

| Requirement | Verified by |
|---|---|
| REQ-1 Fresh capacity stays ready at once (`mode: capacity`); SB-114 semantics unchanged | `recovery::tests` (existing SB-114 tests pass unchanged) |
| REQ-2 Without capacity, a `limit` stop becomes probe-ready at stop + 2/5/10/15 min by attempt; never before; the reset time is not consulted | `recovery::tests::a_stop_without_capacity_is_probed_on_the_backoff_not_at_the_reset` |
| REQ-3 A `billing` stop, or a pool whose every compatible account is spend-capped, is never probe-ready | `recovery::tests::a_billing_stop_is_never_probed` |
| REQ-4 Claim accepts a probe only when the shared readiness function says so; persisted, claim-once, records `mode` | `recovery::tests::a_probe_claim_is_checked_by_the_same_rule_and_taken_once` |
| REQ-5 The hook registers from StopFailure input; account from the session's limit binding, else the ordinary CLI's; repeated stops bump `attempt`; claimed → resumed on a new stop | `recovery::tests::repeated_stops_of_one_session_raise_the_attempt`, CLI test `recovery_hook_never_fails_the_session` |
| REQ-6 The hook never fails the session: no owner, bad JSON, unknown account → exit 0, logged by code | CLI test `recovery_hook_never_fails_the_session` |
| REQ-7 Old `recovery.json` files (SB-114, no `kind`/`attempt`/`mode`) still load | `recovery::tests::an_sb114_record_still_loads` |
| REQ-8 The plugin ships the hook; docs (CLI, CONTRACTS, plugin README, switching-accounts skill) state the rule and its limits | plugin validate in `./scripts/check.sh`; review |

## Checks and next task

`./scripts/check.sh` with lean Cargo settings. The live check needs an operator
session that actually reaches a limit, so it is not claimed from the synthetic
tests. Next: FD-62 and Fabric consume `mode: probe` rows like `capacity` rows (SB-114
REQ-5); SB-116, stall detection from the host.

## Evidence the same night

Two background subagents of the fabric-switchboard session (the skill releases) stopped on
"You've hit your session limit · resets 8:10am" (`rate_limit`, HTTP 429). They were sent
«resume» at once and both went on to finish their releases (agent-stack 0.28.1). The rule
held before Switchboard shipped anything.
