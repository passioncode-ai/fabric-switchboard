# A sign-in store for a machine with no graphical login (SB-287)

Design note, 2026-11-07. Status: decided, not built. Owner: fabric-switchboard. Peers: studio-agent
(Mac Studio spaces), Project Observatory (OBS-361, the LaunchDaemon route), Linux member VMs (SB-88, SB-292).

## What is asked

studio-agent (board `d3848fb7`, then session `d53abe38` with the operator's words «давай доводи сам
до конца, все автономно без меня») needs Switchboard's `serve` to keep a space's sign-ins on a
Mac Studio space that has no graphical login. Observatory's OBS-361 runs a space's jobs as
LaunchDaemons with `UserName`, installed by a root-owned validator, and asks Switchboard for a store
that works there.

## What is true today (evidence)

- Every opener builds the platform vault: `serve` (`Runtime::native`,
  `crates/switchboard-runtime/src/lib.rs:2876`), the app (`Runtime::desktop`, `lib.rs:2881`), and a
  CLI or MCP call with no owner running (`execute_offline`, `lib.rs:3570`). The vault is the
  Keychain on macOS, DPAPI on Windows and the Secret Service on Linux
  (`crates/switchboard-core/src/vault.rs:40`).
- A macOS user with no graphical login has no unlocked login keychain: `serve` under a LaunchDaemon
  answers `Credential storage unavailable`; started from an SSH session after
  `security unlock-keychain`, it stores (studio-agent finding (b), backlog SB-287).
- studio-agent measured that Studio spaces have no `gui/<uid>` launchd domain and that `user/<uid>`
  refuses agents (backlog SB-287, 2026-11-07).
- Linux without a session bus has no Secret Service either, and storage refuses and names the fix
  (`crates/switchboard-core/tests/storage.rs:793`, test `linux_vault_refuses_without_a_session_bus`).
  A member's VM (SB-292) is that case.
- Two more secrets live in the same platform store: the backup key (`BackupKey`,
  `crates/switchboard-core/src/backup.rs:28`, Keychain, DPAPI or Secret Service) and the settings
  signing key (SB-272, its vault item named in `settings-signing.json`).
- Where no keychain exists, Claude Code keeps its own sign-in in a 0600 file,
  `.credentials.json` in its config home, which Switchboard already reads
  (`crates/switchboard-runtime/src/external.rs:340`).

## Options

| Option | What it gives | Why not, or why |
|---|---|---|
| Unlock the login keychain at boot | the Keychain as today | needs the account password stored somewhere readable at boot: a worse secret than the tokens it protects |
| The System keychain | a keychain with no login | only root writes it; the daemon runs as the space's user (`UserName`) |
| A file encrypted with a key on the same disk | nothing an attacker with the disk lacks | obfuscation; it also needs a key store, which is what is missing |
| **A file store, 0600 in the data folder** | sign-ins kept with the protection Claude Code itself uses there | **chosen** |

## Decision

1. **A `FileVault`** (`crates/switchboard-core/src/vault.rs`) keeps each item as
   `<data>/vault/<id>` (folder 0700, files 0600, written by `private_fs::private_write`, read with
   `read_private_strict` under a size cap; ids are the UUIDs the store already uses, anything else
   refused). It holds the account credentials, the backup key and the settings signing key, so a
   headless space needs no other store.
2. **The choice is the data folder's, not the process's.** `switchboard vault use file|native`
   writes `<data>/vault.json` (`{"kind":"file","since":<time>}`, 0600). Every opener (`serve`, the
   app, offline CLI and MCP calls) builds its vault through one function, `vault::open(root,
   consent)`, which reads that file. A flag on `serve` alone would let an offline `switchboard
   accounts` write into the Keychain while the daemon reads files.
3. **Only where the platform store does not work.** `vault use file` is refused while the native
   store answers a probe (a write and delete of a throwaway item): a person's Mac keeps the
   Keychain. The refusal names the reason. `vault use native` is always allowed.
4. **Moving between them.** Switching moves every item the store names from the old vault to the
   new one, then deletes the old copy; an item that cannot be read stays and is listed
   (`not_moved`). A fresh headless space has nothing to move: its accounts are signed in there
   (SB-256 headless sign-in).
5. **What a person and an agent see.** `switchboard status` and MCP `switchboard_status` name the
   vault kind; the log writes `vault_kind {kind}` once at start; the window shows nothing new (it
   never runs headless). `switchboard uninstall` removes `<data>/vault/` and `vault.json`.
6. **The daemon definition.** Switchboard documents the LaunchDaemon it runs under (label,
   `UserName`, `HOME`, `ProgramArguments` `switchboard serve --data-dir <space data>`,
   `KeepAlive {SuccessfulExit: false}`); Observatory's validator installs it (OBS-361). Switchboard
   installs no daemon itself.

## Threat model

The file store protects a sign-in exactly as well as the space's user account does: anyone who can
read that user's files, or root, can read the tokens. That is the protection Claude Code and Codex
give their own sign-ins on a machine with no keychain. Backups made in a file-store space are
encrypted with a key kept in the same store, so they protect nothing extra on that disk and still
protect a copy taken elsewhere.

## Tests to write with the code

- a file vault puts, reads and deletes an item, with modes 0700 and 0600, and refuses an id that is
  not a UUID or a file that is a link;
- `vault use file` is refused while the native store works (a fake native store), and allowed when
  it does not;
- an offline CLI call in a data folder marked `file` reads the file store, not the native one;
- switching moves every item and lists one it cannot read;
- uninstall removes the file store.

## Not in this note

At-rest encryption bound to hardware (Secure Enclave, TPM) needs a login session or signed
entitlements on macOS; it is a later option, not a blocker.
