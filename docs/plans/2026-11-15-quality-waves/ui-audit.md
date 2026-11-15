# Window audit, 2026-11-15 (Q2)

Every page of the demo (`?demo=1`) in Playwright WebKit at 1280, 960 and 760 px, after the Statistics width fix (head `3b07b71e`). A: breaks use or shows wrong information; B: looks broken; C: copy, consistency, small layout. Each item names its source.

## A
1. Activity: the new *Sessions on this computer* and the older *Sessions* fold list different sessions with different counts (`renderActivity` appends `sessionTreePanel` and `sessionsPanel`).
2. About at 1280: the License row runs two URLs together (`productPanel`, two inline `address()` links, no separator).
3. About: "Automatic switching: Off by default" contradicts DEC-0008 (on by default).
4. Groups at 960 and 760: group names break letter by letter (`.fold-title { overflow-wrap: anywhere }` with `.group-card .fold-state { white-space: normal }`).
5. Accounts at 760: the *+ Add account* menu opens over the sidebar (`.menu { right: 0 }` once header actions are left-aligned below 820 px).
6. Groups holding accounts but no folders ("Group work · no folders") appear in menus but not on Groups or Balancing; Statistics puts a repository under a group named "no folders".
7. Statistics at 960 and 760: *Who works where* hides its key columns behind a faint horizontal scroll.
8. Settings: Codex *Turn on* is active under "No group has a Codex account yet", while Accounts shows a Codex account in a group.
9. Settings counts working sessions differently from Activity (4 vs 2 in the demo).

## B
10. Activity panel rows sit outside the panel's text edge (inline `paddingInlineStart` replaces `.event-row` padding, and `.about-panel > .event-list` has a negative margin).
11. Activity panel badges end short of the dividers (same cause).
12. *Launches of the last day* fold has a white background (the transparent rule is scoped to `.events-panel .day-fold`).
13. Activity: the Sessions fold and the Events panel touch and differ in width.
14. Activity Sessions fold: a stray divider under the last row.
15. Statistics *By day*: a wide dark vertical scrollbar (only `height` set on `.stat-scroll::-webkit-scrollbar`).
16. Statistics tiles wrap 4+1 at 960 and 3+2 at 760.
17. Settings Codex *Turn on* and API keys *Add OpenRouter key* stretch to the full width (buttons straight in a grid).
18. Settings Balancing: each group checkbox is a large card (radio-card class).
19. Panel widths differ from page to page (`.about-panel { max-width: 980px }` vs full-width folds).
20. Agents: row controls misaligned; *Set up* shifts when a slot is empty.
21. Accounts: the Kimi row shows usage as plain lines with no bar.
22. Accounts: "… · estimated" wraps alone onto a line.
23. Accounts: "Ordinary kimi: Demo · Allegretto ·" ends with a dangling separator.
24. Sidebar footer at 960 and 760: the version line wraps.

## C
25. Settings: double parentheses in "Edit Claude Code in group work (no folders) (Managed route)…".
26. Activity: "A launch in tools-cli ended with no end recorded" and "Gone without an end" read as contradictions; "By group: Group alpha · no folders 2" reads as part of the name.
27. Activity: extra gap above the finding line.
28. Activity Events: detail larger than title, a raw id shown, jargon footnote.
29. Activity: the demo's old Sessions list names an account Accounts does not have.
30. Agents: internal identifiers in copy (vault slot names, `global: true`, a docs path).
31. Agents: unclear chain status line.
32. Agents: Hermes and OpenCode say "no account with room" where the blocker is no OpenRouter key.
33. Agents: "Applies to every session Switchboard starts on this computer." four times in one panel.
34. Agents: two control sizes in one panel.
35. Agents: the "✓ Default" marker looks like a button.
36. Agents: native disclosure triangles beside chevron folds.
37. Agents Connect: "tools registered" beside "command-line tool not found on PATH".
38. Agents at 960: a command wraps mid-word.
39. Text buttons (*Open API keys*, *Open Settings*, *Pause countdown*, *Copy*) do not look clickable.
40. Navigation: the Groups badge counts folder rules but reads as a group count.
41. Intro paragraphs capped at 72ch above full-width panels.
42. Groups at 960: the subtitle wraps early.
43. Groups: a chain pins an account that is not in the group.
44. Groups: awkward copy ("Move one there with Move to another machine… on Accounts").
45. Groups vs Agents: a group "Tools" beside a "Tools only" chip.
46. API keys: agents named differently on one page.
47. API keys: unclear phrasing.
48. API keys: "2 groups have no key: agents there cannot start" contradicts the machine-key fallback above.
49. Accounts: badge "Sign in again" vs buttons "Sign in".
50. Accounts: the Switch/Select note sits after the Codex section.
51. Accounts: *In use now* details are blue like links.
52. Accounts: "Switch on an account's row makes it the account of Claude Code on this computer."
53. Accounts: the expanded card shows the reset twice.
54. Settings: jargon ("byte for byte"), lowercase command names.
55. Organizations: no *+ New organization* in the header; "by your word".
56. Statistics at 760: "By day" alone on a second line of the pinned bar.
57. Statistics footnote: 12 lines, an internal version.
58. About: jargon ("user claims", "HTTP/SSE proxy", "Imported OAuth").

## Status, 2026-11-15 (Agents page)

- Items 20, 35: the takeover switch and a “Tools only” label share one width, the default marker is a plain label (`.default-badge`), so the columns line up.
- Item 34: the default agent's choices take the compact control height, like the permissions switch.
- Item 38: a command wraps only after a slash or a hyphen (`breakableCode`, `<wbr>`), never inside a word.
- Item 31: the fallback chain reads on its own line, its link after it.
- Item 37: the PATH note says registered agents still reach the tools through the app; Copylot en-US `50b7ce3a`, ru-RU `f94d8fd9`.
