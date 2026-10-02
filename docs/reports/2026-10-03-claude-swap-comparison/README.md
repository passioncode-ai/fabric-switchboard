---
report:
  id: fabric-switchboard/2026-10-03-claude-swap-comparison
  title: "Claude Swap vs Fabric Switchboard: switching, isolation, renewal, rotation"
  kind: analysis
  project: fabric-switchboard
  domains: [accounts, auth, security, competitive-intelligence]
  as_of: 2026-10-03
  status: active
  valid_until: 2027-01-01
  summary: >-
    Claude Swap (realiti4/claude-swap at 3a4e5c1) and Switchboard 0.5.0 (67bf1a3) read side by side,
    plus an independent review of Switchboard's switching code with probe tests. Switchboard is safer
    on locked Keychains, change checks before writing, lost locks, damaged configs, isolated sessions,
    encrypted backups and limit detection. It lagged in five places that can lose a refresh token or
    sign the user out: MCP tokens dropped on switch, the outgoing generation saved before the locks,
    no owner check of the live token, a renewed token discarded when storing fails, and two refreshers
    when Claude Swap keeps running. The fixes are tracked in docs/PLAN-0.5.md (0.5.1 section).
  sources:
    - name: realiti4/claude-swap source
      url: "https://github.com/realiti4/claude-swap/tree/3a4e5c14873eb5b32f182d55c68da98ac8c0db45"
      read_at: 2026-10-03
    - name: Fabric Switchboard source at 67bf1a3
      path: "crates/switchboard-runtime/src/external.rs"
      read_at: 2026-10-03
    - name: Switchboard probe tests (scratch worktree, removed)
      path: "docs/reports/2026-10-03-claude-swap-comparison/README.md#data-and-method"
      read_at: 2026-10-03
  produced_by:
    agent: claude-code
    task: PLAN-0.5 follow-up review (0.5.1)
  supersedes: []
  consumers: [docs/PLAN-0.5.md, docs/ACCOUNTS-AND-ROTATION.md]
---

# Claude Swap vs Fabric Switchboard

## Главное

- **Where Switchboard was riskier (P1, fixed in 0.5.1):**
  1. A switch wrote the target's captured credential item over the live one, dropping Claude Code's
     `mcpOAuth`, `mcpOAuthClientConfig`, `mcpXaaIdp`, `mcpXaaIdpConfig` and `pluginSecrets` — MCP
     servers were signed out after every switch. Claude Swap keeps the live copies
     (`credentials.py:191-267`).
  2. The outgoing account's newest token was saved before Claude Code's locks were taken; a refresh
     in between was overwritten (probe `probe_refresh_between_harvest_and_lock_is_overwritten`).
     Claude Swap reads and backs it up under the locks (`switcher.py:6790`, `7005`).
  3. Nothing checked that the live token belongs to the account named in `~/.claude.json`; an
     interrupted switch or a half-finished `/login` let background sync store account B's token as
     account A (probes `probe_lost_lock_leaves_capture_that_pairs_identity_a_with_token_b`,
     `probe_sync_adopts_another_accounts_tokens`). Claude Swap asks `/api/oauth/profile`
     (`switcher.py:6384-6430`) and stashes foreign tokens.
  4. A renewed token was discarded when storing it failed, and the account then read "Sign in again".
     Claude Swap never drops a spent token's successor (`switcher.py:2296-2410`).
  5. After importing from Claude Swap, both tools renewed the same lineage; the slower one loses it.
- **Where Switchboard is stronger (kept):** refuses a locked Keychain instead of falling back to a
  plaintext file; re-reads config and credential under the locks and aborts if either changed; never
  rolls back after losing a lock; refuses a damaged `~/.claude.json`; isolated sessions carry no
  refresh token; backups are AES-256-GCM (Claude Swap: plaintext export, base64 `.enc`); detects
  provider limit errors from transcripts and proxy 429s (Claude Swap reacts to usage thresholds only).
- **Same in both:** Claude Code lock names and order, the hashed Keychain service name for a custom
  config dir, `/usr/bin/security -i` with hex on stdin and the 4 KB argv fallback, renewing inactive
  accounts only, one strike means dead, nothing is killed or restarted.
- **Unverified folklore:** Claude Swap's "~30 s Keychain cache" for running sessions cites no Claude
  Code source (`switcher.py:7270-7273`); Switchboard's 60 s switch grace rests on it (board SB-06).

## Данные и метод

Read-only reading of both code bases at fixed commits, with `file:line` references. Nothing was run
against real accounts, the Keychain or `~/.claude*`. Four probe tests were written in a scratch
worktree of Switchboard `67bf1a3` and failed there, confirming items 1–3; the worktree was removed.
The full gap table below ranks each mechanism by risk to the user's real accounts.

## Выводы для проекта

| # | P | Mechanism | Claude Swap | Switchboard 0.5.0 | 0.5.1 decision |
|---|---|---|---|---|---|
| 1 | P1 | Shared credential keys on switch | live copies win (`credentials.py:191-267`) | overwritten from the target's snapshot (`external.rs:826-843`) | start from the live item, replace only `claudeAiOauth` |
| 2 | P1 | Outgoing generation | saved under the locks (`switcher.py:7005-7167`) | saved before them (`lib.rs:840-874`) | saved under the locks, before the first write; failure aborts the switch |
| 3 | P1 | Owner of the live token | `/api/oauth/profile` + classification (`switcher.py:6384-6576`) | none | a token whose lineage belongs to another stored account is never filed under this one; an unknown lineage is checked against `/api/oauth/profile` |
| 4 | P1 | Renewed token that cannot be stored | stashed and adopted next pass (`switcher.py:2296-2410`) | discarded, account marked dead (`refresh.rs:208-212`) | kept in memory and adopted before the next grant; a storage failure is never a dead lineage |
| 5 | P1 | Claude Swap still running after import | n/a | both renew one lineage | renewal is skipped for identities Claude Swap still manages while it runs; the import result says so |
| 6 | P2 | Wiped live token (`invalid_grant`) | treated as signed out, switch proceeds (`switcher.py:6497-6503`) | capture fails, switch refused | treated as signed out |
| 7 | P2 | Stale Claude Code lock | waits up to 9 s, takes over after 60 s / 10 s (`claude_locks.py:46-129`) | fails at once, never takes over | bounded wait and stale takeover with the same thresholds |
| 8 | P2 | Existing `.credentials.json` on macOS | rewritten to the new credential (`credentials.py:954-1038`) | left with the old account's token | rewritten when it exists, restored on rollback |
| 9 | P2 | Managed API key when activating OAuth | cleared (`credentials.py:797-937`) | left in place | `primaryApiKey` removed from the config, restored on rollback |
| 10 | P2 | Isolated session lifetime | full lineage seeded | access token only, renewed within 10 min of expiry | renewed when less than 4 h remain; the live token is used for the account Claude Code is on |
| 11 | P2 | Native operation from inside a Switchboard home | refused (`switcher.py:6661-6690`) | follows the process env | refused when `CLAUDE_CONFIG_DIR` points into Switchboard's data folder |
| 12 | P2 | Claude Swap session profiles on import | newest generation lives there (`session.py:272-286`) | ignored | the profile's credential is preferred when it expires later |
| 13 | P2 | Identity returned by the token endpoint | quarantine on mismatch (`autoswitch.py:836-877`) | ignored | a mismatch is a dead lineage for that account |
| 14 | P3 | Usage 429 `Retry-After` | honoured + 900 s | ignored | board |
| 15 | P3 | Dead-lineage memory across restarts | persisted | memory only | persisted (fingerprints only) |
| 17 | P3 | Keychain account name at sign-in | `$USER` → OS user | `$USER` only | `username()` |
| 18 | P3 | Doc: usage retry | — | docs said 401/403, code retries 401 | docs fixed |

Accepted, unchanged: a credential item larger than the 4 KB `security -i` line goes to argv as hex,
exactly as Claude Code itself does for the same item (no prompt-free alternative exists).

## Источники

- Claude Swap at `3a4e5c14873eb5b32f182d55c68da98ac8c0db45`: `switcher.py`, `credentials.py`,
  `claude_locks.py`, `session.py`, `oauth.py`, `autoswitch.py`, `usage_store.py`, `transfer.py`.
- Fabric Switchboard at `67bf1a3`: `crates/switchboard-runtime/src/{external,launch,lib,refresh,limits,monitor}.rs`,
  `crates/switchboard-core/src/{security_cli,lib,backup}.rs`, `crates/switchboard-proxy/src/lib.rs`.

## Поправки

- **2026-10-03.** Row #14 was planned for the board; it was implemented in 0.5.1 instead
  (PLAN-0.5 REQ-40: numeric `Retry-After` honoured up to 6 h, else 900 s). The final review of
  the branch also found that an idle renewal of the live account dropped the successor when
  the token endpoint named another owner, signing Claude Code out; fixed in 0.5.1 (REQ-30,
  `an_idle_renewal_issued_to_another_account_stays_with_claude_code_only`).
