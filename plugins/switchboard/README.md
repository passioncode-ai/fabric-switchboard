# Switchboard plugin for coding agents

This plugin connects a coding agent to [Fabric Switchboard](https://passioncode.ai/switchboard/),
the desktop app that keeps several Claude Code and Codex accounts on one machine, routes
managed sessions through a local proxy and rotates off an exhausted account.

| Field | Value |
|---|---|
| Version | follows Switchboard (`plugin.json`) — 0.6.0 at the time of writing |
| Ships | `.mcp.json` (server `switchboard`, runs `switchboard mcp` over stdio) and the skill `switching-accounts` |
| Skill purpose | read status and remaining usage honestly, switch a managed session's account, apply optional project rules and report them, change the ordinary Claude Code login only on the operator's explicit OK |
| Mutations | only through Switchboard's own tools; nothing in the plugin writes files |
| Network | none of its own; `switchboard_usage` with `refresh: true` asks the provider for one account |
| Secrets | never accepted, printed or read; no tool returns a credential |

## Requirements

Switchboard 0.4 or later on the same machine, with the `switchboard` CLI on `PATH`. The
desktop app's Agents panel links the bundled CLI to `~/.local/bin/switchboard` on macOS;
on Windows keep `switchboard.exe` from the download on `PATH`. Without the CLI
the MCP server cannot start, and the skill says so instead of guessing.

## Install

Claude Code:

```sh
claude plugin marketplace add passioncode-ai/fabric-switchboard
claude plugin install switchboard@switchboard
```

Or through the PassionCode launcher once this plugin is released as a member:
`npx @passioncode-ai/passioncode@latest update`.

Codex registers the server directly:

```sh
codex mcp add switchboard -- switchboard mcp
```

Restart the agent afterwards; plugins and skills load at session start. Sessions that
Switchboard launches itself already get the server when the CLI is available, without
the plugin.

## Use

The skill triggers on requests such as "switch account", "how much quota is left",
«переключи аккаунт», «сколько осталось лимита». Invoke it explicitly as
`/switchboard:switching-accounts` when installed as a plugin, or `/switching-accounts`
from a skills directory.

## Validate

From the repository root:

```sh
python3 scripts/check_plugin.py
python3 -m unittest scripts/test_check_plugin.py
claude plugin validate ./plugins/switchboard --strict
claude plugin validate . --strict
```

`check_plugin.py` holds what `--strict` does not read: one version across the manifests
and the skill, the skill's front matter read strictly, every tool name in the skill
checked against `crates/switchboard-cli/src/mcp.rs`, every apply action documented, no
home paths, evals that parse.

## License

Open source under the GNU AGPL-3.0; a commercial license is available for use that does not
meet the AGPL's terms — [passioncode.ai/business](https://passioncode.ai/business/). SPDX:
`AGPL-3.0-only OR LicenseRef-PassionCode-Commercial`. Plugin 0.4.0 and earlier were released
under PolyForm Noncommercial or Internal Use.
