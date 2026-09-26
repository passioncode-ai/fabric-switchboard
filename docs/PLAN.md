# Execution plan
Selected pipeline profile: research/specification → implementation → verification → repository handoff. Scope, evidence, dependencies, and resume are retained in every packet. Operator requested autonomous execution; no routine design/model reapproval.

| Order | Module / packet | Scope | Depends on / carries | Gate | State |
|---|---|---|---|---|---|
| 1 | Research and contracts | docs/ | four pinned source trees | every technical claim has source | in progress |
| 2 | Account workbench core | crates/switchboard-core | CONTRACTS.md: account/vault/route types | deterministic isolated tests | planned |
| 3 | Desktop interface | src/ and frontend config | CONTRACTS.md IPC and ux/scenarios.md | typecheck, build, interaction | planned |
| 4 | Managed routing | crates/switchboard-proxy | core route snapshot | auth + in-flight switching tests | planned |
| 5 | Native integration | src-tauri/ | core/proxy/IPC | native bundle; login/launch fixtures | planned |
| 6 | Review and handoff | scripts/, docs/evidence | all integrated modules | focused checks, fresh clone, push | planned |

Build agents own separate worktrees. Root integrates explicit commits and examines convergence. Research source trees remain outside product repository and are never shipped as dependencies. No integration into Observatory or Fabric parent repositories.

Release horizons: 0.1 local manual workbench/managed routes; 0.2 supervised sessions and Desktop handoff; 0.3 Windows native acceptance and signed distribution. These are milestones, not implied completed work. Exact follow-on requirements and checks are in SPEC.md §§9,12–14.
