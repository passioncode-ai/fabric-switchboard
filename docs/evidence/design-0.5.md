# Design evidence — 0.5 Accounts screen

Run: [PLAN-0.5](../PLAN-0.5.md), REQ-7…REQ-11. Browser demo (`npx vite --host 127.0.0.1 --port 5193`,
`/?demo=1`, managed Chrome) with the synthetic fixture in [demo.ts](../../src/demo.ts): 10 Claude
and 2 Codex accounts at load, 13 and 3 after the checks below (16 rows). No real account, vault, proxy or Terminal is used by
the demo. Figma is off for this product ([foundation](../ux/foundation.md)); tokens are the vendored
PassionCode set ([manifest](../../brand/passioncode/manifest.json)).

| Screenshot | What it shows |
|---|---|
| [accounts-1280.png](design-0.5/accounts-1280.png) | 1280×720, light: In use now (Claude saved, Codex unsaved with Add to Switchboard), Automatic switching bar with Turn on for Claude Code, first rows of the Claude Code group |
| [accounts-1280-full.png](design-0.5/accounts-1280-full.png) | the whole page: provider groups, pool sub-groups, one row per account with badges, quota cell and one primary action; Sign in again, Disabled/Enable, API billing and No quota check states |
| [signin-pending.png](design-0.5/signin-pending.png) | the sign-in banner that replaced the login dialog |
| [about-backups.png](design-0.5/about-backups.png) | About → Backups: folder, the no-passphrase limit, list with Restore, Back up now; Restore over existing accounts → “0 accounts restored · 12 already here.” A row with a limit shows “Limit reached · until …” |
| [accounts-740-dark.png](design-0.5/accounts-740-dark.png) | 740×560, dark: the quota cell moves to a second line; `document.documentElement.scrollWidth` = 740 = `innerWidth` (no horizontal scroll) |

Interaction checks, run in the page with `evaluate_script` on 2026-10-02 (results verbatim):

| Check | Result |
|---|---|
| + Add account opens a menu, focus on its first item | snapshot: `menu "Add account"`, first `menuitem` focused |
| Sign in to another Claude account → banner → finished without a Finish click | after 5 s: banner gone, notice “signin-1@example.test added to Claude Code · default.”, rows 12 → 13 |
| Add to Switchboard (Codex) | notice “release@example.test added to Codex CLI · default. Switchboard keeps its sign-in up to date.”; card “✓ Saved as release@example.test” |
| Switch on North team | notice “Synthetic switch: Claude Code now uses North team…”; the only “✓ In use” row is North team |
| Turn on for Claude Code | “Automatic switching is on for Claude Code in work…”; bar reads “Automatic switching is on · Claude Code · work · Claude Code account · at 90% used” |
| Import from Claude Swap | “2 Claude Swap profiles imported or updated in the default pool · 1 skipped · 1 could not be read — check them in Claude Swap and import again.” |
| Keyboard on the South team menu | focus lands on “Select for managed sessions”, ArrowDown → “Check usage”, Escape closes the menu and focus returns to “More actions for South team” |
| Row menu on South team | items: Select for managed sessions, Check usage, Launch isolated…, Sign in again, Rename or disable…, Remove…; a click outside closes it |

Density: a row is about 53 px against the 0.4 card's three-column body plus an action row; the 12-account
fixture at load fits in 1341 px of page height at 1280 px width
([accounts-1280-full.png](design-0.5/accounts-1280-full.png)).

Not observed: the native window (only the packaged smoke check ran, see
[release-0.5](release-0.5.md)), VoiceOver, and any real account.
