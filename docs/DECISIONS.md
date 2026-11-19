# Decisions: Fabric Switchboard

**Append-only.** Every settled thing that shapes the product, the architecture, security, data or
process and still holds. To change one, add a new entry and edit only the old entry's status line;
never renumber, never delete. The doc map ([DOCMAP.md](DOCMAP.md)) names this file as the decision
register. Decisions made before 2026-11-08 live where they were made (plans' `D-<n>`, board rows,
research notes); the ones still in force that the board cites are seeded below with their source.

**Next free ID:** `DEC-0034` (`DEC-0027` is held for SB-418, a person's off switch for an account, and `DEC-0028` for SB-419, groups fed from the default group, both on open branches; `DEC-0018` and `DEC-0020`, held for the window and fleet plan's modules, were written on 2026-11-17)

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
- **Status:** Accepted; its "work in" half superseded on 2026-10-22 by SB-171 (see *Amended*)
- **Context:** it replaces REQ-002, under which a new repository could become a group of its own.
- **Decision:** a repository in no group stays in the default group and is never made a group of
  its own; a repository a group's agents create or work in joins that group by itself.
- **Amended 2026-10-22 (SB-171, tagged 0.6.68, first published in 0.6.70).** A folder never joins a group because a group's agent
  worked in it: that join (SB-131's `worked` rows) had moved 33 folders nobody chose, with their worktrees
  and clones, into three groups (SB-171). A folder joins a group by itself only as a worktree or clone of a repository the group
  holds, or as a repository of a source the person set for the group (`github.com/<owner>`); a
  deleted folder leaves by itself. The scan that found worked folders was removed and a `worked`
  row is never added (test `a_folder_never_joins_a_group_because_its_agent_worked_there`,
  [`suggest.rs`](../crates/switchboard-runtime/src/suggest.rs); PR #282 `dd4a85ef`; CHANGELOG
  0.6.68). Operator 2026-10-22: «не может одна папка гулять … самостоятельно переноситься, нет».
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
- **Status:** Accepted (built in part: waves 1 and 2 released in tags `v0.7.73` and `v0.7.74`, PR #667 `02d960a2` and #671 `313596fd`, first published as release `v0.7.76`; the early pass of WF-5.2e in `v0.7.103`, PR #759; wave 3 open, SB-352)
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

### DEC-0014: Settings owns standing configuration

- **Date:** 2026-11-14
- **Status:** Accepted (built in `w1-window`: SB-363, WF-1.1)
- **Decision:** what stays set until someone changes it lives on one Settings page: automatic
  switching, balancing between groups, Codex in group folders, terminals in organization folders,
  running in the background, updates, language and appearance, backups and usage counts. Other pages
  show the state in one line and link there; Accounts keeps only "Automatic switching is on for N
  groups" and the warnings about the accounts themselves.
- **Consequences / affects:** the window's `settings` page (SCN-093), About keeps the product,
  terms and tour, Groups points to Settings; [the plan](plans/2026-11-14-window-and-fleet/WF-01-foundations.md).
- **Source:** operator 2026-11-14 («automatic switching, вот этот меню большое, пусть оно будет в
  настройках, переорганизовать, чтобы оно не показывалось здесь на главном экране»).

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

### DEC-0016: A backup group lends first, keeps no minimum and never borrows

- **Date:** 2026-11-14
- **Status:** Accepted (built on branch `wf6`: SB-368, WF-6)
- **Context:** balancing (DEC-0012) lends between working groups only, and each lender keeps two
  accounts with room. The operator asked for a group of accounts kept in reserve that supports a
  group out of room and comes back once that group has its own room again. The plan said a backup
  group holds no folders; the store refuses a group without a folder (`projects.rs:445`), and an
  older Switchboard reading such a store would refuse it too, so that wording could not ship.
- **Decision:** any chosen group but `default` can be marked backup. It never borrows, keeps no
  minimum, and is asked first: a group out of room takes a backup group's account before any other
  group's. A backup account goes home once the borrower has an account of its own (its own or one it
  shares by the person's choice, never another loan) with room, after the account's one-hour
  cooldown; and at once when the backup group's own folders have requests and no room
  (`home_needs`). Organization boundaries hold as in DEC-0012. The mark lives in `balancing.json`
  (`backup`), not in `accounts.json`: an older Switchboard reads the store unchanged and treats the
  group as an ordinary chosen one. Marking a group chooses it; taking it out of the chosen groups
  unmarks it.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#backup-groups-wf-6-dec-0016), [CLI](CLI.md)
  `balance groups --backup-add|--backup-remove`, MCP `switchboard_balance` `backup_add`,
  `backup_remove`, the window's *Backup: lends only* switch (SCN-090 step 7), `balance status`
  `backup` and `available[].backup`.
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-06-backup-group.md))
  («добавить возможно группу backup аккаунтов … когда … не заканчивают аккаунты тогда они туда
  переключаются и поддерживают эту группу пока там не появится свой аккаунт потом возвращается
  обратно в бэкап»).

### DEC-0017: The accounts on other machines are read in the background, and still never routed here

- **Date:** 2026-11-14
- **Status:** Accepted (built on branch `wf4`: SB-366, WF-4); amends DEC-0013's "on request";
  its cadence and what is read amended by DEC-0033 (every minute, with sessions and health)
- **Context:** an account moved to another machine is turned off here (DEC-0013), so its row read
  "Not checked while disabled", and its copy there was read only when a person asked
  (`machines --live`). The operator asked that the usage of accounts on another device come here
  and that the network work not fail.
- **Decision:** the owner reads each machine group's Switchboard over ssh every 15 minutes while it
  runs: one `accounts list` call per machine, 20 seconds at most, machines one after another, on a
  thread of its own (never under the store's lock, never on the async worker). It keeps each
  account's used percent, usage status and whether its copy is on in `remote-usage.json` (0600; ids,
  numbers, codes and times, no label). A failed read keeps the last good one with its age and a code
  (`unreachable`, `refused`, `unreadable`); three in a row read that machine once an hour. The
  numbers are shown (the Accounts row, `machines list`, MCP `switchboard_machines`, Attention's
  `refused_there`) and never used to switch, rotate or lend here: DEC-0013's routing rule stands.
  When Fabric's hub carries each machine's holdings (SB-177), it replaces ssh.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#background-reads-of-other-machines-sb-366-dec-0017),
  the lifecycle rows in AGENTS.md (a child `ssh` every 15 minutes, `remote-usage.json`), SCN-092
  step 8, the Accounts row of an account on another machine.
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-04-remote-usage.md))
  («чекинг подписок, который на другом устройстве, должен все равно сюда синхронизироваться как-то,
  продумать все это сетевое взаимодействие … чтобы не вылетали»).

### DEC-0018: Switchboard is the machine's place for agent settings

- **Date:** 2026-11-14
- **Status:** Accepted (built in part: WF-10.1, the *Agent settings* block on Agents, released in
  0.7.82, PR #688 `d310411e`, SB-372; WF-10.2 templates and WF-10.3 shared skills and plugins
  open). Held in the window and fleet plan and written here on 2026-11-17 from that plan; nothing
  was decided about it since.
- **Context:** the settings every session Switchboard starts runs under (permissions, the default
  agent, which agents take over work, fallback chains) were reachable over MCP and the CLI and
  only partly in the window, and other products offered their own permission choices.
- **Decision:** Switchboard is the machine's place for agent settings: permissions (bypass or
  ask), the default agent, which agents take over work, fallback chains, versioned agent
  templates (a change is a new copy beside the old one) and the skills and plugins the homes it
  makes carry. Other products link to it instead of keeping their own copy. Every control has an
  MCP tool (DEC-0019).
- **Consequences / affects:** the Agents page's *Agent settings* block (`src/main.ts`, WF-10.1),
  MCP `switchboard_agent_set` and `switchboard_chain_set`, the
  [WF-10 module](plans/2026-11-14-window-and-fleet/WF-10-agents.md),
  [evidence/backlog.md](evidence/backlog.md) (SB-372). Whether Fabric and Fabric Dashboards drop
  their own permission choices was asked on the board (WF-10, *Questions*). Code comments of the
  session duty officer cited this id by mistake until 2026-11-17; the duty officer is DEC-0021.
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-10-agents.md))
  («это централизованное место switchboard … темплейты … скиллы, возможно, какие-нибудь
  подгрузим, типа общие плагины»), recorded in the plan by #684 `fcac671c`.

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

### DEC-0020: Routing between the person's own machines is opted in and moves a sign-in, never a request

- **Date:** 2026-11-14
- **Status:** Assumed, not built (SB-374, WF-12.3): it rests on the window and fleet plan's
  assumed answer 3; the plan says the question was asked on the Fabric board, and no answer is
  recorded in this repository. It narrows DEC-0013 once built. Held in that plan and written here
  on 2026-11-17 from it.
- **Context:** an account on another machine belongs to that machine's group and is never routed
  here ([DEC-0013](#dec-0013-an-account-on-another-machine-belongs-to-that-machines-group-and-to-no-group-here)); its
  usage is read in the background, still with no routing (DEC-0017). The operator asked to think
  about automatic routing between machines.
- **Decision:** opted in per pair of the person's own machines, only those Switchboard already
  moved accounts to: when a group here had no account with room for two passes and another of
  the person's machines holds an account with room in its machine group there, Switchboard moves
  that sign-in here (send and recall, recorded in `seats.jsonl`), at most once an hour per
  account, and moves it back when that machine needs it. Requests are never relayed between
  machines. This supersedes DEC-0013's "never routed" for that case only. An organization's
  devices (target-state plan W2) and a seat delegated to a teammate (DEC-0025) are separate.
- **Consequences / affects:** the [WF-12 module](plans/2026-11-14-window-and-fleet/WF-12-fleet.md)
  (12.3), [evidence/backlog.md](evidence/backlog.md) (SB-374); when built, `switchboard_balance`
  gains `machines`.
- **Source:** operator 2026-11-14, the window review ([plan](plans/2026-11-14-window-and-fleet/WF-12-fleet.md))
  («быстро иметь возможность перекинуть аккаунты между машинами, также подумать про
  автоматический роутинг между машинами»), the plan's question 3, recorded by #684 `fcac671c`.

### DEC-0021: A duty officer restarts stalled Claude Code sessions and closes finished ones

- **Date:** 2026-11-15
- **Status:** Accepted (built on branch `duty`: SB-384)
- **Context:** the session watch (SB-379) reports a stalled or duplicated session but acts on
  none, and sessions that stall or finish their work keep their terminal and memory for hours.
  The operator asked for a duty officer in the lifecycle that closes a hung session and resumes it
  in a new terminal, restarts a session that has run for hours, and takes away one that finished
  and only hangs.
- **Decision:** the one-minute session watch calls the duty officer; every 5 minutes, while it is
  on (default on, `duty.json`), it reads the interactive Claude Code sessions of this computer
  (process plus its `sessions/<pid>.json`) and acts on at most 3 of them:
  - a newer second process of one session: closed (`duplicate`);
  - idle for `idle_minutes` (60): closed (`idle`), the work is in the transcript;
  - idle between turns for 2 minutes and started over `max_age_hours` (4) ago: restarted (`old`);
  - waiting for an answer for `waiting_minutes` (60): restarted with a prompt to continue (`waiting`);
  - busy or in a shell with no status change and no transcript write for `stuck_minutes` (45):
    restarted with a prompt to continue (`stuck`).
  Close is SIGTERM, then SIGKILL after 10 s. Restart is close, then a new Terminal window that runs
  `claude --resume <session>` in its folder with the home's own exports and config folder (a
  Switchboard home's `launch.command`, minus its launch id), and the bypass setting of SB-298. A
  session restarted 3 times in 6 hours is closed instead. A session in a Fabric Dashboards console
  is left to Dashboards. A session hosted elsewhere (Orca's terminal host, iTerm) is acted on like
  one in Terminal, and its resumed copy opens in Terminal, since Switchboard cannot open a tab in
  another program; the host is recorded with each action. Codex sessions are not watched yet.
  Every action is a line in `duty.jsonl` (500 kept) and an oplog event `session_duty`, codes only.
  Off with `switchboard duty off`, MCP `switchboard_duty {on: false}` or the window's Settings.
  Windows only reports.
- **Amended 2026-11-15 (0.7.95).** On 0.7.93 the duty officer acted on the operator's sessions in Orca: 7 were
  restarted into new Terminal windows, where every hook failed (`node: command not found`, no PATH),
  and 6 idle ones were closed, which the operator saw as working sessions closing; windows of
  sessions it ended stayed open. Now it acts only on sessions Switchboard started (a launch record
  of the last 30 days names the session, or the duty officer resumed it); a session the person
  started in their own terminal, any host, is listed (`left_to_person`) and never touched. The
  resume script exports the PATH a run started by the owner gets, and runs `claude` without
  `exec` so an end by a signal leaves status 0 and the window closes. Operator: «вот эти окна потом
  не закрываются, а работающие сессии отдельные закрылись».
- **Amended again 2026-11-15 (0.7.96).** The operator: «чтобы они не зависали и не убивались раньше
  чем доработают, только когда закончила сессия работу можно ее кикать, либо если диагноз застой
  - типо работает но нет». The 0.7.95 scope (only sessions Switchboard started) is lifted: every
  interactive session outside Fabric Dashboards is watched again, under rules that never end work.
  A session is closed only when finished (idle for `idle_minutes` with none of its own commands
  running: a background task that will wake it is work) and restarted only when stalled (busy,
  status and transcript still for `stuck_minutes`, no CPU in its process tree since the previous
  check, no command of its own running or one hung three times as long). `waiting` (a question for
  the person) and the age rule `old` are gone; of two copies of one session only a finished one is
  closed. Live dry read on the MacBook with this build: 30 sessions watched, 16 running commands,
  nothing due (the one duplicate has both copies working).
- **Amended 2026-11-16 (SB-392 S-1).** A session that set itself a wake-up (`ScheduleWakeup`, at
  most an hour) reads `idle` while it waits, so with `idle_minutes` at 60 it could be closed in the
  minute before it wakes: an end before the work finished. Idle closes only after 65 minutes
  whatever `idle_minutes` says (`duty::WAKE_UP_FLOOR`).
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#session-duty-officer-sb-384-dec-0021), the
  lifecycle rows in AGENTS.md (signals to session processes, Terminal windows, `duty.json`,
  `duty.jsonl`), SCN-096, Settings → Session duty officer.
- **Source:** operator 2026-11-15 in this session («Каждую минуту дежурный пробегает по сессиям и
  смотрит, не стоят ли они … закрыть предыдущую сессию и переоткрыть ее в новом сессии … сессий,
  которые … висят больше … 2-3-4 часов … просим ее остановиться, перезагружаем и продолжаем … если
  сессия уже закончила работа, она все еще висит, мы ее уводим»).

### DEC-0022: An organization is a label, never a lock on where an account works

- **Date:** 2026-11-17
- **Status:** Accepted (built on branch `org-seats`: SB-403)
- **Context:** an account's organization decided where it could work. A group took only accounts
  of its own organizations: `SEAT_CHOSEN` refused a placement or a new sign-in, `SEATS_ELSEWHERE`
  a binding, `SEAT_OUTSIDE` a selection and every request on the proxy, `ASSIGN_OUTSIDE` a word on
  an account whose team a group served (SB-109, SB-282, SB-313, SB-314). On 2026-11-17 the
  operator's Mac showed the cost: `accounts recall` of two organization accounts into personal
  groups answered `placed_error` and left them in `default`; assigning an organization alone to an
  account a personal group used was refused; one Claude Team seat placed in an organization pulled
  its teammates the person never named; and an organization's checklist read "no signed-in account"
  while one worked for it, and asked for "your {organization} address" although an organization may
  have several domains or none.
- **Decision:** an organization is a label on an account (whose work it does, what it is delegated
  for), never a lock. A person's act (assigning an organization, adding one, placing, moving,
  sharing, binding a group, signing in into a group, calling an account back) is never refused
  because of organizations; when the account is outside the group's organizations the answer
  carries a warning (`outside_organization`) and the group serves it. Local work is bounded only by
  group folders: the proxy serves any account the group holds. Automatic acts may prefer: automatic
  switching and the SB-98 move choose the group's own organizations' accounts first and take
  another when none has room; balancing (DEC-0012) still lends nothing across an organization's
  boundary. Assigning one seat of a provider team places that seat only; its team follows only
  when the person asks (`team`); a seat of the team signed in later is attributed by its team only
  after the team is claimed. The group owns the data, the account only pays: a session in a group's
  folder runs in the group's context whatever account (or key) serves it, the first of the group's
  contexts the account holds, else the group's first (round 1 of the review, 2026-11-17: before,
  such a launch ran personal while a terminal in the same folder ran in the organization). Automatic
  picks outside the group's organizations are recorded (`failover_outside`, `outside`). Statistics
  count an account under its first context once. Known limitation (round 2 of the review): a launch
  outside a group's managed route uses the account's isolated home `homes/<account id>`, one per
  account, so an account serving a personal group and an organization's group keeps both groups'
  Claude state there; managed launches run in each group's own runtime and are not affected. Keying
  that home by account and context would move every existing home and sign-in, so it is left to a
  board row. Supersedes the refusing parts of SB-109, SB-282,
  SB-313 and SB-314 and the domain-only seat step of SB-202; DEC-0009 (a domain only suggests)
  stands, and an organization may have several domains or none.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#an-organization-is-a-label-sb-403-dec-0022-2026-11-17),
  [ux/scenarios.md](ux/scenarios.md) (SCN-062, SCN-069, SCN-070, SCN-076, SCN-077),
  [CLI.md](CLI.md) (`org assign --also`, `--team`), the skill's
  [tools reference](../plugins/switchboard/skills/switching-accounts/references/tools.md),
  [brand/strings.md](brand/strings.md), [ORGANIZATION.md](ORGANIZATION.md),
  [ORGANIZATION.ru.md](ORGANIZATION.ru.md), the plan
  [org-seat-delegation](plans/2026-11-17-org-seat-delegation/README.md), [CHANGELOG](../CHANGELOG.md),
  [evidence/backlog.md](evidence/backlog.md) (SB-403).
- **Source:** operator 2026-11-17: «У нас может быть несколько e-mail на организацию … чтобы это не
  было блокером. А вообще у нас в организации может и не быть имейлов … тогда мы другие способы
  входа используем»; «чтобы подписки, которые у нас привязываются к организации, они не блочились,
  чтобы мы могли их все равно передавать между группами … другую организацию поставить … все
  избирать, а не ограничивая всю локальную работу. Локальную работу подписки на машине
  ограничивается папками и PassionCode-ами, которые там уже создают сами пользователи.»; the live
  call back of 2026-11-17 03:40 UTC relayed by the coordinating session.

### DEC-0023: Claude API keys go to a pay-per-token group that starts off and runs only under a daily ceiling

- **Date:** 2026-11-17
- **Status:** Accepted (built on branch `token-group`: SB-401; plan
  [2026-11-17-token-group](plans/2026-11-17-token-group/README.md))
- **Context:** a Claude API key is charged per token and has no limit of its own in Switchboard;
  in the default group every folder in no group ran on it. The operator, 2026-11-17: «Такие сессии
  за токены они сразу добавляются в отдельную папку, в которой нету никаких проектов … Изначально
  она выключена. И обязательно, когда при включении контролируем usage».
- **Decision:**
  - D-1: one reserved group, made on first need with the `tokens` flag (pool `tokens`, or the
    first free `tokens-<n>`), named *Pay-per-token keys*. It may hold no folder and holds only
    Claude API-key accounts.
  - D-2: a Claude API key added to the default group (or to `tokens` before the group exists)
    lands there; an explicit other group is still allowed; existing keys stay where they are.
  - D-3: the group starts off. A group switched off routes no folder (bindings), and the proxy,
    launches and the fallback refuse its accounts; nothing joins the pay-per-token group by itself
    (sources, found worktrees, settings import), and it is not deleted while it holds a key.
  - D-4: turning it on needs a daily ceiling (`spend.json`, scope `project:<pool>`, no OpenRouter
    key of the group's own needed). The proxy checks it before every request a key of the group
    serves, against its own day counter (never drained, seeded at owner start from
    `statistics.json`); reached, it refuses and the owner opens the ceiling's question once per
    group and UTC day.
  - D-5: money is Anthropic's list price of the tokens each answer reports (`prices.rs`, source
    and read date beside the table); an unknown model, an answer with no model, or counts that
    were cut are priced at the dearest family and marked estimated, never free.
  - D-6: folders join the group one by one, by the person.
  - D-7 (review, 2026-11-17): a key of the pay-per-token group never leaves it nor serves another
    group: no share, no loan (balancing never names the group), no move by a save or a restore;
    the person removes the account to take it out. A key launches only in managed mode (an
    isolated home would hold its credential and reach the provider past the ceiling). The proxy
    also checks the chosen account's home group after route selection and before an SB-98 move.
  - D-8 (review): the proxy fails closed: an unreadable store refuses (`STORE_UNREADABLE`), an
    unreadable `spend.json` is no ceiling, and the day counter takes over a poisoned lock rather
    than read as zero.
  - D-10 (second review, 2026-11-17; supersedes D-2's "an explicit other group is still allowed"):
    every Claude API key the person adds, restores or lets a group go lands in the pay-per-token
    group, whatever group was named; once that group exists a Claude API key moves only into it
    and is newly shared or lent to no group (`CLAUDE_KEY_STAYS`); `accounts move` refuses one to
    any other group. A key in another group from before stays where it is (nothing that works is
    broken) and may be moved in. Any Claude API key launches only in managed mode, so its
    credential never lands in a session's home; the fallback and dispatch pass one over. Reason:
    the simplest rule that leaves no unguarded key, as the reviewer recommended.
  - D-11 (second review): an account whose home group is switched off serves no group sharing it
    (route selection, rotation and failover pass over it; the routes it held move to another
    account of the same kind there); balancing leaves an off group out and returns its loans. (Its
    Codex route: see D-15.)
  - D-12 (second review): the proxy loads the day's spend before it listens, from
    `tokens-spend.json` (written after each answer of a pay-per-token key) and
    `statistics.json`, the larger winning; a file that exists and cannot be read leaves the day
    unknown, and the group refuses until the next UTC day (`TOKENS_SPEND_UNKNOWN`).
  - D-13 (second review): any model a Claude route answers with is priced (an unknown one at the
    dearest family); web searches at $10 per thousand; a prompt past 200,000 tokens at the
    long-context premium (input x2, output x1.5) and the faster tier at six times the price, both
    marked estimated, so a ceiling never reads less than was spent.
  - D-14 (second review): the ceiling is checked before each request, so requests already in
    flight when it is crossed still finish; the day can pass the ceiling by what those requests
    cost (documented in CONTRACTS), never by a request started after.
  - D-15 (third review, 2026-11-17; supersedes D-3's "routes no folder" and D-11's Codex line): a
    group switched off keeps its folders' routes, closed, so a session there gets the proxy's
    `GROUP_OFF` refusal and never falls through to the ordinary sign-in. One rule for every group,
    ordinary, organization-bound or pay-per-token: Claude bindings, the Codex route and the shell
    map keep an off group's folder pointed at it; only a folder an on group also holds is served
    by that on group (an off group is listed last). Reason: before, an off group bound to an
    organization dropped out of the bindings and its sessions ran on the person's own sign-in,
    which is what switching it off was meant to stop.
  - D-16 (third review): the rule that a Claude API key lands in the pay-per-token group lives in
    the store's save path (`save_account` with upsert), so every way in (the add dialog, the CLI,
    Claude Swap's import, a restore) goes through it; the plain `add` stays a primitive that the
    operation layer routes itself. A key is never placed in a machine group, and one that comes
    home goes to the group it left by `leaving_pool` (the pay-per-token group).
  - D-17 (third review): a Claude API key from before, outside the pay-per-token group, is never
    selected or launched by the window, the CLI or the fallback (`KEY_OUTSIDE_TOKENS`); the
    window offers *Move to Pay-per-token keys*, and a move to `tokens` makes the group when it is
    missing. When a key enters that group, the background pass removes `ANTHROPIC_API_KEY` from
    the isolated home an earlier version wrote for it (`homes/<id>/settings.json`). The day's
    spend file is written off the counter's lock, newest first; a write that fails leaves the day
    unknown (the group refuses) until a write succeeds, retried at most every 5 s. Switching a
    group off moves its routes only to an account the usual checks allow (enabled, not given to
    an agent, not used elsewhere), one of the group's own organizations first and one outside
    them after it (DEC-0022: an organization never refuses).
  - D-18 (fourth review, 2026-11-17; sharpens D-10 and D-17): a Claude API key outside the
    pay-per-token group is never newly selected, rotated or failed over to, launched in any mode,
    or named by a new route (`KEY_OUTSIDE_TOKENS`, in the store's select and at every publish).
    The rule reads the group: the owner makes it at start, before the proxy listens, when a
    Claude API key is saved without it, and every later save of a key makes it. A route that
    already named it before the upgrade keeps serving (nothing that works is broken),
    and Attention lists the key (`key_outside_tokens`) with *Move to Pay-per-token keys*; its
    account row offers the same move instead of *Select*. A spend file that could not be saved
    refuses with its own sentence (`TOKENS_SPEND_UNSAVED`, until a save succeeds, tried every
    5 s off the request's thread), apart from a day that could not be read
    (`TOKENS_SPEND_UNKNOWN`, the whole UTC day). The group a saved key needs is made in the same
    publish as the key, so a save that fails leaves none, and a restore counts such a key failed
    and goes on. A switched-off group's folder that a group switched on also holds is that
    group's in the Claude bindings too, even when it writes no route there.
  - D-19 (fifth review, 2026-11-17): every way a key reaches the group makes it in the same
    publish as the key: the add command (`Store::add_routed`), a save (`upsert`) and a move
    (`Store::move_key_to_tokens`), so nothing that fails leaves an empty group. For a Claude API
    key, `tokens` names the pay-per-token group whatever its pool id (`tokens-2` when an ordinary
    group holds `tokens`), and Attention gives the group's own id in its action. A restore brings
    the groups back before the keys, so an ordinary group's `tokens` and the backup's own
    pay-per-token group (with its folders) both return, and a key is never counted as returned
    to the default group. Known limit: the CLI and MCP without an owner read no group until an
    owner has started once, so until then they may still select a key from before; the owner's
    start makes the group before its proxy takes a request.
- **Consequences:** a key in the default group from before stays there, named under Attention
  until it is moved (D-18); turning a group off stops its sessions at their next request; the reached question
  is the same one paid hand-overs use, and *Double* raises the ceiling at once.

### DEC-0024: Provider keys come from Observatory by name, through a pipe, and every launch on one is counted

- **Date:** 2026-11-17
- **Status:** Accepted (built on branch `observatory-keys`: SB-404 waves K1 to K3; plan
  [2026-11-17-observatory-keys](plans/2026-11-17-observatory-keys/README.md))
- **Context:** the operator, 2026-11-17: «Чтобы можно было добавить токены ChatGPT, OpenRouter,
  Anthropic, либо они подтягивались в Observatory автоматически, и можно было бы их использовать
  для запуска агентов сразу». Project Observatory already holds the operator's provider keys by
  name and hands a value only to a command's environment (`use_secret.py run --vault-only`);
  Switchboard saved a key only from a person's pipe.
- **Decision:**
  - O-1: Switchboard lists the keys Observatory holds from `vault.py list` (names and metadata,
    never a value): slots named `ANTHROPIC_*`, `OPENAI_*` or `OPENROUTER_*` with `KEY` or `TOKEN`
    in the name, without `ADMIN`, `MANAGEMENT`, `MGMT`, `BYOK`, `PROVISIONING` or `PREVIOUS`
    (no agent runs on an organization's admin key, a provisioning key, a BYOK credential or an
    archived copy), and none Observatory marks "not a credential". Observatory missing is an
    answer (`state: absent`), never an error.
  - O-2: adding moves the value through a pipe only. The CLI, the MCP server or the window asks
    the owner for a plan (`Operation::ObservatoryKeyPlan`, which checks the slot against the list
    and what is saved here), then runs each save as `use_secret.py run --vault-only --env E
    project NAME -- /bin/sh -c 'k="$NAME"; unset NAME; printf %s "$k" | "$0" "$@"' switchboard
    --data-dir D --json <save>`: the engine puts the value in the shell's environment, the shell
    moves it into an unexported variable and writes it to the stdin of the ordinary save
    (`accounts add --secret-stdin`, `agents anthropic set --key-stdin`, `agents openrouter set
    --key-stdin`), which reads it as when a person pipes a key. No process of Switchboard's has
    it in an argument or its environment; the engine scrubs it from what the save prints. A save
    takes at most a minute; macOS and Linux only (`UNIX_ONLY` elsewhere).
  - O-3: where a key goes. An Anthropic key becomes a Claude API-key account (by default), which
    lands in the pay-per-token group (DEC-0023) and is metered there; `to: agent` or `both` also
    saves it as a named key for Hermes's direct launch (SB-390), which no ceiling counts and the
    answer says so. An OpenRouter key becomes this computer's key or a group's, under its ceiling
    (SB-72). A key already saved there is kept unless replaced; a slot already added as an account
    is refused. An OpenAI key is listed and not added (`OPENAI_NOT_YET`): no ceiling counts one
    (the proxy prices Anthropic's models only, and a Codex API-key account has no ceiling), so
    adding it would let an agent spend it unchecked. It waits for OpenAI prices in the proxy.
  - O-4: the slot each saved key came from is recorded in `observatory-keys.json` (0600: slots and
    targets, never a value; the newest 256, a target gone is dropped), so a rotation in
    Observatory can be followed later (K4).
  - O-5: launches on a key are counted. Claude Code on the pay-per-token group runs through the
    proxy (managed), with `--model` when one is named. Hermes on the pay-per-token group runs
    through the proxy too: the launch writes a Hermes home of its own whose `config.yaml` names a
    provider `switchboard` pointing at the group's route (`transport: anthropic_messages`, the
    agents' capability by `key_env`, never a provider key) and `auth.adopt_external_logins:
    false`, and starts `hermes --provider switchboard -m <model>` (a model is needed there). Every
    answer is then priced at Anthropic's list price against the group's daily ceiling, so the
    plan's K-4 is met without a ceiling of a named key's own. Hermes on a named Anthropic key
    (`--anthropic`) stays as SB-390 built it, unmetered, and is labelled so (`metered: false`).
  - O-6: `launch_keys` lists the pay-per-token group's keys for Claude Code and Hermes
    (`kind: group`), usable only when the group is on, has a ceiling, holds the folder and has the
    key selected; otherwise `why_not` says which.
  - O-7 (review round 1, 2026-11-17): a key Observatory records as expired is listed and not
    added (`KEY_EXPIRED`); a model goes only with a named key (`MODEL_FOR_ACCOUNT`); the shell
    turns tracing off before it expands the value, and shell start-up variables are not passed on;
    Observatory's own refusal line is passed on bounded; a duplicate account after a lost record is
    refused by the store's own same-key check; Hermes's side tasks are pinned to the metered
    provider with no fallback, and provider key variables are unset for that launch; the window
    reads Observatory once and again only on *Read again*, within 30 s of the backend's 20 s.
  - O-8 (review round 2, 2026-11-17): exported bash functions never reach the pipe shell, which
    writes with `command printf`; a refusal that may hold a key (`sk-` in any case, or 24 or more
    key characters in a row) is never shown; a record not written after a save is reported beside
    the save, not as a failure of it; the metered Hermes home has an empty `.env` of its own, and
    more provider and cloud variables are unset. Adding a key from Observatory as an account fails closed
    while a saved Claude API key's stored value is gone, naming it by its label, and asks to try
    again when a read failed only for now (`SAVED_KEYS_UNREADABLE_NOW`, review round 3): the retry the review
    feared is that path's. The store's own add keeps the 2026-11-08 rule (an unreadable row never
    blocks a new account, an incident on the Mac Studio with exactly such API keys), so a person's
    add is not broken again.
- **Consequences:** an agent (or the operator over MCP) adds a key Observatory holds without a
  person typing it; a Hermes launch on the pay-per-token group no longer needs `agents connect`
  and leaves `~/.hermes` alone, while one on an ordinary group starts as before. Open: a rotation
  in Observatory is not followed yet, and a key Observatory marks leaked is not refused yet (K4);
  OpenAI keys wait for OpenAI prices in the proxy.
- **Source:** the operator's words above (2026-11-17); branch `observatory-keys`.

### DEC-0025: A subscription is delegated to a teammate's machine as a guest, under a lease

- **Date:** 2026-11-17
- **Status:** Accepted (built on branch `org-delegation`: SB-403 waves S2 to S4; the live run with a
  teammate's machine, S5, waits for that machine)
- **Context:** the operator wants a teammate's computer to use one of an organization's
  subscriptions in console sessions through Switchboard, without the teammate ever signing in with
  it, revocable at any time, more added later; the teammate keeps their own subscription in Claude
  Desktop, which Switchboard must never touch. `accounts send` already moved a sign-in over ssh
  (SB-283), but over an ssh account that reached a shell, into an account the receiver could pass
  on, with nothing that ends it while the operator's machine cannot reach the teammate's.
- **Decision:**
  - The delegation rides `accounts send --delegate` over ssh into a group of the teammate's
    machine (D-1 of the [plan](plans/2026-11-17-org-seat-delegation/README.md)). The teammate's
    machine admits the operator's ssh key only with the forced command `'<program>' ssh-serve
    --sender-key KEY --pool GROUP` (the path quoted, `restrict`, replacing any older line for that
    key), which answers exactly the calls a delegation into that one group needs (save, prove,
    lists cut to the caller's seats, that group and its organizations, usage, lease, call back,
    removal), never a shell, and refuses a plain move. The receiver's seat operations are sent
    only by it: the local CLI, MCP and window never send them; a process holding the control token
    can, with the same trust as the person's own Keychain; against an uncooperative teammate the
    real revocation is signing the subscription out at the provider. `ssh-serve` carries no proof
    of its own beyond that token, since any secret it held would be readable by the same
    processes.
  - The saved account is a guest: it carries the sender's settings signing key and its group; it
    stays in that group, never `default`; it is never sent on, handed over, given to an agent,
    lent, shared, marked elsewhere or made the ordinary sign-in; only a call naming that key
    calls it back or removes it. The rules are checked on every publication of the store, so a
    path that forgets them fails closed. A backup never carries a guest.
  - A lease bounds it: 48 hours by default, 1 to 720. The sender's background read of the machine
    (DEC-0017, every 15 minutes) extends it. Past it, the receiver stops renewing at once and,
    once its own owner ran 15 minutes past both the lease and its own start, switches the seat
    off and cleans its homes, but keeps its credential, the only live copy (the receiver's
    renewals rotate the refresh token): the sender's next read revives it, and a call back takes
    it. Thirty days after the lease ended the credential is removed; a call back then asks the
    sign-in the sender kept, which is spent if the receiver had renewed it. Review round 1
    (2026-11-17) chose this over deleting at once, which a closed laptop over a weekend was enough
    to trigger and which lost the organization's sign-in; the sender does not pull tokens at each
    read (one live copy at a time, fewer tokens on the wire).
  - Threat model: the teammate owns the receiving machine. The rules keep an honest Switchboard
    from passing a guest on, using it outside its group or keeping it after a call back; they do
    not stop a person who reads their own Keychain. Against an uncooperative teammate the
    revocation is signing the subscription out at the provider.
  - A switch keeps Switchboard off the person's own sign-in (`own-sign-in off`): the ordinary
    Claude Code and Codex sign-in is never read or written (source sync, capture, import,
    activation, renewal, Claude Swap), a launch copies no MCP sign-in out of it, and the duty
    officer leaves the person's own sessions alone. Claude Desktop is outside Switchboard either
    way; no code names its bundle, data folder or Keychain item.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#delegated-seats-and-the-own-sign-in-switch-sb-403-s2-to-s4-dec-0025-2026-11-17),
  [ORGANIZATION.md](ORGANIZATION.md#a-teammates-computer) and its Russian twin, [CLI.md](CLI.md)
  (`accounts send --delegate`, `accounts lease`, `own-sign-in`, `ssh-serve`), the skill's
  [tools reference](../plugins/switchboard/skills/switching-accounts/references/tools.md),
  [ux/scenarios.md](ux/scenarios.md) (SCN-102, SCN-103), [brand/strings.md](brand/strings.md),
  `AGENTS.md` Lifecycle (the lease calls, `own-sign-in`), [CHANGELOG](../CHANGELOG.md),
  [evidence/backlog.md](evidence/backlog.md) (SB-403).
- **Source:** operator 2026-11-17, quoted in the [plan](plans/2026-11-17-org-seat-delegation/README.md):
  «он хочет себе подключить вторую подписку … у него добавится подписки организации, которые
  делегированы ему … и отзыв этой подписки … чтобы [a teammate] на своей машине мог ей
  пользоваться, но по факту авторизоваться он типа не будет с ней»; «[a teammate] может
  использовать Claude Desktop для своей обычной стандартной подписки … убедиться, что мы не
  трогаем сессии для десктопного Claude Code».

### DEC-0026: A window that does not own the data folder never updates the owner's copy

- **Date:** 2026-11-17
- **Status:** Accepted (SB-414, branch `update-notices`)
- **Context:** studio-agent's request bc06a63d: on the Mac Studio a private `~/Applications` 0.7.111
  window ran beside a 0.7.112 `switchboard serve` from `/Applications`; `serve` owned the data
  folder, the window owned nothing and said nothing about its version, and `switchboard app updates`
  answered a bare refusal. The window's own update loop ran all the same (`updates::start` does not
  look at ownership): it checked, downloaded and swapped its bundle, and it could never restart into
  it on its own (`owner_unavailable`). Where the window runs from the same `/Applications` copy as
  `serve`, both could swap one bundle at once: `serve` through `self_update::install_bundle`
  (DEC-0011) and the window through the updater plugin, with no lock between the two processes.
- **Decision:**
  - The owner says what it is and where it runs from (`Operation::Status`: `owner_kind` `app` or
    `serve`, `owner_copy` the `.app` bundle, the AppImage or the program's folder); a window that is
    not the owner reads it over the control channel and shows its own version beside the owner's,
    with what to do when it is older (reopen it, or open the owner's copy).
  - A window that does not own the data folder and runs from the owner's copy installs nothing into
    it when the owner keeps that copy current (the owner says so: `owner_updates_copy`, true for the
    app, and for a `serve` only when it is an installed copy that updates itself, so never on
    Windows, with `<data>/auto-update` on and an update pass that succeeded within 15 minutes,
    or no pass yet and its update loop started within 15 minutes; review rounds 2 and 3); the window's check ends
    at once (`update_check owner_updates`, once) and Settings offers no update button. When the
    owner does not keep it current, the window checks and installs as before (review round 1,
    P1-1). A pass where the store is held and the owner does not answer installs nothing
    (`owner_unanswered`), and the window says so in Settings and in the answer to *Check for
    updates*.
  - A window from another copy of its own keeps checking, downloading and installing into that copy,
    which only it writes. It does not restart into it on its own (there is no owner to drain);
    *Restart to update* stays the person's, and the window says to use the owner's copy instead.
  - `switchboard app updates` without the app (a `serve` owns the data folder, or nothing does)
    answers the owner, its version, its own update loop's last pass and the non-owner window's
    version (`window-run.json`, one record per window, matched to its process by the process's
    start) instead of a refusal; every action and every switch still needs the app (`APP_ONLY`).
  - A `serve` whose program was replaced (a window's install, another account's update, apt)
    exits with status 75 only when a supervisor runs it (a launchd job, or the main process of a
    systemd unit: `serve_update::supervised`); started by hand it logs `update_restart
    unsupervised` once and keeps running on the old program, since an exit would stop it for good
    (review round 3). Until its first update pass, a `serve` that started within 15 minutes still
    counts as keeping its copy, so a window does not install beside it in those first 90 s.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#a-window-that-does-not-own-the-data-folder-sb-414-2026-11-17),
  [CLI](CLI.md), `src-tauri/src/updates.rs`, `crates/switchboard-runtime/src/copies.rs`,
  [AGENTS.md lifecycle](../AGENTS.md#lifecycle) (`window-run.json`).
- **Source:** studio-agent board request bc06a63d (board thread dca6e14d), 2026-11-17; decided by
  the implementing agent under the operator's standing delegation (`~/.claude/CLAUDE.md`, "Act,
  then report"), recorded in this change.

### DEC-0028: A group fed from the default group keeps its ready accounts, by priority

- **Date:** 2026-11-17
- **Status:** Accepted (built on branch `default-feed`: SB-419)
- **Context:** balancing (DEC-0012) lends between chosen groups by their last hours and days, one
  account a pass, and only once a group is already out of room. The operator asked for a switch
  per group that keeps it always working: accounts that ran out go back to the default group,
  accounts with room, or soon to have it, are placed ahead of need, and when there are not enough,
  a group with a higher priority is served first.
- **Decision:** a group can be fed from the default group (off by default; `default` itself never).
  A fed group keeps `ready` ready accounts per provider it works with (2 by default; 1 to 10): an
  account is ready when it has room on fresh figures (the group's switching threshold less its
  hysteresis) or every window holding it resets within the look-ahead (30 minutes by default;
  0 to 720). An account is *out* only when a limit refusal holds it or one of its windows is at or
  past the switching threshold itself (90 % by default, DEC-0008), with no reset within the
  look-ahead; between the room line and the threshold it is still in use. A group works with a
  provider when an account of it serves the group or was fed to it, or its sessions asked for it in
  6 hours; a group with no account is fed only what its sessions asked for. The feed is a second
  policy on balancing's pass (every 15 minutes, a minute after a group ran out or after any change
  of the feed) and files. After balancing's own plan, on the accounts it left alone, it:
  (1) gives back a placement waiting for a sign-in (`sign_in`) and ends a park whose account has
  room again, after its cooldown (`rested`); (2) parks each own account of a fed group that is out:
  a park shares it with `default` (it keeps serving its own group, where automatic switching
  passes it by) so another fed group can take it once its limit resets (`park`, never delayed by
  the cooldown); (3) fills the fed groups from the reservoir, the default group's accounts lent
  nowhere and parked accounts of other groups, highest priority first (equal priorities by group
  id), first to one ready account each, then to `ready`, the account with room now first (the most
  room, then the latest reset), then the soonest reset (`feed`); (4) gives a fed group with demand
  (requests in 6 hours, or a limit refusal in about the last 15 minutes as this owner counted it:
  never the day counts a fresh start reads, so a refusal this morning is not one now) and still no
  ready account one ready account of the lowest fed group of a lower priority, even its last (an
  account the feed placed there moves, `taken` then `priority`; its own account is shared with it),
  but not an account that served that group in about the last 30 minutes unless the higher group is
  refusing requests now; (5) after those takes, gives back a placement that is out, never delayed,
  but only while another account with room now serves its group, so a fed group never loses its
  last account and its sessions always have one to go to (`spent`); a return that relies on an
  account lent in the same pass is held when that lend did not happen; (6) gives back the extra
  placements of a group holding at least `ready` + 2 accounts with room now, down to `ready`,
  never the one its sessions use now (`surplus`). The two short windows are read in the statistics'
  quarter-hour buckets, so they are about 30 and 15 minutes; "serving" reads the day counts after a
  restart (erring toward protecting an account), and a lost `statistics.json` leaves it empty until
  this owner has counted for about 30 minutes, when a priority move may take an account in use.
  Every move is a loan like balancing's (a
  share; nobody's home changes), takes effect from the next request, waits out the account's
  one-hour cooldown except a park, a spent placement and one waiting for a sign-in, and is one
  `balancing.jsonl` line with its reason and figures. Never moved: the ordinary CLI's sign-in, an
  account given to an agent, one turned off (today `enabled`; the person's own off switch of SB-418
  replaces it), a delegated seat, one used on another machine or in a machine group; nothing
  crosses an organization's boundary (`loan_admits`). Turning a group's feed off gives back at once
  every account the feed moved in or out of it, and takes no priority. A move is recorded in the
  store's activity as a `balancing` entry (`lent`, `returned`), so an older Switchboard still reads
  the store; the window's Activity names the feed's moves from the journal.
- **Consequences / affects:** `balancing.json` gains `fed`, `ready`, `lookahead_minutes` and the
  loans' `feed` mark. An older Switchboard reads the file but does not keep the feed: it drops
  `fed`, `ready` and `lookahead_minutes` the next time it writes, and its balancing pass returns
  every feed loan as `not_chosen` (a loan between groups it does not see chosen; one between two
  groups chosen for balancing it takes for its own and returns once the borrower is quiet), so after
  a downgrade the feed is off and what it moved goes back. CLI `project feed`, `feed status`,
  `feed set`; MCP `switchboard_feed`; the Groups page's switch, priority field and map, Activity's
  *Fed from Default* (SCN-104); [CONTRACTS](CONTRACTS.md#groups-fed-from-the-default-group-sb-419-dec-0028).
- **Source:** operator 2026-11-17 («хочется, чтобы появилась автоматизация ротации из дефолтной
  группы в папке. Она включаемая и выключаемая для каждой папки … показываем стрелочками, что это
  связано … Те, которые выбывают, они уходят в дефолтную папку отстаиваться … аккаунты, которые
  скоро станут активными, наперед мы подготавливаем … У папок типа есть свой приоритет»); design
  target-state `06-default-feed.md`.

### DEC-0031: A short rate limit of the pay-per-token key is waited out inside the proxy

- **Date:** 2026-11-18
- **Status:** Accepted (built on branch `tokens-spend`: SB-453). The number is the next one free
  after the decisions on open pull requests (DEC-0027 #821, DEC-0029 #832, DEC-0030 #836); a
  pull request that lands first with the same number moves this one to the next free.
- **Context:** the pay-per-token group usually holds one Claude API key, so DEC-0002's move to the
  group's next account finds none, and a `429` for the key's rate limit reached the session as an
  error. On 2026-11-18 (board thread a9844103) a session and its nine subagents met `429` within
  two minutes and stood still until they were closed for inactivity. An API rate limit refills
  within the minute and the provider says how long to wait (`Retry-After`).
- **Decision:** a `429` for a Claude API key at home in the pay-per-token group, when no DEC-0002
  move takes the request, is waited out inside the proxy and the same request sent again on the
  same key, when the provider asked for at most 30 seconds and did not say `x-should-retry:
  false`: at most three tries and 60 seconds of waiting per request, the group's switch and
  ceiling checked again before each try. Nothing reached the session before, so it sees only the
  final answer. This is a second deliberate replay beside DEC-0002, limited to a request that did
  no work and charged nothing; no other replay is added.
- **Consequences / affects:** a waiting request holds one of the proxy's 64 slots; a session that
  goes away ends its wait. Each refused try counts as `limited` in the statistics and the journal
  records `rate_limited` once per request. `AGENTS.md` (Local rules),
  [CONTRACTS](CONTRACTS.md#pay-per-token-group-sb-401-dec-0023).
- **Source:** board thread a9844103 (Fabric Dashboards, 2026-11-18), relayed by the coordinator
  of SB-452/SB-453: "the proxy should handle an upstream 429 with retry-after: a bounded wait and
  retry inside the proxy before any byte reached the client (a request that did no work, like
  DEC-0002's rule; never retry after bytes were sent)".

### DEC-0033: Each machine is read every minute through its own Switchboard, its sessions and health with its accounts

- **Date:** 2026-11-19
- **Status:** Accepted (built on branch `sb-550-remote-minute`: SB-550). Amends DEC-0017's cadence
  (15 minutes, then hourly after three failures) and what is read; DEC-0013's routing rule and
  DEC-0025's lease cadence stand. The number is the next one free after the decisions on open pull
  requests (DEC-0027 #821, DEC-0029 #832, DEC-0030 #836, DEC-0032 #856); a pull request that lands
  first with the same number moves this one to the next free.
- **Context:** the Accounts row of an account on another machine showed this computer's own old
  check as its bar and share ("17%, 8d ago") while the line under it gave the background read of
  its copy there ("Used 0% on the machine, 14 min ago"): two numbers for one account. The window
  read that file once and did not read it again, so a fresh read could still say "Not read yet",
  and the line under the bar was drawn over it. The operator asked to tell when something does
  not work, and to ping the sessions on other machines every minute through Switchboard, never by
  asking the sessions themselves, on a teammate's Switchboard inside the organization too.
- **Decision:**
  - The owner reads each machine group's Switchboard over ssh every minute while it runs, one
    machine after another on a thread of its own, each call given up at 20 seconds, never two
    passes at once. Three failures in a row read that machine every five minutes.
  - One call, `switchboard --json remote-status`, answers that machine's accounts (label,
    provider, on or off, used share with when its own check observed it, its check's status), its
    sessions (the Sessions page's rows cut to ids and codes: id, group, why, agent, launch and
    session ids, since when; never a folder, a label or text a session wrote; the ended rows only
    counted) and its own `agent-health/1` report. A Switchboard there that does not know the call
    is read with the older calls in the same read (`accounts list`, then `sessions --rows`; only
    `accounts list` through an older restricted entry, which refuses the other), and until it is
    updated it is read at the old cadence, every 15 minutes, and asked the one call again an hour
    later. The real cost per machine is one ssh call a minute; an older Switchboard costs two calls
    (one through a restricted entry) every 15 minutes, and a machine that stopped answering one
    call of at most 20 seconds every five minutes.
  - A teammate's restricted entry (`ssh-serve`, DEC-0025) answers the same call cut to the seats
    the calling key delegated and the sessions of the group its key line names, with no health:
    that computer's other data stays its own.
  - `remote-usage.json` keeps per machine the accounts' shares, the session rows and counts, the
    health checks' states and which call answered: ids, codes, counts and times only (0600).
  - The row of an account on another machine shows its copy there: bar, share and age from the
    last read; this computer's own older check is shown only while there is no read there, and
    says so. The window reads the file again each minute while Accounts or Groups is on screen,
    and when the owner writes it (event `machines-changed`, one `stat` per 15-second look).
  - Needs attention names a machine that did not answer three reads in a row
    (`machine_unreachable`), and from a read that worked its failing health checks or its owner
    not running (`machine_health`) and its sessions that need a person: a second copy or a board
    run gone quiet (`machine_sessions`; a gone run is history there). The last good items are
    kept through one or two failed reads, so they do not come and go; `since` is when a read that
    worked first saw the condition. `agent-health/1` counts them in the check `machines`.
    Switchboard itself sends no message about them.
  - The leases of seats delegated to a machine are extended at most every 15 minutes, as before;
    a failed extension is tried again five minutes later, not at every read.
- **Consequences / affects:** [CONTRACTS](CONTRACTS.md#background-reads-of-other-machines-sb-366-dec-0017),
  `AGENTS.md` Lifecycle (the child `ssh` and `remote-usage.json` every minute, the window's
  `machines-changed`, the idle budget), [CLI.md](CLI.md) (`remote-status`, `ssh-serve`,
  `attention`, `health`), the MCP descriptions of `switchboard_attention`, `switchboard_health` and
  `switchboard_machines`, the skill's [tools reference](../plugins/switchboard/skills/switching-accounts/references/tools.md),
  [ux/scenarios.md](ux/scenarios.md) (SCN-092 step 8), [CHANGELOG](../CHANGELOG.md),
  [evidence/backlog.md](evidence/backlog.md) (SB-550).
- **Source:** operator 2026-11-19, a screenshot of Accounts with the machine group of a remote
  machine and two accounts: «вот такие как мне отслеживать и числа с полоской отличаются и в
  целом надо понимать если что-то не работает и вести пинг по сессиям которые удаленны раз в
  минуту через свитчборд а не через запросы напрямую к сессиям и на удаленный свитчборд в рамках
  организации то же самое».

