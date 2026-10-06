# Packet XA-01 — cross-agent continuation and fallback chains

**State:** designed 2026-10-06 (operator request and decisions below); SB-70 released in 0.6.7; SB-71 (core, CLI, MCP) and SB-73 phase 1 (Claude Code ↔ Codex) released in 0.6.8; SB-72 waits on the OpenRouter door's daily reset (project-observatory-dashboard#175); SB-74 on Fabric's ADR. **Board:**
SB-70 … SB-74. **Org track:** fabric-workspace roadmap RM-19 (the pipeline part also under
RM-06). **Program link:** org-index agent-memory N-018 step 2 (cross-provider pack), N-021,
N-022. **Shared:** [contracts](../CONTRACTS.md), [continuation skeleton](n-018-continuation.md),
[agent catalog](../AGENT-SUPPORT.md), [accounts and rotation](../ACCOUNTS-AND-ROTATION.md).

## The ask (operator, 2026-10-06)

1. Move work from Claude Code to Codex — and on to other agents (Kimi Code, Hermes, …) — **without
   losing context**. Today `switchboard continue` refuses another provider (`PROVIDER_MISMATCH`,
   crates/switchboard-runtime/src/continuation.rs).
2. When an account runs out and **no account is left to switch to**, a **fallback agent takes the
   work over**: a chain of agents, each continuing from the same workflow checkpoint.
3. The same switch for an agent **inside a pipeline stage**, keeping the stage's context.
4. The operator chooses the agents and their order **on their own machine, per task**; paid
   agents get spending limits — machine-wide, per project, or per task at the moment of the
   switch. **Everything is reachable over MCP** as well as the app and the CLI.

## Decisions (operator, 2026-10-06)

| ID | Decision |
|---|---|
| D-1 | The switch to the next agent is **automatic** when the provider's accounts are exhausted; the person is notified, never asked first. |
| D-2 | Paid agents (API keys, billed per token) are allowed, under a **daily ceiling**; ceilings can also be set per project and per task, at the switch. A Claude subscription is never handed to a third-party agent (the existing subscription guard). |
| D-3 | Chains and their order are **the operator's, per machine, per project and per task**. A preset `claude-code → codex → kimi-code → hermes` ships; no chain is applied until one is set. |
| D-4 | Every setting and action has an MCP tool, beside the app and the CLI. |

## What exists (receipts)

- **Context carrier.** Project Observatory memory/0.1 workflows: checkpoint, constraints, declared
  credentials by name, git snapshot, related records; the handoff pack is provider-neutral except
  `to.provider` (required) and an optional same-provider `transcript` pointer, which another
  provider's executor ignores. The engine refuses an accept from another provider than the
  offer names (store/workflow.py:1220), so a cross-provider move is a handoff created with
  `to: {provider: "codex", …}`. Offers without the lease token: reasons `limit`, `crash`,
  `restart` only, after the silence window, at most one per interval; `force` is terminal-only
  (project-observatory session, 2026-10-06). Nothing on the engine side is in flight for
  N-021/N-022; build against its `main`.
- **Same-provider continuation** — `switchboard continue` (SB-52, v0.6.2): reads the workflow,
  offers the handoff (`--reason limit`), launches the account's session with
  `OBSERVATORY_WORKFLOW_ID`, `OBSERVATORY_HANDOFF_ID`, the Observatory MCP server and a first
  prompt to accept; never reads the old transcript, never accepts on the session's behalf.
- **Limit evidence** (SB-41) and **rotation** pick another account of the same provider and pool;
  when none is eligible, nothing happens today (SB-25: the automatic wake is unbuilt).
- **Agent catalog** (SB-56): 30 agents with level `mcp` / `proxy` / `launch`, `agents connect`
  and `agents launch`; Codex, Kimi Code, Hermes, OpenClaw, ZCode are `proxy` level. Third-party
  agents run only on API-key accounts.
- **Paid-key ceilings.** Observatory's OpenRouter door issues every working key with a USD
  ceiling (`openrouter.py issue --limit`, `limit --set`, `ping` reports spend); keys land in the
  vault by name.
- **Fabric.** Pipelines (AR-5) are designed, not built; the stage binds a capability and Fabric
  resolves the executor at run start and journals it. In-work switching is ADR-0052
  (`switchCoordinator.ts`): same provider, runtime and scope only — "No cross-provider fallback is
  implied"; "a context handoff to a new conversation requires its own explicit admission". Fabric
  launches its executors itself (PTY, managed launch). Kimi as a Fabric executor is CO-220; Hermes
  is connected over ACP (ADR-0119) (fabric session, 2026-10-06).

## Ownership of the seam — one launcher per session

A session has exactly one owner of its launch, and that owner decides its switch:

- **A session Switchboard launched, or the ordinary Claude Code / Codex** — Switchboard decides
  and launches the next executor (this packet).
- **A Fabric pipeline stage** — Fabric decides, journals and launches (a new Fabric ADR extending
  ADR-0052 to cross-provider context handoff with its own admission). Switchboard supplies the
  mechanics as a tool: the next executor's account and key, the handoff offer, and
  `switchboard_continue` with `launch: false` returning what to start, so Fabric never gets a
  second launcher.

## Design

### Executors and chains (SB-71)

- **Executor** = agent (catalog id) + credential source: a saved Switchboard account (Claude Code,
  Codex: subscription or API key) or a paid key by name (Observatory vault, e.g. an OpenRouter key
  for Kimi or Hermes). Validated against the catalog: the agent must be able to load an MCP server
  (to accept the handoff) and take a first prompt non-interactively; agents that cannot are not
  offered as chain members.
- **Chain** = ordered executors, stored in the data folder with three scopes, the narrowest wins:
  machine default → project (SB-53 projects) → task (one workflow id, set at or before the
  switch). Presets ship (`claude-code → codex → kimi-code → hermes`); none is active until chosen.
- Surfaces (D-4): the app (a Fallback section in Automatic switching and in each project), CLI
  `switchboard chain list|set|clear [--project|--workflow]`, MCP `switchboard_chain_get`,
  `switchboard_chain_set`; the read tool is in the read-only set.

### Spending ceilings (SB-72)

- Three ceilings, all optional, the strictest applies: **daily** machine-wide, **per project**,
  **per task** (given at the switch: CLI flag, MCP argument, app field).
- Enforced at the provider, not by counting tokens locally: each paid executor runs on a key
  issued for its scope through Observatory's door (`openrouter.py issue --limit … --to
  vault:<project>/…`), so a runaway agent stops at the provider's 402 even if Switchboard is not
  running. A daily reset needs the door extended (OpenRouter keys can carry a reset period; the
  door exposes only a total ceiling today) — that extension is a pull request to
  project-observatory-dashboard, never a bypass with the provisioning key.
- Before each switch to a paid executor Switchboard reads the key's spend (`ping`); a ceiling
  already reached skips that executor and moves down the chain, logged.

### Cross-provider continue (SB-70, N-018 step 2)

- `switchboard continue <wf> --agent <id> [--account <id> | --key <name>]`: the handoff is created
  with `to.provider` = the executor's provider; `PROVIDER_MISMATCH` is removed and the engine's own
  accept check stays the authority.
- Launch per agent: the Observatory MCP server and Switchboard's MCP in the agent's own config
  (Claude `--mcp-config`, Codex `[mcp_servers]`, Kimi Code `mcp.json` with
  `KIMI_CODE_TRUST_WORKSPACE=1` for headless, Hermes per its catalog notes), the workflow and
  handoff ids in the environment, and the first prompt through the agent's non-interactive flag.
  The per-agent launch recipe becomes catalog data (`continue_recipe`), tested per agent with a
  synthetic Observatory.
- The pack is provider-neutral; the first prompt tells the executor to accept, read constraints
  first, continue from the checkpoint and write checkpoints after each step.

### Automatic fallback (SB-73; absorbs SB-25)

- Trigger: limit evidence on the executor of an open workflow (SB-41) **and** rotation finds no
  eligible account of that provider for it.
- Action (D-1): take the next executor in the chain whose credential is usable and whose ceiling
  is not reached; offer the handoff (`--reason limit`, the engine's silence and rate rules); launch
  it; notify (app notice, tray, log `fallback` event, MCP status). One offer per workflow at a
  time; the old session is never killed; an executor that fails to start is logged and the chain
  moves on; an empty or exhausted chain is reported, not looped.
- Never: replay a streaming request, put a credential into the pack or prompt, hand a Claude
  subscription to a third-party agent.

### Pipelines (SB-74 here; Fabric ADR under RM-06)

- `switchboard_continue` MCP tool (with SB-64) and `launch: false`, so a pipeline owner can ask
  for the next executor, its credential and the handoff without Switchboard starting a process.
- task-pipeline stage checkpoints (N-024, done) are the stage's context; the Fabric ADR decides
  who switches a stage and journals it.

## Order (the answer to "when")

1. **SB-70** cross-provider continue, Claude Code ↔ Codex (smallest step, unblocks the rest; the
   engine is ready).
2. **SB-71** executors and chains (machine/project/task) with every surface, MCP included.
3. **SB-72** spending ceilings, with the door's daily-reset extension in project-observatory.
4. **SB-73** automatic fallback on top of 1–3; Kimi Code and Hermes recipes join the catalog with
   live acceptance on an operator API key.
5. **SB-74** the pipeline tool surface, after Fabric's ADR.

Before SB-70: SB-62 (P1) — the sign-in rename/duplicate bug — so the accounts the chain uses
are not duplicated across pools.

## Acceptance

Synthetic Observatory and fake agents in tests for every refusal and every step; then live, on the
operator's machine: a Claude Code workflow interrupted by a real limit continues in Codex from its
checkpoint; then in Kimi Code on a capped key, stopping at the ceiling. N-021/N-022 (joint
acceptance, one-executor fencing) are run against these builds.
