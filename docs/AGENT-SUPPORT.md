# Agents Switchboard works with

Generated from [`catalog/agents.json`](../catalog/agents.json) by `scripts/agents_doc.py`; do not edit by hand. Facts and sources: [research, 2026-10-05](research/agents-2026-10-05.md). Ranking: https://openrouter.ai/apps (global ranking, read 2026-10-05).

Every agent here can use Switchboard at one of three levels:

- **Tools only** — it registers `switchboard mcp`: it reads which account handles requests and how much quota is left, switches the account of a managed session, and knows the project of its folder.
- **Through Switchboard** — it can also send its model requests to Switchboard's local proxy (`http://127.0.0.1:<port>/claude/<pool>` for the Anthropic Messages API, `…/codex/<pool>/v1` for the OpenAI Responses and Chat Completions APIs), which serves the pool's selected account and switches accounts for it. It is set up once in the agent's own config; `switchboard agents connect <id>` prints the exact lines.
- **Launch from Switchboard** — configured by environment alone, so `switchboard agents launch <id> --pool <pool> --dir <folder>` (or *Agents → Other agents → Set up → Launch*) starts it in Terminal. Agents set up once in their config launch the same way.

**OpenRouter key.** Agents with an OpenRouter recipe also launch on one OpenRouter key saved in Switchboard (`switchboard agents openrouter set --key-stdin`, or *Agents → OpenRouter key for agents*): `switchboard agents launch <id> --openrouter [--model <id>]`. The key reaches the agent only through its environment, read by the session script when it starts; no config file, argument or script holds it.

**Accounts.** A subscription sign-in (Claude.ai, a Claude setup token, ChatGPT) is for the provider's own client — Claude Code or Codex — by the providers' terms. Other agents get their own proxy key (`switchboard agents key`, derived from the session capability, never stored in their config), and the proxy serves that key only from **API-key accounts**; a pool whose selected account is a subscription sign-in answers 403. Projects still apply: a project's account starts only inside its folders.

| # | Agent | Kind | Level | MCP | Anthropic endpoint | OpenAI endpoint | Headless |
|---|---|---|---|---|---|---|---|
| 1 | [Hermes Agent](https://github.com/NousResearch/hermes-agent) · #1 on OpenRouter | cli | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `hermes -z "<prompt>"` |
| 2 | [Kilo Code + Kilo CLI](https://github.com/Kilo-Org/kilocode) · #2 on OpenRouter | cli+ide | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `kilo run --auto "<prompt>"` |
| 3 | [Claude Code](https://code.claude.com/docs/en/setup) · #3 on OpenRouter | cli+ide | Launch from Switchboard | yes | yes (`ANTHROPIC_BASE_URL`) | no | `claude -p "<prompt>"` |
| 4 | [Cline + Cline CLI](https://github.com/cline/cline) · #4 on OpenRouter | cli+ide | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `cline "<prompt>"` |
| 5 | [Freebuff](https://github.com/CodebuffAI/freebuff) · #5 on OpenRouter | cli+desktop | Tools only (`switchboard mcp`) | yes | no (own service) | no | — |
| 6 | [omp (oh-my-pi)](https://github.com/can1357/oh-my-pi) · #6 on OpenRouter | cli | Through Switchboard (set up once in the agent) | yes | yes (`ANTHROPIC_BASE_URL`) | Responses, Chat Completions | `omp -p "<prompt>"` |
| 7 | [Codex CLI](https://github.com/openai/codex) · #7 on OpenRouter | cli | Through Switchboard (set up once in the agent) | yes | no | Responses | `codex exec "<prompt>"` |
| 8 | [Command Code](https://commandcode.ai/docs) · #8 on OpenRouter | cli+desktop | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `cmd -p "<prompt>"` |
| 9 | [pi](https://github.com/earendil-works/pi) · #9 on OpenRouter | cli | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `pi -p "<prompt>"` |
| 10 | [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) · #10 on OpenRouter | cli+desktop | Through Switchboard (set up once in the agent) | yes | unverified with a local endpoint | unverified | `dsh --profile headless "<prompt>"` |
| 11 | [OpenClaw](https://github.com/openclaw/openclaw) · #11 on OpenRouter | cli+desktop | Through Switchboard (set up once in the agent) | yes | yes (`ANTHROPIC_BASE_URL`) | Responses, Chat Completions | `openclaw agent exec "<prompt>"` |
| 12 | [OpenHands CLI](https://github.com/OpenHands/OpenHands-CLI) · #12 on OpenRouter | cli | Through Switchboard (set up once in the agent) | yes | yes (`LLM_BASE_URL`) | Responses, Chat Completions | `openhands --headless -t "<prompt>"` |
| 13 | [ZCode](https://github.com/zai-org/ZCode) · #13 on OpenRouter | cli+desktop | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `zcode -p "<prompt>"` |
| 14 | [Kimi Code CLI](https://github.com/MoonshotAI/kimi-code) | cli | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `kimi -p "<prompt>"` |
| 15 | [Auggie CLI](https://github.com/augmentcode/auggie) | cli | Tools only (`switchboard mcp`) | yes | no (own service) | no | `auggie --print "<prompt>"` |
| 16 | [Aider](https://github.com/Aider-AI/aider) | cli | Launch from Switchboard | no | yes (`ANTHROPIC_API_BASE`) | Chat Completions | `aider --message "<prompt>" --yes-always` |
| 17 | [OpenCode](https://github.com/anomalyco/opencode) | cli | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `opencode run "<prompt>"` |
| 18 | [Continue + cn](https://github.com/continuedev/continue) | cli+ide | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `cn -p "<prompt>"` |
| 19 | [Goose](https://github.com/aaif-goose/goose) | cli+desktop | Launch from Switchboard | yes | yes (`ANTHROPIC_HOST`) | Responses, Chat Completions | `goose run -t "<prompt>"` |
| 20 | [Gemini CLI](https://github.com/google-gemini/gemini-cli) | cli | Tools only (`switchboard mcp`) | yes | no (own service) | no | `gemini -p "<prompt>"` |
| 21 | [Qwen Code](https://github.com/QwenLM/qwen-code) | cli | Launch from Switchboard | yes | yes (`ANTHROPIC_BASE_URL`) | Responses, Chat Completions | `qwen -p "<prompt>"` |
| 22 | [Crush](https://github.com/charmbracelet/crush) | cli | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `crush run "<prompt>"` |
| 23 | [Zed agent](https://github.com/zed-industries/zed) | ide | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | — |
| 24 | [Amp](https://ampcode.com) | cli+desktop | Tools only (`switchboard mcp`) | yes | unverified with a local endpoint | unverified | `amp -x "<prompt>"` |
| 25 | [Cursor CLI](https://cursor.com/docs/cli/installation.md) | cli | Tools only (`switchboard mcp`) | yes | no (own service) | no | `agent -p "<prompt>"` |
| 26 | [GitHub Copilot CLI](https://github.com/github/copilot-cli) | cli | Through Switchboard (set up once in the agent) | yes | yes (`COPILOT_PROVIDER_BASE_URL`) | Responses, Chat Completions | `copilot -p "<prompt>" --allow-all-tools` |
| 27 | [Factory Droid](https://docs.factory.com/droid-cli/quickstart.md) | cli+desktop | Through Switchboard (set up once in the agent) | yes | yes (config) | Responses, Chat Completions | `droid exec "<prompt>"` |
| 28 | [Warp](https://github.com/warpdotdev/Warp) | cli+desktop | Tools only (`switchboard mcp`) | yes | no (own service) | no | — |
| 29 | [Kiro CLI](https://kiro.dev/docs/getting-started/installation.md) | cli | Tools only (`switchboard mcp`) | yes | no (own service) | no | `kiro-cli chat --no-interactive --trust-all-tools "<prompt>"` |
| 30 | [Devin Desktop / Devin CLI (formerly Windsurf/Cascade)](https://docs.devin.ai/desktop/getting-started.md) | cli+ide | Tools only (`switchboard mcp`) | yes | no (own service) | no | `devin -p "<prompt>"` |

## Per agent

### Hermes Agent

`hermes` · Through Switchboard (set up once in the agent) · Nous Research · license MIT

- Tools: `hermes mcp add switchboard --command switchboard --args mcp`
- Setup: `switchboard agents connect hermes --pool <pool>`; launch: `switchboard agents launch hermes --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch hermes --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`, HERMES_HOME isolated). `~/.hermes/.env` and `hermes auth` pool entries win over the launch's key, so the launch runs in its own `HERMES_HOME`; whether an empty home starts Hermes's setup wizard is unverified.
- **Accounts:** Adopts external logins by default (reads Claude Code's credential files): set auth.adopt_external_logins: false. Impersonates Claude Code when handed an sk-ant-oat subscription token; give it an Anthropic API-key account only, never a Claude subscription token.
- Notes: A trailing /v1 is stripped and /v1/messages appended; a third-party endpoint gets the key as x-api-key (the pinned adapter's api_key style). Key fields: key_env / api_key / key_cmd. The base URL comes from provider config; the pinned adapter does not read ANTHROPIC_BASE_URL itself (an SDK-level env fallback with no configured base is possible but unverified).
- Sources: [1](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/website/docs/integrations/providers.md#L1338-L1354), [2](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/agent/anthropic_adapter.py#L374-L386), [3](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/agent/anthropic_adapter.py#L455-L469), [4](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/website/docs/user-guide/features/mcp.md#L33-L39), [5](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/website/docs/reference/cli-commands.md#L125-L127), [6](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/website/docs/getting-started/installation.md#L45), [7](https://github.com/NousResearch/hermes-agent/blob/7157422022ff06f3e632d1dd394ee1253b17ad37/website/docs/integrations/providers.md#L160-L197)

### Kilo Code + Kilo CLI

`kilo` · Through Switchboard (set up once in the agent) · Kilo-Org · license MIT

- Tools: add `switchboard` (`switchboard mcp`) to `~/.config/kilo/kilo.json`
- Setup: `switchboard agents connect kilo --pool <pool>`; launch: `switchboard agents launch kilo --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch kilo --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`). A key saved with `kilo auth login` for openrouter wins over the launch's key.
- Notes: Declare a custom provider with "npm": "@ai-sdk/anthropic", options.baseURL and options.apiKey (or env); the exact top-level key is not given in the research, so no snippet. Key sent as x-api-key. A model entry needs limit; {env:VAR} resolves only in global or trusted config. The extension reads the same files.
- Sources: [1](https://github.com/Kilo-Org/kilocode/blob/831805118a8c896cc8f55449bea9435e51e6e507/packages/kilo-docs/pages/ai-providers/openai-compatible.md), [2](https://github.com/Kilo-Org/kilocode/blob/831805118a8c896cc8f55449bea9435e51e6e507/packages/core/src/plugin/provider/anthropic.ts#L19-L24), [3](https://github.com/Kilo-Org/kilocode/blob/831805118a8c896cc8f55449bea9435e51e6e507/packages/kilo-docs/pages/automate/mcp/using-in-cli.md#L16-L70), [4](https://github.com/Kilo-Org/kilocode/blob/831805118a8c896cc8f55449bea9435e51e6e507/packages/kilo-docs/pages/code-with-ai/platforms/cli.md#L575-L613), [5](https://github.com/Kilo-Org/kilocode/blob/831805118a8c896cc8f55449bea9435e51e6e507/packages/kilo-docs/markdoc/partials/install-cli.md#L6)

### Claude Code

`claude-code` · Launch from Switchboard · Anthropic · license proprietary

- Tools: `claude mcp add --scope user switchboard -- switchboard mcp`
- Setup: `switchboard agents connect claude-code --pool <pool>`; launch: `switchboard agents launch claude-code --pool <pool> --dir <folder>`
- Notes: Switchboard's primary, already-integrated client; also VS Code/JetBrains extensions and a desktop app. ANTHROPIC_API_KEY goes out as X-Api-Key, ANTHROPIC_AUTH_TOKEN as Bearer. With only ANTHROPIC_BASE_URL set the claude.ai login stays active. The only subscription-eligible Anthropic client.
- Sources: [1](https://code.claude.com/docs/en/env-vars), [2](https://code.claude.com/docs/en/llm-gateway-protocol), [3](https://code.claude.com/docs/en/llm-gateway), [4](https://code.claude.com/docs/en/mcp), [5](https://code.claude.com/docs/en/headless), [6](https://code.claude.com/docs/en/setup)

### Cline + Cline CLI

`cline` · Through Switchboard (set up once in the agent) · Cline · license Apache-2.0

- Tools: add `switchboard` (`switchboard mcp`) to `~/.cline/data/settings/cline_mcp_settings.json`
- Setup: `switchboard agents connect cline --pool <pool>`; launch: `switchboard agents launch cline --pool <pool> --dir <folder>`
- Notes: CLI uses createAnthropic, so x-api-key. In the extension it is the "Use custom base URL" checkbox; that the extension takes the same path is unverified. `cline mcp` opens a wizard; CLINE_MCP_SETTINGS_PATH overrides the MCP file. No OpenRouter launch: the TUI sends a fresh install to onboarding before it reads `OPENROUTER_API_KEY`.
- Sources: [1](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/apps/cli/src/commands/auth.ts#L88-L92), [2](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/sdk/packages/llms/src/providers/vendors/anthropic.ts#L18-L26), [3](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/sdk/packages/shared/src/storage/paths.ts#L81), [4](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/docs/cli/cli-reference.mdx#L14-L48), [5](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/sdk/packages/llms/src/providers/builtins.ts#L608-L623), [6](https://github.com/cline/cline/blob/68b24a92b71ee98c3e8ea4aca7a7a1c0f97a43b8/docs/getting-started/installing-cline.mdx#L95)

### Freebuff

`freebuff` · Tools only (`switchboard mcp`) · Codebuff · license unverified

- Tools: add `switchboard` (`switchboard mcp`) to `~/.agents/mcp.json`
- Setup: `switchboard agents connect freebuff --pool <pool>`
- Notes: License inconsistent: repo LICENSE says Apache-2.0, npm says MIT. No headless mode. Models come from Freebuff's own backend (URL fixed at build time). MCP files are also read from {cwd}/.agents/; whether Freebuff's root agent receives the servers is unverified. Needs a Freebuff account.
- Sources: [1](https://github.com/CodebuffAI/freebuff/blob/703e9aa2877b1196b52e5c7791cb73bab2c8889f/README.md#L1-L75), [2](https://github.com/CodebuffAI/freebuff/blob/703e9aa2877b1196b52e5c7791cb73bab2c8889f/sdk/src/agents/load-mcp-config.ts#L89-L101), [3](https://github.com/CodebuffAI/freebuff/blob/703e9aa2877b1196b52e5c7791cb73bab2c8889f/cli/src/cli-args.ts#L51-L72), [4](https://github.com/CodebuffAI/freebuff/blob/703e9aa2877b1196b52e5c7791cb73bab2c8889f/sdk/src/constants.ts#L9-L20), [5](https://github.com/CodebuffAI/freebuff/blob/703e9aa2877b1196b52e5c7791cb73bab2c8889f/freebuff/cli/release/package.json#L5)

### omp (oh-my-pi)

`omp` · Through Switchboard (set up once in the agent) · can1357 · license MIT

- Tools: add `switchboard` (`switchboard mcp`) to `~/.omp/agent/mcp.json`
- Setup: `switchboard agents connect omp --pool <pool>`; launch: `switchboard agents launch omp --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch omp --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`, PI_CODING_AGENT_DIR isolated). A login saved with `/login` wins over the launch's key, so the launch uses its own `PI_CODING_AGENT_DIR`.
- **Accounts:** Impersonates Claude Code when handed an sk-ant-oat subscription token; give it an Anthropic API-key account only, never a Claude subscription token.
- Notes: Alternative: models.yml providers.<id>.baseUrl with api: anthropic-messages; set auth: apiKey to avoid OAuth-style shaping. A trailing /v1 is stripped and /v1/messages appended; Bearer to non-Anthropic hosts. The key env var name is not given in the research. No CLI add command (/mcp add in the TUI); omp also imports servers from ~/.claude.json, ~/.codex/config.toml and ~/.cursor/mcp.json. Launch by Switchboard waits until its key variable and base path are verified.
- Sources: [1](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/docs/models.md#L50-L57), [2](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/packages/ai/src/utils/anthropic-auth.ts#L30-L37), [3](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/docs/models.md#L466-L472), [4](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/packages/ai/src/providers/anthropic.ts#L409-L424), [5](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/docs/models.md#L148), [6](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/docs/mcp-config.md#L15-L44), [7](https://github.com/can1357/oh-my-pi/blob/1c0993c3d12e70042169a951663bb2702e2c0a9e/README.md#L40-L54)

### Codex CLI

`codex` · Through Switchboard (set up once in the agent) · OpenAI · license Apache-2.0

- Tools: `codex mcp add switchboard -- switchboard mcp`
- Setup: `switchboard agents connect codex --pool <pool>`; launch: `switchboard agents launch codex --pool <pool> --dir <folder>`
- Notes: Switchboard's second integrated client. The only client a ChatGPT-subscription account may be routed to.
- Sources: [1](https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L100-L174), [2](https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L79-L84), [3](https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider/src/bearer_auth_provider.rs#L31-L36), [4](https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/config/src/config_toml.rs#L426-L427), [5](https://learn.chatgpt.com/docs/non-interactive-mode), [6](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [7](https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/README.md#L16-L48)

### Command Code

`command-code` · Through Switchboard (set up once in the agent) · Command Code (commandcode.ai) · license proprietary

- Tools: `cmd mcp add --scope user switchboard -- switchboard mcp`
- Setup: `switchboard agents connect command-code --pool <pool>`; launch: `switchboard agents launch command-code --pool <pool> --dir <folder>`
- Notes: npm license UNLICENSED; the GitHub repo holds only a readme. Providers file ~/.commandcode/providers.json (apiKey accepts "$ENV"). Header and appended path are unverified, so suffix is null. Hand-edit field names for MCP JSON are unverified. `cmd login` is required first; whether BYOK works without a Command Code account is unverified. Node 22+.
- Sources: [1](https://commandcode.ai/docs/byok), [2](https://commandcode.ai/docs/mcp), [3](https://commandcode.ai/docs/headless), [4](https://commandcode.ai/docs/quickstart), [5](https://github.com/CommandCodeAI/command-code/tree/5c8f1b48c9d6704210cb3f9a476fdcffe5093e9a)

### pi

`pi` · Through Switchboard (set up once in the agent) · Earendil (earendil-works) · license MIT

- Tools: `pi mcp add switchboard -- switchboard mcp`
- Setup: `switchboard agents connect pi --pool <pool>`; launch: `switchboard agents launch pi --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch pi --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`, PI_CODING_AGENT_DIR isolated). Runs in its own `PI_CODING_AGENT_DIR` so a saved `auth.json` cannot win.
- **Accounts:** Impersonates Claude Code when handed an sk-ant-oat subscription token; give it an Anthropic API-key account only, never a Claude subscription token.
- Notes: Config file ~/.pi/agent/models.json; overriding only baseUrl keeps built-in models. A plain key goes out as x-api-key, ANTHROPIC_AUTH_TOKEN as Bearer. The appended path is unverified (the research's example uses …/claude/<pool> without /v1), so suffix is null.
- Sources: [1](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/ai/src/api/anthropic-messages.ts#L1068-L1075), [2](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/coding-agent/test/model-registry.test.ts#L95-L120), [3](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/ai/src/api/anthropic-messages.ts#L1013-L1018), [4](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/coding-agent/docs/mcp.md#L9-L62), [5](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/coding-agent/docs/cli.md#L25-L48), [6](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/coding-agent/README.md#L28-L43), [7](https://github.com/earendil-works/pi/blob/b78e6a9085343ec0f308c3d377da72528b4cf7ee/packages/ai/src/types.ts#L17-L23)

### DeepSeek Harness

`deepseek-harness` · Through Switchboard (set up once in the agent) · DeepSeek · license MIT

- Tools: add `switchboard` (`switchboard mcp`) to `$DSH_HOME/cordis.patch.yml`
- Setup: `switchboard agents connect deepseek-harness --pool <pool>`; launch: `switchboard agents launch deepseek-harness --pool <pool> --dir <folder>`
- Notes: Developer preview; Web UI first (`npx @deepseek-ai/dsh web` opens it on 127.0.0.1:3080); a global install command is unverified. Anthropic via a "Custom model API" in llm-pi-ai: providers.<id>.baseURL, api: anthropic-messages, apiKeyEnv, models; header and appended path unverified. Whether a loopback baseURL is accepted is unverified. Session-log upload to DeepSeek is on by default. Per-profile MCP patch: $DSH_HOME/profiles/<profile>/cordis.patch.yml.
- Sources: [1](https://github.com/deepseek-ai/deepseek-harness/blob/5badb15009ae1756c3afe0ae0cef1faafc290ccc/docs/user/guide/providers.md#L21-L71), [2](https://github.com/deepseek-ai/deepseek-harness/blob/5badb15009ae1756c3afe0ae0cef1faafc290ccc/README.md#L5-L27), [3](https://github.com/deepseek-ai/deepseek-harness/blob/5badb15009ae1756c3afe0ae0cef1faafc290ccc/apps/cli/README.md#L14), [4](https://github.com/deepseek-ai/deepseek-harness/blob/5badb15009ae1756c3afe0ae0cef1faafc290ccc/docs/config-catalog.md#L2022-L2056), [5](https://github.com/deepseek-ai/deepseek-harness/blob/5badb15009ae1756c3afe0ae0cef1faafc290ccc/apps/cli/reference/README.md#L129)

### OpenClaw

`openclaw` · Through Switchboard (set up once in the agent) · OpenClaw · license MIT

- Tools: `openclaw mcp set switchboard '{"command":"switchboard","args":["mcp"]}'`
- Setup: `switchboard agents connect openclaw --pool <pool>`; launch: `switchboard agents launch openclaw --pool <pool> --dir <folder>`
- **Accounts:** Impersonates Claude Code when handed an sk-ant-oat subscription token; give it an Anthropic API-key account only, never a Claude subscription token.
- Notes: A personal-assistant gateway with a coding headless mode (agent exec, coding tool profile). ANTHROPIC_BASE_URL is a fallback to models.providers.<id>. apiKey takes the key value or an env-var name as a SecretRef env marker (the pinned worked example uses "LITELLM_KEY") — name e.g. SWITCHBOARD_TOKEN and export it from the key command to keep the key out of the file. /v1/messages is appended (only /messages if the base ends in /v1); x-api-key. A custom baseUrl trusts that loopback origin automatically. `openclaw mcp add` probes the server before saving. Launch by Switchboard waits until its key variable and base path are verified. No OpenRouter launch: an interactive OpenClaw session runs through its Gateway daemon, which never sees a launch's environment; only one-shot `openclaw agent exec --auth-env-only` takes a per-run key.
- Sources: [1](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/docs/gateway/config-tools/custom-providers.md#L14-L82), [2](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/packages/ai/src/transports/anthropic-transport-stream.ts#L79-L87), [3](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/docs/cli/mcp/transports.md#L21-L30), [4](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/docs/cli/mcp/registry.md#L82-L101), [5](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/docs/cli/agent.md#L26-L38), [6](https://github.com/openclaw/openclaw/blob/78e406060b56bac2d09810161ac7b483861e9879/README.md#L28-L41)

### OpenHands CLI

`openhands` · Through Switchboard (set up once in the agent) · OpenHands · license MIT

- Tools: `openhands mcp add switchboard --transport stdio switchboard mcp`
- Setup: `switchboard agents connect openhands --pool <pool>`; launch: `switchboard agents launch openhands --pool <pool> --dir <folder>`
- Notes: No longer actively maintained (README points to Agent Canvas); low-value target. Set LLM_MODEL=anthropic/<model> too. Header and appended path at the pinned LiteLLM version are unverified, so suffix is null. Without the override the default base is the All-Hands cloud LLM proxy. Launch by Switchboard waits until its key variable and base path are verified.
- Sources: [1](https://github.com/OpenHands/OpenHands-CLI/blob/954f2ba646e8d749261a8f2b2b7e3031fa39be9f/openhands_cli/stores/agent_store.py#L133-L221), [2](https://github.com/OpenHands/OpenHands-CLI/blob/954f2ba646e8d749261a8f2b2b7e3031fa39be9f/README.md#L29-L35), [3](https://github.com/OpenHands/OpenHands-CLI/blob/954f2ba646e8d749261a8f2b2b7e3031fa39be9f/openhands_cli/argparsers/main_parser.py#L65-L93), [4](https://github.com/OpenHands/OpenHands-CLI/blob/954f2ba646e8d749261a8f2b2b7e3031fa39be9f/openhands_cli/mcp/mcp_utils.py#L167-L173), [5](https://github.com/OpenHands/software-agent-sdk/blob/edaac806d1599a4ee1662fd4008fbcd7b537ecd1/openhands-sdk/openhands/sdk/llm/utils/model_features.py#L192-L197)

### ZCode

`zcode` · Through Switchboard (set up once in the agent) · Z.ai · license Apache-2.0

- Tools: add `switchboard` (`switchboard mcp`) to `~/.zcode/cli/config.json`
- Setup: `switchboard agents connect zcode --pool <pool>`; launch: `switchboard agents launch zcode --pool <pool> --dir <folder>`
- Notes: Desktop DMG from https://zcode.z.ai/; the CLI's hosted install.sh URL is unverified (npm zcode-app-cli is a third-party wrapper). Provider in ~/.zcode/v2/provider_config.json (ZCODE_PERSONAL_PROVIDER_CONFIG_FILE overrides): the personal-rules object {providerRules: [...]}, a rule {providerId, providerName?, enabled?, config{group standard-personal, access{api-key}, api{anthropic-messages, baseUrl}, personalModelIds}} per personalProviderConfigRulesSchema. access.apiKey is the literal key value. /v1 is added if absent, then /messages; sends x-api-key and Bearer (an explicit Authorization header wins).
- Sources: [1](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/apps/zcode-cli/packages/adapters/src/model/model-execution.ts#L282-L435), [2](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/packages/provider/src/config/provider-data-schema.ts#L4-L70), [3](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/apps/zcode-cli/README.md#L139-L180), [4](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/apps/zcode-cli/packages/i18n/src/locales/en-US.ts#L31-L50), [5](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/README.en.md#L14-L97), [6](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/packages/provider/src/config/rule-data-schema.ts)

### Kimi Code CLI

`kimi-code` · Through Switchboard (set up once in the agent) · Moonshot AI · license MIT

- Tools: add `switchboard` (`switchboard mcp`) to `~/.kimi-code/mcp.json`
- Setup: `switchboard agents connect kimi-code --pool <pool>`; launch: `switchboard agents launch kimi-code --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch kimi-code --openrouter [--model <id>] --dir <folder>` (`KIMI_MODEL_API_KEY`). Runs on the Kimi Code subscription by default; an OpenRouter launch sets `KIMI_MODEL_*` for that session only and leaves its login and config alone. Reads `KIMI_MODEL_*` only; a shell `OPENROUTER_API_KEY` is ignored.
- Notes: Replaces "Proto Agent" (no established agent by that name). Provider config in ~/.kimi-code/config.toml; x-api-key with Authorization forced to null; shell ANTHROPIC_* vars are ignored (only a declared api_key_env is read). No shell MCP add command (/mcp-config in the TUI); user-level MCP is ~/.kimi-code/mcp.json ($KIMI_CODE_HOME/mcp.json). Project MCP servers are off in headless runs unless trusted or KIMI_CODE_TRUST_WORKSPACE=1. /login manages only the Kimi Code OAuth managed account; a config.toml provider is an independent API source (credential priority api_key/api_key_env > [providers.<name>.env] > startup error), so a configured custom provider needs no /login per the pinned docs — live confirmation pending.
- Sources: [1](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/providers.md#L62-L110), [2](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/providers.md#L20-L42), [3](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/kosong/src/providers/anthropic.ts#L1192-L1220), [4](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/agent-core-v2/src/human/llm/requester/bases/anthropic/requester.ts#L179-L181), [5](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/env-vars.md#L6), [6](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/customization/mcp.md#L13-L72), [7](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/reference/kimi-command.md#L110-L129), [8](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/guides/getting-started.md#L27-L53)

### Auggie CLI

`auggie` · Tools only (`switchboard mcp`) · Augment Code · license proprietary

- Tools: `auggie mcp add switchboard -- switchboard mcp`
- Setup: `switchboard agents connect auggie --pool <pool>`
- Notes: Replaces Roo Code (shut down, repo archived). Needs an Augment login and subscription; inference runs in Augment's cloud, and BYOK is an Enterprise backend setting, not a client base URL. Automation auth goes in AUGMENT_SESSION_AUTH. MCP only.
- Sources: [1](https://docs.augmentcode.com/cli/reference), [2](https://docs.augmentcode.com/cli/integrations), [3](https://docs.augmentcode.com/cli/setup-auggie/install-auggie-cli), [4](https://docs.augmentcode.com/cli/setup-auggie/authentication), [5](https://docs.augmentcode.com/cosmos/guides/admin-checklist), [6](https://github.com/augmentcode/auggie/blob/9cc3ead419db9486ad44e6e4bba30ecd6784ccff/LICENSE.md)

### Aider

`aider` · Launch from Switchboard · Aider-AI · license Apache-2.0

- Tools: no MCP support.
- Setup: `switchboard agents connect aider --pool <pool>`; launch: `switchboard agents launch aider --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch aider --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`)
- Notes: No MCP support. Anthropic comes from litellm 1.82.3: it reads ANTHROPIC_API_BASE, then ANTHROPIC_BASE_URL, appends /v1/messages and sends x-api-key. Run with --model anthropic/<model>. Last tagged release v0.86.0 (2025-08-09).
- Sources: [1](https://github.com/BerriAI/litellm/blob/61409275c8d8478d0a7ffc23d375a4fd86717b23/litellm/main.py#L2845-L2860), [2](https://github.com/BerriAI/litellm/blob/61409275c8d8478d0a7ffc23d375a4fd86717b23/litellm/llms/anthropic/common_utils.py#L444-L455), [3](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/requirements.txt#L184), [4](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/args.py), [5](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/website/docs/scripting.md#L13-L43), [6](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/website/docs/install.md#L33-L58), [7](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/website/docs/llms/openai-compat.md)

### OpenCode

`opencode` · Through Switchboard (set up once in the agent) · anomalyco (formerly sst) · license MIT

- Tools: add `switchboard` (`switchboard mcp`) to `~/.config/opencode/opencode.json`
- Setup: `switchboard agents connect opencode --pool <pool>`; launch: `switchboard agents launch opencode --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch opencode --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`). A key saved with `/connect` (auth.json) likely wins over the launch's key (unverified at runtime).
- Notes: Posts to ${baseURL}/messages with x-api-key; an explicit baseURL wins. apiKey accepts {env:VAR}.
- Sources: [1](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/opencode/src/provider/provider.ts#L1810-L1832), [2](https://github.com/vercel/ai/blob/85464f4e2026d9fc0274424c0171a25742836411/packages/anthropic/src/anthropic-provider.ts#L105-L130), [3](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/opencode/src/provider/provider.ts#L243-L249), [4](https://github.com/vercel/ai/blob/07edaea05f90a0d46a0447849af3ae6ae5d8f662/packages/openai/src/openai-provider.ts#L148-L162), [5](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/web/src/content/docs/mcp-servers.mdx#L70-L126), [6](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/web/src/content/docs/cli.mdx#L339-L386), [7](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/web/src/content/docs/index.mdx#L37-L82)

### Continue + cn

`continue` · Through Switchboard (set up once in the agent) · Continue (continuedev) · license Apache-2.0

- Tools: add `switchboard` (`switchboard mcp`) to `~/.continue/config.yaml`
- Setup: `switchboard agents connect continue --pool <pool>`; launch: `switchboard agents launch continue --pool <pool> --dir <folder>`
- Notes: Model with provider: anthropic, apiBase (…/claude/<pool>/v1) and apiKey; x-api-key; no env var. The enclosing models-list shape is not given in the research, so no snippet. cn skips tools unless --allow "*". Extension ID Continue.continue; config shared by the extension and cn.
- Sources: [1](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/core/llm/llms/Anthropic.ts#L43-L451), [2](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/packages/openai-adapters/src/apis/AnthropicUtils.ts#L66-L91), [3](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/core/llm/index.ts#L1023-L1030), [4](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/packages/openai-adapters/src/apis/OpenAI.ts#L90-L96), [5](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/docs/customize/deep-dives/mcp.mdx#L30-L133), [6](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/docs/cli/headless-mode.mdx#L5-L46), [7](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/docs/snippets/cli-install.mdx)

### Goose

`goose` · Launch from Switchboard · aaif-goose (formerly Block) · license Apache-2.0

- Tools: add `switchboard` (`switchboard mcp`) to `~/.config/goose/config.yaml`
- Setup: `switchboard agents connect goose --pool <pool>`; launch: `switchboard agents launch goose --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch goose --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`)
- Notes: x-api-key; v1/messages is appended. `goose run --with-extension "switchboard mcp"` adds Switchboard for one run. Desktop: brew install --cask block-goose.
- Sources: [1](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/crates/goose/src/providers/anthropic_def.rs#L48-L72), [2](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/crates/goose-providers/src/api_client.rs#L509-L520), [3](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/crates/goose-providers/src/openai.rs#L382-L400), [4](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/documentation/docs/guides/config-files.md#L11-L170), [5](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/documentation/docs/guides/running-tasks.md#L17-L109), [6](https://github.com/aaif-goose/goose/blob/d7ce0cdf8c4e1fb55aabed5b6717f141c4c8220f/documentation/docs/getting-started/installation.md#L22-L78)

### Gemini CLI

`gemini-cli` · Tools only (`switchboard mcp`) · Google · license Apache-2.0

- Tools: `gemini mcp add -s user switchboard switchboard mcp`
- Setup: `switchboard agents connect gemini-cli --pool <pool>`
- Notes: Gemini protocol only: GOOGLE_GEMINI_BASE_URL gateway mode still speaks Gemini. `gemini mcp add` defaults to project scope, hence -s user. MCP only.
- Sources: [1](https://github.com/google-gemini/gemini-cli/blob/fb972b2f87fe7d5b06d37eac711490162d98de2c/packages/core/src/core/contentGenerator.ts#L63-L92), [2](https://github.com/google-gemini/gemini-cli/blob/fb972b2f87fe7d5b06d37eac711490162d98de2c/packages/core/src/core/contentGenerator.ts#L344-L375), [3](https://github.com/google-gemini/gemini-cli/blob/fb972b2f87fe7d5b06d37eac711490162d98de2c/docs/tools/mcp-server.md#L1116-L1158), [4](https://github.com/google-gemini/gemini-cli/blob/fb972b2f87fe7d5b06d37eac711490162d98de2c/docs/cli/headless.md#L8-L13), [5](https://github.com/google-gemini/gemini-cli/blob/fb972b2f87fe7d5b06d37eac711490162d98de2c/README.md#L38-L54)

### Qwen Code

`qwen-code` · Launch from Switchboard · QwenLM · license Apache-2.0

- Tools: `qwen mcp add switchboard switchboard mcp`
- Setup: `switchboard agents connect qwen-code --pool <pool>`; launch: `switchboard agents launch qwen-code --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch qwen-code --openrouter [--model <id>] --dir <folder>` (`OPENAI_API_KEY`, QWEN_HOME isolated). A `modelProviders` entry in settings wins over the launch's environment, so the launch uses its own `QWEN_HOME`.
- Notes: Also set ANTHROPIC_MODEL. Alternative: modelProviders.anthropic[] with baseUrl/envKey and security.auth.selectedType: "anthropic". Bearer with a claude-cli User-Agent to non-Anthropic hosts. Doubled-/v1 behaviour is unverified; the research says use …/claude/<pool>. Qwen OAuth free tier ended 2026-04-15 (API keys only).
- Sources: [1](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/packages/core/src/core/anthropicContentGenerator/anthropicContentGenerator.ts#L264-L296), [2](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/docs/users/configuration/auth.md#L216-L234), [3](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/docs/users/configuration/model-providers.md#L242-L258), [4](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/packages/core/src/core/openaiResponsesContentGenerator/responses-pipeline.ts#L595-L617), [5](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/docs/users/features/mcp.md#L20-L110), [6](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/docs/users/features/headless.md#L22-L25), [7](https://github.com/QwenLM/qwen-code/blob/544d753229f31feca679a85932437650f359e769/README.md#L56-L62)

### Crush

`crush` · Through Switchboard (set up once in the agent) · Charm · license FSL-1.1-MIT

- Tools: add `switchboard` (`switchboard mcp`) to `~/.config/crush/crushrc`
- Setup: `switchboard agents connect crush --pool <pool>`; launch: `switchboard agents launch crush --pool <pool> --dir <folder>`
- OpenRouter key: `switchboard agents launch crush --openrouter [--model <id>] --dir <folder>` (`OPENROUTER_API_KEY`). The interactive TUI takes no model flag: pick the model in its picker (Ctrl+L); the key comes from the launch.
- Notes: Source-available (FSL-1.1-MIT). Config is a Bash-style crushrc; crush.json is deprecated, and the MCP and provider lines are crushrc statements, not shell commands. x-api-key, or Authorization if the key starts with "Bearer ". Exact path joining is unverified, so suffix is null.
- Sources: [1](https://github.com/charmbracelet/crush/blob/8da349060b7df148d209979be0a5e9c9281d1f15/internal/agent/coordinator.go#L1072-L1190), [2](https://github.com/charmbracelet/crush/blob/8da349060b7df148d209979be0a5e9c9281d1f15/README.md#L253-L310), [3](https://github.com/charmbracelet/crush/blob/8da349060b7df148d209979be0a5e9c9281d1f15/docs/config/README.md#L281-L292), [4](https://github.com/charmbracelet/crush/blob/8da349060b7df148d209979be0a5e9c9281d1f15/internal/cmd/run.go#L33-L65), [5](https://github.com/charmbracelet/fantasy/blob/d272c40391c5f87aa241c5b82014f83bdc48778a/providers/openai/openai.go#L191-L223), [6](https://github.com/charmbracelet/crush/blob/8da349060b7df148d209979be0a5e9c9281d1f15/LICENSE.md)

### Zed agent

`zed` · Through Switchboard (set up once in the agent) · Zed Industries · license GPL-3.0-or-later

- Tools: add `switchboard` (`switchboard mcp`) to `~/.config/zed/settings.json`
- Setup: `switchboard agents connect zed --pool <pool>`; launch: `switchboard agents launch zed --pool <pool> --dir <folder>`
- Notes: Set language_models.anthropic_compatible.<Name>.api_url = …/claude/<pool> with available_models; the model-entry shape is not given in the research, so no snippet. Key via the UI or <PROVIDER_NAME>_API_KEY; always X-Api-Key. No headless agent run is documented (unverified). ACP external agents handle their own auth.
- Sources: [1](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/crates/anthropic/src/anthropic.rs#L483-L507), [2](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/docs/src/ai/use-api-access.md#L419-L467), [3](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/crates/open_ai/src/responses.rs#L863-L869), [4](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/docs/src/ai/mcp.md#L53-L74), [5](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/docs/src/ai/external-agents.md#L39-L53), [6](https://github.com/zed-industries/zed/blob/7aec07ccdedf8bf1ce8723c7e4305ba753651ea6/docs/src/installation.md#L14-L17)

### Amp

`amp` · Tools only (`switchboard mcp`) · Sourcegraph · license proprietary

- Tools: `amp mcp add switchboard -- switchboard mcp`
- Setup: `switchboard agents connect amp --pool <pool>`
- Notes: A "Custom URL" connection with format anthropic-messages (<base>/v1/messages, Bearer) exists, but connections are stored server-side and hosts must be "reachable from Amp": whether a 127.0.0.1 proxy works is unverified, so rated MCP until tested. Requires an Amp account (amp login / AMP_API_KEY).
- Sources: [1](https://ampcode.com/docs/markdown/customize/model-routing), [2](https://ampcode.com/docs/markdown/customize/mcp), [3](https://ampcode.com/docs/markdown/cli/execute-mode), [4](https://ampcode.com/docs/cli), [5](https://ampcode.com/llms.txt), [6](https://www.npmjs.com/package/@sourcegraph/amp)

### Cursor CLI

`cursor-cli` · Tools only (`switchboard mcp`) · Cursor · license proprietary

- Tools: add `switchboard` (`switchboard mcp`) to `~/.cursor/mcp.json`
- Setup: `switchboard agents connect cursor-cli --pool <pool>`
- Notes: No provider or base-URL option. MCP file shared with the editor; `agent mcp` has no add command. Headless flags: --force, --output-format json, --approve-mcps. Needs Cursor auth (agent login / CURSOR_API_KEY). Old binary name cursor-agent. MCP only.
- Sources: [1](https://cursor.com/docs/cli/reference/configuration.md), [2](https://cursor.com/docs/cli/reference/parameters.md), [3](https://cursor.com/docs/cli/mcp.md), [4](https://cursor.com/docs/mcp.md), [5](https://cursor.com/docs/cli/headless.md), [6](https://cursor.com/docs/cli/installation.md)

### GitHub Copilot CLI

`copilot-cli` · Through Switchboard (set up once in the agent) · GitHub · license proprietary

- Tools: add `switchboard` (`switchboard mcp`) to `~/.copilot/mcp-config.json`
- Setup: `switchboard agents connect copilot-cli --pool <pool>`; launch: `switchboard agents launch copilot-cli --pool <pool> --dir <folder>`
- Notes: Also set COPILOT_MODEL; COPILOT_PROVIDER_BEARER_TOKEN is the Bearer alternative to the key. Header and appended path are unverified, so suffix is null (the research's example base is …/claude/<pool>). COPILOT_OFFLINE=true stops calls to GitHub; whether BYOK plus offline skips GitHub login is unverified. Needs a Copilot subscription; MCP config requires tools. Launch by Switchboard waits until its key variable and base path are verified.
- Sources: [1](https://github.com/github/docs/blob/ddc34e6c76d8836ec12ed3edfaffb1c4be41829e/content/copilot/how-tos/copilot-cli/customize-copilot/use-byok-models.md#L47-L59), [2](https://github.com/github/docs/blob/ddc34e6c76d8836ec12ed3edfaffb1c4be41829e/content/copilot/how-tos/copilot-cli/customize-copilot/use-byok-models.md#L132-L144), [3](https://github.com/github/copilot-sdk/blob/6b4f3a3bde7eb9a8604617b91effe0f4a2e3921b/docs/auth/byok.md#L204-L219), [4](https://github.com/github/docs/blob/ddc34e6c76d8836ec12ed3edfaffb1c4be41829e/content/copilot/reference/copilot-cli-reference/cli-command-reference.md#L658), [5](https://github.com/github/docs/blob/ddc34e6c76d8836ec12ed3edfaffb1c4be41829e/content/copilot/reference/copilot-cli-reference/cli-command-reference.md#L941-L994), [6](https://github.com/github/copilot-cli/blob/a9ba11a191255b3f7b323b425b717f7db14b6c74/README.md#L41-L91), [7](https://github.com/github/copilot-cli/blob/a9ba11a191255b3f7b323b425b717f7db14b6c74/LICENSE.md#L1-L4)

### Factory Droid

`factory-droid` · Through Switchboard (set up once in the agent) · Factory · license unverified

- Tools: add `switchboard` (`switchboard mcp`) to `~/.factory/mcp.json`
- Setup: `switchboard agents connect factory-droid --pool <pool>`; launch: `switchboard agents launch factory-droid --pool <pool> --dir <folder>`
- Notes: License unverified (public repo holds docs only). Settings file ~/.factory/settings.json; x-api-key by default, "authMode": "bearer" sends Bearer. Select with droid exec --model "custom:<Display-Name>-<index>". droid exec needs FACTORY_API_KEY (a Factory account); whether BYOK works without a Factory login is unverified.
- Sources: [1](https://docs.factory.com/model-independence/byok.md), [2](https://docs.factory.com/harness/mcp.md), [3](https://docs.factory.com/droid-exec/overview.md), [4](https://docs.factory.com/droid-cli/quickstart.md)

### Warp

`warp` · Tools only (`switchboard mcp`) · Warp · license AGPL-3.0

- Tools: add `switchboard` (`switchboard mcp`) to `~/.warp/.mcp.json`
- Setup: `switchboard agents connect warp --pool <pool>`
- Notes: Client AGPL-3.0 with MIT UI crates. Anthropic BYOK takes a key only, no base URL. The Agent CLI (brew install --cask warp-agent-cli) documents no one-shot flag and reads ~/.warp_cli/.mcp.json. Cannot use the proxy; Warp can host Claude Code or Codex, which can use Switchboard themselves.
- Sources: [1](https://docs.warp.dev/agents/inference/custom-inference-endpoint.md), [2](https://docs.warp.dev/agents/inference/bring-your-own-api-key.md), [3](https://docs.warp.dev/agents/capabilities/mcp.md), [4](https://docs.warp.dev/agents/cli/configuration.md), [5](https://docs.warp.dev/agents/cli/reference.md), [6](https://docs.warp.dev/agents/cli/quickstart.md), [7](https://github.com/warpdotdev/Warp/blob/023685490da35f4cfe01084b8a5b5792cf70cb2a/README.md#L52-L56)

### Kiro CLI

`kiro-cli` · Tools only (`switchboard mcp`) · AWS (Kiro) · license proprietary

- Tools: `kiro-cli mcp add --name switchboard --scope global --command switchboard --args mcp`
- Setup: `switchboard agents connect kiro-cli --pool <pool>`
- Notes: Closed source; formerly Amazon Q Developer CLI. No base URL or BYOK is documented (absence beyond the read pages is unverified); inference runs on Amazon Bedrock through Kiro. Headless needs KIRO_API_KEY (paid plans only). MCP only.
- Sources: [1](https://kiro.dev/docs/models.md), [2](https://kiro.dev/docs/reference/settings.md), [3](https://kiro.dev/docs/cli/headless.md), [4](https://kiro.dev/docs/mcp/configuration.md), [5](https://kiro.dev/docs/getting-started/installation.md), [6](https://github.com/aws/amazon-q-developer-cli/blob/15cc8f3cd18c4272925ce1c7053268eedff1ea0a/README.md#L4)

### Devin Desktop / Devin CLI (formerly Windsurf/Cascade)

`devin` · Tools only (`switchboard mcp`) · Devin (formerly Windsurf) · license unverified

- Tools: `devin mcp add -s user switchboard -- switchboard mcp`
- Setup: `switchboard agents connect devin --pool <pool>`
- Notes: Windsurf docs redirect to Devin; Cascade was removed in v3.9.19 (2026-09-08). License unverified. No BYOK or base URL documented outside air-gapped builds. Devin Desktop can host ACP agents, possibly a way to run a Switchboard-backed agent inside it (unverified). MCP only.
- Sources: [1](https://docs.devin.ai/desktop/changelog.md), [2](https://docs.devin.ai/desktop/models.md), [3](https://docs.devin.ai/cli/extensibility/mcp/configuration.md), [4](https://docs.devin.ai/cli/reference/commands.md), [5](https://docs.devin.ai/work-with-devin/devin-cli.md), [6](https://docs.devin.ai/desktop/devin-local.md)
