<sub>ssheleg skills — task-pipeline · ux-scenarios · sheleg-design · copywriting · brand-voice</sub>

# 0.3 engineering delivery evidence

Scope: [account capture and rotation](../ACCOUNTS-AND-ROTATION.md), [plan](../PLAN-0.3.md), [research delta](../RESEARCH-0.3-IMPORTS.md). All credential fixtures are synthetic; actual Claude/Codex authorization was not read or replaced to test this change.

## Source and independent packets

- [Core/proxy](https://github.com/passioncode-ai/fabric-switchboard/commit/0f59012): identity upsert, metadata migration, quota windows/reset parsing, generation checks and policies.
- [External adapters](https://github.com/passioncode-ai/fabric-switchboard/commit/a04de91), [rollback ownership fix](https://github.com/passioncode-ai/fabric-switchboard/commit/8bddbb8): capture, Claude Swap import, native activation and research.
- [UI/scenarios](https://github.com/passioncode-ai/fabric-switchboard/commit/04cd02d): onboarding, active identity, import result, usage details and policies.
- [Atomic route/cooldown and single native policy](https://github.com/passioncode-ai/fabric-switchboard/commit/8fde378).
- [Integrated source and build commit](https://github.com/passioncode-ai/fabric-switchboard/commit/0b415ef6f5c7b8046afe55152a730fd620afab7d) adds the lifetime-bound monitor, current-source cache/sync, CLI/Tauri operations, enriched official login, documentation and version. Both builds used this clean commit; final receipt changes are documentation only.

## Checks

Full integrated gate passed with exit 0: **81 tests passed, 0 failed, 1 opt-in native Keychain test ignored**. TypeScript/Vite, formatting and strict Clippy passed. Documentation check: 34 Markdown files, 182 relative links, 0 errors. Command: `./scripts/check.sh` (TypeScript/Vite, fmt, all Rust tests, clippy with denied warnings, relative docs links, whitespace). Ignored opt-in Keychain fixtures do not count as run. Local command log is retained in ignored `artifacts/check-0.3-final.log`; these counts were computed from its test-result lines.

Root inspected integrated browser demo `127.0.0.1:5177/?demo=1`: separate current/native and managed badges, quota/reset disclosure, policy fields/defaults, import confirmation and partial result (two imported, one failed, one skipped). The UI packet also exercised blank-label capture, enable/save/stop, native activation confirmation/result and 390×844 layout. These are synthetic observations, not native/provider acceptance. Hysteresis wording was corrected to headroom below threshold to match the policy.

External packet Windows x64 `cargo xwin check`: PASS, native execution NOT_RUN. Brand/UX linter scripts are absent: NOT_RUN. Repository documentation validation is separate.

## Artifacts and limits

macOS universal app/CLI: BUILT for arm64 and x86_64 with `python3 scripts/build_macos.py --identity 4E9DE832FB92D08CFD02B215BE1E0EF9B1E854CE --arch universal`. Both signatures verified strictly with Developer ID Application: Sergei Viktorovich Sheleg (KJ35UYYL22). Native CLI smoke passed: version 0.3.0, empty accounts and rotation status in a temporary data directory. No private key was exported. Notarization is NOT_RUN: no saved notarytool profile supplied. Gatekeeper assessment returned exit 3, `Unnotarized Developer ID`; a valid signature alone does not pass Gatekeeper.

Windows x64 CLI/NSIS: BUILT with `python3 scripts/build_windows_cross.py` using cargo-xwin/MSVC target and NSIS. CLI and app PE machine headers are x64; the NSIS bootstrap is x86 with x64 payload. CLI uses static CRT with no external VCRUNTIME import. Native Windows execution is NOT_RUN and Authenticode is NOT_SIGNED: no Windows certificate is available. Hosted checks remain billing-blocked as recorded for 0.2; no full hosted suite was dispatched.

[Draft engineering release](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/untagged-d7c7cf8b04d5f90c39c1) contains both archives and their individual receipts. All four assets were downloaded again with `gh release download v0.3.0-beta.1`; SHA-256 matched their local originals. [Machine-readable combined receipt](artifacts-0.3.json) records hashes, toolchains, signatures and acceptance status. Archive SHA-256: macOS `ec9158a1f5f55320e23c0a80e5a68630d9ff8ef8981f84a703c9992b0ddab66e`; Windows `cc70df00f3930250536963d6dcaadfa5a9134613f20eb452fc9d0d6ef329021f`.

Real provider login, live token adoption timing, actual usage responses, native Windows execution, screen-reader acceptance and unattended long-running rotation: NOT_RUN. Inactive OAuth snapshots can expire; only the current official client's newer generation is adopted automatically. Unsupported Codex secrets/ephemeral stores report unavailable.

## Delivery and resume

Source commit `0b415ef6f5c7b8046afe55152a730fd620afab7d` was pushed to `origin/codex/bootstrap`; `git ls-remote` matched. A separate fresh remote clone ran `npm ci` and the full `./scripts/check.sh` successfully: 81 passed, 0 failed, 1 ignored; TypeScript/Vite, formatting, strict Clippy and docs validation passed (34 Markdown files, 182 relative links, 0 errors at the source commit). Local ignored logs: `artifacts/check-0.3-final.log` and `artifacts/fresh-check-0.3.log`. Final receipt-only changes receive a docs/whitespace check and remote equality verification at handoff; they do not change the binaries. Entry: [HANDOFF](../HANDOFF.md). Historical 0.2 receipts/artifacts remain separate.

---

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**

- [`task-pipeline`](https://github.com/ssheleg/task-pipeline) — bounded implementation and Git delivery
- [`ux-scenarios`](https://github.com/ssheleg/super-ux) — current auth and rotation scenarios
- [`sheleg-design`](https://github.com/ssheleg/sheleg-design-skill) — Workbench UI extension
- [`copywriting`](https://github.com/ssheleg/super-ux) — interface and CLI recovery text
- [`brand-voice`](https://github.com/ssheleg/super-ux) — canonical feature facts
