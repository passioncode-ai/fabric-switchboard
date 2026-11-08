# A member's seat, and who may open a gate (SB-292, SB-291)

Design note, 2026-11-08. Status: proposed; code waits for the estate's common source of truth
(SB-291) and the architect role in the contract (SB-292). Owner: fabric-switchboard. Due
2026-11-13 with SB-283.

## What is asked

- **SB-292** (North Star gap G4, fabric-workspace `knowledge/north-star.md` at `adfade60`, sections
  3 and 7; board `4a65469b`): a member's Claude seat is one line of the organization's joining
  checklist and is reversed at leaving. (1) The administrator's agent grants a seat to a member's
  machine or VM by a command, recorded with who ordered it. (2) Leaving takes it back, proven by a
  test that the seat no longer serves a request there. (3) Switchboard on Linux in the member's VM
  (SB-88) holds only the groups of that member's projects.
- **SB-291** (operator decision relayed by fabric-workspace, board thread `eefa7959`, message
  `23746581`, 2026-11-06): levels owner, organization admin, project architect, user. A command of
  an admin, an architect or their agents passes and is logged; a user's waits for approval. The
  caller's level comes from the common source of truth, never from a field the caller sets.

## What exists (0.7.28)

| Piece | Where |
|---|---|
| Move a sign-in to another machine over ssh, one holder, undone on failure | `account_send::send`, `crates/switchboard-runtime/src/account_send.rs:55`; CLI `accounts send`, MCP `switchboard_account_send` (0.7.26) |
| Hand an account over without its credential: a signed record, the receiver signs in | `account_handover::hand_over`, `crates/switchboard-runtime/src/account_handover.rs:71`; `accounts hand-over`, `accounts receive --apply [--headless]` (0.7.22) |
| An administrator's settings file, signed; import refused unless signed by the published key | `settings_signing`, `crates/switchboard-runtime/src/settings_signing.rs:1`; `settings export --sign`, `settings import --signer` (SB-272) |
| What a settings file carries: groups by repository identity, organizations, agents' settings, rotation, ceilings; never an account | `settings_bundle`, `crates/switchboard-runtime/src/settings_bundle.rs:1` (SB-176, SB-270) |
| A data folder whose sign-ins live in files, for a machine with no graphical login | `vault use file` (SB-287, 0.7.25; [headless-vault](headless-vault-2026-11-07.md)) |
| The operator's quoted words open the five money refusals, recorded in `spend.json` | `Operation::OnOperatorsWord`, `window_only_refusal` at `crates/switchboard-runtime/src/lib.rs:3622` (SB-299, 0.7.28; [ceiling-doors](ceiling-doors-2026-11-07.md)) |
| A read-only MCP server refuses every write tool | `crates/switchboard-cli/src/mcp.rs:950` |
| A dispatched run asks before acting unless told `bypass` | `Permissions::Ask`, `crates/switchboard-runtime/src/launch.rs:2970`; `PERMISSIONS_INVALID`, `crates/switchboard-runtime/src/agent_settings.rs:46` (SB-298) |
| The organizations read from PassionCode.ai carry a `role`, which Switchboard does not keep | the read's fixtures, `crates/switchboard-runtime/src/vesta.rs:731` |

A Claude refresh issues a new refresh token and ends the old one, so a sign-in has one holder
([two-machines](two-machines-2026-10-23.md) section 1 and D-2). Every decision below keeps it.

## Decisions (SB-292)

| # | Decision | Why |
|---|---|---|
| S-1 | **Granting a seat is `accounts send` into the member's group, or `hand-over` when the member signs in themselves.** Send when the organization holds the subscription and the member's machine answers ssh; hand-over when the member's own sign-in is wanted or ssh is not open to them. | Both exist and keep one holder. Send needs no person at the member's end; hand-over needs one sign-in there (a human gate). |
| S-2 | **Every grant and recall writes one line to `<data>/seats.jsonl`** (0600): the account id, the other machine as ssh named it, the group, `granted` or `recalled`, the time, and the order: the operator's `Decision` (by, instruction verbatim, at, source) when the call carried one, else `by: agent` with the caller named by the transport. Newest 200 lines kept; nothing that signs in. | "Recorded with who ordered it" (G4 item 1). The `Decision` shape is the one SB-299 already checks (`spend::Decision`), so one validator serves both. |
| S-3 | **Leaving is `accounts recall ID --from user@host`: the seat moves back, not a fresh sign-in.** This machine runs `switchboard accounts surrender ID --json` there over ssh. Surrender marks the account used elsewhere there (that machine stops renewing it), prints the provider's tokens on stdout, which travel inside the ssh pipe into this machine's store and never to a terminal, chat or file. This machine saves them, clears its own *elsewhere* mark, checks the provider answers, then runs `accounts remove ID` there. Any failure before the remove clears the mark there and leaves this machine as it was. | The mirror of send. A fresh sign-in on the organization's machine would not end the member's lineage: only a refresh does, and the stale token here cannot refresh (D-2). |
| S-4 | **The proof of leaving is a request refused there.** After the remove, this machine asks the member's Switchboard over ssh for `accounts list --json` (the account is absent) and `usage ID` (answers not found). The acceptance test runs the member's side as a second data folder with its proxy: a managed request that names the recalled account's group answers with no account to serve it. | G4 item 2 asks for a test that the seat no longer serves a request there, not for a list that no longer shows it. |
| S-5 | **A VM that no longer answers is a recorded gap, not a silent success.** `recall` against an unreachable machine writes `recall_unreachable` to `seats.jsonl` and leaves the account marked elsewhere here; the account comes back only by a fresh sign-in here, a human gate, and the member's lineage lives until its own token expires. Destroying the VM ends it sooner. | No route ends another machine's lineage from outside (S-3). Saying so beats claiming a recall. |
| S-6 | **The member's VM holds only its member's groups: `settings export --group POOL` (repeatable) writes a file with those groups only**, signed with `--sign`; the VM imports it with `--signer`. Accounts never travel in it; the seat arrives by S-1 into one of those groups. | G4 item 3. The VM reads nothing it was not given; scoping happens where the file is written, on the administrator's side, so the VM needs no list of what to drop. |

## Decisions (SB-291)

| # | Gate today | Owner, admin, architect (their agents too) | User |
|---|---|---|---|
| L-1 | The five money refusals (`window_only_refusal`) | pass with the caller's own words as the `Decision`; logged in `spend.json` `loosened` and `ceiling_loosened` | wait: Switchboard files the request (`fabric_question_ask`-shaped: the operation, scope and amount) and refuses with the request id |
| L-2 | Granting or recalling a seat (S-1, S-3) | owner and admin for any group of their organization; an architect for their project's groups | wait, as L-1 |
| L-3 | Read-only MCP (`mcp.rs:950`) | unchanged: a read-only server is a property of the connection, chosen by whoever registered it, not a level | unchanged |
| L-4 | `--permissions ask` on a dispatched run | the dispatching agent may choose `bypass` | `ask` only; `bypass` is refused |

- **L-5. Where the level comes from.** From the common source of truth once it exists (Vesta SOT,
  paused in thread `eefa7959`): Switchboard asks it for the caller's role in the organization the
  group belongs to, with a token the caller's transport carries, and caches the answer for a
  minute. Today `Decision.by` is a field the caller writes (`"operator"`); that is the interim the
  operator's words of 2026-11-07 allow on the operator's own machines, and it is the one thing
  SB-291 replaces.
- **L-6. Unknown is a user.** A caller whose role cannot be read (no source of truth, unreachable,
  no token) is treated as a user: its loosening waits. Failing closed costs a question to a person;
  failing open costs money or a seat.
- **L-7. The role Vesta already returns is not trusted for this.** Switchboard reads `role` today
  for display only (`vesta.rs:731`); it describes the person who connected this Switchboard, not
  the caller of a given command.

## Open

- The source of truth's API for a caller's role (SB-291 waits for it).
- The architect role in the agent contract (SB-292 item L-2 waits for it).
- D-6 of [two-machines](two-machines-2026-10-23.md) is still unmeasured: whether a refresh on one
  machine ends the other's sign-in within one request. S-3 and S-5 assume it does, as the SB-21
  history on one machine showed.
- Codex sign-ins on two machines follow their own rules (SB-16 packet); S-3 is written for Claude
  first and is refused for Codex until measured.

## Next

1. S-6 (`settings export --group`, built on branch `feat/sb-292-export-group`) and S-2 (`seats.jsonl` for send and hand-over): no dependency.
2. S-3 and S-4 (`surrender`, `recall`, the two-folder acceptance test).
3. L-1 to L-6 when the source of truth answers a caller's role.
