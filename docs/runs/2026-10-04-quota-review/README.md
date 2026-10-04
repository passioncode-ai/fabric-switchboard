# System review and quota ordering — 2026-10-04

Objective: reliable account switching without losing sessions; make available quota and waits legible. Operator requests autonomous research, a single execution plan and parallel research agents. Uses the repository's four-stage pipeline.json; existing model inherited throughout. Source base: `46ee4b9` (fresh origin/main); 0.5.4 lifecycle already landed, released beta.2 is 0.5.3.

## Requirements and checks

| Requirement | Receipt |
|---|---|
| R1 inspect every current product/plan block and research developer threads | report section matrix and three source-grounded research packets |
| R2 one priority queue with cold-reader task context | canonical backlog and linked plan packets |
| R3 show accounts with fresh remaining quota first, constrained accounts by recovery time | deterministic UI logic tests; provider/pool boundaries retained |
| R4 date and remaining time on rows and windows | countdown arithmetic tests, browser demo at desktop/narrow widths |
| R5 unknown/stale/failed/sign-in/disabled remain distinct; expiry never invents new quota | edge fixtures and reset-boundary tests |
| R6 handoff available in Git | gate exit, pushed SHA, fresh checkout |

## Scope and decisions

Repository-owned source, scenarios, evidence, report and plan only. Synthetic credentials/data only. No real authentication, provider requests, running-session wake, release, install or other repository changes. Operator-assisted and release gates remain open. Existing canonical backlog remains the sole task-status register. The report is a dated cut; packets carry deps/evidence/resume. Unknowns do not become zero.

UI decision: preserve provider/pool grouping; fresh available rows first (more headroom first), then constrained rows with a known wait (earliest first), constrained rows without a known wait, unknown quota, sign-in-required, disabled. Blocked multi-window recovery uses the latest exhausted-window reset; any missing blocking reset makes it unknown. A runtime limit timestamp is a retry hold, not a promised quota reset. Existing PassionCode tokens/component layer; no animation or kit migration. Date stays visible, timer updates by wall clock at minute precision, pausable and stopped when hidden/off Accounts; no tick announcements or request/replay.

## Parallel coordination

Root owns UI source and shared register leases. `research_quota`, `research_sessions`, `research_system` own separate raw Markdown packets; after reproducing bounded defects they received disjoint core/runtime source scopes. Each sends an immediate message to root and affected siblings for findings affecting the current task, including evidence, confidence and a proposed contract change. Root reconciles findings before verification; peers do not edit shared registers. Local research messages are not external communication.

## Stages and resume

0 Research/specification: root judgment against source ledger (AGENTS, pipeline, HANDOFF, SPEC, CONTRACTS, PLAN-0.5, ACCOUNTS-AND-ROTATION, lifecycle, UX/brand, canonical backlog; prior RPT claude-swap-comparison as dated historical context).
1 Implementation: first bounded improvement R3–R5, fail-first tests.
2 Verification: repository gate and synthetic rendered acceptance; classify findings and external blind spots.
3 Git handoff: commit/push, verify SHA/fresh checkout, release leases. Resume via the final handoff and canonical plan links; do not infer released state from merged source.

Contradictions: SCN-031 claims work continues while SB-25 records stopped sessions; correct expectation and keep continuation as a deliberate future feature. Published 0.5.3 and main 0.5.4 differ intentionally. Mechanical audit's clean probes do not prove provider/native acceptance.
