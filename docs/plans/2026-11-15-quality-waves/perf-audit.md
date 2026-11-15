# Latency audit, 2026-11-15 (W4)

The operations log since 2026-11-12 and the code at `3b07b71e`. At load around 40 the slow operations answer in 0.06 to 0.12 s (`sessions`, `launches`, `recovery`, `capacity`, `current`); at load 250 to 590 the same calls logged p50 7.5 to 13.7 s: mostly CPU starvation, made worse by the items below. `control_slow` is logged at 5 s or more: `launches` 1114 (p50 13 s, max 140 s), `recovery` 574 (p50 13.7 s), `sessions` 494, `dispatch` 249 (p50 17 s, max 195 s), `project_suggest` 86, `mcp_homes` 4 (Keychain, about 60 s). `transaction_held` 78 holds of 5 s or more (`rotate` 33, max 48 s; `source_sync` 11, max 91 s). `metadata_write_slow` 359. `vault_slow` 123.

1. `dispatch_start` runs two engine subprocesses (`continuation::show`, `observatory_mcp`) on the async worker, not in `blocking::run`.
2. Rotation reads Keychain items inside the owner transaction each pass, per policy.
3. Source sync reads Claude Swap's state and credentials under the transaction (up to 91 s).
4. Fsync'd writes under the store lock; `match_external` takes the state lock, so `current_accounts` waits behind writes.
5. `launches` (history) rebuilds from 748 records and resends 757 KB per call; needs an index by folder mtime and paging.
6. `recovery` parses `recovery.json`, scans every launch record and walks the process table per call.
7. `machine_sessions::scan` reads every transcript folder per process (fixed for the watch on branch `bugs`: once per folder per scan).
8. Dispatch and recovery claim read up to 46 credentials (`sign_in_required` instead of the kept verdict).
9. `session_watch`: overlapping watches, two process walks, the tree recomputed per call (overlap and walks fixed on branch `bugs`).
10. `project_suggest` re-reads Git files of all 174 group folders per call.
11. `project-suggestions.json` keeps 2078 `gone` rows; expire after 24 h.
12. Suggestion settle and add write under the transaction; batch them.
13. `usage` for one account reads up to 50 credentials for lineage matching.
14. `sign_in_required` recomputed per policy and caller each pass.
15. `sessions` re-reads every home's `.session.json` per call.
16. `statistics.json` (1.3 MB) rewritten with fsync every 15 minutes.
17. The limit scan stats about 500 transcripts every 30 s pass; filter folders by mtime first.
18. Renderer: full re-render with `outerHTML` compare on each refresh; a refresh on every folder event (about 480 a day); debounce.

## Status, 2026-11-15

- Item 1: done. `dispatch_start` and the continuation branches of `execute` run the engine (`continuation::show`, `observatory_mcp`, `offer`, `continue_default`) through `blocking::run`, off the async worker.
- Item 9: overlap and process walks done in #706; the tree per call stays open.
- Item 11: done. `mark_gone` forgets a `gone` row two days after its folder left (`GONE_KEPT`; the notification reads one day); test `a_gone_row_is_forgotten_two_days_after_its_folder_left`.
- Item 17: not taken as proposed. A folder's modification time moves when a file is created or removed in it, not when Claude Code appends to a transcript, so filtering folders by it would miss a long session still writing; the per-file stat stays until a safe signal exists.
- Item 18: the window reads the folder list once, two seconds after the last of a burst of folder events, not once per event.
