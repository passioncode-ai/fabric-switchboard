# Board

Open work that outlives a run. Priority: P1 blocks a user outcome, P2 degrades one, P3 is hygiene.
A row closes only with a receipt (commit, command output or test name).

| ID | P | Item | Source | Status |
|---|---|---|---|---|
| SB-01 | P1 | Live provider acceptance (PA-01): real login, managed request, quota and rotation with live accounts, extended to project rules and `switchboard mcp` from a real agent session | [packets/provider-acceptance.md](../packets/provider-acceptance.md), PLAN-0.4 | open, needs operator accounts |
| SB-02 | P1 | Native Windows acceptance: install, launch, DPAPI vault, PowerShell homes, MCP on Windows | [packets/windows.md](../packets/windows.md) | open, needs a Windows host |
| SB-03 | P2 | Windows code signing (certificate or signing service) | release-0.3.1 evidence | open, operator decision |
| SB-04 | P2 | Stale Claude account-lock takeover after a crash mid-activation (B-19 remainder): needs Claude Code's own lock lease semantics verified | PLAN-0.4 B-19 | open |
| SB-05 | P3 | Windows installer does not embed `switchboard.exe`; agents find it on PATH only | PLAN-0.4 REQ-5 | open |
| SB-06 | P2 | Does a running Claude Code session pick up a native activation without restart? Measure on a live session before documenting a stronger claim | ACCOUNTS-AND-ROTATION, PLAN-0.4 D-1 | open |
| SB-07 | P2 | `project_rules_are_optional_visible_and_applied_on_request` failed once inside `./scripts/check.sh` (second apply returned a tool error instead of `already_in_effect`), then passed 6/6 alone and 5/5 full-workspace runs. The assertion now prints the tool's answer; capture it on the next failure and fix the race | 0.4 run, 2026-09-29 | open |
| SB-08 | P1 | Finish packets P-E (plugin + skill) and P-F (website): both stopped on the session spend limit; state in HANDOFF | 0.4 run, 2026-09-29 | open |
| SB-09 | P1 | Release 0.4.0-beta.1 (REQ-11): versions, notarized macOS, Windows cross-build, GitHub release, site, local install, launcher member | PLAN-0.4 | **done except the launcher member** — [release record](release-0.4.md): tag `v0.4.0-beta.1` = `bd0cf5d`, notary `573e15d6-…` Accepted, anonymous `spctl` accepted, site Worker `2cd961df-…`, `/Applications` 0.4.0; launcher `family.json` member moved to SB-10 |
| SB-10 | P2 | Add `switchboard` to the PassionCode launcher's `family.json` (`ref: v0.4.0-beta.1`, `kind: plugin`, `path: plugins/switchboard`), release the launcher, then rerun the site updater so the page shows the launcher line | handoff 2026-09-30 | open, owned by the launcher repository |
| SB-11 | P2 | Scheduled nightly `checks` job is skipped on every run (the Warsaw-hour gate misses delayed cron starts) while the run reads success | [issue #5](https://github.com/passioncode-ai/fabric-switchboard/issues/5); fix 2026-10-01: `scripts/nightly_clock.py` decides by the planned slot, `scripts/test_nightly_clock.py` in the gate | fix landed; open until a scheduled run shows `checks` running |
| SB-12 | P1 | Post-release acceptance of the Keychain fix: install the next release, open the app once, confirm by ACL metadata (`security dump-keychain -a`, no `-d`) that every account sits under `ai.passioncode.fabric-switchboard.shared` trusting both `ai.passioncode.fabric-switchboard` and `switchboard`, and that `switchboard mcp` shows no dialog | [KEYCHAIN.md](../KEYCHAIN.md#acceptance-after-the-next-release-operator), handoff 2026-10-01 | open, needs the release |
| SB-13 | P3 | Data protection keychain (`kSecUseDataProtectionKeychain` + `keychain-access-groups`): needs an App ID and a Developer ID provisioning profile, and the CLI wrapped in its own app-like bundle | [KEYCHAIN.md](../KEYCHAIN.md#alternative-rejected-the-data-protection-keychain) | open, operator decision |
