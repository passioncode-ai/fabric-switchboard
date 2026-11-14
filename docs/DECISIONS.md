# Decisions: Fabric Switchboard

**Append-only.** Every settled thing that shapes the product, the architecture, security, data or
process and still holds. To change one, add a new entry and edit only the old entry's status line;
never renumber, never delete. The doc map ([DOCMAP.md](DOCMAP.md)) names this file as the decision
register. Decisions made before 2026-11-08 live where they were made (plans' `D-<n>`, board rows,
research notes); the ones still in force that the board cites are seeded below with their source.

**Next free ID:** `DEC-0021` (`DEC-0014`, `DEC-0016` to `DEC-0018` and `DEC-0020` are held for the window and fleet plan's modules)

## Format

```markdown
### DEC-0007: <one line, in the present tense>

- **Date:** 2026-08-03
- **Status:** Accepted
- **Context:** what forced the choice
- **Decision:** what was chosen, stated so it can be obeyed
- **Consequences / affects:** the documents that state it, each citing this id
- **Source:** where it was decided, and the commit that recorded it
```

---

### DEC-0001: Documentation is governed: a decision register, a doc map and a gate

- **Date:** 2026-11-08
- **Status:** Accepted
- **Context:** the setup audit ([plan](evidence/plans/2026-11-08-doc-audit.md), F-1) found no single
  home for decisions: plans, board rows and research notes each numbered their own.
- **Decision:** a decision still in force has an entry here with a `DEC-<nnnn>` id; the doc map
  names each kind of fact's home; `./scripts/check.sh` runs the documentation checks.
- **Consequences / affects:** [DOCMAP.md](DOCMAP.md), [evidence/backlog.md](evidence/backlog.md) (SB-308)
- **Source:** `/task-pipeline setup`, #529 `0e94629`

### DEC-0002: A refused Claude Code or Codex request moves once to the group's next account

- **Date:** 2026-10-11
- **Status:** Accepted
- **Context:** a request the provider refused with 429 did no work, and the group may hold another
  usable account.
- **Decision:** such a request moves **once**, before any byte reached the session, to the group's
  next usable account (`Store::failover`). An agent's API-key request is never moved, and no other
  replay, refresh or fallback is added as an incidental fix.
- **Consequences / affects:** `AGENTS.md` (Local rules), [CONTRACTS.md](CONTRACTS.md)
- **Source:** operator decision 2026-10-11 (SB-98), recorded in `4f98b72`

### DEC-0003: The app stays resident; closing the window is not a quit

- **Date:** 2026-10-05
- **Status:** Accepted
- **Context:** background rotation, renewal and backups run only while the app or `serve` runs.
- **Decision:** closing the window hides it and the owner keeps running; it ends only through
  Quit or a signal. The installed app starts itself at login with `--background`, on by default.
- **Consequences / affects:** `AGENTS.md` (Lifecycle, *Residency*)
- **Source:** operator decision 2026-10-05 (SB-28), recorded in `da9c5bf`

### DEC-0004: The installed app checks for updates every five minutes

- **Date:** 2026-10-17
- **Status:** Accepted
- **Context:** the check ran every 6 hours (SB-55, 0.6.1).
- **Decision:** the installed app reads the release feed 90 s after start and then every five
  minutes, also after a failure, as every PassionCode product does.
- **Consequences / affects:** `AGENTS.md` (Lifecycle, *Update check*)
- **Source:** operator 2026-10-17, recorded in `50b0520`

### DEC-0005: Usage counts are on from the first start

- **Date:** 2026-11-04
- **Status:** Accepted
- **Context:** issue #128's rule of 0.6.82 sent nothing, not even the install, before the person
  answered.
- **Decision:** counts are on and sent from the first start, a notice says so once, About turns
  them off for Switchboard only, and an install that answered "off" under the old rule while the
  shared switch read off stays off.
- **Consequences / affects:** [ANALYTICS.md](ANALYTICS.md)
- **Source:** operator decision 2026-11-04 (SB-246), recorded in `7037f2a`; Fabric's own rule is
  its ADR-0127, whose amendment 3 records this (passioncode-ai/fabric#65)

### DEC-0006: A person reads group, API keys and automatic switching

- **Date:** 2026-10-24
- **Status:** Accepted
- **Context:** the window said pool, project and Tokens for the same things in different places.
- **Decision:** every sentence a person reads says group (never pool), API keys for the page of
  keys and automatic switching for rotation; identifiers in commands, tools and JSON keep their
  names.
- **Consequences / affects:** [brand/terminology.md](brand/terminology.md), [brand/strings.md](brand/strings.md)
- **Source:** decisions D-1 and D-2 of the 0.6.82 review, recorded in `50ce27e`

### DEC-0007: A repository in no group stays in the default group

- **Date:** 2026-10-20
- **Status:** Accepted
- **Context:** it replaces REQ-002, under which a new repository could become a group of its own.
- **Decision:** a repository in no group stays in the default group and is never made a group of
  its own; a repository a group's agents create or work in joins that group by itself.
- **Consequences / affects:** `AGENTS.md` (Lifecycle, background monitor), [brand/terminology.md](brand/terminology.md)
- **Source:** SB-146, recorded in `fc31426`

### DEC-0008: Automatic switching of managed sessions is on by default

- **Date:** 2026-11-08
- **Status:** Accepted
- **Context:** frame-agent's runs in the group `agents` stopped on an account's weekly limit while
  automatic switching of managed sessions was off there; only the ordinary Claude Code's switching
  was on. Until now a group had no policy until someone saved one, and no policy meant off.
- **Decision:** every provider and group that has accounts serving it gets an enabled managed
  policy with the initial settings (90 % threshold, 10 points of headroom, 1800 s cooldown,
  300 s freshness), on every computer and for groups made later; a policy someone saved, off
  included, is never changed. Switching of the ordinary Claude Code (`claude_cli`) stays opt-in:
  it changes the sign-in every `claude` on the computer uses.
- **Consequences / affects:** [ACCOUNTS-AND-ROTATION.md](ACCOUNTS-AND-ROTATION.md), [CHANGELOG](../CHANGELOG.md), `AGENTS.md` (Lifecycle, background monitor)
- **Source:** operator, 2026-11-08 about 16:45 UTC, frame-agent session `0faeb32f`: «переводи на
  sergeysheleg4 если упрутся и велючи автопереключение по дефолту для все и передай в проекты
  фабрики куда надо», relayed on the board (request `486dd759`); SB-310

### DEC-0009: An e-mail domain suggests an organization; only the person or an administrator places an account

- **Date:** 2026-11-08
- **Status:** Accepted
- **Context:** design §10 of [organizations-2026-10-21](research/organizations-2026-10-21.md)
  placed an account in the organization holding its address's domain. On 2026-11-08 at 18:06 UTC a
  PassionCode.ai read added an organization with the domain of four accounts that personal groups
  held; the rule moved them, and every request and move on them was refused with
  `SEAT_OUTSIDE` until someone noticed. One organization may also use several domains.
- **Decision:** an account belongs to an organization by the person's word or by its provider
  team only. A domain suggests the organization; the person or an administrator confirms by
  assigning the account, signing it in to a group of that organization, moving it there or
  binding its group. Accounts the rule placed before keep their organization as the person's word
  where their groups agree, once per store, so nothing running changes.
- **Consequences / affects:** [CONTRACTS.md](CONTRACTS.md) (Organizations, SB-285),
  [ux/scenarios.md](ux/scenarios.md) (SCN-069, SCN-070, SCN-071, SCN-076, SCN-077),
  [ORGANIZATION.md](ORGANIZATION.md), [ORGANIZATION.ru.md](ORGANIZATION.ru.md), [CLI.md](CLI.md),
  [brand/strings.md](brand/strings.md), [CHANGELOG](../CHANGELOG.md), [evidence/backlog.md](evidence/backlog.md) (SB-285)
- **Source:** operator, 2026-11-06, studio-agent `d3848fb7` item 4: «не делать никакую привязку
  по имейлу жесткую»; the incident of 2026-11-08 on the operator's Mac; SB-285

### DEC-0010: An account given to an agent stays observed and is never chosen by itself

- **Date:** 2026-11-10
- **Status:** Accepted
- **Context:** a Claude subscription account was given to Frame Agent: a long-lived token of it
  sits in that agent's vault. The only way to keep Switchboard off it was `elsewhere` (SB-175),
  which also turns the account off, stops its renewal and its usage checks, so its activity was no
  longer visible.
- **Decision:** a separate state, *given to an agent* (`assigned_to`, the agent's name). The account
  stays enabled, renewed and observed on the usual schedule; every automatic path (rotation,
  failover, fallback chains, recovery, folder rules, launches, native activation by rotation)
  passes over it, and a manual switch or selection is refused unless the person uses it anyway,
  which is journaled. Giving it clears `elsewhere`; marking it `elsewhere` ends the assignment.
- **Consequences / affects:** [CONTRACTS.md](CONTRACTS.md) (SB-322),
  [ACCOUNTS-AND-ROTATION.md](ACCOUNTS-AND-ROTATION.md), [CLI.md](CLI.md),
  [ux/scenarios.md](ux/scenarios.md) (SCN-089), the skill's `references/tools.md`,
  [CHANGELOG](../CHANGELOG.md), [evidence/backlog.md](evidence/backlog.md) (SB-322); schema version 12
- **Source:** operator, 2026-11-10: «если аккаунт переходит на такой режим, чтобы он убирался из
  свитчборда ... такие аккаунты мы видели, мы трекали их активность ... но мы не могли на них перейти
  использовать без специального переключения»; SB-322

### DEC-0011: `serve` keeps itself current, and follows an app another account replaced

- **Date:** 2026-11-11
- **Status:** Accepted
- **Context:** a computer that runs only `switchboard serve` never updated by itself (SB-305 gave
  it a command, `switchboard update`, that nobody ran on a schedule): the Mac Studio's three
  accounts stayed on 0.7.49 while 0.7.50 and 0.7.51 shipped, and three `serve`s sharing one app
  would keep the old program even after it was replaced.
- **Decision:** `serve` checks the release feed as the app does (90 s after start, then every
  five minutes, DEC-0004) and installs a newer version itself when its account may replace the
  app; when it may not, it says so once (`update_waiting not_writable`) and downloads nothing. A
  `serve` whose program file was replaced (by another account's `switchboard update`, another
  `serve` or the app) drains and exits with status 75, so its supervisor (`KeepAlive
  {SuccessfulExit: false}`, systemd `Restart=on-failure`) starts it again on the new program.
  `<data>/auto-update` `off` installs nothing; a replaced program is still followed. A
  development build or a test's binary never does either.
- **Consequences / affects:** `AGENTS.md` (Lifecycle, *Update check*), `docs/CONTRACTS.md`;
  on a computer whose app another account owns, that account runs `switchboard update` on a
  schedule (the Mac Studio: studio-agent's `aiteam1`).
- **Source:** operator 2026-11-11 («мы обновляем весь софт автоматически, софт должен
  обновляться везде. И на студиях, типа у меня, и у сотрудников»), SB-333

### DEC-0012: Accounts are lent between the groups the person chose, by their last hours and days

- **Date:** 2026-11-13
- **Status:** Accepted (not built yet: SB-352)
- **Context:** automatic switching and the one move of a refused request stay inside one group
  (DEC-0002, DEC-0008). On 2026-11-12 22:55Z twelve agents of one group stopped on their limit
  while another group held an account with room, and an agent shared it by hand.
- **Decision:** off by default; the person chooses the groups that take part. Among them
  Switchboard lends accounts, never moves them: a lent account keeps its home group and serves the
  borrower as a shared account until the need passes. A pass every 15 minutes reads the last
  6 hours against the 7-day average (requests, limit refusals) and each account's fresh room; the
  neediest chosen group takes one account per pass from a chosen group that keeps at least two
  accounts with room; a lent account returns after 2 hours with no pressure in the borrower, or
  when its home group needs it; an account waits 1 hour between moves. A group not chosen is never
  read or touched; no account crosses an organization's boundary (SB-109); the ordinary CLI's
  account, an account given to an agent (DEC-0010), one used elsewhere, disabled or waiting for a
  sign-in is never lent. Every lend and return is recorded with its figures; turning balancing
  off returns every lent account. On the operator's MacBook the first groups are Agents,
  PassionCode, Apps & Soft and default, and every group made after it is turned on joins it.
- **Consequences / affects:** a setting `balancing.groups`, the Groups page, CLI `balance`, MCP
  `switchboard_balance`, `balancing.jsonl`; design
  [adaptive balancing](research/adaptive-balancing-2026-11-13.md).
- **Source:** operator 2026-11-13 in session `59745113` («адаптивная система на основе данных за
  прошлые часы и дни работы … перемешивать аккаунты между выбранными группами … но если группу не
  выбрали не трогаем, сделать такой выбор групп которые участвуют»), and his answers the same
  night: lend rather than move; the 15-minute pass, 6 hours and 7 days, two accounts kept, return
  after 2 hours, 1 hour between moves; on this MacBook Agents, PassionCode, Apps & Soft, default
  «и все новые».

### DEC-0013: An account on another machine belongs to that machine's group, and to no group here

- **Date:** 2026-11-13
- **Status:** Accepted (built in `sb-355-machine-groups`: SB-355)
- **Context:** an account moved to another machine (`accounts send`, SB-283) stayed in its old
  group here, turned off, with its machine in free text (`locations`, SB-284). On the operator's
  MacBook 17 accounts were used elsewhere, under five spellings of three machines, mixed into the
  groups that work here.
- **Decision:** every other machine an account lives on is a group of its own here, a *machine
  group*: one per ssh destination (the user, host and port `ssh -G` resolves; every name used for
  it is an alias, so `studio-sshlg` and `sshlg@100.69.20.19` are one machine, and two macOS users
  on one Mac are two machines). It has no folders and holds exactly the accounts that live there.
  An account joins it when a move to that machine succeeds and leaves every group here in the same
  change (its home becomes the machine group; shares, loans and selections go; the group it left
  is kept to come back to). It comes back with `accounts recall`, into the group named, else the
  group it left, else the default group; from one machine to another is a call back, then a move.
  A machine group never routes a request, rotates, fails over, lends or borrows here, and is
  never carried to another machine. Accounts already marked elsewhere join their machine once, by
  the move recorded in `seats.jsonl`, else by their location matched to a known machine; one whose
  machine cannot be told stays where it is and is reported, never guessed.
- **Consequences / affects:** `accounts.json` gains `machines` and `returns_to` (schema version 14),
  the CLI `machines` and `accounts recall --pool`, MCP `switchboard_machines`, the Groups page's
  *Other machines* (SCN-092), [MOVING](MOVING.md#machine-groups), [CONTRACTS](CONTRACTS.md#machine-groups-sb-355),
  design [machine groups](research/machine-groups-2026-11-13.md).
- **Source:** operator 2026-11-13 in session `59745113` («вынести аккаунты, которые уходят на
  какие-то удаленные машины, в отдельные группы этих удаленных машин … Каждая удаленная машина -
  это новая своя группа … перекидывать аккаунты между группами … так я буду видеть все свои
  удаленные машины и какие там на них стоят аккаунты»).

### DEC-0015: Attention is a view, not a group

- **Date:** 2026-11-14
- **Status:** Accepted (built in `w1-backend`: SB-365, WF-3.1 and 3.3)
- **Context:** accounts that need a person (a sign-in that ended, a seat an organization ended, a
  renewal the provider refused, usage checks that keep failing, a move whose answer was lost) were
  only badges on their rows, spread over the groups, and a spending ceiling's open question lived
  on another page. The operator asked for them at the top, under *Attention*, and for each to come
  back where it was once fixed.
- **Decision:** what needs a person is one list built on each read from what the owner already
  knows (the snapshot, the sign-in verdicts in memory, `seats.jsonl`, `spend.json`); it is never
  stored and never a group. An account listed there stays in its group with its selection, and
  nothing moves when it is listed, fixed or leaves the list. Each item names its reason and the one
  action that fixes it, as an MCP tool and a window control. The window, `switchboard attention`
  and MCP `switchboard_attention` read the same answer.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#attention-sb-365), [CLI](CLI.md), the
  skill's tool reference, a lost move's `unknown` line in `seats.jsonl`; the Accounts page's
  *Needs attention* section (WF-3.2).
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-03-attention.md))
  («Подписки, которые у нас с ошибками, нужно залогиниться, их сразу поднимаем вверх, в отдельную
  категорию, типа "Attention" … после залогинивания они возвращаются туда же, где были»).

### DEC-0019: Every window action has an MCP tool or a written reason

- **Date:** 2026-11-14
- **Status:** Accepted (built in `w1-backend`: SB-363, WF-1.3)
- **Context:** the CLI to MCP gap was closed by a test (SB-134), but the window's commands were
  not checked: about a hundred Tauri commands and no table. The operator asked that nothing work in
  the window and not over MCP; the other way round is allowed.
- **Decision:** every command registered in the window's `tauri::generate_handler![..]` names the
  MCP tool that does the same, or a written reason it has none: the person's own act (a secret,
  another tool's sign-ins, a removal that cannot be restored), launching an agent from an agent
  (SB-64, SB-74), or window plumbing (a system picker, a notice shown once, the window's start
  signal, the tray's language). The test `every_window_command_has_an_mcp_tool_or_a_reason`
  (`src-tauri/src/parity.rs`) fails on a command with no row, a row whose command is gone and a row
  naming a tool the MCP server does not list. A new window command lands with its tool or its
  reason in the same change. MCP may do more than the window. This adopts the contract's DEC-0121
  here ahead of SB-200.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#window-to-mcp-parity-sb-363); every later
  module of the [window and fleet plan](plans/2026-11-14-window-and-fleet/README.md).
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-01-foundations.md))
  («не должно быть никакой функции, которая работает в интерфейсе и не работает в MCP. Могут быть
  наоборот»).
