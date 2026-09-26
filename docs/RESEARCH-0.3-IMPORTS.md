# Native account import and activation — source evidence

Research uses source files, never real account material. Third-party code was not executed. Local read tests use synthetic credentials only.

## Claude Swap comparison

Pinned source: [realiti4/claude-swap@9aa6d029](https://github.com/realiti4/claude-swap/tree/9aa6d0292736173e70d4c5d8e2026210fe39a9ee).

- macOS and Windows use `~/.claude-swap-backup`. `sequence.json` owns the account index; `configs/.claude-config-{slot}-{email}.json` owns saved native config. [paths.py:85](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/paths.py#L85), [switcher.py:327](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L327).
- Backup `.enc` files are base64, not encryption. macOS file fallback takes precedence over Keychain because it may be the newer generation. Backup Keychain service is `claude-swap`, account `account-{slot}-{email}`. [credentials.py:1054](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/credentials.py#L1054).
- `cswap add` captures the existing live login, updates an existing identity, and rejects identity/credential drift. Active detection reads live identity rather than trusting the saved active slot. [switcher.py:3482](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L3482), [switcher.py:1926](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L1926).
- Default Claude macOS OAuth service is `Claude Code-credentials`. A custom profile hashes NFC-normalized raw `CLAUDE_CONFIG_DIR`, including syntactic trailing slash differences, with SHA256 and takes eight hexadecimal characters. [session.py:232](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/session.py#L232).
- Native switching coordinates with `.oauth_refresh.lock`, the legacy config-home sibling `.lock`, and the global config `.lock`; it updates credentials and splices `oauthAccount`. [claude_locks.py:66](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/claude_locks.py#L66), [switcher.py:7173](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L7173).
- Upstream claims macOS clients can take roughly 30 seconds to notice a changed Keychain item; file-backed clients notice on the next message. This is an upstream compatibility claim, **not measured acceptance** of Switchboard against a live provider. [switcher.py:7258](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L7258).
- Usage must read a live isolated session's current credential generation without consuming its refresh token. The upstream adopts that generation after the session exits. [switcher.py:4815](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/switcher.py#L4815).

## Quota and switching comparison

The same pinned Claude Swap source separates quota scheduling from switching. Its scheduler has a 180-second floor, active/inactive ceilings of 300/600 seconds, and an urgent 60-second path near a threshold ([poll_policy.py:72](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/poll_policy.py#L72)). Its rotation ranks remaining headroom across binding windows and applies hysteresis/cooldown, freshness and quarantine ([autoswitch.py:1](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/autoswitch.py#L1)).

Switchboard adopts separate usage windows, explicit threshold/headroom/cooldown, fresh-data eligibility and visible hold reasons. Its own bounded scheduler uses normal 180-second checks and failure backoff; it does not reproduce Claude Swap's refresh/quarantine engine. This is a deliberate boundary: the official current CLI remains the refresh owner and inactive snapshots may expire. Partial response headers cannot establish full subscription capacity for automatic selection or indefinitely postpone the independent quota probe. [Policy](../crates/switchboard-core/src/rotation.rs), [monitor](../crates/switchboard-runtime/src/monitor.rs).

## Codex primary source

Pinned source: [openai/codex@e72da2b5](https://github.com/openai/codex/tree/e72da2b53805894878023d01949a25a082e0a5cb).

File mode reads `CODEX_HOME/auth.json`. Direct keyring uses service `Codex Auth`, account `cli|` plus the first 16 SHA256 hexadecimal characters of canonicalized `CODEX_HOME`. Auto mode prefers keyring before file. [storage.rs:235](https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/login/src/auth/storage.rs#L235), [storage.rs:431](https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/login/src/auth/storage.rs#L431).

The newer secrets keyring backend is separate from direct keyring. Windows defaults to secrets; unsupported secrets/ephemeral backends are reported unavailable by Switchboard, never as missing accounts. [types.rs:115](https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/config/src/types.rs#L115), [types.rs:147](https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/config/src/types.rs#L147).

## Implementation and evidence

[external.rs](../crates/switchboard-runtime/src/external.rs) owns the adapter. `capture_current` reads current native stores, `capture_at` enriches official isolated login, `read_claude_swap` returns per-row import results, and `activate_claude` obtains compatible locks before a current-identity compare and atomic file/Keychain operations. Native auth context stays in the vault-only `Credential`; `CapturedProfile` and `ImportBatch` do not implement Debug or Serialize.

Import does not write upstream profiles, change permissions, log credentials, run OAuth refresh grants or execute third-party programs. It bounds files/indexes, rejects path traversal, checks stability of index/config/credential reads and distinguishes absent from unreadable Keychain. Unsupported or malformed rows are counted as failures. It retains native shared auth fields and writes only `oauthAccount` into existing Claude config during activation.

Lock directories are refreshed every second. Their file identities are checked before writes and cleanup; a replacement lock is not removed. An activation write failure attempts rollback. Losing ownership after a credential write returns an explicit partial-activation error and does not overwrite a subsequent owner's state.

Focused synthetic evidence: `cargo test -p switchboard-runtime external:: --lib` covers native-context retention, read-only mode preservation and symlink rejection, partial/malformed/traversal import, file-over-Keychain precedence, source drift with unchanged index, native identity CAS and rollback, lock contention/heartbeat/replacement, and Codex keyring preference over stale files. These tests do not establish actual CLI hot reload or native Windows behavior.

Boundaries: only the process's configured home/root config is observable; arbitrary `codex -c` or project/managed overrides in another process cannot be inferred. Current Claude capture supports OAuth. Independent mismatched `CLAUDE_SECURESTORAGE_CONFIG_DIR` is refused to avoid pairing one profile's identity with another's token. Native Claude activation refuses an existing identity with unreadable/missing primary credentials rather than discarding it. Account UUID/email from local config or JWT is a local identity hint, not provider verification.

## 0.3.2 live detection correction

The operator authorized investigation of their active login on 2026-09-27. Default config identity and the OAuth Keychain item both existed. The signed 0.3.1 native-framework reader reported Claude unavailable; the bounded system command read valid OAuth JSON in 0.02 seconds (only shape/presence reported, never values). Claude Swap explicitly uses the stable `/usr/bin/security` executable to avoid per-interpreter/per-application access prompts: [macos_keychain.py](https://github.com/realiti4/claude-swap/blob/9aa6d0292736173e70d4c5d8e2026210fe39a9ee/src/claude_swap/macos_keychain.py). 0.3.2 adopts that external-read path with a 5-second deadline, 64 KiB cap, missing-vs-denied distinction, no shell interpolation and sanitized errors. It does not grant broad ACL access or alter external Keychain writes.
