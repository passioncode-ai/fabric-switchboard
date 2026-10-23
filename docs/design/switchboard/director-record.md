---
surface: switchboard-desktop
surface_class: product
revision: 2026-10-22, branch design/sb-164-screens (0.6.63)
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
