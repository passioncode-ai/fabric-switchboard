# Switchboard CLI — fallback when the MCP server is absent

The `switchboard` CLI reads and writes the same account store as the desktop app and the
MCP server. With the app or `switchboard serve` running it talks to that owner; otherwise
it takes the store's lock for one command and releases it. It never prints a credential.

`--json` wraps a success as `{"ok": true, "data": …}` on stdout and a failure as
`{"ok": false, "error": "…"}` on stderr. Exit status: 0 success, 1 operational failure,
2 invalid arguments. `--data-dir ABSOLUTE_PATH` goes before the subcommand and selects a
separate data folder; leave it out unless the operator named one.

## Equivalents

| MCP tool | CLI |
|---|---|
| `switchboard_status` | `switchboard --json status` (runtime mode and proxy only), `switchboard --json current` (signed-in CLI accounts), `switchboard --json rotation status` |
| `switchboard_accounts` | `switchboard --json accounts list` |
| `switchboard_usage` | `switchboard --json usage` (cached observations); `switchboard --json usage ACCOUNT_ID` asks the provider for that account |
| `switchboard_switch`, target `route` | `switchboard accounts select ACCOUNT_ID --provider claude --pool work` |
| `switchboard_switch`, target `claude_cli` | `switchboard accounts activate ACCOUNT_ID` — the operator's explicit OK first |
| `switchboard_project_context` | `switchboard --json project show --path /path/to/project` |
| `switchboard_project_set` | `switchboard project set --path /path/to/project --account ACCOUNT_ID --expires-in-hours 8` (`--target managed` or `claude-cli`; `--paused` saves it switched off) |
| `switchboard_project_remove` | `switchboard project remove --path /path/to/project --provider claude` |
| `switchboard_project_apply` | `switchboard project apply --path /path/to/project` (`--global` only after the operator's OK) |
| — | `switchboard project list` — every rule with its state: active, paused or expired |

`project show`, `set`, `remove` and `apply` default to the current folder when `--path` is
left out; a relative `--path` is refused.

## Reading the results the same way

- `switchboard usage` without an id never contacts a provider; an unknown value is shown
  as unknown, never as zero.
- `project apply` prints one line per provider with the same fixed message the MCP
  result carries; with `--json` each entry has the `action` from the table in `SKILL.md`.
- `accounts select` is the `route` switch: it reaches every managed session of that pool
  from its next request, not a native session.
- `accounts activate` changes the ordinary Claude Code login for every `claude` session on
  the machine. Ask first.
