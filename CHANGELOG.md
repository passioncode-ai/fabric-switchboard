# Changelog

Release notes for Fabric Switchboard. The release workflow publishes the `## X.Y.Z` section of
this file as the notes of release `vX.Y.Z` ([DISTRIBUTION.md](docs/DISTRIBUTION.md)); the
release pull request renames `Unreleased` to the version. While Windows signing is switched off,
that section must say `windows_authenticode: NOT_SIGNED`. Notes of 0.5.3-beta.1 and earlier are
on their [GitHub releases](https://github.com/passioncode-ai/fabric-switchboard/releases).

## Unreleased

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
