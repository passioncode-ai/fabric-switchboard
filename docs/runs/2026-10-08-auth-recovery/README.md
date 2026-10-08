# Session and sign-in recovery — 2026-10-08

## Scope and request
Restore the operator's working sessions after reported provider confusion and sign-in loss.
Keep existing terminals alive, preserve Codex authorization and shared MCP sign-ins.
Use the repository four-stage pipeline profile; same current model throughout.

## Requirements and checks
- R1: repair installed owner/CLI skew; metadata-only LaunchAccount before/after.
- R2: recover the previously active Claude identity; official auth status and identity match.
- R3: fix activation after an absent OAuth section; synthetic regression and rejection tests.
- R4: preserve terminal identities; compare Orca inventory before/after, no terminal sends.
- R5: push a resumable handoff; remote SHA and fresh-checkout link checks.

## Source ledger
- AGENTS.md, pipeline.json, docs/CONTRACTS.md: control, activation and live-auth boundaries.
- docs/ACCOUNTS-AND-ROTATION.md; docs/ux/scenarios.md SCN-028: account recovery.
- docs/HANDOFF.md at 9b5b1e1: 0.6.11 release pending; unrelated release work excluded.
- docs/evidence/retro.md: preserve locks, identity, secret lineage; fail-first regression.
- Fabric Workspace knowledge rules and roadmap: RM-20 continuation context.
- Live sanitized observations: old owner rejects LaunchAccount (HTTP 400); after graceful
  restart into installed 0.6.10 it returns HTTP 200 with an account id. Proxy descriptor
  unchanged. No fallback chains configured; no fallback events in retained lifecycle log.
- Official Claude auth status: loggedIn=false; live Keychain JSON remains an object with
  MCP OAuth data but no claudeAiOauth section. Native activation refuses with
  Unsupported OAuth JSON schema. Configuration still identifies the last activated account.
- Orca inventory: current project terminal is Codex. Operator says affected terminal
  was closed. Session search disabled; original provider change is not yet attributable.

Contradictions: owner restart fixes command compatibility, not the missing Claude login.
No evidence currently attributes a Codex-to-Claude conversion to Switchboard.

## Plan and recovery boundaries
1. Complete read-only diagnosis and graceful installed-owner restart (done).
2. Reproduce missing OAuth section in synthetic fixtures, then recognize only a JSON
   object whose OAuth section is absent as signed out. Preserve all existing refusal rules.
3. Recover the saved identity using owned locks and native activation; keep shared MCP data.
   This is operator-requested recovery, never a fixture or provider acceptance test.
4. Run focused tests and repository gate, document limits, push branch and verify.

## Resume
Implement synthetic regression in external.rs; no other repository changes or release
approval. Existing task lease: auth-recovery-20261008.
