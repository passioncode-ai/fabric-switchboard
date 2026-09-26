# Fabric Switchboard
Read docs/HANDOFF.md, docs/SPEC.md and docs/CONTRACTS.md before changes.
Scope is this repository only. Never change a user's live Claude/Codex auth, settings, MCP, keychain items, or running agents during tests. Tests use synthetic credentials and temporary homes; native Keychain tests use this app's service only and clean up.
Secrets never enter logs, errors, screenshots, source or events. No secret-returning frontend commands. Account identity is claimed until provider verification; a local JWT decode is not verification.
Use branches codex/*. Commit and push task-owned work under standing operator authorization. No force push. Do not publish releases or claim signing/notarization without evidence.
Full hosted checks are nightly at 23:00 Europe/Warsaw; no push/PR full-suite triggers. Focused local checks first. Windows is planned until run on Windows.
