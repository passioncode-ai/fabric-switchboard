# SB-82: fallback chains as the contract's runner routes (design note, 2026-11-02)

**Row.** [SB-82](../evidence/backlog.md): adopt fabric-agent-contract's runner routes (DEC-0026, DEC-0029,
[runners.md → Runner routes](https://github.com/passioncode-ai/fabric-agent-contract/blob/main/docs/specification/runners.md#runner-routes),
read at `d1b905ee`) for Switchboard's fallback chains, with one preference source shared with Fabric's fallback order
(Fabric ADR-0125, `docs/adr/0125-an-agent-launch-may-follow-the-operators-fallback-order.md`).

## What Switchboard does today (main `9c5b529`)

- **A chain** is `Chain {scope: machine | project | workflow, executors: [{agent, account_id?, key?}] ≤ 8}` in
  `accounts.json` (`crates/switchboard-core/src/chains.rs:10-43`); `chain_for` picks workflow, then project, then
  machine (`chains.rs:248-259`). `Executor` is `deny_unknown_fields`: a new field makes an older build refuse the file.
- **Agent ids** are Switchboard's (`catalog/agents.json`): Cursor is `cursor-cli`, the contract's `cursor-agent`;
  `agent_catalog::CONTRACT_ALIASES` maps them for output only (`agent_catalog.rs:12-21`), and input naming the
  contract id is refused. The ids are stored in chains, `agents-settings.json`, `handovers/*.json`, `spend.json`'s
  `last_handover` and `launches/*.json`.
- **The walk** (`crates/switchboard-runtime/src/fallback.rs`, `resolve_with` L116-260, `hand_over` L1099-1148) follows
  the chain only: it never appends an agent (contract rule met). The default agent is used only by an operator's
  `switchboard continue` without an account, as a one-step chain (`fallback.rs:419-536`).
- **Its record** is one oplog `fallback {outcome}` per walk (codes only, no agent ids by design, `oplog.rs:5-7`) and the
  overwritten `spend.json` `last_handover {at, workflow, outcome, skipped[{agent, reason}]}`. The reasons are
  Switchboard's (`agent_disabled`, `no_usable_account`, `agent_not_automatic_yet`, spend-gate codes), not the
  contract's closed probe results; a refusal from the offer itself is dropped (`fallback.rs:1139`), steps after a
  `launch_failed` are not recorded, and a Claude Code or Codex binary that is missing is found only after the offer.
- **Quota unknown**: an account with no usage observation is ranked last and still run (`fallback.rs:58-61`, Kimi
  `kimi.rs:405-411`), unattended too.
- **Permission mode** is not modelled: Claude Code inherits the operator's settings (`carry.rs:503-526`), Codex its
  defaults, Kimi's first turn is `kimi -p`, which asks for no approval (`kimi.rs:880-884`), Hermes and OpenCode run
  with no permission configuration. Nothing compares the executor's containment with the candidate's.

## Decisions

- **D-1. A chain is a host order, not a route, until the contract route is pinned.** As Fabric's ADR-0125 decision 1:
  what Switchboard records names the basis `host-order` and no route revision. When a project-pinned `runner-route`
  revision exists (the contract repin, later), it takes precedence and the records name it.
- **D-2. Contract runner kinds at the edges, Switchboard ids in storage.** Input accepts the contract kind
  (`cursor-agent`, `kiro`) and stores the Switchboard id; records and answers carry both (`agent`, `runner_kind`). No
  stored id is renamed, so no file needs a migration and no older build refuses one. Why: five stores hold the ids;
  a rename buys nothing a reader of the record does not already get from `runner_kind`.
- **D-3. One walk record per walk, with the contract's probe results.** `<data>/fallback-walks.jsonl` (0600, newest
  last, at most 200 lines, rotated): `{at, event: runner-selected | runner-exhausted, basis, workflow, unattended,
  passed_over: [{agent, runner_kind, probe, detail}], selected?: {agent, runner_kind, account_id?, spawned: true}}`,
  every candidate above the selected one exactly once and in order (`FAC-SEM-034`), every candidate for an exhausted
  walk. `probe` is the closed set: `not-catalogued`, `not-admitted` (`agent_not_automatic_yet`), `not-installed`,
  `not-connected` (sign-in required), `quota-unknown`, `refused` (disabled, spend gate, permission mode), `spawn-failed`
  (`launch_failed`); Switchboard's own reason stays in `detail`. The oplog keeps its code-only `fallback` event.
  Read by `chain list --json` and MCP `switchboard_chain_get` as `walks` (the last 20). `last_handover` stays for the
  window until it reads `walks`.
- **D-4. Every candidate is judged before the offer.** A missing Claude Code or Codex binary is `not-installed` at the
  walk, not a `launch_failed` after it; a refusal from the offer joins `passed_over` as `refused`, and the walk goes on
  to the next candidate where the contract's failure class says the candidate cannot serve (`request invalid` and
  `outcome unknown` still stop the walk).
- **D-5. Unattended walks pass over `quota-unknown`.** The automatic fallback (surface `fallback`) passes over an
  account with no usage observation, and a paid step whose spend cannot be read under a ceiling (already
  `spend_unknown`), as `quota-unknown`; an operator's `switchboard continue` keeps today's ranking.
- **D-6. Permission mode: the operator's decision (asked 2026-11-02).** The contract says a candidate that cannot run
  under the launch's mode is `refused`, never run looser; today a Claude Code executor can be followed by Kimi `-p`,
  which asks for no approval. Recommended: a containment class per agent in the catalogue (`asks` for Claude Code
  under the operator's settings and Codex defaults; `none` for Kimi `-p`, Hermes and OpenCode as launched now), and an
  unattended step whose class is weaker than the first executor's passed over as `refused` (`detail:
  permission_mode`). It changes what runs today, so it waits for the answer; D-1 to D-5 do not.
- **D-7. One preference source with Fabric.** Fabric's *Fallback order* is a device-local setting of Fabric's
  (ADR-0125 decision 1); Switchboard's chains are Switchboard's. Until the contract route lands, the two are not merged:
  Switchboard exposes its machine chain read-only (`chain list --json`, already) so Fabric can show it, and a board
  request asks Fabric whether its order should read Switchboard's machine chain as its default.

## Work graph

| Node | Does | Files | Needs |
|---|---|---|---|
| N1 | walk record (D-3), probe mapping, `walks` in chain get | `fallback.rs`, new `walks.rs`, `lib.rs` (chain read), `mcp.rs` | none |
| N2 | contract kinds on input and in records (D-2) | `agent_catalog.rs`, `lib.rs` `SetChain`, `chains.rs` validation | none |
| N3 | judge before the offer (D-4), unattended quota-unknown (D-5) | `fallback.rs`, `launch.rs` (binary lookup), `kimi.rs` | N1 |
| N4 | permission containment (D-6) | `catalog/agents.json`, `fallback.rs` | the operator's answer, N1 |
| N5 | docs: CONTRACTS (Fallback chains, Automatic fallback), AGENTS.md writes row, CLI.md, plugin tools.md, CHANGELOG | docs | N1-N3 |

Tests per node: N1 a walk with a skipped, a gated and a selected candidate writes one line with each passed-over
candidate once, in order, with its probe; an exhausted walk lists every candidate. N2 `chain set cursor-agent` stores
`cursor-cli` and reads back `runner_kind: cursor-agent`. N3 a missing `claude` binary is `not-installed` and the next
candidate is tried; an unattended walk passes over an unobserved account as `quota-unknown`, `continue` by hand runs it.
