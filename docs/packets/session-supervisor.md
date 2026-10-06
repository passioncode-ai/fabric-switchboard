# Packet SS-01 — recovery and owned sessions

**State:** planned, board row SB-65 (P2). Not started; parts shipped elsewhere: a pending sign-in survives in the banner and is cleaned up (SB-42), continuation on another account (SB-52). **Owns:** launcher, persistent pending-login metadata, Sessions UI and corresponding scenario additions.
**Shared:** [contracts](../CONTRACTS.md), [spec §§4,8,15](../SPEC.md), [operations](../OPERATIONS.md).

Persist a nonsecret pending-login journal before spawning: UUID, provider, label/pool, phase, owned home, process birth identity, saved-account UUID if present. Startup reconciles each entry without reading unrelated homes. Provide Resume, Finish, and Cancel with explicit process state. Crash between vault save and metadata cleanup must not duplicate accounts. Deletion must remain idempotent and owned-path-only.

Use a session record with PID plus OS creation identity, provider version, home, working directory, launch mode, route pool and app capability generation. Track active requests separately from OS process life. Provide open project / inspect state / deliberate close or drain actions; never global `pkill` or process-name matching.

Desktop extension is a separate adapter within this packet: discover supported Codex Desktop versions, prove idle detection, persist intended handoff, ask before closing a user-owned Desktop, change the supported auth boundary, reopen and verify authenticated identity. If none of these can be proven, refuse takeover and offer a new managed CLI session. No promise of changing an existing WebSocket midstream.

Tests: crash at every journal phase, launch not acknowledged, PID reuse, cancellation racing completion, denied Keychain cleanup, repeated Finish, app restart during stream, stale capability, external session untouched. GUI must show unresolved state and recovery; no error-only stderr dead end. Deliver an exact compatibility table and update SCN-003/005/006/012.
