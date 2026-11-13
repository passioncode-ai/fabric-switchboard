# Machine groups: an account on another machine is in that machine's group (SB-355)

Design note, 2026-11-13. Status: accepted as [DEC-0013](../DECISIONS.md#dec-0013-an-account-on-another-machine-belongs-to-that-machines-group-and-to-no-group-here);
built in branch `sb-355-machine-groups`. Owner: fabric-switchboard.

## What is asked

The operator, 2026-11-13 (session `59745113`): accounts that went to other machines leave the
groups here and sit in groups of their own, one per machine; accounts move between those groups
and the groups here in both directions; the window shows every machine and the accounts on it.
He also asked how the machines are linked, how Switchboard knows a machine is his, whether
colleagues see it, and how delegation works: answered in [MOVING](../MOVING.md#machine-groups).

## What existed (0.7.75)

- `accounts send` (SB-283) moves a Claude or Codex sign-in over ssh and marks it `elsewhere` with
  `locations[id] = <ssh name>`; the account keeps its group here (`account.pool`), turned off.
- `accounts recall` (SB-292, SB-315) takes it back; `seats.jsonl` records both, with the ssh name.
- `targets` lists the machines named by moves and remote runs, merged by `ssh -G` (SB-340).
- On the operator's MacBook: 17 accounts elsewhere, located as `studio-appvillis`,
  `sshlg@100.69.20.19`, `passioncode@100.69.20.19` and two sentences a person typed.

## Decisions

1. **A machine group is not a group of folders.** It is a record of its own in `accounts.json`
   (`machines`: pool, name, `ssh -G` identity, aliases, created), never in `projects`. Nothing that
   reads groups for folders, bindings, the terminal hook, balancing, settings sync or backups'
   groups sees it. Its pool is `machine_<name>`; the underscore is a character a group's pool never
   takes (`pool_from_name`), so the two kinds never collide.
2. **Membership is the account's home.** An account on a machine has `pool = machine_<name>` and
   is always marked `elsewhere` (the store refuses one without the other). Joining drops its
   selections, the shares other groups held of it and its loans (SB-282, SB-352 prune them as for
   any home change), and keeps the group it left in `returns_to`. A machine pool never gets a
   rotation policy (`ensure_default_policies` skips it, `set_policy` refuses it), so it never
   rotates, fails over or is lent here.
3. **One ssh destination is one machine.** The key is `user@host:port` as `ssh -G` resolves the
   name, read once when the account joins (5 s at most, never under the store's lock); every name
   used for it is an alias. Three macOS users on one Mac Studio are three machines: three
   Switchboards, three data folders, three groups.
4. **Moves both ways reuse the two ssh verbs.** Here to a machine: `accounts send`; it joins the
   machine group once the copy there answered. A machine to here: `accounts recall`, into
   `--pool`, else the group it left, else the default group. A machine to another machine: one
   `accounts send` of an account that is on a machine runs the call back first, then the move; if
   the move fails the account is back here, never lost. "Use here" (clearing the mark by hand)
   brings it home the same way.
5. **The accounts already elsewhere join once.** `machines adopt` (a dry run; `--apply` writes)
   and, once, the owner's first start of this version (`machine-groups.json` records it). Each
   account marked elsewhere and in no machine group is placed by, in order: the move recorded in
   `seats.jsonl` that was not called back; its location equal to a known machine's name, alias or
   identity; its location's first words (up to a comma, colon or bracket, spaces as hyphens)
   equal to one. Known machines are the existing machine groups and `targets` (moves and runs).
   An account none of these places stays where it is and is listed as `unplaced`.
6. **Live state is read on request, bounded.** `machines --live` (and the window's *Read their
   state*) reads each machine's Switchboard with `remote status`, all machines at once, 20 s each
   at most, on blocking threads, never under the store's lock (the SB-347 lesson). An unreachable
   machine reads `unreachable`; its accounts are still listed from the record here.
7. **Schema version 14** while a machine group, a `returns_to` entry or a `machine` journal entry
   exists. A 0.7.75 or older reader refuses `accounts.json` ("Unsupported account metadata
   version"); going back means restoring a backup, which keeps each machine's accounts as accounts
   marked elsewhere in the group they left (an older reader ignores the backup's machine fields).

## What it does not do

- It proves nothing about who owns a machine: Switchboard trusts whatever ssh reaches without a
  prompt. Proposed follow-up (SB-356): pin the other machine's settings signing key
  (`settings signer`, SB-272) at the first move and compare it on every later ssh call, so a
  machine that changed hands or a hijacked alias is refused instead of trusted.
- A machine group is local to this data folder; nothing is published to PassionCode.ai or another
  person.
