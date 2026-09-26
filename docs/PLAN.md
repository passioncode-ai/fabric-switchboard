# Execution plan and delivery state

Pipeline profile: research/specification → implementation → verification → repository handoff. Operator authorized autonomous design/build. Evidence determines completion, not whether a stage has a document.

| Order | Packet | Owner paths / dependency | State and gate |
|---|---|---|---|
| 1 | Four-source research | docs/research; fixed commits | complete: 4 HEADs, 69 line ranges resolve |
| 2 | Core | crates/switchboard-core; CONTRACTS | implemented: 19 default tests, separate native Keychain smoke |
| 3 | Interface | src; IPC and scenarios | implemented: typecheck/build, browser interactions; native window blocked by locked Mac |
| 4 | Managed routing | crates/switchboard-proxy; core snapshot | implemented: 9 synthetic upstream tests |
| 5 | Native integration | src-tauri; core/proxy | implemented: 8 fixtures, release .app builds; real login/response acceptance pending |
| 6 | Repository handoff | scripts/docs/README | source committed and delivery receipt recorded in HANDOFF |

Independent build/research agents used separate worktrees with explicit ownership. Root integrated explicit commits and reviewed seams; final bounded launcher correction was made in root with sole-file ownership. All integrated work is on `codex/bootstrap`; member branches contain no separately required unmerged delivery. External research trees stay outside this product and are not runtime dependencies. No changes to Observatory or Fabric parent/submodule pins.

Next tasks in order:
1. [PA-01 real-provider acceptance](packets/provider-acceptance.md) and native window inspection on an unlocked Mac.
2. [SS-01 recovery/session supervisor](packets/session-supervisor.md), including graceful graphical startup errors and optional Desktop handoff.
3. [AR-01 recommendation/automatic routing](packets/automatic-routing.md).
4. [WIN-01 Windows native port](packets/windows.md).
5. [DIST-01 signed distribution](packets/distribution.md).

v0.1 is the local manual workbench and managed routing baseline. These pending packets are not represented as shipped. Runtime/session recovery and authentic client compatibility are more urgent than adding a tray icon or another routing heuristic.

## 0.2 CLI and native distribution extension

The operator promoted CLI, macOS signing and Windows build into current scope. [CLI release plan](CLI-RELEASE-PLAN.md) supersedes the original order for these items. Shared Runtime/Owner and CLI are implemented; Windows now has DPAPI, DACL and launcher implementations. Packaging uses exact-commit universal macOS and native Windows workflows. [0.2 release evidence](evidence/release-0.2.md) is the execution ledger; earlier table counts describe 0.1 only. PA-01, native Windows interactive acceptance, notarization and signed updates remain distinct gates.
