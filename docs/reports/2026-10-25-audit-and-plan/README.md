---
report:
  id: fabric-switchboard/2026-10-25-audit-and-plan
  title: "Switchboard after 0.6.82: audit of open work, cleanup, and the plan to a first organization"
  kind: audit
  project: fabric-switchboard
  domains: [planning, organizations, reliability, backlog]
  as_of: 2026-10-25
  status: active
  valid_until: 2026-11-15
  summary: >-
    One place for everything open in Switchboard on 2026-10-25: the board (50 open rows), the Fabric
    project board (380 messages read), branches, worktrees and stashes, and the first-organization rollout
    accepted in Vesta on 2026-10-23. Finished leftovers were removed. The owner still stalls on 0.6.82
    (control_slow up to 38 s), so a hotfix comes first. Work is split into hotfixes, the first-organization
    release, the product plan after it, and the long backlog, with the questions the operator must answer.
  sources:
    - {name: "Board", path: "docs/evidence/backlog.md", read_at: 2026-10-25}
    - {name: "HANDOFF", path: "docs/HANDOFF.md", read_at: 2026-10-25}
    - {name: "0.6.82 iteration canvas", path: "docs/runs/2026-10-24-iteration-ui-ux/canvas.md", read_at: 2026-10-25}
    - {name: "Organizations design", path: "docs/research/organizations-2026-10-21.md", read_at: 2026-10-25}
    - {name: "Two machines design", path: "docs/research/two-machines-2026-10-23.md", read_at: 2026-10-25}
    - {name: "Session home per context", path: "docs/research/session-home-per-context-2026-10-24.md", read_at: 2026-10-25}
    - {name: "Vesta rollout: the operator's two Macs and Appvillis (accepted 2026-10-23)", url: "https://github.com/passioncode-ai/passioncode-vesta/blob/main/docs/design/2026-10-23-rollout-estate-and-appvillis.md", read_at: 2026-10-25}
    - {name: "Organization roadmap (RM-21, RM-26, RM-28, RM-34)", url: "https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/roadmap.md", read_at: 2026-10-25}
    - {name: "switchboard.log on the operator's Mac, 0.6.82 from 2026-10-24 21:59 UTC", path: "~/Library/Logs/Fabric Switchboard/switchboard.log (local, not in Git)", read_at: 2026-10-25}
---

# Switchboard after 0.6.82: audit and plan

The operator's ask (2026-10-25): see what other sessions did here, what is on the board, what debts
exist, what is in worktrees and branches; remove what is finished; bring what is unfinished or
abandoned into one place; split the tasks into priorities and hotfixes, a product plan for the
rollout to the first organization, later plans and the backlog; then work it through, record
decisions, and merge it into one finished release we integrate.

This page is that one place. Task status stays in [the board](../../evidence/backlog.md); this page
classifies it and says what each group waits on.

## 1. What was found

### Repository, branches, worktrees

| Item | State on 2026-10-25 | Evidence |
|---|---|---|
| `main` | `1494fec`, clean, equal to `origin/main` | `git status -sb`, `git branch -vv` |
| Open pull requests | none | `gh pr list --state open` |
| Remote branches | `main` only (24 merged branches removed by the 0.6.82 session) | `git branch -r` after `fetch --prune` |
| Worktrees | only the main clone (the iteration's six worktrees were removed after `50ce27e`) | `git worktree list` |
| Stash | one entry `WIP on fix/sb-185-vault-lock`, byte-identical to what landed in `ebbf95e` (0.6.81): **dropped** | `git diff stash@{0} ebbf95e -- crates/switchboard-core/` printed 0 lines |
| Leftovers in `~/DATA/_worktrees` | seven `sb-*-check.log` gate logs of merged runs (10-22, 10-23) and `.sb-snapshots/before-{7,8}.json` (group snapshots of 10-17): **removed** | `rm -v` |
| Agent-sync leases | none | `git ls-remote origin 'refs/agent-sync/leases/*'` |
| CI | nightly `main` green on 2026-10-24 22:45 and 23:50 UTC; release runs 0.6.80 to 0.6.82 green | `gh run list` |
| Installed | app and CLI 0.6.82 | `switchboard --version`, `CFBundleShortVersionString` |

### Issues and Fabric tasks

| Item | Action |
|---|---|
| Issue #128 (analytics consent) | done in 0.6.82 (N6): **closed** with the receipt |
| Issue #36 (COM-11/12 provenance) | open, is SB-47 |
| Fabric task «Убрать зависания … 0.6.81» | released; moved to review (the operator closes it) |
| Fabric tasks SB-108, SB-114, SB-115 (in review since 10-11 to 10-13) | released long ago; the operator closes them, live checks stay in the board rows |

### The owner still stalls on 0.6.82 (the finding that reorders the plan)

The HANDOFF's next task was to read `control_slow` and `vault_slow` after a day on 0.6.81/0.6.82 and
close SB-174 and SB-163 if quiet. It is not quiet. From 0.6.82's start (2026-10-24 21:59:01 UTC) to
2026-10-25 11:40 UTC:

| Event | Count | Longest |
|---|---|---|
| `control_slow` `project_suggest` | 19 | 22.5 s |
| `control_slow` `sessions` | 12 | 36.9 s |
| `control_slow` `recovery` | 7 | 29.1 s |
| `control_slow` `current_accounts` | 1 | 37.8 s |
| `control_slow` `openrouter_models` | 1 | 10.7 s |
| `control_slow` `snapshot` | 1 | 5.6 s |
| `vault_slow` | 4 | 11.9 s |
| `transaction_held` | 0 | |

`project_suggest` is the session-start hook's call that SB-186 (0.6.81) made skip the transaction, and
no `transaction_held` was logged, so the stall is not the owner transaction. A single vault read of
11.9 s exceeds the 5 s bound of one `/usr/bin/security` call, so something waits on several reads in
a row or on a lock outside the store. SB-174 and SB-163 stay open; this is hotfix H-1 below.

### The Fabric project board

380 messages read (285 unread and everything since 2026-10-20), nothing acknowledged. Open for
Switchboard:

- **Unanswered ask** `9c0e7327` (thread `27a35c9e`, traffic-growth-agent, 2026-10-25): the operator's
  «закрой все долги», reply once with closed or still open. Switchboard has not replied.
- **Unanswered ask** `0fa67613` (thread `bb87af23`, Fabric 0.3.7 released): reply adapted, row or not
  affected. Minor.
- **Our unanswered questions:** Q-1 to Fabric `1b623476` (thread `1ddb20ce`: will Fabric take
  hand-overs and limit stops into its sessions, from which release); the context-profile owner
  `158bba09` (thread `12bd8b4c`: fabric-workspace said not the owner and pointed at Observatory, which
  has not answered; blocks SB-184).
- **Gated adoption:** DEC-0121 action parity, SB-200, after CT-124 and CT-125.
- **Not tracked:** DEC-0096 (optional `extensions/connections/0.1`); Switchboard declares no request
  capability and cannot `com.enroll` without the operator's `board.respond` grant.
- The three findings Dashboards filed (SB-174, SB-166, SB-181) are tracked; two are done.

About 285 of the 380 messages are other projects' releases and roll calls that do not touch Switchboard.

## 2. The first organization

The sources agree on the order (Vesta rollout, accepted by the operator 2026-10-23, D-R4):

1. **Wave 0 and 1: PassionCode.ai on the operator's two Macs** (Mac Studio holds the hub, MacBook `full`),
   collection proven on ourselves.
2. **Wave 2: Appvillis, a pilot on 2 or 3 of its people's Macs** (owner `sergey@appvillis.com`), then
   **Wave 3**, the rest of about 10 Macs.

macOS only; Windows, Linux and Appvillis cloud machines are outside this rollout. Each machine runs one
Switchboard group per organization whose folders take only that organization's accounts (SB-109).

### What Switchboard already ships for it

| Capability | Release | Row |
|---|---|---|
| Local organizations; accounts attributed by person, provider team or e-mail domain | 0.6.61 | SB-158 |
| A group bound to an organization refuses personal and other organizations' accounts; statistics by organization; organizations in backups | 0.6.65 | SB-109 |
| Link to PassionCode.ai/Vesta (PKCE sign-in, re-read every 6 h; names and domains, no members) | 0.6.66 | SB-159 (live check open) |
| A group whose accounts all left its organization stays closed | 0.6.72 | SB-170 |
| An account marked "used on another machine" | 0.6.74 | SB-175 |
| Settings bundle export and import between machines, no credentials | 0.6.75 | SB-176 |
| Every launched Claude Code and Codex session records its context (`SWITCHBOARD_CONTEXT`), badge on Activity | 0.6.80 | SB-110 phase 1, SB-111 |
| Codex sessions in group folders use the group's Codex accounts (off by default, never run live) | 0.6.49 | SB-143 |

### What blocks or weakens the rollout

| Gap | Wave | Row | Waits on |
|---|---|---|---|
| Plain `claude` / `codex` is not bound to the organization: work must be launched through Switchboard | 2 | SB-183 (due 2026-11-14) | nothing; design settled 2026-10-24 |
| Kimi Code, Hermes and OpenCode sessions carry no context | 2 | **untracked**, new row | nothing |
| Per-organization git, SSH, `gh`, MCP and plugin profile | 2 | SB-184 | an owner (Observatory silent on `12bd8b4c`) |
| Live link to Vesta | 2 | SB-159 | one operator sign-in (`switchboard org connect`) |
| One Claude account on two Macs: does one Mac's renewal sign the other out | 0 | SB-178 | operator and a spare account on both Macs |
| Automatic "held elsewhere" across the estate | 0 | SB-177 | Fabric CO-412 (Fabric 0.4.0, undated) |
| Organization OpenRouter key lands in the group's key slot | 2 | SB-167 (by 2026-11-14) | Observatory 0.30.0 (2026-11-07) |
| Joining and leaving: Switchboard's part of the onboarding playbook | 3 | SB-160 | Vesta playbook evals |
| Session end and quota history per account for the assessment agent | later | **untracked** (RM-34 AS-04) | nothing |
| A short runbook for an organization's person: install, group, seats, connect, launch | 2 | **untracked**, new row | nothing |
| Codex in an organization group, live | 2 | SB-143 | a group with a Codex account |
| Live acceptance on the operator's Mac (renewal, limit switch, a day of use) | 0 | SB-01, SB-15 | operator accounts |
| Which accounts and which group carry Serge Khurs's work | 2 | SB-125 | operator decision |

## 3. Every open task, classified

### A. Hotfixes and correctness, before anything else (release 0.6.83)

| # | Row | P | Why now |
|---|---|---|---|
| H-1 | SB-174, SB-163 | P1 | Owner answers stall up to 38 s on 0.6.82 (table above); the session-start hook and Dashboards time out |
| H-2 | SB-198 | P2 | `recovery claim` grants a second claim on a claimed stop: two hosts continue one conversation |
| H-3 | SB-196 | P2 | A hand-over leaves the stop `waiting`: the same conversation can be continued twice |
| H-4 | SB-199 | P2 | Stale `waiting` stops kept 7 days and re-attempted |
| H-5 | SB-191 | P2 | 12 update restarts in 48 h interrupt managed sessions |
| H-6 | SB-07 | P2 | Two flaky integration tests in the gate |
| H-7 | board | P3 | Reply to `9c0e7327` (still open) and `0fa67613` |

### B. Release «first organization» (target 0.7.0, by 2026-11-14)

The Switchboard side of Vesta waves 0 to 2, as one release:

1. SB-183: the opt-in shell hook, so plain `claude` and `codex` in an organization's folders run in that
   organization's context.
2. New row: context for Kimi Code, Hermes and OpenCode launches.
3. SB-167: the organization's OpenRouter key into the group's slot (after Observatory 0.30.0).
4. SB-159, SB-178, SB-143, SB-15: live checks on the two Macs (operator gates named in section 5).
5. New row: the person's runbook (docs, the window's first run for an organization context).
6. SB-160: Switchboard's part of joining and leaving (attribution, signed-out state).
7. SB-184 if an owner is decided (see question Q-4), else the rollout notice keeps saying the profile is shared.
8. SB-177 only if Fabric CO-412 lands before the release; otherwise the manual mark (SB-175) holds.

### C. Product plan after the first organization

| Theme | Rows | Waits on |
|---|---|---|
| Fallback chains in the window, scenario first | SB-71 (UI), SB-73 phase 2 | nothing |
| Paid fallback with ceilings and grants | SB-72, SB-96, SB-97 | Observatory grant registry |
| Pipelines and continuation over MCP | SB-64, SB-74, SB-82 | Fabric's ADR; contract runner routes |
| Action parity with the contract | SB-200 | CT-124, CT-125 |
| Folder attribution with Fabric's project id | SB-141 | CT-64, CT-66 |
| Responder provenance | SB-47 (issue #36) | Fabric COM-11 |
| Rights per group and agent | SB-124 | operator decision |
| Silent sessions, dynamic risk at hand-over | SB-116, SB-123 | Fabric session host |
| One error text for every surface | SB-189 | nothing |
| Window leftovers | SB-192, SB-193, SB-195 | nothing |
| Codex renewal of inactive accounts | SB-16 | operator go and an operator-assisted acceptance |
| Assessment data (session end, quota history) | new row | nothing |

### D. Long backlog and hygiene

SB-02 (Windows acceptance), SB-06 (activation without restart, measurement), SB-13 (data protection
keychain), SB-32 and SB-92 (Windows signing), SB-58 (upstream Observatory), SB-91 (download page),
SB-148 (data palette), SB-194 (Fabric ADR-0127 amendment), SB-197 (`sessions --all` identity).

### E. Rows to close or split (no new work)

| Row | Why |
|---|---|
| SB-190 | superseded by the operator's decision of 2026-10-24 (sessions live in Dashboards, then Fabric) |
| SB-65 | the window's Sessions view is superseded the same way; the done parts have receipts |
| SB-130, SB-138, SB-139, SB-115 | fixed and released; only a live check or a separate tail remains: close and keep the tail as its own row |
| SB-168 | design done; built in SB-175 to SB-178 |
| SB-176 | built (CLI carries it; window import deliberately not built) |
| SB-126 | handed to Fabric |
| SB-114 | producer released; host integration is Fabric's (FD-105, Q-1) |

## 4. One finished release

"Merge everything into one finished project we integrate" maps to two releases, because one of them
cannot wait: **0.6.83** (section A, days) and **0.7.0 «first organization»** (section B, by 2026-11-14,
the date SB-183 and Vesta's wave 2 already carry). Each runs through `/task-pipeline iteration` with
worktrees per node, as 0.6.82 did, and ends at the published release, the local install and a board
with section E closed.

## 5. Human gates (no route avoids them)

| Gate | For | How |
|---|---|---|
| One sign-in to PassionCode.ai | SB-159 live check | `switchboard org connect` or Organizations → *Connect PassionCode.ai* |
| A spare Claude account signed in on both Macs | SB-178 | operator at both machines |
| Seats for the Mac Studio and for Appvillis people | wave 0, wave 2 | a spending decision |
| `board.respond` grant for Switchboard | answering board requests as a responder | Fabric, Settings → Agent access |
| Closing Fabric tasks in review | board hygiene | the operator in Fabric |

## 6. Questions for the operator

Asked in the session of 2026-10-25; answers are recorded in section 7 when given.

- **Q-1.** Two releases (0.6.83 hotfix, then 0.7.0 first organization) or one?
- **Q-2.** Which organization does Switchboard accept first: PassionCode.ai on the two Macs (Vesta D-R4), or straight to the Appvillis pilot?
- **Q-3.** The shell hook (SB-183): opt-in per machine, or on by default on a machine that holds an organization group?
- **Q-4.** The per-organization git/SSH/gh/MCP profile (SB-184): Switchboard owns it, wait for Observatory, or out of 0.7.0?
- **Q-5.** Fallback chains and paid ceilings (SB-71 UI, SB-72): in 0.7.0 or after?
- **Q-6.** Which accounts and group carry Serge Khurs's work (SB-125)?

## 7. Decisions

None yet.
