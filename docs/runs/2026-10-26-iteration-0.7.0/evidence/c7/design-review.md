# C-7 design review: spending ceilings in the window (SCN-083, SCN-084)

Run 2026-10-27 by the C-7 builder, following the task-pipeline design-tester procedure itself
(pass B only: the API keys page has no Figma frame). Browser: Playwright WebKit from
`/Users/sshlg/DATA/_worktrees/pw` (script `c7.js`, kept outside the repository), over the Vite
demo of branch `w4/c7` (`?demo=1&ceilings=set|reached|failed`, and no parameter). Viewports 740
and 1100 px wide, 900 high, light and dark, English and Russian: 36 captures; the 18 kept here are
each state in English light 1100 and Russian dark 740, scaled to 700 px wide. Captures are the
page's `main` (API keys), the top of the page down to the last banner (Agents) or the viewport
(dialogs). The Vite server and every browser this run started were closed after it.

## What was checked, with the capture

| Step | Capture | Seen |
|---|---|---|
| SCN-083 1, 3: ceilings set | `set-*.png` | machine panel: credit line with the key's own limit, then below a rule “Daily ceiling: $10.00”, “Counts every OpenRouter key…”, *Set ceiling…*, *Clear*; Alpha Web row “Daily ceiling: $3.00”, “Key's own limit: $20.00, resets monthly”, the warning; Tools row “A daily ceiling needs the group's own key.”; *Task ceilings* with task wf_01234567, $2.00, *Clear* |
| SCN-083 alt: monthly key limit | `set-*.png` (Alpha Web) | “The key's own limit resets monthly, not daily. A running agent can spend up to the key's own limit; …” with a warn-colour rule |
| SCN-084 2: question on API keys | `reached-keys-*.png` | two banners, oldest first (this computer $10.00, Alpha Web $3.00), each with *Double to $20.00* / *Keep $10.00* side by side |
| SCN-084 2: question on Agents | `reached-agents-*.png` | the same banners above the default agent panel |
| SCN-084 4: keep | `answered-*.png` | “This computer's daily ceiling stays $10.00. Paid agents wait until it resets.”; its banner gone, the other stays |
| SCN-084 3: double | `doubled-*.png` | “This computer's daily ceiling is now $20.00.”; the line reads “Daily ceiling: $20.00, doubled from $10.00” |
| SCN-083 2: dialog | `dialog-*.png` | “Daily ceiling for this computer”, the explanation, *Dollars per UTC day* prefilled 10.00, range help, *Cancel* and *Save ceiling* (primary) |
| SCN-083 6: clear | `clear-*.png` | “Remove the daily ceiling of Alpha Web, $3.00?”, consequence, *Remove ceiling* in the danger style |
| SCN-083/084 errors: read failed | `failed-*.png` | the core's sentence (delete spend.json, then restore a backup) with *Retry*, on the page's top |
| no key, no ceiling | `none-*.png` | the page as before C-7: no ceiling block without a saved key |

## The two answers (SCN-084, C-Q1)

Measured on every capture with a question (24): both answers carry the one class
`button ceiling-answer`, the same computed text colour, background, border and weight (550), the
same height (36 px) and, inside one banner, the same width (139 and 139 px at 1100 English, 148
and 148 in Russian, 249 and 249 at 740 where they stack); neither has `primary`, neither carries a
`recommended` or preselected state, and focus does not land on either when the page opens. The
order is fixed, *Double* then *Keep*, as the scenario lists them. Keyboard (Alt+Tab from the page
title, WebKit): *Refresh*, then *Double* and *Keep* of the first question, then of the second, then
the machine panel's actions.

Measured on all 36 captures: horizontal page overflow 0 px; no clipped element in the banners,
ceiling blocks or dialogs; no console or page error. Contrast, lowest over both themes: answer
text 15.96, banner lead 14.77, banner detail 5.84, ceiling line 15.96, warning 15.96, notes 6.21;
all above 4.5.

## Findings

1. **Fixed in this change.** The machine ceiling first sat between the key's facts and its
   actions, so two rows of buttons ran together; it now sits under the actions, set apart by a rule.
2. **Fixed in this change.** A group row's ceiling block stood far from its row (flex gap plus a
   margin); the row now wraps with a small gap.
3. **Fixed in this change.** The read-failed banner said “could not be read” twice in Russian (the
   lead plus the core's sentence); the core's sentence for an unreadable file now stands alone.
4. **Not a defect.** In `answered-*` and `doubled-*` the remaining banner's first button shows the
   hover colour: after the click the next banner moved up under the pointer. With the pointer away
   both answers read the same (every `reached-*` capture).
5. **Not blocking, proposed board row.** The hand-over notification of SCN-082 step 3 (“{agent}
   took over a task in {folder}.”) is not posted: spend.json's `last_handover` carries neither
   the agent nor the folder, and the store event `fallback` carries only offered/failed. It needs
   C-5's record to name the agent and folder.
6. **Not blocking, proposed board row.** Russian money keeps the window's existing form (`$5.00`);
   Copylot suggests `5 $` (ru-currency-position, advisory). Changing it is a window-wide decision.
