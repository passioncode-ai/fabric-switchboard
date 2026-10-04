---
report:
  id: fabric-switchboard/2026-10-04-system-review
  title: "Switchboard system review: quota, accounts and continuity"
  kind: audit
  project: fabric-switchboard
  domains: [accounts, auth, security]
  as_of: 2026-10-04
  status: active
  valid_until: 2026-11-04
  summary: >-
    Current source and all product/plan sections were reviewed against developer reports and primary sources.
    Partial headers could freshen unobserved quota, same-second backups could replace full copies,
    and ordinary JSON content could hide a session timestamp. Bounded repairs accompany quota ordering
    and date/countdown UI. Provider/native acceptance and session wake remain separate open work.
  sources:
    - {name: "Source baseline 46ee4b9", url: "https://github.com/passioncode-ai/fabric-switchboard/tree/46ee4b9a249b3da9612e7ca40308ea12178b92a6", read_at: 2026-10-04}
    - {name: "Quota research and official/developer sources", path: "docs/reports/2026-10-04-system-review/raw/quota.md", read_at: 2026-10-04}
    - {name: "Session research and official/developer sources", path: "docs/reports/2026-10-04-system-review/raw/sessions.md", read_at: 2026-10-04}
    - {name: "System research and release receipts", path: "docs/reports/2026-10-04-system-review/raw/system.md", read_at: 2026-10-04}
  produced_by: {agent: codex, task: quota-order-countdown}
  supersedes: []
  consumers: [docs/evidence/backlog.md, docs/runs/2026-10-04-quota-review/PLAN.md]
---

<sub>ssheleg skills — task-pipeline · project-audit · agent-sync · ux-scenarios · sheleg-design · copywriting · evidence-docs · project-reports</sub>

# System review — 2026-10-04

## Main result

The priority is reliable local work under the deliberately selected identity (SPEC §3). Requested improvement: accounts with fresh remaining quota first, constrained accounts by shortest known wait; absolute reset date plus days/hours/minutes. Provider/pool groups remain boundaries. Unknown/stale/failed evidence never becomes zero or a claimed usable account.

Source is 0.5.4 with lifecycle already landed; published/installed beta.2 is 0.5.3. Release API success was read, but artifacts were not downloaded or reinstalled by this run. [Release receipt](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37157469157). This review does not approve release SB-34.

Three reproduced source defects were repaired alongside the UI: historical quota freshness, backup replacement within one second, and timestamp misattribution caused by ordinary content materialization. [Verification](../../evidence/quota-order-2026-10-04.md) separates fail-first local checks, browser inspection, independent review and external acceptance.

## Section-by-section coverage

| Product/plan block | Evidence and conclusion | Plan home |
|---|---|---|
| Goals, boundaries, account architecture (SPEC1–3, PLAN0.5 D-1…D-3) | Provider/pool/immutable identity and no-replay are sound local boundaries. Compact rows previously ranked by creation date, obscuring quota. | SB-35, [brief/plan](../../runs/2026-10-04-quota-review/PLAN.md) |
| Login, capture/import, renewal (SPEC4, PLAN0.5 0.5.1/0.5.2/0.5.3) | Scoped homes, active exclusion, lineage and saved successor have fixtures; external refresh races support retaining one spender. Old documentation understated renewal and misstated cleanup retry. | [SYS-LOGIN and token custody](raw/system.md), SB-15/16/20/42 |
| Vault, metadata, backup (SPEC5, PLAN0.5 D-5) | Local rollback/private filesystem tests exist. Same-second filenames could destroy complete/other-store backups, reproduced with MemoryVault and fixed. Cross-device portability intentionally absent. | [SYS-BACKUP](raw/system.md), SB-37, SB-13/29 |
| Managed selection/proxy (SPEC6–7, 0.5.1 proxy fixes) | Captured request identity and no automatic replay retained; stable port/capability already landed. No HTTP/SSE result proves hot-native adoption. | [system matrix](raw/system.md), SB-06/25 |
| Homes and native limits (SPEC8, PLAN0.5 D-6) | Access-only snapshots remain deliberate. Timestamp parser was affected by ordinary JSON content and fixed. 60s grace is heuristic; fixed ~30s adoption claim corrected. | [sessions](raw/sessions.md), SB-38/41/25 |
| Quota/reset/rotation (SPEC9, PLAN0.5 D-2/D-6) | Multiple windows require latest exhausted reset; inferred holds are distinct. Partial headers falsely freshened low weekly/model windows; conservative evidence age repairs this, with authoritative JSON recovery. | [quota](raw/quota.md), SB-36/39/40, SB-29 |
| Accessibility/visual/copy/privacy (SPEC10–11) | Existing tokens and component layer retained. Visible date+duration, pause, no tick announcements/focus moves. Browser/keyboard inspection scoped; screen reader/native acceptance absent. | SCN-032, [evidence](../../evidence/quota-order-2026-10-04.md) |
| Windows (SPEC12) | CI-native fixtures/NSIS != interactive install/provider acceptance. CLI packaging, signatures and uninstaller signing remain distinct. | [SYS-WINDOWS](raw/system.md), SB-02/03/05/32 |
| Tests, CI and release (SPEC13, PLAN0.5 released sections) | Scheduled job receipt resolves SB-11 condition; full suites stay nightly. This branch has local gate only; 0.5.4 release still open. | SYS-RELEASE, SB-34/31 |
| Follow-on and known limits (SPEC14–15) | Residency/home-sharing/export/continuity are decisions or deliberate next features, not hidden completed work. Stale refresh/startup/Finish claims corrected in current spec/runbook. | SYS-RESIDENCY/RECOVERY, SB-28/29/30/20 |
| CLI/owner/MCP/rules (SPEC16 and 0.4 carry-over) | Bounded owner protocol and tool authority retained. Legacy MCP fixtures do not prove modern real-client compatibility. Historic packet and project-rule flaky receipt remain reconciliation work. | SYS-MCP, SB-07/08/43 |

Every current canonical row is mapped in [the unified plan](../../runs/2026-10-04-quota-review/PLAN.md); section packets carry exact scopes, inputs, dependencies, edge cases, checks and resume tasks. The board is the sole editable status register.

## Method, source weight and limits

Three parallel agents owned separate section packets and sent blocking findings immediately to root and affected peers. Root implemented the user-facing change; peers repaired bounded source defects, then an independent session reviewer checked convergence. Collaboration messages are the local coordination channel, not authorization to send external messages or alter running provider sessions.

Forum/GitHub issue observations are reproduction leads, not current incidence proof. Technical choices are supported by primary source code/docs/RFCs and local synthetic regressions: [RFC9700 rotating-token security](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14), [RFC9110 Retry-After](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after), [Claude Code resume documentation](https://code.claude.com/docs/en/cli-reference), [OpenAI Codex backend source](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs). All discovery threads and source-specific cautions are in the raw packets; no quotes/credentials/provider bodies were captured.

Mechanical [collector sidecar](raw/collector/sanitized-audit.json): seven probes ran, two blind, zero pattern findings. Both root fields were normalized to repository-relative '.' and the sidecar path names this saved copy before Git; no machine paths or secrets included. Collector reads tracked/committed state and sampled history, so its clean verdict does not measure UI, provider, native vault or runtime correctness. Its telemetry "library/tool" classification is insufficient for this released desktop; no production incidence measurement was taken. Manual findings above are separate and do not overwrite the original clean probe meaning.

The [self-contained collector HTML](raw/collector/sanitized-audit.html) was generated from that
sidecar with the skill's renderer and an explicit manual-review pointer. Browser open/render
acceptance is NOT_RUN (`--no-open` behavior) because the operator was using the browser.

## Design and executable next work

[Unified plan](../../runs/2026-10-04-quota-review/PLAN.md) is the single order/dependency/context entry. First next autonomous implementation: SB-39 (enforce provider retry deadline for every caller, seconds and HTTP-date parsing). Session continuation follows typed identity/scope evidence and a versioned live adoption/resume probe; generic 429 does not prove an account-scoped quota. Release/live/Windows/decision gates remain explicit.

## Skills actually used

`project-audit` collected profile/probes; `task-pipeline` selected the repository's four-stage profile and persisted the requirement/plan/handoff; `agent-sync` claimed task/shared files and recorded convergence; `ux-scenarios` defined SCN-032 and corrected SCN-031; `sheleg-design` retained token hierarchy and checked date/timer layout; `copywriting` supplied bounded operational labels and own humanization pass; `evidence-docs` separated test/browser/live claims; `project-reports` created this dated repository report and wiki index. No foreign planning entry or agent-stack implementation was used; MCP changes remain future work.

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**

- [`task-pipeline`](https://github.com/ssheleg/task-pipeline) — единый план и проверка
- [`project-audit`](https://github.com/ssheleg/task-pipeline) — диагностика проекта
- [`agent-sync`](https://github.com/ssheleg/agent-sync) — владение общими файлами
- [`ux-scenarios`](https://github.com/ssheleg/super-ux) — сценарии списка
- [`sheleg-design`](https://github.com/ssheleg/sheleg-design-skill) — дата и таймер
- [`copywriting`](https://github.com/ssheleg/super-ux) — подписи
- [`evidence-docs`](https://github.com/ssheleg/task-pipeline) — проверяемые выводы
- `project-reports` — отчет в Git — not a skill this family ships

<sub>A star on [the bundle](https://github.com/ssheleg/sshlg-skills) helps.</sub>
