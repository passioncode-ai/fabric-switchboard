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

Filled in from the sections once they are complete.

## State

| Wave | Items | State |
|---|---|---|
| W0 | in flight before this plan: PR #815 (SB-414, SB-408), 0.7.115 with SB-410 and SB-415 | in progress |
