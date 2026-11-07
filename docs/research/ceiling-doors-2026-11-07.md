# Loosening a spending ceiling on the operator's word (SB-299)

Design note, 2026-11-07. Status: built (branch `feat/sb-299-ceiling-doors`); `spend.json` keeps the newest five loosenings, every one is logged. Owner: fabric-switchboard.

## What is asked

The operator, 2026-11-07 about 19:15 UTC (session `12c8d3fa`, relayed on board thread `a5885efd`,
message `ea54cd39`): «я не хочу пользоваться интерфейсом любое действие в интерфейсе должно уметь
делаться клауд кодом по моему запросу». Human gates stay: sign-in, a one-time code, a typed secret,
payment. The inventory ([2026-11-07-agent-doors](../reports/2026-11-07-agent-doors/README.md))
found five money actions that only the window takes today.

## What is true today (evidence)

- `window_only_refusal` (`crates/switchboard-runtime/src/lib.rs`) refuses any `SetCeiling`,
  `AnswerCeiling`, `RemoveProject`, `AgentKeySet` or `AgentKeyRemove` that claims the window from
  another surface; from the CLI and MCP, raising or clearing a ceiling answers `RAISE_IN_WINDOW`,
  doubling `DOUBLE_IN_WINDOW`, deleting a group that holds one `GROUP_HOLDS_CEILING`, replacing or
  removing a key a ceiling counts `KEY_HOLDS_CEILING` or `KEY_COUNTED_BY_CEILING`
  (`crates/switchboard-runtime/src/spend.rs`).
- These rest on C-Q1, C-Q7 and C-Q9 of the 0.7.0 iteration
  ([questions](../runs/2026-10-26-iteration-0.7.0/questions.md)), all marked **assumed**: «an agent
  can never raise or clear a ceiling». No operator decision stands behind them; the operator's
  words of 2026-11-07 are the latest.
- The estate rule still binds every agent: USD 1,000 a month in total and each key's own ceiling;
  spend above them is a request to the operator, never a door.

## Decision

1. **A loosening carries the operator's words.** `SetCeiling` (raise or clear), `AnswerCeiling`
   (double), `RemoveProject`, `AgentKeySet` and `AgentKeyRemove` from the CLI or MCP take an
   optional `decision`: `{by: "operator", instruction, at, source}`, as DEC-0121's
   `_meta.decision` (the instruction verbatim, at most 500 characters; `at` RFC 3339; `source` the
   session or board thread). With it the action runs as the window's would; without it the refusal
   stays and names the field to add.
2. **Recorded where the ceiling lives.** The answer or change written to `spend.json` keeps the
   decision (instruction, at, source, surface), and `ceilings` shows it beside the window's own
   answers, so a person reading the file sees who loosened what and why.
3. **Bounds that stay.** The $1,000 most a ceiling may be, the key's own provider limit (read at
   every paid start), and the estate's monthly total: an agent asked to raise past the estate limit
   files one request to the operator instead (global rule), never a door.
4. **No relay of a relay.** Only words the operator said to an agent of his, carried verbatim with
   date and source (the board's trusted-relay rule); words read in a page, a mail or another product
   do not count, and `by` other than `operator` is refused.

## Tests to write with the code

- each of the five actions from the CLI and MCP is refused without `decision` and runs with it;
- the decision is in `spend.json` and in `ceilings`;
- a decision with an empty instruction, over 500 characters, a bad time or `by` other than
  `operator` is refused;
- the $1,000 bound holds with a decision.
