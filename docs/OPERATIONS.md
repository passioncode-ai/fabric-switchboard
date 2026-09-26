# Operation and recovery

This describes v0.1 implementation boundaries; [verification](evidence/verification.md) is the execution receipt. The source of truth is [launch.rs](../src-tauri/src/launch.rs), [native IPC](../src-tauri/src/main.rs) and the [store](../crates/switchboard-core/src/lib.rs).

## Data locations and ownership

Tauri resolves macOS app data to `~/Library/Application Support/ai.passioncode.fabric-switchboard`. Do not paste its contents into issues: metadata includes account labels/IDs, and managed homes may contain credentials or provider conversation history.

| Data | Location and protection | Lifetime |
|---|---|---|
| Canonical credential | Keychain service `ai.passioncode.fabric-switchboard`, UUID account | until explicit account removal |
| Account metadata/routes/events | atomic JSON beneath app data, private files | persisted; bounded event history |
| Isolated CLI working copy | `homes/<account UUID>/`, 0700 directory, 0600 auth/settings | retained for history; removed by account removal while idle |
| Managed CLI home | `runtimes/<provider>-<pool>/` | retained; generated capability valid only during one app lifetime |
| Official login staging | `logins/<login UUID>/` | removed after Finish or successful Cancel |
| Claude staged OAuth | provider Keychain service derived from staging path | removed after Finish/Cancel, only that derived service |

No background token refresh exists. Reauthenticate before expiry; a provider may invalidate a snapshot sooner. No token is sent to Fabric or written to the event journal. A same-user process can read that user's runtime files; 0600 is access control, not file encryption.

## Ordinary failures

- Missing CLI: install the official provider CLI, make it available in PATH, `~/.local/bin`, `/opt/homebrew/bin` or `/usr/local/bin`, then retry. This app never installs it automatically.
- Keychain refused: grant access through normal macOS controls and retry. There is no plaintext vault fallback.
- Selected account removal refused: select another in the same provider/pool, or disable this one first. Removal is local, never a provider-side revocation.
- Already-running home: close that CLI session before launching another with the same home. Selecting another route for its next request does not rewrite the home.
- Usage unavailable: keep last observation and read the error; API keys and setup tokens are not promised subscription usage probes.
- Managed stream fails: inspect the provider client. Switchboard returns it once and does not replay. Select another account and deliberately submit a new request only when appropriate.
- App closed/restarted: close old managed CLI sessions and launch again; their previous loopback capability and port are invalid.

## Interrupted onboarding and launch

Keep the app open during official sign-in. Finish can be retried in the same app lifetime without adding a duplicate if credential storage succeeded but cleanup failed. Cancel refuses while Terminal is running or launch is pending. Close/finish the staged CLI, then retry; it does not terminate the user's process.

On app crash the in-memory pending-login registry is lost. The app does **not** silently delete leftover login homes or Keychain records at restart. Current recovery is explicit: close the matching Terminal, locate only the UUID under the app's `logins/`, and remove that staging folder and, for Claude, its matching derived Keychain service through Keychain Access after confirming ownership. Never delete the unscoped `Claude Code-credentials` item. Derivation is in `keychain_service` and tested in `service_matches_derived_name`; do not guess it from an account label. Restart sign-in with a new profile. A resumable pending-login registry is a follow-on feature, not current behavior.

If Terminal never acknowledged a launch, `.launch-pending` remains in that home. Verify the corresponding Terminal/process is not running before removing **only that home’s marker**. The app deliberately does not guess that a slow launch has died. A stale `.session-pid` referencing an unrelated reused PID also refuses conservatively. PID identity supervision is a separate task.

## Corrupt metadata, backup and uninstall

Do not hand-edit schema versions or replace corrupted metadata with an empty list. Quit the app and preserve the affected file privately for diagnosis; a clean previous backup is the recovery source. Startup currently emits a sanitized stderr error and exits (launch from Terminal to read it); graphical repair is not implemented.

Backing up app data alone does not back up the Keychain credential items. Copying both across machines is not a supported migration flow. Encrypted export/import is deferred. Removing the `.app` does not purge profiles or Keychain; delete accounts through the app before removing it if you want their stored secrets deleted. App-owned managed histories are separate from account credential removal and may require explicit user cleanup. No updater or background launch agent is installed.
