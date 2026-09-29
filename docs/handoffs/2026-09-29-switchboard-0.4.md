# Handoff — Switchboard 0.4 (agents, project rules, audit repair) — 2026-09-29

**State: parked on purpose.** The operator stopped this run to move work to Fabric. Everything
below is committed on `main`; nothing is released. Versions still read `0.3.2`.

## Objective

Audit Switchboard against the PassionCode ecosystem (license, brand, icon, site), let coding
agents read remaining usage and switch accounts through MCP, add optional project rules, add a
PassionCode skill, fix the audited bugs, update the website, and ship 0.4.0-beta.1. Brief,
decisions, REQ table, MCP contract and defect list: [PLAN-0.4](../PLAN-0.4.md).

Operator decisions that bind the next agent (PLAN-0.4 D-1..D-4): agents switch managed sessions
freely, the global Claude Code login only with `global: true`; project rules are **optional,
off unless saved, always visible and easy to pause** — the main job stays rotation so limits
continue across projects; full release cycle; design in code, no Figma.

## Done (with receipts)

| Area | What | Receipt |
|---|---|---|
| License | PolyForm NC OR Internal Use, CLA, notices — landed by another session | `71298f9` |
| Project rules | core rules, schema 3 only while rules exist, cascade on account removal | `crates/switchboard-core/tests/projects.rs` (5 tests) |
| Runtime | rule ops, `apply` per calling session, session detection, `UNAVAILABLE` sources so synthetic owners never read or write real sign-in | `crates/switchboard-runtime/src/projects.rs` tests |
| MCP | `switchboard mcp [--read-only]`, 8 tools, JSON-RPC edge cases | `crates/switchboard-cli/tests/mcp.rs` (4 tests; a planted pool-check defect made it fail) |
| Launch | managed/isolated sessions get the tools (`--mcp-config`, `[mcp_servers]`) and `SWITCHBOARD_SESSION` | `launched_sessions_get_switchboard_tools_with_exact_quoting` |
| Agents IPC | `agent_setup`, `link_cli` (never replaces a foreign file) | `link_is_created_reused_and_never_replaces_foreign_files` |
| Backend bugs | B-01, B-03, B-06..B-10, B-13, B-14, B-17..B-20 | test names in [PLAN-0.4](../PLAN-0.4.md#req-status-at-park-2026-09-29) |
| Desktop bugs + brand | B-02, B-04, B-05, B-11, B-12, B-15, B-16; design system 1.1.0, System/Dark/Light, About license, bundle metadata | [design-0.4 evidence](../evidence/design-0.4.md), `scripts/test-ui-logic.mjs` |
| Desktop 0.4 | Projects screen, active-rules strip + nav count, Agents screen | browser demo `docs/evidence/design-0.4/{projects-light,agents}-1280.png`, SCN-025/026 |
| Plugin + skill | `plugins/switchboard` (MCP server + `switching-accounts` skill), evals, validator | `python3 scripts/check_plugin.py`; `claude plugin validate ./plugins/switchboard --strict` and `claude plugin validate . --strict` passed |
| Build script | macOS build embeds the signed CLI at `Contents/MacOS/switchboard` | `scripts/build_macos.py` — **not yet exercised by a real build** |
| Website | branch `agent/switchboard-0.4-site` @ `1ead099` in `passioncode-ai/passioncode-ai.github.io`, pushed, **not merged, not deployed** | site `npm run check` exit 0, `node --test scripts/switchboard-release.test.mjs` exit 0, `npm run build` exit 0 |

Gate at park: `./scripts/check.sh` exit 0 (13 test result lines ok, 0 FAILED) on the commit
that merged the plugin. Hosted CI: nightly only, not dispatched.

## Open (board: [backlog.md](../evidence/backlog.md))

- **SB-09 release 0.4.0-beta.1** — the exact next task, below.
- **SB-07** one unexplained failure of `project_rules_are_optional_visible_and_applied_on_request` inside `check.sh`; the assertion now prints the tool's answer.
- Launcher member: add `switchboard` to `passioncode/family.json` (needs the release tag first; see `working-in-passioncode` releasing reference).
- SPEC.md not yet updated for 0.4; wiki page for Switchboard not created (stage 9).
- Native (Tauri webview) rendering of the new screens not observed; only the browser demo.
- SB-01 live provider acceptance, SB-02 Windows host, SB-03 Windows signing, SB-04 stale lock, SB-05 Windows installer without CLI, SB-06 native reload behaviour.

## Exact next task — release 0.4.0-beta.1

1. `python3 ~/.local/share/observatory-agent-updates/agent_updates.py check` if present; `git pull --ff-only` on `main`.
2. Bump to `0.4.0` everywhere (workspace `Cargo.toml`, `package.json`/lock, `src-tauri/tauri.conf.json`, `plugins/switchboard/.claude-plugin/plugin.json`, marketplace entry, SKILL `metadata.version`); `./scripts/check.sh` (the plugin validator then also compares package.json).
3. `python3 scripts/build_macos.py --identity "Developer ID Application: …" --notary-profile fabric-notary` (profile verified working 2026-09-29: 81 accepted submissions). Check `Contents/MacOS/switchboard` inside the app, `spctl -a -vv`, native smoke.
4. `python3 scripts/build_windows_cross.py` (re-downloads the xwin SDK cache).
5. Write `docs/evidence/release-0.4.md`, tag `v0.4.0-beta.1` on `main`, `gh release create` with SHA256SUMS, verify anonymous downloads.
6. Website: rebase `agent/switchboard-0.4-site` on the site's `origin/main`, run `node scripts/update-switchboard-release.mjs v0.4.0-beta.1`, replace the screenshot from the native 0.4 app, `npm run check && npm run build`, merge, deploy per the site's `DEPLOYMENT.md`, record live hashes.
7. Launcher: `family.json` member `{name: switchboard, repo: passioncode-ai/fabric-switchboard, ref: v0.4.0-beta.1, kind: plugin, path: plugins/switchboard}` — check whether the vendor step accepts a prerelease tag; release the launcher per its README; then `npx @passioncode-ai/passioncode@latest update` locally and switch the Agents row back to the launcher command.
8. Install the notarized app in /Applications (keep the old one for rollback), link the CLI from the Agents screen, and try `switchboard_status` from a real Claude Code session (start of SB-01).

## Where things are

- Code: `crates/switchboard-core/src/projects.rs`, `crates/switchboard-runtime/src/{projects,agents}.rs`, `crates/switchboard-cli/src/mcp.rs`, `src/main.ts` (Projects/Agents), `src/ui-logic.ts`.
- Contracts: [CONTRACTS.md — 0.4 section](../CONTRACTS.md#agents-and-project-rules-extension-04); operations: [OPERATIONS.md — 0.4](../OPERATIONS.md#04-project-rules-and-agents); CLI: [CLI.md — For agents](../CLI.md#for-agents).
- The site repository's local checkout `~/DATA/passioncode-ai.github.io` sits on an old unpushed branch `codex/switchboard-release-032` from an earlier session; it was left untouched. Work from `origin/main`.
