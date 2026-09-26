Contract: brand-contract v1
# Canonical facts
- F-001: macOS universal and Windows x64 source/adapters/builds; native Windows acceptance remains NOT_RUN ([release evidence](../evidence/release-0.2.md), checked 2026-09-26).
- F-002: Managed selection affects next request (SPEC.md §6; implementation verification separately recorded).
- F-003: No provider secrets in metadata (CONTRACTS.md vault boundary; tests required).

- F-004: Capture reuses current CLI authorization; official console sign-in adds another account; Claude Swap import is read-only ([external adapter](../../crates/switchboard-runtime/src/external.rs), synthetic tests).
- F-005: Current CLI identity and managed next-request route are separate ([runtime](../../crates/switchboard-runtime/src/lib.rs), [account contract](../ACCOUNTS-AND-ROTATION.md)).
- F-006: Quota cadence normally 180 seconds, at most 2 due accounts per 10-second pass, failure backoff up to 1800 seconds; actual observation time governs freshness ([monitor](../../crates/switchboard-runtime/src/monitor.rs)).
- F-007: Rotation defaults off; initial editor defaults 90% threshold, 10-point headroom, 1800-second cooldown, 300-second maximum age ([interface](../../src/main.ts), [CLI](../../crates/switchboard-cli/src/main.rs)).
