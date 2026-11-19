---
report: 2026-11-19-lifecycle-remote-access-audit
module: M5 work that hangs (open PRs, backlog statuses, stale branches, plan state)
read_at: origin/main c997483b (release 0.7.123 merged, its release run in progress)
method: git and gh reads only; nothing built
---

# M5: work that hangs

None of the three open PRs can merge as they stand; PR #829 reuses the backlog id SB-418 that PR #821 owns; about 30 backlog rows still say "in review" for work already merged and shipped.

## Open PRs

| PR | Branch, head | Behind main | Conflicts (`git merge-tree`) | Decision id | Review so far | What lands it |
|---|---|---|---|---|---|---|
| #821 SB-418 W1.2 an account the person turns off stays off | `account-off` `5b3e8ecd`, 14 commits, +2125/-138 in 62 files | 29 commits | 16 files (AGENTS.md, CHANGELOG, `cli/src/main.rs`, `cli/src/mcp.rs`, `cli/tests/mcp.rs`, `runtime/src/monitor.rs`, `runtime/src/remote_usage.rs`, DECISIONS, HANDOFF, SKILL.md, tools.md, test-names.txt, test-ui-logic.mjs, `src-tauri/src/parity.rs`, ru.ts, `src/main.ts`) | DEC-0027 (reserved on main); the branch rewrites "Next free" to DEC-0029, must stay DEC-0034 | Rounds 1 to 4 with Studio gate EXIT 0 at `5b3e8ecd`; round 4 still fixed a P1, so two consecutive clean rounds are not met | Merge main and resolve; fix M5-01, M5-02; regenerate test-names; two clean rounds; gate on the merged head; release |
| #829 SB-418 organization daily usage (`org-usage/1`) | `sb-418-org-usage` `a2f2faec`, 1 commit, +222/-5 | 36 commits | 3 doc files (CHANGELOG, HANDOFF, backlog); code merges cleanly | none needed | No review round; full gate never ran | New backlog id (M5-07); resolve docs; review rounds; gate; `off: None` in its test if #821 lands first |
| #832 SB-421 named users of one Linux host reach a group's subscriptions through the proxy | `proxy-named-users` `870686bc`, 13 commits, +3315/-188 in 30 files | 32 commits | 17 files (AGENTS.md, `cli/src/main.rs`, `cli/src/mcp.rs`, `cli/tests/mcp.rs`, `proxy/src/tests.rs`, `runtime/src/lib.rs`, CLI.md, DECISIONS, HANDOFF, backlog, scenarios.md, tools.md, check-error-vocabulary.mjs, adapter.ts, demo.ts, main.ts, types.ts) | DEC-0029 (held on main); the branch sets "Next free" to DEC-0030, must stay DEC-0034; its rows SB-442 and SB-456..459 are free | Security round 1 only (two P1 and six P2 fixed); security round 2 and review round 1 not done | Merge main; fit SB-517 (M5-10); decide M5-09; security round 2 and two clean review rounds; gate plus `cargo xwin`; an operator-assisted run on the organization's VM |

## Findings

| id | sev | where | what happens | fix |
|---|---|---|---|---|
| M5-01 | P1 | #821 merged with main: `runtime/src/monitor.rs:2992`, `:3050`, `:3107` on main | SB-518 added three `switchboard_core::Account {..}` literals in tests without `off`; #821 adds `Account.off`, so the merged tree does not compile, and these are not textual conflicts | Add `off: None`; gate on the merged head |
| M5-02 | P1 | #821 against main: `runtime/src/resumable.rs:296-334` (`continue_as`), `boot_resume.rs`, duty restarts, restart waves, SB-509 Continue | DEC-0027 says continuation never uses an account the person turned off, but these paths resume a session in its own config folder (an isolated home holds that account's sign-in) without asking `Account::usable()`: the person turns X off, the Mac reboots, boot resume continues X's session on X | Before any continuation map the config folder to its account and refuse `ACCOUNT_OFF`; tests for boot resume and the duty officer |
| M5-03 | P2 | #821 `core/src/persistence.rs:67` (schema 17) | A store with an off account becomes schema 17, which 0.7.123 and older refuse; after a partial rollback an older CLI or `serve` stops with "Unsupported account metadata version" | Keep it; name the downgrade path in the release notes; test a clear refusal |
| M5-04 | P2 | #821 `runtime/src/monitor.rs` vs main `:948` (SB-360) | Main's guard is `!a.enabled`, #821's `!a.usable()`; equivalent only while `validate_snapshot` forces `enabled=false` with `off` | Take `usable()`; grep main's new paths (SB-518 budget, SB-550 remote) for `.enabled` |
| M5-05 | P3 | #821 `cli/src/mcp.rs:2295` vs `core/src/off.rs:67` | `availability` computed twice, will drift | Use the core function or test them against each other |
| M5-06 | P3 | #821 vs main's SB-481 (`fallback.rs`, `kimi.rs`) | The Kimi takeover path must skip Kimi accounts turned off; auto-merged, unverified | A test: an off Kimi account is never the SB-481 candidate |
| M5-07 | P1 | #829 `docs/evidence/backlog.md:427` | #821 and #829 both add a row SB-418 for different features; the plan's W10.0 list repeats the clash | Renumber #829's row to a free id and its references |
| M5-08 | P2 | #829 `runtime/src/statistics.rs:537`, `:573` | (a) a day's requests are filed under the account's organization at read time, so moving a seat moves its history and a removed account's history disappears; (b) `m.extend(json!(t))` dumps every `Tally` field, so `org-usage/1` silently gained SB-452's `mismatched` | (a) document or key the tally by organization; (b) an explicit field list |
| M5-09 | P2 | #832 `runtime/src/proxy_users.rs:470` (`group_pool`) | `allow-user` accepts any group, including the pay-per-token group (DEC-0023), machine groups (DEC-0013) and groups switched off; a named user on the pay-per-token group spends the API key, against DEC-0029 | Refuse machine groups and the pay-per-token group (or document it); tests |
| M5-10 | P2 | #832 `cli/src/mcp.rs:138` vs main's SB-517 (`mcp.rs:242-330`) | `switchboard_proxy_users` has a read form (`action: list`) but no `READ_FORMS` entry, and `idempotentHint:true` though `rotate` is not idempotent and `disallow` revokes | `READ_FORMS` entry and `writes` arm; correct annotations |
| M5-11 | P3 | #832 `proxy/src/lib.rs:713-720` | `named_user` runs a blocking `symlink_metadata` (and a reload) inside the async handler on every named request | Move the reload off the request path |
| M5-12 | P3 | #832 `proxy/src/peer.rs:639-649` | The accept loop admits one connection at a time and asks the password database per named uid; slow NSS stalls everyone | Keep SB-456 with a time bound |
| M5-13 | P2 | main `cli/src/mcp.rs:176`, `:196`, `:263` | `switchboard_account_remove` ("cannot be undone") and `switchboard_project_delete` are `destructiveHint:false, idempotentHint:true`; `DELETES` lists only the Kimi and OpenRouter removals; W1.5 half open | Add both to `DELETES` |

## Backlog rows with the wrong status

These say `in` or `in review` but are merged; the version is the first published tag that contains them (v0.7.115 and v0.7.118 were never published: their work shipped in v0.7.116 and v0.7.119).

| Rows | Status says | Actually |
|---|---|---|
| SB-361 | branch `attribution-dec0177` | merged #760, v0.7.103 |
| SB-406 item 7, SB-416 | branch `suite-hardening-1` | merged #817, v0.7.116 |
| SB-408, SB-410, SB-414, SB-415 | "in tag v0.7.115, no GitHub release yet" | shipped in v0.7.116 |
| SB-412 | branch `sessions-duty-dispatch` | merged #812, v0.7.114 |
| SB-417 | PR #818 | v0.7.117 (window follow-up #825 in v0.7.119) |
| SB-419 | "in review" | merged #824, v0.7.119 |
| SB-420 | PR #826 | v0.7.119 |
| SB-440, SB-443, SB-445 | PR #831 | v0.7.119 |
| SB-447, SB-448, SB-449 | branch `launch-stale-2` | merged #837, v0.7.119 |
| SB-500, SB-501, SB-502 | "in:" | merged #841, v0.7.119 |
| SB-452, SB-453 | branch `tokens-spend` | merged #839, v0.7.120 |
| SB-455 | branch `login-path` | merged #838, v0.7.121 |
| SB-423 | PR #830 | v0.7.121 |
| SB-506, SB-509, SB-530 | PR #853 | v0.7.121 |
| SB-515 | PR #850 | v0.7.121 |
| SB-379, SB-392, SB-353 | in progress naming deleted branches `bugs`, `sessions-keys`, `sb-077-audit` | keep in progress, drop the branch names |

Also: the plan README State table still shows W1.1, W1.6, #838 and #830 in review (all merged); `DECISIONS.md:9` says DEC-0028 is held (written by #824); plan 08 step 5 cites SB-422, which has no row; plan items W1.3, W1.4, W1.5, W1.7 never filed their "new row" rows.

## Stale branches on origin

Origin holds `main`, the three PR branches and `board-share` (`b176bab9`, 2026-11-17): its head equals PR #818's merged as `aada621c`, empty diff: safe to delete.

## Plan `docs/plans/2026-11-17-target-state` against main

| Wave / item | State | Evidence |
|---|---|---|
| W0 (#815, #816) | done | v0.7.116 |
| W1.1 SB-417 board share | done | #818, #825 |
| W1.2 SB-418 account off | partial | #821 not merged |
| W1.3 restore turns switching policies off | not started | `core/src/backup.rs:723-725` still sets `policy.enabled = false`; no row |
| W1.4 money paths without a ceiling | partial | SB-452/453, SB-535 done; SB-96, SB-216 open |
| W1.5 MCP annotations, silent hooks | partial | SB-517 done; M5-13 open |
| W1.6 SB-419 feed | done | follow-ups SB-430, SB-431 open |
| W1.7 key-leak audit | partial | SB-420 shipped; SB-425..427 open |
| W4.4 SB-454 saved launches | not started | designed only |
| W4.5 section 08 | partial | steps 1-3 done; 5, 6, 8, 9, 10 open (SB-461, 462, 446, 454, 463, 464; SB-422 has no row) |
| W4.6 section 09 moves between machines | done | SB-473..476; SB-480 open |
| W2 devices of an organization | not started | SB-371, SB-374, SB-403 S5 |
| W3 test suite under load | partial | A1-A3 done (#817); A4-A6 no rows |
| W4 structure | not started | nothing filed |
| W5 Fabric Sessions moves out | partial | SB-397 W0, R1, W1.0-W1.2h merged |
| W6 docs and board | done | #823 |
| W7 acceptance by the person | not started | SB-01, SB-02, SB-06, SB-178, R1.7 |
| W10.0 open PRs land | partial | #838, #848, #830, #836 merged; #829, #821, #832 open |
| W10.1 lifecycle | mostly done | SB-505..510, SB-530 done; SB-529 open |
| W10.2 full access, MCP bugs | done | SB-515, SB-517 (M5-13 remains) |
| W10.3 smart usage | partial | SB-518 merged; SB-519 open |
| W10.4 one folder-to-group resolver | not started | SB-511..514 |
| W10.5 Launch flow with key minting | not started | SB-516, SB-520..522, SB-454 |
| W10.6 remote session acts | not started | SB-523, SB-524 |
| W10.7 subscriptions board | not started | SB-525..528 |
| W10.8 two MCP doors | not started | SB-478 |
