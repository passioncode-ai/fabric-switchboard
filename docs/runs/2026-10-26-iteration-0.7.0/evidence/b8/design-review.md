# B-8 design review: the first run of an organization machine (SCN-076)

Run 2026-10-27 by the B-8 builder, following the task-pipeline design-tester procedure itself
(pass B only: there is no Figma frame for this card). Browser: Playwright WebKit from
`/Users/sshlg/DATA/_worktrees/pw`, over the Vite demo of this branch (`?demo=1&org=setup`,
`&group=none`, `&usage=ask`, and a start with the tour not yet seen). Widths 740 and 1100 px,
height 900, light and dark, English and Russian. Every process was closed after the run.

## What was checked, with the capture

| Step of SCN-076 | Capture | Seen |
|---|---|---|
| The card waits for the usage question | `waits-usage-en-light-1100.png` | the question is on screen, no card |
| The card waits for the tour | `waits-tour-en-light-1100.png` | the tour card is on screen, no organization card |
| 1, 3: the card after both, the seat to sign in | `setup-{en,ru}-{light,dark}-{740,1100}.png` | title, five step lines with *Done* / *Needs you* / *Optional*, *Sign in with your Northwind address*, *Hide* |
| 2: no group yet | `nogroup-en-light-1100.png`, `nogroup-ru-dark-740.png` | *+ Group for Northwind*; the empty page keeps today's first-account line |
| 2: the group dialog prefilled | `group-dialog-en-light-1100.png`, `group-dialog-ru-dark-740.png` | name “Northwind”, organization Northwind chosen |
| 7: the empty Accounts page leads with the seat | `setup-*.png` (below the card) | “Sign in with your Northwind address. It goes to the group “Northwind web”.” |
| Alt: *Hide* | `hidden-accounts-en-dark-740.png`, `hidden-orgs-en-dark-740.png` | gone from Accounts, still at the top of Organizations |
| Alt: no organization | `guard-no-org-en-light-1100.png` | no card, today's page |

Measured on every capture: horizontal overflow 0 px; no clipped element inside the card or the
empty state; no console or page error. Focus order (Alt+Tab, WebKit) from the page title:
*Refresh*, the card's action, *Hide*, then the provider-terms notice and the rest of the page, in
both languages. Contrast of the new elements against the card: state chips 5.64 to 6.21 (light),
8.60 to 8.77 (dark); the step lines 6.21 (light), 8.71 (dark); all above 4.5.

## Findings

1. **Fixed in this change.** The first build left the sign-in to the empty page below the card,
   so the card read *Needs you* with no button: at 900 px the page's line sits below the fold
   (first `setup-ru-dark-1100.png` of the run). Now the card keeps its action as the screen's one
   primary and the page's two buttons go quiet while the card is on screen (SCN-076 step 7
   amended).
2. **Fixed in this change.** With no group, the card's *+ Group for Northwind* and the empty
   page's *+ Add account* were both primary (VIS-21); the page's is quiet now.
3. **Not blocking, proposed board row** (the backlog is edited under its lease at integration). On Organizations the header's *+ New organization* (existing) and
   the card's action are both primary (`hidden-orgs-en-dark-740.png`).
4. **Not blocking, proposed board row** (the backlog is edited under its lease at integration). The *Terminals* line names `switchboard shell off` as plain text;
   a code style would set the command apart.
5. **Demo only.** The demo's *In use now* still shows its synthetic CLI accounts with
   *Add to Switchboard* on a computer the demo calls new; the real app reads the CLIs.
