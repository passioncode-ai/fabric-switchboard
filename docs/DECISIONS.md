# Decisions: Fabric Switchboard

**Append-only.** Every settled thing that shapes the product, the architecture, security, data or
process and still holds. To change one, add a new entry and edit only the old entry's status line;
never renumber, never delete. The doc map ([DOCMAP.md](DOCMAP.md)) names this file as the decision
register. Decisions made before 2026-11-08 live where they were made (plans' `D-<n>`, board rows,
research notes); the ones still in force that the board cites are seeded below with their source.

**Next free ID:** `DEC-0008`

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
