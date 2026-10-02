# Switchboard 0.5.1 — Claude Swap parity and review fixes (2026-10-03)

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

**Exact next task:** SB-19, the v0.5.1-beta.1 release: tag, notarized build, GitHub release, site,
local install, then the Fabric workspace snapshot.

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
