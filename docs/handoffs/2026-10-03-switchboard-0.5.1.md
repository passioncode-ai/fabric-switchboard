# Handoff — Switchboard 0.5.1: Claude Swap parity and review fixes — 2026-10-03

**State: merged to `main`, release v0.5.1-beta.1 in progress (board SB-19).**

Objective (operator, Russian, paraphrased): study how switching, session isolation and credential
substitution work; read Claude Swap's code; review again; finish whatever is missing; fix the bugs;
bring the documentation up to date; cover the project with tests; release; update the Fabric
workspace after the child repositories.

Where things are:
- REQ-18…REQ-48 and the decisions: [PLAN-0.5 §0.5.1](../PLAN-0.5.md#051--claude-swap-parity-and-review-fixes).
- Source comparison: [RPT fabric-switchboard/2026-10-03-claude-swap-comparison](../reports/2026-10-03-claude-swap-comparison/README.md).
- Checks, planted defects, final review and coverage: [release-0.5 §0.5.1](../evidence/release-0.5.md#051).

## Done

| What | Where |
|---|---|
| Switch keeps Claude Code's MCP and plugin sign-ins; outgoing generation stored under Claude Code's locks before any write; a copy newer than Claude Code's item is left alone | `external.rs` `activate`, `lib.rs` `replace_native` |
| Lineage of the live sign-in, and how it is filed:<br>• Own / Foreign / Unresolved, from stored copies plus `/api/oauth/profile`, with the organization compared<br>• nothing foreign or unattributed is filed or switched from<br>• the owner is asked before each native rotation, at most once a minute | `refresh.rs` `lineage`, `filable`, `learn_live_owner`; `monitor.rs` |
| A grant never loses its successor:<br>• the grant runs in its own task and keeps the successor, keyed by the spent token<br>• when the token endpoint names an owner, only that owner's copies get it<br>• the other holders of the spent token are dead | `refresh.rs` `spend`, `settle`, `adopt_stash`; core `Store::adopt_refreshed_for` |
| Idle renewal of the account in use, when Claude Code has left it expired for more than 300 s:<br>• takes only the two credential locks<br>• writes a kept successor first<br>• rewrites an existing plaintext copy | `refresh.rs` `renew_idle_live`, `external.rs` `LiveItem`, `write_live`, `Locks::acquire_with` |
| Claude Swap coexistence:<br>• detected<br>• its accounts are not renewed by Switchboard, and their newest generations are followed<br>• an unreadable pass keeps what Swap held<br>• a session profile is trusted only while its own config names the slot | `external.rs`, `refresh.rs` `follow_claude_swap`, `SwapView`; UI notices |
| Lock handling and the plaintext copy:<br>• stale-lock takeover (60 s, config 10 s, wait 9 s)<br>• the macOS plaintext copy is rewritten only when it exists<br>• `primaryApiKey` is removed<br>• a wiped live sign-in counts as signed out<br>• operations from inside Switchboard's data folder are refused | `external.rs` |
| Isolated launches renew below 4 h; dead lineages persisted as fingerprints; usage probe honours 429 `Retry-After` | `lib.rs`, `refresh.rs`, `monitor.rs`, proxy `probe_usage_detailed` |
| Packets merged:<br>• packet-d: launch reservations, pid reuse<br>• packet-e: proxy journal, timeouts, 32 MiB, client headers<br>• packet-f: error vocabulary check, sign-in banner outcomes | `launch.rs`, proxy, `scripts/check-error-vocabulary.mjs`, `src/` |
| Docs in the same change | ACCOUNTS-AND-ROTATION, SPEC §7, CONTRACTS §0.5.1, OPERATIONS §0.5.1, scenarios SCN-003/019/028/029, PLAN-0.5, evidence, board SB-04/15/18/19 |

## Decisions

No new operator decision was needed: every fix tightens D-2 and C-2 toward Claude Swap's behaviour.

The one extension is REQ-33, which renews the live account only when Claude Code itself has left
it expired and idle, under Claude Code's own locks.

Accepted:
- A credential item above the 4 KB `security -i` line goes to argv as hex, as Claude Code does.
- Kept successors live in memory. Quitting before they are stored loses them, and the row then
  reads *Sign in again*.

## Open (board)

- **SB-19:** release 0.5.1-beta.1.
- **SB-15:** operator acceptance on the real Mac. It is now extended to the following:
  - a switch keeps MCP sign-ins;
  - with Claude Swap running beside Switchboard, Switchboard follows it rather than racing it.
- **SB-01, SB-02, SB-06, SB-16:** unchanged.

## Exact next task

Finish SB-19 in this order, recording each step in release-0.5.md:

1. Tag `v0.5.1-beta.1` on `main` and publish the release with the notarized macOS build, the
   Windows build and `SHA256SUMS`.
2. Update the site.
3. Install to `/Applications`, keeping 0.5.0 for rollback.
4. Republish the Fabric workspace snapshot.
