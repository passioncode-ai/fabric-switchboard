# Wave 4 WebKit design pass (pass B): iteration 0.7.0 as one product

Run 2026-10-28 by the design tester over branch `w5/webkit-pass` (from `iteration/0.7.0` at
`a476db1`), following the task-pipeline design-tester procedure, pass B only: no screen in scope
has a Figma frame. Machine-readable report: [design-review.json](design-review.json)
(`design-review/1`, 26 rows, persona P-01 as the scenarios name it).

## Verdict

**No finding blocks the release.** Five should-fix findings (all `risk`) and three notes. Every
scoped scenario step reads in the scenario's words in English and Russian. Measured on all 344
captures: horizontal overflow 0 px, no element outside the viewport, no clipped text (the only
hits are `sr-only` headings, which is expected), no console or page error.

**The gate is NOT_RUN, not PASS.** `design_gate.py report design-review.json --class product
--panel docs/ux/test-panel.md --scope SCN-074,...,SCN-084,SCN-037,SCN-066 --require build` exits 3:
“coverage cannot be computed without a test panel with portraits; the gate does not pass on it”.
`design_gate.py panel docs/ux/test-panel.md` exits 3 too: the file does not exist, and super-ux
`ux-foundation` builds it. The rows themselves raise no gate problem (the same report with
`--class internal`, a structural check only, exits 0). Every judgement below rests on the
scenarios, not on panel portraits.

## How it ran

- **Browser:** Playwright 1.63.0 WebKit r2359 from `/Users/sshlg/DATA/_worktrees/pw` (cached, nothing
  downloaded), headless, one browser and one page at a time.
- **Build:** the Vite demo of this worktree (`vite --host 127.0.0.1 --port 1477`, the `dev`
  script on a free port), `node_modules` symlinked from the main checkout and removed after.
- **Matrix:** 43 states × 740 and 1100 px wide (900 high) × light and dark × English and Russian
  = 344 full-page captures. Then section crops at four of those variants, keyboard walks (Alt+Tab)
  in English light 1100, and six actions driven in two variants. Scripts stayed outside the
  repository.
- **States:** `?demo=1` with `org=setup` (plus `group=none`, `backup=1`, `onboard=degraded|failed`,
  and *Hide*), `org=ended`, `shell=` each of the 12 values, `chains=` none, reasons, failed, stale,
  skipped, launch_failed, the chain editor, `ceilings=set|reached|failed`, the ceiling dialog and
  the group key removal.
- **About's paused update line (SCN-037 step 4):** the demo has no parameter for it. For
  `update=paused` only, the demo's update object was rewritten in flight to `ready`, version 0.7.1,
  `restart_after` four hours ahead. This is a test fixture; the repository is unchanged.
- **Machine:** `resource_guard.py check` said wait (load 22 to 64 over 14 cores, mostly
  fseventsd and other sessions). The run waited about 17 minutes, then ran with one page as the
  guard's rule allows. Receipt (`resource_guard.py verify --json`): six processes recorded (the
  Vite server and five WebKit runs), `live: []`, `closed: true`.

## Findings

Each finding names its PNG in this folder, its severity, and where the cause is visible.

### Should fix

1. **A left organization is offered for setup again** (SCN-079 step 1, SCN-076).
   Contoso, which the person left on PassionCode.ai, still gets “Set up Contoso on this computer”.
   Its primary action is *+ Group for Contoso*, or *Sign in with your Contoso address* once its
   seats ended. On Accounts the card leads the page and says nothing about leaving. On
   Organizations two cards push the released line (“Not in your PassionCode.ai organizations
   since …”) below the fold. This conflicts with SCN-079's “leaving is visible … and costs nothing
   until the person removes it”.
   - Cause: `orgSetups` keeps released organizations (`src/ui-logic.ts:1205`), and the checklist
     lists every organization (`crates/switchboard-runtime/src/onboarding.rs:370`).
   - Proposal: a released organization gets no card, or a card that says it was left. Choosing
     between them is a scenario decision for super-ux.
   - PNGs: `f1-released-org-card-orgs-ru-light-1100.png`, `f1-released-org-card-accounts-en-light-740.png`.
2. **Three filled primaries on an organization computer's Accounts** (SCN-076 steps 1 and 7).
   The seat sign-in (the card's, or the empty page's after *Hide*) sits beside *Add to Switchboard*
   on each ordinary CLI signed in to an unsaved account, and *Turn on for Claude Code* once
   accounts exist. Step 7 says the seat sign-in is the page's one primary. B-8 filed this as
   “demo only”, but the installed app shows the same buttons whenever the ordinary Claude Code or
   Codex is signed in to an account Switchboard has not saved. That is likely on a new employee's
   computer.
   - Cause: `src/main.ts:2046` and `src/main.ts:2078` (`button primary row-button`), with no
     check of `orgSetupLeads`.
   - PNG: `f2-three-primaries-accounts-en-light-1100.png`.
3. **Two organizations, two primaries** (SCN-076 alt “several organizations”, SB-228 VIS-21).
   Each card's action is primary, so Organizations shows two. Accounts does too in a fresh window;
   in this run's Accounts capture Northwind was hidden by an earlier *Hide* in the same browser
   context.
   - Cause: `src/main.ts:143` inside the loop at `src/main.ts:406`.
   - Proposal: only the first shown card's action is primary.
   - PNG: `f1-released-org-card-orgs-ru-light-1100.png`.
4. **The terminal hook's state does not stand apart** (SCN-075 step 1 and its alt states).
   The state line uses the muted `account-meta` style, with no space after the explanation, so
   “On, but no start file holds the hook yet, so terminals do not follow organization folders.”
   reads as the paragraph's last sentence. A start file the hook could not use (a link, another
   user's, the block removed, PowerShell policy, Constrained Language Mode) looks exactly like
   “zsh: ~/.zshrc, added”. The *Codex in group folders* section above has the same layout and
   predates wave 4.
   - Cause: `src/main.ts:507`; `src/main.ts:513` adds `.shell-hook-problem`, but
     `src/style.css:584-586` has no rule for it.
   - PNGs: `f3-hook-inactive-en-light-1100.png`, `f3-hook-state-and-problem-ru-dark-740.png`.

### Notes

5. **Task ceilings name a command as prose.** The line names `switchboard continue --ceiling <usd>`
   as plain text, while the organization card sets `switchboard shell off` as code (SB-228).
   Cause: `src/main.ts:1273`; the machine key's CLI line has the same habit
   (`src/main.ts:1331`, older). PNG: `ref-apikeys-ceilings-en-light-1100.png`.
6. **The card's ended-seat line has no way out.** With a seat ended, the card's *Your seat* line
   says the accounts “must sign in again” and leads with sign-in. The account card adds “If
   Northwind removed your seat, ask its administrator”. The card should carry that way out too.
   PNG: `f1-released-org-card-accounts-en-light-740.png`.
7. **Older primaries beside wave 4's.** These are not wave 4's own:
   - Groups' *+ New group* and Codex *Turn on* (`src/main.ts:391`, `src/main.ts:481`);
   - Agents' *Use Subscriptions first* (`src/main.ts:1006`) with Connect's *Link switchboard into
     ~/.local/bin* below the fold (`src/main.ts:1115`).
   They belong with finding 2 in one window-wide decision on primaries.
8. **Out of scope, low contrast.** A folder path line on the Groups card measures 3.1:1 in the
   light theme (SB-135's folder list, older than this iteration). It is a board row, not part of
   this pass.

Russian money keeps the window's `$10.00` form (Copylot's `5 $` advice stays a window-wide
decision, as C-7 recorded). «Нужны вы» for *Needs you* fits and passed Copylot, but reads
literal. Both stay as they are.

## What passed, by screen

- **Accounts:**
  - The organization card shows five steps with Done, Needs you and Optional chips, one action,
    and *Hide*.
  - `switchboard shell off` is set as code.
  - Degraded state: one caveat, the widest read first. Failed state: the title and the note only.
  - *Restore from backup* is quiet beside the card's action.
  - Ended seats carry the organization's line, with no generic duplicate.
  - SCN-074's notice uses the scenario's words, with *Open Groups*.
- **Groups:**
  - All 12 hook states read in the scenario's words in both languages.
  - *Copy* sits beside every `init` line, and *Add the block again* has its note.
  - *Turn off* and *Turn on* show the scenario's notices as toasts, and focus stays on the toggle.
  - The chain line links to Agents and focuses the group's *Set for this group…*.
  - The ended-seat group line uses the warning colour.
  - Codex's organization line is present.
- **Organizations:** the released line keeps its date, and the *PassionCode.ai* mark is gone from
  Contoso.
- **Agents:**
  - The chain panel shows the lead line, the rows, and every readiness reason.
  - Last hand-over and *Passed over* lines are present; the failed read keeps its *Retry*.
  - The preset shows the notice with focus on *Edit…*.
  - The editor: ends disabled, the key pin read-only, *(not automatic)* agents, *Save chain* the
    dialog's one primary, focus on step 1's agent.
  - *Clear* asks first, with focus on *Cancel*.
  - The ceiling banners come first, oldest first.
- **API keys:**
  - Ceiling lines, the key's own limit and the monthly warning are present, and so is “A daily
    ceiling needs the group's own key.”
  - Task ceilings are read-only.
  - The ceiling dialog has focus in the amount.
  - The key removal shows SB-230's sentence with focus on *Cancel*; the read-failed banner is shown.
  - *Double* and *Keep* match in class and weight, neither is primary or focused, and *Double*
    gives its notice with focus on the page title.
- **About:** the paused update line names its time, with *Restart to update*.
- **Keyboard and focus:** on every walked screen the order follows the page (title, *Refresh*,
  the card or banner actions, then the rest), and every stop shows a focus ring.
- **Contrast:** the lowest values for wave 4 elements are 5.64 (state chips), 5.68 (failed-read
  notices), 5.84 (banner detail), and 6.21 or more for the rest. All are above 4.5.
- **Motion:** the new elements add none (only the window's existing button hover transition).
- **Cross-screen words:** the same names are used across screens:
  - *Fallback chains* / «Цепочки агентов» on Agents and «Цепочка агентов: …» on Groups;
  - *daily ceiling* / «дневной потолок» on the banner, the panel and the dialog;
  - one money format (`$0.50`, `$10.00`, `$1,000`) in readiness, banners and ceiling lines.
- **Button weights:**
  - a bordered button for each row's main act;
  - text buttons for secondary acts;
  - danger text for *Clear* and *Remove*;
  - a danger fill only in confirmation dialogs.

## Captures kept

The finding PNGs are listed above. One reference per screen:

- `ref-accounts-org-card-ru-dark-740.png`
- `ref-groups-ended-chain-hook-en-light-740.png`
- `ref-agents-chains-reasons-ru-dark-740.png`
- `ref-agents-editor-ru-dark-740.png`
- `ref-apikeys-ceilings-en-light-1100.png`
- `ref-apikeys-question-ru-dark-740.png`
- `ref-about-update-paused-ru-dark-740.png`

All are scaled to at most 700 px wide.

## Not run

- The installed app, a live sign-in, a real PassionCode.ai read and the system notifications
  (the demo has none).
- The test panel's personas: the file is absent, which makes the gate NOT_RUN.
- A Figma comparison (pass D): no frame exists for these screens.

## After the fixes

Run 2026-10-28 on branch `w5/ui-final` with the window changes of commits `b0f4421`, `72009cb`,
`3e99349` and `20a77fb` (`dbf7842` is the CLI and MCP half of finding 1 and changes no screen). Same browser (Playwright 1.63.0 WebKit r2359 from `/Users/sshlg/DATA/_worktrees/pw`,
nothing downloaded, headless, one browser and one page at a time), the Vite demo of this worktree
on `127.0.0.1:1478`, closed after. 40 captures for the findings (5 states × 740 and 1100 px wide,
900 high × light and dark × English and Russian) and 5 single references, each named
`after-<state>-<lang>-<theme>-<width>.png`, scaled to 700 px wide and reduced to 256 colours.
Measured on all 45: horizontal overflow 0 px, no console or page error, and at most one visible
filled (`.primary`) button per screen.

| Finding | Result | Captures |
|---|---|---|
| 1. A left organization is offered for setup again | **Fixed.** Contoso (left on PassionCode.ai) gets no card on Accounts or Organizations (`?demo=1&org=ended`: one card, Northwind's). With Northwind's card still on top, Contoso's own released line started at 883 px (1100) and 1191 px (740) in a first run of these captures, so a line above the card now names the organizations left; it starts at 232 px (1100) and 282 px (740), inside a 900 px window in all eight variants. The CLI and MCP checklist leave it out too and name it under `organization.left`. | `after-f1-released-org-orgs-*` (8), `after-f1-released-org-accounts-*` (8) |
| 2. Three filled primaries on an organization computer's Accounts | **Fixed.** `?demo=1&org=setup`: the card's *Sign in with your Northwind address* is the one filled button; both *Add to Switchboard* buttons are bordered. After *Hide*, the empty page's seat sign-in is the one filled button. | `after-f2-org-lead-accounts-*` (8), `after-f2-after-hide-accounts-en-light-1100.png` |
| 3. Two organizations, two primaries | **Fixed.** `?demo=1&org=setup&second=1` (Northwind and Fabrikam): only Northwind's *Sign in with your Northwind address* is filled; *+ Group for Fabrikam* is bordered, on Organizations and on Accounts. | `after-f2-two-orgs-orgs-*` (8), `after-f2-two-orgs-accounts-en-light-1100.png` |
| 4. The terminal hook's state does not stand apart | **Fixed.** `?demo=1&org=setup&shell=inactive`: the state line of *Terminals in organization folders* and of *Codex in group folders* is body text (14 px, ink colour) 12 px below its explanation; each start file the hook could not use has a 3 px warning edge (`--warn`: rgb(138, 75, 11) light, rgb(246, 186, 117) dark) with ink text. | `after-f3-hook-problem-groups-*` (8), `after-n7-groups-en-light-1100.png` |
| 5. Task ceilings name a command as prose | **Fixed.** “Set from the command line:” then `switchboard continue --ceiling <usd>` as code. The machine key's older CLI line is unchanged. | `after-n5-task-ceilings-apikeys-en-light-1100.png` |
| 6. The card's ended-seat line has no way out | **Fixed.** “Your Northwind accounts in “Northwind web” must sign in again. If Northwind removed your seat, ask its administrator.” | `after-f1-released-org-orgs-*`, `after-f1-released-org-accounts-*` |
| 7. Older primaries beside wave 4's | **Fixed.** Groups' Codex *Turn on* is bordered beside *+ New group*; Agents' *Link switchboard into ~/.local/bin* is filled only while *Use Subscriptions first* is not offered above it (`?demo=1&chains=none`: one filled button). | `after-n7-groups-en-light-1100.png`, `after-n7-agents-en-light-1100.png` |
| 8. Low contrast of a folder path (out of scope) | Not touched; still a board row. | none |

New strings passed Copylot `copylot_check` (register ui, voice “PassionCode.ai site voice v2”, no
findings): the ended-seat line and “Set from the command line:”, en-US
`fa1dcb8d-162f-4189-9129-8588dcd4e566`, ru-RU `69026e86-2806-421d-9717-87b7027bd70a`; the line
naming left organizations, en-US `52eda78b-f78b-41ac-910f-a7eaa89cf3c7`, ru-RU
`fb4d783a-fec7-4f95-8333-db1300a4fa61`; the checklist's English summaries, en-US
`247113cd-970b-499c-9056-05bf4aff7355`.

Still not run: the installed app, the test panel's personas (the gate stays NOT_RUN), and the
Tauri `onboard` command's own filling of `released` (the window filters instead).
