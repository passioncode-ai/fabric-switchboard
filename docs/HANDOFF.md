# Released — Switchboard v0.6.1 (2026-10-05)

[Release record](evidence/release-0.5.md#release-v061-2026-10-05--automatic-updates-other-agents-complete-backups).
New since 0.6.0: **automatic updates** on by default (SB-55, #68) — the installed app checks
`latest.json` 90 s after start and every 6 h, verifies and installs on its own; **the 30 most used
coding agents** work through Switchboard's tools, proxy or launch (SB-56, #69,
[AGENT-SUPPORT.md](AGENT-SUPPORT.md)), with a public page at
`https://passioncode.ai/switchboard/agents/` (passioncode-ai.github.io #46); **complete backups**
carry projects, rules, selections and settings and restore a new install (#66); lost credentials
of known accounts come back and uninstall keeps session history (#67). Installed here; its first
update check answered `current`. Copies at 0.6.0 or earlier need one manual update.

**SB-52 built** (in review, `agent/sb-52-continuation`): `switchboard continue <wf> --account <id>
--dir <checkout>` hands an Observatory workflow to another account of the same provider and
launches its session ([run record](packets/n-018-continuation.md#run-record-2026-10-05),
CONTRACTS → Workflow continuation, SCN-038). Found an engine issue on the way (SB-58).

**Exact next task:** SB-49 (idle CPU ≈ 0.29 %), then SB-57 (sign-in source flapping), SB-07,
SB-16; SB-52 next steps are a desktop entry and the `switchboard_continue` MCP tool. **Waiting on the operator:** whether Fabric apps add `iid` and
`environment` to analytics events so sshlg-growth counts them (asked by the growth session
2026-10-05; ANALYTICS.md changes with it); SB-15 day of use incl. a project with two
repositories and the first automatic update at the next release; SB-02 Windows; SB-03 Azure
signing; `switchboard uninstall --yes` acceptance; a live third-party agent run needs an API-key
account. **Other apps:** analytics integration passioncode-ai/fabric#12, passioncode-ai/fabric-inbox#27.

---

# Released — Switchboard v0.6.0, the first stable release (2026-10-05)

[Release record](evidence/release-0.5.md#release-v060-2026-10-05--the-first-stable-release).
New since 0.5.5: **projects** reserve their own accounts — a project is one or more folders with a
pool of its own; launches outside its folders, foreign accounts inside them, native activation,
native rules and native rotation of its accounts are refused ([CONTRACTS → Projects
(0.6)](CONTRACTS.md), SCN-034); a **five-step first-run tour** (SCN-035); the Windows installer
embeds `switchboard.exe` (SB-05). SB-51 done (growth registry), SB-31 done. Installed here.

**Exact next task:** SB-52 — build the N-018 continuation skeleton from its
[packet](packets/n-018-continuation.md) (operator's yes to the Observatory MCP entry is recorded).
Then SB-49 (idle CPU ≈ 0.29 %), SB-07, SB-16. **Operator:** SB-15 day of use incl. a project with
two repositories, SB-02 Windows, SB-03 Azure signing, `switchboard uninstall --yes` acceptance.
**Other apps:** analytics integration passioncode-ai/fabric#12, passioncode-ai/fabric-inbox#27.

---

# Released — Switchboard v0.5.5-beta.1 (2026-10-05)

[Release record](evidence/release-0.5.md#release-v055-beta1-2026-10-05). Operator decisions of
2026-10-05 built and shipped: compact two-line account cards with a status mark (#54); adaptive
quota cadence SB-48 — 180 s for rotation pools, the managed route and the CLI's account, 600 s
otherwise and just after a reset (#55); residency SB-28 — close hides, tray Open/Quit, login item
with `--background` (#56); anonymous usage analytics SB-50 with a PassionCode-wide installation id
([ANALYTICS.md](ANALYTICS.md), #57). Installed here; the broker target is `on_demand` (no idle
stop, policyRevision 6) so a Quit stays a Quit. Agents may release when the operator asks
([DISTRIBUTION.md](DISTRIBUTION.md)).

**Exact next task:** SB-49 — read the idle hour of 0.5.5 (sampler `target/tmp/idle.sh`, log
`target/tmp/idle055.log`, both untracked) into the release record and decide whether the 0.2 %
target holds. Then SB-51 (growth registry; asked the growth session), SB-07, SB-16 (needs the
operator's go). **Operator:** SB-15 day of use incl. close/reopen/login on this Mac, SB-02
Windows, SB-03 Azure, SB-31 stable timing, `switchboard uninstall --yes` acceptance.
**Other apps:** analytics integration tasks passioncode-ai/fabric#12, passioncode-ai/fabric-inbox#27.

---

# Released — Switchboard v0.5.4-beta.3 (2026-10-05)

[Release record](evidence/release-0.5.md#release-v054-beta3-2026-10-05). Published from `023314c` by run
37238681118; verified download; site serves it; installed here (0.5.3-beta.2 kept for rollback); the
lifecycle broker now starts Switchboard with `--background` (enrolled, policyRevision 5). Since the
last handoff also landed: SB-42 (structured sign-in result), SB-29 (`switchboard uninstall`), SB-43
(MCP client matrix), SB-46 (docs reconciled), SB-47 packet for Fabric COM-11 (waits on Fabric
COM-01/COM-03). **Next autonomous:** SB-07 (instrumented, waiting for a failure), SB-05, SB-16.
**Operator:** SB-15 day of use, SB-02 Windows, SB-03 Azure, SB-28/SB-48 product decisions,
SB-31 stable release, `switchboard uninstall --yes` acceptance (F8 orphan item).

---

# SB-30 — background start (2026-10-04)

[Run record](runs/2026-10-04-sb-30-background/README.md). `--background` starts hidden with no focus
(measured); unknown arguments are ignored. **Post-install step (after a release with SB-30):** enrol
the broker target — `backgroundLaunch` on for `switchboard.desktop` (local-lifecycle owner CLI,
`docs/runbooks/local-lifecycle.md` in `~/DATA/sshlg-personal-os`) — then check that closing the
window and the broker's restart bring no window forward.

---

# SB-41 — typed, attributable limit evidence (2026-10-04)

**Start here:** [run record](runs/2026-10-04-sb-41-limit-evidence/README.md); contract in
[CONTRACTS](CONTRACTS.md#typed-attributable-limit-evidence-sb-41-2026-10-04). Limit holds now carry
kind, the provider's reset apart from the estimated hold, confidence and an unknown scope; a new
session's limit is charged to the account it runs on even with an equal reset; holds survive a
restart (`limit-evidence.json`). Gate exit 0, 346 Rust tests. SB-25 still needs SB-06's live probe;
the code side of its prerequisites (SB-41) is in place. **Next autonomous tasks:** SB-30, SB-43,
SB-42, issue #36, SB-29, SB-07.

---

# SB-40 — Codex limits beyond the two windows (2026-10-04)

**Start here:** [run record](runs/2026-10-04-sb-40-codex-limits/README.md); contract in
[CONTRACTS](CONTRACTS.md#codex-limits-beyond-the-primary-and-secondary-windows-sb-40-2026-10-04).
Codex observations now carry spend control, workspace credit/usage limits, a reached limit no
window shows, and each metered feature's windows — as ordinary windows, so 0.5.3 still opens the
store. Rotation, the list, CLI and MCP weigh the account's own windows. Gate exit 0, 337 Rust
tests. **Exact next autonomous code task:** SB-41 — typed, attributable limit evidence
(`docs/reports/2026-10-04-system-review/raw/sessions.md` SES-B).

---

# SB-39 — one provider not-before for every quota check (2026-10-04)

**Start here:** [run record](runs/2026-10-04-sb-39-quota-deadline/README.md) (brief, decisions, REQ
table, verification, review rulings); contract in [CONTRACTS](CONTRACTS.md#provider-not-before-for-quota-checks-sb-39-2026-10-04);
runbook entry in [OPERATIONS](OPERATIONS.md). Board: SB-39 done, SB-44 (stop signals at start-up)
found by the gate and done.

A provider 429 on a quota check now sets a not-before that the desktop, `switchboard usage`,
MCP, the offline CLI and the background all honour (`monitor::check` → `UsageGate`), kept in
`<data>/usage-holds.json` across restarts and bound to the token generation it answered.
`Retry-After` is read in seconds and every RFC 9110 date form; the six-hour cap became seven
days. A manual quota check no longer holds the owner's transaction; refresh grants stop at the
start of the quit drain. Gate: `./scripts/check.sh` exit 0, 327 Rust tests.

**Not released.** Source is 0.5.4 with SB-35…39 and SB-44 under `## Unreleased`; the next release
is SB-34 (`v0.5.4-beta.1`), which needs the nightly run on the merged `main` and a release
approver. **Exact next autonomous code task:** SB-40 — Codex additional limits, credits and spend
control; official schema pinned at `openai/codex@afb436d` (`codex-rs/backend-client/src/client.rs`
`rate_limit_snapshots_from_payload`, `codex-backend-openapi-models`).

---

# Quota ordering and system review — 2026-10-04

**Start here:** [handoffs/2026-10-04-quota-review.md](handoffs/2026-10-04-quota-review.md).
The [unified plan](runs/2026-10-04-quota-review/PLAN.md) maps every current block and carry-over
task; the [dated report](reports/2026-10-04-system-review/README.md) carries primary sources and
developer threads. Canonical rows SB-35…38 cover quota ordering/date/countdown plus reproduced
freshness, backup-generation and session-timestamp repairs. Actual checks and browser limits:
[verification](evidence/quota-order-2026-10-04.md).

**Exact next autonomous code task:** SB-39, shared quota-check retry deadline and Retry-After
seconds/HTTP-date parsing; its packet includes scopes, edge cases and fail-first checks.
SB-06/25 adoption and continuation need typed identity/scope evidence and live versioned probes.
SB-15/02/34 remain separate native/Windows/release gates. Source stays 0.5.4, unreleased;
this task neither publishes nor installs. Preserve the lifecycle and release receipts below.

---

# Landed, not released — Switchboard 0.5.4: lifecycle contract (2026-10-04)

**Start here:** [handoffs/2026-10-03-switchboard-0.5.4-lifecycle.md](handoffs/2026-10-03-switchboard-0.5.4-lifecycle.md) (board SB-27), [PR #23](https://github.com/passioncode-ai/fabric-switchboard/pull/23), merged as `f0b8d7f` after the branch was brought up to `main` (0.5.3-beta.2) by a merge (`45bbdbf`). Gate on `45bbdbf`: `./scripts/check.sh` exit 0 (299 Rust tests). Board: SB-27 and SB-23 done; release is SB-34.

No `security -w` on a timer (quiet probe, read on change); 30 s idle cadence with no timer spawns and writes only on change; stable proxy port and token across restarts; one drain for Quit, last window, `SIGTERM` and `SIGINT` with descriptor cleanup; a bounded log in `~/Library/Logs/Fabric Switchboard/`; SB-23; build pruning; [AGENTS.md → Lifecycle](../AGENTS.md#lifecycle).

**Version:** 0.5.4 in every manifest, unreleased. `main` released the 0.5.3 code twice (`v0.5.3-beta.1`, `v0.5.3-beta.2`); 0.5.4 is the next free patch and ships as `v0.5.4-beta.1` through the release workflow ([DISTRIBUTION.md](DISTRIBUTION.md) *Prereleases*) — the release PR renames `## Unreleased` in [CHANGELOG.md](../CHANGELOG.md).

**Exact next task:** SB-34 — release 0.5.4 as `v0.5.4-beta.1` (release PR, tag, dispatch, the operator's approval), install, then the on-Mac checks listed in the handoff. Residency (SB-28), uninstall (SB-29) and `--background` (SB-30, issue #25) stay open.

---

# Released — Switchboard v0.5.3-beta.2: the first release built and signed in CI (2026-10-04)

[v0.5.3-beta.2](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.3-beta.2) was published from `3123f5f` by the release workflow (run 37157469157). The same app as 0.5.3-beta.1 (same team, Keychain trust carries over) with the Windows fixes. The downloaded set is verified, the site serves it, and it is installed here with beta.1 kept for rollback: [release record](evidence/release-0.5.md#release-v053-beta2-2026-10-04).

**Both release gates were approved by an agent on the operator's explicit instruction** ("release it yourself, autonomously", 2026-10-04). The organization's written rule says an agent never approves a release run; keeping or amending that rule is the operator's decision.

**Next:** SB-15 (operator acceptance of 0.5 on this Mac), then SB-25 (continue a session stopped on a limit), and the 0.5.4 lifecycle branch `claude/lifecycle-contract` (another session's) when it is ready.

---

# Windows native fixtures green — prerequisite for `v0.5.3-rc.3` (2026-10-03)

The `v0.5.3-rc.2` rehearsal ([run 37145664175](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37145664175)) failed seven `switchboard-runtime` tests on Windows. Branch `agent/windows-runtime-fixtures` fixes them:
- **Product:** `control::CONNECT_TIMEOUT` is 5 s (was 2 s). Windows refuses a dead loopback port only after about 2 s of SYN retries, so a stale `control.json` returned *did not complete* instead of the lock-guarded fallback.
- **Behaviour unchanged:** Codex's `secret_auth_storage` default no longer reads the build target; a non-macOS context is refused either way.
- **Fixtures:** drive-absolute paths, the heartbeat's directory open, PowerShell quote doubling, the `.session-process` marker, and the native script test comparing resolved paths (TEMP is an 8.3 short path on hosted runners).
- **Clippy 1.99** (`unnecessary_sort_by`) in `backup.rs` and `limits.rs`; it broke the nightly macOS job.
- **`nightly.yml` gains a `windows` job** running the release job's native fixtures without the release environment.

**Verified:** [run 37147791274](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37147791274) on `2370767` — `checks` (macos-14) success, `windows` success. Locally: `./scripts/check.sh` (272 passed); `cargo +1.99.0 clippy --workspace --all-targets -- -D warnings` clean.

Merged as `979c31e` (#28). The `v0.5.3-rc.3` rehearsal ([run 37148716778](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37148716778)) then passed the Windows tests and refused the receipt: *the source tree changed during the build*. Measured in nightly [run 37151375816](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37151375816): `src-tauri/Cargo.toml` was reported modified with no content diff. The `windows-latest` checkout (`core.autocrlf=true`) wrote CRLF, and `tauri build` rewrote the manifest with LF. Fixed on `agent/windows-clean-tree`:
- `.gitattributes` `* text=auto eol=lf`;
- `package_windows.py` names the changed paths;
- the nightly `windows` job runs the unsigned release build and checks the tree is clean.

Verified in [run 37152183420](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37152183420) on `f21acd4`: the tree is clean after the build; `windows` and `checks` both succeed.

**Outcome:** rehearsal `v0.5.3-rc.4` (run 37152992597, publish=false) was approved on the operator's instruction and is fully green: preflight, macOS, Windows, publish. The downloaded set checks out: the GPG `SHA256SUMS.asc` signature is good with the org key `63B3…B6A7`, sums 4/4 OK, attestations 4/4 (`gh attestation verify <file> --owner passioncode-ai --signer-repo passioncode-ai/.github`), the quarantined macOS app is accepted as "Notarized Developer ID", and the Windows receipt says `windows_authenticode: NOT_SIGNED` (Azure identity validation is pending). **Next:** a real `v0.5.x` tag is the operator's decision; Windows signing turns on with `AZURE_SIGNING_ENABLED=true` once the certificate profile exists.

---

# Released — Switchboard v0.5.3-beta.1: sessions never lost beside Claude Swap; a quiet desktop (2026-10-03)

[v0.5.3-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.3-beta.1) was published from `21d005c`.

**Scope:** [PLAN-0.5 §0.5.3](PLAN-0.5.md#053--sessions-are-never-lost-the-app-stays-quiet), REQ-51…67. It covers two audits and a review, all fixed with tests.

**Release:**
- notary `be798b70-…` Accepted;
- site `8a13743`;
- installed in `/Applications`; 0.5.2 is kept for rollback.

**Measured on the operator's Mac** ([record](evidence/release-0.5.md#release-v053-beta1-2026-10-03)):
- 0 `SecurityAgent` entries;
- 1 `security` process in 200 s (0.5.2: 30);
- the bundle is 31 MB (0.5.2: 49 MB).

**Not confirmed:** the one account that reads *Sign in again* did not recover from Claude Swap's files. The SB-21 hypothesis is not confirmed; the operator has to sign in again on that row.

**A parallel run builds 0.5.4 lifecycle work on top of 0.5.3** (Fabric session, branch `claude/lifecycle-contract`). It covers:
- a stable proxy port;
- SIGTERM with `control.json` cleanup;
- a rotated log;
- an idle cadence;
- SB-23.

**Next:**
1. The agent-memory design in Project Observatory (DEC-0250, PB-137; Fabric ADR-0105). Its module M9 here is switching through a new session: the `claude --resume` probe first.
2. Then SB-15.

**Releases are now built and signed only in CI** (branch `feat/release-in-ci`, organization decision of 2026-10-03). The sections below this one still describe signing by hand on the operator's Mac; [DISTRIBUTION.md](DISTRIBUTION.md) is current.
- [`release.yml`](../.github/workflows/release.yml) replaces `build-windows.yml`. A `vX.Y.Z` tag starts `preflight`, then `macos` and `windows` in the protected `release` environment, then the organization's `publish`. `-rc` tags rehearse with `publish=false`.
- macOS: `build_macos.py --external-notarization`, the shared notarize action on the app, then `--finish-external`, which notarizes the CLI as its own ZIP and requires its ticket to list both slices' `CDHash`.
- The team comes only from `SWITCHBOARD_SIGNING_TEAM`. CI passes `vars.APPLE_TEAM_ID`; a build without it is a development build ([KEYCHAIN.md](KEYCHAIN.md)).
- Windows is built natively. Azure Artifact Signing is wired but off (`AZURE_SIGNING_ENABLED=false`). Opening the Azure account is the operator's step ([DISTRIBUTION.md → Human steps](DISTRIBUTION.md#human-steps-azure-artifact-signing-for-windows-operator)).
- Tests: `scripts/test_build_macos.py`, `test_package_windows.py`, `test_release_preflight.py` (in `./scripts/check.sh`); `keychain_macos` tests `a_build_without_the_team_variable_trusts_no_team`, `only_a_well_formed_team_id_becomes_a_requirement`.
- **Next for releases:** a `release-approvers` member (whoever pushed the tag included, since the operator's amendment of 2026-10-03; never an agent) approves the `-rc` rehearsal, and its jobs are read to the end. The macOS smoke test and the Windows `tauri bundle` step have never run on a hosted runner. The 0.5.4 release PR then renames `## Unreleased` in [CHANGELOG.md](../CHANGELOG.md) and keeps `windows_authenticode: NOT_SIGNED` in it.

---

# Released — Switchboard v0.5.2-beta.1 (2026-10-03)

[v0.5.2-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.2-beta.1) is published from `e4c2a0d`.
- **What it adds** ([#21](https://github.com/passioncode-ai/fabric-switchboard/pull/21)): an `invalid_client` answer holds every renewal without blaming an account, and a config the switch creates skips onboarding. Comparison rows #16 and #19–#22 are closed in [PLAN-0.5 §0.5.2](PLAN-0.5.md#052--the-last-rows-of-the-comparison).
- **Release:** notary `d6a85a26-…` Accepted; site `4f68dd7`; installed in `/Applications`, with 0.5.1 kept for rollback.
- **Live check after install:** 0 `SecurityAgent` entries. One account reads *Sign in again* right after Claude Swap stopped (board SB-21; record in [release-0.5.md](evidence/release-0.5.md#release-v052-beta1-2026-10-03)).

**Exact next task:** SB-21, so that Switchboard also takes a newer generation from Claude Swap's files after Swap stops. Before it, the operator recovers the one account (*Sign in again*). After it comes SB-15.

---

# Released — Switchboard v0.5.1-beta.1: Claude Swap parity and review fixes (2026-10-03)

**Start here:** [handoffs/2026-10-03-switchboard-0.5.1.md](handoffs/2026-10-03-switchboard-0.5.1.md).

What changed since 0.5.0:
- Claude Swap's code was read side by side with ours ([report](reports/2026-10-03-claude-swap-comparison/README.md)).
- Every gap that could lose a refresh token or file one account's token under another is closed:
  - a switch keeps the MCP sign-ins;
  - the outgoing generation is stored under Claude Code's locks;
  - the lineage is checked;
  - a grant keeps its successor even when its caller gives up;
  - Claude Swap coexistence;
  - stale-lock takeover;
  - idle renewal of the account in use.
- An adversarial review of the branch found 12 more issues; all are fixed with tests.
- Coverage is 86.10 % of lines; the rest is the OS boundary ([evidence](evidence/release-0.5.md#051)).

**Released** as [v0.5.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.1-beta.1)
(`2f4e429`, notary `fb3aa43b-…` Accepted, site `852cbeb`, installed, 0 `SecurityAgent`).
The workspace knowledge page is updated. The content snapshot waits on Fabric's claude-code 2.1.288
repin, which another run holds. **Exact next task:** confirm `workspace.mjs lag` shows
fabric-switchboard `current`, then SB-15.

---

# Released — Switchboard v0.5.0-beta.1 (2026-10-02)

**Start here:** [handoffs/2026-10-02-switchboard-0.5.md](handoffs/2026-10-02-switchboard-0.5.md). [v0.5.0-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.5.0-beta.1) is published from `e6c5e54`: macOS universal notarized (`0b19c796-…` Accepted) and stapled, Windows x64 unsigned cross-build, `SHA256SUMS-0.5.0.txt`. The site selects it (`a7e5a8b`). `/Applications` holds 0.5.0, 0.4.1 kept for rollback. On top of 0.4.1's vault ([KEYCHAIN.md](KEYCHAIN.md), unchanged): Claude Code's item only through `/usr/bin/security`, renewal of inactive Claude accounts, switching on provider limit errors, automatic encrypted backups, one-click accounts. First live switch on the operator's Mac: new session `ok`, 0 `SecurityAgent`. Record: [release-0.5.md](evidence/release-0.5.md). **Exact next task:** SB-15 — a day of use with automatic switching on, then confirm renewal and a limit switch from the journal.

---

# Released — Switchboard v0.4.1-beta.1, Keychain fix installed (2026-10-01)

**Start here:** [handoffs/2026-10-01-release-0.4.1-beta.1.md](handoffs/2026-10-01-release-0.4.1-beta.1.md). [v0.4.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.4.1-beta.1) is published from `15851e3`: macOS universal signed, notarized (`c93a1bdc-…` Accepted) and stapled, Windows x64 unsigned cross-build, `SHA256SUMS-0.4.1.txt`; the first release under the AGPL. The site selects it (Worker `cacbdcf6-…`). `/Applications` holds 0.4.1; after one launch every Switchboard Keychain item sits under `ai.passioncode.fabric-switchboard.shared` trusting the app and its CLI (SB-12 done). Record: [release-0.4.1.md](evidence/release-0.4.1.md). **Exact next task:** SB-10, the launcher member with `ref: v0.4.1-beta.1`.

---

# Keychain dialogs fixed on main, release pending (2026-10-01)

**Start here:** [handoffs/2026-10-01-keychain-shared-trust.md](handoffs/2026-10-01-keychain-shared-trust.md). macOS kept asking whether Switchboard may use its Keychain items because each item trusted only the executable that wrote it, so the bundled CLI (`switchboard mcp`) and every development build asked per item. Items are now written to `ai.passioncode.fabric-switchboard.shared` with an access list trusting the app and its CLI by signature; ordinary use never shows a dialog; the app moves items saved by earlier versions, asking at most once per item. Decision and evidence: [KEYCHAIN.md](KEYCHAIN.md). Gate green; no release cut. **Exact next task:** release 0.4.1-beta.1 by the human steps in the handoff, then SB-12.

---

# Final check — 2026-10-01

Organization-wide final pass (README, licence wording, versions, links, manifests, AGENTS.md,
gate, private-data and secret scans, open issues) on a fresh `--recurse-submodules` clone of
`1edde0e`. Found and fixed in one change:

- **Issue #5 / SB-11, nightly clock gate.** The `clock` job compared the *start* hour with 23 in
  Warsaw, and GitHub starts scheduled runs late, so `checks` was skipped on every scheduled run
  (still true on 2026-10-01: runs `36795099207`, `36798441752`). Now
  [`scripts/nightly_clock.py`](../scripts/nightly_clock.py) takes the cron line that fired
  (`github.event.schedule`), finds the slot it was planned for and runs when that slot is 23:00
  Warsaw — one full run per date, DST-correct; the duplicate line leaves a `::notice`.
  [`scripts/test_nightly_clock.py`](../scripts/test_nightly_clock.py) (6 tests, in
  `./scripts/check.sh`) was watched failing against the old start-hour rule (3 failures).
  **Not yet proven hosted:** issue #5 stays open until a scheduled run shows `checks` running.
- **Manifests:** the Cargo workspace and every crate now carry `repository` and `homepage`, and
  `package.json` names the repository and homepage.
- **`.gitleaksignore`:** three reviewed false positives (SHA-256 digests in a dated design record,
  a synthetic OAuth test fixture), so `gitleaks detect --no-git --source .` over the tracked tree
  exits 0.

Verified without change: README quick start — the installed `/Applications/Fabric Switchboard.app`
CLI (`switchboard 0.4.0`, `Contents/MacOS/switchboard`) answered `initialize`, `tools/list` (4
read-only tools) and `switchboard_accounts` → `{"accounts":[]}` over stdio with a throwaway
`--data-dir`, and the README's `claude -p … --strict-mcp-config` proof returned the same; versions
match `v0.4.0-beta.1`; every relative link and anchor resolves; licence wording is AGPL with the
PolyForm/MIT history kept. **Open for the organization:** org-index `check_private.py` P2 flags five
third-party authors' addresses inside the copyright notices of `THIRD_PARTY_NOTICES.md`; those
notices are kept verbatim (ADR-0092 point 4), so the exemption belongs in the checker.
**Exact next task:** unchanged — SB-10, the launcher member (below).

---

# Licence — AGPL-3.0 or commercial (2026-09-30)

**Start here for the licence change:** [handoffs/2026-09-30-agpl-standard.md](handoffs/2026-09-30-agpl-standard.md). From 2026-09-30 Switchboard is `AGPL-3.0-only OR LicenseRef-PassionCode-Commercial` (Fabric ADR-0092): [LICENSE](../LICENSE) is the AGPL-3.0 text, [COMMERCIAL-LICENSE.md](../COMMERCIAL-LICENSE.md) the commercial offer, every manifest carries the SPDX expression, and About says so. v0.4.0-beta.1 keeps PolyForm and v0.3.1-beta.1 and earlier keep MIT. No release was cut; the next release is the first AGPL one. The repository now follows the organization's repository standard (org-index `check_format.py` 0 findings). **Exact next task:** unchanged — SB-10, the launcher member (below).

---

# Released — Switchboard v0.4.0-beta.1 (2026-09-30)

**Start here:** [handoffs/2026-09-30-release-0.4.0-beta.1.md](handoffs/2026-09-30-release-0.4.0-beta.1.md). [v0.4.0-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.4.0-beta.1) is published from `bd0cf5d`: macOS universal signed, notarized and stapled (anonymous download → `spctl` `accepted, source=Notarized Developer ID`), Windows x64 unsigned cross-build, `SHA256SUMS-0.4.0.txt`. The published CLI serves `switchboard mcp`; `claude mcp list` → `✔ Connected`. The site selects it; `/Applications` holds 0.4.0. Record: [release-0.4.md](evidence/release-0.4.md). **Exact next task:** SB-10, the launcher member. The sections below are earlier history.

---

# Parked — Switchboard 0.4 (2026-09-29)

**Start here:** [handoffs/2026-09-29-switchboard-0.4.md](handoffs/2026-09-29-switchboard-0.4.md). Agents over MCP (`switchboard mcp`), optional project rules, the `switchboard` plugin with the `switching-accounts` skill, Projects/Agents screens, design system 1.1.0 and 19 of 20 audited fixes are on `main`, gate green. Nothing is released; versions read 0.3.2. The release named here as the next task was done on 2026-09-30 (above). Plan and decisions: [PLAN-0.4](PLAN-0.4.md); board: [evidence/backlog.md](evidence/backlog.md). The sections below are earlier history.

---

# Contributor readiness — 2026-09-30

Documentation only; no behaviour change and the exact next task above is unchanged. [AGENTS.md](../AGENTS.md) now opens with what Fabric Switchboard is and sends a contributor to the organization's [CONTRIBUTING.md](https://github.com/passioncode-ai/.github/blob/main/CONTRIBUTING.md) and this repository's [CONTRIBUTING.md](../CONTRIBUTING.md) (now titled with the full name). [SECURITY.md](../SECURITY.md) names the two private channels: GitHub private vulnerability reporting (enabled) and contact@passioncode.ai. Checks on a fresh worktree: `npm ci` exit 0, `./scripts/check.sh` exit 0, `npm run app:build` exit 0 (unsigned `.app`).

Same day, README gained **Quick start for a new teammate** (install, configure, MCP, develop), measured on this machine: the published macOS ZIP matches `SHA256SUMS-0.3.1.txt` and its CLI runs (`switchboard 0.3.1`), but Gatekeeper rejects the app (`Unnotarized Developer ID`) and 0.3.1 has no `mcp` command. The plugin installs into a temporary HOME (`claude plugin install switchboard@switchboard` → 0.4.0) and connects only with the source-built CLI on `PATH` (`claude mcp list` → `✔ Connected`; with 0.3.1 → `Failed to connect`); `switchboard_accounts` over stdio answered `{"accounts":[]}` against a temporary data dir. Both gaps close with the 0.4.0-beta.1 release, notarized — the exact next task above.

---

# License change — 2026-09-29

Switchboard is now source-available: `PolyForm-Noncommercial-1.0.0 OR LicenseRef-PolyForm-Internal-Use-1.0.0` ([LICENSE](../LICENSE), verbatim PolyForm texts), commercial license on request. Releases up to and including `v0.3.1-beta.1` and commits up to and including `7c36f4a` stay MIT. Every crate uses `license-file.workspace = true` and `publish.workspace = true` (`publish = false`); `package.json` carries the SPDX expression. Contributions go through [CLA.md](../CLA.md), [CONTRIBUTING.md](../CONTRIBUTING.md) and the PR template checkbox. GitHub private vulnerability reporting is enabled.

[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) is generated by `python3 scripts/third_party_notices.py` from `cargo metadata --locked` for the three release targets plus the non-dev npm packages; `./scripts/check.sh` runs it with `--check`. A dependency change regenerates it in the same commit. `build_macos.py`, `build_windows_cross.py` and the Windows workflow copy `LICENSE` and the notices into every archive; the published 0.3.1 archives predate this and do not carry them. The drafts `v0.2.0-beta.1` and `v0.3.0-beta.1` (no tags) were deleted. The English README is primary; the Russian one is [README.ru.md](../README.ru.md).

# Active repair — 0.3.2

Operator: repair installed launch, detect existing Claude login, expose Claude Swap import, update GitHub/site and install locally. [Plan](LAUNCH-REPAIR-0.3.2.md), [evidence](evidence/release-0.3.2.md). Work branch `main` (the only branch; see [Repository layout](#repository-layout)). Current CLI detection fixed via bounded Apple security executable; real current profile captured and six Swap profiles imported without failed rows. Signing/notarization and final artifact receipts are separate remaining gates. Keep existing real provider sessions and auth untouched.

Exact next task: build/verify signed app and CLI from clean source, inspect actual Accounts/import/usage, install in Applications preserving rollback, publish matching GitHub archives and update owning website release manifest. Apple notarization requires operator-local credentials; never export a signing key or disable Gatekeeper.

---

# Fabric Switchboard — start here

**Objective:** a Fabric account workbench for Claude Code/Codex, CLI and macOS/Windows builds. Current extension: capture existing CLI authorization, official console login for additional accounts, Claude Swap import, actual active identity, detailed quota and opt-in rotation.
**Owner:** `passioncode-ai/fabric-switchboard`; 0.3.1 source is tag `v0.3.1-beta.1` (`9e20a49`). The repository is public with default branch `main`; [0.3.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.3.1-beta.1) is published. See [anonymous verification](evidence/publication-0.3.1.json). Observatory and other projects are unchanged.

Read [0.3 account/rotation contract](ACCOUNTS-AND-ROTATION.md) → [research delta](RESEARCH-0.3-IMPORTS.md) → [spec](SPEC.md) → [shared contracts](CONTRACTS.md). Current checks/artifacts: [0.3.1 release evidence](evidence/release-0.3.1.md); [0.3 implementation evidence](evidence/release-0.3.md) is historical. [0.2](evidence/release-0.2.md) and [0.1](evidence/verification.md) reports are historical.

## Completed implementation

- Four original comparator repositories researched at fixed commits; Claude Swap revisited for current capture, backups, live identity and compatible locks.
- Tauri/Rust/TypeScript desktop and native CLI share protected vault, immutable HTTP/SSE request identity, isolated official-login homes and macOS/Windows launch adapters.
- Current Claude/Codex capture, read-only Claude Swap import, provider/pool/account/organization upsert, actual native identity distinct from managed selection. Native context stays vault-only.
- Explicit native Claude activation with lock heartbeat/ownership, identity compare, config preservation and guarded rollback. No process kill or prompt replay.
- Detailed quota/reset windows and generation checks, bounded fair polling/backoff. Ordinary CLI owns OAuth refresh; its newer current generation can be adopted for known profiles.
- Persisted same-provider/pool policies with freshness, enabled/expiry eligibility, threshold/headroom/cooldown and hold reasons. Managed route/cooldown publish atomically. One native Claude policy can be enabled across all pools. Manual choice starts cooldown; disabling policy stops rotation.
- Runtime monitor/cache/CLI/Tauri wiring; [CLI](CLI.md) and [operations](OPERATIONS.md) describe usage and recovery.

Packets and shared API: [PLAN-0.3](PLAN-0.3.md). Branches `codex/v03-core`, `codex/v03-imports`, `codex/v03-ui` integrated by cherry-pick; no unmerged packet code is a prerequisite.

## Decisions and boundaries

Metadata writes schema2 and lazily reads schema1; no destructive downgrade. Secrets never reach renderer/control output, logs or Git. Existing streams keep their identity. Native activation explicitly updates ordinary Claude auth; tests use fixtures. Local identity is a source claim until authenticated provider evidence exists.

Inactive snapshots can expire; recapture/reimport or official login is recovery. No competing refresh grant (superseded in 0.5: inactive Claude accounts are refreshed, the active one never — [PLAN-0.5](PLAN-0.5.md) D-2). Codex file/direct macOS keyring capture is supported; newer secrets/ephemeral stores report unavailable. Other shells' transient overrides cannot be inferred. Native Codex takeover, crash-resumable login IDs, indefinite account pins, quarantine, encrypted cross-device export and automatic updates remain follow-ons.

Signing is separate from notarization. Native Windows and provider acceptance remain separate. Hosted full checks remain nightly only; previous hosted run was billing-blocked before any step.

## 0.3.1 PassionCode design packet

Completed: exact shared PassionCode v1.0.0 tokens and S mark vendored with immutable source commit and SHA-256; fixed dark appearance; gold next-request selection separated from blue current identity; native PNG/ICNS/ICO regenerated from that same S vector; app/workspace packages bumped to 0.3.1. The independent switchboard-core crate retains its existing 0.1.0 version. Credentials and routing behavior are unchanged.

Read [design evidence](evidence/design-0.3.1.md), [source manifest](../brand/passioncode/manifest.json), [SCN-023](ux/scenarios.md#scn-023--recognize-switchboard-across-desktop-surfaces) and [contracts](CONTRACTS.md). Parent packet: [public launch](https://github.com/passioncode-ai/passioncode-ai.github.io/blob/508e91793fcb79d6a59bd2265551dbc76f7a8f97/docs/tasks/2026-09-26-public-launch.md).

Checks: npm ci, full `./scripts/check.sh` (81 passed, 1 opt-in Keychain test ignored), 2 canonical asset hashes, 3 generated native hashes, token-reference and version gate; repeat icon generation is byte-identical after sorting ICNS chunks. Synthetic browser review covered Accounts, Activity and Add account at 1280×720 and 740×560. No native/auth test run for this visual patch. Brand lint is NOT_RUN: this repository has no docs/brand/lint.py; identity labels reviewed manually.

## Exact next task

Public release and anonymous archive/hash checks are complete ([receipt](evidence/publication-0.3.1.json)). Website and organization profile publication are tracked in the [website handoff](https://github.com/passioncode-ai/passioncode-ai.github.io/blob/main/docs/HANDOFF.md). Both binaries use source `9e20a49ad7ef917068c266eff4283180209965dc`; later commits contain documentation only.

Acceptance remains [PA-01](packets/provider-acceptance.md), extended with SCN-018..023. Native Windows acceptance needs an available Windows host. macOS notarization needs the saved notarytool profile; Windows signing needs its separate certificate/service. Do not read real auth merely to test. Keep the beta limits visible until the corresponding acceptance actually passes.

## Reproduce and local-only state

```sh
git clone --branch main git@github.com:passioncode-ai/fabric-switchboard.git
cd fabric-switchboard
npm ci
./scripts/check.sh
cargo build --release --locked -p switchboard-cli
```

Keep dependencies, target/dist/artifacts, research clones, app data, Keychain and all provider homes/credentials outside Git. No Fabric parent submodule pin changed. Pushed source is a handoff; draft assets are engineering artifacts, not evidence of provider/native acceptance. Final push/fresh-checkout receipt belongs in the current release report.

## Repository layout

Consolidated 2026-09-27: `main` is the only branch, locally and on `origin`. Every other branch was an ancestor of it or carried patches already integrated (`git cherry` / `git range-diff` showed equal patches; the one `codex/cli` difference was the 0.2.0 version bump that `main` already contains). Removed: remote `codex/bootstrap`, `codex/passioncode-design-v031`; local `codex/{cli,core,research,ui,v03-core,v03-imports,v03-ui,windows,windows-ui,launch-release-032}` and their sibling worktrees. Releases point at commit SHAs or tags, not branches, so none moved. Build outputs for 0.2.0, 0.3.0 and 0.3.1 were deleted locally after their SHA-256 matched the GitHub release assets; 0.3.2 outputs, receipts and logs remain under ignored `artifacts/`. The Windows SDK cache `artifacts/xwin` is re-downloaded by `scripts/build_windows_cross.py` on the next cross-build.
