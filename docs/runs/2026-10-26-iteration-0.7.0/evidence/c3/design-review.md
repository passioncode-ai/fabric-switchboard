# C-3 design review: fallback chains in the window (SCN-080, SCN-081, SCN-082)

Run 2026-10-27 by the C-3 builder, following the task-pipeline design-tester procedure itself
(pass B only: SCR-03 has no Figma frame). Browser: Playwright WebKit from
`/Users/sshlg/DATA/_worktrees/pw`, over the Vite demo of branch `w4/c3`
(`?demo=1`, and `&chains=none|reasons|failed|stale|skipped|launch_failed`). Viewport 740 and
1100 px wide, 900 high, light and dark, English and Russian. Captures are element shots of the
panel, the dialog or the group card. The Vite server and every browser were closed after the run.

## What was checked, with the capture

| Step | Capture | Seen |
|---|---|---|
| SCN-080 1: nothing set | `none-{en-light-1100,ru-dark-740}.png` | lead line; *This computer* “Not set” with *Use Subscriptions first* (the page's one primary) and *Edit…*; each group “Uses this computer's chain, which is not set”; “No hand-over yet.” |
| SCN-080 2, SCN-082 1-2: populated | `panel-{en,ru}-{light,dark}-{740,1100}.png` | this computer, two groups (own chain; this computer's), one task row; chips in order with arrows between them, a pinned account or key in muted text; one readiness line per step; last hand-over and the steps passed over |
| SCN-082 1: every readiness line | `reasons-{en-light-1100,ru-dark-740}.png` | ready on an account, needs a sign-in, no account with room, today's ceiling reached, less than $0.50 left, key pin, not automatic yet, not installed |
| SCN-082 2: last hand-over | `skipped-*.png`, `launch_failed-*.png` | “today's ceiling reached” with three passed-over steps; “the launch failed after the offer” |
| SCN-080 3: editor | `editor-{en,ru}-{light,dark}-{740,1100}.png` | numbered steps, agent and account selects, *Move up* / *Move down* / *Remove* under them, *+ Add agent*, *Save chain* primary |
| SCN-080 alt: eight steps | `editor-eight-{en-light-1100,ru-dark-740}.png` | *+ Add agent* disabled, “A chain holds at most 8 agents.”, the key pin read-only, Aider and Goose “(not automatic)”, Hermes and OpenCode “The OpenRouter key” |
| SCN-080 alt: empty save | `editor-empty-save-*.png` | “Add an agent, or clear the chain instead.”, nothing saved |
| SCN-080 5, SCN-081 3: clear | `clear-{en-light-1100,ru-dark-740}.png` | question, consequence, *Clear chain* in the danger style |
| SCN-080 errors: read failed | `failed-*.png` | “Chains are unavailable. Refresh to retry.” with *Retry*, no rows |
| SCN-080 errors: refresh failed | `stale-*.png` | the last rows stay, with “The list may be out of date: the last read failed.” |
| SCN-081: Groups line | `groups-{en-light-1100,ru-dark-740}.png` | “Fallback: own chain” as an underlined link; it opens Agents with focus on that group's *Edit…* |

Measured on every capture (38 checks): horizontal page overflow 0 px; no element of the panel or
the dialog outside its box; no clipped element (the Groups page's visually hidden heading
excepted, by design); no console or page error. Contrast against its background, lowest over both
themes: chip text 17.18, pinned account 6.73, readiness text 6.21, agent name 15.96, notes 6.21,
step number 6.21, Groups link 6.21; all above 4.5. The disabled account select of Hermes and
OpenCode is dimmed as a disabled control (WCAG 1.4.3 exempts it).

Keyboard (WebKit, Alt+Tab): in the editor the order is agent, account, *Move up*, *Move down*,
*Remove* per step, then *+ Add agent*; a disabled move or account select is skipped. *Enter* on
step 2's *Move up* gives `codex, claude-code, kimi-code, hermes` and leaves focus on step 1's
*Move down*; *Enter* again restores the order with focus on step 2's *Move down*. *Escape* closes
the dialog and focus returns to *Edit…*. Saving a reordered chain shows “This computer's chain is
saved.” and the row reads `Codex CLI → Claude Code → Kimi Code CLI → Hermes Agent`.

## Findings

1. **Fixed in this change.** In the first build the moves sat beside the selects; at the dialog's
   width the selects cut their text (“Any account w…”, “The OpenRoute…”). The moves now sit under
   the selects, at both widths.
2. **Fixed in this change.** “{agent} · Not automatic yet” in the agent select was cut in Russian;
   it reads “{agent} (not automatic)” («(не автоматически)») now.
3. **Fixed in this change.** The arrow between chips sat inside the next chip; it sits between
   them now.
4. **Fixed in this change.** At 740 px the readiness lines stacked the agent above its reason and
   doubled the panel's height; they stay on one line, the name in a fixed column.
5. **Fixed in this change.** WebKit does not focus a button on click, so the dialog returned focus
   to the row, not to *Edit…*; the dialogs now return focus to the row's *Edit…* (a task row's
   *Clear* returns to this computer's *Edit…*, since the row goes). The Groups line lands on the
   group's *Edit…* or *Set for this group…*.
6. **Fixed in this change.** The Groups line looked like plain text; it is underlined as a link.
7. **Not blocking, proposed board row.** A paid step shows “Spend not read yet.” always: the chains
   view carries `spend: null` (C-2) and reads no spend, so the “${remaining} left (read {age} ago)”
   of the first draft of SCN-082 cannot be shown until the view carries the last read.
8. **Demo only.** The demo's readiness does not apply a group's account rules, so “Alpha Web”
   reads ready on “Engineering”, an account of another group; the real view does (C-2,
   `chain_view::preview` reads the group's first folder).
