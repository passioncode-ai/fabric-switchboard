---
surface: switchboard-desktop
surface_class: product
revision: 2026-10-24, branch iter/0.6.82-n3-visual (0.6.82, node N3); before it 2026-10-22, design/sb-164-screens (0.6.63)
---

# Director record: Fabric Switchboard desktop window

## Brief

Surface: product UI, a local desktop workbench (Tauri window, 740 to 1180+ px wide, light and dark).
Job: see which account every agent works on, switch or select one, and manage groups, organizations, keys and agents without reading a manual.
Constraint: the PassionCode v1.0.0 design system vendored in `brand/passioncode/` and native semantic HTML controls (`docs/ux/screens.md`); no cinematic motion; Russian and English.
Falsifier: at the 740 px minimum window, a row's controls overlap its text or a table hides a column with no way to reach it; or the same kind of content sits at different widths on different pages.
Scenario: SCN-001 (accounts), SCN-067 (statistics), SCN-068 (groups), SCN-069 (organizations).

## Mode

redesign: a detailed pass within the existing identity (operator 2026-10-22, «проработай над дизайном всех экранов детальнее»), entered at verify plus visual QA: 32 captures before (8 pages, light and dark, 1180 and 740 px, WebKit, demo data, Russian), the same 32 after.

## References

none found
Searched: no reference server was usable in this session (Refero and Mobbin need a sign-in that was not available); the pass corrects a shipped identity against its own pages rather than setting a new direction, so the comparison base is the before captures.

## Critique

- Accounts, Kimi Code row at 740 px → usage lines overlapped the Launch button and the ⋯ menu wrapped below the row → the Kimi usage cell takes the second row like every other account (820 px breakpoint).
- Accounts, "Ordinary kimi" line at 740 px → ran past the container's right edge → wraps.
- Accounts, usage sub-lines → cut with an ellipsis while space was free → wrap to two lines.
- Accounts, "Pause countdown" → a full button for a rare action between the overview and the list → a text button.
- Accounts and Groups, selected row and active rule card → a 3 px coloured stripe down the left side (V005) → state carried by a tinted surface and a 1 px accent border; the text already says Selected / In use / Active.
- Tokens and Groups, the lead-in note → touched the first card → one spacing rule after every lead-in note.
- Tokens, group key rows → prose set in the monospace face → the body face, name strong, state muted.
- Groups → the page title and the first section both read "Groups" → the section heading is kept for structure and hidden visually.
- Groups, Codex in group folders → a primary button on a switch with no Codex account to use → secondary until a group has one.
- Agents, Tokens, About → cards capped at 780 px under a header that spans the page while Accounts and Statistics run wider → one card width (980 px) with prose capped at 72ch.
- Statistics at 740 px → the date column wrapped into two lines and the last columns were cut → cells do not wrap, tables keep a minimum width and scroll sideways, narrow cell padding lets four-column tables fit.
- Interface copy → 35 strings carried an em dash against the operator's rule → rewritten; `check-locale.mjs` now fails on U+2014 in any interface string.

## Markers

npx sheleg-design-skill --lint src @ 58163e7 + working tree (sheleg-design-skill 1.66.0): 0 S1, 0 S2, 0 S3, 0 waived. Before the pass: 2 S1, V005 at style.css:43 and :192.

## Alignment

Falsifier checked on the after captures: no overlap at 740 px on any page in either theme; statistics tables show every column or scroll; card pages share one width. Contrast of the tinted selected row was measured later (SB-169, below); visual judgment is this agent's single pass.

## Open

None. Closed by SB-169 (2026-10-23):

- Agents shows two Refresh buttons: kept one, the page header's (`reload` looks the agents up too); the Installed section's is gone. WebKit at 740 px, both themes: one Refresh, no page error.
- At 740 px the header actions stack with the primary action on top: kept. The primary action is what the page is for; Refresh is quiet and secondary, and the Agents page, where Refresh was doubled, has no primary action.
- Contrast of the tinted selected row and card: measured and asserted by `scripts/check-brand.mjs` (pairs `row-selected`, `card-selected`). Lowest: `--accent-text` on the light selected row, 5.20:1; muted text 6.11:1 light, 7.90:1 dark (WCAG 1.4.3 AA, 4.5:1). The check fails when `style.css` stops mixing what it measures.

## 0.6.82 visual system (node N3, 2026-10-24)

Mode: update within the identity, from the survey `docs/runs/2026-10-24-iteration-ui-ux/survey/visual.json`
(VIS-01, 02, 04 to 10, 14, 17, 18, 22, 23, 25, 26, 28 to 30). Before and after captures: 8 pages,
1100 and 740 px, light and dark, WebKit, demo data, plus the chart, the "Who works where" table, the
Agents panel, the API key dialog, a navigation hover and the In use now card with an 85-character
label (`_worktrees/sb-iter-shots/n3/before`, `n3/after`, each with `metrics.json`).

Decisions, each a role in `src/tokens.css` that the component layer reads:

- One heading scale (`--t-heading`, 16 px semibold): every h2 that opens a part of a page or a card,
  on all eight pages. Measured: h2 sizes were 12, 16 and 22 px by page; now 16 px everywhere except an
  empty state's title and a dialog title (22 px, `--t-section`, kept on purpose).
- Two control heights: `--control-h` 36 px (header, panel and dialog actions, inputs, selects,
  chips, appearance options) and `--control-h-compact` 30 px (row actions, text buttons, the ⋯ menu,
  segments). Measured: 28, 30, 36, 38, 39 px controls before; 30 and 36 after (the sortable column
  title and the usage cell are table content, not controls).
- Selects are drawn like inputs (no native menu chrome, a chevron of two gradients in the muted
  role): 22 px before, 36 px after, the same as the inputs beside them.
- One "chosen" language (`--chosen-bg`, `--chosen-border`, `--chosen-ink`): gold-tinted fill, a
  1 px gold line, ink text, for the segment that is on, choice chips, the Default badge, appearance
  options and model cards. The segment that is on was a white thumb at 1.08:1 on its track in light;
  its line is now 5.73:1 (light) and 12.41:1 (dark) on the track.
- Hover tints toward the ink (`--hover-bg`, 7 % ink into the panel), so it darkens in light and
  lightens in dark: 1.15:1 and 1.17:1 against the panel, where the raised panel was 1.08:1 in light
  and a lighter, not darker, button.
- One notice shape: 1 px line, control radius, the same padding, body text, for the result and error
  notices, the terms notice, the active-rule strip, a found folder and the sign-in banner; only the
  tone changes.
- Chart series (`--chart-1..6`): link (gold in dark, gold-brown in light), info, positive, brand
  magenta, warning mixed with magenta, muted mixed with plum. Every series is 3:1 or more on the
  panel in both themes (lowest 3.84:1, magenta in light) and every pair is at least 0.117 apart in
  OKLab (dark gold and blue-grey were the same colour in 0.6.81; light gold was 1.34:1).
- Axis labels keep their size: the chart scales a 900-unit drawing, so the label's size in drawing
  units follows the panel width through container query units: 11.3 px rendered at 740 px and
  11.5 px at 1100 px (7.4 px and 11.2 px before).
- A table wider than its panel shows it: a shade on the edge that has more and a visible thin
  scrollbar.
- Paths and commands are ink in the data face; only a real link takes the link colour. A group name
  is plain text with a small info mark; its dotted line shows on hover or focus only.
- The In use now card wraps the e-mail between words and the "Saved as" line under it (two lines at
  most); with the 85-character label nothing leaves the card at 740 or 1100 px (it ran out before and
  the e-mail broke every four characters).
- At 740 px the header actions stay a row beside the title; the sidebar version line ellipsizes;
  "Pause countdown" stays on the sort note's line; Kimi Code's actions keep 12 px from its list.
- `--font-mono` is defined (the data face) and `.current-card` is one rule.

Markers: `npx sheleg-design-skill --lint src` was not run in this node (the download was refused
in this session): NOT_RUN, not a pass.

Checks: `node scripts/check-brand.mjs` now also resolves `color-mix` roles, measures the six series on
the panel (3:1), every series pair (OKLab 0.09), the chosen fill and line, text on the hover tint, and
asserts that the segment, chips, appearance options, model cards, navigation and buttons use those
roles. Planted regressions fail it: series 4 back to the link role (OKLab 0.000) and series 1 back to
the bright gold in light (1.34:1).

Open (needs the window code, node N5): colouring only a share of limits and errors worth attention
(the CSS has `.is-alert`; the window would add it to cells at 1 % or more); every cell is ink until
then.
