# Canvas of 0.6.82 (iteration 2026-10-24)

Inventory: survey/ux.json (30), survey/visual.json (31), survey/copy.json (18),
survey/product.json (23): 102 findings, 7 P1. Every finding sits in exactly one node below or in
the next pool.

## Decisions taken without asking (recorded; reversible)

- D-1 Vocabulary. The person sees **group** (Russian «группа») for what the CLI and MCP
  identifiers call `project` and the store calls `pool` (operator decision SB-146, 2026-10-19:
  a project is a group). Command names, tool names and JSON fields stay as they are (stable API);
  every sentence a person reads says group. `pool` is shown only as the group's id where an id is
  needed. The feature is **automatic switching** («автоматическое переключение»); `rotation`
  stays an identifier only. docs/brand/terminology.md defines both.
- D-2 The page of API keys is called **API keys** («API-ключи»), not Tokens: Statistics already
  uses tokens for usage counts (CPY-14).
- D-3 Issue #128 follows the operator's decision of 2026-10-08 as written in the issue (nothing
  queued or sent until the person answered the switch once; a first-run notice; Settings switch).
- D-4 recovery.json keeps rows of the last 7 days and at most 512 (PRD-03); older waits are
  dropped at read and on the next write.

## Nodes

| Node | Wave | Risk | Owns (files) | Findings |
|---|---|---|---|---|
| N1 owner reliability | A | R3 | crates/switchboard-core (counters), crates/switchboard-runtime except launch.rs and analytics.rs | PRD-01, PRD-02, PRD-03, PRD-12, PRD-13, PRD-14, PRD-17 |
| N2 CLI and MCP words | A | R2 | crates/switchboard-cli, core agent_keys.rs, runtime launch.rs (messages only), plugin tools.md | CPY-02 (backend), CPY-08/09 (CLI, MCP), CPY-01/06 (CLI, MCP prose), PRD-04, PRD-07, PRD-15 |
| N3 visual system | A | R2, product surface | src/style.css, src/tokens.css, scripts/check-brand.mjs | VIS-01, 02, 04, 05, 06, 07, 08, 09, 10, 14, 17, 18, 22, 23, 25, 26, 28, 29, 30 |
| N4 docs truth | A | R1 | docs/*.md except docs/ux/*, docs/evidence/*, HANDOFF; README*.md; docs/brand/* | PRD-05, PRD-06, PRD-16 (as a list for the orchestrator), CPY-12, CPY-13, CPY-08 (README.ru), D-1/D-2 in terminology.md |
| N5 window behaviour and words | B | R2, product surface | src/main.ts, src/ui-logic.ts, src/adapter.ts, src/i18n.ts, src/locales/*, src/demo.ts, docs/ux/* | UX-01..12, 14, 17..24, 26, 28..30; VIS-03, 11, 12, 13, 15, 16, 19, 21, 27, 31; CPY-04, 05, 06, 07, 10, 11, 14, 15, 17, 18 (window side) |
| N6 analytics consent (#128) | C | R3 | runtime analytics.rs, src-tauri/src, src/main.ts (notice), locales, docs/ANALYTICS.md, docs/ux/scenarios.md | PRD-08 |

Seams: N2 and N5 share error sentences (backend text vs window rewrite table): N5 owns
src/adapter.ts, N2 the backend strings; the rewrite keys move with the backend text (N2 lists
every changed backend sentence in its report, N5 updates the table). N3 and N5 share class
names: N3 never renames a class, N5 may add classes that N3's tokens style.

## Next pool (not this release)

CPY-03 (one error text for every surface, moved into the runtime), PRD-09/UX-13 (fallback chains
in the window, SB-71: scenario first), PRD-10 (stopped sessions and hand-over setting in the
window), PRD-18 (update restarts during managed sessions), PRD-19 (SB-139 capability literal),
PRD-21 (SB-183 shell hook), PRD-23 (SB-07 flaky tests), PRD-11 (CLI command aliases `group`),
UX-13, UX-16, UX-25 (Linux scenario), UX-27 (demo first run), VIS-20, VIS-24 (motion).
