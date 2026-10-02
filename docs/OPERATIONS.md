# Operation and recovery

This describes v0.3 implementation boundaries; [release evidence](evidence/release-0.3.md) records tested platforms. The source of truth is [launch.rs](../crates/switchboard-runtime/src/launch.rs), [shared runtime](../crates/switchboard-runtime/src/lib.rs), [native IPC](../src-tauri/src/main.rs) and the [store](../crates/switchboard-core/src/lib.rs).

## Data locations and ownership

GUI and CLI resolve macOS app data to `~/Library/Application Support/ai.passioncode.fabric-switchboard`; Windows uses `%LOCALAPPDATA%/ai.passioncode.fabric-switchboard`. Do not paste its contents into issues: metadata includes account labels/IDs, and managed homes may contain credentials or provider conversation history.

| Data | Location and protection | Lifetime |
|---|---|---|
| Canonical credential (macOS) | login keychain, service `ai.passioncode.fabric-switchboard.shared`, UUID account; access list trusts the app and its bundled CLI by signature ([KEYCHAIN.md](KEYCHAIN.md)). Development builds use `….development`. Items under the 0.4 service `ai.passioncode.fabric-switchboard` are moved on first read | until explicit account removal |
| Windows canonical credential | `vault/<UUID>.dpapi`, DPAPI CurrentUser encrypted, user-only protected DACL | until explicit account removal; bound to Windows user/machine context |
| Account metadata/routes/events | atomic JSON beneath app data, private files | persisted; bounded event history |
| Isolated CLI working copy | `homes/<account UUID>/`, 0700 directory, 0600 auth/settings | retained for history; removed by account removal while idle |
| Managed CLI home | `runtimes/<provider>-<pool>/` | retained; generated capability valid only during one app lifetime |
| Official login staging | `logins/<login UUID>/` | removed after Finish or successful Cancel |
| Claude staged OAuth (macOS) | provider Keychain service derived from staging path | removed after Finish/Cancel, only that derived service |
| Claude staged OAuth (Windows) | private `.credentials.json` under the login staging home | captured into DPAPI vault; staging cleaned after Finish/Cancel |
| CLI control capability | private `control.json`, separate loopback address and token | rotated when GUI/serve starts; removed on normal shutdown |

The owner polls quota in the background and can adopt the ordinary CLI’s refreshed credential generation for an already captured identity. Since 0.5 it also renews **inactive** Claude OAuth accounts itself (never the account the ordinary Claude Code uses); a rejected sign-in shows Sign in again on its row, which is then the recovery. A provider may invalidate a snapshot sooner. No token is sent to Fabric or written to the event journal. A same-user process can read that user's runtime files; 0600 is access control, not file encryption.

Windows private filesystem operations use current-user DACLs, reject reparse points and multi-link files, and replace atomically with `MoveFileExW`. A sharing violation refuses the write while preserving the old file; there is no delete-then-rename fallback. DPAPI encrypts canonical credentials, while native CLI working copies remain private plaintext as required by the provider. Implementation and native fixtures: [windows.rs](../crates/switchboard-core/src/windows.rs).

## Ordinary failures

- Missing CLI: install the official provider CLI, make it available in PATH, `~/.local/bin`, `/opt/homebrew/bin` or `/usr/local/bin`, then retry. This app never installs it automatically.
- Keychain refused: the app and the CLI never ask during ordinary use; they return a message instead. "Saved by an earlier Switchboard" → open the app once; it moves each such account, asking at most once per account, and not again after Deny until it restarts. "This copy of Switchboard" → unlock the login keychain or use the app from Applications. "Development build" → that build sees only what it saved itself; use the signed app. There is no plaintext vault fallback. Design and evidence: [KEYCHAIN.md](KEYCHAIN.md).
- Windows vault refused: use the original Windows user context and verify profile access. Copying a DPAPI blob to another user is not an account migration method; reauthenticate instead.
- Control capability refused: stop its GUI/serve owner, then restart it to rotate the capability. Do not bypass a live owner with manual file edits. An uncertain mutation result requires inspecting state before retrying.
- Selected account removal refused: select another in the same provider/pool, or disable this one first. Removal is local, never a provider-side revocation.
- Already-running home: close that CLI session before launching another with the same home. Selecting another route for its next request does not rewrite the home.
- Usage unavailable: keep last observation and read the error; API keys and setup tokens are not promised subscription usage probes.
- Managed stream fails: inspect the provider client. Switchboard returns it once and does not replay. Select another account and deliberately submit a new request only when appropriate.
- App closed/restarted: close old managed CLI sessions and launch again; their previous loopback capability and port are invalid.

## Interrupted onboarding and launch

Keep the app open during official sign-in. Finish can be retried in the same app lifetime without adding a duplicate if credential storage succeeded but cleanup failed. Cancel refuses while Terminal is running or launch is pending. Close/finish the staged CLI, then retry; it does not terminate the user's process.

On app crash the in-memory pending-login registry is lost. The app does **not** silently delete leftover login homes or Keychain records at restart. Current recovery is explicit: close the matching Terminal, locate only the UUID under the app's `logins/`, and remove that staging folder and, for Claude, its matching derived Keychain service through Keychain Access after confirming ownership. Never delete the unscoped `Claude Code-credentials` item. Derivation is in `keychain_service` and tested in `service_matches_derived_name`; do not guess it from an account label. Restart sign-in with a new profile. A resumable pending-login registry is a follow-on feature, not current behavior.

If Terminal never acknowledged a launch, `.launch-pending` remains in that home. Verify the corresponding Terminal/process is not running before removing **only that home’s marker**. The app deliberately does not guess that a slow launch has died. A stale `.session-pid` referencing an unrelated reused PID also refuses conservatively. PID identity supervision is a separate task.

On Windows the launcher uses a new PowerShell console, a UTF-8 BOM script and a PID plus process-creation FILETIME marker. Environment changes and execution-policy bypass apply only to the child, not the machine. A closed/reused PID cannot authorize deletion of a currently matching owned process. Native provider login and interactive console acceptance remain separate from synthetic launcher fixtures.

## Corrupt metadata, backup and uninstall

Do not hand-edit schema versions or replace corrupted metadata with an empty list. Quit the app and preserve the affected file privately for diagnosis; a clean previous backup is the recovery source. Startup currently emits a sanitized stderr error and exits (launch from Terminal to read it); graphical repair is not implemented.

Backing up app data alone does not back up the Keychain credential items. Copying both across machines is not a supported migration flow. Encrypted export/import is deferred. Removing the `.app` does not purge profiles or Keychain; delete accounts through the app before removing it if you want their stored secrets deleted. App-owned managed histories are separate from account credential removal and may require explicit user cleanup. No updater or background launch agent is installed.

## 0.3 native profiles and metadata

The complete contract is [accounts and rotation](ACCOUNTS-AND-ROTATION.md). Metadata version 2 accepts version 1 with defaults, then writes version 2 on mutation. Do not downgrade to 0.2 against the same metadata after migration. No automatic destructive downgrade is provided.

Current CLI observation reads the authorization visible to the running process, not arbitrary other shells. For custom `CLAUDE_CONFIG_DIR`/`CODEX_HOME`, launch the GUI or `serve` owner from the intended environment: connected CLI capture/current commands execute in that owner’s environment. Offline capture uses the caller’s environment. A mismatched secure-storage override or unsupported Codex secrets backend reports unavailable. Official isolated login remains available.

Native activation refuses locks held by Claude; wait for its login/refresh to finish and retry. A lock lost after credential write, or failed rollback, needs explicit official Claude sign-in before retrying; the app will not kill processes or overwrite another lock owner. The active identity should be re-read after any uncertain result. An activation success is a storage result, not proof of a provider response or instantaneous client cache reload.

Automatic rotation is off by default. Disable the applicable policy to stop it. Native Claude has only one globally enabled policy; other pools must wait or use managed routes. Stale/failed quota, reset crossings, expired candidates and exhausted accounts hold the selection. Manual managed choice atomically starts its cooldown. Closing the owner stops polling/rotation. The CLI `rotation status` separates saved settings from a running monitor.

## 0.4 project rules and agents

- **Rules are optional.** With none saved nothing changes: selection and rotation behave as in 0.3. List them with `switchboard project list` or the desktop Projects screen; pause with `switchboard project set --path <folder> --account <id> --paused` or remove with `switchboard project remove --path <folder> --provider claude`.
- **Downgrade.** While any rule exists, `accounts.json` is schema 3 and a 0.3 build refuses to open it without changing it. Remove every rule with 0.4 (the file returns to schema 2), then open 0.3.
- **Agents cannot reach the tools.** `switchboard --json status` must work in the agent's shell; the plugin runs `switchboard mcp` from PATH. On macOS the desktop's Agents panel links `~/.local/bin/switchboard` to the CLI inside the app. A managed session launched while no CLI was found has no tools; relaunch it after linking.
- **An agent switched the wrong account.** The Activity screen shows `account_selected`/`project_rule applied`; select the right account or pause the rule. A global Claude Code change is recorded like a manual activation and is undone by activating the previous profile.

## 0.5 Claude Code item and token renewal

- **Claude Code's item.** From 0.5 Switchboard reads, writes and deletes `Claude Code-credentials` (and an official sign-in's staged item) only through `/usr/bin/security`, the executable Claude Code itself uses, with the value on stdin as hex; neither Switchboard nor Claude Code meets a consent dialog for it afterwards. A `Claude Code-credentials` item that a 0.4 build *created* (activation while Claude Code was signed out) trusts only that build; if macOS keeps asking about it, sign out and in once in Claude Code (`claude /logout`, then `claude /login`) so Claude Code recreates it. Switchboard's own account storage is unchanged from 0.4.1 ([KEYCHAIN.md](KEYCHAIN.md)).
- **Token renewal.** Inactive Claude accounts are renewed through `platform.claude.com` before their quota checks and on their own schedule ([ACCOUNTS-AND-ROTATION](ACCOUNTS-AND-ROTATION.md)). A row that shows Sign in again had its sign-in rejected by the provider; nothing but a new sign-in or capture recovers it, and Switchboard never switches Claude Code to it. The account signed in to the ordinary Claude Code, or seen there within 15 minutes, is never renewed by Switchboard.
