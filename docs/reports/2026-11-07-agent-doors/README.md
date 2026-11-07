---
report: every window action of Fabric Switchboard against its agent door (SB-299)
project: fabric-switchboard
date: 2026-11-07
source: operator 2026-11-07 about 19:15 UTC, session `12c8d3fa` (fabric-agents-database), relayed on board thread `a5885efd` message `ea54cd39` («я не хочу пользоваться интерфейсом любое действие в интерфейсе должно уметь делаться клауд кодом по моему запросу»)
---

# Every window action against its agent door

Main at `f95d448`. The window calls 92 Tauri commands (`#[tauri::command]` in `src-tauri/src/main.rs`).
Each was traced to the runtime `Operation` it builds and to the CLI command (`crates/switchboard-cli/src/main.rs`,
the `run` match) and MCP tool (`crates/switchboard-cli/src/mcp.rs`, `tools()` and its dispatch) that reach the same
`Operation`. The CLI and MCP parity test (`every_cli_command_has_an_mcp_tool_or_a_reason`) covers the CLI against MCP,
never the window; this is the first reading of the window against both.

## No door at all

| Window action | Tauri command | Why there is none | What a door needs |
|---|---|---|---|
| GitHub owners, repositories, clone | `github_owners`, `github_repositories`, `github_clone` (`src-tauri/src/main.rs:521`, `:530`, `:539`) | the `Github*` operations are reached only from the window | `switchboard github owners\|repos\|clone --into <absolute folder>` and an MCP tool |
| Link the CLI into `~/.local/bin` | `link_cli` (`:1151`) | runs `agents::link_bundled_cli` in the app | `switchboard link`, or an onboarding step |
| Usage analytics on or off | `set_analytics` (`:1183`) | a runtime method, not an `Operation` | `Operation::Analytics { enabled }`, CLI `analytics on\|off`, MCP tool |
| Open at login on or off | `set_login_item` (`:1229`) | lives in the app process (`residency::set`) | an operation that writes the same login entry, with a read |
| Install updates automatically, check now, restart to update | `set_auto_update` (`:1245`), `check_for_updates` (`:1265`), `restart_to_update` (`:1260`) | the app's `updates` module, outside the runtime | the app exposes them over the control socket; a restart goes through the lifecycle broker |
| Cancel a PassionCode.ai connection | `passioncode` action cancel (`:605`) | `VestaCancel` has no CLI or MCP route | `org connect --cancel` and an MCP action |

Reads with no door: `pending_logins`, `analytics_status`, `login_item`, `update_status` (fields in `status`, or `login list`).

## Refused outside the window on purpose (money)

| Action | Refusal |
|---|---|
| Raise or clear a spending ceiling | `RAISE_IN_WINDOW` (`crates/switchboard-runtime/src/spend.rs:87`) |
| Double a reached ceiling | `DOUBLE_IN_WINDOW` (`spend.rs:62`) |
| Delete a group that holds a ceiling | `GROUP_HOLDS_CEILING` |
| Replace or remove an agent key a ceiling counts | `KEY_HOLDS_CEILING`, `KEY_COUNTED_BY_CEILING` |
| Any of the five claiming the window over the control socket | `window_only_refusal` (`crates/switchboard-runtime/src/lib.rs:3553`) |

The operator's decision names only sign-in, a one-time code, a typed secret and payment as human gates. Raising a
ceiling is not a payment: it permits spending up to a new amount. Proposed door: the CLI and MCP take the raise, the
double and the deletion when the call carries the operator's words verbatim with date and source (as DEC-0121's
`_meta.decision`), recorded with the ceiling's answer; the estate limit of USD 1,000 a month and each key's own
ceiling still bind, and a raise above them stays a request to the operator. This changes C-Q1 and C-Q7 to C-Q9 of the
spending design, so it is the operator's to confirm before code: asked on the board with the decision recorded in
SB-299.

## Partial

| Action | What is missing |
|---|---|
| Create a group with its organizations (`save_project`) | MCP edits an existing group only (`ProjectEdit`); the CLI `project save` creates but takes no organization |
| PassionCode.ai (`passioncode`) | connect and disconnect are CLI only; cancel is nowhere |
| Delete a group, save or remove an agent key | CLI only, and refused when a ceiling is involved (above) |
| How one agent connects (`agent_connect`) | MCP only approximates it through `switchboard_onboarding` |
| Set a ceiling, answer a ceiling | first or lower, and keep, work from CLI and MCP; raise, clear and double are the window's (above) |

## CLI only on purpose

`mcp_parity::PARITY` gives MCP no tool for capture, import from Claude Swap, add with a secret, remove an account,
restore a backup, remove a Kimi account, save or remove an OpenRouter key, delete a group (reason PERSON), and for
`launch`, `kimi launch`, `agents launch` (reason LAUNCH, SB-64 and SB-74). An agent in a terminal reaches all of them;
an agent with MCP alone does not. Whether the CLI counts as the agent's door, or each needs an MCP tool behind the
operator's words, is the second question for SB-299. The one MCP-only action is the Kimi sign-in state: the CLI has no
`kimi status`.

## Human gates that stay human

- Sign-ins: Claude and Codex (`login begin`), Kimi (`kimi login`, `kimi relogin`), PassionCode.ai (browser consent).
  The agent begins, polls, finishes and cancels; the person signs in.
- Typed secrets: an API key, a setup token or auth JSON (`accounts add --secret-stdin`), an OpenRouter key
  (`agents openrouter set --key-stdin`). They travel on stdin only; an agent issues keys through Observatory's doors
  and could pass a vault slot name instead of a value.
- Payment and spend above the limits.

## Next

1. Doors for the actions with none, in this order: analytics (built: CLI `analytics on|off`, MCP
   `switchboard_analytics`) and update reads; `github owners|repos|clone`
   (built: CLI `github`, MCP `switchboard_github` and `switchboard_github_clone`); `link`; update and login-item switches
   over the control socket; cancelling a PassionCode.ai connection (built: CLI `org cancel`, MCP
   `switchboard_organization_set` `passioncode_cancel`, and `passioncode_disconnect` with it).
2. The ceiling question to the operator (above), then the door with the operator's words.
3. MCP tools for the PERSON group behind the operator's words, if the operator wants MCP and not only the CLI.
