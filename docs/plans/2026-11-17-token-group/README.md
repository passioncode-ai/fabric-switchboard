# Claude Code sessions on an Anthropic API key, in a group of their own (SB-401, 2026-11-17)

Board row: [SB-401](../../evidence/backlog.md). Entry point for the next agent: this file, then the
wave table at the end.

## What was asked (the operator's words, 2026-11-17)

> хочу, чтобы мы сделали поддержку сессий клауд-кода, который подключается токеном антропика, а не
> подпиской. Чтобы можно было залогиниться и добавить такую сессию тоже, которая по токену, и чтобы
> еще насчитала usage, и мы могли её там добавить куда-нибудь в папку или ещё куда-то себе
> специальную. Такие сессии за токены они сразу добавляются в отдельную папку, в которой нету
> никаких проектов, и проекты все добавляются туда потом индивидуально, чтобы случайно не начался
> спенд подписку с токеном, вдруг нет лимитов. Изначально она выключена. И обязательно, когда при
> включении контролируем usage.

## What already exists (measured 2026-11-17, file:line on main `82d86e0b`)

- An API-key Claude account (`AuthKind::ApiKey`, `crates/switchboard-core/src/lib.rs:72-77`), added
  with `accounts add --kind api-key` (`crates/switchboard-cli/src/main.rs:1150-1166`) or the window's
  "Add an API key or token" dialog (`src/main.ts:2756-2783`). The proxy sends it as `x-api-key`
  (`crates/switchboard-proxy/src/lib.rs:848-849`).
- Its tokens are counted per day, account, pool, client and folder (`statistics.json`,
  `crates/switchboard-proxy/src/stats.rs:10-18`), but not its money: no price table, and its spend
  reads `spend_unknown` (`docs/CONTRACTS.md:684`).
- Failover moves an API-key request only to another API-key account (`rotation.rs:374-383`);
  balancing lends OAuth accounts only (`balancing.rs:328-333`).
- SB-390's `anthropic-keys.json` serves Hermes only (`anthropic_keys.rs:1-10`) and stays as it is.

## The gaps, and what could spend a key by accident

1. Both add paths put the key in the `default` group (`main.rs:1161`, `main.ts:2761`): every folder
   in no group then runs on it (DEC-0007).
2. A group needs at least one folder (`core/src/projects.rs:445`, `:623`) and has no off switch.
3. A new account starts enabled (`core/src/lib.rs:849`).
4. Deleting a group returns its accounts to `default` (`core/src/projects.rs:1068`, `:1183-1215`).
5. Folders join a group by themselves (SB-121 work, project sources, settings import).
6. A folder rule, a switch by hand or `accounts move` can put a key under live sessions ungated
   (SB-217).
7. No money is computed, so no ceiling can stop a key's spend; the gate runs only at starts.

## Decisions (made under the operator's delegation; reasons beside each)

- **D-1. One reserved group, `tokens`** (shown as *Pay-per-token keys*), created on first need, kind
  `tokens`. It may hold zero folders and accepts only `ApiKey` Claude accounts. Reason: the
  operator's "отдельная папка, в которой нету никаких проектов".
- **D-2. A new API-key account goes to `tokens`, not `default`,** in the CLI, MCP and the window;
  an explicit other group is still allowed but `default` is refused for a key. Existing keys stay
  where they are (nothing that works is moved), and Needs attention names a key in `default`.
- **D-3. The group starts off** (`enabled: false`). While off: no folder is bound to it, the proxy
  refuses its requests with a sentence naming the switch, folder rules and switches that name its
  accounts are refused, nothing joins it by itself (SB-121, sources, settings import), and deleting
  it is refused while it holds an account.
- **D-4. Turning it on needs a daily spending ceiling** (USD per UTC day, the existing ceiling
  scope `project:tokens`), checked in the proxy before every request of the group, not only at a
  start. A reached ceiling refuses further requests and opens the existing "double or keep"
  question. Reason: "обязательно, когда при включении контролируем usage".
- **D-5. Money is computed from the provider's own token counts** with a price table in the code
  (per million tokens: input, output, cache write, cache read, by model family), its source URL and
  read date beside it; a model the table lacks is priced at the dearest family and marked
  `estimated`. The model of each request is recorded with its counts.
- **D-6. Folders are added one by one,** by the person (window, CLI `project edit --add-folders`,
  MCP) and never by a found worktree or clone.

## Waves

| Wave | What | State |
|---|---|---|
| T1 | Core: group kind `tokens`, zero folders allowed for it, `enabled` on groups (absent = on for every other group), new keys land there, `default` refused for a key; migration none | next |
| T2 | Safety: bindings skip an off group, proxy refuses an off group, rules and switches refused, no auto-join, delete refused, Needs attention for a key in `default` | after T1 |
| T3 | Money: model per request in `statistics.json`, price table, spend per account and group per day, `switchboard usage --money`, MCP | after T1 |
| T4 | Ceiling: turning on requires a daily ceiling; the proxy checks it per request; reached → refuse + question | after T2, T3 |
| T5 | Window: the group card with its switch, ceiling, today's spend and *Add folder*; the add-key dialog says where the key goes; scenarios, Copylot | after T4 |
| T6 | Docs (CONTRACTS, CLI, ANALYTICS if a count is added), DEC record, release | last |
