<sub>ssheleg skills — task-pipeline · ux-scenarios · sheleg-design · copywriting · brand-voice</sub>

# 0.3 engineering delivery evidence

Scope: [account capture and rotation](../ACCOUNTS-AND-ROTATION.md), [plan](../PLAN-0.3.md), [research delta](../RESEARCH-0.3-IMPORTS.md). All credential fixtures are synthetic; actual Claude/Codex authorization was not read or replaced to test this change.

## Source and independent packets

- [Core/proxy](https://github.com/passioncode-ai/fabric-switchboard/commit/0f59012): identity upsert, metadata migration, quota windows/reset parsing, generation checks and policies.
- [External adapters](https://github.com/passioncode-ai/fabric-switchboard/commit/a04de91), [rollback ownership fix](https://github.com/passioncode-ai/fabric-switchboard/commit/8bddbb8): capture, Claude Swap import, native activation and research.
- [UI/scenarios](https://github.com/passioncode-ai/fabric-switchboard/commit/04cd02d): onboarding, active identity, import result, usage details and policies.
- [Atomic route/cooldown and single native policy](https://github.com/passioncode-ai/fabric-switchboard/commit/8fde378).
- Root integration adds the lifetime-bound monitor, current-source cache/sync, CLI/Tauri operations, enriched official login, documentation and version.

## Checks

Full integrated gate passed with exit 0: **81 tests passed, 0 failed, 1 opt-in native Keychain test ignored**. TypeScript/Vite, formatting and strict Clippy passed. Documentation check: 34 Markdown files, 182 relative links, 0 errors. Command: `./scripts/check.sh` (TypeScript/Vite, fmt, all Rust tests, clippy with denied warnings, relative docs links, whitespace). Ignored opt-in Keychain fixtures do not count as run. Local command log is retained in ignored `artifacts/check-0.3-final.log`; these counts were computed from its test-result lines.

Root inspected integrated browser demo `127.0.0.1:5177/?demo=1`: separate current/native and managed badges, quota/reset disclosure, policy fields/defaults, import confirmation and partial result (two imported, one failed, one skipped). The UI packet also exercised blank-label capture, enable/save/stop, native activation confirmation/result and 390×844 layout. These are synthetic observations, not native/provider acceptance. Hysteresis wording was corrected to headroom below threshold to match the policy.

External packet Windows x64 `cargo xwin check`: PASS, native execution NOT_RUN. Brand/UX linter scripts are absent: NOT_RUN. Repository documentation validation is separate.

## Artifacts and limits

macOS universal app/CLI: PENDING BUILD. Use existing operator-authorized Developer ID without exporting its private key. Notarization needs a saved notarytool profile; signature is not notarization.

Windows x64 CLI/NSIS: PENDING BUILD. Cross-compilation does not establish native behavior. No Windows Authenticode certificate is available. Hosted checks remain billing-blocked as recorded for 0.2; no full hosted suite was dispatched.

Real provider login, live token adoption timing, actual usage responses, native Windows execution, screen-reader acceptance and unattended long-running rotation: NOT_RUN. Inactive OAuth snapshots can expire; only the current official client's newer generation is adopted automatically. Unsupported Codex secrets/ephemeral stores report unavailable.

## Delivery and resume

Source/evidence will be pushed and checked from a fresh remote checkout before final handoff. Entry: [HANDOFF](../HANDOFF.md). Historical 0.2 receipts/artifacts remain separate.

---

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**

- [`task-pipeline`](https://github.com/ssheleg/task-pipeline) — bounded implementation and Git delivery
- [`ux-scenarios`](https://github.com/ssheleg/super-ux) — current auth and rotation scenarios
- [`sheleg-design`](https://github.com/ssheleg/sheleg-design-skill) — Workbench UI extension
- [`copywriting`](https://github.com/ssheleg/super-ux) — interface and CLI recovery text
- [`brand-voice`](https://github.com/ssheleg/super-ux) — canonical feature facts
