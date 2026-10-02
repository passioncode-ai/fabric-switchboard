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
3. **A path every store, machine or account shares is a seam, not a detail.** A folder, a Keychain
   item or a log read by more than one store or machine gets an owner field and a rule for what
   each owner may delete, written in the spec before building. Source: run 2026-10-02 (backups).
   Retire after two runs adding shared paths pass the seam tier with no finding of this class.
4. **A call with an irreversible remote effect must outlive its caller.** Anything that spends a
   one-time credential (a refresh grant, a consumed code) runs in a task no deadline or dropped
   request can cancel, and records its result before returning; its custody is keyed by the
   thing spent, not by whoever asked. Name both in the spec's failure column. Source: run
   2026-10-03. Retire after two runs adding such calls pass review with no finding of this class.

## Run stamps

| Date | Run | Commit | Diverged |
|---|---|---|---|
| 2026-10-02 | PLAN-0.5 prompt-free switching | `2cf0088` | yes — entries below |
| 2026-10-02 | PLAN-0.5 backups and limit errors, release v0.5.0-beta.1 | `e6c5e54` | yes — entry below |
| 2026-10-03 | PLAN-0.5 §0.5.1 Claude Swap parity and review fixes | `13a8712` | yes — entry below |

## Recent log

### 2026-10-03 — a token-spending call was cancellable, and its successor had one owner

- **Symptom:**
  - 133 runtime tests and the full gate were green at `dc17508`, yet the final review found two P1s
    (release-0.5 §0.5.1 R-1, R-2):
    - a grant whose response named another account dropped the successor;
    - a grant awaited inside a 25 s deadline (or a control request that disconnected) could be
      dropped after the provider had already rotated the token.
  - The same review found the successor keyed by one account id, so a second copy holding the same
    spent token could never adopt it.
- **Surfaced at:** stage 5, independent review. **Owned by:** stage 3 spec. The renewal contract
  said what happens when *storing* fails, but not when the caller stops waiting, and not whose
  successor it is when several accounts hold one token.
- **Root cause:** an irreversible remote effect (the grant spends the old token) was modelled as an
  ordinary async call, and the token's custody was modelled per account rather than per token.
- **Fix:** a code change, with three planted-defect checks:
  - the grant runs in its own task and records the successor before returning;
  - the successor is keyed by the spent token;
  - an owner named by the token endpoint routes it.
- **Catches it next time:** standing instruction 4.

### Standing-instruction prune (2026-10-03)

- Instruction 2 held: the stage-0 fetch found no unread remote commits. That is 1 of the 3 runs it
  needs to retire.
- Instructions 1 and 3 stay.

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

### 2026-10-02 — shared backup folder and partial copies, again past a green gate

- **Symptom:** 175 green tests, yet seam review 2 found that stores shared one backup folder and
  pruned each other, partial backups pushed out the complete one, a second restore changed
  accounts without identity, and a restart charged the previous account's limit to the new one
  ([release-0.5](release-0.5.md#backups-and-limit-errors-second-operator-request-2026-10-02)).
- **Surfaced at:** stage 5 review. **Owned by:** stage 3 spec — the folder, the key and the
  transcripts were specified as if one store and one account existed.
- **Fix (grade: structural):** owner (store and key) in every backup, prune only own files and keep
  the most complete, token matching on restore, journal-dated attribution; each with a test
  watched failing against the planted defect.
- **Check next time:** standing instruction 3.
