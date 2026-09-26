# 0.3.2 launch and active-account repair

## Confirmed cause
Installed 0.3.1 passes `codesign --verify --deep --strict` but Gatekeeper rejects it as `Unnotarized Developer ID` and quarantine is present. The process itself starts in an isolated profile. The first transient Loading observation did not establish a persistent release IPC defect; repeated isolated native release and debug runs rendered Accounts.

Real active Claude authorization exists. The 0.3.1 Security.framework read returned unavailable; `/usr/bin/security` read valid OAuth JSON in 0.02 seconds. The fix follows the pinned Claude Swap stable-binary strategy ([research](../RESEARCH-0.3-IMPORTS.md)). The operator explicitly authorized real-account discovery/import in this task. No secret values are recorded here.

## Implemented
Bounded external Keychain reader; 12-second read-only IPC deadlines; account/runtime metadata independent of credential prompts; Import Claude Swap inside Add account; packaged native frontend smoke mode with memory vault/temp store. Public macOS build now requires a notary profile by default; explicit local engineering override remains. Installed CLT fallback is process-local. See [repair plan](../LAUNCH-REPAIR-0.3.2.md).

## Checks and real-account acceptance
Focused: three Keychain child-process tests pass (missing/denied, bounded output, deadline kill/reap); metadata-read-under-mutation-lock regression passes; five frontend deadline cases pass. Full source gate passed: 85 Rust tests passed, 0 failed, 1 opt-in Keychain test ignored; five frontend deadline cases passed; format, Clippy, frontend build and docs checks passed. Final build-script/docs changes received the native smoke harness positive/absent-marker/nonzero/timeout probes and docs validation. Actual corrected CLI sees both current Claude/Codex accounts, saved current Claude, imported 6 Claude Swap profiles with 0 failed/0 skipped, and matches current Claude to a saved profile. No external auth switch, OAuth refresh grant or paid inference performed.

A real quota request for the current saved Claude profile succeeded, returning two windows and an observation timestamp (16% measured at the check, not a lasting quota claim). The new debug bundled app passed native frontend readiness. Builds, signed GUI acceptance and public delivery receipts pending. Apple notary profile unavailable at intake; do not call a signed-only download Gatekeeper-ready. Native Windows and live inference acceptance remain NOT_RUN.
