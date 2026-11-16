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
