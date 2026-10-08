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

## Recovery receipt

At 13:01 UTC the old owner drained in 44 ms and the installed 0.6.10 owner started
from the application bundle. The metadata-only LaunchAccount check changed from
HTTP 400 to HTTP 200 with a resolved account, and the proxy descriptor was byte-identical.
This proves the compatibility blocker removed, not a completed FD-25 embedded-console run.

At 13:06 UTC the last configured Claude identity was recovered. The live Keychain object
contained only shared MCP keys. Under the two native credential locks, with identity and
unchanged-value checks, that signed-out envelope was normalized to an empty provider OAuth
section understood by installed 0.6.10. Secret bytes stayed in memory and on stdin to
`security -i`; no secret was written to argv, a log, this repository or a plaintext backup.
Locks were released, then the bundled CLI performed its normal `accounts activate` against
the saved identity (exit 0). The shared MCP values matched before/after. This was authorized
live recovery, separate from all synthetic regression testing. No login flow was launched.

Official `claude auth status`: loggedIn=false/exit 1 before; loggedIn=true,
authMethod=claude.ai/exit 0 after. `switchboard current`: both providers available;
Claude matches the pre-incident configured identity; Codex matches its initial identity.
All 29 inventoried Orca terminals remain connected: 20 Claude, 4 Codex, 1 Kimi, 4 shells.
The current project terminal retained the same handle and Codex identity. No terminal input
was sent, no agent was stopped, and rotation policies and empty fallback chains were retained.

## Implementation and verification

`external::wiped` now recognizes an object containing only known shared MCP/plugin keys
with its provider OAuth section absent. Unknown envelopes, scalar/array JSON and malformed
JSON remain refused; a missing Keychain item beneath a configured account remains refused.
The same predicate governs capture and activation under the existing native locks.

- Fail-first: `cargo test -p switchboard-runtime mcp_only_live_item --locked` exited 101
  before the fix: Unsupported OAuth JSON schema instead of the signed-out result.
- `cargo test -p switchboard-runtime external::tests --locked`: 41 passed after the fix.
- Rejection coverage additionally exercises activation and verifies unchanged bytes.
- `npm ci --ignore-scripts`: exit 0; audit reported one existing high-severity dependency
  advisory, not altered as part of account recovery.
- UX linter: NOT_RUN, `docs/ux/lint.py` does not exist. SCN-028 updated; no visual or copy change.
- Full gate: `./scripts/check.sh` exit 0; 480 Rust tests passed, 3 opt-in tests ignored;
  build, UI checks, clippy, documentation and script gates passed. Initial attempts caught a
  typo in the new rejection fixture field and a trailing blank line; both corrected.

## Remaining scope and exact next task

The operator's closed Codex-to-Claude terminal cannot be attributed from these observations:
Orca session search is disabled, Switchboard fallback chains are empty and retained lifecycle
logs contain no fallback events. Do not claim the missing provider sign-in caused a process
replacement, or that a restart proves every active conversation can generate a response.
No release approval was given and no release was published by this run.

Next: include this fix in the next approved signed release; then verify the installed build
recognizes the signed-out envelope through the synthetic suite's acceptance receipt. If a
provider changes again, retain the affected Orca terminal and capture its handle, launch command
and runtime provider before closing it. FD-25 can retry its own in-place console acceptance.

## Skills actually used

- task-pipeline: repository four-stage profile, bounded requirements and fail-first evidence.
- agent-sync: task/file leases and guarded handoff.
- ux-scenarios: SCN-028 recovery edge and coverage.
- orca-cli: read-only terminal inventory and history capability check.
- project-reports: discovery of earlier project reports; this file is task evidence, not a report.

Router sources: repository AGENTS.md and pipeline.json; no design or user-facing copy changed.

## Source and delivery

Implementation: [282076a](https://github.com/passioncode-ai/fabric-switchboard/commit/282076af5517d257df0f68b74ec7ab0a64fc558d).
The installed app remains the published 0.6.10; the source fix needs the next approved release.
Local build cache measured 4.3 GB, below the 10 GB cap. No test owner or native lock remains.
Hosted nightly CI and Windows/live generation acceptance were not run by this task.
