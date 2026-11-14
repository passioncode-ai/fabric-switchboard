# WF-4 Usage of accounts on other machines (SB-366, wave 3)

> подпискам, которые not checking, while disabled, они не disabled, то есть чекинг подписок, который
> на другом устройстве, должен все равно сюда синхронизироваться как-то, продумать все это сетевое
> взаимодействие, которое здесь есть, чтобы не вылетали

## Now

- An account on another machine is marked elsewhere and so disabled here (SB-175); `cardState`
  returns `disabled` (`src/ui-logic.ts:279`) and the row reads "Not checked while disabled"
  (`src/main.ts:2279`).
- Its copy there is read only on request: `machines --live` (three machines at once, 20 s per ssh
  call, `machines.rs`), `remote status` (SB-327). DEC-0013 says "on request".
- Long-term route: each machine publishes what it holds through Fabric's hub (SB-177, open).

## Sub-modules and tasks

**4.1 Background read.** The owner reads each machine group's Switchboard over ssh every 15 minutes
while it runs (never from `serve` on a machine without the ssh trust), one machine at a time, the
same `remote_check::status` call `--live` uses, and keeps per account: used percent, windows,
`usage_status` there, `as_of`. Stored in `remote-usage.json` (0600, ids and numbers only).

**4.2 Network budget and failure.** 20 s per call, one call per machine per pass, never under the
owner transaction, never on the async worker (`blocking::run`); a failed read keeps the last answer
with its age and a reason code (`unreachable`, `refused`, `timeout`); three failures in a row back
off to an hour; the window and MCP answer from the file at once. Log `remote_usage` with counts and
codes only.

**4.3 Rows.** An account on another machine shows "Used 42 % on studio-sshlg, 12 min ago" (or "Not
read: studio-sshlg did not answer, 2 h ago"), never "disabled". Its quota feeds WF-12's routing and
the fleet view; it never feeds rotation here (DEC-0013).

## MCP

`switchboard_machines` gains each account's last read (`there`, `as_of`, `reason`) without `live`;
`live: true` still forces a fresh read.

## Decisions

DEC-0017 amends DEC-0013: reads in the background, still no routing.

## Questions

None open; the hub (SB-177) replaces ssh when Fabric ships it.

## Checks

A fake remote (the `remote_check` test seam) answering, timing out and refusing; idle budget test
(`an_idle_hour_on_a_fake_clock_stays_inside_the_budget`) extended with the 15-minute read; AGENTS.md
lifecycle row; live read on this MacBook against the Studio's three copies.

## Depends on

WF-2 (rows). Feeds WF-3 (`refused_there`) and WF-12.
