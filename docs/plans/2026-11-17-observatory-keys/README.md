# Provider keys for agents by name, from Observatory (SB-404, 2026-11-17)

Board row: [SB-404](../../evidence/backlog.md). Builds after SB-401
([plan](../2026-11-17-token-group/README.md)) merges: it reuses that change's add, group and
ceiling code.

## The operator's words (2026-11-17)

> И еще напоминаю про добавление токенов. Чтобы можно было добавить токены ChatGPT, OpenRouter,
> Anthropic, либо они подтягивались в Observatory автоматически, и можно было бы их использовать
> для запуска агентов сразу. Допустим, Claude Code агента … выпустить токен Anthropic с
> [организацией] и добавить его в … отдельный пул, и она у нас будет работать только для
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
  both. Corrected in review (2026-11-17): only what runs through the pay-per-token group is
  counted, at the proxy; a direct Hermes launch on the named key (`--anthropic`, SB-390) is counted
  by no ceiling, so the two uses are not one count (see K-7).
- **K-3. Launch at once.** The window's Agents page and `agents launch` offer every key by name in
  the folder's group; a Claude Code launch on a key is managed (through the proxy), never isolated.
- **K-4. Usage is controlled.** Every key an agent runs on counts against a ceiling: the group's
  daily ceiling for a Claude key, the key's own ceiling for OpenRouter, and for Hermes on an
  Anthropic key a ceiling of its own, counted at list price like SB-401's. Superseded for Hermes
  by K-7: Hermes is metered through the proxy on the pay-per-token group, and a named key's
  direct launch stays unmetered and is labelled so.
- **K-5. Gates stay with the person.** Creating an Anthropic or OpenAI key is the person's step in
  the provider's console; the value goes straight into Observatory's vault, never into a chat.

## Waves

| Wave | What | State |
|---|---|---|
| K1 | Read Observatory's key names (CLI `keys observatory`, MCP, window list) | built on `observatory-keys` (2026-11-17) |
| K2 | Add by name: Claude key to the pay-per-token group, Anthropic/OpenAI/OpenRouter to agent keys, through a pipe; record the slot | built (Anthropic, OpenRouter); OpenAI deferred, K-6 |
| K3 | Launch on a key from the window and the CLI; Hermes on an Anthropic key metered against a ceiling | built: Hermes through the proxy on the pay-per-token group, K-7 |
| K4 | Follow a rotation in Observatory; refuse a key Observatory marks leaked | after K2 |
| Live | A team's Anthropic key in the pay-per-token group with three of the operator's private project folders, model `claude-fable-5-2`, running a full audit of one of them | waits for the key (person) and SB-401's release |

## Decisions taken while building K1 to K3 (2026-11-17, recorded as DEC-0024)

- **K-6. OpenAI keys are listed, not added.** No ceiling in Switchboard counts an OpenAI key: the
  proxy prices Anthropic's models only (`switchboard-proxy` `prices.rs`), and a Codex API-key
  account has no ceiling of its own. Adding one would let an agent spend it unchecked, against
  K-4. The list shows it with `why_not`; adding it waits for OpenAI prices in the proxy and a
  Codex key in the pay-per-token group (board row).
- **K-7. Hermes is metered through the proxy, not by a ceiling of a named key.** Hermes reads its
  provider from its own config file, and its named providers take `api`, `key_env` and
  `transport: anthropic_messages` (checked in Hermes 0.22.2's `runtime_provider_custom.py`). A
  launch on the pay-per-token group writes a Hermes home of its own pointing a provider
  `switchboard` at the group's route with the agents' capability and runs `hermes --provider
  switchboard -m <model>`, so every answer is priced at list price against the group's daily
  ceiling, as Claude Code's are. A named Anthropic key's direct launch (SB-390) stays unmetered
  and is labelled so (`metered: false`); an Anthropic key added from Observatory goes to the
  pay-per-token group by default.
- **K-8. The value never passes through Switchboard's own arguments or environment.** Observatory
  only places a value in a command's environment, so the command it runs is `/bin/sh -c`, which
  moves it into an unexported variable, unsets it and pipes it into the ordinary save's stdin.
  macOS and Linux only for now.
- **K-9. Nothing replaces a saved key by surprise.** A named key or an OpenRouter key already
  saved where the add would go is kept unless `--replace`; a slot already added as an account is
  refused (one key, one account).
- **K-10. Hermes on an ordinary group starts as before.** Only the pay-per-token group's launch
  writes the home of its own; elsewhere Hermes keeps its `~/.hermes` set up by `agents connect`.

## Deferred

- K4 (rotation and leaks), the live run with a team's Anthropic key (waits for the key), OpenAI keys
  (K-6). A real Hermes run through the proxy was not observed in this change.

