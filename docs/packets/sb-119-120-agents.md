# SB-119 / SB-120 — agents you can add to switching, and OpenCode on OpenRouter

Operator, 2026-10-14. Requests:
- «Давай сразу поддержку OpenCode сделаем у нас в Switchboard и в наших пайплайнах
  переключения сессий; модели давай из OpenRouter выбирать, но будем вот эти рекомендовать»,
  followed by five model links;
- an Orca agent-settings screen («и вот так хочу, чтобы можно было добавлять агентов новых к нам
  в свитчер, UI/UX причесать, баги все исправить и убедиться, что доделано всё до конца
  максимально качественно»);
- Fabric gets its own settings screen (default agent, agents, Keep Awake). That ask went to the
  Fabric session on 2026-10-14; it is Fabric's work, not this packet's.

## What exists (measured on main `bd60530`)

- `catalog/agents.json` lists 31 agents, each with `binary`, `install`, `url`, MCP, endpoint and
  OpenRouter recipes. OpenCode already launches on the saved OpenRouter key
  (`agents launch opencode --openrouter`).
- Only Claude Code, Codex, Kimi Code and Hermes can continue a workflow or stand in a fallback
  chain. OpenCode cannot.
- The Agents page is one long column of cards: what agents can do, Connect, OpenRouter key,
  Hermes model, and a grid of 31 tiles that all say "Set up". Nothing tells which agents are
  installed. The model for OpenRouter launches is a free-text field.
- `launch::find_program` already finds a CLI on PATH and in the usual per-user and Homebrew
  folders.

## The five recommended OpenRouter models (verified on openrouter.ai/api/v1/models, 2026-10-14)

All five accept tool calls. Prices are in USD per 1M input tokens.

| Model | Context | Input |
|---|---|---|
| `xiaomi/mimo-v2.6-pro` | 1.05M | 0.44 |
| `anthropic/claude-haiku-5.5` | 1M | 0.10 |
| `google/gemini-3.8-flash` | 1M | 0.75 |
| `qwen/qwen3.8-flash` | 1M | 0.15 |
| `deepseek/deepseek-v4.1-flash` | 1M | 0.04 |

Reference for choosing coding agents: https://artificialanalysis.ai/agents/coding-agents.

## Decisions

| # | Decision | Why |
|---|---|---|
| D-1 | The catalog carries `openrouter_recommended`: the five ids above, in that order. The model picker shows them first, marked *Recommended*. Below them it lists every OpenRouter model that accepts tools, read from `GET https://openrouter.ai/api/v1/models` (public, no key, cached 6 h, 10 s timeout), and also takes any id typed by hand. | "Choose from OpenRouter, recommend these". The live list keeps up with OpenRouter, the recommendation is ours, and a typed id still works offline. |
| D-2 | **Installed** means the agent's `binary` is found by `find_program`. Detection runs when the Agents page opens or *Refresh* is pressed, never on a timer. | Same lookup as launches; no background cost (LC-04). |
| D-3 | Each agent has a **Takes over work** state (named so to keep it apart from account switching), Enabled or Disabled. The default is Enabled for an installed agent that can continue, and stays Disabled while the agent is not installed. A disabled agent is skipped in fallback chains (reason `agent_disabled`) and refused by `continue --agent`. It is saved in `<data>/agents-settings.json` (0600) and carried in backups as a setting. | "Add agents to our switcher" is a decision per agent, not a property of the catalog. |
| D-4 | The **Default agent**: *Auto* or one installed, enabled agent that can continue. `switchboard continue` without `--account` uses it: Hermes and OpenCode on the OpenRouter key, Claude Code, Codex and Kimi Code on the account a one-step chain of that agent would choose now. Fallback chains keep their own order (the default does not seed them: amended 2026-10-14 after review, since a chain is the operator's explicit order, SB-71). The CLI and MCP publish it with `schema` and `default_ready`, so Fabric can take it as the machine default (Fabric keeps its per-project override). | One home for the machine's default; Fabric reads it. |
| D-5 | OpenCode joins continuation and chains the way Hermes did (SB-107): its own config for this launch only, the Observatory MCP, the handoff prompt as its first message, and the model chosen per D-1. The saved key comes through the environment. The person's own OpenCode login, sessions and config are never written. Exact flags: research note in this packet's appendix. | The operator's first ask. |
| D-6 | The Agents page is rebuilt in the order of the operator's reference. **Default agent** chips come first. **Installed · N detected** follows, one row per agent: name, the command Switchboard runs, Enabled/Disabled, Default or *Set default*, a docs link, and a disclosure with connect steps and the model choice. **Available to install · N** gives install commands with a copy button. *Connect an agent* (MCP) and the *OpenRouter key* stay as two compact sections below. The Hermes model moves into Hermes's row. | The operator's reference, in this app's own design language: tokens, buttons and spacing already in `style.css`, no new visual system. |

## Requirements

| Requirement | Verified by |
|---|---|
| REQ-1 The recommended models come first; the live list is tool-capable only, cached, and fails soft to the recommended plus typed ids | runtime `openrouter_models` tests; ui-logic |
| REQ-2 Installed or not is decided by `find_program`; Refresh re-detects | runtime agents tests |
| REQ-3 Enabled/Disabled and the default persist, are backed up, and are refused for an agent that cannot continue | runtime and CLI tests |
| REQ-4 A disabled agent is skipped in chains and refused by `continue` | fallback and continue tests |
| REQ-5 OpenCode continues a workflow on the OpenRouter key with the chosen model, isolated from the person's own OpenCode setup | launch tests (script contents); live check by the operator |
| REQ-6 The rebuilt Agents page | SCN-057…SCN-060; WebKit demo with screenshots; ui-logic |
| REQ-7 CLI `agents list` (installed, enabled, default, command), `agents enable/disable`, `agents default`, `agents models`; MCP mirrors the read side | CLI and MCP tests |

## State at the interruption (2026-10-14, usage limit)

Done on `feat/sb-119-agents`, not yet gated:
- catalog: `openrouter_recommended` and an OpenCode recipe with `OPENCODE_CONFIG_CONTENT` (env key wins over `auth.json`; the model is declared);
- `agent_settings.rs`: installed detection, enable/disable, default, `SWITCHABLE`, the OpenRouter model list;
- launch: OpenCode continuation, `{model}` in `extra_env`, agent install folders in `find_program`;
- fallback: an OpenCode step and the `agent_disabled` skip;
- `continue`: the default agent and the disabled refusal;
- ops and CLI: `agents status|enable|disable|default|models`;
- backup and uninstall carry `agents-settings.json`.

Targeted tests pass: 76 in runtime. `agents status` live: 11 agents detected, 374 models.

Next:
1. The Agents page UI (plan D-6): Tauri commands `agents_overview`, `agent_enable`, `agent_default`, `openrouter_models`; adapter, demo and types; ru strings.
2. A model-picker dialog.
3. Tests for the OpenCode continuation script and fallback.
4. Regenerate AGENT-SUPPORT.md; docs: CLI, CONTRACTS, AGENTS lifecycle (the model fetch is a new network call).
5. `./scripts/check.sh`, verifier review, WebKit demo, then release 0.6.31.
