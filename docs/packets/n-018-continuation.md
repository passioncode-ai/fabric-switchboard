# Packet SB-52 / N-018 — continuation walking skeleton (owner materialization)

**State:** built 2026-10-05 (run record at the end), merged in #73 and released in v0.6.2; live acceptance open; the desktop entry and the `switchboard_continue` MCP tool are SB-64. **Board:** SB-52. **Program packet:** org-index
`docs/observatory/programs/2026-10-04-agent-memory/packets/N-018.md` (rev 1, PB-137/M9).
**Engine contract:** memory/0.1, `passioncode-ai/project-observatory-dashboard` `5c31318`
([#159](https://github.com/passioncode-ai/project-observatory-dashboard/pull/159), N-016 + N-025);
installed engine 0.15.0 on this machine. Design read: that repository's `docs/design/AGENT-MEMORY.md`.

## What the engine already guarantees (not rebuilt here)

One executor per workflow by lease token; `LeaseLost` for a write with an old token, its content
kept as a `step_result`; a handoff without the token only for `limit`/`crash`/`restart` after
`SILENCE_SECONDS` = 120 s of silence, at most one offer a minute; acceptance once per pack,
returning the checkpoint *as it is now* (`checkpointAdvanced`); packs carry constraints first and
each declared key's state; `accountRef` must match `[A-Za-z0-9][A-Za-z0-9._:-]{0,63}`; a session
names its workflow through `OBSERVATORY_WORKFLOW_ID`, which the Stop hook records. Switchboard's
negatives "late old-token write" and "repeated finished work" are therefore engine refusals that
the skeleton must surface, not re-implement.

## Switchboard's part

`switchboard continue <wf_…> --account <id> [--mode isolated|managed] --dir <project>` (CLI, MCP
tool `switchboard_continue` later):

1. **Read** `project-observatory full workflow show <wf> --json`
   (`checkpoint_latest`): refuse unless the workflow is open, every declared key is `vault`
   (`unknown` and `missing` block, named), no handoff is already waiting, and the executor's
   provider equals the account's (the same-provider skeleton; cross-provider is N-018 step 2).
   The project directory must be the workflow's git artifact checkout, or the user names it.
2. **Offer** `project-observatory full workflow handoff <wf> --to-provider <p> --to-account
   <switchboard account id> --reason limit` — the agents' rule, no `--force`, no terminal. A
   refusal (`silence`, rate) is reported verbatim and nothing is launched.
3. **Launch** the account's session in the project directory, as `launch` does today, plus:
   `OBSERVATORY_WORKFLOW_ID=<wf>`, `OBSERVATORY_HANDOFF_ID=<handoff>`, the Observatory MCP server
   declared in the session's own config next to Switchboard's (the user scope is not visible in
   an isolated home), and a first prompt: accept `<handoff>` with `observatory_handoff_accept`
   (its own `sessionId`), continue from the answer's checkpoint, obey its constraints. Switchboard
   never accepts on the session's behalf — acceptance is the new executor's act.
4. **Never:** read or copy the old session's transcript, put a credential into the pack or the
   prompt, or retry a launch with a new offer automatically.

## Targets

| Path | Change |
|---|---|
| `crates/switchboard-runtime/src/continuation.rs` (new) | `plan` (step 1, parses `show --json` into a typed readiness), `offer` (step 2, parses `handoff <id> offered until <t>`), errors as fixed vocabulary |
| `crates/switchboard-runtime/src/launch.rs` | `launch_with` takes an optional `Continuation {workflow_id, handoff_id}`: two env vars, the Observatory MCP entry, the first prompt |
| `crates/switchboard-runtime/src/lib.rs` | `Operation::Continue` through the owner transaction |
| `crates/switchboard-cli/src/main.rs` | `switchboard continue` |
| tests | a fake `project-observatory` executable (`SWITCHBOARD_OBSERVATORY_BIN`) answering `show`/`handoff`; the launch host fixture records the script |
| docs | CONTRACTS (continuation), CLI, SCN-034, this packet → run record |

## Acceptance (from N-018) mapped to tests

| N-018 | Test |
|---|---|
| positive: unavailable executor → resume open step from the pack only, obeying constraints | fake engine: open workflow, silent executor → offer made, launch script carries both ids, the MCP entry and the prompt; no transcript path anywhere in it |
| negative: no repeated finished work | engine `checkpointAdvanced` is passed through; the prompt says to continue from the acceptance answer, not the pack |
| negative: late old-token write | engine `LeaseLost` — covered by the engine; the skeleton test asserts Switchboard never presents or stores a lease token |
| negative: foreign project | `--dir` outside the workflow's artifact checkout refused before the offer |
| negative: wrong account/session, rollback safe | provider mismatch, disabled account, expired credential, keys not in vault, a waiting handoff: refused before the offer; a launch failure after the offer leaves the offer to lapse (`offerTtlSeconds`), reported |

## Open before building

- **Operator:** is the Observatory MCP server allowed into an isolated or managed home (its
  `OBSERVATORY_HOME` and venv path are machine-specific)? Alternative: the session calls
  `project-observatory` through Bash, which needs no MCP entry but is a weaker contract.
- **Observatory:** a `--json` answer for `workflow handoff`, so step 2 does not parse prose.
- **Live acceptance** (two real accounts, a real limit) is paid/provider work and needs the
  operator's bounded authorization; the skeleton ships on the synthetic harness.

## Run record (2026-10-05)

Built as targeted, with these decisions and findings:

- **Open questions closed.** The operator's yes to the Observatory MCP entry in a session's home
  (2026-10-05) is used: the entry is derived from the installed engine (launcher `#!` interpreter,
  `full-path`/`mcp/server.py`, `--home` from Switchboard's environment), never copied from the
  user's own agent configs. The engine still has no `--json` for `handoff`, so its one-line
  answer is parsed strictly (`parse_offer`), and anything else is refused as unreadable.
- **Handoff target.** `--to-provider` is the executor's own provider name, so the pack names the
  provider the workflow's sessions write; `--to-account` is the Switchboard account id (a UUID,
  inside the engine's `accountRef` pattern).
- **Preflight before the offer.** `launch::preflight` runs every check `launch` makes before it
  touches a home, plus the credential (isolated) or route selection (managed), so a launch that
  would be refused never leaves an offer behind.
- **Finding, engine 0.16.0:** a checkpoint's `git` artifact path with a UUID folder name is
  stored redacted (`…/[redacted]/repo`). Switchboard matches a redacted component to exactly one
  folder; the engine's own pack then reads git from a path that does not exist
  (`git_snapshot` → `not an existing absolute directory`), so the pack carries no git state for
  such a checkout. Filed as SB-58 for the Observatory owners.

| Check | Result |
|---|---|
| Unit and fake-engine tests | `cargo test -p switchboard-runtime continuation` — 12 passed, 1 ignored (`real_engine_contract`) |
| Launch tests | `a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt`, `preflight_refuses_what_a_launch_would_refuse_without_writing` |
| Planted defects (each separately, restored after) | folder check removed, `--force` added to the offer, waiting handoff ignored, key states ignored, provider mismatch accepted: each failed a test |
| Real engine, scratch workspace | engine 0.16.0, `OBSERVATORY_HOME` in the session scratchpad, a synthetic workflow backdated past the silence rule: read → plan → MCP server → offer `handoff:34f6193d9e640027` → second plan refused `HANDOFF_WAITING`. A fresh workflow's offer refused by the engine's silence rule, reported verbatim |
| Not run | a real session accepting the handoff on a second real account after a real limit (paid provider work; operator) |
