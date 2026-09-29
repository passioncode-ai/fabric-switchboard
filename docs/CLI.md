# switchboard CLI

Version 0.3 extends a native CLI with current-account capture, Claude Swap import and quota rotation over the same account store, vault, request proxy and launcher used by the desktop. [CLI release plan](CLI-RELEASE-PLAN.md) fixes the behavior; [release evidence](evidence/release-0.2.md) records which hosts actually passed. This file is an operational contract, not a claim of real-provider acceptance.

## Install and first use

Use the `switchboard` executable beside the macOS app in the signed archive, or `switchboard.exe` from the Windows artifact. Building only the CLI from source:

```sh
cargo install --path crates/switchboard-cli --locked
switchboard --help
switchboard --json accounts list
```

Help/version do not open account storage. Default data root is shared with GUI: `~/Library/Application Support/ai.passioncode.fabric-switchboard` on macOS, `%LOCALAPPDATA%/ai.passioncode.fabric-switchboard` on Windows. `--data-dir ABSOLUTE_PATH` selects a deliberately separate metadata root. Credentials stay in the native vault by UUID; never mix metadata roots casually or treat this flag as account export.

## Account management

```sh
switchboard accounts list
switchboard accounts add --provider claude --kind api-key --label Work --pool work --secret-stdin < /private/path/to/credential
switchboard accounts select ACCOUNT_UUID --provider claude --pool work
switchboard accounts update ACCOUNT_UUID --label Work --enabled false
switchboard accounts remove ACCOUNT_UUID
switchboard --json events
```

`--secret-stdin` is mandatory for add; a secret value is never accepted as an argument. Redirect from a private credential file or a trusted secret manager. Do not put tokens in shell history. Provider/kind are explicit; supported kind spellings are `api-key`, `setup-token` and `oauth`. OAuth input is provider credential JSON. UUID identifies an account; labels are display text. Removal has the same selected-account/active-home guards as the GUI.

Without a running owner, metadata operations acquire the Store’s exclusive lock and release it after the command. With GUI or `serve` running, they use the owner’s authenticated loopback control server. Both paths run core validation. The CLI must not convert an authentication failure into a local-write fallback.

## Running sessions and official sign-in

Keep the desktop open **or** run this in a separate terminal:

```sh
switchboard serve
```

Then use another terminal:

```sh
switchboard status
switchboard login begin --provider codex --label Work --pool work
switchboard login finish LOGIN_UUID
switchboard accounts select ACCOUNT_UUID --provider codex --pool work
switchboard launch ACCOUNT_UUID --mode managed --working-directory /absolute/project/path
```

On Windows use an absolute drive or UNC project path, quoted if it contains spaces. CLI login/launch need a live owner because its proxy and pending-login registry must outlive the command. Missing owner returns an instruction to start GUI or serve. Managed launch requires that account selected in its provider/pool; isolated mode launches a private credential snapshot and is not changed by later selection.

`login begin` opens the installed official provider CLI in a private home. Complete its login, then `finish` imports only that new credential into the OS vault and cleans staging. If the account is saved but its staging home cannot be removed, `finish` reports `Account saved; isolated login cleanup needs attention.`, releases the sign-in slot and retries the cleanup before the next `login begin` (`failed_login_cleanup_releases_the_slot_after_the_account_is_saved`). To cancel, close/finish that Terminal process, then `switchboard login cancel LOGIN_UUID`. It never kills an unrelated process or invokes global logout. App/server restart invalidates pending in-memory login IDs; recovery limits are in [operations](OPERATIONS.md).

Select another same-pool account while the managed response streams: the current response keeps its snapshot; the next request sees the new account. `Ctrl-C` in `serve` stops the owner; managed clients then need a new launch. Only one owner may hold a data root.

## Usage, output and failures

```sh
switchboard usage
switchboard usage ACCOUNT_UUID
switchboard --json status
switchboard --json activity
```

Without an ID, usage reads cached observations; an ID requests a provider check. Unsupported/unknown quota is an error or unknown state, never zero. API billing is not subscription utilization.

`--json` writes `{ "ok": true, "data": ... }` to stdout on success and `{ "ok": false, "error": "..." }` to stderr on failure. Exit status is 0 for success, 1 for an operational failure and 2 for invalid arguments. `--help` succeeds without touching the vault. Do not log stdin or treat parser diagnostics as a place to print credentials. Output never includes the control capability, provider token, credential JSON or provider request/response payload. The [CLI implementation and integration fixtures](../crates/switchboard-cli/src/main.rs) define this envelope.

The local `control.json` contains a capability and is private runtime state. It is not an API discovery file to share. It does not grant authority on LAN: clients require exact loopback host/port, no redirects/proxies, bounded bodies and a per-runtime bearer. Browser Origins are refused. Same-user malicious processes are outside the protection boundary, just as for the official CLIs they could inspect.

## Capture, import and automatic rotation

```sh
switchboard current
switchboard accounts capture --provider claude --pool default
switchboard accounts import-claude-swap --pool default
switchboard accounts activate ACCOUNT_UUID
switchboard rotation set --provider claude --pool default --target claude-cli --enabled true
switchboard rotation status
switchboard rotation set --provider claude --pool default --target claude-cli --enabled false
```

Capture reuses existing local authorization without another login. Import reads Claude Swap and reports imported/failed/skipped counts without credentials. Activate changes the ordinary Claude Code account with identity checks and compatible locks; only captured/imported Claude OAuth profiles qualify. Account selection for the proxy stays separate.

Rotation targets are `managed` and `claude-cli` (serialized `claude_cli`). `--threshold` defaults to 90, `--hysteresis` to 10, `--cooldown` to 1800 seconds, `--max-age` to 300 seconds. Hysteresis is headroom below the threshold. Only one native Claude policy may be enabled across all pools because the ordinary native account is one shared target. Existing streams are never replayed. Configuration persists offline; status explicitly reports whether the owner/monitor runs. Full behavior and limitations: [account and rotation contract](ACCOUNTS-AND-ROTATION.md).
