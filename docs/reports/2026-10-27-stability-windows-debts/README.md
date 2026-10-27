---
report: stability-windows-debts
project: fabric-switchboard
date: 2026-10-27
kind: audit
sources: docs/evidence/backlog.md at main 871e92a; .github/workflows/nightly.yml; iteration/0.7.0 74b47ef
---

# Stability, reliability and Windows debts (2026-10-27)

Operator request 2026-10-27: «проверим все долги по стабильности и работоспособности этого проекта и поддержки на Windows, и давай все доделаем». Read from the backlog at `871e92a`: 81 open rows of 229. This page sorts the ones about stability, reliability and Windows into what an agent finishes now, in waves, and what waits on a person or another project. Status of each row stays in the backlog.

## Measured facts

- Nightly on `main` (run 40429778333, 2026-10-27) is green on Windows x64 and arm64: `cargo test -p switchboard-core -p switchboard-runtime -p switchboard-cli` runs there.
- On `iteration/0.7.0` the runtime's tests do not compile for Windows (`cargo xwin check --tests`: 39 errors, `WF` in `continuation.rs`, `fallback.rs`; SB-224). Unmerged branches are outside the nightly run, so this would turn the first nightly after the 0.7.0 merge red. It blocks the release.
- The PowerShell hook ran in PowerShell 7.6.9 on macOS (A-8); Windows PowerShell 5.1 and a Windows machine never ran it.

## Waves

| Wave | Rows | What | Gate |
|---|---|---|---|
| W1 | SB-224 | Runtime tests compile and pass on Windows: gate unix-only helpers | `cargo xwin check --tests` for x64; nightly Windows jobs on `iteration/0.7.0` |
| W2 | SB-225, SB-226, part of SB-02 | PowerShell: transition test also under Windows PowerShell 5.1 where present, paths as base64 in `shell init pwsh`, load-time apply only interactive, UTF-16LE profiles, Constrained Language Mode; carried links on Windows through junctions and hard links (Q7 option b) with tests that run on the Windows runners | the same, plus one dispatched nightly run on `iteration/0.7.0` (Windows x64, arm64, macOS, Linux) |
| S1 | SB-191, SB-196, SB-198, SB-199, SB-204, SB-205 | Close: released in 0.6.83 and marked "to close" | backlog receipts |
| S2 | SB-215, SB-218, SB-219, SB-220, SB-209, SB-210, SB-223, SB-221 (launch test), SB-07 | Runtime reliability batch: hand-over records safe across versions and re-gated, nested sessions, launch-record scans off async workers, hook calls side by side, repeat window, hook edges, key races, flaky tests | runtime, core, cli tests; one blind round |
| S3 | SB-207, SB-208 | Updates: `urgent` from the release script; About names the six-hour pause | release script test, ui-logic |
| S4 | SB-222, SB-227, SB-228, SB-229 | 0.7.0 window leftovers: honest hook-off line, leaving marks only from well-formed reads, degraded caveat, chains re-read and clock | ui-logic, WebKit |
| S5 | SB-213, SB-214 | Spend gate edges (R3): key slot re-checked in the transaction; gate budget counts only gates | two rounds incl. adversarial |
| after release | SB-174, SB-163 | Measure a day of `control_slow` with `lock_ms` on the installed 0.7.0; close or name the next cause | the day's log |

## Waits on a person or another project

| Row | Gate |
|---|---|
| SB-02 (live part) | a Windows machine the agent can reach (operator queue item Q-A); W1, W2 and the Windows runners cover everything else |
| SB-01, SB-15, SB-178, SB-06 | live accounts, operator-assisted acceptance |
| SB-13, SB-16 | operator decision / go |
| SB-32 | SB-03 (Windows signing of the uninstaller) |
| SB-92 | rehearsal of the shared signing action on an `-rc` tag, with the release environment |
| SB-91 | the website repository |
| SB-216, SB-217 | decisions C-N1, C-N2 for 0.7.0, revisit after it |
