# Release iteration 2026-10-24: UI/UX and the whole product (0.6.82)

run: task-pipeline iteration (references/iteration.md), skill 1.102.0
operator ask (2026-10-24, verbatim): «Я хочу, чтобы ты прогнал типа итерацию, UI UX и улучшение всего продукта.»
model: claude-opus-5-5 (1M context), the most capable available; inherited by every subagent
loop: one release iteration; the next one starts on the operator's go
branch: iter/0.6.82-ui-ux, worktree ~/DATA/_worktrees/sb-iter-0682
before it: release 0.6.81 (SB-185, SB-186) on branch fix/sb-185-vault-lock

## Stage 0 brief

- Goal: one release that makes the window, the CLI and the MCP server better for the person using
  them, chosen from a survey of every area, not fix by fix.
- Routes: super-ux (behaviour, docs/ux/scenarios.md), sheleg-design (look, motion),
  copywriting (words, docs/brand/), evidence-docs (claims), task-pipeline (delivery).
- Decided without asking (the operator's rules: decide by recorded decisions, then the goal,
  then the safest reversible option): scope is what the survey finds and an agent can close;
  items that need the operator (live accounts, a Windows host, signing decisions) stay rows;
  the release is the agent's (AGENTS.md, operator rules "Releases are yours").
- Surface class: product (the desktop window is the product's main surface).

## Source ledger

| Source | Read | State |
|---|---|---|
| docs/evidence/backlog.md | open rows listed 2026-10-24 | current |
| docs/HANDOFF.md | top entries | current |
| Fabric board (fabric-switchboard project) | messages since 2026-10-23 12:00 | no open request addressed here |
| GitHub issues | #128 (analytics before consent), #36 (COM-11) | open |
| switchboard.log | control_slow, transaction_held 2026-10-22..24 | led to SB-185, SB-186 (0.6.81) |
| docs/ux/*, docs/brand/*, docs/design/switchboard/director-record.md | by the survey agents | see survey/ |

Contradictions: none found yet.

## Survey (wave 0)

Four read-only agents, outputs in survey/: ux.json (code vs scenarios), visual.json (WebKit walk,
both themes, 740 and 1100 px), copy.json (brand pack, locales, CLI help, MCP descriptions),
product.json (backlog, CLI/MCP/window parity, docs truth, log, issue #128).
