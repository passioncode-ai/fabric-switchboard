# Packet DIST-01 — signed delivery

**State:** 0.2 packaging/signing tools implemented; [execution receipt](../evidence/release-0.2.md) names actual builds and signature state. Since then: notarized and stapled macOS builds (0.4.0), builds made only by the [release workflow](../DISTRIBUTION.md#how-a-release-happens) (0.5.4), signature-verified automatic updates (0.6.1, SB-55). Open gates: Windows Authenticode (SB-03) and clean-machine Windows acceptance (SB-02).
**Owns:** release packaging/signing/update metadata; never committed certificates or signing secrets.
**Shared:** [spec §§10,12,13](../SPEC.md), [verification](../evidence/verification.md).

Prerequisites for a generally supported release: operator-owned Developer ID/team authorization, release naming/channel decision, successful real-provider acceptance. The operator separately authorized an engineering beta build and its Developer ID signature before live-provider acceptance. Build exact reviewed commit with lockfiles, record compiler/SDK and artifact hashes, sign hardened runtime with minimum required entitlements, notarize and staple, verify on a separate clean Mac under normal Gatekeeper settings. Never instruct users to disable Gatekeeper to make a release pass.

Add signed update manifest with independent update key custody, version/rollback policy, explicit restart boundary, interrupted-download recovery and retention of user profiles. No update may replace a running client's credential lineage. Windows gets its own code-signing/SmartScreen acceptance after WIN-01.

Release evidence must name commit, checks, native hosts, hashes, signing/notary result, supported CLI versions and known limitations. Publish only under the repository's release authorization; a pushed source branch is not a signed release. Nightly checks remain scheduled; do not add full push/PR suites as a side effect.
