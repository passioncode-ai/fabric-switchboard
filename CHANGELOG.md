# Changelog

Release notes for Fabric Switchboard. The release workflow publishes the `## X.Y.Z` section of
this file as the notes of release `vX.Y.Z` ([DISTRIBUTION.md](docs/DISTRIBUTION.md)); the
release pull request renames `Unreleased` to the version. While Windows signing is switched off,
that section must say `windows_authenticode: NOT_SIGNED`. Notes of 0.5.3-beta.1 and earlier are
on their [GitHub releases](https://github.com/passioncode-ai/fabric-switchboard/releases).

## Unreleased

### Usage analytics

- Release builds count installs, days of use and connected accounts — numbers and kinds only,
  never account names, e-mail addresses, sign-ins or pool names — with PassionCode's own
  analytics server. A random installation number shared by PassionCode apps on this computer
  lets one person using several of them count once. Turn it off in About → *Share anonymous
  usage counts*; that switch applies to every PassionCode app. What is sent:
  [ANALYTICS.md](docs/ANALYTICS.md).

### Stays running

- Closing the window no longer quits Switchboard: quota checks, automatic switching, sign-in
  renewal and backups keep running. A menu-bar icon (on Windows, in the notification area)
  opens the window again or quits Switchboard.
- Switchboard opens at login, in the background, without a window. Turn it off in About →
  *Open at login*. `switchboard uninstall` removes the login item.

### Quota checks

- Accounts nobody is using are checked every ten minutes instead of every three, and again just
  after one of their limits resets; the account in use, the one your managed sessions use and
  every account in a pool that switches automatically stay on three minutes. Without automatic
  switching, numbers now count as current for 15 minutes, so idle rows no longer read stale.

### Interface

- The account list is compact again: each row's quota fits in two lines — the meter with the
  share used and how long ago it was checked, then the countdown and date of the next reset (or
  when a limited account is back, or when the next check runs). A coloured mark on the provider
  icon shows the account's state at a glance: available, running low, limited, stale or failed.
  The limit details moved into the row's quota disclosure, which now lists each window on one line.

## 0.5.4-beta.3 — 2026-10-05

Switchboard now stays quiet while nothing changes, and it respects every limit the providers
report: no quota check before the time a provider asked for, no feature limit mistaken for the
whole account, no estimated hold shown as a reset. Signed by the same team (`KJ35UYYL22`) as
0.5.3, so the Keychain's trust carries over.
(The `v0.5.4-beta.1` and `v0.5.4-beta.2` tags never became releases: their runs stopped in the
Windows job — first on the encoding of these notes, then on a Windows build error — and both
are fixed here.)

**Windows:** windows_authenticode: NOT_SIGNED. The archive is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Quota, limits and accounts

- `switchboard uninstall` shows what Switchboard created on this Mac and, with `--yes`, removes
  it: saved accounts' credentials, the old unused vault key, the `~/.local/bin` link and the data
  folder's own files. Backups and the Claude Code and Codex sign-ins are never touched.
- Finishing a sign-in whose temporary folder could not be removed now reports the account as
  added (with a note) instead of an error, and finishing it again returns the same account; the
  folder is removed on the next try.
- An account held after a limit error shows whether the time is the provider's own reset
  (*Limit resets*) or an estimate (*Retry hold until*). Holds survive a restart, and a new
  Claude Code session's limit is charged to the account it runs on even when its reset time
  happens to match another account's.
- Codex accounts show the limits the Codex app itself reads beyond the two usage windows: a
  personal spend limit, a workspace out of credits or at its usage limit, and each feature's
  own limit. A limit on one feature no longer marks the whole account as used up; spend and
  workspace limits block it whatever the percentages say.
- When a provider answers a quota check with “too many requests”, nothing checks that account
  again before the time it asked for — not the background, not Check usage, not
  `switchboard usage` or MCP. Its `Retry-After` is read in seconds or as a date and kept whole
  (up to seven days; 15 minutes when it gives none). The wait survives a restart and ends at
  once with a new sign-in. A row whose check failed shows when the next check runs. A quota
  check no longer holds up other actions while the provider answers, and two checks of one
  account share one request.
- Account rows show fresh remaining quota first within each pool, followed by the shortest
  known wait. Reset dates include a countdown in days, hours and minutes; Pause countdown
  keeps the displayed duration still. Unknown or stale quota stays labelled.
- Partial quota responses no longer make unobserved windows appear fresh. Backups created
  in the same second keep separate copies, and session attribution reads timestamp metadata
  without materializing conversation content.

### Lifecycle

Version 0.5.4: the organization's [lifecycle contract](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/lifecycle.md)
applied to Switchboard ([PR #23](https://github.com/passioncode-ai/fabric-switchboard/pull/23);
[AGENTS.md → Lifecycle](AGENTS.md#lifecycle)).

- **No Keychain read on a timer.** The background looks at Claude Code's and Codex's sign-in
  sources without decrypting anything or starting a process — file stamps, the Keychain item's
  attributes and access list — and reads only when one of them changed. A locked keychain, a
  refusal or an item that would ask is remembered and not tried again until it changes or a
  person acts (a locked keychain is looked at again after 1, then up to 30 minutes), so no
  background loop can raise a Keychain dialog. Capture, a person's action, still reads directly.
- **Idle means idle.** One background pass every 30 s instead of 10 s; renewals are rescheduled
  only when credentials change; Claude Swap is found in the process table without running `ps`;
  a quota check that finds nothing new is not written (flushed within 15 minutes and at stop);
  the window polls a sign-in only while one waits.
- **Managed sessions survive a restart or an update.** The proxy's port and token are kept in
  `proxy.json` and reused; if another program took the port, the proxy moves, keeps the token
  and repoints the managed homes.
- **A clean stop.** Quit, the last window, `SIGTERM` and `SIGINT` (app and `switchboard serve`)
  run one drain with an 8 s deadline and a hard exit at 10 s; `control.json` names its owner's
  pid and is removed at stop, and one left by a dead owner counts as stale.
- **A bounded log** of codes and numbers only in `~/Library/Logs/Fabric Switchboard/`
  (5 × 5 MB, 0600).
- `/usr/bin/security` calls and the backup write no longer hold the async workers (SB-23).
- Local builds keep the current and the previous release in `artifacts/` and unregister removed
  app bundles.
- `--background` starts the app without showing its window or taking focus (and, on macOS,
  without a Dock icon), as the local lifecycle broker does for apps it keeps running; opening it
  again shows the window. Unknown launch arguments are ignored, and App Nap no longer slows the
  account service while no window is open.
- A `SIGTERM` or `SIGINT` that arrives while the app or `switchboard serve` is still starting
  now runs the normal drain instead of ending the process with its control file left behind.

## 0.5.3-beta.2 — 2026-10-04

The first release built, signed and published by GitHub Actions instead of a laptop, with the
Windows fixes its rehearsals found. The app is the same 0.5.3, signed by the same team
(`KJ35UYYL22`), so the Keychain's trust in it carries over.

**Windows:** windows_authenticode: NOT_SIGNED. The archive is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

- **Windows: an elevated session's own files count as its own.** An elevated process owns new
  files as `BUILTIN\Administrators`, so storage it had just created read as foreign and was
  refused.
- **Windows: a stale control file falls back as it should.** Windows refuses a connection to a
  port nobody listens on only after about two seconds; the bound is now five, so a left-over
  `control.json` leads to the lock-guarded fallback instead of *Control connection did not
  complete*.
- **Windows builds are checked every night.** Native tests, the unsigned release build, and a
  clean-tree check after each. Text checks out as LF on every platform, so a Windows build no
  longer looks like a changed source tree.
- **Releases are built and signed only in GitHub Actions.** A `vX.Y.Z` tag starts
  `.github/workflows/release.yml` in the protected `release` environment; a member of
  `release-approvers` approves (whoever pushed the tag may). macOS: the
  universal app and CLI are signed with the organization's CI Developer ID, the app is notarized
  and stapled, and the standalone CLI is notarized on its own (a bare executable cannot be
  stapled; the receipt names both submissions). Windows: built natively on `windows-latest`, with Azure Artifact Signing
  ready behind `AZURE_SIGNING_ENABLED`, which is off for now. Every file is attested (Sigstore),
  summed in `SHA256SUMS` and signed with the organization's release key (`SHA256SUMS.asc`).
  Building and signing on a laptop remain for debugging and are never published.
- **No team id in the code.** The Keychain trust check reads the team only from
  `SWITCHBOARD_SIGNING_TEAM` at build time (the release environment's `APPLE_TEAM_ID`); a build
  without it is a development build ([KEYCHAIN.md](docs/KEYCHAIN.md)).
- The manual `build-windows.yml` workflow is replaced by the release workflow; a Windows build
  of a commit is a rehearsal on an `-rc` tag.
