# Retrospective

## Standing instructions (at most ten; each with its retirement trigger)

1. **A new state that gates an action is enforced where the action is, not where the state is
   born.** When a change introduces a condition that must block an operation (a dead lineage, a
   missing key), list every caller of that operation in the spec's failure-behaviour column before
   building, and put the guard in the shared function they all reach. Source: run 2026-10-02.
   Retire when two consecutive runs that add such a state pass the seam tier with no finding of
   this class.
2. **Fetch and read the remote integration branch's HANDOFF before the grill.** A local `main`
   is a snapshot; a parallel run may have released since. Source: run 2026-10-02. Retire after
   three runs whose stage-0 fetch found no unread remote commits.

## Run stamps

| Date | Run | Commit | Diverged |
|---|---|---|---|
| 2026-10-02 | PLAN-0.5 prompt-free switching | `2cf0088` | yes — entry below |

## Recent log

### 2026-10-02 — three breaks passed a green unit gate

- **Symptom:** 142 Rust tests and the full gate were green, yet the seam review found that a
  rejected refresh lineage (1) stalled the monitor's due queue and (2) could still be activated
  through rotation, project apply and the MCP switch, and that (3) the vault re-keyed over sealed
  files although PLAN-0.5 forbade it ([release-0.5](release-0.5.md#seam-review-and-its-fixes)).
- **Surfaced at:** stage 5 review (task-pipeline `verifier-seam`). **Owned by:** stage 3 spec —
  the contract named the failure behaviour of the new states for one caller only, and (3) was
  specified but never given a test.
- **Root cause:** the dead-lineage state lived in `refresh.rs`, and its guard was written at the
  one entry point being edited (`Operation::ActivateNative`) instead of in `activate_native`,
  which four callers share; the queue effect came from returning before the health write that
  every other failure path performs.
- **Fix (grade: structural):** the guard moved into `activate_native` for every caller; the dead
  path writes failed health; the vault refuses without its key. Each has a test, and the queue
  test was watched failing against a planted defect.
- **Check next time:** standing instruction 1, plus a seam-tier reading before the commit of any
  change that adds a gating state.

### 2026-10-02 — a parallel release changed the base under the run

- **Symptom:** at release time `origin/main` was 11 commits ahead (`1cfc793`), including
  0.4.1-beta.1 with a different Keychain vault already installed on the operator's Mac; the
  branch could not fast-forward and its vault (D-1) duplicated shipped work.
- **Surfaced at:** stage 7 (land on `main`). **Owned by:** stage 0 harvest — `git fetch` and the
  repository's handoff on the remote were not read at the start; the local `main` snapshot was
  taken as current.
- **Fix (grade: procedural):** rebased onto `1cfc793`, withdrew the file vault, kept KEYCHAIN.md.
- **Check next time:** standing instruction 2.
