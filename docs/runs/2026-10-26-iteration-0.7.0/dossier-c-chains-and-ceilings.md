# Dossier C: fallback chains screen and paid spending ceilings

One dossier per cluster of inventory items (`references/iteration.md` → *Module dossiers*),
written in parallel with the others, read-only on every repository: the dossier is the only
write. **A dossier is a design, not a plan**: it does not cut waves. Evidence for every claim:
`file:line @commit`, or a command and its output. No secret values, ever.

Written 2026-10-26 against:

- passioncode-ai/fabric-switchboard @ `a9ff8fa` (worktree `iteration/0.7.0`; every bare path below is this repository at this commit)
- passioncode-ai/project-observatory-dashboard @ `04514443` (local clone `~/DATA/project-observatory-open-source`; the OpenRouter door with daily reset)
- ssheleg/project-observatory @ `56894b01` (the operator's private engine clone; read only to compare its door)
- passioncode-ai/fabric-workspace @ `cd58ca1d` (`knowledge/authority.md`)
- passioncode-ai/fabric-agent-contract @ `898d816f` (`docs/DECISIONS.md` DEC-0027)

Operator decision driving this module: **D-5 of 2026-10-25, "Both in 0.7.0: the chains screen
(SB-71) and paid ceilings (SB-72)"** (`docs/reports/2026-10-25-audit-and-plan/README.md:250`).

## Contents

- 1. Items
- 2. As-is
- 3. To-be
- 4. Options considered
- 5. Contracts touched
- 6. Nodes
- 7. Deletes
- 8. Questions
- 9. Out of scope / program

## 1. Items

| id | title (short) | still holds? (yes / no + evidence / changed) | severity |
|---|---|---|---|
| SB-71 | Fallback chains the operator sets: the window's screen | **yes, changed scope.** Core, CLI and MCP shipped in v0.6.8 (`docs/evidence/backlog.md:78`); what is left is only the screen. Verified: the Tauri handler list (`src-tauri/src/main.rs:1097`) has no chain command (`grep -n -i chain src-tauri/src/main.rs` prints only an unrelated iterator at :1248), and the window has no chain UI: `grep -n -i chain src/main.ts` finds only two sentences that mention chains (`src/main.ts:745`, `src/main.ts:762`) and no control. The adapter already carries the chain refusals (`src/adapter.ts:172-186`), so the error vocabulary exists ahead of the screen | P1 |
| SB-72 | Spending ceilings for paid fallback agents (machine daily, per group, per task at the switch), enforced at the provider, spend read before each switch | **yes, nothing built.** No ceiling type exists in core (`crates/switchboard-core/src/chains.rs:24-43`: `Executor {agent, account_id?, key?}`, `Chain {scope, executors, updated_at}`, no money field), and the hand-over never reads spend: `pass_with` goes from `resolve_with` straight to `hand_over` (`crates/switchboard-runtime/src/fallback.rs:725-734`) and `hand_over` calls `act` for every `Step::Run` in order (`fallback.rs:753-781`). The only spend read in the product is the API keys page and MCP status (`crates/switchboard-runtime/src/agent_keys.rs:31-69`, `crates/switchboard-cli/src/mcp.rs:856-876`). **Changed fact:** the backlog row says the door's daily reset landed (`docs/evidence/backlog.md:79`); true for the public door (`observatory/engine/tools/openrouter.py:656-665,2089-2090,2118-2119 @04514443`, `RESETS = ("daily", "weekly", "monthly")`), but the operator's private engine clone hard-codes a monthly reset (`tools/openrouter.py:300 @56894b01`, `"limit_reset": "monthly"`, no `--reset` flag in its argparse at :634-645). Which door issues the operator's keys decides whether a daily provider ceiling is reachable today (Q5) | P1 |
| SB-73 (part) | "status in the app and MCP" of the automatic fallback | **yes.** The outcome code is logged (`fallback.rs:364-366`) but the store keeps only `offered` / `failed` (`fallback.rs:735-743`, allowlist `crates/switchboard-core/src/lib.rs:430`), and nothing in the window or MCP says why the last hand-over did not happen. Phase 2 remainder (keyed pins, live acceptance) stays in the program (section 9) | P1 |
| SB-96 | A chain pins only keys the project's policy names (grant check) | **yes, blocked.** Needs the project grant in Observatory's registry; the meter exists (OBS-55 done in 0.23.0, `docs/backlog.md:84 @04514443`) but grants are projections Fabric's hub writes, and without one the meter answers `unknown-state` = deny (`docs/design/METER.md:61-74 @04514443`). Keyed pins stay skipped (`fallback.rs:202-209`, reason `agent_not_automatic_yet`). Section 9 | P1 |
| SB-97 | Per-grant metering through the proxy | **yes, blocked** on the same registry (`docs/evidence/backlog.md:104`); the `/openrouter/<pool>` proxy route is unbuilt (`docs/evidence/backlog.md:153`). Section 9 | P2 |

Finding outside the rows, owned here because the screen exposes it (node C-8): a group's chain
applies only when the limited executor's **account** is in that group. `pass_with` derives the
pool from the executor account (`fallback.rs:698-704`) and picks the chain (`fallback.rs:705`)
before it reads the checkout (`fallback.rs:710-724`). The ordinary CLI's account is in the default group (an ordinary sign-in
cannot join a group: MCP server instructions, `switchboard_account_move` description), so a
workflow worked by plain `claude` in a group's folder gets the machine chain, not the group's.
With SB-183 (the shell hook, in 0.7.0) more work runs that way.

## 2. As-is

**Chains (SB-71 core, v0.6.8).**

- Data: `Snapshot.chains: Vec<Chain>` (`crates/switchboard-core/src/lib.rs:270`); scopes
  `Machine`, `Project {pool}`, `Workflow {id}` (`chains.rs:13-22`); an executor is an agent id
  plus at most one of `account_id` or `key` (a vault name, never a value) (`chains.rs:24-35`).
  At most 8 executors, 256 chains (`chains.rs:10-11`).
- Validation: agent id shape, account or key not both, keys never for Claude Code or Codex,
  a provider's client only on its provider's account, any other agent never on a subscription
  sign-in, Kimi accounts only for Kimi Code (`chains.rs:84-133`); on load (`chains.rs:135-165`).
- Writes: `Store::set_chain` refuses duplicates and logs `fallback_chain set|cleared`
  (`chains.rs:194-244`); `chain_for` picks workflow, else project, else machine, else none
  (`chains.rs:248-259`); removals prune (`chains.rs:169-189`).
- Runtime: `Operation::Chains {workflow?, pool?}` returns `{chains, effective, presets}` and
  `SetChain {scope, executors, preset?}` checks the catalog (`can_continue`) and resolves the one
  preset (`crates/switchboard-runtime/src/lib.rs:449-464,3033-3075`). The preset is
  `subscriptions-first = claude-code, codex, kimi-code, hermes`
  (`crates/switchboard-runtime/src/agent_catalog.rs:53-56`). Note the gap between what a chain
  may name (`can_continue`: MCP and a headless form, `agent_catalog.rs:38-40`) and what the
  fallback can run (`SWITCHABLE = claude-code, codex, kimi-code, hermes, opencode`,
  `agent_catalog.rs:45`): any other named agent is skipped `agent_not_automatic_yet`
  (`fallback.rs:225-229`).
- CLI: `switchboard chain list|set|clear` with `agent`, `agent@ACCOUNT` or `agent#project/env/NAME`
  (`crates/switchboard-cli/src/main.rs:692-747,1342-1355`). MCP: `switchboard_chain_get` (read)
  and `switchboard_chain_set` (write) (`crates/switchboard-cli/src/mcp.rs:84-85,845-855,920-948`).
- Window: nothing. The Agents page renders Default agent, Installed, Available, an OpenRouter
  pointer and Connect (`src/main.ts:736-741`); two sentences mention chains the person cannot
  see or set (`src/main.ts:745,762`). The Activity journal already names the events
  ("Fallback chain", "Hand-over": `src/ui-logic.ts:98,102`; Russian «Цепочка агентов»,
  «Передача», `src/locales/ru.ts:819,821`).

**Automatic fallback (SB-73 phase 1 and phase 2 parts).**

- Runs only while a chain exists and some account is limited (`fallback.rs:595-602`), at most
  one scan a minute and four workflows a pass (`fallback.rs:22-33`).
- Resolution is pure and synchronous (`resolve_with`, `fallback.rs:112-261`): disabled agents
  first (`:142-148`), Kimi Code on saved Kimi accounts (`:150-180`), OpenCode and Hermes on the
  saved OpenRouter key, both skipped when the step pins an account or a key (`:181-224`),
  Claude Code and Codex on the least-used usable account (`:231-259`).
- Paid steps today: Hermes and OpenCode run on the OpenRouter key of the **checkout folder**
  (`store.agent_key_in("openrouter", Some(&working_directory))`,
  `crates/switchboard-runtime/src/launch.rs:1745-1747,1913-1915`), that is the group's own key
  (SB-144) or the machine key. Their only bound is the key's own OpenRouter ceiling
  (`docs/CONTRACTS.md:470-472`: "Spend is bounded by the key's own OpenRouter ceiling").
  Readiness reads only that a key exists, never its balance (`launch.rs:1662-1672`).
- Hand-over walks `Step::Run` in order; a refusal before the offer moves on, a failure after
  the offer stops (`fallback.rs:753-781`). Outcome codes go to the log (`fallback.rs:364-366`);
  the store event keeps `offered` or `failed` (`fallback.rs:735-743`).
- Nothing notifies the person of a hand-over in the app: `grep -rn -i "fallback\|hand-over"
  src-tauri/src/*.rs` prints nothing.

**Spend reading.**

- `agent_keys::credit` asks OpenRouter `GET /key` and returns `limit`, `limit_remaining`,
  `limit_reset`, `usage`, `usage_daily` (UTC day), never the key (`agent_keys.rs:30-69`); a
  10 s timeout (`agent_keys.rs:13`); tests point it at a loopback stand-in through
  `SWITCHBOARD_OPENROUTER_BASE`, loopback only (`agent_keys.rs:10-27,157-205`).
- The window shows one line per key (`openrouterCreditLine`, `src/ui-logic.ts:447-455`) and a
  per-key spend table on Statistics (`src/main.ts:1765-1795`). MCP `switchboard_openrouter_status`
  reads the same, plus every group's key (`mcp.rs:856-876`).

**Ceilings, authority and the provider door (outside this repository).**

- fabric-workspace `knowledge/authority.md:35-62 @cd58ca1d`: L0 is each upstream key's ceiling
  plus the estate total of USD 1,000 a month; per-project limits are not computed by anyone yet
  ("until both ship, the binding limits are the estate total below (L0) and each provider key's
  own ceiling", `:51-53`). A task ceiling is a question, not a wall: «ceiling X exhausted, double
  to 2X?» with no recommended option, only the operator's own session answers *double*, anyone
  answers *keep*, a yes is tied to the value so the next hit asks again (`:56-62`). The registry
  row for Switchboard says "Project pools carry daily USD ceilings (RM-20, SB-72); a fallback
  chain pins only the keys the project policy names" (`:347`).
- The public door issues keys with a ceiling and a reset period and moves them
  (`issue --limit N --reset daily`, `limit NAME --set N --reset daily`,
  `observatory/engine/tools/openrouter.py:7,10,2089-2090,2118-2119 @04514443`), and can split a
  project's spend policy across its keys (`issue-for-policy --policy`, `:2091-2106`). It is a
  local operator tool holding the provisioning key; Switchboard never holds that key.
- The grant registry (OBS-55, the meter) answers `grant check` only for grants Fabric's hub has
  projected; with none it denies (`docs/design/METER.md:57-74 @04514443`). Project spend policies
  wait on Fabric CO-280 (`authority.md:51`). **No date** is named for project grants that would
  carry Switchboard's group ceilings; the audit leaves SB-72's date to "Observatory's grant
  registry" (`docs/reports/2026-10-25-audit-and-plan/README.md:177,250`).
- Contract DEC-0027 (`docs/DECISIONS.md:608-650 @898d816f`) defines `budgets[]` in a
  `fabric-service/0.1` usage report. Switchboard publishes no usage report today
  (`grep -rn "fabric/v1/usage\|budgets" crates src-tauri/src docs/CONTRACTS.md` prints nothing),
  so DEC-0027 is not touched now; its mapping is recorded in section 5 for when it is.

**Storage constraint that shapes the design.** `Snapshot` is `deny_unknown_fields`
(`crates/switchboard-core/src/lib.rs:255-257`) and every stored event is checked against a fixed
allowlist on load (`lib.rs:403-433,522-528`). A new Snapshot field or a new event detail makes
`accounts.json` unreadable to a 0.6.x build after a downgrade. Settings that need no such
coupling live in their own files and ride in backups through `SETTING_FILES`
(`crates/switchboard-core/src/backup.rs:70-71`: `login-item`, `auto-update`,
`agents-settings.json`).

**Scenarios.** SCN-039 (chains, entry point CLI and MCP only, "An app screen comes later",
`docs/ux/scenarios.md:815-834`), SCN-040 (automatic fallback; alt path "a step pinned to a paid
key ... skipped until SB-72", `:836-853`), SCN-065 (group keys, `:1357-1374`). The last id in the
index is SCN-073 (`docs/ux/scenarios.md:83`). `docs/ux/lint.py` does not exist (project
CLAUDE.md "Remaining setup").

## 3. To-be

One design in five parts. Parts A and B answer SB-71 and the SB-73 status remainder; parts C and
D answer SB-72 within what 0.7.0 can ship without the grant registry; part E is the seam finding.

### A. Chain readiness and status, one read (SB-71, SB-73 status)

- New pure function `chain_view::preview(snapshot, chain, folder?, kimi, spend?) -> Vec<StepView>`
  in a new file `crates/switchboard-runtime/src/chain_view.rs`, built on `resolve_with`
  (unchanged rules) with no executor and no workflow: what each step would do **now**.
  `StepView {agent, name, state: "ready" | "skipped" | "not_automatic", reason?: code,
  account_label?, key_scope?: "group" | "machine"}`. For a project scope the folder is the
  group's first folder, so `project_allows` (SB-102) is honoured.
- `Operation::Chains` (`lib.rs:3033`) gains, additively, `readiness: {<scope key>: [StepView]}`
  for every chain set and `last_handover: {at, workflow_short, outcome, skipped: [{agent, reason}]}`
  read from the new `spend.json` record of part C (never from Snapshot events, see section 4).
  A fresh read never starts the engine or a network call: the readiness for paid steps shows
  the last spend read with its age, and says "not read yet" otherwise.
- MCP `switchboard_chain_get` returns the same additive fields; the CLI `chain list` prints one
  readiness line under each chain.
- Tauri commands `chains`, `set_chain` (`{scope, executors?, preset?}`) and nothing else: the
  renderer never sees a credential, only labels and codes. Adapter, types and demo data follow.

### B. The screen (SB-71)

On **Agents**, a new panel **Fallback chains** between *Installed* and *Available to install*
(the page where agents are enabled and the default is chosen; XA-01 named "a Fallback section"
and the Agents page is where 0.6.82 put every agent setting):

- Lead line: when a task's accounts run out, Switchboard hands it to the next agent in its chain;
  nothing is handed over until a chain is set.
- Row **This computer**: the chain as chips in order (`Claude Code → Codex → Kimi Code → Hermes`)
  or "Not set"; actions *Use Subscriptions first* (one click, the preset), *Edit…*, *Clear*.
- Rows **per group** (every group, from `snapshot.projects`): "Uses this computer's chain" or its
  own chain; *Set for this group…*, *Edit…*, *Clear*.
- Rows **per task** (workflow chains, normally set by an agent or the CLI at the switch): the
  short workflow id, its chain, *Edit…*, *Clear*. No "add task" form in 0.7.0 (an id is typed by
  agents, not people; CLI and MCP keep it).
- Under each row, readiness from part A, one short line per step: "Ready: Work Max",
  "Skipped: no account with room", "Skipped: disabled under Installed", "Not automatic yet",
  "Skipped: today's ceiling reached" (part D). An agent named in a chain but not installed reads
  "Not installed".
- Under the panel: "Last hand-over: {time}, {outcome}" with the skipped steps' reasons.
- **Editor dialog** (one for every scope, titled by scope): an ordered list of up to 8 steps.
  Each step: agent (a select of the five switchable agents; an existing step naming another
  agent stays, marked "Not automatic yet"), account ("Any account with room" by default, or a
  saved account the core rules accept for that agent, filtered in the window by a pure
  function that mirrors `validate_executor`), *Move up*, *Move down*, *Remove* (buttons with
  accessible names; no drag-only control). *+ Add agent* until 8. A step pinned to a key
  (`agent#project/env/NAME`, set by CLI or MCP) is shown read-only, "Pinned to key {name}:
  not automatic yet (waits for project grants)", and kept unless removed: the window never
  creates a key pin in 0.7.0 (SB-96). *Save* sends the whole list (`SetChain`); every refusal
  shows the core's own text, already in the error vocabulary (`src/adapter.ts:172-186`).
- The Groups page gets one line per group card, "Fallback: this computer's chain / own chain",
  linking to the panel (no second editor).
- Failure handling: a failed read shows "Chains are unavailable. Refresh to retry." with the last
  known list marked as possibly stale, like Sessions (`terminology` line 139 pattern).

### C. Spending ceilings: model and storage (SB-72)

- A new private file `<data>/spend.json` (0600), owned by a new module
  `crates/switchboard-runtime/src/spend.rs`, carried by backups (`SETTING_FILES` gains
  `spend.json`). Not in `Snapshot`, so a 0.6.x build still reads `accounts.json` after a
  downgrade (section 2, storage constraint).
- Content:
  - `ceilings: [{scope: ChainScope, usd: decimal, set_at, doubled_from?: decimal}]`, at most
    256, one per scope. Meaning by scope: **machine** = USD per UTC day across every saved
    OpenRouter key; **project** (group) = USD per UTC day on that group's own key (refused for a
    group without its own key: "Give this group its own key under API keys first", since a group
    on the machine key shares one balance with every other folder); **workflow** = USD on the
    key since the task's first paid hand-over (part D explains the measure). Values are cents,
    `0.01 ≤ usd ≤ 1000` (the estate total, `authority.md:54`).
  - `questions: [{id, scope, value, reached_at, spent, answer?: "keep" | "double", answered_at?,
    answered_by?: "window" | "cli" | "mcp"}]`: one open question per (scope, value), at most 64,
    answered ones kept 30 days.
  - `baselines: [{workflow, key_slot: "machine" | <pool>, usage_at_first_handover, at}]` for task
    ceilings, at most 256, dropped with the workflow's chain or after 30 days.
  - `last_handover` (part A).
- Pruning as `chains::prune`: a removed group drops its ceiling, questions and baselines; a
  removed group key turns its group ceiling into an open question-free "inactive" state shown in
  the window (never silently deleted: the operator set a money value).
- Answers (the operator rule, `authority.md:56-62`): **keep** closes the question and leaves the
  value; **double** sets the ceiling to 2 × value, records `doubled_from`, closes the question,
  clears the workflow's 15-minute retry mark so the next pass may try again. A new hit at the
  new value opens a new question. *Double* is accepted only from the window (the operator's own
  session on this machine); CLI and MCP may answer *keep* and read everything (Q1).
- Events: `ceiling set|cleared|reached|kept|doubled` go to the log (`oplog`, codes and numbers,
  never a key or label) and to `spend.json`; no new Snapshot event (section 4).

### D. The spend gate at the switch (SB-72 enforcement, SB-73)

- `hand_over` (`fallback.rs:753`) gains a gate called before `act` for each paid `Step::Run`.
  Paid = Hermes and OpenCode (OpenRouter key), and Claude Code or Codex resolved to an
  **API-key** account when a ceiling covers the step (Q3). Subscriptions and Kimi Code are not
  paid and never gated.
- The gate reads spend lazily, only for the paid step about to run: `agent_keys::credit` for the
  key `agent_key_in("openrouter", Some(dir))` selects (the same key the launch will use,
  `launch.rs:1746`), plus, when a machine ceiling is set, every other saved OpenRouter key
  (concurrently, each with the existing 10 s timeout). Nothing is read when no paid step is
  reached, so the idle budget (AGENTS.md *Idle budget*) is unchanged.
- Decisions, in order, each a fixed skip reason; the chain then moves on:
  1. `key_exhausted`: the key's own ceiling (L0) has less than `MIN_REMAINING_USD = 0.50` left
     (`limit_remaining` known and below it). Applies with or without Switchboard ceilings: a
     session started on an empty key dies at the provider's 402 after the offer, which stops the
     chain (`fallback.rs:772-773`) and wastes the hand-over.
  2. `ceiling_reached`: the strictest of the machine, group and task ceilings that cover the step
     is reached (machine: sum of `usage_daily`; group: the group key's `usage_daily`; task:
     the key's `usage` minus the task's baseline). Opens the question of part C.
  3. `spend_unknown`: a ceiling covers the step and a needed read failed (OpenRouter refused or
     did not answer, or an API-key account whose spend cannot be read). Unknown is a denial,
     as the meter treats an unknown grant (`METER.md:67`).
  With no Switchboard ceiling covering the step and the key's balance unreadable, the step runs
  (the provider still enforces L0) and the log says `spend_unread`; today's behaviour for every
  user without ceilings is unchanged apart from rule 1.
- The first paid hand-over of a workflow records its task baseline (`usage` of the key) in
  `spend.json` after a successful offer.
- Every pass records `last_handover` with the outcome and the skipped steps' reasons; outcome
  codes extend the log's set with `ceiling_reached`, `key_exhausted`, `spend_unknown`.
- Notification (D-1 "the person is notified"): the app's existing 15-second counter poll for
  project folders (AGENTS.md *Project folder notifications*) also reads a `spend.json` change
  counter and posts one system notification per hand-over offered and per question opened
  ("Daily ceiling $5 for Alpha reached. Double it or keep it in Switchboard."), nothing asked
  in the notification itself.
- What Switchboard does **not** do in 0.7.0: change a key's ceiling at the provider, call the
  Observatory door, or meter requests (section 4, Q4). The provider-side ceiling stays the one
  the key was issued with; the window shows it beside Switchboard's own, and warns when the
  key's own ceiling is not daily or is above the group's daily ceiling ("A running agent can
  spend up to the key's own ceiling; Switchboard's ceiling decides only whether it is started").

### E. A group's chain follows the checkout (finding, section 1)

`pass_with` reads the workflow's checkout first and takes the chain's pool from it: the
executor account's pool when that group holds the checkout; else the one group holding the
checkout folder; else, with several groups holding it (SB-157), the executor account's pool and
the log says `group_ambiguous`. A workflow in a group's folder on the ordinary CLI then gets the
group's chain, as the screen will say it does.

### What the items get

| Item | Answered by |
|---|---|
| SB-71 | A, B (and the scenarios of node C-1) |
| SB-72 | C, D: the key's own ceiling read at every paid switch, Switchboard's machine and group daily ceilings and a task ceiling checked at the switch, the double-or-keep question; the provider-enforced part is the key's ceiling (L0). Grant-backed project ceilings stay SB-96 |
| SB-73 status | A (`last_handover`, readiness), D (notification) |
| finding | E |

## 4. Options considered

| Choice | Options | Chosen | Why |
|---|---|---|---|
| Where the chains screen lives | (a) a section in Automatic switching on Accounts, as XA-01 sketched (`docs/packets/cross-agent-continuation.md:84`) (b) a panel on Agents (c) its own page | (b) | Automatic switching on Accounts moves accounts within one provider; chains order agents, and Agents already holds Enabled/Disabled and the default agent, which the chain resolution reads (`fallback.rs:142-148`). A new nav item would be the ninth (`src/main.ts:315`) |
| Where ceilings and questions are stored | (a) `Snapshot` (like chains) (b) a separate `spend.json` in backups | (b) | `Snapshot` is `deny_unknown_fields` and events are allowlisted on load (`lib.rs:255-257,522-528`): (a) makes a downgrade to 0.6.83 unreadable once a ceiling is set; (b) follows `agents-settings.json` (`backup.rs:71`) |
| What enforces the ceiling | (a) Switchboard issues a key per scope through the Observatory door (XA-01 design, `cross-agent-continuation.md:92-97`) (b) the key's own ceiling at the provider plus Switchboard's check at the switch (c) metering through the proxy (SB-97) | (b) now, (a)/(c) program | (a) needs the provisioning key and a door on every machine; the door lives only on the operator's estate and an organization's Macs have none; a product calling the operator's door is outside its authority (`authority.md:16-19`). (c) needs the `/openrouter/<pool>` route, unbuilt. (b) needs nothing new outside this repository |
| Machine daily measure | (a) sum of `usage_daily` over Switchboard's saved OpenRouter keys (b) only spend Switchboard started | (a) | OpenRouter reports per key and per UTC day (`agent_keys.rs:62-66`); Switchboard cannot attribute a key's spend to its own launches without metering. (a) over-counts (manual launches on the same key count), which errs toward stopping, never toward spending |
| Task measure | (a) key `usage` delta since the first paid hand-over (b) defer task ceilings to SB-97 (c) a per-task key from the door | (a), labelled | D-2 asks for task ceilings at the switch (`cross-agent-continuation.md:26`). (a) over-counts when two agents share the key (safe direction) and is checked only at a switch, which the label says. (c) is the program's (Q2) |
| Group ceiling on the machine key | (a) allow, measured on the shared key (b) refuse until the group has its own key | (b) | A shared key's daily spend is every folder's; a group ceiling on it would stop one group for another's spend, and authority rule 2 ties a project's money to a key issued for that project (`authority.md:16-19`) |
| Minimum left on a key before a hand-over | (a) `> 0` (b) a floor of USD 0.50 | (b) | A hand-over's first turn reads the checkpoint and constraints; a key with cents left fails after the offer, and a failure after the offer stops the chain (`fallback.rs:772-773`). The Hermes rehearsal spent $0.07 on probes and one full hand-over (`docs/CONTRACTS.md:479-480`), so 0.50 leaves room for several turns. A constant, documented, changed by a release |
| Who may answer *double* | (a) window only (b) window and an interactive CLI (c) any surface | (a), Q1 | The rule says only the operator's own session (`authority.md:59`). The window is a person; an MCP call is an agent, and the CLI is run by agents too. Dashboard parity (operator rule 2026-10-24) lets an agent act on the operator's chat words only through an operator credential, which Switchboard does not have |
| Status of the last hand-over | (a) new Snapshot event details (b) `spend.json` record plus the log | (b) | Same downgrade reason as storage; Activity keeps `Hand-over offered/failed` unchanged |

## 5. Contracts touched

| Contract | Owner (repository / session) | Change | Owner must agree first? |
|---|---|---|---|
| `Operation::Chains` answer (`lib.rs:3033-3040`), MCP `switchboard_chain_get` | fabric-switchboard | additive: `readiness`, `last_handover` | no |
| New `Operation::{Ceilings, SetCeiling, AnswerCeiling}`; MCP `switchboard_ceilings` (read, read-only set), `switchboard_ceiling_set` (write), `switchboard_ceiling_answer` (write, *keep* only); CLI `switchboard ceiling list|set|clear|keep`; `switchboard continue --ceiling <usd>` | fabric-switchboard | new | no (the plugin's tool list `plugins/switchboard/skills/switching-accounts/references/tools.md` changes with it, a plugin change through `make-skill`) |
| Tauri IPC (`src-tauri/src/main.rs:1097`) | fabric-switchboard | new commands `chains`, `set_chain`, `ceilings`, `set_ceiling`, `answer_ceiling`; numbers and codes only, never a credential (AGENTS.md *Local rules*) | no |
| `<data>/spend.json` and backup `SETTING_FILES` (`backup.rs:71`) | fabric-switchboard | new file, additive to backups | no |
| Log event `fallback` outcome codes (`fallback.rs:364-366`), new log event `ceiling` | fabric-switchboard | additive codes | no |
| `docs/CONTRACTS.md` → Fallback chains, Automatic fallback; new section Spending ceilings | fabric-switchboard | doc of record | no |
| `docs/ANALYTICS.md` `app_active` props | fabric-switchboard | additive counts `chains`, `ceilings` (Q6) | no (privacy change rule: planted-identifier test in the same change) |
| Observatory OpenRouter door (`openrouter.py issue/limit --reset daily`) | project-observatory-dashboard | **untouched**: read-only reliance on keys it issues; no call from Switchboard in 0.7.0 | no |
| Observatory grant registry / meter (`grant check`) | project-observatory-dashboard, Fabric hub (grant projection) | **untouched** in 0.7.0; SB-96 later | yes, later |
| fabric-workspace `knowledge/authority.md:56-62` (ceiling question) | fabric-workspace | consumer: implemented as written; one note to add there after release, "first product implementation: Switchboard 0.7.0, window-only *double*" (the page says "products follow it as they meet ceilings") | no |
| Contract DEC-0027 `budgets[]` (`fabric-agent-contract docs/DECISIONS.md:608-650 @898d816f`) | fabric-agent-contract | **not touched**: Switchboard publishes no usage report. When it does: machine ceiling → `{scope: machine, kind: daily, period}`; group → `{scope: pool, subject: <pool>, kind: daily, period}`; task → `{scope: job, kind: "task", since: <first paid hand-over>}` (a cumulative window; `per_job` carries no window and no spend, so it does not fit; an unknown kind is shown generically, DEC-0027); `enforced: true`, `tripped` while a question is open | no |
| `docs/ux/scenarios.md`, `docs/ux/screens.md` | fabric-switchboard | new scenarios and screen (node C-1) | no |
| `docs/brand/terminology.md`, `strings.md`, `src/locales/ru.ts` | fabric-switchboard | new terms: spending ceiling «потолок расходов», daily ceiling «дневной потолок», key's own ceiling «собственный лимит ключа» | no |

## 6. Nodes

Scenario first: C-1 lands before any window node. Strings of C-3 and C-7 go through
`copywriting` and Copylot: `copylot_check` in `en-US` and `ru-RU`, `register: ui`, with the brand
pack's facts (`docs/brand/voice.md`, `terminology.md`, `facts.md`); no Switchboard UI voice is
saved in Copylot (`copylot_voices_list q=Switchboard` returns only "PassionCode.ai site voice"
and "site voice v2", ids `f85d8875-…`, `041b4138-…`), so the check runs with `voiceRef` the
site voice v2 and the report ids go into the change; Russian is Copylot's adaptation
(`copylot_prepare`) within the spend limit.

| id | repo | owns (files) | does | tests (★ = fails today) | risk | deps | one pass? |
|---|---|---|---|---|---|---|---|
| C-1 | fabric-switchboard | `docs/ux/scenarios.md`, `docs/ux/screens.md`, `docs/ux/flows.md` | Writes the scenarios FIRST (ids provisional, renumbered at integration if another dossier took them): **SCN-074** "See and set the fallback chain in the window" (machine row, preset, editor, clear); **SCN-075** "Give a group its own fallback chain" (group rows, Groups card line); **SCN-076** "See whether a chain can take work now" (readiness, last hand-over, task rows); **SCN-077** "Cap what paid agents may spend a day" (machine and group daily ceilings, task ceiling at the switch, key's own ceiling beside it); **SCN-078** "A ceiling is reached: double it or keep it" (question, notification, window-only *double*). Updates SCN-039 (entry point gains the window, drops "An app screen comes later"), SCN-040 (alt paths `key_exhausted`, `ceiling_reached`, `spend_unknown`, group chain by checkout), SCN-065 (group key row shows its daily ceiling). Adds **SCR-03 Agents → Fallback chains** and the API keys ceilings to screens.md | ★ `grep -c "SCN-074" docs/ux/scenarios.md` prints 0 today; `python3 scripts/check_docs.py` | R1 | none | yes |
| C-2 | fabric-switchboard | new `crates/switchboard-runtime/src/chain_view.rs`; `crates/switchboard-runtime/src/lib.rs` (the `Chains` arm only); `src-tauri/src/main.rs` (`chains`, `set_chain`); `src/adapter.ts`, `src/types.ts`, `src/demo.ts` | Part A: `preview`, `Chains` gains `readiness` and `last_handover`; IPC and demo data (a machine chain, one group chain, one task chain, a skipped Hermes) | ★ `owner_tests::the_chain_view_names_why_each_step_would_be_skipped` (fails today: no `readiness` field); `chain_view::tests::a_key_pinned_step_reads_not_automatic_and_a_disabled_agent_reads_disabled`; MCP handshake still lists the tools; typecheck | R2 | C-1 | yes |
| C-3 | fabric-switchboard | `src/main.ts` (Agents panel, editor, Groups card line), `src/ui-logic.ts` (`chainSummary`, `moveStep`, `stepReasonText`, `accountsForStep`), `src/locales/ru.ts`, `src/style.css`, `scripts/test-ui-logic.mjs` | Part B. Every string through copywriting and Copylot (C-1's scenarios name them). Design review gate (`task-pipeline:design-tester`) in WebKit on the demo, English and Russian | ★ `test-ui-logic.mjs`: `moveStep(['claude-code','codex'], 1, -1)` gives `['codex','claude-code']` and `accountsForStep('hermes', [subscription])` is empty (fails today: functions absent); `check-locale.mjs`; `check-error-vocabulary.mjs`; `check-brand.mjs` | R2 | C-1, C-2 | no (screen plus review round) |
| C-4 | fabric-switchboard | new `crates/switchboard-runtime/src/spend.rs`; `crates/switchboard-runtime/src/lib.rs` (`Ceilings`, `SetCeiling`, `AnswerCeiling` arms); `crates/switchboard-core/src/backup.rs` (`SETTING_FILES`) | Part C: `spend.json` (0600, bounded, pruned), ceilings per scope, the refusal of a group ceiling without its own key, questions with *keep* / *double* (double only with `answered_by: window`), backups | ★ `spend::tests::a_doubled_ceiling_is_tied_to_its_value_and_the_next_hit_asks_again`; `a_group_without_its_own_key_cannot_take_a_daily_ceiling`; `double_is_refused_from_mcp_and_cli`; `an_unreadable_spend_file_hands_nothing_over_never_no_ceiling` (fail closed, as `agent_settings_unreadable`, `fallback.rs:610-614`); backup round trip carries `spend.json` | R3 | C-1 | yes |
| C-5 | fabric-switchboard | `crates/switchboard-runtime/src/fallback.rs` (`hand_over` gate, outcome codes, baseline, `last_handover`), `crates/switchboard-runtime/src/agent_keys.rs` (a `credit_for(store, folder)` helper and a sum over keys) | Part D. The gate is a parameter of `hand_over` (like `act`) so tests inject spend without the process-wide `SWITCHBOARD_OPENROUTER_BASE` | ★ `fallback::tests::a_hermes_step_on_a_key_with_nothing_left_is_skipped_not_offered` (today `hand_over` calls `act` for the Hermes step: fails); `a_reached_group_ceiling_skips_the_paid_step_and_opens_one_question`; `an_unread_balance_with_a_ceiling_is_a_denial_and_without_one_runs`; `a_subscription_step_is_never_gated`; `nothing_is_read_from_openrouter_when_no_paid_step_is_reached` (idle budget) | R3 | C-4, C-8 | no (gate, then baseline and status) |
| C-6 | fabric-switchboard | `crates/switchboard-cli/src/main.rs` (`ceiling` command, `continue --ceiling`, the command-to-tool table at :2863), `crates/switchboard-cli/src/mcp.rs` (three tools), `crates/switchboard-cli/tests/mcp.rs`, `plugins/switchboard/skills/switching-accounts/SKILL.md`, `.../references/tools.md` | D-4 parity for ceilings; `switchboard_ceiling_answer` takes `keep` only and names the window for *double* | ★ `mcp::handshake_lists_tools_and_rejects_malformed_messages` extended to `switchboard_ceilings` (fails today: absent); `ceiling_answer_double_over_mcp_is_refused_with_the_window_named`; CLI `ceilings_are_set_listed_kept_and_cleared_from_the_cli`; `python3 scripts/check_plugin.py` | R3 | C-4 | yes |
| C-7 | fabric-switchboard | `src/main.ts` (API keys: machine and group ceilings, key's own ceiling beside them; question banner on Agents and API keys), `src/ui-logic.ts` (`ceilingQuestion`, `ceilingWarning`), `src/locales/ru.ts`, `src-tauri/src/suggestions.rs` (counter poll for `spend.json`: notifications), `src-tauri/src/main.rs` (`ceilings`, `set_ceiling`, `answer_ceiling`), `src/adapter.ts`, `src/types.ts`, `src/demo.ts` | The money surface. The question shows two buttons of equal weight, *Double to ${2X}* and *Keep ${X}*, no primary, no preselection (the rule's "without a recommended option") | ★ `test-ui-logic.mjs`: `ceilingQuestion({value: 5})` gives two actions with the same class and no `recommended` flag (fails today: absent); `ceilingWarning` says the key's own ceiling is monthly when `limit_reset: monthly`; design review in WebKit, both languages | R3 | C-3, C-4, C-5 | no (screen plus review round) |
| C-8 | fabric-switchboard | `crates/switchboard-runtime/src/fallback.rs` (`pass_with` order: checkout before `chain_for`) | Part E | ★ `fallback::tests::a_groups_chain_applies_to_a_workflow_in_its_folder_on_the_ordinary_cli` (today the machine chain applies: fails); `a_folder_in_two_groups_keeps_the_executors_group_and_logs_ambiguous` | R2 | C-1 | yes |
| C-9 | fabric-switchboard | `docs/CONTRACTS.md`, `AGENTS.md` (lifecycle: the monitor reads OpenRouter `/key` only at a paid switch; `spend.json` in Writes), `docs/ANALYTICS.md` (Q6), `docs/brand/terminology.md`, `docs/brand/strings.md`, `docs/packets/cross-agent-continuation.md` (state line), `docs/evidence/backlog.md` and `docs/HANDOFF.md` (under the agent-sync lease), `CHANGELOG.md` | Docs of record in the same change as the code; backlog: SB-71 done, SB-72 split (shipped part done; grant-backed project ceilings and provider-side per-scope keys to SB-96/SB-97), SB-73 status part done | ★ `grep -c "## Spending ceilings" docs/CONTRACTS.md` prints 0 today; `python3 scripts/check_docs.py`; `python3 scripts/agents_doc.py --check`; `grep -n "$(printf '\342\200\224')"` over every changed doc prints nothing | R1 | C-2 … C-8 | yes |

Count by risk: R1 2 · R2 3 · R3 4.

Shared files and order: `lib.rs` (C-2 then C-4, distinct match arms), `fallback.rs` (C-8 then
C-5), `src/main.ts` and `src-tauri/src/main.rs` (C-3 then C-7). Every R3 node gets the three
blind verifier tiers plus a live check on a synthetic key at the stand-in, never a real key.

## 7. Deletes

| Node | Removed |
|---|---|
| C-1 | `docs/ux/scenarios.md:820` "An app screen comes later (SB-71 remainder)."; SCN-040's alt path "a step pinned to a paid key, or Kimi Code on a paid key → skipped until SB-72 and SB-73 phase 2" rewritten to name SB-96 (keyed pins stay skipped for the grant, no longer for the ceilings) |
| C-5 | `crates/switchboard-runtime/src/fallback.rs:12-14` header sentence "skipped with its reason until its launch recipe and paid-key ceilings exist (SB-72, …)" rewritten: ceilings exist, keyed pins wait for SB-96; the comment at `fallback.rs:202-203` "waits for SB-72/SB-96" becomes "waits for SB-96" |
| C-9 | `docs/CONTRACTS.md:419-420` "spending ceilings for paid keys are SB-72" replaced by a link to the new section; `docs/CONTRACTS.md:486-489` "(SB-72, SB-96, SB-73 phase 2)" narrowed to SB-96 and phase 2; backlog rows SB-71 closed, SB-72 closed with its split named |
| none | No code path is removed: keyed pins keep `agent_not_automatic_yet`; the preset, CLI and MCP chain tools are unchanged |

## 8. Questions

| id | question | options → consequence | recommendation | whose (operator / peer: name it) | blocks |
|---|---|---|---|---|---|
| Q1 | Where may *double* be answered? | (a) window only → an agent can never raise a ceiling; the operator must open the window (b) window and an interactive CLI with a typed confirmation → a terminal agent could type it (c) also MCP when the operator says so in chat (dashboard parity) → needs an operator credential Switchboard does not have | (a) | operator | C-4, C-6, C-7 |
| Q2 | Task ceiling measure in 0.7.0 | (a) key `usage` since the first paid hand-over, over-counting when agents share the key, checked only at a switch → ships now, labelled (b) no task ceiling until SB-97 metering → D-2's third level missing in 0.7.0 (c) a per-task key from the door → needs the door on the machine and the provisioning key | (a) | operator | C-4, C-5 |
| Q3 | Are Claude Code or Codex steps on an **API-key** account gated? | (a) yes when a ceiling covers them, `spend_unknown` since their spend cannot be read → a ceiling can never be bypassed through an API-key account, but such a step stops being used once any ceiling is set (b) no, ceilings cover OpenRouter steps only, and the window says so → an API-key Claude account spends without a Switchboard ceiling | (a) | operator | C-5 |
| Q4 | Should Switchboard ever move the key's own provider ceiling (through the door) when the operator doubles? | (a) no, Switchboard's ceiling only, the window warns when the key's own ceiling is lower, monthly or higher (b) yes, call `openrouter.py limit NAME --set 2X --reset daily` → needs the door's key name for each slot, the provisioning key and the door on this machine | (a) in 0.7.0; (b) after SB-167 delivers keys from Observatory with their names | operator, then project-observatory-dashboard | none in 0.7.0 |
| Q5 | Which door issued the keys the operator uses today, and are they daily? | (a) the public door (`--reset daily` exists, `openrouter.py:2089 @04514443`) (b) the private engine (monthly only, `tools/openrouter.py:300 @56894b01`) → the "provider-enforced daily ceiling" in SB-72's row holds only for (a) | read the key's `limit_reset` from `switchboard agents openrouter status` before release and write the result into SCN-077; move keys to the public door if any reads `monthly` | operator (a measurement the agent takes; the key move is the operator's) | C-9 (evidence) |
| Q6 | Analytics for the new surfaces | (a) counts only in `app_active`: `chains`, `ceilings` → one privacy change, no new event (b) new events `chain_changed`, `ceiling_decided` → more insight, two catalogue rows and the planted-identifier test (c) none | (a) | operator (privacy rule, AGENTS.md *Local rules*) | C-9 |

## 9. Out of scope / program

| Item | Why later |
|---|---|
| SB-96: keyed pins (`agent#project/env/NAME`) checked against the project's grant, and run | Needs a project grant projected by Fabric's hub and read by Observatory's meter; without one the meter denies (`METER.md:57-74 @04514443`). No date (`docs/reports/2026-10-25-audit-and-plan/README.md:185`) |
| SB-97: per-grant metering through the proxy, and an exact task measure | Needs the `/openrouter/<pool>` proxy route and the grant id (`docs/evidence/backlog.md:104,153`); replaces part D's task measure when it lands |
| Per-project ceiling read from the project's spend policy (RM-20) instead of set in Switchboard | Project policies wait on Fabric CO-280 and Observatory J-11 (`authority.md:51-53`); when they ship, the group ceiling becomes a read of the policy, the window shows its source, and the local value stays only as a stricter override |
| Per-scope keys issued by Switchboard through the door (XA-01's original enforcement) | Door is operator-estate only; an organization's Macs have none (`authority.md:16-19`); revisit with SB-167's delivered keys |
| SB-73 phase 2 remainder: Kimi Code on a paid key, a Hermes executor's own limit, live acceptance on a capped key | `docs/evidence/backlog.md:80`; the live acceptance of XA-01 ("in Kimi Code on a capped key, stopping at the ceiling", `cross-agent-continuation.md:150-152`) can run after 0.7.0 on part D |
| A "task chain" form in the window | Workflow ids are typed by agents, not people; CLI and MCP keep it; revisit if SCN-076 shows people need it |
| A `fabric-service/0.1` usage report with DEC-0027 `budgets[]` | Switchboard publishes none; mapping recorded in section 5 |
