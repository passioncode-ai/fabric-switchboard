# WF-1 Foundations: Settings page, folding sections, window to MCP parity (SB-363, wave 1)

> automatic switching, вот этот меню большое, пусть оно будет в настройках, переорганизовать, чтобы
> оно не показывалось здесь на главном экране ... не должно быть никакой функции, которая работает
> в интерфейсе и не работает в MCP. Могут быть наоборот ... но никак не наоборот.

## Now

- Pages: `type Page` at `src/main.ts:38` (accounts, tokens, projects, orgs, agents, activity, stats,
  about); no Settings page. `render()` rebuilds the shell (`src/main.ts:349`).
- Automatic switching is drawn on Accounts by `renderPolicies` (`src/main.ts:2393`): status line,
  *Turn on for Claude Code*, a ⚙ menu, one row per policy.
- Standing configuration is spread: balancing, Codex route and terminal hook on Groups
  (`balancingSection` 561, `codexRouteSection` 594, `shellHookSection` 628); backups, analytics,
  appearance, language and updates on About (`renderAbout` 2661).
- Folding is ad hoc: `<details>` in `agentRow` (1065) and `availableAgentsPanel` (1079), toggle sets
  `quotaOpen`, `expandedProjects`, `mapOpen`. No shared component.
- Parity: CLI to MCP is tested (`every_cli_command_has_an_mcp_tool_or_a_reason`,
  `crates/switchboard-cli/src/main.rs:5006`, table from 4781). Window to MCP is not: 100 Tauri
  commands in `src-tauri/src/main.rs`, no check. `set_language` has no tool; the launch, sign-in and
  key-entry commands follow the CLI's written reasons.

## Sub-modules and tasks

**1.1 Settings page.** New `Page` `settings` in the sidebar (between Statistics and About).
- 1.1.1 Sections: *Automatic switching* (the policies of `renderPolicies`, moved whole), *Balancing
  and backup* (from Groups; WF-6 adds backup), *Codex in group folders*, *Terminals*, *Updates*,
  *Language and appearance*, *Backups*, *Usage counts*. About keeps the product, terms and tour.
- 1.1.2 Accounts keeps one line for switching: "Automatic switching is on for N groups" with a link to
  Settings, and the warning when the monitor is unavailable. Nothing else of `renderPolicies`.
- 1.1.3 Deep links: `goTo('settings', 'switching')` from every place that pointed at the old panels.

**1.2 Section component.** One `section(title, count, body, {open, key})` with a header that folds,
a count, the open state kept across `render()` by `key` (the way `summary.dataset.focus` is kept
today, `src/main.ts:351`), keyboard and `aria-expanded`. Replaces the ad hoc toggles where they fold
a whole block; used by WF-2, WF-7, WF-8, WF-11.

**1.3 Window to MCP parity.**
- 1.3.1 A Rust test in `src-tauri` that lists every `#[tauri::command]` registered in
  `invoke_handler` and requires each to name an MCP tool or a written reason (`PERSON`, `LAUNCH`,
  `UI_ONLY` for pickers and notices), as the CLI table does.
- 1.3.2 Close the gaps it finds: `switchboard_settings` (language, appearance; `set_language` today
  has none), and any command WF-2 to WF-11 add lands with its tool in the same change.
- 1.3.3 The reverse stays allowed: an MCP-only tool needs no window control.

## Interface notes

The Settings page is a list of folding sections (1.2), each with its state in its header ("On for
3 groups", "Off"). No section is open by default except the one a link opened.

## MCP

No new behaviour, so no new tool beyond 1.3.2. The test is the gate for every later module.

## Decisions

DEC-0014 (Settings owns standing configuration) and DEC-0019 (window to MCP parity) land here.

## Questions

Settings as a sidebar page rather than part of About: assumed yes (README, question 2).

## Checks

ui-logic cases for the section state; the parity test seen failing on a planted command; WebKit demo
pass of Accounts and Settings (`?demo=1`); scenarios SCN-0xx updated (switching moved), new
SCN for Settings; strings in the registry with Copylot report ids.

## Depends on

Nothing. Every other module uses 1.2 and is held to 1.3.
