# 04 Surfaces and distribution

Part of the [target state and refactoring plan](README.md). This section covers everything a person
or an agent touches: the desktop window and its pages, the menu-bar icon and notifications, the
`switchboard` CLI, the MCP server and its plugin, updates, install and uninstall on every platform,
usage counts, logs, digests and crash records, the release pipeline, and the brand, copy and UX
scenario rules that are the truth for the window. It includes the surfaces of the operator's three
additions of 2026-11-17: an account turned off (and since when), the session pool's wait and its
reason, and the devices of an organization.

Read at `0e09c0f2` (branch `target-state`, on top of `f5e2c77e`, release 0.7.114). Nothing was
built or run for this section (the disk is nearly full): every claim about current behaviour cites
`file:line` or a test name read in source; what could not be checked that way says "not verified".
Severity: P0 breaks a user outcome now, P1 blocks a target outcome, P2 degrades one, P3 hygiene.

Sources of truth used: [SPEC](../../SPEC.md) §3, §10, §16; [CONTRACTS](../../CONTRACTS.md) (Uninstall,
Attention, Window to MCP parity); [DECISIONS](../../DECISIONS.md) DEC-0003, 0004, 0005, 0006, 0008,
0011, 0013, 0014, 0015, 0019, 0022, 0025; [CLI](../../CLI.md); [INSTALL](../../INSTALL.md);
[DISTRIBUTION](../../DISTRIBUTION.md); [ANALYTICS](../../ANALYTICS.md); [ux/screens](../../ux/screens.md),
[ux/scenarios](../../ux/scenarios.md); [brand](../../brand/README.md); the Lifecycle table of
[AGENTS.md](../../../AGENTS.md); the window and fleet plan
([WF README](../2026-11-14-window-and-fleet/README.md)); the window audit
([ui-audit](../2026-11-15-quality-waves/ui-audit.md)); open rows of [the board](../../evidence/backlog.md).

### 1. Window shell, navigation and startup

**Target.** One window, left navigation, every page reachable in one press, the current page marked
for a screen reader; a page that cannot read shows an error with *Retry*, never an empty list; a
re-read that fails after a successful action keeps the last list under one line with *Retry*
(SPEC §3, §15; SCN-024, SCN-043; screens.md "Window behaviour and words": UX-06, UX-07). One primary
action per screen (VIS-21, UX-01). Density: five account rows and *Add* visible at 1100x760, no
horizontal scroll at 900 px (SPEC §10). Keyboard: dialogs open on their first field, Escape cancels,
focus returns to the trigger (SPEC §10, SCN-013). A browser without the native app sees only "Open
the native app"; the demo (`?demo=1`) carries a banner.

**Now.** Ten pages: `type Page` at `src/main.ts:41`; sidebar loop `src/main.ts:417-422` (Accounts, API
keys, Groups, Organizations, Agents, Sessions, Activity, Statistics, Settings, About), titles and
subtitles `src/main.ts:428-429`, header actions `src/main.ts:432-445`, error, loading and stale states
`src/main.ts:474-484`, browser fallback `src/main.ts:448-451`, demo banner `src/main.ts:426`. `render()`
(`src/main.ts:392-401`) rebuilds the whole shell and compares `outerHTML` with the last render to skip
a background repaint; a 60 s `backgroundRefresh` (`src/main.ts:4661`). The window holds about 60
module-level `let` state variables (`src/main.ts:22-857`, e.g. `capacityState` at 637, `dutyState` at
673, `balancingState` at 727). Pure logic is tested in `scripts/test-ui-logic.mjs` (about 1100
assertions over `src/ui-logic.ts`); there is no DOM, Playwright or end-to-end test in the gate
(`package.json:12-20`, `scripts/check.sh`): page behaviour is proved only by manual WebKit design
reviews recorded under `docs/runs/`.

**Gap.** P2: no automated test renders a page, so a regression in `src/main.ts` (4675 lines) is caught
only by a person. P3: the 60-variable global state makes stale-read bugs easy (each page has its own
`*Loading`/`*Error` trio). P3: there is no Machines page although SCN-092 and WF-12.1 (SB-374) want
one; *Other machines* lives inside Groups (`src/main.ts:545-557`).

**Refactor.** Split `src/main.ts` into one module per page (`pages/accounts.ts`, `pages/groups.ts`,
...) plus `shell.ts` (sidebar, header, notices, tour) and `components/` (`foldSection`, dialogs,
menus, rows). Replace the per-page `let` trios with one typed store of reads (`Read<T> = {value,
loading, error, at}`) and one loader per Tauri command. Keep `ui-logic.ts` pure and split it by page
too (2224 lines). Add a DOM test runner (happy-dom or WebKit through Playwright, run against the demo
adapter) with one smoke test per page and state (loading, error, empty, populated), and put it in
`scripts/check.sh`.

### 2. Accounts page

**Target.** Accounts by machine: this computer first (Claude Code, then Codex), then each other
machine as its own section; one filter row; Claude and Codex *In use now* alike; *Given to agents*
names the agent, its organization, its machine and where it works (WF-2, R-01 to R-05, SB-364). Each
row: provider mark, label with badges, e-mail or credential type, a two-line quota cell (meter, share
used, check age; then countdown and reset, or the failed, stale or sign-in text), one primary action
(Switch, Select, Sign in, Enable) and a ⋯ menu (SCR-01 0.5 revision, SCN-021, SCN-032). The selection
says "Selected for next request", never claims the provider accepted it (SPEC §3). Unknown, stale and
failed are never shown as zero (SPEC §9). Accounts keeps only "Automatic switching is on for N groups"
and warnings about the accounts (DEC-0014).

**Now.** `renderAccounts` `src/main.ts:2478`, `renderCurrent` (In use now) `src/main.ts:2980`, sections
by machine `accountSections` `src/ui-logic.ts:507`, row `accountRow` `src/main.ts:2734` (badge
"Disabled" at 2755, *Enable* at 2800, "Not checked while disabled" or the remote line at 2884), card
state `cardState` `src/ui-logic.ts:289`, *Given to agents* `givenSection` `src/main.ts:2194`. Released
0.7.80 and 0.7.82 (SB-363, SB-364 rows). Tests: `test-ui-logic.mjs` over `accountSections`,
`cardState`, `quotaOrder`, `autoSwitchPossible`.

**Gap.** Audit items 21 to 23 and 49 to 53 (Kimi row without a bar, dangling separators, badge "Sign in
again" vs button "Sign in", *In use now* details styled as links) are not listed as fixed in the
audit's status sections ([ui-audit](../2026-11-15-quality-waves/ui-audit.md), which claims every item
is fixed, kept or recorded): not verified in the built window, P3 (SB-379, SB-192). P2: live
acceptance with real accounts is still open (SB-01, SB-15).

**Refactor.** Move the row, quota cell and ⋯ menu into `components/accountRow.ts` with a typed view
model built in `ui-logic` (state, badges, primary action, menu items), so the row is one pure function
tested without the DOM, and the same view model feeds the CLI's human line (section 12).

### 3. An account turned off, and since when (operator addition 1)

**Target.** One switch takes an account out of everything until a person turns it on (README,
addition 1). Every surface says it is off, who turned it off and since when: the window row ("Off
since 17 Nov, 18:02, turned off by you"), its ⋯ menu (*Turn on*), Needs attention never lists it as a
problem, `switchboard accounts list` (a `STATE` of `off` and a `SINCE` column), `--json` and MCP
`switchboard_accounts` (`enabled: false`, `off_since`, `off_by`: `person`, `agent`, or the cause). An
account off because it works on another machine reads as a different state ("On {machine}", DEC-0013),
never as the person's off. Nothing turns it on by itself (an import, sync, restore, recall or lease).

**Now.** One bit, `Account.enabled` (`crates/switchboard-core/src/lib.rs:244`); no time and no cause
is stored (no `disabled_at`, `off_since` or similar anywhere in `crates/`, `src/`, `src-tauri/src/`).
`Store::update_fields` (`crates/switchboard-core/src/lib.rs:2159-2196`) records the change only as
`account_updated` with detail `success`, so the journal cannot say which field changed. Enabling
clears the elsewhere mark and brings the account home from its machine group (`lib.rs:2186-2190`),
and clearing an elsewhere mark enables the account again (doc comment `lib.rs:2197-2200`): the
person's off and "used elsewhere" share the bit. Window: badge "Disabled" (`src/main.ts:2755`),
*Enable* (`src/main.ts:2800`), "Not checked while disabled" (`src/main.ts:2884`). CLI human list:
`STATE` `disabled` (`crates/switchboard-cli/src/main.rs:5245-5251`), no time. MCP: `switchboard_accounts`
returns `enabled` only (`crates/switchboard-cli/src/mcp.rs:449`).

**Gap.** P1: "since when" and "by whom" cannot be shown on any surface because they are not stored.
P1: the off state is ambiguous (person's off vs elsewhere vs a seat lease ended); a recall or an
elsewhere clear turns an account on, contrary to the addition. P2: wording says "Disabled" while the
operator's words are "turned off"; terminology has no entry for it (docs/brand/terminology.md).

**Refactor.** Core (area 01) adds `off: Option<Off { at, by, cause }>` beside `enabled`, journalled as
`account_turned_off` / `account_turned_on` with the cause. Surfaces read one derived field
`availability` (`on`, `off_by_person`, `elsewhere`, `lease_ended`) from the snapshot, so the window,
CLI and MCP word it from one place; a golden test fixes the CLI and MCP shapes.

### 4. Needs attention

**Target.** What needs a person, one list at the top of Accounts, each item with its reason and the one
action that fixes it, the account staying in its group (DEC-0015, CONTRACTS "Attention"); the window,
`switchboard attention` and MCP `switchboard_attention` read the same answer. The target adds: the
session pool's queue (section 5), an owner crash found at start, and agent health in the
`agent-health/1` sense (SB-393).

**Now.** `attentionSection` `src/main.ts:3022`, actions `attentionAct` `src/main.ts:3047-3062`, reason
words `attentionReasonText` `src/ui-logic.ts:624` (twelve reasons: `session_duplicate`,
`session_stuck`, `session_gone`, `dispatch_refused`, `sign_in`, `seat_ended`, `renewal_refused`,
`usage_failing`, `move_unknown`, `refused_there`, `key_outside_tokens`, `ceiling_question`), core
`crates/switchboard-runtime/src/attention.rs`; CONTRACTS table lists eleven (no `refused_there` row,
CONTRACTS.md "Attention" table). Crashes are recorded in `bugs.jsonl` (`crates/switchboard-runtime/src/bugs.rs:70`,
test `a_start_after_a_crash_records_it_and_a_drain_marks_its_run_clean`) and read by `switchboard
bugs` and `switchboard health` (CLI parity rows `crates/switchboard-cli/src/main.rs:6396-6397`), but
there is no attention reason for them and no window surface (`src/main.ts` has no `health` or `bugs`
read; only `watchProblems` reports window errors, `src/main.ts:7-9`).

**Gap.** P2: a crash of the owner and the `health` report are invisible in the window. P3: CONTRACTS's
reason table misses `refused_there`. P1 for addition 3: see section 5.

**Refactor.** One `AttentionReason` enum generated from `attention.rs` into `src/types.ts` (or checked
by a test that compares the two lists), so a reason added in Rust fails the build until the window
words it.

### 5. The session pool: its wait and the reason (operator addition 3)

**Target.** Board work never starves behind interactive sessions: a share of the pool is reserved for
board runs, interactive sessions past their quiet time are counted out, a refused start waits in a
queue, the queue's wait and reason are visible in Needs attention, and Fabric is told when a slot frees
(README, addition 3). Every surface says: cap, used, reserved for the board, queued runs with their
age, why each waits ("34 sessions work, the limit is 20, 26 are interactive"), and when the last slot
freed. Window: Settings → *Coding sessions* and a Needs attention item with the queue; CLI
`switchboard capacity` (human and `--json`); MCP `switchboard_capacity`.

**Now.** The pool is a cap only (`crates/fabric-sessions-core/src/capacity.rs:1-11`): `decide`
refuses with `AT_CAPACITY` (`capacity.rs:88-100`), no queue and no reservation; interactive and board
sessions count alike (`used`, `capacity.rs:57-62`, quiet after `machine_sessions::QUIET_MINUTES`,
which the window words as 30 minutes, `src/main.ts:645`). Refusals are counted for a day in
`dispatch-refusals.json` (SB-412, CONTRACTS "Refused dispatches"); the window shows "N board runs were
refused in the last hour ... No session has ended since." (`dispatchRefusedLine`,
`src/ui-logic.ts:613-619`) in Settings (`src/main.ts:650-651`) and as the Needs attention item
`dispatch_refused` whose action opens Settings (`src/main.ts:3057`). The capacity section's limit
dialog defaults to 20 (`src/main.ts:660`). Tests: `a_quiet_session_does_not_count_and_the_cap_refuses_the_start_that_would_pass_it`
(`capacity.rs:136`), two tests in `dispatch_refusals.rs`.

**Gap.** P1: no queue, no reserved share, no wait time and no per-run reason on any surface; the
measured starvation of 2026-11-17 (cap 20, 34 counted, README addition 3) shows only as a refusal
count. P2: the attention item's action only opens Settings; there is no act that admits the oldest
waiting run. P2: `switchboard_capacity` and the window show used and quiet counts but not which
sessions count (interactive vs board), so the person cannot see why the pool is full.

**Refactor.** The queue lives in `fabric-sessions-core` (area 03); surfaces read one `pool/1` view
`{cap, used, reserved_board, interactive, board, quiet, queue: [{request, since, reason}], last_freed_at}`
built once and shared by Tauri `capacity`, CLI and MCP (a golden file for each), replacing the three
ad hoc readers (`capacitySection`, `dispatchRefusedLine`, the attention item).

### 6. Groups page

**Target.** A readable page: one state line per group card, folding to accounts, repositories,
agents and keys; standing settings in Settings (WF-8, SB-370, DEC-0014); delete or turn off only after
confirming (SCN-042); a repository in no group is in the default group (DEC-0007).

**Now.** `renderProjects` `src/main.ts:515`; *Other machines* `machinesSection` `src/main.ts:545`;
repository map and folder rules fold (`src/main.ts:517`). Released 0.7.82 (SB-370). Audit items 4, 6,
40, 42, 44, 45 recorded as fixed or kept ([ui-audit](../2026-11-15-quality-waves/ui-audit.md)).

**Gap.** P2: the window's own first-time notice and the system notification still say "Turn this off
under Groups" (`src/ui-logic.ts:1705`, `src-tauri/src/suggestions.rs:107`, Russian at 102, test
assertion `suggestions.rs:405`) while the terminal hook's switch moved to Settings → *Terminals in
organization folders* (`src/main.ts:3803`, DEC-0014): the instruction points to the wrong page.
P3: Groups has no SCR record (screens.md "Additions in 0.7.0 to pages without a screen record").

**Refactor.** Move the machines section out of Groups into the Machines page (section 7). Strings
that name a page take the page from one table (`PAGE_TITLES`) so a move renames every mention.

### 7. Organizations, other machines and devices of an organization (operator addition 2)

**Target.** Organizations page: the organizations the person has, set by them or read from
PassionCode.ai, with connect, read, bind, leave and reconnect walked end to end and one status line
per connection state (WF-9, SB-371; SCN-069 to SCN-079; an organization is a label, DEC-0022).
Machines: a page for the person's own machines (WF-12.1, SB-374, SCN-092), accounts moved both ways
(DEC-0013, DEC-0017). Addition 2: a device joins an organization's Switchboard by an invitation the
organization's owner makes (never a shared password); the Organizations page shows each
organization's devices (name, owner, last seen, seats held there, lease end), *Invite a device…*,
*Remove device…* (its seats called back), and a device's own view of the organization it joined; the
same in CLI (`switchboard org devices`, `org invite`, `org join`, `org leave`) and MCP.

**Now.** `renderOrganizations` `src/main.ts:4231` (organizations, domains, placement; no machines or
devices). PassionCode.ai membership is read as organizations with a `role` (`crates/switchboard-runtime/src/vesta.rs:743-750`,
test fixtures). Other machines: `machinesSection` `src/main.ts:545-557` under Groups; CLI `machines
list|adopt|place|forget|trust|keys`, `targets` (parity rows `crates/switchboard-cli/src/main.rs:6392-6408`);
MCP `switchboard_machines`, `switchboard_machine_set`, `switchboard_targets`, `switchboard_seats`,
`switchboard_account_send|recall`. Delegation to a teammate's machine by an `authorized_keys` forced
command (DEC-0025, CLI.md "A seat delegated to a teammate's machine"). No invitation, join or device
concept exists in code (no match for `invit` in `crates/switchboard-runtime/src/`).

**Gap.** P1 (addition 2): devices of an organization are missing on every surface. P2: SB-371 (WF-9
end-to-end walk) open; SB-374 (Machines page, admin view; waits SB-177, SB-291) open. P2: the
delegation path asks the person to edit `~/.ssh/authorized_keys` by hand (CLI.md lines 175-182), which
the operator's "presses nothing" rule and the invitation target replace.

**Refactor.** One `fleet` read model (machines, their Switchboards, seats, leases, organization
membership) served by one Tauri command, CLI `machines`/`org devices` and one MCP tool, replacing the
separate `machines`, `targets`, `seats` readers in the window.

### 8. Agents page

**Target.** The central place for agent settings: installed agents, which take over work, the
default agent, permissions (bypass or ask), fallback chains per computer, group and task with their
readiness, OpenRouter, Anthropic and Observatory keys, Hermes and Kimi (SB-372, which cites DEC-0018; SCR-03;
SCN-057 to SCN-060, SCN-080 to SCN-082, SCN-097, SCN-101). Templates and shared skills and plugins
(WF-10.2, 10.3).

**Now.** `renderAgents` `src/main.ts:1308`; chains panel and editor (`chainsPanel`, `chainDialog`,
screens.md SCR-03); permissions block released 0.7.82 (SB-372 row). Audit items 20, 30 to 38 recorded
as fixed in the audit's status of 2026-11-15 and 2026-11-16.

**Gap.** P2: WF-10.2 (versioned templates) and 10.3 (shared skills and plugins) open (SB-372). P2:
launching an agent has no MCP twin (`launch_agent`, `kimi_launch` carry `Not(LAUNCH)`,
`src-tauri/src/parity.rs:95,113`; SB-64, SB-74 open). P3: SB-383 (permissions per group by autonomy
level) waits on Fabric.

**Refactor.** The Agents page is the largest single renderer region (`src/main.ts:1308-1642`); split
into installed agents, chains, keys and Kimi modules.

### 9. API keys page

**Target.** Three folding blocks (this computer, group keys, task ceilings), one compact row per group
key, daily ceilings with *Set ceiling…* / *Clear* and the question banner (WF-7, SB-369; SCN-065,
SCN-083, SCN-084, SCN-100, SCN-101); keys named, never shown (DEC-0024).

**Now.** `renderTokens` `src/main.ts:1643`; ceiling UI `ceilingBlock`, `ceilingDialog`,
`ceilingBanners` (screens.md SCN-083 notes). Released 0.7.82 (SB-369). Saving or removing an
OpenRouter key and a Kimi account has no MCP twin: `openrouter_save`, `openrouter_remove`,
`kimi_remove` carry `Not(PERSON)` (`src-tauri/src/parity.rs:98,100,114`); CLI `agents openrouter
remove`, `agents anthropic remove`, `kimi remove` likewise (`crates/switchboard-cli/src/main.rs` parity
table).

**Gap.** P2: removing a key or an account is not a typed secret, yet it has no agent door, while
`switchboard_account_remove` exists for Claude and Codex accounts (inconsistent rule; the operator's
dashboard parity rule makes every window button an agent action). Saving a key by name from
Observatory exists (`switchboard_observatory_key_add`); typing a key value stays the person's.

**Refactor.** One `keys` module behind Tauri, CLI and MCP with `save_from_observatory`, `remove`,
`model`, `ceiling`, so "remove" is one call with one parity row.

### 10. Sessions page and the session views

**Target.** One page with every coding session here and on other machines, grouped (needs you,
running, waiting, ended in the last day), with the acts Switchboard has (close the quiet copy, open
here, end, continue) and restart waves (SCR-04, SCN-098, SCN-099; SB-395, SB-397). One session view,
not several.

**Now.** `renderSessionsPage` `src/main.ts:3561`, restart panel and dialogs (SCR-04 coverage list),
each act has an MCP twin except `session_attach` (`src-tauri/src/parity.rs:132-137`, reason: an agent
reads output with `switchboard attach`). Activity still renders two more session lists before its
events: `sessionTreePanel` ("Sessions on this computer") and `sessionsPanel` ("Launched by
Switchboard"), `src/main.ts:3722-3723`. Golden answers for session commands exist for CLI and MCP
only (`crates/switchboard-cli/tests/golden/sessions-cli.json`, `sessions-mcp.json`, test in
`crates/switchboard-cli/tests/golden.rs`).

**Gap.** P2: three session surfaces (Sessions page plus two Activity panels) count and name sessions
differently (audit item 1 found two of them disagreeing; the third was added later). P3: SB-65's
superseded remainder belongs to Fabric Dashboards; the window and Fabric's console still both show
sessions (not verified whether they agree).

**Refactor.** Keep the Sessions page as the only session list; Activity links to it. The session-row
shape (SB-395 `session-row`) is the one view model for window, CLI `sessions --rows` and MCP.

### 11. Activity and Statistics

**Target.** Activity by day with kind and account filters and paging; Statistics folding to top 10
with *Show all*, sticky headers, a section index, no long scrolling (WF-11, R-14, SB-373; SCN-011,
SCN-067). Events never carry payloads, tokens or provider bodies (SPEC §7).

**Now.** `renderActivity` `src/main.ts:3721-3765` (day folds, filters, paging, "Show more");
`renderStats` `src/main.ts:3286`; core `crates/switchboard-core/src/activity.rs`; CLI `events --kind
--account --before`, `stats`; MCP `switchboard_events`, `switchboard_statistics`. Released 0.7.82
(SB-373).

**Gap.** P3: the event journal records `account_updated` without the field (section 3), so Activity
cannot say "turned off". P3: audit items 10 to 16 and 26 to 29 (panel padding, scrollbar, tile
wrapping, copy) are not in the audit's fixed lists; not verified.

**Refactor.** Event wording from one table keyed by `action` and a typed `detail` (not a free string),
shared by `eventAction`/`eventDetail` (`src/ui-logic.ts`) and the CLI's human `events`.

### 12. Settings page

**Target.** Every standing setting of this computer on one page, one folding section each, none open
until opened, each header saying its state (DEC-0014, SCN-093): automatic switching, balancing, coding
sessions (the pool), the duty officer, own sign-in, Codex in group folders, terminals in organization
folders, running in the background, updates, language and appearance, backups, usage counts.

**Now.** `renderSettings` `src/main.ts:3786-3810`, twelve `foldSection`s with keys `switching`,
`balancing`, `session-limit`, `session-duty`, `own-sign-in`, `codex-route`, `terminals`, `background`,
`updates`, `language`, `backups`, `usage-counts`. Released 0.7.80 (SB-363). Each section reads its own
state on page open (`src/main.ts:3792`).

**Gap.** P3: the `updates.rs` module comment still says the switch is in "About → Running in the
background" (`src-tauri/src/updates.rs:5`). P3: Settings has no SCR record. P2: the pool section lacks
the target's queue and reservation (section 5).

**Refactor.** Each section becomes a component with `read`, `state line`, `body` and its Tauri command,
so a section's header state and its body share one read.

### 13. About, tour, first run and the provider terms notice

**Target.** About keeps the product, terms and tour (DEC-0014); five-step tour beside its target, never
over it (SCN-035, VIS-12); provider terms notice on Accounts until dismissed, and in About (SCN-049);
usage notice once on the first start (SCN-073, DEC-0005); demo first run `?demo=1&empty=1` (UX-27).

**Now.** `renderAbout` `src/main.ts:3766-3784` (definitions, tour, terms, product panel); "Automatic
switching: On by default" (`src/main.ts:3779`, audit item 3 fixed); tour steps `src/main.ts:50-52`.
UX-27 done (SB-192 row).

**Gap.** P3: About's definitions still use the jargon audit item 58 named ("Imported OAuth",
`src/main.ts:3777`); not listed as fixed. P3: the remaining SB-192 items (VIS-13 rest) open.

**Refactor.** None beyond section 1's page split.

### 14. Menu-bar icon, residency, login item and notifications

**Target.** Closing the window hides it; the icon offers *Open Switchboard*, *Quit Switchboard* and,
once an update is ready, *Restart to update*; login item on by default, off in Settings (DEC-0003,
SCN-033, Lifecycle table). Native notifications: a folder joined or left a group, the terminal hook's
first write, a hand-over offered, a ceiling question, an update ready (screens.md "Notifications").
Target addition: the icon says when something needs the person (Needs attention count), and a
notification fires once per new attention item a person must act on (sign-in ended, pool starving)
(SPEC §14 lists "menu-bar monitoring" as follow-on).

**Now.** Tray menu `src-tauri/src/residency.rs:254-286` and `tray` at 387; update notices at 291-351;
login item `set` at 191 (refused outside the installed app, line 193); keep-alive plist at 89-101
(tests in `residency.rs`, 7). Notifications from `src-tauri/src/suggestions.rs:241-289` (folders,
terminal hook, ceilings; the shell hook text at 98-110). No notification or tray badge for attention
items (no attention read in `residency.rs` or `suggestions.rs`).

**Gap.** P2: a sign-in that ended or a starving pool reaches the person only when the window is open.
P2: the wrong page in the terminal hook notification (section 6). P3: SB-02 lists Windows
notification-area behaviour as unaccepted.

**Refactor.** One `notify` module in `src-tauri` with a table (event, title, body in both languages,
once-key), replacing the per-feature `notification().builder()` calls in two files.

### 15. The CLI

**Target.** Account CRUD, usage, activity, login, launch, `serve`, and every window act, with human and
`--json` output; `--json` writes `{ok, data}` to stdout or `{ok: false, error}` to stderr; exit 0, 1
(operational), 2 (invalid arguments); `--help` never opens storage; no credential, capability or
payload in output (SPEC §16, CLI.md "Usage, output and failures"). Every CLI command has an MCP tool
or a written reason (`every_cli_command_has_an_mcp_tool_or_a_reason`). Target addition: every window
button has a CLI twin (operator's dashboard parity), proved by a test; every `--json` answer has a
golden file.

**Now.** `crates/switchboard-cli/src/main.rs` (6967 lines): clap definitions to about line 867, `run`
from 2379 to 3945 (one function, about 1570 lines), `print_result` from 4970 to 5815 (about 845 lines),
the CLI to MCP table at 6318-6570 and its test at 6584. Tests: 36 in `tests/commands.rs`, 13 in
`tests/mcp.rs`, 8 in `tests/shell.rs`, 12 in `main.rs`; golden answers only for sessions
(`tests/golden/`, 2 files). The MCP stack test runs under a 1 MiB stack (`scripts/check.sh`).

**Gap.** P2: no test checks window to CLI parity (the window's acts against CLI commands); some window
acts may have only an MCP or only a Tauri path (not verified per command). P2: `--json` shapes are
pinned by golden files only for sessions; every other answer can change shape unnoticed. P3: SB-197
(`sessions --all` home naming) open.

**Refactor.** Split `main.rs` into `cli/args.rs` (clap), one `cli/commands/<area>.rs` per command
group (accounts, org, machines, sessions, agents, keys, project, settings), and `cli/render/<area>.rs`
for human output; `run` becomes a dispatch table. Generate the CLI and MCP parity tables from one
registry (command path, operation, MCP tool, window command, reason) so the three parity tests read
one source. Add golden files per area, generated as the sessions ones are.

### 16. The MCP server and its plugin

**Target.** Every window action has an MCP tool (DEC-0019); tools carry honest annotations
(read-only, destructive, idempotent, open world); `--read-only` lists only read tools; the plugin's
`.mcp.json` registers `switchboard mcp`, its hooks run the recovery, monitor and project-suggestion
hooks; the tool reference matches the server (CLI.md "For agents"). A tool that changes the
operator's choices takes `decision` (SB-299, SB-385).

**Now.** `crates/switchboard-cli/src/mcp.rs` (2423 lines): 92 tools in `tools()` (38 read, 54 write;
`mcp.rs:129-224`), matching CLI.md's "38 read tools" and 92 entries in
`plugins/switchboard/skills/switching-accounts/references/tools.md`. Annotations by one rule
(`mcp.rs:226-246` when written, 2026-11-17; on main since SB-517 at `mcp.rs:236-249`, with `DELETES` at
`mcp.rs:263`): the three tools that end a process are destructive and not idempotent; every other
write tool was `destructiveHint: false, idempotentHint: true` (SB-517 added `DELETES`), every tool `openWorldHint: false`.
`decision` for analytics and machine trust (`mcp.rs:248-262` when written). The server follows an updated binary
(`mcp.rs:54-110`, SB-375). Plugin hooks (`plugins/switchboard/hooks/hooks.json:1-61`) call bare
`switchboard ... 2>/dev/null || true`.

**Gap.** P2: annotations are wrong for several tools: `switchboard_account_remove` deletes a stored
sign-in that cannot be restored but reads `destructiveHint: false`; `switchboard_project_delete`,
`switchboard_backup_restore` are not idempotent in effect; `switchboard_login` begin,
`switchboard_kimi_login`, `switchboard_github_clone`, `switchboard_account_send` are not idempotent;
`switchboard_remote`, `switchboard_account_send|recall`, `switchboard_settings_sync`,
`switchboard_github*`, `switchboard_openrouter_models` reach other machines or the network yet read
`openWorldHint: false`. P2: plugin hooks need `switchboard` on `PATH`; when it is not, every hook fails
silently (`|| true`), so limit stops and monitors are lost with no trace. P2: no MCP twin for capture,
Claude Swap import, launches and key removal (sections 8, 9; DEC-0019 reasons `PERSON`, `LAUNCH`).
P3: no `switchboard_continue` (SB-64, SB-74).

**Refactor.** Move the tool table out of `mcp.rs` into a typed registry (`name`, `title`, `schema`,
`kind: read|write|destructive|external`, handler), derive annotations from `kind`, and generate
`tools.md` from it (a check fails on drift). Hooks call an absolute path the plugin resolves at
install, or `switchboard` with a fallback that records the miss in `bugs.jsonl`.

### 17. Parity across window, CLI and MCP

**Target.** Dashboard parity: every window button has a CLI and an MCP twin; MCP may do more, never
less (DEC-0019, R-15); the only exceptions are acts that need a person's body (typing a secret, a
sign-in, a consent), named once. Operator's rules: deleting, launching and approving inside an
ordered task are agent acts.

**Now.** Window to MCP: `every_window_command_has_an_mcp_tool_or_a_reason` over 126 Tauri commands
(`src-tauri/src/main.rs`, `generate_handler!` at 2069; table `src-tauri/src/parity.rs:24-167`, about
20 rows carry a reason). CLI to MCP: `every_cli_command_has_an_mcp_tool_or_a_reason`
(`crates/switchboard-cli/src/main.rs:6584`). Window to CLI: no test. Reasons that are not a person's
body: `capture_current` and `import_claude_swap` (`parity.rs:50-51`, saving the sign-in the CLI
already holds), `openrouter_remove`, `kimi_remove` (100, 114), the three launches (74, 95, 113),
`session_attach` (132-137).

**Gap.** P2: the PERSON reason covers acts with no typed secret (capture, import, removals), contrary
to the operator's dashboard parity and full-delegation rules. P2: window to CLI parity untested
(section 15). P2: launches without an MCP twin (SB-64, SB-74).

**Refactor.** One parity registry (section 15) with a `person_gate` field limited to `secret`,
`sign_in`, `consent`; a test fails any other exemption.

### 18. Updates on every platform

**Target.** The installed app and `serve` read the feed 90 s after start and every five minutes, also
after a failure (DEC-0004, DEC-0011); download in the background, verify the minisign signature, install
(macOS bundle at once, Windows NSIS `/P /UPDATE` after the drain, Linux AppImage replaced in place),
restart at an idle moment at most once in six hours unless urgent, never cutting a managed request
(LC-16, SB-387); off in Settings; a `.deb` says it is behind with the exact command (SB-408); `serve`
replaced by another account drains and exits 75 (DEC-0011).

**Now.** `src-tauri/src/updates.rs` (2428 lines, 25 tests): platform rules `updates.rs:1-21`,
`availability` 117-131 (a non-AppImage Linux copy is unavailable), `.deb` message 85-87, idle and
drain rules 151-268, resumable download 1093-1260, Linux AppImage install 1449-1484, restart and
relaunch helper 305-342 and 1569-1700. `serve`: `crates/switchboard-runtime/src/serve_update.rs` (5
tests), `self_update.rs` (6 tests); CLI `update` (reason row in the CLI parity table) and `app
updates|check-updates|restart-to-update`; MCP `switchboard_app`. Tray *Restart to update*
`residency.rs:263-286`.

**Gap.** P3: SB-408 (`.deb` reports the latest version and the install command) open; today the text
only says "Install the newer .deb from the releases page" (`updates.rs:87`). P2: SB-387's `task` for
a Fabric console run open. P2: SB-02: Windows update path not accepted on a real host.

**Refactor.** `updates.rs` mixes pure rules (availability, idle, cooldown, drain) with the Tauri loop and
the download; move the pure rules into `switchboard-runtime` beside `serve_update.rs` so the app and
`serve` share them (today two implementations of the same cadence).

### 19. Install, uninstall and platforms

**Target.** macOS universal (14 and later, notarized), Windows x64 and arm64 (NSIS, Authenticode),
Linux x64 and arm64 (.deb, AppImage, CLI tarball); each download with `SHA256SUMS`, its signature and a
Sigstore attestation (INSTALL.md); the CLI linked into the app's copy (Agents → *Link switchboard*);
`switchboard uninstall` plans first, then removes vault items, the CLI link, the terminal hook and the
data folder unless `--keep-data` (CONTRACTS "Uninstall"); live acceptance on every platform.

**Now.** Packaging scripts `scripts/build_macos.py`, `package_windows.py`, `package_linux.py` with
tests in the gate (`scripts/check.sh`); uninstall `crates/switchboard-runtime/src/uninstall.rs` (9
tests), CLI only (no window or MCP twin, reason `PERSON`). Windows: the fix for SB-243 (no account
could be saved on Windows) is in code (`crates/fabric-sessions-core/src/windows.rs:188`
`check_foreign_file`, refusals logged `external_file_refused`,
`crates/switchboard-runtime/src/external.rs:83`), but the row still reads open and native acceptance
(SB-02) has not happened. Linux: interactive use per desktop and live provider acceptance "not
verified" (INSTALL.md, Linux limit). Download page lacks Linux and Windows on ARM (SB-91).

**Gap.** P1: SB-02 and SB-243 acceptance on a Windows host; SB-01 live provider acceptance. P2: SB-91.
P3: SB-92 (shared Windows signing action).

**Refactor.** None in code; the acceptance packets (`docs/packets/windows.md`,
`docs/packets/provider-acceptance.md`) become scripted where a host exists (studio machines run Linux
in containers; a Windows host is the operator's gate).

### 20. Usage counts, logs, digests, error counts, crashes and health

**Target.** Counts and kinds only, on from the first start, sent only after the notice was drawn (DEC-0005,
DEC-0157 via SB-385), off in Settings, an MCP door that needs the operator's words (ANALYTICS.md);
lifecycle log 5 x 5 MB, codes only (LC-12); hourly and daily `ops.digest` lines (SB-250); error counts
per day and code (SB-248); crashes found at the next start in `bugs.jsonl`, `switchboard bugs`, MCP
`switchboard_bugs` (SB-379 W3); one `agent-health/1` report (SB-393).

**Now.** `crates/switchboard-runtime/src/analytics.rs` (2111 lines, 29 tests; `shown_version`,
`shared_no` at 247-275, merged in PR #762 `56bef611`); `digest.rs` (tests
`an_hour_folds_by_operation_with_totals_and_its_window`, `hours_the_computer_slept_through_are_never_written`);
`oplog.rs:19` (5 MiB cap); `bugs.rs` (tests `only_an_unclean_run_whose_process_is_gone_crashed`,
`the_list_keeps_the_newest_two_hundred`); `health.rs` (tests
`our_report_reads_as_valid_by_the_contracts_rules`,
`needs_attention_crashes_and_an_error_storm_set_the_worst_state`). The window reports its own errors
as codes (`src/bugs.ts`, `scripts/test-bugs.mjs`).

**Gap.** P2: health and crashes have no window surface (section 4). P3: the board rows for merged work
read "in review" (SB-385 merged in #762; SB-357 in #765; SB-391 in #760; SB-404, SB-407, SB-411,
SB-413 released 0.7.113 and 0.7.114, `git log`), so a reader of the board sees open work that is done.

**Refactor.** None structural; the privacy test of planted identifiers stays the gate for any new prop
(AGENTS.md local rules).

### 21. Release pipeline and nightly

**Target.** A release is a merged release PR (versions and a CHANGELOG section) and a pushed `vX.Y.Z`
tag; the workflow builds, signs, notarizes, attests, sums, GPG-signs and publishes every platform and
the updater feed; a published release is never rewritten (DISTRIBUTION.md, `.github/workflows/release.yml:1-36`).
Full hosted checks run nightly at 23:00 Warsaw, never per push (AGENTS.md). Checks-based products need
no click (operator rule).

**Now.** `scripts/release_bump.py` moves the version in every file (97 lines, tested by
`test_release_bump.py`); `scripts/release_preflight.py` refuses a mismatch or a missing CHANGELOG
section; `release.yml` (550 lines) on tag push; `nightly.yml` (160 lines): two cron lines for daylight
saving decided by `scripts/nightly_clock.py`, macOS, Windows x64 and arm64, Linux x64 and arm64 jobs
(`nightly.yml:37,62-67,123`). Versions agree at 0.7.114 (`package.json:3`, `src-tauri/tauri.conf.json:4`,
`Cargo.toml:5`, plugin manifest). The release job's signing runs in a protected `release` environment
approved by a person (`release.yml` header): whether it is checks-based today is not verified.

**Gap.** P2: whether a release waits for a reviewer is not verified from the repository (environment
rules live in GitHub). P3: SB-354, SB-361, SB-362, SB-234, SB-410 flaky or stack-bound tests under load
make nightly red for reasons unrelated to a change. P3: SB-350 (the architect's judged audit) open; the
architect advises and never gates (AGENTS.md).

**Refactor.** Area 05 owns the test hardening; here, the release workflow reads the parity and golden
checks as release blockers (they are in `check.sh`, so they already run nightly).

### 22. Brand, copy, locales and UX scenarios as the truth for the window

**Target.** A person reads group, API keys, automatic switching (DEC-0006); every user string is in the
registry, checked by Copylot in English and Russian, no em dash (brand README, global rule); the window
follows `docs/ux/scenarios.md` and `screens.md`, every page has a screen record, every scenario is in
the index and moves from draft to validated with evidence.

**Now.** Gate checks: `check-brand.mjs`, `check-locale.mjs`, `test-strings-registry.mjs`,
`check-strings-registry.mjs`, `check-error-vocabulary.mjs`, `docs/ux/lint.py` (`scripts/check.sh`).
User-facing strings in `src/` carry no U+2014 (the occurrences in `src/main.ts` and `src/ui-logic.ts`
are code comments only). Scenarios: 103 headings; the index (`docs/ux/scenarios.md:7-110`) is split by
a blank line after SCN-013, lacks SCN-096 and SCN-097 (headings at 2077 and 2098), and several index
titles still say project or pool where the heading says group (SCN-025, SCN-034, SCN-062 to SCN-065);
the linter reports a missing index row only as a warning (`docs/ux/lint.py:1097-1106`, U003). 24
scenarios are validated, 79 draft. Screen records exist only for Accounts, Activity, Agents and
Sessions (SCR-01 to SCR-04); Groups, Organizations and API keys are listed "without a screen record"
(screens.md), Settings and Statistics have none.

**Gap.** P3: DEC-0018 (WF-10, cited by SB-372) and DEC-0020 (routing between machines, cited by SB-374)
are held but never written (`docs/DECISIONS.md:9`), so two built or planned behaviours have no
decision record. P3: index drift and missing screen records; scenarios for the three additions (account off and
since when, pool queue, devices of an organization) do not exist yet. P3: the window's copy "Turn this
off under Groups" (section 6) shows the registry checks words, not where they point.

**Refactor.** Make U003 an error, generate the index from the headings, and add SCR records for
Groups, Organizations, API keys, Settings, Statistics and the coming Machines page; write SCN entries
for the three additions before their builds.

## Summary

| Capability | State | Gap severity | Backlog ids |
|---|---|---|---|
| 1 Window shell, navigation, startup | partial | P2 | SB-374 |
| 2 Accounts page | partial | P2 | SB-01, SB-15, SB-192, SB-379 |
| 3 Account turned off, since when | partial | P1 | new (addition 1) |
| 4 Needs attention | partial | P2 | SB-393 |
| 5 Session pool wait and reason | partial | P1 | SB-412, new (addition 3) |
| 6 Groups page | partial | P2 | SB-370 |
| 7 Organizations, machines, devices of an organization | partial (devices missing) | P1 | SB-371, SB-374, new (addition 2) |
| 8 Agents page | partial | P2 | SB-372, SB-64, SB-74, SB-383 |
| 9 API keys page | done (parity gap) | P2 | SB-369 |
| 10 Sessions page and session views | partial | P2 | SB-395, SB-397, SB-65 |
| 11 Activity and Statistics | done (copy items unverified) | P3 | SB-373, SB-379 |
| 12 Settings page | done | P3 | SB-363 |
| 13 About, tour, first run, terms | done | P3 | SB-192 |
| 14 Menu-bar icon, residency, notifications | partial | P2 | SB-02 |
| 15 CLI | partial | P2 | SB-197 |
| 16 MCP server and plugin | partial | P2 | SB-64, SB-74 |
| 17 Parity across window, CLI and MCP | partial | P2 | SB-363, SB-64, SB-74 |
| 18 Updates on every platform | partial | P2 | SB-408, SB-387, SB-02 |
| 19 Install, uninstall, platforms | partial | P1 | SB-01, SB-02, SB-243, SB-91, SB-92 |
| 20 Usage counts, logs, digests, crashes, health | partial | P2 | SB-385, SB-393, SB-379 |
| 21 Release pipeline and nightly | done (flaky tests) | P3 | SB-350, SB-354, SB-361, SB-362, SB-234, SB-410 |
| 22 Brand, copy, locales, UX scenarios | partial | P3 | none yet |

Counts: done 5 (9, 11, 12, 13, 21; each with a minor gap noted), partial 17, broken 0, missing 0 as whole
capabilities (devices of an organization is missing inside 7, the pool queue inside 5, the off time
inside 3).
