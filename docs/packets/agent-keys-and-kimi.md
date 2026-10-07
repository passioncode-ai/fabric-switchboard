# XA-02 — An OpenRouter key for agents, Hermes's model and provider, Kimi Code accounts

**State:** designed 2026-10-07 (operator request); SB-79 first, then SB-80, then SB-81.
**Request (operator, 2026-10-07):** «Kimi на подписке у меня … его бы добавить в свитчер тоже и
гермес туда добавить чтобы он показывал на какой моделе работает и какой провайдер + возможно
сменить провайдера, можно отдельный ключ опенроутера поставить в настройки который будет
использоваться для запуска агентов которые поддерживают это типо OpenClaw и Hermes и Pi и других
топ популярных агентов».
**Facts:** [research 2026-10-07](../research/kimi-hermes-openrouter-2026-10-07.md) (sources pinned to
commits; what is unverified is marked there). Earlier design: [XA-01](cross-agent-continuation.md).

## Outcomes

1. **SB-79 — one OpenRouter key for agents.** The person saves an OpenRouter key once (Agents →
   *OpenRouter key*); Switchboard keeps it in the OS vault and shows what the key may still spend
   (`GET https://openrouter.ai/api/v1/key`: `limit_remaining`, `usage_daily`). Agents that can run
   on OpenRouter launch on it from the Agents screen, the CLI and MCP, on the model the person
   picks (a default model, changeable per launch).
2. **SB-80 — Hermes's model and provider.** The Agents screen shows the model and provider the
   ordinary Hermes runs on, read from `hermes config get model --json` (secrets masked by Hermes),
   and changes them with `hermes config set model.provider|model.default` on request.
3. **SB-81 — Kimi Code subscription accounts.** Kimi Code accounts are saved and signed in through
   the official `kimi login`, listed with their plan and their 5-hour, 7-day and monthly usage, and
   launched in a project; the ordinary `kimi` is shown in *In use now*.

## Decisions

| ID | Decision | Why |
|---|---|---|
| A-1 | The OpenRouter key lives in the OS vault under its own UUID, recorded in the store as an *agent key* (service, saved time, default model) — never in a config file, a launch script, an argument or a log. | Same rule as every credential here. |
| A-2 | A launched agent gets the key through its environment, read at launch by the session script from `switchboard agents key --service openrouter` (the absolute CLI path), so no key value is written to disk. | Research Q3: every listed agent reads the key from env; flags (`--api-key`, `-k`) land in process listings. |
| A-3 | Each agent's OpenRouter recipe is data in `catalog/agents.json` (`openrouter`: env names, base URL, model flag or env, an isolation env such as `HERMES_HOME` / `PI_CODING_AGENT_DIR` where a saved key would win). | One place to correct a recipe; agents change. |
| A-4 | Switchboard never writes another agent's own config to put the key there. Changing Hermes's provider or model (SB-80) is the one write, only on the person's request, through Hermes's own `hermes config set`. | The person asked for it; Hermes keeps its own format. |
| A-5 | Kimi Code refresh tokens rotate (research Q1a), so a Kimi credential exists in exactly one place: the account's own `KIMI_CODE_HOME` under Switchboard's data folder. Switchboard never copies it into the vault or another home and never refreshes it; it reads the access token in-process only to ask `GET {base}/usages` and `/me` while the token is valid. | A copy breaks at the first refresh. |
| A-6 | The ordinary `kimi` (`~/.kimi-code`) is shown, not switched, in the first version: moving a credential between homes is undocumented (research Q1b). A switch, if added, moves files (never copies) while no `kimi` holds the refresh lock. | Avoid breaking the person's own login. |
| A-7 | Kimi's OAuth token is used only by the official `kimi` binary and for the account's own usage and profile reads; it is never handed to a third-party agent or the proxy (research Q1d: the docs send third-party agents to a Console API key). | Terms. |
| A-8 | Spend ceilings (SB-72) build on SB-79: a key issued by Project Observatory's OpenRouter door with `limit` and `limit_reset: daily` is saved the same way; Switchboard shows its remaining credit and refuses a launch when it is spent. | One key path for pasted and issued keys. |

## Contracts (to be filled by each slice)

- Store: `Snapshot.agent_keys: Vec<AgentKey>`; `Store::{set_agent_key, agent_key, remove_agent_key}`.
- Operations: `AgentKeySet {service, key, model}`, `AgentKeyStatus {service}`, `AgentKeyRemove {service}`,
  `AgentKeyValue {service}` (CLI only, never MCP or IPC); `AgentLaunch` gains `via: "proxy" | "openrouter"`
  and `model`.
- Catalog: `openrouter: {key_env, base_env?, base_url?, model_flag? | model_env?, extra_env, isolate_env?}`.

## Slices

1. SB-79a store, vault, operations, CLI (`switchboard agents key set|status|remove --service openrouter`, key from stdin), MCP status tool.
2. SB-79b catalog recipes and `agents launch --openrouter [--model]`; app: Agents → OpenRouter key panel, *Launch on OpenRouter* per agent.
3. SB-80 Hermes model/provider read and change (app, CLI, MCP).
4. SB-81 Kimi Code accounts (provider `kimi`): sign-in in an owned home, plan and usage, launch, *In use now*, chains.

Each slice: tests first, scenarios (SCN-045…), Russian strings, CONTRACTS, CHANGELOG, board.
