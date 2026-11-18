# 01 Accounts and the runtime core

Part of [the target-state plan](README.md). Read at `0e09c0f2` (branch `target-state`, on top of
`f5e2c77e`, version 0.7.114). Every claim about current behaviour carries `file:line` (paths are
repository-relative, `core` = `crates/switchboard-core/src`, `rt` = `crates/switchboard-runtime/src`,
`proxy` = `crates/switchboard-proxy/src`) or a test name; "not verified" marks what was not read
through. No test was run for this section (the disk is nearly full); test names are checked to
exist, not to pass.

## What the area is for

The accounts core keeps a person's Claude Code and Codex sign-ins (and API keys) in the OS vault,
knows how much quota each has left, renews the inactive ones, moves the work of a group onto its
next usable account when one runs out, and serves managed sessions through a loopback proxy that
never hands a real credential to the session. The owner (desktop app or `switchboard serve`) is
the one process that does all of this for a data folder. It serves North Star sections 3 (a
person's Claude seat), 4 (accounts and seats per group, fallback chains, ceilings), 6 (work
continues on the next account) and 10 (seats at boot), as listed in `AGENTS.md` "Read first" 1a.

Sources of truth used: [SPEC](../../SPEC.md) §1 to §9, §15, §16;
[ACCOUNTS-AND-ROTATION](../../ACCOUNTS-AND-ROTATION.md); [KEYCHAIN](../../KEYCHAIN.md);
[CONTRACTS](../../CONTRACTS.md) sections SB-39, SB-41, SB-98, SB-101, SB-102, SB-106, SB-287,
SB-401; [DECISIONS](../../DECISIONS.md) DEC-0002, DEC-0003, DEC-0008, DEC-0010, DEC-0013,
DEC-0022 to DEC-0025; `AGENTS.md` Lifecycle table and idle budget; scenarios SCN-002 to SCN-012,
SCN-018 to SCN-022, SCN-027 to SCN-034, SCN-047, SCN-052, SCN-066, SCN-072; open rows of
[the board](../../evidence/backlog.md).

Two structural facts recur in most capabilities below and are the main refactoring targets:

- **The owner is two god objects.** `Runtime` (`rt/lib.rs:1339-1438`) holds about 45 fields:
  twenty `Mutex`es of unrelated subsystems (logins, current-account cache, rotation logs,
  backups, analytics, suggestions, statistics, bindings, balancing, Vesta, OpenRouter models).
  Every operation runs through one free function `execute` (`rt/lib.rs:5452` to `8990`, about
  3,540 lines) over an `Operation` enum of 169 variants (`rt/lib.rs:121-1227`), used by the owner
  and by the offline path (`execute_offline`, `rt/lib.rs:4815`), so every arm handles
  `runtime: None`. SB-362 (stack overflow of the `execute` future in debug tests) is a symptom.
- **The background pass is one sequential function.** `MonitorHandle::run_with_stop`
  (`rt/monitor.rs:145-426`) runs about 25 unrelated jobs in a fixed order every 30 s: vault and
  write timing logs, source sync, backups, Vesta, limit scan, folder suggestions, bindings, quota
  pacing, renewals, quota checks, native owner lookup, rotation under the transaction, balancing,
  Kimi, fallback, hand-overs, digests, launch pruning, dispatch sweep, stop spool, remote usage,
  delegated leases, session watch, analytics, flush, statistics, ceilings. A slow step delays
  every later one; the stop flag is checked only at three points (`rt/monitor.rs:213`, `220`,
  `228`).

---

### 1. Account store and metadata

**Target.** One owner process holds an exclusive lock on the data folder; metadata is one
schema-versioned JSON document written atomically (temp, fsync, rename), 0700/0600, symlinks
refused; a corrupt or unknown version refuses to start, never shows an empty list; a mutation
publishes memory only after the disk write succeeds; readers never wait for a write
([SPEC](../../SPEC.md) §5; `AGENTS.md` Lifecycle "Store lock", "Writes"). Accounts carry
provider, kind, pool, label, enabled; account ids are generated UUIDs (SPEC §3).

**Now.** `persistence::open` (`core/persistence.rs:154-191`) takes `instance.lock` with
`try_lock_exclusive` (`:163`), accepts schema 1 to 16 (`:180`), validates the snapshot and fails
closed on corruption. `Store` (`core/lib.rs:345-393`) keeps `state: Mutex<Snapshot>`, a
`published: RwLock<Arc<Snapshot>>` for readers (SB-296), a `credential_writes` mutex taken before
`state` (H-2), and seven more memory-only sets the proxy and runtime exchange (`in_use`,
`limited`, `refused`, `paced`, `ended`, `failover_tally`). `Store::open` (`core/lib.rs:933-981`)
also runs two migrations (`stale_shares_prune`, `retire_domain_rule`). Quota timestamps stay in
memory and are flushed every 15 minutes (`flush_if_older`, `rt/monitor.rs:420`). Tests:
`crates/switchboard-core/tests/storage.rs`, `write_mark_tests`, `published_tests`
(`core/lib.rs:2790`, `2843`).

**Gap.** P2: the store is a 2,900-line file that mixes persistence, policy, routing, failover and
seven pieces of runtime state that are not metadata (the memory-only sets are written by the
runtime and read by the proxy, so their freshness depends on the 30 s pass, `rt/monitor.rs:232-247`).
P3: `Store::match_external` takes the state lock that fsync'd writes hold (SB-394). P3:
`upsert` refuses a Claude API key whose named pool is a machine group before redirecting it
(SB-406 item 1, `core/lib.rs` around `1565` to `1576`). Errors are `String` everywhere; callers
classify them by text (`Store::usage_refusal_code`, `core/lib.rs:2645`; `owner_unreachable`,
`rt/lib.rs:4904-4914`).

**Refactor.** Split `Store` into `MetadataStore` (lock, snapshot, publish, migrations),
`AccountVault` (credential lock and vault access) and a separate `RuntimeSignals` struct for the
memory-only sets, owned by the runtime and handed to the proxy explicitly. Introduce a typed error
enum (`StoreError::{Locked, NotFound, Disabled, Vault(..), ...}`) with stable codes, and map to
sentences only at the surface; this removes text matching in `owner_unreachable` and
`usage_refusal_code`. Write the lock hierarchy (credential lock, state lock, published lock, the
runtime transaction, the macOS Keychain mutex `core/keychain_macos.rs:81`) as a module comment
plus a debug-build order check.

### 2. Credential vault

**Target.** Canonical secrets live in the platform store: macOS Keychain items under
`ai.passioncode.fabric-switchboard.shared` trusted by the app and its CLI, no consent dialogs
after migration; Windows DPAPI CurrentUser files with user-only DACLs; Linux Secret Service; no
plaintext fallback ([SPEC](../../SPEC.md) §5, §12; [KEYCHAIN](../../KEYCHAIN.md) Decision). A file
store is an explicit per-folder choice for a machine with no graphical login, never a silent
fallback (CONTRACTS SB-287). Claude Code's own item is touched only through `/usr/bin/security`
with the value on stdin (ACCOUNTS-AND-ROTATION, Switch step 5 paragraph).

**Now.** `Vault` trait and `MemoryVault` (`core/vault.rs:8-42`), `NativeVault` with per-platform
impls (`:46-192`), `FileVault` (`:262-319`, plain JSON 0600, 64 KiB cap), `open_vault`
(`:251`) chosen by `vault.json` (`vault_kind`, `:216`). macOS calls are serialized by a global
`KEYCHAIN` mutex (`core/keychain_macos.rs:81`) and every keychain reference is kept for the
process life (`keep_for_life`, `KEPT` at `:538`, the SB-254 mitigation). `CountedVault`
(`core/lib.rs:692-779`) counts writes and slow reads. Tests: `file_vault_tests`
(`core/vault.rs:321+`), `a_file_store_is_refused_while_the_platform_store_works`.

**Gap.** P1: SB-254 (owner killed by a Security framework teardown on a background queue) is
mitigated, not reproduced, and stays open until a week without the crash. P2: one process-wide
Keychain mutex means a stalled Keychain read stalls every vault user; SB-410 found a real stall of
read-only requests behind it (in review). P2: SB-12 and SB-15 acceptance (no consent dialog over a
day) never closed on the operator's Mac. P3: [SPEC](../../SPEC.md) §2 and §5 say "no plaintext
vault fallback" while `FileVault` stores plaintext JSON by explicit choice: the SPEC must name the
exception. P3: SB-13 (data protection keychain) open, operator decision. P1: SB-243 (Windows,
foreign-owned files refused) fixed on a branch, released 0.7.1, never confirmed on a Windows host;
SB-02 open.

**Refactor.** Move all Keychain work onto one dedicated vault thread with a request queue and a
per-call deadline (instead of a global mutex taken on whatever thread calls), so a stall is one
queue's stall and is observable; the SB-254 lifetime rule (references released on the thread that
made them) then holds by construction. Return typed `VaultError::{Absent, Locked, Denied,
Unavailable}` instead of the shared text `keychain::ABSENT`.

### 3. Turn an account off completely, until turned on by hand (operator, 2026-11-17)

**Target** ([README](README.md) operator's addition 1). One switch takes an account out of
everything: no selection, rotation, failover, balancing, renewal, usage check, launch, dispatch,
delegation or fallback step uses it, and nothing turns it on again by itself (an import, a sync,
a restore, a recall, a lease). The window, CLI and MCP say it is off and since when. Today's
baseline is SCN-009 ("disabled account stays visible and is removed from active route") and
SPEC §3 ("Disabled accounts remain visible but cannot be selected/used until enabled").

**Now.** There is no "off by hand" state. `Account.enabled` (one bool) is written by the person
(`Store::update_fields`, `core/lib.rs:2159-2195`, which drops the account's routes when off,
`:2183-2184`) and also by four automatic meanings: used on another machine (`mark_elsewhere_at`,
`core/lib.rs:2224`; `machines::place`, `core/machines.rs:272`), a delegated lease that ended
(`end_lease`, `core/delegation.rs:354`), a seat called back (`surrender_delegated`,
`core/delegation.rs:419`), and a copy given back to another machine (`rt/account_send.rs:693-720`).
No timestamp records when an account was turned off. Path by path:

| Path | Where | Respects off? |
|---|---|---|
| Manual select (window, CLI, MCP, `--force`) | `select_transaction` requires `a.enabled`, `core/lib.rs:2504`; `checked_credential`, `:2537` | yes |
| Proxy route | `Store::route` -> `checked_credential`, `core/lib.rs:2561-2575`; off drops routes, `:2183` | yes |
| Proxy failover and route repair | `failover_candidate` "enabled", `core/rotation.rs:405`; `rotation_credential_eligible`, `core/rotation.rs:346-356` | yes |
| Automatic switching (managed, `claude_cli`) | `rotation_credential_eligible` checks `account.enabled` (`core/rotation.rs:354`) | yes |
| Native Switch by hand | `replace_native` refuses "Account is disabled", `rt/lib.rs:9278-9279` | yes |
| Balancing | `usable` requires `account.enabled`, `rt/balancing.rs:337-343` | yes |
| Inactive OAuth renewal | `renew_due` filters `a.enabled`, `rt/monitor.rs:956-957`; test `an_account_used_elsewhere_or_turned_off_is_never_renewed_here` | yes |
| Idle renewal of Claude Code's live sign-in | `renew_idle_live` takes copies from `matching_accounts` (`rt/lib.rs:9013-9042`, no `enabled` filter) at `rt/refresh.rs:1409`; spends the live refresh token and files the successor into off copies | **leaks** (spends a refresh token for an identity whose every copy is off) |
| Source sync and Claude Swap adoption | newer generations are written into every copy of the identity (`adopt_refreshed_for`, `core/lib.rs:1997`; `follow_claude_swap`, `rt/refresh.rs:1562`); `enabled` is kept | partial: no use, but credential writes into an off account; not verified that none re-enables |
| Usage checks (background) | `due` requires `a.enabled`, `rt/monitor.rs:580-586` | yes |
| Usage check by a person or MCP | refused `USAGE_DISABLED`, `proxy/lib.rs:1360`, `:1485`; test `a_turned_off_account_is_refused_without_reading_its_credential` | yes |
| Launch (isolated, managed, in place) | `rt/launch.rs:2747` refuses | yes |
| Continue, recovery, capacity notices | `rt/continuation.rs:444`; `rt/recovery.rs:212`, `:315` | yes |
| Fallback chains and dispatch | `resolve_with` usable requires `a.enabled`, `rt/fallback.rs:154`; dispatch reaches it through `continue_default` -> `account_for_default` -> `pick_for_default` -> `resolve_with` (`rt/lib.rs:5033`, `rt/fallback.rs:507-551`, `:612`) | yes for saved accounts; **Kimi Code accounts have no off state at all** (`KimiAccount`, `core/kimi_accounts.rs:17-29`), so a Kimi step cannot be turned off |
| Send to another machine | refused while off, `rt/account_send.rs:312` | yes |
| Turning "used elsewhere" off by hand | `mark_elsewhere_at(on=false)` sets `a.enabled = true` unconditionally, `core/lib.rs:2224` | **leaks**: an account off by hand before it was marked comes back on |
| Recall from another machine | `recall_take_as` calls `mark_elsewhere(id, false)` (`rt/account_send.rs:926`, `:947`) -> enabled; the other machine's answer carries no off flag (`rt/account_send.rs:773-775`) | **leaks**: a copy that was off there comes home on |
| Give to an agent | `assign_to_agent` sets `enabled = true` when the account was marked elsewhere, `core/lib.rs:2453-2455` | person's act; conflates "elsewhere" with "off" |
| Delegated seat: lease extended | `extend_lease` revives an ended seat with `enabled = true`, `core/delegation.rs:331-335` | **leaks** when the receiving person had turned the seat off before the lease ended |
| Delegated seat: call back undone | `surrender_delegated(undo)` sets `enabled = undo && !ended`, `core/delegation.rs:419` | **leaks**: a seat off by hand is turned on by an undo |
| Restore from backup | `upsert` creates the account enabled, then `store.update(.., false)` with the result ignored (`core/backup.rs:707-709`, `:802-804`) | **leaks**: published on for a moment, and stays on if the second write fails |
| Capture, Claude Swap import, Sign in again | `upsert`/`renew_rows` keep `enabled` ("disabled remains disabled", ACCOUNTS-AND-ROTATION "Three ways", reimport paragraph) | yes (no direct test found for import of an off account: not verified) |
| Machine group adoption at start | `machines::place` only sets off, `core/machines.rs:272` | yes |
| Window, CLI, MCP "off since" | no field exists | **missing** |

**Gap.** P1 for the whole capability: six leaks (idle live renewal, `mark_elsewhere` off,
recall, lease extension, call-back undo, restore), Kimi accounts cannot be turned off, no "since",
and the off state is not distinguishable from "used elsewhere" or "lease ended" in the UI or MCP.
No backlog row yet (new; the README asks for one per wave item).

**Refactor.** Replace the bool with an explicit state: `off: Option<Off { by: Person | Elsewhere
| LeaseEnded | CalledBack, since: i64 }>` plus a separate `held_by_person: Option<i64>` that only
`update_fields` sets and clears. Every automatic writer (`mark_elsewhere_at`, `place`,
`end_lease`, `extend_lease`, `surrender_delegated`, recall, restore, `assign_to_agent`) may set or
clear only its own reason and never the person's. One predicate `Snapshot::usable_here(&Account)`
used by every path in the table (today each path re-implements its own mix of `enabled`,
`elsewhere`, `assigned_to`, `delegated`, `key_outside_tokens`, group `off`). Restore writes the
account with its state in one publish (`upsert` taking the state). Add an `enabled` (person) flag
to `KimiAccount`. Carry the off flag in the surrender answer. Property test: for every operation
in `Operation`, an account held off by the person is never selected, checked, renewed, launched,
lent, sent or re-enabled.

### 4. Add the current CLI account (capture)

**Target.** One click (or `accounts capture --provider`) reads the sign-in Claude Code or Codex
already holds, without OAuth or logout; reads twice to refuse a changing source; reports missing,
unavailable and available separately; never files another account's lineage under this name; a
first capture works offline (ACCOUNTS-AND-ROTATION "Three ways" 1, "Nothing moves backwards";
SCN-018).

**Now.** `Operation::CaptureCurrent` arm `rt/lib.rs:5500`; `external::capture_current`
(`rt/external.rs:516-523`, fresh read) and `capture_current_cached` (`:529-539`, probe-then-read
behind a process-global `WATCH`, `:791`). Tests `capturing_another_accounts_lineage_under_this_name_is_refused`,
`capturing_an_unattributed_sign_in_never_overwrites_a_saved_copy` (`rt/lib.rs`).

**Gap.** P3: the source watch and the signed-out flag are process statics (`WATCH`,
`CLAUDE_SIGNED_OUT`, `rt/external.rs:760`, `:791`), so two runtimes in one process (tests) share
them and the monitor tests need their own static clock (`rt/monitor.rs:2754-2758`). P2: Codex
capture covers file storage and macOS Keychain mode only; keyring modes report unavailable
(ACCOUNTS-AND-ROTATION "Platform and storage boundaries").

**Refactor.** Make the source watch a field of the runtime's `NativeSources` (it is already a
function-pointer struct, `rt/lib.rs:1283-1316`), injected rather than static. Split
`rt/external.rs` (3,852 lines) into `claude_code_item` (locks, live item, activation),
`codex_source`, `claude_swap` and `source_watch`.

### 5. Sign in to another account (Terminal and headless)

**Target.** One click opens the official `claude auth login` / `codex login` in a fresh app-owned
home; the app polls `pending/complete/ended` and finishes by itself; *Sign in again* updates the
same row; Cancel ends only the sign-in it started; a sign-in open across a restart comes back
and a saved one is never taken back (SPEC §4; ACCOUNTS-AND-ROTATION "Three ways" 2; CONTRACTS
SB-42, SB-65, SB-83, SB-87, SB-103; SCN-003, SCN-016). Headless: the owner runs the CLI as its
child and takes the pasted code (CONTRACTS "Headless sign-in (SB-256 N1)").

**Now.** `Runtime::begin_login` `rt/lib.rs:2633`, `finish_login` `:2661`, `adopt_logins`
`:2797`, `login_status` `:2811`, `cancel_login` `:2850`; logins kept in four runtime mutexes
(`logins`, `stale_logins`, `completed_logins`, `expected_logins`, `rt/lib.rs:1343-1352`).
Headless: `rt/headless_login.rs:55` (`start`), `:160` (`submit_code`), `:178` (`stop`), a static
running map (`:42`). Tests `login_state_reads_completion_and_an_exited_terminal`,
`a_sign_in_open_across_a_restart_is_taken_back_and_kept_from_the_sweep` (`rt/launch.rs`),
`a_pasted_code_completes_the_sign_in_and_the_browser_url_is_only_recorded`
(`rt/headless_login.rs`).

**Gap.** P1: SB-01 (live provider acceptance) never run; sign-in completion is fixture-proven
only. P2: SB-256 (sign-in autopilot N2 to N4) open. P2: SB-65 recovery metadata partly open
(Sessions view done). P3: login state lives in four maps plus journal files plus a static map for
headless sign-ins.

**Refactor.** One `Logins` component (its own module and struct) owning the journal, the maps
and the headless children, with one state machine `Started -> Pending -> Completed | Ended |
Cancelled`; the runtime holds one field instead of four.

### 6. Kimi Code accounts

**Target.** Several Kimi Code subscription accounts, signed in through Kimi's own flow, with
their windows' usage, usable as fallback executors (CONTRACTS "Kimi Code accounts (SB-81)";
SCN-047).

**Now.** `kimi::begin_login` `rt/kimi.rs:549`, `finish_login` `:686`, `sweep_logins` `:640`;
watch every 10 min while a chain is set (`kimi::watch`, called at `rt/monitor.rs:316`).
Metadata in `snapshot.kimi_accounts` (`core/kimi_accounts.rs:17`).

**Gap.** P2: no off state (see capability 3), no pool, and a different metadata shape from
`Account`, so every consumer of "an account" has a Kimi branch (`KimiChoice`,
`rt/fallback.rs:526-542`).

**Refactor.** Make Kimi a provider of the ordinary `Account` (kind `OAuth`, provider `Kimi`) with
its own usage parser, so off, pools, labels and attention apply uniformly.

### 7. Claude Swap import and coexistence

**Target.** Import reads `~/.claude-swap-backup` without running or changing Claude Swap; a
running Claude Swap renews its accounts and Switchboard follows; a switching Claude Swap holds
native rotation; a stopped one still hands over its last generations; nothing moves backwards
(ACCOUNTS-AND-ROTATION "Claude Swap beside Switchboard", "Nothing moves backwards"; SCN-019).

**Now.** `read_claude_swap` `rt/external.rs:1396`, `claude_swap_activity` `:1170`,
`claude_swap_signature` `:1360`; `follow_claude_swap` `rt/refresh.rs:1562`,
`catch_up_with_claude_swap` `:1654`; import arm `rt/lib.rs:5549`. Tests
`importing_from_claude_swap_never_moves_an_account_backwards`,
`a_running_claude_swap_renews_its_accounts_and_switchboard_follows_it` (named in the doc; the
first verified in `rt/lib.rs`).

**Gap.** P3: done per SB-21; no open row. Swap state lives inside `RefreshState`
(`swap_held`, `swap_switching`, `swap_running`, `rt/refresh.rs:85-104`).

**Refactor.** Move the Swap fields to a `SwapWatch` struct beside the source watch (see
capability 9 refactor).

### 8. Native activation of Claude Code (Switch)

**Target.** One click makes a saved Claude OAuth account Claude Code's sign-in: refuses an
unsaved outgoing account, takes Claude Code's locks in its order, preserves the outgoing
generation in every copy, keeps shared MCP and plugin keys, rewrites only `oauthAccount`, rolls
back on failure, never kills a process, writes the item only through `/usr/bin/security`
(ACCOUNTS-AND-ROTATION "Which account is active" steps 1 to 5; SCN-028).

**Now.** `activate_native` / `activate_native_anyway` `rt/lib.rs:9044-9120`, `replace_native`
`:9270`; `external::activate_claude` `rt/external.rs:1994`, `with_credential_locks` `:1591`.
Tests `shared_keys_come_from_the_live_item_and_account_keys_from_the_target`
(`rt/external.rs`), `a_live_lock_is_waited_for_and_a_stale_one_is_taken_over` (named in the
doc, not verified).

**Gap.** P2: SB-06 (does a running session adopt a switch, and the unsourced "about 30 s"
adoption delay the 60 s limit grace rests on) open. P2: the native switch runs under the owner
transaction with Claude Code's files and Keychain (`rt/monitor.rs:297-307`; SB-163 T-4 keeps it
there by decision). P3: Codex native takeover not implemented (ACCOUNTS-AND-ROTATION, end of
"Which account is active"; by design).

**Refactor.** None beyond capability 4's split of `external.rs`; the activation sequence is
already a pipeline of checked steps.

### 9. OAuth renewal (refresh.rs)

**Target.** Inactive Claude OAuth accounts are renewed before expiry (600 s margin) by the owner
of the real data folder only; the ordinary Claude Code account is renewed only when idle-expired
for over 300 s under Claude Code's credential locks; one renewer per lineage; a grant never loses
its successor; `invalid_grant` is permanent until a new sign-in; `invalid_client` holds all
renewals an hour; transient failures back off 60 s to 1800 s (ACCOUNTS-AND-ROTATION "Inactive
Claude accounts are refreshed", "One renewer per lineage"; PLAN-0.5 D-2; SCN-029). Codex renewal
is planned (SB-16).

**Now.** `RefreshState` (`rt/refresh.rs:56-110`, about 20 mutexes/atomics);
`ensure_fresh_with` `:763`, `reserve_due` `:909`, `spend` `:1006`, `Reservation::grant` `:1051`
(task that stashes the successor first), `adopt_all_kept` `:1195`, `renew_idle_live` `:1389`,
`learn_live_owner` `:1852`; grants disabled for `--data-dir` owners (`rt/lib.rs:3695`) and
at drain (`rt/lib.rs:3780`). Tests `an_inactive_expired_account_is_refreshed_in_every_pool`,
`the_ordinary_claude_code_account_is_never_refreshed`,
`a_grant_whose_caller_gives_up_still_keeps_the_successor`,
`invalid_grant_requires_sign_in_until_the_credential_is_replaced`.

**Gap.** P2: SB-163 still open for evidence: `renew_live` holds the owner transaction across the
network grant (last measured 16.5 s, SB-163 note of 2026-10-29). P2: kept successors live in
memory only; a quit while the vault refuses loses them (ACCOUNTS-AND-ROTATION "Kept successors
live in memory"). P3: SB-16 (Codex renewal) researched, not built. P1: SB-178 (does renewing on
one machine sign the other out) unmeasured, blocks RM-21 S1. P1: idle live renewal ignores the
off state (capability 3).

**Refactor.** Split `RefreshState` into four owned parts: `Grants` (endpoint, in-flight, stash,
backoff, client hold), `Verdicts` (dead lineages, usage rejections, first rejection, sign-in
caches, verdict generation), `LineageOracle` (profile owners, tries, native context known) and
`SwapWatch`. Move `renew_live`'s grant out of the transaction with the `in_flight` refusal on
activation that SB-163's design note proposes. `refresh.rs` is 4,848 lines; its tests
(`rt/refresh.rs:2154` onward, over half the file) move to `refresh/tests.rs`.

### 10. Usage and quota checks

**Target.** While an owner runs, enabled OAuth accounts are checked on the fixed provider quota
endpoints: 180 s for accounts whose figures decide something (rotation pool, routed, in the
ordinary CLI), 600 s otherwise and just after a reset; near the limit 60 s from 95 % and 30 s
from 98 % plus the pool's next account every minute; at most 4 per pass; a provider 429 sets a
not-before no caller crosses, kept across restarts; unknown is never zero; a failed check keeps
the last good figures (ACCOUNTS-AND-ROTATION "Usage lifecycle"; CONTRACTS SB-39, SB-40, SB-106;
SCN-008, SCN-021, SCN-032).

**Now.** `due` / `due_now` `rt/monitor.rs:580-593`, checks loop `:269-279`, `check` `:767`,
`check_by_person` `:781`, `check_as` `:792`, `probe` `:664`; cadence in core
`next_quota_check` / `near_limit_check` (`core/lib.rs:180-212`), `pull_in_next_accounts`
(`core/lib.rs:1136`); `UsageGate` (`rt/usage_gate.rs`, not-before `hold` `:211`,
`coalesce` `:324`, persisted to `usage-holds.json`); parsers `proxy/lib.rs:1459-1786`. Tests
`a_rate_limited_usage_check_waits_as_the_provider_asks` (`rt/monitor.rs`),
`no_caller_checks_before_the_providers_not_before` (`rt/lib.rs`),
`a_hold_survives_a_restart_and_never_stores_a_token` (`rt/usage_gate.rs`).

**Gap.** P2: SB-336 part 1 (control requests slow under CPU starvation, `usage` median 11 s)
open. P3: the quota parser and HTTP probe live in the proxy crate though the proxy never calls
them for routing (`probe_usage_*`, `proxy/lib.rs:1385-1425`).

**Refactor.** Move quota probing and parsing (`proxy/lib.rs:1315-1786`) to its own crate module
`switchboard-core::quota` or a `switchboard-quota` crate; the proxy keeps only header parsing for
responses. Make the check scheduler a task with its own tick, separate from the monitor pass (see
capability 19).

### 11. Limit evidence

**Target.** An account is held as limited on two managed 429s in five minutes, a proxy refusal,
or a Claude Code transcript marker written while it was in use; holds last until the provider's
reset (else 15 minutes, at most seven days), survive restarts, carry `kind`, `confidence`,
`scope`, `event_id`; attribution follows the session; no transcript content is stored
(ACCOUNTS-AND-ROTATION "Limit errors"; CONTRACTS SB-41, SB-98, SB-102; SCN-031).

**Now.** `LimitState` (`rt/limits.rs:135` onward: `refused` `:250`, `bind` `:326`, `refresh`
`:429`), transcript scanning (`transcript_files` `:641`, `markers` `:692`, cached scanner
`:729`), `scan_limits` `rt/monitor.rs:1161` in a blocking section (`:211`); one held set
`Runtime::held` (`rt/lib.rs:3250`, WF-5.2b). Tests `evidence_survives_a_restart_and_a_hold_is_not_a_reset`
(`rt/limits.rs`), `a_limit_error_switches_despite_spare_quota_and_skips_limited_candidates`
(`crates/switchboard-core/tests/rotation.rs`).

**Gap.** P2: SB-06 underlies the 60 s grace after a switch. P2: SB-114 REQ-5 (host integration
of stops) open. P3: transcript scanning reads only the ordinary Claude config folder
(`$CLAUDE_CONFIG_DIR` or `~/.claude`); limits hit in isolated homes are seen only through the
proxy (not verified whether `Runtime::transcripts`, `rt/lib.rs:2393`, includes isolated homes).

**Refactor.** None structural; `limits.rs` is cohesive. Share the held-set computation with
balancing and dispatch (already one function since WF-5.2b).

### 12. Automatic switching per group

**Target.** Managed switching is on by default for every provider and group with accounts and
never changes a saved policy (DEC-0008); `claude_cli` switching is opt-in and only one native
policy is enabled per store; candidates stay inside the provider and group, are enabled, have
usable credentials and fresh quota with headroom; a limited account in use moves at once inside
the cooldown; a failed candidate is set aside and the next tried; a manual selection starts the
cooldown; no replay (ACCOUNTS-AND-ROTATION "Automatic switching"; CONTRACTS SB-98, SB-102;
SCN-022, SCN-029).

**Now.** `ensure_default_policies` (`core/rotation.rs:101`, missing pairs from
`missing_managed_policies` `:72-94`), called every pass under the transaction
(`rt/monitor.rs:300-311`); decision `rotation_decision_with` (`core/rotation.rs:188`), eligibility
`:346`; execution `rotate_with` (`rt/monitor.rs:1248-1438`, 190 lines) in a blocking section under
`hold("rotate")` (`:297-307`); set-aside candidates `Runtime::failed_switches`. Tests
`every_group_with_accounts_gets_managed_switching_on_and_a_saved_off_stands`,
`a_failover_moves_only_to_an_account_of_the_same_kind` (`crates/switchboard-core/tests/rotation.rs`),
`a_candidate_that_fails_to_activate_is_set_aside_and_the_next_one_tried_at_once`,
`automatic_switching_never_moves_to_an_account_given_to_an_agent` (`rt/monitor.rs`).

**Gap.** P1: a restore saves every backed-up policy switched off
(`core/backup.rs:722-727`, "Restored switched off"), and `missing_managed_policies` treats a saved
off as the person's word, so after a reinstall managed switching stays off in every group, against
DEC-0008; SCN-030 ("policies return switched off") and DEC-0008 contradict each other; no test
asserts the restored state. P2: SB-367 WF-5 marked complete in its note, but the row is still
`open`. P2: [SPEC](../../SPEC.md) §9 still says "Rotation is off by default". P3: SB-07 (a race in
`managed_session_reads_usage_and_switches_only_inside_its_pool`, flaky) open.

**Refactor.** Restore managed policies as they were saved (or drop them and let
`ensure_default_policies` recreate them on), keep `claude_cli` off; add the test. Split
`rotate_with` into decide (pure, over a snapshot and the held set) and act (select or activate),
so the decision runs outside the transaction and only the act takes it.

### 13. Managed proxy: routing and capabilities

**Target.** Loopback only, stable port and token across restarts, a new token when the port must
move; constant-time capability check before the body; per-folder capabilities so a folder taken
out of a group stops spending its accounts; header allowlists; 32 MiB body, 16 in flight, 600 s
idle read timeout, no total cap; no redirect; draining answers 503 with `Retry-After`; the
`(Account, Credential)` pair is captured once per request (SPEC §6, §7; CONTRACTS SB-118, SB-133,
SB-140, SB-269; AGENTS.md Lifecycle "Managed-mode proxy").

**Now.** `start_proxy` `rt/lib.rs:3476-3566` (record, rotation on a taken port, salts, folder and
released tokens, Codex folders); `ProxyHandle::start_on` `proxy/lib.rs:210`; `relay`
`proxy/lib.rs:651-1059` (about 410 lines: auth `:685-738`, draining `:756`, route `:786-805`,
pool refusals, body read `:828-839`, folder attribution `:841-866`, the upstream loop with
headers `:870-965`, failover `:970-999`, response relay and `AuditedStream` `:1000-1059`).
Tests in `proxy/tests.rs` (2,750 lines).

**Gap.** P2: SB-387 (an update cut managed sessions for seconds) in progress; SB-367 C8 fixed.
P3: SB-406 item 7 (`proxy/peer.rs:49` uses `libc` in a test module built on every OS; Windows
test build fails). P3: SPEC §7 still describes the 0.1 timeouts in places but matches the code on
the 600 s idle read (consistent).

**Refactor.** Split `relay` into `authenticate` (capability, folder token, agent token),
`admit` (draining, pool refusal, route, kind checks), `build_upstream` (URL and headers per
provider and kind), `send_with_failover` and `relay_response`. Each becomes unit-testable without
a full gateway. Move capability derivation (`agent_token`, `pool_token_salted`, `folder_token`,
`proxy/lib.rs:159-190`) to a `capability` module.

### 14. Failover once on 429 and route repair (DEC-0002)

**Target.** A Claude Code or Codex session's request answered 429 did no work and moves once,
before any byte reached the session, to the group's next usable account of the same kind, which
also serves the requests after it; a second refusal returns as is; an agent's API-key request is
never moved; a selected account that cannot serve (expired, gone) moves the pool without holding
it; compare-and-move so concurrent refusals agree; never onto a pay-per-token key from a
subscription session (DEC-0002; CONTRACTS SB-98, SB-101, SB-102; SCN-052).

**Now.** `relay` failover `proxy/lib.rs:970-999` (`official && !moved`), `Store::failover`
`core/lib.rs:1257`, `failover_unusable` `:1277`, `failover_candidate` `core/rotation.rs:405`
(`Fits`, `:385-399`), compare via `select_transaction(expect)` `core/lib.rs:2484-2548`; refusals
taken by the monitor (`take_refusals`, `core/lib.rs:1219`). Tests
`a_session_refused_for_a_limit_continues_on_the_next_account_without_seeing_it`,
`a_refusal_moves_once_to_the_next_account_and_events_contain_no_secrets` (`proxy/tests.rs`),
`failover_moves_the_pool_off_a_refused_account_and_holds_it`
(`crates/switchboard-core/tests/rotation.rs`).

**Gap.** P3: done. Held accounts reach the proxy only through `Store::set_limited` on the next
30 s pass, so a second session's request in that window may still try the refused account first
(it then fails over itself); not a defect, a latency.

**Refactor.** With capability 1's `RuntimeSignals`, the proxy publishes a refusal into the held
set at once instead of waiting for the pass.

### 15. Pay-per-token group spend (proxy side)

**Target.** Claude API keys live in one reserved group that starts off; the proxy checks the
group's daily ceiling before each request, counts list-price spend per answer, fails closed on
an unreadable store or spend file, loads the day before listening, and checks the chosen
account's home group before a route or a failover (DEC-0023 D-1 to D-19; CONTRACTS "Pay-per-token
group (SB-401)").

**Now.** `pool_refusal` `proxy/lib.rs:607-633`, checked before route (`:784`), after route
(`:809-813`) and on failover (`:988`); `tokens::Spend` `proxy/tokens.rs:63-285`, `Ceilings`
`:297`; prices `proxy/prices.rs`; `tokens_group_for_keys` before the proxy listens
(`rt/lib.rs:3479`). Tests `the_day_is_loaded_from_the_spend_file_and_the_statistics_the_larger_winning`,
`a_spend_file_that_cannot_be_written_refuses_until_a_write_succeeds` (`proxy/tokens.rs`),
`tokens_group_tests` (`rt/lib.rs:15156`).

**Gap.** P3: overshoot by requests in flight when the ceiling is crossed (accepted, D-14).
P3: SB-406 items 1, 2, 4, 8 (edges after review) open. P2: board drift: SB-401 reads
"in progress" though released in 0.7.112 (`755f9129`).

**Refactor.** `Ceilings::cents` reads `spend.json` by stamp on the request path
(`proxy/tokens.rs:297`); hand the proxy a pushed ceiling map from the owner instead, so the
request path does no file I/O.

### 16. Codex route

**Target.** With the switch on, Codex sessions in a group's folders use that group's Codex
accounts through one marked provider block in the person's `config.toml`, written at owner start
and taken out at stop; another provider the person chose is never replaced; folders outside
groups keep the person's own sign-in (AGENTS.md Lifecycle "Writes", SB-143; SCN-066).

**Now.** `codex_route::apply` `rt/codex_route.rs:151`, `remove` `:166`, `folders` `:26`;
`Runtime::codex_route_sync` `rt/lib.rs:1587-1611`, called in `open_with` (`:1583`) and at
shutdown (`:3799`); proxy side `codex_auto` `proxy/lib.rs:1063`. Tests
`the_provider_goes_in_around_the_persons_text_and_comes_out_leaving_it_as_it_was`,
`another_provider_chosen_by_the_person_is_never_overwritten` (`rt/codex_route.rs`).

**Gap.** P2: [SPEC](../../SPEC.md) §8 says "no mutation of global config.toml"; SB-143 writes
marked lines there by opt-in: SPEC must name the exception. P2: a crash leaves the block pointing
at a proxy that is gone until the next owner start rewrites it (by design of `apply`/`remove`;
not verified what a Codex session does meanwhile). P3: `codex_route_sync(true)` runs inside
`Runtime::open_with`, so opening a runtime has a side effect on the person's files.

**Refactor.** Move the call out of `open_with` into `Owner::start`, next to the other start-time
writes, so a plain `Runtime` open is pure.

### 17. Owner lifecycle: start, control, store lock, drain

**Target.** One owner per data folder; signals caught before `control.json` is published; the
control listener proves its identity by HMAC over a fresh challenge on the same connection;
128 KiB requests, 4 MiB responses; CLI and MCP go to the owner, else offline under the store lock,
retrying while an owner drains or starts; quit drains in 8 s, hard exit at 10 s, a quit never
undone by the login item; no grant starts after the drain begins (SPEC §16; AGENTS.md Lifecycle
LC-01, LC-11; DEC-0003; SB-44, SB-181, SB-254).

**Now.** `Owner::start_with_sources` `rt/lib.rs:3664-3766` (vault kind log, runtime, control,
backups, restore at start, grants off for `--data-dir`, adopt logins, a blocking start sweep for
machine groups, Kimi and login leftovers, crash log, monitor); `Owner::shutdown` `:3768-3821`;
`StopSignals` `:3596-3638`; hard exit `arm_hard_exit_with` `:3579`; control `ControlHandle::start`
`rt/control.rs:65`, `proof` `:141`, `request` `:405`, `limit_secs` `:424`; `call_within`
`rt/lib.rs:4983-5011` (retry every 200 ms up to `STORE_WAIT` 8 s). Tests
`live_owner_accepts_mutation_and_never_returns_secret`, `a_group_change_waits_longer_than_a_read`
(`rt/control.rs`), `lifecycle_tests` (`rt/lib.rs:13766`), `monitor_drops_without_retaining_the_runtime`.

**Gap.** P1: SB-174 (control requests unanswered over 10 s at times) and SB-336 part 1 open; the
owner transaction (`Runtime::mutations`, `rt/lib.rs:1353`) is shared by control mutations and
background steps (`hold`, `rt/lib.rs:2313`), so any long background hold blocks every mutation.
P2: SB-414 (a window that does not own its data folder says nothing) open. P2: `shutdown`
returns "once the store's lock is free" (doc comment `rt/lib.rs:3762-3767`) but the lock is
dropped only when the last `Arc<Runtime>` drops; spawned tasks (Vesta read
`rt/monitor.rs:186-208`, analytics flush `:410`, the start sweep `rt/lib.rs:3710`) hold clones
(not verified whether any can outlive the drain). P3: the doc comment of `call` sits above the
test module (`rt/lib.rs:4915-4917`), detached from `call` at `:4980`.

**Refactor.** Split the transaction: one lock per resource (routes and policies; Claude Code's
native item; group membership), so a rotation never blocks a folder save. Replace the 3,540-line
`execute` with per-domain handlers (`accounts`, `groups`, `sessions`, `fallback`, `keys`,
`machines`) behind a trait, each a boxed future (fixes SB-362 by construction). Make shutdown
await a `JoinSet` of every task the owner spawned, so "the store's lock is free" is a fact.

### 18. Backups and restore

**Target.** Encrypted backups beside (not inside) the data folder after a credential, account
or policy change (at least 60 s apart) and daily, newest 10 kept; only the real owner of the real
data folder writes them; a new data folder restores the newest backup this machine can open by
itself; nothing newer and no choice made on this install is overwritten; a restored copy starts
from the newest generation saved anywhere (SCN-030; ACCOUNTS-AND-ROTATION "Nothing moves
backwards"; AGENTS.md Lifecycle "Writes").

**Now.** `Runtime::maybe_backup` `rt/lib.rs:3189-3243` (blocking write, 60 s floor, daily);
`restore_at_start` `rt/lib.rs:3145`; `backup::write` `core/backup.rs:265`, `restore` `:497`,
`prune` `:473`, keys `KeychainKey` `:977`, `DpapiKey` `:1002`, `FileKey` `:1040`,
`SecretServiceKey` `:1099`. Tests `backups_follow_changes_and_restore_into_a_fresh_install`
(`rt/lib.rs`), `a_restored_copy_starts_from_the_newest_generation_saved_elsewhere`,
`a_reinstall_gets_organizations_their_accounts_and_groups_back` (`core/backup.rs`).

**Gap.** P1: restored managed policies are saved off (capability 12). P1: a restored off account
is published on first, and stays on if the follow-up write fails (`core/backup.rs:707-709`,
`:802-804`; capability 3). P3: SB-406 item 1 (a backup's key placed on another machine is lost at
restore). P2: SB-15 (operator acceptance of a restore) open. P3: `restore` is a single function of
about 470 lines (`core/backup.rs:497-966`).

**Refactor.** Split `restore` by entity (organizations, groups, accounts, policies, selections,
settings) in the order the code already follows; each part returns its counts. Accounts are
restored with their full state in one `upsert` call.

### 19. Background pass and the idle budget

**Target.** With nothing changing, an hour of passes reads no sign-in, writes no metadata and no
backup (one digest line), reads at most one credential per pass for each enabled policy's current
account plus one per source sync; sign-in sources are probed quietly and read only when they
changed; a pass that panics is caught and the next one runs (AGENTS.md "Idle budget"; LC-04,
LC-08).

**Now.** `MonitorHandle::start` `rt/monitor.rs:55`, panics caught per poll (`:37-52`, test
`a_pass_that_panics_is_caught_and_the_next_one_runs`), the pass `:145-426`; idle budget test
`an_idle_hour_on_a_fake_clock_stays_inside_the_budget` (`rt/monitor.rs:2751`); long transaction
holds logged with phases (`Held`, `rt/lib.rs:4279-4343`, test `a_long_hold_names_where_its_time_went`).

**Gap.** P2: one sequential pass of about 25 jobs (see the intro); a stall in any job (a Git read
in suggestions, a Keychain read, an ssh read) delays quota checks and rotation; SB-163, SB-174,
SB-336 are its symptoms. P2: the idle budget is proven on a fake clock only; AGENTS.md's CPU and
RSS targets are "measured on a release, not by the gate" and no recent measurement is linked.
P3: SB-234, SB-300, SB-361, SB-410 (tests flaky under load) show timing-dependent tests around
this loop.

**Refactor.** Turn the pass into a scheduler of independent jobs, each with its own cadence,
deadline, stop check and `transaction_held`-style timing (quota checks, renewals, rotation,
source sync, limits, suggestions and bindings, balancing, fallback and hand-overs, housekeeping),
so a slow job delays only itself; the idle budget test then asserts per job. Give every job an
injectable clock (several already take `time`), removing the remaining static test clocks
(`rt/monitor.rs:2754-2758`).

---

## Documentation drift found

- [SPEC](../../SPEC.md) §9 "Rotation is off by default" contradicts DEC-0008.
- [SPEC](../../SPEC.md) §2 and §5 "no plaintext vault fallback" do not name the SB-287 file store.
- [SPEC](../../SPEC.md) §8 "no mutation of global config.toml" does not name the SB-143 Codex route.
- [SPEC](../../SPEC.md) §6 "No automatic retry in v0.1" predates DEC-0002; it should point at it.
- SCN-030 "policies return switched off" contradicts DEC-0008 (capability 12).
- Board statuses behind the code: SB-401 "in progress" (released 0.7.112), SB-404 "in review"
  (released 0.7.113), SB-403 "in progress" (S1 to S4 released by 0.7.113), SB-367 "open" though
  its note says WF-5 complete.

## Summary

| # | Capability | State | Gap severity | Backlog ids |
|---|---|---|---|---|
| 1 | Account store and metadata | done | P2 | SB-394, SB-406 |
| 2 | Credential vault | partial | P1 | SB-254, SB-410, SB-12, SB-15, SB-13, SB-243, SB-02 |
| 3 | Turn an account off completely | broken | P1 | new (operator 2026-11-17) |
| 4 | Capture the current CLI account | done | P3 | none |
| 5 | Sign in (Terminal, headless) | partial | P1 | SB-01, SB-256, SB-65 |
| 6 | Kimi Code accounts | partial | P2 | new (off state, uniform account) |
| 7 | Claude Swap import and coexistence | done | P3 | none open |
| 8 | Native activation (Switch) | done | P2 | SB-06, SB-163 |
| 9 | OAuth renewal | partial | P1 | SB-163, SB-16, SB-178 |
| 10 | Usage and quota checks | done | P2 | SB-336 |
| 11 | Limit evidence | done | P2 | SB-06, SB-114 |
| 12 | Automatic switching per group | partial | P1 | SB-367, SB-07, new (restore turns it off) |
| 13 | Managed proxy routing and capabilities | done | P2 | SB-387, SB-406 |
| 14 | Failover once on 429 (DEC-0002) | done | P3 | none |
| 15 | Pay-per-token group spend | done | P3 | SB-401, SB-406 |
| 16 | Codex route | done | P2 | none (SPEC drift) |
| 17 | Owner lifecycle and control | partial | P1 | SB-174, SB-336, SB-414, SB-362 |
| 18 | Backups and restore | partial | P1 | SB-15, SB-406, new |
| 19 | Background pass and idle budget | partial | P2 | SB-163, SB-174, SB-336, SB-234, SB-300, SB-361, SB-410 |

Counts: done 10, partial 8, broken 1, missing 0 (capability 3 counted as broken: it exists as
`enabled` and leaks in six paths). Live provider acceptance (SB-01) and native Windows acceptance
(SB-02) are open for the whole area.
