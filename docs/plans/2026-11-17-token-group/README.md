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

- **D-7 (review fixes, 2026-11-17).** A key of the pay-per-token group never leaves it nor serves
  another group: no share, no loan, no move by a save or a restore (one check in `publish` and
  `hold`, plus validation of `shared_accounts`, `default_shared`, `default_lent`); the person
  removes the account to take it out. A save of the group moves a listed key in, never shares it.
  A key launches only in managed mode. The proxy also checks the chosen account's home group after
  route selection and before an SB-98 move. Reason: two reviewers showed a shared key and an
  isolated launch spending outside the ceiling; the instruction was to refuse every move out.
- **D-8 (review fixes).** The proxy fails closed: an unreadable store refuses, an unreadable
  `spend.json` is no ceiling, and the day counter takes over a poisoned lock (its numbers stay
  right) instead of reading zero. The counter is the proxy's own and is never drained; the owner
  seeds it at start from `statistics.json`. A Claude answer with no model or with cut counts is
  priced at the dearest family and marked estimated.
- **D-9 (review fixes).** A reached ceiling opens the existing "double or keep" question once per
  group and UTC day (answered or not), from the owner's background pass.

## Waves

| Wave | What | State |
|---|---|---|
| T1 | Core: group kind `tokens`, zero folders allowed for it, `off` on groups, new keys land there, `default` refused for a key | done |
| T2 | Safety: bindings skip an off group, proxy refuses an off group, launches and fallback refuse, no auto-join, delete refused | done; Needs attention for a key left in `default` is open (board row to file) |
| T3 | Money: model per request, price table, cost per account and group per day in `statistics.json`, Statistics *List price* | done; `switchboard usage --money` not built (the Statistics report carries the cost) |
| T4 | Ceiling: turning on requires a daily ceiling; the proxy checks it per request against its own day counter; reached: refuse and ask once a day | done |
| T5 | Window: the group card with its switch, *Spending*, *Add folder…*; turn-off confirmation; the add-key dialog says where the key goes; scenarios SCN-042, SCN-100; Copylot | done; a window walk-through on a display is NOT_RUN |
| T6 | Docs (CONTRACTS, CLI, CHANGELOG, DEC-0023, terminology, strings) | done; release after merge |

## Review fixes (2026-11-17)

Two reviews of `token-group` found P0 to P2 defects; all P0 and P1 and the listed P2 are fixed on
this branch: the day counter (P0-1), a shared key (P0-2), the isolated launch and fallback (P1-3),
the bindings fingerprint (P1-4), fail-open reads (P1-5), the inactive ceiling, today's spend, the
reached question and its card state, the edit dialog, *Add folder…*, the turn-off confirmation,
the field note, the translated name and the add notice (P1-6 to P1-10), saturating sums, the
ceiling cache by path, balancing, `GroupSwitch` reporting, two doc comments, an explicit `tokens`
pool, the badge and terminology (P2). Tests: `switchboard-proxy` `tests::the_pay_per_token_group_*`,
`tests::a_reached_ceiling_*`, `tests::a_claude_answer_whose_counts_were_cut_is_never_free`,
`tokens::tests::*`, `prices::tests::*`; core `a_pay_per_token_key_stays_in_its_group_and_serves_no_other`,
`pool_flags_name_*`; runtime `tokens_group_tests::*`, `bindings::tests::a_group_switched_off_*`;
`scripts/test-ui-logic.mjs` SB-401 block.

## Review fixes, round 2 (2026-11-17)

The second review found no P0. Fixed on this branch: core P1-1 (every Claude API key in the
pay-per-token group, D-10 in [DEC-0023](../../DECISIONS.md#dec-0023-claude-api-keys-go-to-a-pay-per-token-group-that-starts-off-and-runs-only-under-a-daily-ceiling)),
P1-2 (an account switched off at home, D-11), and the P2 items (the spend file and its load before
listening, D-12; premiums and any model priced, D-13; the overshoot written down, D-14; settings
import, the Codex route, the fallback). Window P1-1 to P1-5 and its P2 items: the demo
(`?tokens=reached`), API keys, clearing the ceiling, launching a key, the translated name in every
picker, the move and edit locks, Retry, the background read, the reached copy, the add notice, turn
on with the ceiling, an empty Repositories part. The decision record was renumbered DEC-0023
(DEC-0022 is SB-403's). First real use (operator, 2026-11-17): one Claude API key (Claude Fable
5.2, priced) in three folders: add it (it lands in the group), *Add folder…* three times, set the
daily ceiling with "Turn the group on now", and sessions there run on it.

Next task for the next agent: file the board row for Needs attention naming a Claude key left in
the default group, then merge `token-group` through the repository's policy and release.
