# Account capture, native activation and quota rotation — 0.3

This is the implementation contract for the operator's additional account flow. [Plan and packet ownership](PLAN-0.3.md), [source research](RESEARCH-0.3-IMPORTS.md), [runtime](../crates/switchboard-runtime/src/lib.rs), [external adapters](../crates/switchboard-runtime/src/external.rs), [policy](../crates/switchboard-core/src/rotation.rs) and [monitor](../crates/switchboard-runtime/src/monitor.rs) are the corresponding receipts. Live provider acceptance is separate from fixture coverage.

## Three ways to add a profile

1. **Capture current CLI account** reads the authorization that Claude Code or Codex already stores locally. It does not start OAuth or log out. Label defaults to the source email when available; provider and pool remain explicit. Config and credentials are read twice to refuse a changing source. The app reports missing, unavailable and available separately. CLI: `switchboard accounts capture --provider claude` or `--provider codex`.
2. **Sign in to another account** opens a console running the installed official `claude auth login` or `codex login` in a fresh app-owned home. The user completes the provider's account selection. Finish captures that home, saves the profile, then cleans the staging home. The ordinary account remains available; no global logout is needed. CLI: `switchboard login begin --provider claude --label Work`, followed by `switchboard login finish LOGIN_ID`.
3. **Import Claude Swap profiles** reads `~/.claude-swap-backup` without executing Claude Swap or modifying its data. The sequence index names per-slot credential and config backups; the base64 `.enc` file takes precedence over a legacy macOS Keychain backup. Each invalid account counts as a failed import without revealing its content. Successful profiles go into Switchboard's protected vault. CLI: `switchboard accounts import-claude-swap`.

Reimport identifies an account by provider, pool and source account/organization identity. Email is a label, not the unique key. If identity is absent, exact token equality is the bounded fallback. The same known identity updates its saved generation; disabled remains disabled. A changed generation invalidates prior quota. Imported identity is a source claim, not proof that the provider accepted a request. Store `upsert` tests cover these boundaries.

## Which account is active

The **current CLI account** is observed from native authorization, independent of Switchboard's route. The renderer receives identity metadata and a matched local account ID, never token material. A logged-in account can be visible before it is captured. Current observations are cached for at most 30 seconds; capture, import and activation invalidate the cache. UI periodic refresh is separate from provider quota polling. `switchboard current` reads the actual CLI source.

The **selected managed account** is the route used for the next local-proxy request. An existing stream retains its captured credential. Multiple provider/pool routes can coexist. `accounts select` changes this route; it does not claim to change every console on the machine.

**Use in Claude Code** explicitly activates a saved Claude OAuth profile in the ordinary CLI store. The runtime preserves the current profile's newest credential generation if that identity is already managed, then the adapter checks the expected native identity under Claude-compatible lock directories. It replaces credentials and only the `oauthAccount` part of the native config, preserving other settings. Failure restores the prior state where possible and reports a sanitized failure if rollback itself fails. No process is killed and no prompt is replayed. A client may cache its token; the exact adoption delay requires live version-specific acceptance. Codex native takeover is not implemented; its capture, isolated login/launch and managed routing are available.

## Usage lifecycle

While the desktop or `switchboard serve` owns the store, a scheduler considers due enabled OAuth accounts every 10 seconds, probes at most two per pass, and normally schedules each successful observation 180 seconds later. It uses fixed provider quota endpoints, not inference requests. Oldest due goes first. Larger collections can wait longer than 180 seconds; actual timestamps are authoritative.

Claude observations preserve five-hour and seven-day limits and supported model windows. Codex preserves primary and secondary windows. The aggregate is the highest reported utilization. Inference response headers report only the five-hour and seven-day windows; they update those windows inside the stored observation of the same credential generation and keep the others (for example `seven_day_opus`), dropping a kept window whose reset has passed. The aggregate is recomputed and `observed_at` moves to the header time (`header_usage_merges_into_stored_windows_and_is_not_journaled`). Each window retains its reset time when provided. Missing, malformed, nonfinite or out-of-range observations do not become zero. A failed probe preserves the last valid result and records failed health; repeated failures back off up to 1800 seconds. API keys and setup tokens do not imply subscription quota. See proxy parser fixtures and monitor backoff tests. New observations and health records more than 60 seconds ahead of the system clock are refused; already stored times are not re-checked against the clock, so setting the clock back never blocks opening the store or later writes. A newer write supersedes such a future-stamped record, and the monitor treats its schedule as due (`clock_set_back_after_observation_keeps_store_open_and_writable`, `schedule_written_before_the_clock_moved_back_is_due_now`).

The ordinary provider CLI owns its token refresh. Every 180 seconds, the monitor can adopt a newer current credential generation for an identity already captured in Switchboard. It never silently adds a new identity or enables a disabled profile. Switchboard does not run a competing refresh grant. Inactive imported snapshots can expire; recapture/reimport or official sign-in is the recovery. This limits unattended rotation when every remaining snapshot has expired.

## Automatic switching

Policies persist per provider, pool and target. Only one native Claude policy can be enabled across pools within this store, because the ordinary CLI account is one shared target. Default is off. Targets are `managed` (Claude/Codex next-request route) and `claude_cli` (ordinary Claude OAuth). The initial UI/CLI settings are 90% threshold, 10 percentage points hysteresis, 1800-second cooldown and maximum observation age 300 seconds. The user can change them within backend bounds.

A policy must know the current account and have usable recent quota. At threshold, candidates stay within the same provider/pool, must be enabled with usable unexpired credentials and fresh successful quota, and must have sufficient headroom below threshold. The deterministic policy chooses a candidate; a native switch additionally rechecks the current identity so an external manual login cannot be overwritten using an earlier decision. A manual selection starts the cooldown. Turning off the policy stops further automatic movement; it does not undo an already completed selection.

Hold reasons include cooldown, current account unavailable, usage unavailable, below threshold and no eligible account. A write/activation failure is shown separately from a successful switch. If every account is exhausted, stale, disabled or expired, keep the current selection and explain why. A 429 response is not automatically replayed. Only subsequent requests may use a newly selected identity.

```sh
switchboard rotation set --provider claude --target managed --enabled true
switchboard rotation set --provider claude --target claude-cli --enabled true
switchboard rotation status
switchboard rotation set --provider claude --target claude-cli --enabled false
```

An offline policy change is saved but cannot monitor or switch until an owner runs. `rotation status` reports that distinction. Quota, control and metadata errors remain sanitized.

## Activity journal

The store keeps the latest 256 sanitized events. A managed request records one outcome: `request` with `success`, `unauthorized`, `rate_limited`, `upstream_error` or `network_error`, or `proxy aborted` when a successful stream ends early. Response-header usage and stream start are not journaled, so request traffic cannot quickly evict account and selection history (`each_request_journals_one_outcome_and_keeps_account_history`). Older files that contain `proxy started`/`completed` still open.

## Platform and storage boundaries

macOS current Claude OAuth uses its native Keychain namespace (including custom-home suffix); Windows Claude uses the native credential file. Codex file capture is supported; compatible direct macOS Keychain mode is read where configured. Unsupported keyring/secrets modes report unavailable and offer isolated official login. Environment/config overrides apply to the process running Switchboard; a Finder-launched app cannot infer an arbitrary other shell's transient variables.

Metadata version 2 reads version 1 with defaults and writes new fields atomically. Older binaries must not write version 2. Native context needed for activation is inside the protected credential envelope, not metadata or IPC. Imports never change third-party file permissions. Bounded reads, traversal/reparse checks, identity validation and source drift checks apply before importing.

Synthetic tests cover storage/policy, external adapters and monitor/control integration. Real CLI adoption timing, provider responses and native Windows behavior require the acceptance packet; a cross-build does not establish them. Packaging/signing evidence is recorded per artifact in the release report.
