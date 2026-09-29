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
| SB-09 | P1 | Release 0.4.0-beta.1 (REQ-11): versions, notarized macOS (`--notary-profile fabric-notary`), Windows cross-build, GitHub release, site, local install, launcher member | PLAN-0.4 | open |
