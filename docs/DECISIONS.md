# Decisions: Fabric Switchboard

**Append-only.** Every settled thing that shapes the product, the architecture, security, data or
process and still holds. To change one, add a new entry and edit only the old entry's status line;
never renumber, never delete. The doc map ([DOCMAP.md](DOCMAP.md)) names this file as the decision
register. Decisions made before 2026-11-08 live where they were made (plans' `D-<n>`, board rows,
research notes); the ones still in force that the board cites are seeded below with their source.

**Next free ID:** `DEC-0012`

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

