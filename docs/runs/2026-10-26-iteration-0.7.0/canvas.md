# Iteration 0.7.0 «first organization»: canvas

Release iteration through `/task-pipeline iteration`, target 2026-11-14 (plan
[2026-10-25](../../reports/2026-10-25-audit-and-plan/README.md) §3B, decisions D-3, D-5, D-6).
Survey: that plan. Dossiers, written in parallel on `a9ff8fa`:

| Dossier | Items | Nodes | Risk |
|---|---|---|---|
| [A, context binding](dossier-a-context-binding.md) | SB-183 (shell hook, default on, PowerShell), SB-201 | A-0 … A-9, A-10, A-11 | 2 × R3 (A-2 scripts, A-3 rc installer) |
| [B, the organization's person](dossier-b-organization-person.md) | SB-202, SB-160, SB-125 | B-1 … B-10 | 1 × R3 (B-5 ended seat) |
| [C, chains and ceilings](dossier-c-chains-and-ceilings.md) | SB-71, SB-72 | C-1 … C-9 | 4 × R3 (C-4 … C-7, money) |

Not in this iteration: SB-167 (7 days after Observatory's OBS-191), SB-184 (Observatory OBS-236, a 0.7.x after
2026-11-14), SB-177 (Fabric CO-412). Live checks SB-159, SB-178, SB-143, SB-15 are the operator's
([checklist](../../reports/2026-10-26-operator-acceptance/README.md)).

## Integration pass 1

**Renamed.** Dossier A's SB-201 nodes, written as B-1 and B-2 there, are **A-10** and **A-11** here (dossier B
owns the B ids). Scenario ids proposed by all three started at SCN-074; the last in `docs/ux/scenarios.md` is
SCN-073, so: dossier A takes **SCN-074, SCN-075**; dossier B **SCN-076 to SCN-079**; dossier C **SCN-080 to
SCN-084** and **SCR-03**.

**One seam per pair, designed together.**
- **A with B (the checklist's `hook` step).** B-3's `onboard` step `hook` reads A-1's `switchboard shell status`
  (the map file), never the rc files: A-1 owns the format, B-3 only reads it. Order: A-1 before B-3.
- **A with C (plain `claude` in a group folder).** C-8 resolves a group's chain by the checkout's folder before the
  executor's account: the case A's hook makes common. C-8 lands before A-3 turns the hook on by default.
- **B with C (an ended seat in a chain).** B-5 puts an ended seat in the sign-in-required set; C-5's readiness
  already skips that set, so a chain never hands work to an ended seat. No shared file.
- **Shared files.** `docs/ux/scenarios.md` (A-7, B-1, C-1) becomes one node **S-1**, written once through
  super-ux. `src/main.ts`, `src/ui-logic.ts`, `src/locales/ru.ts` (A-7, B-6 … B-8, C-3, C-7) are touched by one
  UI node per wave, in sequence. `crates/switchboard-cli/src/main.rs` (A-1, B-3, C-6) and
  `crates/switchboard-runtime/src/lib.rs` (A-3, A-4, B-4, C-2, C-4) are edited in disjoint hunks, one node at a
  time per wave.

A second pass changed nothing.

## Waves

| Wave | Nodes | Gate |
|---|---|---|
| 1 | S-1 (scenarios SCN-074 … SCN-084, SCR-03, amendments to SCN-039, 040, 051, 056, 065, 066), A-0 (measurement) | `/ux-lint` green; A-0 recorded |
| 2 | A-1, A-5, A-10, A-11, B-2, B-5, C-2, C-4, C-8 | each node's tests, risk-class rounds |
| 3 | A-2, A-3, A-4, B-3, B-4, C-5, C-6 | same |
| 4 | A-7, A-8, B-6, B-7, B-8, C-3, C-7 (UI, in sequence), Copylot on every string (en-US, ru-RU) | WebKit pass of the window, both themes, en and ru |
| 5 | A-9, B-9, C-9, release | doc gate, `./scripts/check.sh`, release |
| after | B-10 (SB-125 live setup), the operator's acceptance | human gates |

Questions: [questions.md](questions.md).

## Wave 2 state (2026-10-26)

| Node | Branch, commit | Rounds | Open |
|---|---|---|---|
| A-1, A-5 | `w2/shell` `e86d0c6` | round 1: seam fail (capture not refused, wildcard match, map rule) | fix cycle 1 running |
| A-10, A-11, B-2 | `w2/context` `268df9a`, fix `f4139e3` | round 1: unit pass, seam fail (provider collision); round 2: blocker closed | Dashboards must read `agent` (board `67535b26`, thread `3f0ec524`): **release gate for 0.7.0**; wave 4: catalog names in `sessionAgentName`, a fill test for kimi, codex, hermes |
| B-5 (R3) | `w2/seat` `0e0b0fb`, fix `42e095c` | round 1: unit pass, seam fail (renew_idle_live log); fix cycle 1 done | adversarial round owed |
| C-8 | `w2/chains` `4919d70` | none yet | C-4, C-2 half built in the worktree (builder stopped for disk) |

Builders run one at a time since 2026-10-26 02:00 UTC (the MacBook's disk: 2 to 6 GB free; fabric-workspace's request). A shared
`CARGO_TARGET_DIR` served stale binaries twice: a green run counts only when its output names the node's tests.
