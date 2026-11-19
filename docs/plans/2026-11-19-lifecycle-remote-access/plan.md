---
plan: 2026-11-19-lifecycle-remote-access
spec: spec.md
report: ../../reports/2026-11-19-lifecycle-remote-access-audit/README.md
rule: every wave ends with its PRs merged after two consecutive clean review rounds and the Studio gate on the merged head; releases go out between waves; a finding that blocks nothing becomes a backlog row, not more work in its wave
---

# Plan: waves W11.0 to W11.8

| Wave | Content | Spec | Findings | Backlog rows | Exit |
|---|---|---|---|---|---|
| W11.0 | Windows stack (v0.7.123 stopped); release 0.7.124; board hygiene | S7.1 | SB-558, M5 statuses | SB-558, SB-559 | 0.7.124 published with a green Windows nightly; every backlog status true |
| W11.1 | Security: value-returning doors, per-launch capabilities, recall identity, guest seat states, typed machine proof, verifiable decisions, `ssh-serve` limits | S2.2, S3.1..S3.6 | M3-01..10, M3-17, M3-18, M2-01, M2-02, M2-10, M2-21 | SB-560 | merged; release |
| W11.2 | One continuation module | S1.1..S1.7 | M1-01, M1-03..07, M1-21..23, M1-45, M5-02 | SB-463, SB-561 | every continue path through the one builder; release |
| W11.3 | Lifecycle robustness | S2.4, S4.1..S4.6 | M1-02, M1-08..20, M1-24..35, M1-37..41, M1-43, M1-44, M2-15 | SB-562 | merged; release |
| W11.4 | Release pipeline | S5.1 | M4-01..09, M4-27 | SB-563 | a release run that proves the feed serves its tag |
| W11.5 | Fleet updates | S5.2..S5.6 | M4-10..24, M4-26, M4-28, M4-29 | SB-564 | every machine shows its version against the feed; `remote update` works on the Studio spaces |
| W11.6 | Remote machines and acts | S2.1, S2.3, S6.1..S6.4 | M2-03..09, M2-11..14, M2-16..20, M2-22, M2-24, M2-25, M3-11, M3-12 | SB-523, SB-524, SB-565 | pause, restart, continue, output and typing on the Studio from the MacBook in the CLI, MCP and window |
| W11.7 | Hanging PRs | S7.2 | M5-01, M5-03..13, M3-13, M3-20 | SB-418, SB-421, SB-566 | #821, #829, #832 merged or closed with reasons |
| W11.8 | Docs and the P3 sweep | S7.3 | M2-23, M3-14..16, M3-19, M4-25, M1-36, M1-42 | SB-567 | no open finding of this report without a row and an owner |

Order: W11.0 first (a release is blocked). W11.1 and W11.2 next, in parallel worktrees (different files: access doors and proxy vs resume builder). W11.3 after W11.2 (both touch `duty.rs` and `resumable.rs`). W11.4 any time (workflows only). W11.5 after W11.4. W11.6 after W11.1 (it needs the typed proof). W11.7 after W11.2 (#821 needs the account check in the builder). W11.8 last.

Every finding id of the report is in exactly one wave's Findings cell (M5-02 belongs to W11.2, which #821 then relies on). Each wave's PR names its finding ids in its body; the backlog row of a wave lists the ids it closes and is set to `released in X.Y.Z` in the release PR that ships it.
