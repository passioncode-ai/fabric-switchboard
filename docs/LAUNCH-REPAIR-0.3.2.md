# Launch and distribution repair — 2026-09-27

## Scope and authorization
Operator requests a rebuilt, launchable app and matching current GitHub/website downloads. Keep existing account/runtime behavior. Application owner: this repository; site owner: passioncode-ai/passioncode-ai.github.io. Existing public beta and website may be updated within that request. No signing key export, global Gatekeeper changes or real-provider test. Current model/settings retained.

## Source ledger and findings
Read AGENTS, HANDOFF, INSTALL, DISTRIBUTION, release-0.3.1 evidence, native setup/runtime, frontend bootstrap, site HANDOFF/DEPLOYMENT/manifest/updater. Source main a1ff940; published binary source 9e20a49. Installed 0.3.1 signature passes strict verification but spctl rejects it as Unnotarized Developer ID; quarantine is present. Developer ID Sheleg available; no notarytool profile metadata found in default Keychain. Local archive opens a window but stays on Loading your workbench; investigate transport separately. No crash report for Switchboard found. Contradictions: passing source tests did not establish native frontend readiness.

## Requirements and checks
- REQ-1: diagnose/rebuild native launch; observe real bundled UI readiness with isolated synthetic state.
- REQ-2: gate packaging on ready native frontend and strict signature; preserve resumable notarization and verify Gatekeeper once Apple profile exists.
- REQ-3: exact source, version, checksums and public archives agree; verify anonymous downloads.
- REQ-4: site selects same release and accurately names remaining platform limits; verify deployed page and redirects.

## Plan / resume
1. Diagnose frontend/native startup; improve bounded recovery and add native smoke verification.
2. Run focused regressions and full gate; build clean 0.3.2 Mac universal and Windows x64.
3. Sign; notarize if authorized Apple credentials become available. Publish exact checked artifacts, update site and receipts.
4. Commit/push owning repositories and verify remote SHAs.

Apple authorization is a material prerequisite: asked for existing notary profile or operator-local setup. Do not claim public launch repair while Gatekeeper rejects the archive. Native Windows and real-provider acceptance remain separate.
