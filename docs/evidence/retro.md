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

5. **Taking an operation out of a shared lock is a contract change for everyone who waited on
   that lock.** Before building, list each waiter (the quit drain, other operations, the
   background) and what it relied on the lock to guarantee, in the spec's failure column. Source:
   run 2026-10-04 (SB-39: the drain stopped covering a manual quota check that could still spend
   a refresh token). Retire after two runs that change lock scope pass the seam tier with no
   finding of this class.
6. **A test that fails once and passes on rerun is a product race until shown otherwise.**
   Read the failure for its mechanism before calling it flaky. Source: run 2026-10-04 (SB-44: a
   "flaky" shutdown test was a real signal-handler race at start-up). Retire after three runs in
   which every intermittent failure met was traced to its mechanism without this reminder.

7. **An open question on the board is never an invariant in code.** When a design depends on
   something the board lists as unverified (SB-06-style), the spec states both answers and the
   code takes the branch that stays correct under either; a bound the code must respect gets its
   worst-case sum written in the spec. Source: runs 2026-10-04 (SB-40, SB-41). Retire after two
   runs touching unverified behaviour pass review with no finding of this class.

8. **Before a release, the other platform is compiled.** `cargo xwin check` and `clippy` for
   `x86_64-pc-windows-msvc` (core, proxy, runtime, cli) run before a tag when the nightly has not
   covered the merged changes; every script a release job runs reads text as UTF-8. Source: run
   2026-10-05 (two release runs stopped in the Windows job). Retire after three releases whose
   first run succeeds on both platforms.

## Run stamps

| Date | Run | Commit | Diverged |
|---|---|---|---|
| 2026-10-02 | PLAN-0.5 prompt-free switching | `2cf0088` | yes — entries below |
| 2026-10-02 | PLAN-0.5 backups and limit errors, release v0.5.0-beta.1 | `e6c5e54` | yes — entry below |
| 2026-10-03 | PLAN-0.5 §0.5.1 Claude Swap parity and review fixes | `13a8712` | yes — entry below |
| 2026-10-04 | SB-39 provider not-before for every quota check (+ SB-44) | `6d67fbe` | yes — entry below |
| 2026-10-04 | SB-40 Codex limits beyond the two windows | `585c255` | yes — entry below |
| 2026-10-04 | SB-41 typed, attributable limit evidence | `64068c0` | yes — entry below |
| 2026-10-05 | SB-30, SB-43, SB-42, SB-46, SB-29, release v0.5.4-beta.3 | `d918734` | yes — entry below |

## Recent log

### 2026-10-05 — the release found what no gate had compiled

- **Symptom:** `v0.5.4-beta.1` stopped in the Windows preflight (`UnicodeDecodeError`: the notes
  read as cp1252) and `v0.5.4-beta.2` in the Windows fixtures (`E0433`: `external.rs` referred to
  the Unix-only `security_cli`, broken since #23). Neither published; `beta.3` succeeded.
- **Surfaced at:** stage 7, the release workflow. **Owned by:** stage 6 — no step between #23 and
  the release compiled the Windows target, and the nightly had not run on the merged changes.
- **Root cause:** a Unix-only module reached from shared code, and a release script relying on the
  runner's default encoding; the local gate runs on macOS only.
- **Fix:** code (#43, #45) with a locale test; Windows cross-check before tagging; standing
  instruction 8.
- **Also:** SB-30's first review proved "no focus" was assumed from the API, not measured — the
  measurement (`lsappinfo front` polling) showed Accessory still came forward; Prohibited fixed it.
  Standing instruction 7 covers this class (an unverified behaviour taken as fact).


### 2026-10-04 — SB-40 and SB-41: a bound that was not computed, a heuristic that froze an open question

- **Symptom:** green gates, then review findings: SB-40 could emit 17 windows to a store that
  accepts 16 (P2); SB-41's session binding assumed a running session never adopts a switch — the
  open SB-06 question, decided in the direction that disabled the feature (P1), and its hold was
  bounded from the pass clock, rewriting the file every pass (P2).
- **Surfaced at:** stage 5 review. **Owned by:** stage 3 spec — SB-40's spec named the 16-window
  limit but no arithmetic over the worst case; SB-41's spec stated "a session runs on one
  sign-in" as a fact although the board lists it as unverified.
- **Root cause:** a limit was quoted without its worst-case sum; an open question was encoded as
  an invariant instead of as the conservative branch.
- **Fix:** code with fail-first tests (run records); standing instruction 7.
- **Catches it next time:** standing instruction 7.


### 2026-10-04 — a lock taken away, and a new state read by the background

- **Symptom:** the full gate was green (320 tests) when two blind reviews found two P2s
  ([run record](../runs/2026-10-04-sb-39-quota-deadline/README.md#independent-review)):
  - with `Operation::Usage` moved out of the owner transaction, the quit drain no longer waited
    for a manual quota check, which could then spend a refresh token after the drain;
  - a hold recorded under a clock later set back was rebased only by a check, never by the
    background due filter, which then skipped the account for the size of the correction.
- **Surfaced at:** stage 5, independent review (seam and unit tiers). **Owned by:** stage 3 spec.
  The failure column listed every *caller* of the new gate (standing instruction 1) but not every
  *waiter* on the lock the change removed, nor every *reader* of the new state.
- **Root cause:** the spec modelled the change as "add a guard" and "remove a lock" separately;
  the lock's other guarantee — "the drain sees every operation finish" — had no line of its own.
- **Fix:** code, each with a fail-first test (`a_quota_check_left_running_at_quit_spends_no_refresh_token`,
  `the_background_rebases_a_hold_without_a_credential`); a standing instruction (5).
- **Catches it next time:** standing instruction 5; for readers, instruction 1 now reads both ways
  in practice — list who *reads* a new state as well as who must be blocked by it.
- **Also:** the gate's intermittent `serve` shutdown failure was a real start-up signal race
  (SB-44), fixed in the same run; standing instruction 6.

### Standing-instruction prune (2026-10-04)

- Instruction 1 held: the gate sits in `monitor::check`, and the seam tier found no caller that
  bypassed it — 1 of the 2 runs it needs to retire.
- Instruction 2 held and was needed: the stage-0 fetch found 7 unread commits on `main`; its
  retirement count restarts.
- Instruction 3: `usage-holds.json` got an owner and a deletion rule in the spec, but the unit
  tier found the rule only partly implemented (pruning) — a finding of this class; it stays.
- Instruction 4: the drain finding is of this class (a grant started where nothing outlives the
  exiting process); it stays.

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
