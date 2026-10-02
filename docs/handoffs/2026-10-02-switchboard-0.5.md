# Handoff — Switchboard 0.5: prompt-free switching, renewal, one-click accounts — 2026-10-02

**State: implemented on branch `agent/switchboard-0.5`, not released.** Objective (operator, Russian,
paraphrased): no extra Keychain or other confirmations when switching accounts, an automatic
switcher that keeps working, the active console account added with one button and no extra windows,
a compact grouped account list, every core feature rechecked. Brief, decisions D-1…D-4, root causes
C-1…C-8, REQ table, contracts and plan: [PLAN-0.5](../PLAN-0.5.md). Checks and the seam review:
[release-0.5](../evidence/release-0.5.md). Screens: [design-0.5](../evidence/design-0.5.md).

## Done

| What | Where |
|---|---|
| Switchboard's own vault: **unchanged from 0.4.1** ([KEYCHAIN.md](../KEYCHAIN.md)); the file vault built first in this run was withdrawn at integration (PLAN-0.5 D-1) | — |
| `/usr/bin/security` transport (stdin hex, 4 KB fallback, exit codes) | `crates/switchboard-core/src/security_cli.rs` |
| Claude Code credential item written/deleted, isolated sign-in item read/deleted, only through `/usr/bin/security` (stdin hex) | `crates/switchboard-runtime/src/external.rs`, `launch.rs`, `external_keychain.rs` |
| Renewal of inactive Claude OAuth accounts (never the Claude Code account or one seen there in 15 min, never a row without identity), 401 retry, separate renewal pass, `invalid_grant` → Sign in again, dead lineage never activated | `crates/switchboard-runtime/src/refresh.rs`, `monitor.rs`, `lib.rs`; `Store::adopt_refreshed` |
| Switching an expired-but-renewable account; live generation kept in every pool; native rotation accepts renewable accounts | `lib.rs` `replace_native`, core `rotation.rs` |
| `LoginStatus` operation (CLI `login status`, Tauri `login_status`), optional sign-in label | `lib.rs`, `launch.rs`, `crates/switchboard-cli/src/main.rs`, `src-tauri/src/main.rs` |
| Accounts screen: In use now with Add to Switchboard, + Add account menu, sign-in banner that finishes on its own, one-click Claude Swap import, grouped compact rows with one primary action and a row menu, one-click Turn on for Claude Code | `src/main.ts`, `src/ui-logic.ts`, `src/style.css`, `src/demo.ts` |
| Docs in the same change | SPEC, CONTRACTS, ACCOUNTS-AND-ROTATION, OPERATIONS, CLI, RESEARCH-0.3 note, AGENTS.md, scenarios SCN-003/018/019/020/022 + new SCN-028/028, screens, flows, brand strings, skill text, wiki page `projects/fabric-switchboard` |

## Decisions

D-1 superseded by 0.4.1's shared-trust vault (no second migration of the operator's accounts); D-2 renew inactive Claude accounts only (supersedes 0.3's "no competing
refresh grant"); D-3 the Claude row's primary action is Switch without a dialog; D-4 design in code.
Accepted risk: the read-only MCP server's `switchboard_usage` can renew an inactive token inside
Switchboard (stated in the tool description and skill).

## Open (board)

- **SB-17** release 0.5.0-beta.1 (in progress), then **SB-15** operator acceptance on the real Mac.
- **SB-16** Codex renewal. Earlier rows unchanged.
- Windows compile of runtime/CLI was not run here (`ring` needs MSVC headers); the Windows build is the proof.

## Exact next task

Finish SB-17 by the 0.4.1 release path, then SB-15 with the operator present.
