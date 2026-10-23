# The owner transaction: what holds it, and the order to shorten it

Design note, 2026-10-23, for SB-163. Status: measurement built (0.6.76), the changes below
decided and not built.

## 1. What is measured

`transaction_held` (crates/switchboard-runtime/src/lib.rs, `Held`) logs a background step that
held the owner transaction 5 s or more. On the operator's Mac (`~/Library/Logs/Fabric
Switchboard/switchboard.log`, read 2026-10-23 04:00 UTC):

| Day | Step | Holds of 5 s or more | Longest | Median |
|---|---|---|---|---|
| 2026-10-22 | `worked` | 3 | 51.9 s | 23.7 s |
| 2026-10-22 | `rotate` | 10 | 21.4 s | 8.7 s |
| 2026-10-22 | `suggest_found` | 9 | 20.7 s | 9.5 s |
| 2026-10-22 | `source_sync` | 3 | 18.2 s | 8.6 s |
| 2026-10-22 | `renewal` | 5 | 16.6 s | 7.9 s |
| 2026-10-22 | `vanish` | 2 | 15.8 s | 15.8 s |
| 2026-10-23 | `suggest_found` | 3 | 27.9 s | 9.9 s |
| 2026-10-23 | `renewal` | 3 | 20.4 s | 5.9 s |
| 2026-10-23 | `rotate` | 3 | 16.0 s | 11.0 s |
| 2026-10-23 | `source_sync` | 2 | 14.9 s | 14.9 s |

`worked` is gone since 0.6.68 (SB-171). The machine ran out of disk twice on 2026-10-23
(once with 142 MiB left, once with none, both seen by this session's builds) and swapped on
2026-10-19 to 21 (SB-163's row), so a part
of every number is the machine's.

## 2. What each step does under it

- `renewal` (monitor.rs, `hold("renewal")`): `refresh::ensure_fresh`, a token grant over the
  network for one inactive Claude account, then the vault write.
- `source_sync` (monitor.rs): `refresh::renew_idle_live` (a grant when Claude Code left its own
  token expired), `follow_claude_swap`, `sync_live_sources` (sign-in files and the Keychain).
- `rotate` (monitor.rs): `rotate`, which may write Claude Code's files and Keychain item.
- `suggest_found` (lib.rs): the suggestion file and, for each found folder, `save_project`.

Reads never wait (the fast path, SB-138, SB-166, SB-150); writes from the window, the CLI and
MCP do.

## 3. Decisions

| # | Decision | Why |
|---|---|---|
| T-1 | **Measure the phases first.** `Held::phase` marks each part of a step; `transaction_held` carries each phase's milliseconds (`renew_live`, `swap`, `sync`; `refresh`; `snapshot`, `settle`, `add`). Built in 0.6.76. | The table above cannot tell the network from the disk. |
| T-2 | **A token grant leaves the transaction.** The grant runs under a per-account refresh lock (one grant per lineage at a time, as Claude Swap and Claude Code keep it), and the transaction is taken only to store the result, checked against the credential generation it started from (`observe_credential`'s rule): a newer credential stored meanwhile wins and the grant's result is dropped. | A grant is seconds of network; nothing else in the owner needs to wait for it. The generation check keeps the one rule that matters: two writers never store two lineages. |
| T-3 | **`suggest_found` writes once.** Every folder found in a pass joins in one `save_project` per group, not one per folder. | Each save publishes `accounts.json` and syncs bindings. |
| T-4 | **`rotate` keeps the transaction.** A native switch changes the ordinary sign-in and must not interleave with a manual switch or a renewal. | Correctness over latency; it already runs as a blocking section. |

T-2 changes how OAuth renewal is ordered and is built only with a test that runs a grant and a
manual switch on one account at once (the grant's result is dropped, the switch stands).

## 4. Rows

T-1 is SB-163's first part (done in 0.6.76). T-2 and T-3 are SB-179 and SB-180.
