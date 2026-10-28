# Dossier B: the person of an organization (runbook, first run, joining and leaving, the PassionCode.ai group)

One dossier per cluster of inventory items (`references/iteration.md` → *Module dossiers*),
written in parallel with the others, read-only on every repository: the dossier is the only
write. **A dossier is a design, not a plan**: it does not cut waves. Evidence for every
claim: `file:line @commit`, or a command and its output. No secret values, ever.

Written 2026-10-26 against:

- fabric-switchboard `a9ff8fa` (worktree `iteration/0.7.0`); paths below are repository-relative at that commit.
- passioncode-vesta `383c31e` (`docs/design/2026-10-23-rollout-estate-and-appvillis.md`, last changed `f40da46`;
  `docs/design/2026-10-22-onboarding-playbook.md`; `docs/design/2026-10-26-org-source-of-truth.md`).
- fabric-workspace `origin/main` `cd58ca1d` (`knowledge/integration.md`; `docs/reports/2026-10-14-call-serge-khurs/README.md`).
- fabric-dashboards `876eac9` (`src/core/setup.ts:78`, a consumer of `switchboard onboard`).
- The installed CLI on the operator's MacBook: `switchboard --version` → `switchboard 0.6.83`.

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
| SB-202 | A runbook for a person of an organization (install, the organization's group and seats, *Connect PassionCode.ai*, the shell hook, launching work) and the window's first run on a machine with an organization group, scenario first | **yes.** No runbook exists: `ls docs` lists no organization guide (INSTALL.md, AGENT-ONBOARDING.md, CLI.md and others, none about an organization). The first run knows nothing of organizations: the tour's five steps name accounts, switching, groups and agents (`src/main.ts:46-52`); the empty Accounts page leads with "Start with one account" (`src/main.ts:1242`); `switchboard onboard` has five steps, none about an organization (`crates/switchboard-runtime/src/onboarding.rs:85`, `:97`, `:122`, `:165`, `:213`). Row: `docs/evidence/backlog.md:212` | P2 |
| SB-160 | Organizations phase 4, Switchboard's part of joining and leaving: attribution of the new employee's account, the signed-out state when a seat is removed | **yes, changed.** Attribution of a joining seat already holds with no new code once the organization exists: rule 3 (domain) in `Snapshot::attribution` (`crates/switchboard-core/src/organizations.rs:143-183`), computed on every read, never stored. What does not hold: (a) the new seat lands in the default group, not the organization's group (`src/main.ts:1126`, `startLogin(..., pool = 'default')`; the runtime files it under `login.pool`, `crates/switchboard-runtime/src/lib.rs:1443-1449`); (b) a removed seat is told apart from nothing: "sign in again" appears only after `invalid_grant` (`crates/switchboard-runtime/src/refresh.rs:1019-1023`), a 401 that survives a fresh renewal is returned once and forgotten (`crates/switchboard-runtime/src/monitor.rs:552-560`), and a Codex seat, never renewed, reads as a failed check (`src/ui-logic.ts:257-259`); (c) leaving the organization on PassionCode.ai unlinks it silently (`organizations.rs:481-485`). Row: `docs/evidence/backlog.md:169` | P2 |
| SB-125 | Serge Khurs's work runs in the PassionCode.ai group on PassionCode.ai accounts (decided 2026-10-25, D-6) | **yes.** Measured on the operator's MacBook, 2026-10-26: `switchboard --json project list` → 4 groups, all with `"organization": null`; group `passioncode` ("PassionCode") holds 5 accounts (4 on `passioncode.ai`, 1 on another of the operator's domains) and 48 folders, among them `fabric`, `fabric-workspace`, `fabric-switchboard` and `passioncode-vesta`. `switchboard --json org list` → `"organizations": []`, `"passioncode": {"base": "https://org.passioncode.ai", "connected": false}`. (Labels withheld; counts computed from the JSON.) The brief's `switchboard --json projects` is not a command: it answers `{"error":"Invalid arguments. Run switchboard --help.","ok":false}`; the read is `switchboard --json project list`. Row: `docs/evidence/backlog.md:133`; decision `docs/reports/2026-10-25-audit-and-plan/README.md:251` | P2 |

## 2. As-is

**Organizations exist only as a page and a rule.** An organization is a name, domains, provider
organizations and an optional Vesta id (`crates/switchboard-core/src/organizations.rs:32-46`). An
account belongs to one by the person's word, then its provider organization, then its address's
domain (`organizations.rs:143-183`); a group bound to an organization admits only that
organization's seats (`organizations.rs:211-218`), and a group whose seats all left stays bound and
refuses (SB-170, `crates/switchboard-runtime/src/bindings.rs:843`). Nothing reads the network to
attribute.

**PassionCode.ai brings names and domains, no members, no provider organizations.** The read keeps
`orgId`, `name`, `domains` (`crates/switchboard-runtime/src/vesta.rs:363-375`); Vesta's answer also
carries the role (Vesta `docs/design/2026-10-26-org-source-of-truth.md:86`), which Switchboard
ignores. A proposed second scope `org.providers` would add the provider organizations
(`2026-10-26-org-source-of-truth.md:87`; integration ledger: VES-ORG-PROVIDERS design note by
2026-11-07, `knowledge/integration.md:153`). An organization Vesta stops listing is unlinked and kept
as a local one (`organizations.rs:481-485`), and `vesta.json` records no trace of it
(`vesta.rs:61-73`, `Link` with `deny_unknown_fields`).

**The first run is generic.** The usage question comes first, then the five-step tour
(`src/main.ts:60-61`, `tourShown` waits on `tourWaits`), then the empty Accounts page
(`src/main.ts:1242`, led by `firstRunHint`, `src/ui-logic.ts:897-900`). The PassionCode.ai panel
lives only on *Organizations* (`src/main.ts:2270-2294`). A machine whose only group is an
organization's is treated like any other: the person is never told which seat to sign in with,
nor where it goes.

**`switchboard onboard` is the agent's checklist.** Five steps, read from the machine, writing
nothing: `owner`, `accounts`, `project`, `mcp`, `proving_call`
(`crates/switchboard-runtime/src/onboarding.rs:77-235`; table in `docs/AGENT-ONBOARDING.md:27-33`).
Served over MCP as `switchboard_onboarding`, and named in Fabric Dashboards' setup prompt by these
five ids (fabric-dashboards `src/core/setup.ts:78` @`876eac9`).

**Joining.** A sign-in from the window starts in the default group unless a caller passes one
(`src/main.ts:1126`); finishing it files the account under the pool the sign-in began with
(`crates/switchboard-runtime/src/lib.rs:1443-1449`). The notice then says where it went
(`signInNotice`, `src/ui-logic.ts:436-445`). Domain attribution makes the badge appear by itself,
but the organization's group never receives the seat unless the person moves it (SCN-062).

**Leaving.** Three signals, none read as "your seat was removed":

- A Claude renewal answered `invalid_grant` marks the lineage dead and the account `sign_in`
  ("This account's sign-in has ended. Sign in to it again.", `refresh.rs:29`, `:1019-1023`;
  `src/ui-logic.ts:254`); rotation never picks it (`monitor.rs:816-824`).
- A usage check answered 401 is renewed once and probed again; a second 401 returns
  "Provider rejected the credential. Sign in again." (`crates/switchboard-proxy/src/lib.rs:1168`,
  `:1346-1349`; `monitor.rs:552-560`) and is not remembered: the next pass repeats it, and the card
  reads *failed* (`src/ui-logic.ts:257-259`). Whether Anthropic answers a removed Team seat with
  `invalid_grant` or with a 401 on a token that still renews is **not measured**.
- A Codex account is never renewed while inactive (SB-16 open), so a removed ChatGPT workspace seat
  can only show as 401 on its usage check, read as *failed*.
- On the proxy, a 401 or 403 on a managed request is journaled as `unauthorized` and nothing else
  (`crates/switchboard-proxy/src/lib.rs:872-879`); only a 429 moves a request (SB-98).

**Moving a group to another machine.** `switchboard settings export|import` carries groups by
repository identity and organizations by name and domains, no account
(`crates/switchboard-runtime/src/settings_bundle.rs:1-5`; `docs/CLI.md:70-73`). It has no window
surface and no scenario (`grep -n "settings import" docs/ux/scenarios.md` prints nothing).

**Windows.** The same code paths build for Windows, but nothing here was ever run natively (SB-02,
`docs/evidence/backlog.md:9`; packet `docs/packets/windows.md:1-12`): the vault is the Windows
credential store with DPAPI, the data folder `%LOCALAPPDATA%\ai.passioncode.fabric-switchboard`, the
CLI `switchboard.exe` beside the app with no `~/.local/bin` link (`docs/INSTALL.md:55-64`), the
browser for *Connect* opens through `rundll32 url.dll,FileProtocolHandler` (AGENTS.md lifecycle,
child processes), sessions open in PowerShell (`crates/switchboard-runtime/src/launch.rs:426`,
`powershell_quote`), the login item is `HKCU\…\Run`. Vesta was macOS-only until V-9 and opened
VES-WINDOWS (`knowledge/integration.md:65`, `:218`, X-16).

**SB-125 as measured** (section 1): no organization, PassionCode.ai not connected, the
`passioncode` group unbound with one seat outside `passioncode.ai`. Serge Khurs's work is the
spec-writing scenario and project timeline in the Fabric repository (RM-29), on "a stable Fabric"
he takes (fabric-workspace `docs/reports/2026-10-14-call-serge-khurs/README.md`, §1 items 1 and 2);
the notes do not say which machine it runs on.

## 3. To-be

One idea carries all three items: **a person of an organization is set up when one checklist,
computed once in the runtime, reads done**: the organization is known, its group exists, the
person's own seat is in it and signed in, plain sessions in its folders run in its context (SB-183),
and one launch proved it. The agent reads that checklist (`switchboard onboard`), the window shows
it on the first run, the runbook walks it step by step, and SB-125 is its first real run. Joining
and leaving are two states of the same checklist's `seat` step.

### 3.1 The organization checklist (SB-202, runtime; answers SB-202 and the agent half of SB-125)

`onboarding::steps` gains steps that appear **only** when an organization exists on this machine
(local or from PassionCode.ai); a machine without one gets exactly today's five steps.

| id | done when | state otherwise | next |
|---|---|---|---|
| `organization` | at least one organization exists | `optional` while none and PassionCode.ai is not connected | `switchboard org connect` (the person's browser consent: `needs_person`) |
| `org_group` | each organization has exactly one group bound to it (`Snapshot::organization_group`, new, core) | `todo` | one of: `switchboard settings import <bundle> --under <dir> --apply` (a bundle the administrator gave), `switchboard project create --pool <id> --organization <name>`, or bind an existing group that already holds only that organization's seats (`project edit --organization`) |
| `seat` | the group holds at least one account attributed to the organization, enabled, not marked elsewhere, not in `sign_in_required` | `needs_person` | `switchboard login begin --provider claude --pool <group>`; `person`: "Sign in with your {organization} address" |
| `hook` | SB-183's hook is on for this shell (its status call, owned by the SB-183 node) | `optional` (on by default by D-3, so this reads the state and names the one command that turns it off) | SB-183's command |
| `launch` | a session with context `org:<id>` was recorded (SB-110 phase 1 already records it, `organizations.rs:195-209`) | `optional` | `switchboard launch --pool <group> --folder <a group folder>` |

`complete` stays "every step done or optional", so an organization machine without a signed-in seat
is not complete (Q5). Each step carries the organization's id and name; no e-mail, no label leaves
the machine (the checklist is local; the MCP answer goes to the person's own agent).

### 3.2 The window's first run on an organization machine (SB-202, window)

Order stays one card at a time: the usage question (SCN-073), then the tour (SCN-035), then the
**organization card** "Set up {organization} on this computer", rendered from the same checklist
(read through the Tauri `onboard` path, never recomputed in TypeScript). It sits above the Accounts
page and on top of *Organizations*, lists the steps with their state, and offers the one action of
the first step not done: *Connect PassionCode.ai…*, *+ Group for {organization}* (the group dialog
with the organization chosen and its seats preselected), *Sign in with your {organization}
address* (starts the sign-in in that group's pool), or the hook line. It goes away when the
checklist is complete; *Hide* keeps it on *Organizations* only. With more than one organization,
one card per organization not yet set up, newest first.

The empty Accounts page on a machine with an organization group and no account leads with the
seat: "Sign in with your {organization} address. It goes to the group {group}." instead of
`firstRunHint`'s generic line (`src/main.ts:1242`). Without an organization nothing changes.

### 3.3 Joining: a new seat lands in its organization's group (SB-160)

- A sign-in started from the organization card or the empty state begins in the organization's
  group pool (the caller passes it; no default changes).
- Any other sign-in or capture whose account is attributed to an organization that has exactly one
  group, and that landed elsewhere, gets `organization_group: {pool, name}` in its receipt
  (`finish_login`, additive), and the window's notice adds *Move it to {group}* (one action, the
  SCN-062 move). Never moved without the person: AGENTS.md and the MCP instructions keep account
  moves explicit.
- Attribution itself stays as built (rule 3, domain). When Vesta ships `org.providers`, rule 2
  (provider organization) takes over for seats on other addresses (section 9).

### 3.4 Leaving: a seat the organization removed reads as ended (SB-160)

- **One remembered verdict.** A usage check whose 401 survives a fresh renewal (Claude), or a 401 on
  two scheduled checks in a row (Codex, which is not renewed), puts the account into the same
  in-memory set as a dead lineage, keyed by the credential's fingerprint and cleared when the
  credential changes (`RefreshState`, `refresh.rs:51-70`; `sign_in_required`, `:392`). A single 401,
  any 403 and any other status are not verdicts (a 403 can be a plan or region refusal,
  `proxy/src/lib.rs:1346`). Rotation already excludes the set (`monitor.rs:816-824`); nothing new
  moves a session (Q3).
- **Text that names the organization.** An account in that set attributed to an organization reads:
  "{label}'s sign-in has ended. If {organization} removed your seat, ask its administrator; to keep
  using it, sign in again." A personal account keeps today's text.
- **A group whose seats have all ended** says so on its card: "None of {group}'s accounts is signed
  in. Sessions in its folders are refused until one is." (SB-170's line stays for seats that left
  the organization.)
- **Leaving PassionCode.ai.** When a read drops an organization, `vesta.json` records
  `released: [{organization, at}]` (local id and time, no name), and its card says "Not in your
  PassionCode.ai organizations since {date}. Its groups still refuse other accounts. Remove it when
  you no longer need it." The organization, its group binding, the account and its statistics stay
  (organizations research §6 "keeps its history", `docs/research/organizations-2026-10-21.md:87`).
  Switchboard deletes nothing on leaving; removal is the person's (SCN-069 alt path).
- **Logs:** `oplog` events `seat_ended {provider, by: renewal|usage, organization: bool}` and
  `organization_released {count}`: codes and counts only, no label, no organization name
  (AGENTS.md log row).

### 3.5 The runbook (SB-202, docs)

`docs/ORGANIZATION.md`, linked from README *Quick start* and `docs/INSTALL.md`, written for a person,
not an engineer, with each step's proof:

1. Install: `npx --yes @passioncode-ai/passioncode@latest setup --yes --json` (the rollout's one
   command, Vesta rollout `:55`) or the download; open the app once; answer the usage question.
2. Connect PassionCode.ai: *Organizations* → *Connect PassionCode.ai…*, or `switchboard org connect`;
   the browser consent is the person's.
3. The organization's group: import the administrator's settings file
   (`switchboard settings import <file> --under <your code folder> --apply`), or the card's
   *+ Group for {organization}*.
4. The seat: accept the provider invitation that reaches your own mail through the forward (Vesta
   playbook J5 to J7), then *Sign in with your {organization} address*; the badge appears.
5. Plain `claude` and `codex` in the group's folders: SB-183's hook, on by default (D-3); the one
   command that turns it off.
6. Launch work and check the context: Activity shows the organization's badge; `switchboard sessions`
   names `org:<id>`.
7. Proof that setup is complete: `switchboard onboard --status --json` → `"complete": true`.
8. Leaving: what you will see (3.4) and what stays on your computer.

A **Windows** column per step: `switchboard.exe` beside the app instead of the `~/.local/bin` link;
the PowerShell profile hook (SB-183, X-16); `--under` names the folder holding your checkouts
(no `~/DATA` there); the Windows credential store; the notification-area icon; and the line "not
yet verified on a Windows machine" until SB-02 runs (Q6). English first, Russian beside it (Q4).

### 3.6 SB-125: an operator-assisted setup, no code

SB-125 is configuration plus one human gate, run with the 0.7.0 runbook as its first acceptance:

1. On the operator's MacBook (and the Studio): `switchboard org connect` (one browser consent; also
   the SB-159 live check, D-8). PassionCode.ai arrives with `passioncode.ai`.
2. Bind group `passioncode` to PassionCode.ai. Today it would be refused with "This group still
   holds accounts that belong elsewhere. Move them out first." (`organizations.rs:26-27`) because of
   the one seat on another domain: place it in PassionCode.ai by the person's word or move it out
   (Q2).
3. Serge Khurs's machine (Q1, recommended: his own) runs the runbook: his own PassionCode.ai seat(s),
   signed in there, in a group bound to PassionCode.ai holding his checkout of `fabric`. The seats he
   takes are marked *Used on another machine* on the operator's Macs (SB-175, SCN-072), so they are
   neither renewed nor rotated there.
4. **Verification** (read-only, safe to repeat): `switchboard --json project list` shows
   `"organization_name": "PassionCode.ai"` on the group; `switchboard --json org list` lists his
   seats with `"by": "domain"` (or `"person"`); `switchboard --json onboard --status` on his machine
   answers `"complete": true`; `switchboard --json sessions` shows a session with `org:<id>`.
   Receipts go in the backlog row; labels stay out of Git.

## 4. Options considered

| Choice | Options | Chosen | Why |
|---|---|---|---|
| Where the organization checklist is computed | (a) runtime `onboard` steps, read by the window and the agent; (b) a TypeScript checklist in the window; (c) only the runbook | (a) | One truth for three readers (agent, window, runbook); `onboard` already reads the machine and writes nothing (`onboarding.rs:1-4`), and Dashboards already sends agents to it (fabric-dashboards `src/core/setup.ts:78`) |
| How a new machine gets the organization's group | (a) the administrator's settings bundle; (b) a *+ Group for {org}* button prefilled; (c) Vesta sends group templates | (a) and (b) | (a) exists and carries repositories by identity (`settings_bundle.rs:1-5`); (b) needs no administrator; (c) needs a Vesta contract that does not exist |
| Where a joining seat goes | (a) the sign-in starts in the organization's group when started from its card; plus an offer to move a seat that landed elsewhere; (b) move any organization seat automatically; (c) nothing | (a) | Moving accounts without the person is refused by this repository's rules (AGENTS.md; MCP instructions "Never move account IDs"); (c) leaves the seat in the default group, where the ordinary Claude Code rotation could spend it |
| How a removed seat is detected | (a) remember a 401 that survives a fresh renewal (Claude) or repeats (Codex) in the existing in-memory set; (b) persist a new `usage_health.status` value; (c) ask Vesta whether the seat exists | (a) | (b) breaks a downgrade: `UsageHealth::valid` accepts only `ok`, `failed`, `unavailable` (`crates/switchboard-core/src/lib.rs:120-125`), so an older build would refuse the file; (c) Vesta does not know seat state (playbook Q12: Switchboard reads only `/v1/me/organizations`) |
| What leaving does on the machine | (a) mark, keep everything, the person removes; (b) remove the organization and its group binding; (c) delete the seat's credential | (a) | SB-159 decided "keep on leaving" (`backlog.md:168`); (b) would make the group personal and admit the person's own seats into the former employer's folders; (c) deletion is the person's act |
| Where the released mark lives | (a) `vesta.json` `released[]`; (b) a field on `Organization` in `accounts.json` | (a) | (b) changes the snapshot every backup carries and an older build's `deny_unknown_fields` (`organizations.rs:31`) would refuse the whole store; (a) risks only the PassionCode.ai connection reading as unreadable after a downgrade (`vesta.rs:60`), which a reconnect repairs |
| Runbook home | (a) `docs/ORGANIZATION.md` linked from README and INSTALL; (b) a section of INSTALL.md; (c) Vesta's docs | (a) | INSTALL.md is per platform and already long; Vesta's playbook J7 can link to it (section 5) |

## 5. Contracts touched

| Contract | Owner (repository / session) | Change | Owner must agree first? |
|---|---|---|---|
| `switchboard onboard --status --json` and MCP `switchboard_onboarding`: new step ids `organization`, `org_group`, `seat`, `hook`, `launch` | fabric-switchboard | additive; semantics: `complete` is false on an organization machine until a seat is signed in | no; tell Fabric Dashboards, whose setup prompt names the five ids (`src/core/setup.ts:78` @`876eac9`) |
| `docs/AGENT-ONBOARDING.md` step table | fabric-switchboard | additive rows | no |
| `finish_login` receipt (Tauri, CLI `login finish`, control) | fabric-switchboard | additive field `organization_group` | no |
| Monitor status `sign_in_required` (window, `fallback.rs:446`, `:656`, recovery read by Fabric Dashboards every 15 s, `refresh.rs:383-391`) | fabric-switchboard | widened meaning: also a provider-rejected seat | no; note in HANDOFF for Dashboards |
| `vesta.json` | fabric-switchboard (local file) | additive `released[]` | no |
| `docs/ux/scenarios.md`: SCN-074 to SCN-077 | fabric-switchboard | new | no; ids reserved when written (no "Next free ID" register, AGENTS.md); other 0.7.0 dossiers (SB-183, SB-71) also add scenarios |
| `docs/brand/terminology.md`: "seat" (место) | fabric-switchboard brand pack | new term | no (copywriting route) |
| Vesta `GET /v1/me/organizations` | passioncode-vesta | unchanged in 0.7.0; `org.providers` read later | no |
| Vesta playbook J7 text ("signs in on their own machine, in Switchboard", `2026-10-22-onboarding-playbook.md:69`) | passioncode-vesta | a link to `docs/ORGANIZATION.md` | yes, Vesta edits its own doc |
| `knowledge/integration.md` Switchboard row and I2-1 | fabric-workspace | the runbook as the member's Switchboard step | yes, fabric-workspace keeps the page |
| Backlog rows SB-202, SB-160, SB-125 (`docs/evidence/backlog.md`, leased under agent-sync, `docs/AGENT_SYNC.md:30`) | fabric-switchboard | closure receipts | no; lease required |

## 6. Nodes

| id | repo | owns (files) | does | tests (★ = fails today) | risk | deps | one pass? |
|---|---|---|---|---|---|---|---|
| B-1 | fabric-switchboard | `docs/ux/scenarios.md` (SCN-074 to SCN-077 and index rows) | Scenarios first: **SCN-074** "Set up an organization's work on a new computer" (the card, its steps, the empty state, order after SCN-073 and SCN-035); **SCN-075** "A new seat of an organization lands in its group"; **SCN-076** "A seat the organization removed reads as ended"; **SCN-077** "An organization I left on PassionCode.ai stays here, marked". Each with Windows alt paths | ★ `grep -c "^## SCN-07[4-7]" docs/ux/scenarios.md` prints `0` today, `4` after; `python3 scripts/check_docs.py` | R1 | none | yes |
| B-2 | fabric-switchboard | `crates/switchboard-core/src/organizations.rs` (new `Snapshot::organization_group`, `organization_seats`) | The one group bound to an organization (none or several: `None`), and that organization's seats in it | ★ `organizations::tests::the_only_group_of_an_organization_is_found_and_two_are_not`; existing `organizations::tests` stay green | R1 | none | yes |
| B-3 | fabric-switchboard | `crates/switchboard-runtime/src/onboarding.rs`, `crates/switchboard-cli/src/main.rs` (probe fill), `docs/AGENT-ONBOARDING.md` | The checklist of 3.1; probe reads organizations, groups, `sign_in_required`, SB-183's hook state, recorded contexts | ★ `onboarding::tests::an_organization_machine_names_its_group_seat_and_hook`; ★ `onboarding::tests::a_seat_that_ended_is_not_a_signed_in_seat`; guard `onboarding::tests::a_machine_without_organizations_keeps_the_five_steps` (passes today, must keep passing) | R2 | B-2; the SB-183 node's hook status call | yes |
| B-4 | fabric-switchboard | `crates/switchboard-runtime/src/lib.rs` (`finish_login` receipt, capture receipt), `src/main.ts` (`startLogin` callers, notice), `src/ui-logic.ts` (`signInNotice`) | Joining (3.3): sign-in from the card starts in the group's pool; receipt `organization_group`; *Move it to {group}* | ★ runtime `a_new_seat_of_an_organization_outside_its_only_group_is_offered_it`; ★ `scripts/test-ui-logic.mjs` case "signInNotice offers the organization's group"; guard: a personal seat's receipt has no such field | R2 | B-2, B-1 | yes |
| B-5 | fabric-switchboard | `crates/switchboard-runtime/src/refresh.rs` (`RefreshState` rejected set), `crates/switchboard-runtime/src/monitor.rs` (`check`, `:535-561`), `crates/switchboard-runtime/src/oplog.rs` events | Leaving (3.4, detection): a 401 that survives a fresh renewal (Claude) or repeats on two scheduled checks (Codex) joins `sign_in_required` until the credential changes | ★ `monitor::tests::a_claude_seat_rejected_after_a_fresh_renewal_reads_sign_in_required`; ★ `monitor::tests::a_codex_seat_rejected_twice_reads_sign_in_required`; guards `a_single_codex_401_is_not_a_verdict`, `a_403_is_never_a_verdict`, `a_new_credential_clears_the_rejection`, and the idle budget test `an_idle_hour_on_a_fake_clock_stays_inside_the_budget` | R3 | none | no: changes the candidate set of automatic switching and what fallback and recovery read; needs the seam tier |
| B-6 | fabric-switchboard | `src/ui-logic.ts` (`seatEndedText`, group-card line), `src/main.ts` (card and group card), `src/locales/ru.ts` | Leaving (3.4, text): the account card names the organization; the group card when all seats ended | ★ `scripts/test-ui-logic.mjs` cases "an ended seat of an organization names it" and "a group whose seats all ended says so"; WebKit demo `?demo=1&org=ended` at 740 and 1100 px | R2 | B-5, B-1 | yes |
| B-7 | fabric-switchboard | `crates/switchboard-runtime/src/vesta.rs` (`Link.released`), `src/main.ts` (organization card line), `src/locales/ru.ts` | Leaving PassionCode.ai (3.4): record and show the release; nothing removed | ★ `vesta::tests::an_organization_left_on_passioncode_is_marked_with_its_date`; existing storage `organizations_from_passioncode_are_linked_kept_current_and_released` (`crates/switchboard-core/tests/storage.rs:1500`) stays green | R2 | B-1 | yes |
| B-8 | fabric-switchboard | `src/main.ts` (organization card, empty state), `src/ui-logic.ts` (`orgSetupShown`, `orgSetupAction`, empty-state key), `src/demo.ts`, `src/locales/ru.ts`, `src-tauri` command passing `onboard` | The first run of 3.2 | ★ `scripts/test-ui-logic.mjs` "the organization card waits for the usage question and the tour"; ★ "the empty Accounts page on an organization machine leads with the seat"; guard "no organization, no card, today's empty state"; WebKit demo `?demo=1&org=setup`; design-review gate (task-pipeline design-tester) | R2 | B-3, B-4, B-1 | yes |
| B-9 | fabric-switchboard | `docs/ORGANIZATION.md` (new, en and ru), `README.md` (Quick start link), `docs/INSTALL.md` (link) | The runbook of 3.5, with each step's proof command and a Windows column | ★ `test -f docs/ORGANIZATION.md` exits 1 today; `python3 scripts/check_docs.py`; `node scripts/check-brand.mjs`; Copylot `copylot_check` report ids (en-US, ru-RU) in the change; the em-dash grep prints nothing | R1 | B-3 to B-8, the SB-183 node | yes |
| B-10 | fabric-switchboard (no code; live machines) | backlog row SB-125 receipt | SB-125 (3.6): connect, bind `passioncode`, Serge Khurs's machine through the runbook, mark moved seats elsewhere | ★ `switchboard --json project list` shows `"organization": null` on `passioncode` today; after: `"organization_name": "PassionCode.ai"`; `switchboard --json onboard --status` `"complete": true` on his machine; a session with `org:<id>` | R2 | B-9, Q1, Q2, 0.7.0 installed; human gates: the browser consent, his sign-in | no: two machines and two people |

Count by risk: R1 3 (B-1, B-2, B-9) · R2 6 (B-3, B-4, B-6, B-7, B-8, B-10) · R3 1 (B-5).

**Strings through copywriting and Copylot** (en-US and ru-RU, with the project's saved voice;
report ids recorded in each change): the organization card's title and five step lines and its
actions (B-8); the empty-state line (B-8); the *Move it to {group}* notice (B-4); the ended-seat
card text and the all-ended group line (B-6); the released-organization line (B-7); the
`onboard` step summaries and `person` lines, which agents relay to people (B-3); the whole runbook
(B-9). "Seat" enters `docs/brand/terminology.md` before any of them.

## 7. Deletes

- B-3: none; the five steps stay as they are.
- B-6: the generic *failed* reading for an organization seat that the provider rejected
  (`src/ui-logic.ts:257-259` branch, for that case only).
- B-8: the generic empty-state line on an organization machine (`src/main.ts:1242`, that branch
  only); no tour step is removed.
- B-9: the Vesta rollout note "plain `claude` or `codex` is not bound to a context … launched through
  Switchboard" is Vesta's to update once SB-183 ships (`rollout-estate-and-appvillis.md:62-63`); in
  this repository, the research row "Switchboard shows the account as signed out or removed by the
  provider" (`docs/research/organizations-2026-10-21.md:87`) is rewritten to what B-5 and B-6 built.
- B-10: none on disk; the backlog rows SB-202, SB-160 and SB-125 close with receipts.

## 8. Questions

| id | question | options → consequence | recommendation | whose | blocks |
|---|---|---|---|---|---|
| Q1 | On which machine does Serge Khurs's work run, and is it a Mac or Windows? | (a) his own machine with his own PassionCode.ai seat(s) → the runbook is the setup, the seats are marked elsewhere on the operator's Macs; (b) the operator's Macs → one person's seat used for another person's work, which the provider terms forbid (organizations research §6, "never shares one seat between two people") | (a), with the seats named by the operator | operator | B-10 |
| Q2 | The fifth account in group `passioncode` is not on `passioncode.ai`: is it a PassionCode.ai Team seat? | (a) yes → place it in PassionCode.ai by the person's word, which also records its Team so other seats follow (`organizations.rs:554`); (b) no → move it to another group before binding | (a) if it is a Team seat, else (b) | operator | B-10 |
| Q3 | When a seat is found ended in the middle of work, may Switchboard move the group's next request to another of its accounts? | (a) no, the card and notice say what to do → a group with one seat stops, as it must; (b) yes, at the next pass, never replaying → a new automatic fallback, which AGENTS.md admits only as a recorded operator decision (SB-98 is the one so far) | (a) for 0.7.0; (b) as a program row if the pilot asks | operator | B-5 (scope only) |
| Q4 | The runbook's languages | (a) English and Russian (Copylot adapts the Russian within its limit) → the Appvillis team reads it in their language; (b) English only | (a) | operator | B-9 |
| Q5 | `onboard` reads not complete on an organization machine without a signed-in seat: does Fabric Dashboards' setup prompt need to change? | (a) Dashboards lists the new ids → its agents know them; (b) leave the prompt → agents follow `next` anyway | (a), told on the board | peer: Fabric Dashboards | none |
| Q6 | A Windows machine for the runbook's Windows column and SB-02 | already the operator's queue item Q-A (`knowledge/integration.md:248`) | keep the column marked "not yet verified on Windows" until SB-02 runs | operator (existing Q-A) | B-9's Windows proof, not its text |

## 9. Out of scope / program

- **Provider organizations from Vesta (`org.providers`).** Vesta's design note is due 2026-11-07
  (`knowledge/integration.md:153`, VES-ORG-PROVIDERS); reading it makes rule 2 attribute seats on
  addresses outside the organization's domains. `release: program`, a 0.7.x after the scope exists.
- **SB-184** (git, SSH, `gh`, MCP profile per context): Observatory 0.31.0, 2026-11-14
  (`backlog.md:194`); the runbook says the profile is shared until then.
- **Automatic held-elsewhere (SB-177)**: after Fabric CO-412; the runbook uses the manual mark (SB-175).
- **Automatic move off an ended seat** (Q3 (b)): only if the operator decides it.
- **Enterprise provisioning** (Claude SCIM, ChatGPT Admin API): organizations research §6; Vesta's
  later row.
- **A window surface for the settings bundle** (import from a file in the window): useful for people
  who never open a terminal; `release: program` unless the pilot shows the CLI step blocks people,
  since the onboarding agent (V-12) runs the command for them.
- **Kimi Code, Hermes and OpenCode contexts (SB-201)** and **the hook itself (SB-183)**: other
  dossiers of this iteration; this dossier consumes the hook's status call only.
