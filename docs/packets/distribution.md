# Packet DIST-01 — signed delivery

**State:** planned. Local app builds do not satisfy this packet.
**Owns:** release packaging/signing/update metadata; never committed certificates or signing secrets.
**Shared:** [spec §§10,12,13](../SPEC.md), [verification](../evidence/verification.md).

Prerequisites: operator-owned Developer ID/team authorization, release naming/channel decision, successful real-provider acceptance. Build exact reviewed commit with lockfiles, record compiler/SDK and artifact hashes, sign hardened runtime with minimum required entitlements, notarize and staple, verify on a separate clean Mac under normal Gatekeeper settings. Never instruct users to disable Gatekeeper to make a release pass.

Add signed update manifest with independent update key custody, version/rollback policy, explicit restart boundary, interrupted-download recovery and retention of user profiles. No update may replace a running client's credential lineage. Windows gets its own code-signing/SmartScreen acceptance after WIN-01.

Release evidence must name commit, checks, native hosts, hashes, signing/notary result, supported CLI versions and known limitations. Publish only under the repository's release authorization; a pushed source branch is not a signed release. Nightly checks remain scheduled; do not add full push/PR suites as a side effect.
