# Target state and refactoring plan (2026-11-17)

**Order (operator, 2026-11-17):** analyse the form Switchboard must have at the end, every function
that must work correctly and in what form; compare it with how it works now; make a refactoring plan
and close everything in waves to the end, while taking up incoming tasks in parallel.

## Method

1. One section per area, written from the repository's own sources of truth (SPEC, CONTRACTS,
   DECISIONS, the UX scenarios, the Lifecycle table in AGENTS.md, open backlog rows) and checked
   against the code with `file:line` and test names:
   - [01 Accounts and the runtime core](01-accounts-runtime.md)
   - [02 Groups, organizations, seats and keys](02-groups-orgs-keys.md)
   - [03 Sessions and work continuation](03-sessions.md)
   - [04 Surfaces and distribution](04-surfaces.md)
   - [05 Code structure and the test suite](05-code-structure.md)
   - [06 Groups fed from the default group](06-default-feed.md) (the operator's order of 2026-11-17)
   - [07 Saved launches](07-saved-launches.md) (the operator's order of 2026-11-18)
   - [08 Who starts, watches and continues coding sessions](08-session-map.md) (the operator's order of 2026-11-18)
   - [09 Moving accounts between machines](09-machine-moves.md) (the operator's order of 2026-11-18)
   - [10 Launch, fleet and lifecycle](10-launch-fleet-lifecycle.md) (the operator's order of 2026-11-19): attribution, watchers and continuations, full access by default, MCP, the smart usage mode, the Launch flow with key minting, remote session management and the organization's subscriptions board
2. Each capability carries **Target**, **Now**, **Gap** (P0 to P3, backlog ids) and **Refactor**.
3. The gaps and refactors are ordered into waves below. Every wave item becomes a backlog row in
   [docs/evidence/backlog.md](../../evidence/backlog.md) (edited under the agent-sync lease) before
   work starts, so the row, not this page, carries its state.

## Operator's additions (2026-11-17, same day)

These are part of the target state and come first in the waves:

1. **Turn an account off completely, until turned on by hand.** One switch takes it out of
   everything: no selection, rotation, failover, balancing, renewal, usage check, launch,
   dispatch, delegation or fallback step uses it, and nothing turns it on again by itself
   (an import, a sync, a restore, a recall, a lease). The window, CLI and MCP say it is off and
   since when. Today `accounts update --enabled false` exists; the waves verify every path
   against it and close the ones that still touch an account that is off.
2. **Devices of an organization.** A machine joins an organization's Switchboard (by an
   invitation the organization's owner makes, never by a shared password), accounts are shared
   within the organization and moved between its devices, and a device leaves or is removed
   with its seats called back. Builds on machine groups (DEC-0013), delegation (DEC-0025) and
   organizations as labels (DEC-0022).
3. **The session manager starts work when it arrives.** Measured 2026-11-17 17:55 CET on this
   MacBook: the session pool's cap was 20 (set 2026-11-15 on the operator's order relayed on
   thread `ebc84cbb`, "one pool per machine, example 20, board sessions wait in a queue") while
   34 sessions counted, most of them interactive, so every board start was refused
   `at_capacity` and waited with no end in sight. The cap was raised to 40 at 17:58 CET
   (`switchboard capacity set 40`: used 38, free 2) to let work through now. Target: board
   work never starves behind interactive sessions (a share of the pool reserved for the
   board, interactive sessions past their quiet time counted out, the queue's wait and its
   reason visible in Needs attention, Fabric told when a slot frees).
4. **Refactor everything that makes these fragile**, per section 05.
5. **The main screen, the updates and the keys, checked through the interface** (operator,
   2026-11-17 evening: «давай UI главный, где у нас сессии, укомпонуем по-новому. Плюс
   отрефакторим все апдейты, проверим, что у нас там никуда ключи не утекают, что все корректно,
   плюс все новые фичи что-то вписываются, проверим по интерфейсу»): the Sessions page laid out
   anew (W4), the update code refactored (W4), a read-only audit of where keys flow (W1.7) and a
   walk through the interface of every new feature (W7).

## Wave rules

- A wave item ends at its finish line: the full gate (`./scripts/check.sh`) green on the build
  machine, the Windows cross-check for Rust changes, and two consecutive review rounds with no
  P0 or P1. A P2 that blocks nothing becomes a backlog row.
- Behaviour-preserving refactors come after the test suite is hardened against machine load
  (the gate runs at load averages up to 1000), so a structural move is proved by tests that do
  not flake.
- A wave ends with a release (`scripts/release_bump.py`), the local install updated and the
  board told what changed.
- Incoming tasks (board requests, operator orders, review findings) are taken between items by
  priority; they never wait for a wave to end.

## Waves

Sections: 01 (19 capabilities: 10 done, 8 partial, 1 broken), 02 (20: 6 done, 13 partial, 1
missing), 03 (19: 4 done, 14 partial, 1 broken), 04 (22: 5 done, 17 partial), 05 (code
structure: refactors A1 to A7, B1 to B3 and the splits).

### W1. What the operator named today, and the P1s next to it

| Item | What | Row | State |
|---|---|---|---|
| W1.1 | The session pool keeps a share for board work; counting per session (quiet, limit-stopped, waiting and held sessions count out); a refused board start's wait and reason visible without the one-hour cutoff (03 §16) | SB-417 | released in 0.7.117 (PR #818); window follow-up PR #825 in 0.7.119 |
| W1.2 | Turning an account off: a person's switch no automatic process clears; six leaks closed (Claude Code renewal, elsewhere mark, recall, lease, call-back undo, restore); Kimi accounts too; "off since" on every surface (01 §3, 04 §3) | SB-418, DEC-0027 | in review (PR #821) |
| W1.3 | A restore saves every managed switching policy as off, against DEC-0008 (`core/backup.rs:723-725`; SCN-030) (01) | SB-569 | next |
| W1.4 | Money paths without a ceiling: Hermes on a named Anthropic key, a chain pinning another group's key (SB-96), a folder moved out of a capped group (SB-216), the machine ceiling and the pay-per-token meter (02) | SB-96, SB-216, SB-570 | next |
| W1.5 | MCP annotations: `switchboard_account_remove` not destructive, write tools idempotent, network tools `openWorldHint: false` (`mcp.rs:237-249`, `DELETES` at `mcp.rs:263`); plugin hooks silent without `switchboard` on PATH (04) | SB-571 (SB-517 in 0.7.122 added the `DELETES` list it builds on) | next |
| W1.6 | Groups fed from the default group ([06](06-default-feed.md)) | SB-419, DEC-0028 | released in 0.7.119 (PR #824) |
| W4.4 | Saved launches run from the Sessions page, the menu-bar icon, the CLI and MCP ([07](07-saved-launches.md)) | SB-454 | designed |
| W4.5 | One owner per session concern: route, environment, remote launch, continuity, admission with host load, a sessions and load feed for Observatory ([08](08-session-map.md)) | SB-450 to SB-455, SB-461 to SB-464 | designed |
| W4.6 | Moving accounts between machines: the machine chosen, its groups read from it, several at once, the program it runs ([09](09-machine-moves.md)) | SB-473 to SB-476 | in this change |
| W1.7 | Key-leak audit: where every key flows (vault, argv, environment, logs, files, answers), read-only, report in `docs/reports/2026-11-17-secret-flows/` | SB-572 | audit done (private report); P0 fixed by SB-420 in 0.7.119; P1s SB-425 to SB-427 open |

### W10. Launch, fleet and lifecycle ([10](10-launch-fleet-lifecycle.md), the operator's order of 2026-11-19)

W10.0 the open PRs land; W10.1 lifecycle and the watchdog (SB-505 to SB-510, SB-529, SB-530); W10.2 full access for
every agent and the MCP defects (SB-515, SB-517); W10.3 the smart usage mode (SB-518, SB-519); W10.4
one folder-to-group resolver (SB-511 to SB-514); W10.5 the Launch flow with key minting (SB-516, SB-520
to SB-522, SB-454); W10.6 remote session acts and pause (SB-523, SB-524); W10.7 the subscriptions board
(SB-526, SB-527, SB-525, SB-528); W10.8 two MCP doors (SB-478). Ends with a release.

### W2. Devices of an organization (02, 04 §7)

Signed invitation (the envelope settings files use), `switchboard org join` writing the restricted
key line itself, machine groups tagged with their organization, a device registry and the
Machines page (SB-374), one act to move a seat between devices, leave and remove calling every
seat back, a Windows transport for `ssh-serve`, the end-to-end organization walk (SB-371). The live
run on a teammate's machine (SB-403 S5) needs the person: install, tailnet, first join.

### W3. The test suite holds under load (05 A1 to A6)

A1 step durations and a test-name list, A2 Windows builds of the proxy tests and `src-tauri` in
both Windows checks (SB-406 item 7), A3 SB-416 at its cause (an injectable clock): released in 0.7.116 (PR #817).
Then A4 and A5 (29 wall-time assertions to paused time, a nightly stress run under load), A6
golden answers for every read surface on the CLI and MCP.

### W4. Structure (05 A7, B1 to B3 and the splits; 01, 02, 04 Refactor parts)

A7 operation facts as one method naming every variant (the 92-entry `matches!` list), B1 one
process-table module, B2 one remote-machine transport, B3 all operation facts in one place;
then pure moves out of `lib.rs` (`operation.rs`, `owner.rs`, the long arms into their modules,
tests into files), one placement record per account, the schema ladder as a table, the CLI's
`main.rs` and the window's `src/main.ts` split by page with DOM tests, one registry generating
the window, CLI and MCP parity table and a test that every window action has its twins, shared
typed answers, errors as codes.

Also in W4 (operator's addition 5):

- **Sessions page laid out anew (the main screen)**, after W1.1 and W1.6 land, through super-ux
  scenarios and sheleg-design.
- **Updates refactored:** the pure update rules shared by the app and `serve`
  ([04](04-surfaces.md) §18, *Refactor*).

### W5. Fabric Sessions leaves Switchboard (03, 05)

The launch-record store and terminal helpers into `fabric-sessions-core` first, then
`session_host`, `duty`, `restarts`, recovery's storage half, `session_watch` and `resumable`;
Switchboard calls the core; the service binary runs them, so quitting or updating Switchboard
no longer stops them (docs/plans/2026-11-16-session-service/plan.md).

### W6. Docs and board say what is true (all sections)

SPEC §2, §5, §8, §9, §14 against CONTRACTS and DEC-0008; DEC-0007 and `suggest.rs:10-12`
against SB-171; `ORGANIZATION.md:284` and `src/main.ts:1179` (and SCN-070) against DEC-0022; the
terminal-hook notice pointing at Groups (`src/ui-logic.ts:1705`, `suggestions.rs:107`); DEC-0018
and DEC-0020 reserved but never written; SCN-096 and SCN-097 missing from the scenario index;
rows whose status is behind what shipped (SB-352, SB-357, SB-367, SB-385, SB-391, SB-401, SB-403,
SB-404, SB-405, SB-407, SB-411, SB-413, SB-415, SB-243).

### W7. Acceptance that needs the person

SB-01 live providers, SB-06 a running session picking up a switch, SB-178 renewal across two
machines, Windows SB-02 and SB-243, R1.7 a live restart wave on this MacBook, SB-403 S5. Each is
prepared to the last step a person must take and named with its link. Also (operator's addition
5): an interface walk of every new feature (design-tester).

## State

| Wave | Items | State |
|---|---|---|
| W0 | PR #815 (SB-414, SB-408), #816 (SB-410, SB-415) | done: merged; tag `v0.7.115` was never published (a .deb test failed on Windows, nightly 44156588020); 0.7.116 (#822, tag `v0.7.116` at `5e9b0c40`) carries it with the test limited to Unix; published 2026-11-17 |
| W1 | W1.1 released in 0.7.117 (#818 `aada621c`, SB-417), its window follow-ups in 0.7.119 (#825); W1.2 in review (PR #821, SB-418, DEC-0027); W1.6 released in 0.7.119 (#824 `ce62f8c9`, SB-419, DEC-0028); W1.7 key-leak audit done (private report, published only after its fixes ship): P0 fixed in 0.7.119 (#826, SB-420), the P1s SB-425 to SB-427 open (SB-572); W1.3 to W1.5 next (SB-569, SB-570, SB-571); SB-421 (named Linux users at the proxy, board request c3dfcc4e, an organization's VM) in review (PR #832, DEC-0029) | in progress |
| W2 | devices of an organization | planned |
| W3 | A1 to A3 merged (#817 `84e7e352`); A4 to A6 next | in progress |
| W4 | structure | planned |
| W5 | Fabric Sessions | planned |
| W6 | docs and board | done: #823 `3ba78af8` (two review rounds, no P0/P1); left: "under Groups" in folder-rule messages to check, the MCP and onboarding wording to also name a group's sources |
| W7 | acceptance by the person | planned |
| W10 | launch, fleet and lifecycle | W10.0: #848 (SB-505) merged `07f60d03`; #838 (SB-455) and #830 (SB-423) released in 0.7.121; W10.2: SB-515 released in 0.7.121 (#850), SB-517 in 0.7.122 (#855); W10.3: SB-518 merged (#856), ships in 0.7.124 |
