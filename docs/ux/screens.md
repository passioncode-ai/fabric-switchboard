Contract: ux-contract v4
# Screens
**Web surfaces:** no — local authenticated desktop workbench; browser demo contains only fixtures.
Design system: PassionCode v1.0.0, vendored from the [immutable canonical source](../../brand/passioncode/manifest.json); native semantic HTML controls in Tauri. Product-specific states are our decisions, not claimed as kit validation. No cinematic motion. Variance 2 (consistent rows), motion 1 (focus/hover), density 6 (compact account rows and current identity visible before account actions; retained workbench hierarchy).
Design pass 2026-10-22 (SB-164): one card width (980 px, prose 72ch), a spacing rule after every lead-in note, state on a tinted surface instead of a side stripe (marker V005), statistics tables that scroll sideways on a narrow window, and Kimi rows on the same narrow grid as other accounts. Decisions, the critique and what stays open: [director record](../design/switchboard/director-record.md).
Falsifier: user cannot tell which account handles the next request, or cannot recover from a failed addition.
## SCR-01 — Accounts
Status: designed
States: empty, loading, error, populated, add dialog, login dialog, removal confirmation.
Elements (since 0.6.82): one notices block (provider terms, active folder rules), In use now with proxy status, the automatic switching bar, the organization filter when organizations exist, accounts grouped by provider and then by group (“Group · {name}”, “Default group”, “Group {id} · no folders”), per-account quota age, one primary action per row and a ⋯ menu. The provider filter of 0.1 is gone. Secret inputs only in addition.
Scenarios: SCN-001 through SCN-010, SCN-012, SCN-013.
Coverage: [verification](../evidence/verification.md); browser inspected, historical 0.1 state; native 0.2 inspection is recorded in [0.2 evidence](../evidence/release-0.2.md).
Figma: not used
## SCR-02 — Activity
Status: designed
States: empty, populated, error.
Elements: timestamp, operation, account UUID, bounded detail, refresh.
Scenarios: SCN-011
Coverage: [verification](../evidence/verification.md); browser inspected, historical 0.1 state; native 0.2 inspection is recorded in [0.2 evidence](../evidence/release-0.2.md).
Figma: not used

## SCR-01 extension — current accounts and rotation (0.3)
Scenarios: SCN-018 through SCN-022. Current CLI identity panel, capture-first add dialog, explicit official sign-in alternative, Claude Swap import result, quota window disclosure, rotation policy editor/status, and native Claude activation confirmation. Existing semantic controls retained; PassionCode tokens applied in 0.3.1. Integrated browser demo inspected with synthetic profiles; native/live 0.3 acceptance is separately declared in [release evidence](../evidence/release-0.3.md).

## Shared chrome and identity (0.3.1)
SCN-023 covers the fixed dark register, yellow S icon, Switchboard / by PassionCode sidebar, and separate information-blue current-CLI versus gold next-request status. Component structure and account behavior remain unchanged. Native icons are generated from the same canonical SVG via `node scripts/generate-icons.mjs`. Native controls remain the existing component layer; no kit migration or animation added. Browser rendering evidence and limits: [0.3.1 design evidence](../evidence/design-0.3.1.md).

## Startup and import recovery (0.3.2)
SCN-024: primary metadata loads independently of external credential observation; bounded native reads expose Retry. SCN-019: Import Claude Swap remains on Accounts and is also directly reachable from Add account. No visual theme changes.

## SCR-01 revision — compact grouped accounts (0.5)
Scenarios: SCN-003, SCN-018…SCN-020, SCN-022, SCN-028, SCN-029. Accounts are grouped by provider (Claude Code, Codex CLI) with pool sub-groups when a provider has more than one; each account is one ~48 px row: provider mark, label with badges (In Claude Code, Next managed request, Sign in again, Disabled), email or credential type, a quota cell (highest window %, reset or staleness, 3 px meter; click expands windows), one primary action (Switch / ✓ In use for Claude OAuth, Select / ✓ Selected otherwise, Sign in, Enable) and a ⋯ menu for the rest. The provider filter, the per-card action rows, the capture/import/activation dialogs and the Finish sign-in step are gone; + Add account is a menu, In use now offers Add to Switchboard, and a sign-in banner replaces the login dialog. Density 8. Inspected in the browser demo at 1280×720 and 740×560, light and dark: [design 0.5 evidence](../evidence/design-0.5.md).

### Quota order and countdown — 2026-10-04
SCN-021, SCN-032. Within provider/pool groups, fresh remaining quota precedes known waits, unknown data, sign-in and disabled rows. The quota cell is two lines (2026-10-05): meter, share used and check age; then the countdown and date of the next reset or end of wait, or the failed/stale/sign-in text. A coloured status mark on the provider icon carries the card state; the screen-reader label states it in words. Expanded windows show individual waits. Retry holds have their own label. Existing token/component system; no animation. Pause/resume stops automatic timer text updates; ticks only update text, without changing focus. [Verification](../evidence/quota-order-2026-10-04.md).

### Provider terms notice — 2026-10-08
SCN-049. Accounts opens with a neutral banner (panel-2 background, border, the text and a *Dismiss* text button) above the project-rules strip, in every state Accounts renders, including the empty first-account state; it stays until dismissed (`switchboard.terms-notice` in window storage). About carries a *Provider terms* panel with the same text, right after *A local account workbench*. Existing tokens and components; no animation. Checked in the browser demo 2026-10-08 in English and Russian.

## Window behaviour and words (0.6.82, node N5)
Scenarios: SCN-001, 002, 010, 011, 013, 022, 024, 025, 030, 034, 035, 042, 043, 045, 046, 047, 050, 055, 059, 062 to 068, 071.
- Words (D-1, D-2, D-5): the person reads **group** and **automatic switching**; a pool id shows only as a group's id (*Group id* field, “Group {id} · no folders”). The navigation page of keys is **API keys** («API-ключи»); Statistics keeps “tokens” for counted usage. Folder rules are **folder rules**.
- One primary action per screen: while Accounts, Groups or Organizations shows its empty state, the header has no second primary button (VIS-21, UX-01).
- Accounts: the provider terms and active folder rules share one notices block (VIS-13; the first account row moved up 52 px at 740 px and 57 px at 1100 px in the demo). In use now keeps a failed read as “Could not read the sign-in” with *Retry* (UX-04).
- Results of actions are a notice fixed at the bottom of the window (UX-07); a failed re-read after a successful action keeps the last list under a line with *Retry* (UX-06).
- Dialogs open on their first field; a dialog scrolls as one, its actions stay at its bottom; commands break between words where they can (VIS-15, VIS-16). A ⋯ menu returns focus to its trigger after a dialog and sets destructive items under a separator (UX-05, VIS-27).
- Sidebar icons are one inline SVG set on a 16 px grid with a 1.5 px stroke (VIS-19).
- The tour card sits beside its target, never over it (VIS-12).
- New classes live in `src/window.css`; `src/style.css` and `src/tokens.css` are the visual system's.
Evidence: screenshots in `~/DATA/_worktrees/sb-iter-shots/n5/` (local, not in Git), checks listed in `docs/runs/2026-10-24-iteration-ui-ux/nodes/N5.json`.

## SCR-03: Agents, Fallback chains panel and its editor (0.7.0)
Status: designed
Used by: FLW-05
Purpose: see which agents take over a task when its accounts run out, in what order and whether each could take it now, and set that order for this computer, a group or a task (SB-71, SB-73 status, decision D-5 of 2026-10-25).
States: reading; not set (no chain anywhere: lead line, *This computer* “Not set”, *Use Subscriptions first* as the panel's one primary action); populated (machine row, group rows, task rows, each with its readiness lines); last hand-over (offered, failed, every step skipped, none yet); ceiling question banner (two buttons of equal weight, no primary, SCN-084); editor dialog (default, editing, eight steps reached, a step not automatic, a key pin read-only, saving, refused with the core's text, save failed); clear confirmation; read failed (last known rows kept and marked as possibly out of date, *Retry*).
Elements: the panel sits on Agents between *Installed* and *Available to install*; lead line; rows *This computer*, one per group, one per task chain (no add form for tasks); per row the chain as chips in order, *Use Subscriptions first* (machine row only), *Set for this group…* / *Edit…*, *Clear*; one readiness line per step under each row (SCN-082); “Last hand-over: …” under the panel; the editor dialog titled by scope (“Fallback chain: This computer”, “Fallback chain: {group}”), step rows with an agent select (the five agents that can take over work), an account select filtered by the core's rules, *Move up*, *Move down*, *Remove*, *+ Add agent* (until 8), *Save* (primary), *Cancel*; every control a button with an accessible name, no drag-only control. On Groups, each card's *Fallback* line links to its row here (no second editor).
Axes: viewport: 740, 1100; theme: light, dark; text: default; locale: en, ru.
Scenarios: SCN-080, SCN-081, SCN-082, SCN-084; SCN-039 and SCN-040 name it.
Coverage: none yet (nodes C-2, C-3, C-7); design review in WebKit on the demo, both themes, English and Russian, is wave 4's gate.
Resources: `Operation::Chains` (`readiness`, `last_handover`, additive), Tauri `chains`, `set_chain`; the error vocabulary in `src/adapter.ts`; the Activity journal's “Fallback chain” and “Hand-over” entries. Strings are draft copy until copywriting and Copylot (en-US, ru-RU).
Figma: not used

## Additions in 0.7.0 to pages without a screen record (designed, not built)
Groups, Organizations and API keys have no `SCR` id of their own yet (a gap for `/ux-update`, not closed here); their 0.7.0 elements are recorded so the build and the design review have one list. All strings are draft copy.
- **Groups → *Terminals in organization folders*** (SCN-075), under *Codex in group folders*: explanation, state line, one line per start file with its state (added, link skipped and ZDOTDIR skipped with the `switchboard shell init <shell>` line and *Copy*, owner skipped, unreadable, removed by the person with *Add the block again* and the line “Adding the block again saves your choice to keep the hook on, as Turn on does.”, policy blocked on Windows, not yet verified on Windows until SB-02), *Turn off* / *Turn on* (disabled with “Only the installed app changes terminal settings.” outside the installed app). States: reading, on by default, default with no organization, on, off, on but inactive, folder map unreadable, development build, read failed (*Retry*). Built in node A-7 (`shellHookSection` in `src/main.ts`, `shellHookView` in `src/ui-logic.ts`); demo `?demo=1&shell=<state>`. *Codex in group folders* gains one line for organization folders (SCN-066). Each group card gains its *Fallback* line (SCN-081) and, when every organization seat in it ended, “None of {group}'s accounts is signed in…” (SCN-078).
- **Accounts (SCR-01)**: the organization card “Set up {organization} on this computer” above the page after the usage question and the tour (SCN-076: five step lines with their state, one action, *Hide*; states waiting, each step, hidden, several organizations, complete, read failed); the empty state on an organization computer leads with *Sign in with your {organization} address* (SCN-076 step 7); an ended seat's card line with *Sign in again* (SCN-078); the sign-in notice's *Move it to {group}* (SCN-077).
- **Organizations**: the same organization card at the top (also after *Hide*); the released line “Not in your PassionCode.ai organizations since {date}…” in place of the *PassionCode.ai* mark (SCN-079).
- **API keys**: on the machine key panel and each group key row, “Daily ceiling: …” with *Set ceiling…* / *Clear*, the key's own limit and its reset, the warning when that limit is monthly or higher than the group's ceiling, the inactive line after a group key's removal, read-only task ceiling rows, the ceiling dialog (amount, explanation, *Save*, *Cancel*) and the clear confirmation (SCN-083); the ceiling question banner (SCN-084).
- **Activity → Sessions (SCR-02 area)**: rows for Kimi Code, Hermes Agent, OpenCode, OpenRouter agent launches and proxy agent launches with their context badge (SCN-051).
- **Notifications** (native, not a page): the first terminal hook write (SCN-074; the window also shows it once as a notice with *Open Groups*), a hand-over offered (SCN-082), a ceiling question opened (SCN-084).
