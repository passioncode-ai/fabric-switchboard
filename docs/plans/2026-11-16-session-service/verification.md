# Verification of the spec and the plan

The operator asked for two to three rounds on accuracy before any wave starts. Each round runs
independent reviewers against the code and the research, and every finding is either taken into the
next draft (named where) or answered here.

## Round 1 (draft 1, `b37f80d1`)

Three reviewers: accuracy against the code (26 findings), implementation readiness (38), design (17
plus a simpler D-14). Taken into draft 2:

| Finding | Where in draft 2 |
|---|---|
| The resume path needs Switchboard (proxy, fresh tokens): "the person's login" cannot be the fallback while Switchboard is merely down (design 1) | D-6 three states; SS-1 narrowed; SS-9 two profiles |
| Hosts of the old release read old files after a one-time import (design 2, readiness 8, accuracy 7) | D-9 and the data ownership table; W2.2 |
| A forced restart repeats side effects; a session waiting for the person must not be prompted (design 3) | D-14 "Never in the middle of work", "What it is told"; `--now` removed |
| A session ended before its resume was possible (design 4) | D-14 "Preflight before any end" |
| Kinds, homes, Codex, other machines (design 5, 6, accuracy 17) | D-14 "Scope" |
| SS-14's 20 minutes contradicted the timeouts (design 7, accuracy 16) | SS-14 measured from each session's end; busy sessions stay `pending` |
| A retry could make a second copy; duplicates do not ride a wave (design 8) | D-14 "Watched to the end", "Paced" |
| Load and cost of a wave (design 9) | three at a time through admission; R-6 |
| Who may restart everyone (design 10, readiness 19) | D-14 "Who may": the operator's words allow agents; guards |
| The caller's identity (design 11, accuracy 14) | the nearest ancestor process a session file names (W5.2) |
| Install from every product, rollback, off switch, API version, last product, supervisor marker, Background Items (design 12, readiness 7, 10, 11, 15, 16) | D-10; W4.1 to W4.4 |
| The service as a single point of failure for starting sessions (design 13) | D-4: clients start hosts directly |
| Supervising a process the service did not start (design 14, readiness 32) | D-7: the service spawns, Switchboard prepares |
| StopFailure attribution while Switchboard is down (design 15) | D-12; W3.3 |
| A new repository on day one (design 17) | D-1: crates in this workspace until W2's exit |
| The modules are not standalone (accuracy 4, 5, readiness 1) | D-2; W1.2 lists every seam; two roots |
| No way to check "no behaviour change" (readiness 2) | W1.0 golden answers |
| The handshake and two duty officers both ways (readiness 5, 6) | D-13; W2.7 |
| Spool format, socket auth and framing (readiness 12, 13) | W2.4 |
| Hooks on a machine without Switchboard, no stable path (readiness 14, accuracy 8) | D-12; W2.8 |
| Signing a bare binary, the plist, releases (readiness 15) | D-3, D-10; W4.1, W4.8 |
| Windows (readiness 17) | D-3; W0.5 row |
| MCP tools and their registration (readiness 18, accuracy 9) | the README's MCP table; W2.9 |
| The wave against the duty officer and limit stops (readiness 20) | D-14 "Paced" |
| Waits deleted by `SessionEnd` (readiness 21, accuracy 13) | D-14 "What it is told" |
| Restart inside a host needs protocol 2 (readiness 23, accuracy 11) | D-8; W5.1 |
| The digest (readiness 24) | D-14 |
| `communicator_notify` is not a product interface (readiness 25, accuracy 25) | D-14 "Where the outcome goes" |
| No contract rule coverage; LC-14 cited wrongly (readiness 26, 27) | "Contract rules this service meets"; W0.7 |
| W0.1 contradicted itself (readiness 28, accuracy 20) | W0.1 journal first |
| The name decides too much to rename later (readiness 29) | O-1 frozen the day W1 starts |
| `continuing` would read transcript content (accuracy 15) | a `Stop` event or the transcript's write time only |
| The Continue path refuses most sessions and drops the launch id (accuracy 10) | D-14 resumes from the session file's config folder and keeps `SWITCHBOARD_LAUNCH_ID` |
| Permission mode on resume (accuracy 18) | D-14 "Same session, same mode" |
| Background commands (accuracy 26, accuracy 12) | D-14 waits for them like a turn |
| A daemon cannot open Terminal windows (accuracy 19) | resumes the service starts go through hosts; Terminal only for a session that was in Terminal |
| Imprecise wording on the drain, 503, citations (accuracy 1, 2, 3, 21, 22, 23, 24) | "Where we stand", W0 rows, SS-1 |

Not taken: none. Answered: the operator's «может быть, в отдельный репозиторий» is kept as the
destination (D-1), reached at W2's exit instead of day one.

Done while the round ran: W0.2, W0.3, W0.4 on branch `session-w0`.

## Round 2 (draft 2, `a8f29456`)

Two reviewers: accuracy and consistency (23 findings, 4 blockers), simplicity and scope (cuts and a
first release). Taken into draft 3:

| Finding | Where in draft 3 |
|---|---|
| Restart waves can ship first, inside today's Switchboard (simplicity 1) | README "Two outcomes", "R1"; plan R1 |
| W1.2 did not list every seam (accuracy 1) | D-2 lists them; W1.2 |
| Resumes the service starts go through hosts (accuracy 2) | W2.6 |
| The launch id on a resume (accuracy 3) | R1: kept in the restart script; the host's own launch |
| Flags "read from the command line" were unverified (accuracy 4) | R1 uses Switchboard's permission choice, stated; carrying argv flags is a later row |
| W0 rows did not match the branch (accuracy 5) | plan W0 marks each done with its tests |
| W0.4 removed a spooled stop before registering it (accuracy 6) | fixed on `session-w0` (`ecc110e3`): removed only after the owner answered |
| The wave watch ran on the 5-minute duty check (accuracy 7) | the 60 s session watch tick |
| Idle time "65 minutes" (accuracy 8) | "at least 65 minutes" |
| MCP table header and the machine mode (accuracy 9) | MCP table |
| Owner-only recovery calls after the hand-over; the mirror (accuracy 10, simplicity 2) | data ownership: forward; no mirror (D-9) |
| `resumable` lines have no id (accuracy 11) | no re-import needed (D-9) |
| `launches/` writer (accuracy 12) | data ownership: the `Events` trait, spooled or dropped |
| No writer of `sessions-handed-over.json` (accuracy 13) | W2.7 |
| How a session starts (accuracy 14, simplicity 4) | D-4: `host.start` with a direct fallback; dispatch stays in Switchboard |
| The host idle close during a wave (accuracy 15) | later row; R1 restarts promptly in place |
| LC-13, SL-2 (accuracy 16) | contract rules |
| Hook input size and private fields (accuracy 17) | D-12 |
| `UserPromptSubmit` unverified; `Stop` on interrupt (accuracy 18) | R1 decides turns by the session file's status; R1.0 measures `Stop` |
| `waiting` is mid-turn (accuracy 19) | R1: `waiting` stays `pending` |
| `--resume` and the id (accuracy 20) | R1.0 gates it; R-6 |
| The caller's mapping (accuracy 21) | R1.2: an ancestor-pid walk, `caller: null` when none |
| Socket location and its cleanup (accuracy 22) | W2.2 |
| The launch `Start` event registered twice (accuracy 23) | D-12; W2.9 |
| `duty.jsonl` appended without a lock (accuracy, open questions) | fixed on `session-w0` (`ecc110e3`) |
| W1.0 goldens read the real machine (accuracy, open questions) | W1.0 |
| Cuts: the service's own feed, Studio and Linux service, export on uninstall, mirroring, the spawn op, contract decisions before the API settles, retries, re-arming waits, protocol 2 (simplicity 2) | D-3, D-9, D-10, D-8; R1; "Later"; the host's `can` list instead of a protocol bump |
| Merge waves (simplicity 3) | W2 includes accounts; W3 install; W4 Fabric |
| O-2 is not about sessions; a claim lease is new behaviour; one idle time for every app (simplicity 4) | O-2 removed; the lease dropped; D-11 keeps each app's idle time |

Not taken: none.
