# Provider keys for agents by name, from Observatory (SB-404, 2026-11-17)

Board row: [SB-404](../../evidence/backlog.md). Builds after SB-401
([plan](../2026-11-17-token-group/README.md)) merges: it reuses that change's add, group and
ceiling code.

## The operator's words (2026-11-17, session 4c635d5e)

> И еще напоминаю про добавление токенов. Чтобы можно было добавить токены ChatGPT, OpenRouter,
> Anthropic, либо они подтягивались в Observatory автоматически, и можно было бы их использовать
> для запуска агентов сразу. Допустим, Claude Code агента … выпустить токен Anthropic с
> организацией Appvillis и добавить его в … отдельный пул, и она у нас будет работать только для
> [трёх проектов] … можно запустить Hermes агента, который будет с этой
> подпиской идти … Anthropic и модель Fable 5.2 мне нужна.

## What exists (main `80666430`, 0.7.111)

- Anthropic keys for Hermes by name: `switchboard agents anthropic set <name>` (value on stdin, OS
  vault), `agents launch hermes --anthropic <name>` (SB-390).
- One OpenRouter key per machine and one per group (`agents openrouter`), with ceilings (SB-72).
- A Claude API key added as an account, routed through the proxy; with SB-401, it lands in the
  pay-per-token group, off until a daily ceiling is set.
- Observatory holds keys by name (`use_secret.py names <project>`, `run --vault-only`); it never
  returns a value. It has issuing doors for OpenRouter and Cloudflare; Anthropic's Admin API has
  no endpoint to create an API key (checked 2026-11-17: list, retrieve and update only, "To view or
  create your own API keys, go to API keys in the Claude Console"), so an Anthropic key is created
  by a person in the Console.

## Decisions (under the operator's delegation)

- **K-1. Observatory is the source.** Switchboard lists the provider keys Observatory holds
  (`ANTHROPIC_*`, `OPENAI_*`, `OPENROUTER_*` slots that are not admin or management keys), by
  project, env and name, never a value. "Add" moves the value from Observatory into Switchboard's
  OS vault through a pipe (`use_secret.py run --vault-only … -- switchboard … set` reading stdin),
  and records the slot it came from, so a rotation in Observatory is followed.
- **K-2. One place per kind.** A Claude API key becomes an account in the pay-per-token group
  (SB-401: off, a daily ceiling to turn on, folders added one by one). An Anthropic key for
  Hermes and an OpenAI or OpenRouter key become named agent keys. The same Anthropic key may be
  both; its spend is counted once, at the proxy.
- **K-3. Launch at once.** The window's Agents page and `agents launch` offer every key by name in
  the folder's group; a Claude Code launch on a key is managed (through the proxy), never isolated.
- **K-4. Usage is controlled.** Every key an agent runs on counts against a ceiling: the group's
  daily ceiling for a Claude key, the key's own ceiling for OpenRouter, and for Hermes on an
  Anthropic key a ceiling of its own, counted at list price like SB-401's.
- **K-5. Gates stay with the person.** Creating an Anthropic or OpenAI key is the person's step in
  the provider's console; the value goes straight into Observatory's vault, never into a chat.

## Waves

| Wave | What | State |
|---|---|---|
| K1 | Read Observatory's key names (CLI `keys observatory`, MCP, window list) | after SB-401 |
| K2 | Add by name: Claude key to the pay-per-token group, Anthropic/OpenAI/OpenRouter to agent keys, through a pipe; record the slot | after K1 |
| K3 | Launch on a key from the window and the CLI; Hermes on an Anthropic key metered against a ceiling | after K2 |
| K4 | Follow a rotation in Observatory; refuse a key Observatory marks leaked | after K2 |
| Live | An Appvillis Anthropic key in the pay-per-token group with three of the operator's private project folders, model `claude-fable-5-2`, running a full audit of one of them | waits for the key (person) and SB-401's release |
