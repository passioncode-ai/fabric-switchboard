# SB-189: one error text for every surface (task-pipeline run, 2026-11-02)

**Task.** The window rewrites backend refusals into sentences that say what happened and what to do
(`coreErrors` in `src/adapter.ts`); the CLI and the MCP server print the raw backend text (CPY-03,
[survey](../2026-10-24-iteration-ui-ux/survey/copy.json)). Make one sentence per failure, owned by the runtime,
shown by all three.

**Mode.** Full flow (stage 0 triage: more than five files, a public contract (CLI and MCP output), a user-facing
change). Model: the session's own (Opus 5.5), confirmed by the standing operator rule. Loop: off.

## Source ledger

| Source | What it says | State |
|---|---|---|
| `src/adapter.ts:337-480` | `coreErrors`: 193 raw sentences mapped to 134 person sentences (object entries and 8 grouped `for` loops) | current, read 2026-11-02 |
| `src/formatted-errors.ts` | 2 templated refusals with an agent's name, shown verbatim | current; out of scope (already person text) |
| `scripts/check-error-vocabulary.mjs` | every backend error string must be in `safeErrors`, `coreErrors` or an allowlist | current; reads `coreErrors` from the AST |
| `scripts/check-locale.mjs:70-88` | collects window strings, `coreErrors` values among them, for the ru locale | current; reads `coreErrors` from the AST |
| `crates/switchboard-cli/src/main.rs:3178-3184` | CLI prints the raw error (`--json`: `{"ok":false,"error":…}`) | current |
| `crates/switchboard-cli/src/mcp.rs:1079-1082` | MCP returns the raw error as `isError` text | current |
| `crates/switchboard-runtime/src/lib.rs:2699` `add_refusal` | classifies raw refusals by prefix (`Project not found` → `group_removed`) | current; must keep matching raw text |
| `plugins/switchboard/skills/switching-accounts/references/tools.md:452-475` | the refusal texts agents relay | current; one row names a raw sentence |
| `crates/switchboard-cli/tests/mcp.rs:684` | asserts CLI stderr contains `Account not found` | current |
| CONTRACTS → shell env (`docs/CONTRACTS.md:1885`) | unrelated; read for SB-135 | not applicable |
| docs/DOCMAP.md | absent; the project's docs map is AGENTS.md + CONTRACTS + HANDOFF + backlog (recorded, not seeded: the operator's routes table lists `/task-pipeline setup` as remaining setup) | known gap |

Contradictions: none between sources. The backlog row's second clause ("core's `Project not found.` becomes group
wording together with `add_refusal` and the window's keys") predates a shared table; see D-2.

## Decisions

- **D-1. One table, owned by the runtime.** `crates/switchboard-runtime/src/person-errors.json` maps each raw
  backend sentence to its person sentence; `switchboard_runtime::person_errors::for_person` reads it (embedded at
  build), the CLI and MCP apply it at their output, and `src/adapter.ts` imports the same file. Raw strings stay the
  internal vocabulary: classifiers (`add_refusal`), tests and logs keep matching them. Why: moving 193 sentences
  into their `Err(...)` sites would change every internal matcher and test that reads them, for the same result.
- **D-2. `Project not found.` stays raw.** Every surface shows "This group no longer exists. Refresh the list." through
  the table, so the group wording reaches the person; `add_refusal` keeps its prefix. The row's clause is met by D-1.
- **D-3. A sentence that names only a window control names the CLI route too.** Six sentences ("In use now → Add to
  Switchboard", "+ Add account → …", "Add folder…", "the folder picker") are reworded to name both routes, as the
  API-key sentences already do; ru follows; both checked with Copylot.
- **D-4. Machine readers keep the raw text.** `--json` adds `"raw"` beside `"error"` when they differ; the MCP text is
  the person sentence (agents relay it, `tools.md`). A raw sentence not in the table is printed as it is.
- **D-5. `adapter.ts` keeps its safety allowlist** (`safeErrors`, `fallbackErrors`): an unknown sentence is still
  never shown raw in the window.

## REQ spine

| REQ | Requirement | Verified by |
|---|---|---|
| REQ-1 | The CLI prints the person sentence for every raw sentence in the table (text and `--json`), with `raw` in JSON | CLI test over a known refusal; unit test of `for_person` |
| REQ-2 | The MCP server returns the person sentence as its `isError` text | MCP test over a known refusal |
| REQ-3 | The window shows the same sentence as before for every raw key (the table equals the old `coreErrors`, except D-3) | generated-table comparison against the old map; `scripts/test-ui-logic.mjs` |
| REQ-4 | `check-error-vocabulary.mjs` and `check-locale.mjs` read the table and stay green; a raw sentence missing from it still fails | gate; a planted unmapped sentence fails the check |
| REQ-5 | D-3's six sentences name the CLI route; en and ru pass Copylot | Copylot report ids below |
| REQ-6 | Internal matchers unchanged (`add_refusal`, tests on raw text) | full gate green |
| REQ-7 | Docs: CONTRACTS (error text), CLI.md (`raw`), tools.md, CHANGELOG, backlog SB-189, HANDOFF | `scripts/check_docs.py`, review |

## Progress

- [x] 0 Intake: this brief.
- [x] 1-4 Docs, design, spec, plan: D-1..D-5 above are the design; the plan is the four nodes below (no external
  library is involved, so stage 1 fetched nothing).
- [x] 5 Build: N1 table and `for_person`; N2 CLI and MCP; N3 window and scripts; N4 docs. D-3 grew to seven
  sentences: `UNSAVED_CURRENT` (a `safeErrors` sentence) named only *In use now → Add to Switchboard* too.
- [x] 6 Tests: REQ-3 the table equals `origin/main`'s `coreErrors` (193 keys, 6 values changed, the D-3 ones);
  REQ-4 a key removed from the table makes `check-error-vocabulary.mjs` exit 1, restored exit 0; REQ-2 the MCP test
  fails when the server prints the backend's text (`left: "Choose an existing project folder."`); REQ-5 Copylot en
  `f75597e5-1f3e-4b8e-99cb-ccd49d8a7f5e`, ru `97d97260-3c19-4c46-b51c-7c8b26eac9c1` (no findings); `check-locale.mjs`
  failed on the six reworded sentences until ru had them; full gate in the pull request.
- [x] 9 Docs: CONTRACTS *One sentence per refusal (SB-189)*, CLI.md, plugin `tools.md`, scenarios (SCN quote of
  `UNSAVED_CURRENT`), CHANGELOG, backlog SB-189.
- [ ] 10 Acceptance: merged, then released with the next version.
