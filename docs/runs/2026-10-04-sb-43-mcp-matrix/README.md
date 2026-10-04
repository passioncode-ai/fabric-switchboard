# SB-43 — real-client MCP compatibility matrix (2026-10-04)

Board row [SB-43](../../evidence/backlog.md); packet SYS-MCP in
[the system review](../../reports/2026-10-04-system-review/raw/system.md). Script:
[`scripts/mcp_compat.py`](../../../scripts/mcp_compat.py). Evidence:
[`mcp-compat-2026-10-04.json`](../../evidence/mcp-compat-2026-10-04.json).

## Matrix (`switchboard 0.5.4`, `mcp --read-only`, empty scratch data folder)

| Client | Version | Asked | Server answered | `tools/list` | `switchboard_usage` call | Result |
|---|---|---|---|---|---|---|
| MCP Inspector CLI | `@modelcontextprotocol/inspector@2.9.0` | `2025-11-25` | `2025-06-18` | 4 read-only tools | no error | PASS |
| TypeScript SDK v1 | `@modelcontextprotocol/sdk@1.32.0` | `2025-11-25` | `2025-06-18` | same | no error | PASS |
| TypeScript client v2 (2026-07-28 line) | `@modelcontextprotocol/client@2.3.0` | `2025-11-25` | `2025-06-18` | same | no error | PASS |
| Python SDK | `mcp==2.3.0` | `2025-11-25` | `2025-06-18` | same | no error | PASS |
| Claude Code | 2.1.289 | — | — | — | — | NOT_RUN |
| Codex CLI | 0.160.0 | — | — | — | — | NOT_RUN |

After every client closed stdin, no server process was left running. Every client asks for
`2025-11-25`, accepts the server's `2025-06-18` and works; the v2 client, built for the stateless
2026-07-28 specification, still opens with the `initialize` handshake.

## Decisions

- **No new protocol version is advertised.** `2025-11-25` would claim semantics (tasks, icons,
  elicitation changes) the dispatch does not implement; the fallback is proven to work, so the
  server keeps `2025-06-18`, `2025-03-26`, `2024-11-05` (SYS-MCP: "never advertise a modern date
  on unchanged legacy dispatch").
- **The proving call reads only the scratch store.** `switchboard_status` and
  `switchboard_accounts` read this machine's ordinary Claude Code and Codex sign-ins, so they are
  listed, never called (AGENTS.md: no test reads global credentials). A first exploratory call of
  `switchboard_status` did read the current sign-in's identity metadata; its logs were deleted
  and the script was built to avoid it.
- **Claude Code and Codex CLI are NOT_RUN.** Both would need their user MCP configuration changed
  or a model call on the operator's account; they belong to operator acceptance (SB-15).
- Malformed and oversized lines and end of input are covered by the synthetic tests
  (`handshake_lists_tools_and_rejects_malformed_messages`, `LINE_LIMIT`, "End of input ends the
  server").

## Re-run

`python3 scripts/mcp_compat.py target/debug/switchboard --json out.json` (needs `npx`, `npm`,
`node`, `uv` and network for the pinned clients; exit 0 only when every client passes and no
server outlives its client). Not part of `./scripts/check.sh`: it downloads clients.
