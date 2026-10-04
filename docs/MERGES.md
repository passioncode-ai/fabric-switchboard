<!-- agent-sync:merge-log -->

# Merge log

Written by `agent_sync.py merge`. Entries newer than 7 days keep their detail; older ones are compacted to one line each on the next write. Read it before starting work: it is the shortest answer to *what landed while I was on my branch*.

### 2026-10-04T06:42:40Z · `lifecycle-0.5.4` · claude/lifecycle-contract → main · `f0b8d7f` (PR #23)
- run: r-3439612a8
- files: 44 (44 files changed, 3052 insertions(+), 334 deletions(-))
- conflicts: 3, resolved in `45bbdbf` — `scripts/check.sh` (both test lines kept), `docs/evidence/backlog.md` (main's board kept; the branch's SB-25…27 are SB-27…29), `docs/HANDOFF.md` (0.5.4 section above main's 0.5.3-beta.2)
- summary: 0.5.4 lifecycle contract — no Keychain read on a timer, idle cadence, stable proxy, drain on quit, bounded log; SB-23

### 2026-10-03T10:51:03Z · `switchboard-0.5.3-board` · agent/switchboard-0.5.3-board → main · `a8936c2`
- run: r-74550f2ee
- files: 2 (2 files changed, 24 insertions(+), 14 deletions(-))
- conflicts: none
- summary: SB-24 done; 0.5.3 handoff

### 2026-10-03T10:38:02Z · `switchboard-0.5.3-record` · agent/switchboard-0.5.3-record → main · `fbd9a44`
- run: r-74550f2ee
- files: 4 (4 files changed, 130 insertions(+))
- conflicts: none
- summary: v0.5.3-beta.1 release record

### 2026-10-03T09:52:40Z · `switchboard-0.5.3` · agent/switchboard-0.5.3 → main · `6c6dcbf`
- run: r-74550f2ee
- files: 34 (34 files changed, 2611 insertions(+), 315 deletions(-))
- conflicts: none
- summary: 0.5.3: token custody beside Claude Swap, quiet desktop; audits + review fixed

### 2026-10-03T01:14:37Z · `switchboard-0.5.2-record` · agent/switchboard-0.5.2-record → main · `641ab27`
- run: r-74550f2ee
- files: 8 (8 files changed, 142 insertions(+), 4 deletions(-))
- conflicts: none
- summary: v0.5.2-beta.1 release record, SB-21/22, agent-sync guard fix

### 2026-10-02T23:48:36Z · `switchboard-0.5.2` · agent/switchboard-0.5.2 → main · `dda7e0e`
- run: r-47aaa5aa6
- files: 21 (21 files changed, 227 insertions(+), 29 deletions(-))
- conflicts: none
- summary: 0.5.2: invalid_client hold, created config skips onboarding; report rows 16,19-22 closed; PR #21

### 2026-10-02T23:38:30Z · `switchboard-0.5.1-record` · agent/switchboard-0.5.1-record → main · `d34d15b`
- run: r-47aaa5aa6
- files: 7 (7 files changed, 160 insertions(+), 12 deletions(-))
- conflicts: none
- summary: v0.5.1-beta.1 release record, receipts, board SB-19, handoff

### 2026-10-02T23:16:14Z · `switchboard-0.5.1` · agent/switchboard-0.5.1 → main · `af1211d`
- run: r-47aaa5aa6
- files: 41 (41 files changed, 5065 insertions(+), 280 deletions(-))
- conflicts: none
- summary: 0.5.1: Claude Swap parity (MCP keys, outgoing generation under locks, lineage, cancellation-safe grants, owner routing, Claude Swap coexistence, stale locks, idle live renewal) and review fixes; PR #20

### 2026-10-02T22:01:42Z · `switchboard-0.5.0-record` · agent/switchboard-0.5.0-record → main · `8ce6c9e`
- run: r-47aaa5aa6
- files: 8 (8 files changed, 157 insertions(+), 7 deletions(-))
- conflicts: none
- summary: v0.5.0-beta.1 release record and handoff

### 2026-10-02T21:48:55Z · `switchboard-0.5-backup` · agent/switchboard-0.5-backup → main · `03a375f`
- run: r-47aaa5aa6
- files: 30 (30 files changed, 1982 insertions(+), 29 deletions(-))
- conflicts: none
- summary: 0.5.0: automatic encrypted backups, switching on provider limit errors (PR #19)

### 2026-10-02T21:05:18Z · `switchboard-0.5` · agent/switchboard-0.5 → main · `23e11a9`
- run: r-47aaa5aa6
- files: 53 (53 files changed, 2410 insertions(+), 473 deletions(-))
- conflicts: none
- summary: 0.5.0: renewal of inactive Claude accounts, Claude item via /usr/bin/security, one-click accounts (PR #18)

## Compacted

_nothing older than the window yet_
