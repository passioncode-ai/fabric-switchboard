> **How to read this file.** Newest entry first; only the top entry's *Exact next task* is
> current — the next tasks of older entries are history. Open work lives on the
> [board](evidence/backlog.md).

# SB-81 built — Kimi Code subscription accounts; 0.6.11 waits for approval (2026-10-08)

- **0.6.11** (SB-79 both slices, SB-61, SB-68, SB-69, SB-78 remainder): PR #116, tag `v0.6.11`,
  release run 37714019602 — preflight succeeded; macOS and Windows wait for the operator's
  approval of the `release` environment.
- **OpenRouter key for acceptance:** issued through Project Observatory's door —
  `fabric-switchboard-agents`, $10 daily ceiling, in `vault:fabric-switchboard/prod/OPENROUTER_API_KEY`.
  Saving it into Switchboard needs 0.6.11 installed (0.6.10's owner does not know the operation).
- **SB-81** (branch `feat/sb-81-kimi-accounts`): `Snapshot.kimi_accounts`, one owned
  `KIMI_CODE_HOME` per account (`<data>/kimi/<id>/`), sign-in through the official `kimi login`
  in Terminal, tier and plan windows from `GET /usages` and `/me`, launch, sign in again, remove,
  the ordinary `kimi` shown; Accounts → *Kimi Code*, CLI `switchboard kimi`, MCP
  `switchboard_kimi_accounts`; SCN-047, CONTRACTS → Kimi Code accounts, CLI.md, CHANGELOG
  Unreleased. Gate green (473 tests); `cargo xwin check --tests` clean.

**Exact next task:** once run 37714019602 publishes, verify the downloaded set and record it in
docs/evidence/release-0.5.md; with 0.6.11 installed, save the Observatory-issued key
(`use_secret.py run fabric-switchboard OPENROUTER_API_KEY -- sh -c 'printf %s "$OPENROUTER_API_KEY" |
switchboard agents openrouter set --key-stdin --model moonshotai/kimi-k2'`), check
`switchboard agents openrouter status`, launch Hermes on it. Then SB-80 (Hermes model/provider).

---

# SB-79b built — agents launch on the OpenRouter key (2026-10-08)

Branch `feat/sb-79b-openrouter-launch` (continues the Kimi Code session that started the catalog
recipes; two of its recipes were corrected: OpenClaw dropped — its sessions run in its Gateway
daemon, which never sees a launch's environment — and Hermes switched to `--provider openrouter -m`
because `HERMES_INFERENCE_PROVIDER` loses to its config). Ten agents carry an `openrouter` recipe
in `catalog/agents.json`; `switchboard agents launch <agent> --openrouter [--model]` needs no
owner; the session script reads the key from `agents key --service openrouter` when it starts and
stops before the agent without it (zsh and PowerShell); Hermes, pi, omp and Qwen Code run in their
own home. App: *Agents → OpenRouter key for agents* (add, replace, model, remove, balance) and, in
each agent's setup, *Launch on OpenRouter* — the primary action when a key is saved, except Kimi
Code, which stays on its subscription by default (operator 2026-10-08, recipe `default: false`).
Russian strings, SCN-045 updated, SCN-046, CONTRACTS → Agent keys, CLI.md, AGENT-SUPPORT, CHANGELOG.
Checked: `./scripts/check.sh` green; the browser demo (`?demo=1`) in English and Russian.

**Exact next task:** operator acceptance of SB-79 — save a real key in the app, check the balance
line, launch Hermes and Goose on it (Hermes's empty `HERMES_HOME` setup prompt is unverified);
then SB-80 (Hermes model/provider) and SB-81 (Kimi Code subscription accounts and launch).

---

# SB-79 slice 1 built — the OpenRouter key for agents (2026-10-07)

Branch `feat/sb-79-agent-key`, on top of the XA-02 plan (#111). The key is saved once
(`switchboard agents openrouter set --key-stdin`, value in the OS vault, metadata by service with
the default model), its status asks OpenRouter `GET /key` (pinned base; a refused key stays
saved and is named as refused), the model changes with `… model`, `agents key --service
openrouter` prints the value for a session script only. MCP: `switchboard_openrouter_status`
(read) and `switchboard_openrouter_model` (write) — never the key. Encrypted backups carry it
(counted in `Restored.agent_keys`). A failed save leaves nothing behind; journal `agent_key
saved|removed` with labels in both languages. Docs: CONTRACTS → Agent keys, CLI → The OpenRouter
key for agents, SCN-045, CHANGELOG Unreleased, board SB-79. Gate green.

**Exact next task:** SB-79b — catalog `openrouter` recipes (XA-02 A-3), `switchboard agents
launch --openrouter [--model]`, the Agents → OpenRouter key panel (*Launch on OpenRouter* per
agent); then SB-80 (Hermes model/provider) and SB-81 (Kimi Code accounts). A live
`openrouter status` against the real OpenRouter with a real key is operator acceptance.

---

# 0.6.10 published and verified (2026-10-07)

Published 2026-10-07T01:27:04Z after the operator approved run 37544406792; the downloaded set is
verified and recorded (release-0.5.md → v0.6.10), README / README.ru say 0.6.10,
fabric-workspace roadmap/products updated (#63), fabric-dashboards-54 told (FD-25 can run the
in-place console). The site's download routes still answered 0.6.8 right after publication.
This Mac's installed bundle was 0.6.8; it reaches 0.6.10 through its own update check.

**Exact next task:** check `curl -w %{redirect_url} https://passioncode.ai/switchboard/download/macos`
names v0.6.10, then the board: SB-72 (paid-agent ceilings), SB-73 phase 2 (Kimi Code / Hermes),
SB-71 app screen, SB-66 strings.md resync; the next release carries SB-61, SB-68, SB-69 and the
SB-78 remainder (CHANGELOG Unreleased).

---

# 0.6.10 waited to publish; SB-61, SB-68, SB-69, SB-78 closed on main (2026-10-07)

- **0.6.10** (Russian interface SB-76, LC-16 updates SB-77, audit fixes SB-78; carries the never
  published 0.6.9): run 37544406792 — preflight, Windows, macOS and updater jobs succeeded; the
  `publish` job waits for the operator's approval. The dead 0.6.9 run 37518902023 was cancelled.
- **On main since the tag** (ship in the next release, CHANGELOG Unreleased): SB-61 a `--data-dir`
  store never sees or restores the real backups (#103); SB-68 help on every CLI command and
  argument, `accounts update` with one flag (#104); SB-78 remainder — `~/.claude.json` up to
  32 MiB and half-written files, 5 s drain before the Windows installer, honest password refusal,
  SB-57 test on Windows (#105), sign-in renewal of every saved copy in one transaction (#106);
  SB-69 confirmations, rule dialog, per-screen error titles, SCN-042…044 (#107).
- Checks: `./scripts/check.sh` green on each merge; `cargo xwin check --tests` for Windows. The
  unmasked SB-57 test runs on Windows first in tonight's nightly (23:00 Europe/Warsaw).

**Exact next task:** when the operator approves `publish` of run 37544406792, verify the
downloaded set (SHA256SUMS + GPG, `gh attestation verify`, spctl, stapler, CLI version,
latest.json), record it in docs/evidence/release-0.5.md, move README / README.ru status to
0.6.10, update fabric-workspace roadmap/products, tell fabric-dashboards-54 (FD-25). Then the
board: SB-72 (paid-agent ceilings, unblocked by Observatory 0.18), SB-73 phase 2, SB-71 app
screen, SB-66 strings.md resync.

---

# SB-77 built — updates behave as LC-16 says (2026-10-07)

Branch `feat/lc16-parity`: a ready update starts at an idle moment (window hidden, no managed
request in flight, no sign-in waiting, two looks 5 min apart, only while an update waits); About →
*Check for updates* works with the switch off; held releases (`needs_migration`); `update_download`
events; never-check reasons logged. Docs: DISTRIBUTION → Automatic updates, CONTRACTS → Updates to
LC-16, SCN-037, AGENTS lifecycle, strings.md, CHANGELOG. Gate green.

**Exact next task:** release 0.6.10 (Russian interface, SB-78 fixes, SB-77) — the operator approves
the run; then tell fabric-dashboards-54 (FD-25) the version and record the release.

---

# Audit fixes 2026-10-07 (SB-78) — 0.6.9's Windows job failure found and fixed

The fabric-workspace audit of origin/main 4b4b77b found the 0.6.9 release run 37518902023 failing
on Windows (a continuation test lost `#[cfg(not(windows))]`); v0.6.9 cannot publish from that tag,
so the fixes ship as the next version. Branch `fix/audit-2026-10-07` fixes it and the fallback,
Windows-update, formatted-error, in-place and Russian findings (board SB-78, CONTRACTS → Automatic
fallback, CHANGELOG Unreleased). Checks run: `./scripts/check.sh` green on macOS;
`cargo xwin check --tests --target x86_64-pc-windows-msvc` for the crates and `src-tauri` (a
placeholder `switchboard.exe` resource for the check only) — no errors, no warnings.

**Exact next task:** release the next version with the Russian interface and these fixes (the
0.6.9 run stays failed; the operator approves the new run), then SB-77 (LC-16 parity) and the
open part of SB-78.

---

# SB-76 built — Switchboard in Russian; SB-67 fixed, SB-66 mostly (2026-10-07)

Operator request 2026-10-07: a Russian interface for every PassionCode.ai product (this repository
does Switchboard; the organization's standard is fabric-workspace knowledge/localization.md, track
RM-25), language = the system's plus a switch. Branch `feat/i18n-ru`:

- `src/i18n.ts` (`t`, `plural`, locale choice), `src/locales/ru.ts` (763 strings, independently
  proofread), About → Language (Same as system / English / Русский, reloads the window), the tray
  menu in Russian (`set_language` IPC; system language before the window reports —
  `switchboard-core::language`), backend sentences translated where shown, journal labels for the
  core's event vocabulary, Russian-only column widths so longer words do not run under buttons.
- Gate: `scripts/check-locale.mjs` (every interface string has an entry, placeholders equal, three
  plural forms); `scripts/test-ui-logic.mjs` 26 cases; full `./scripts/check.sh` green.
- SB-67 fixed (turn-on numbers, `no_quota` line, Try again after a failed cancel); SB-66: platform
  words (computer / notification area / Windows key protection), brand terms; a full strings.md
  resync stays open.
- Docs: CHANGELOG Unreleased, CONTRACTS → Interface language, SCN-041, brand voice/terminology/
  strings, README and README.ru, board SB-66/67/76.

**Exact next task:** merge `feat/i18n-ru`; after the operator approves 0.6.9 (run 37518902023) the
next release carries the Russian interface — then check the installed app and its tray in Russian.
Next on the board: LC-16 parity with Fabric Dashboards 0.6.1 (activation only at a safe point; a
manual *Check for updates* that works with the switch off — SB-77), SB-61, SB-68, SB-69, SB-72.

---

# SB-75 built — launch in place for an embedded console (2026-10-06)

Fabric Dashboards (session fabric-dashboards-54) asked for a way to host a Switchboard session in
its embedded console. Built: `switchboard launch (<id> | --provider claude|codex) --mode …
--working-directory <dir> --in-place [-- <agent args>]` — the owner prepares the same session and
returns its script; the CLI `exec`s it in the calling terminal; `--provider` resolves the folder's
account (project selection → rule → default selection). Tests in launch.rs, projects.rs and the CLI
suite; docs CLI.md, CONTRACTS → Launch in place, SCN-016, AGENTS lifecycle. Not observed live.

**Exact next task:** ship 0.6.9 (its release run waits for approval), tell fabric-dashboards the
version, then the live acceptances (in place inside Dashboards; a real limit → Codex takes over).

---

# Release 0.6.8 — fallback chains and the automatic hand-over published (2026-10-06)

v0.6.8 published 2026-10-06T14:45:05Z (run 37478551625; downloaded set verified — release record
in release-0.5.md): SB-71 chains (core, CLI, MCP) and SB-73 phase 1 (automatic Claude Code ↔ Codex
hand-over along the chain). Off until the operator sets a chain; neither is observed live. README,
README.ru, the board, XA-01 and the organization's roadmap/products moved to 0.6.8.

**Exact next task:** the live acceptance — set a chain (`switchboard chain set --preset
subscriptions-first`), let a real Claude Code workflow hit its limit and watch Codex take it over
(log `fallback offered`); then SB-72 once project-observatory-dashboard#175 ships in an engine
release, SB-73 phase 2 (Kimi Code / Hermes recipes), the SB-71 app screen, SB-61, SB-66…SB-69.

---

# SB-73 phase 1 — automatic fallback along the chain; 0.6.7 published (2026-10-06)

- **0.6.7 published** 2026-10-06T14:05:33Z (run 37473394537), downloaded set verified — release
  record in release-0.5.md; README 0.6.7; the brand agent has the version and SB-70 receipts. The
  site's download route follows within its 15-minute cron.
- **SB-73 phase 1** (branch `feat/sb-73-automatic-fallback`, see its PR): when an open workflow's
  executor hits its limit and a chain applies, the monitor hands it to the next usable Claude Code
  or Codex account through `Operation::Continue` (isolated, in the workflow's checkout). Once a
  minute at most, only while a chain is set and something is limited; 15-minute retry window.
  Tests with a stand-in engine and a real limit marker. Not observed live.
- **SB-72** waits on project-observatory-dashboard#175 (OpenRouter door: daily/weekly ceilings, and
  a fix — moving a ceiling turned daily keys monthly), handed to that repository's session.

**Exact next task (superseded by the 0.6.8 entry above):** release 0.6.8 — done; then the
live acceptance (a real limit → Codex takes over); SB-72 once #175 ships in an engine release;
SB-73 phase 2 (Kimi Code / Hermes recipes); the SB-71 app screen.

---

# Release 0.6.7; Inbox and Dashboards updated; SB-71 chains built (2026-10-06)

- **0.6.7** (SB-70, Claude Code ↔ Codex continuation) tagged from `7ce4cf1` (#90); release run
  37473394537 approved on the operator's instruction («давай выпускай»); the record goes into
  release-0.5.md once it publishes.
- **Fabric Inbox 0.11.0 and Fabric Dashboards 0.5.6 installed** on this machine (operator closed
  both apps): downloaded sets verified (SHA256SUMS, GPG, attestation, Gatekeeper, team
  `KJ35UYYL22`), replaced bundles kept in `~/DATA/_archive/passioncode-app-rollback-2026-10-06/`
  (Dashboards 0.4.1; Inbox was already 0.11.0 when installed). Both update themselves from now on.
  **The disk ran out mid-copy** (1.1 GB free, swap 13 GB): the partial Dashboards bundle was
  removed and the install redone by `mv` from one verified extraction; the session's scratch
  downloads were deleted. The broker answered `verification_pending` on the first starts (a changed
  bundle is re-verified; minutes under load). The machine sits at ~98 % disk — see the machine
  CLAUDE.md gotcha on swap.
- **SB-71 chains — core, CLI, MCP** (on branch `feat/sb-71-fallback-chains`, see its PR): operator's
  chains per machine/project/task, preset `subscriptions-first`, catalog and subscription checks,
  pruning; SCN-039; CONTRACTS → Fallback chains; CLI.md; plugin tools reference. A chain is only
  recorded — SB-73 makes it act.

**Exact next task:** verify the 0.6.7 publication and record it; send the brand agent the version
and SB-70 receipts; then SB-72 (ceilings: OpenRouter door daily reset in
project-observatory-dashboard first), SB-73 (automatic fallback), the SB-71 app screen.

---

# SB-70 built — Claude Code ↔ Codex continuation (2026-10-06)

`switchboard continue <wf> --account <id> --dir <checkout>` now hands a workflow to an account of
the **other** provider with the same context (XA-01 step 1, N-018 step 2): `PROVIDER_MISMATCH`
removed; the handoff names the receiving harness (`claude-code` / `codex`); the first prompt says
the work comes from another agent (its name sanitized) and tells the session to accept naming no
other provider, so it inherits the offer's (the engine refuses an acceptance naming another).
Tests: `a_workflow_moves_to_another_provider_with_its_context`,
`a_workflow_of_an_agent_switchboard_does_not_hold_can_be_taken_over`,
`the_prompt_of_a_cross_provider_handoff_names_the_previous_agent_safely`. Docs: CLI.md,
CONTRACTS, SCN-038, CHANGELOG `Unreleased`, packets. **Not yet observed live** (a real limit,
Claude Code → Codex). The brand agent (POST-010) waits for the release version and these receipts.

**Exact next task:** ship it in 0.6.7 (after 0.6.6 publishes), send the brand agent the version
and receipts, run the live acceptance; then SB-71 (chains).

---

# SB-62 fixed; every PassionCode product updates itself (2026-10-06)

**SB-62 (P1) fixed for 0.6.6.** A sign-in of an account already saved — from its row, from
+ Add account, or from Try again in the sign-in banner — now updates it where it is: label and
pool kept, every saved copy refreshed, nothing copied into `default`, nothing renamed to its email
(`file_sign_in`, crates/switchboard-runtime/src/lib.rs; the banner keeps the sign-in's label and
pool, src/main.ts). Four runtime tests (two red against the old filing) and a ui-logic case; the
receipt gains `signed_in_again` (CONTRACTS), the notice reads “{label} is signed in again in
{provider} · {pool}.” (strings.md), SCN-003 and CLI.md updated.

**Automatic updates across the organization (operator request).** Measured on this machine:
Switchboard, Project Observatory (engine and app), Fabric Inbox (since 0.10.1), Fabric Dashboards
(since 0.5.6) and the `passioncode` launcher update themselves; **Fabric** builds it for 0.3.3
(its ADR-0121, plan row P-12, branch `agent/release-032-work` @ `6cdde799`). The rule is
lifecycle **LC-16** and the track **RM-19** in fabric-workspace (#47, `deacd77`), with RM-20 for
the cross-agent plan. Installed here: Inbox 0.8.2 and Dashboards 0.4.1 predate their updaters —
0.11.0 and 0.5.6 are downloaded and verified (SHA256SUMS, GPG, attestation, Gatekeeper) and wait
for the person to quit both apps (the broker may not stop a person's app).

**Exact next task:** release 0.6.6 with SB-62 (release PR, tag, approval); then XA-01 from SB-70.
**Operator:** quit Fabric Inbox and Fabric Dashboards once so the verified updates can be
installed; restart Switchboard (*Restart to update*) and press Switch once.

---

# Cleanup and docs pass — repository in its final state for 0.6.5 (2026-10-06)

**Objective (operator):** clean up after the 0.6.5 work, bring every document and plan in line
with the code, delete branches nobody needs, and put every bug found on the board.

**Done.**
- **Two lost commits recovered.** `agent/agents-support` held two commits made after PR #69 was
  merged and never landed: the proxy's agent-capability check now runs every constant-time
  comparison whatever matched, so timing cannot tell which header or token was right
  (`crates/switchboard-proxy/src/lib.rs`), and AGENT-SUPPORT says unverified loopback agents are
  unverified, not supported. Cherry-picked here (gate green).
- **Branches:** every local branch was checked against its merged pull request (#61, #69,
  #81–#84) and deleted; `origin` holds `main` only; no worktrees, stashes, drafts or unfinished
  runs. Release tags stay (v0.6.3 and v0.6.4 are history the CHANGELOG cites).
- **Local leftovers:** 26 untracked files in the git-ignored `artifacts/` that no document cites
  (0.2–0.5 build and debug logs, old receipts) and the pasted session transcript from the repo
  root moved to `~/DATA/_archive/fabric-switchboard-cleanup-2026-10-06/`; files the evidence
  cites and the `xwin` SDK cache stay. `target/` is 6.9 GB, under the 10 GB cap (LC-15).
- **Docs against code** (three read-only audits: user docs, plans/board/handoff, scenarios and
  brand vs UI): README and README.ru status (they said v0.6.0 and v0.5.3-beta.1) and history,
  INSTALL and CLI (SB-05 since 0.6.0, Windows no longer "beta"), CLI.md (header, a new
  `switchboard agents` section, backup file name, `accounts update`), OPERATIONS (boundaries,
  managed-home token, backups, the full log event list), DISTRIBUTION, KEYCHAIN, AGENTS window
  timers, CONTRIBUTING, PLAN-0.4 (MCP table marked historical, REQ rows), PLAN-0.5 (REQ-20
  superseded by SB-59, REQ-63's removed test), SPEC §2 to 0.6.5, packet states, scenario texts
  that described another UI (cancel, second sign-in, Import, badges, Stop), plugin README and
  tools.md (`project_context` returns `project`). In code: the `switchboard_usage` and
  `switchboard_project_apply` MCP descriptions and the `project list` help said what the code
  does not do.
- **Board:** statuses corrected (SB-09, SB-29, SB-39, SB-40, SB-41, SB-44, SB-46 merged and
  released; SB-49 reopened — 0.33 % measured against < 0.2 %; SB-57 released in 0.6.5). Bugs
  found: **SB-62 reproduces** (Try again in the sign-in banner renames the account to its email
  or duplicates it in `default`, which can expose a project's account to native switching),
  **SB-61 cause located** (`--data-dir` backup list/restore reach the real backups). New rows
  SB-64 (SB-52 desktop entry and MCP tool), SB-65 (SS-01), SB-66 (UI copy vs brand pack),
  SB-67 (small UI defects), SB-68 (empty CLI help), SB-69 (states no scenario covers).

**Decided:** GitHub Actions artifacts (~0.9 GB, retained to January) are **kept** — operator,
2026-10-06: «не удаляй артефакты». The installed app runs 0.6.4 code until restarted (its bundle is 0.6.5;
the broker may not restart a person's app).

**Checks run:** `./scripts/check.sh` exit 0 on this branch (see the PR); `scripts/check_docs.py`.

**Exact next task:** SB-62 (P1) — keep label and pool on the pending sign-in and refuse or
update an identity saved in another pool in `finish_login`, with tests; then the cross-agent
plan [XA-01](packets/cross-agent-continuation.md) in its order (SB-70 cross-provider continue →
SB-71 chains → SB-72 ceilings → SB-73 automatic fallback → SB-74 pipelines), decided by the
operator 2026-10-06 (automatic switch, daily ceiling plus project/task ceilings, chains of their
choosing per machine/project/task, MCP for everything); then SB-61, SB-66.
**Operator:** restart Switchboard (tray → *Restart to update*) and press Switch once in
Accounts; decide on the Actions artifacts; SB-15, SB-02, SB-03, uninstall acceptance.

---

# Release 0.6.5 — the native switch no longer sticks on a misfiled sign-in (2026-10-06)

**What this is.** The night sessions of 2026-10-05/06 left an uncommitted attribution rework
for the bug the operator hit in Accounts (*The Claude Code sign-in does not match the account
named in its settings* on every switch). This run reviewed it, fixed what it found, and
released it as 0.6.5 (SB-59). `v0.6.4` was tagged but never published (its macOS job waited on
the Actions budget); its run 37351269626 was cancelled and 0.6.5 carries its changes.

**The bug.** A stored copy holding the live token under another account (an interrupted switch
or import) outranked the provider's answer, and `learn_live_owner` never asked the provider
once a copy said "foreign" — so the switch was refused for good. Now: the cached
`/api/oauth/profile` answer decides first (`refresh::attribution`), the provider is asked in that
case too, one token held by copies of two different accounts names nobody, and a token the
provider names as another **saved** account's is filed under that account while the switch goes
on (`refresh::owner_to_file`, `replace_native`; rows keep their own identity, `Some(owner)`).

**Found and fixed in review (this run).**
- *Data loss in the rework as left:* with the provider unreachable, a single stored copy was
  enough to move the live token into another account — and that copy may be the misfiled one,
  so the other account's real sign-in in its other pools was overwritten. Rerouting now needs
  the provider's word; otherwise `UNCONFIRMED_LIVE` and nothing is written
  (`a_stored_copy_alone_never_moves_the_live_token_to_another_account`, proven red without the
  guard). Attribution is computed once per switch instead of three times.
- *`usage_check` log event:* it classified failures by copies of the proxy's message strings
  and logged the commonest failure (a non-2xx answer) as `other`. The messages are constants in
  `switchboard-proxy` now and `ProbeFailure::kind()` owns the codes (adds `http_error`);
  `usage_checks_report_the_providers_wait_and_never_its_body` asserts them. OPERATIONS lists it.
- *SB-60:* `switchboard mcp --read-only` ran a provider check for `switchboard_usage` with
  `refresh: true`; it now answers with the stored observation and a note.
- The plugin's tool reference said 300 s for the default freshness limit; the code is 900 s
  (`UNPOLICED_MAX_AGE_SECONDS`).
- Docs moved with the code: ACCOUNTS-AND-ROTATION, CONTRACTS, OPERATIONS, SCN alt paths, CLI.md,
  the plugin's tools.md, CHANGELOG 0.6.5. The audits' other findings, of which only summaries
  survived the night session, are now board rows SB-61 (`--data-dir` backups), SB-62 (banner
  re-sign-in renames/duplicates), SB-63 (docs drift).

**Install note (from the night session).** A plain `npm run app:build` without
`SWITCHBOARD_SIGNING_TEAM` compiles `Build::Development` and reads the `…development` Keychain
namespace, so every quota check fails on a release vault (docs/KEYCHAIN.md decision 2). The
operator's machine runs such a local engineering build of this tree **with** the team
compiled in, reporting 0.6.4; auto-update replaces it with 0.6.5.

**Checks run:** `./scripts/check.sh` exit 0; `release_preflight.py --tag v0.6.5 --publish true
--windows-signing false` ok. **Released:** v0.6.5 published 2026-10-06T03:14:46Z, run
37406556811 all green, downloaded set verified —
[release record](evidence/release-0.5.md#release-v065-2026-10-06--the-native-switch-no-longer-sticks-on-a-misfiled-sign-in).
INSTALL.md's attestation command fixed (#82); org roadmap row updated (fabric-workspace #46).

**Exact next task (superseded by the cleanup entry above):** the update to 0.6.5 was confirmed
(#84); Switch in Accounts still needs the person's restart; then SB-62, SB-61,
SB-07, SB-16, SB-52 desktop entry and the `switchboard_continue` MCP tool. **Operator:** SB-15,
SB-02, SB-03, uninstall acceptance, the live third-party agent run (API-key account) — it also
confirms the Kimi `/login` answer and the ZCode snippet from the agents audit below.

---

# Agents audit — Kimi Code, ZCode, Hermes, OpenClaw gaps closed from sources (2026-10-06)

Asked whether Switchboard is adapted for Kimi Code CLI (against the live
[kimi.com/code/docs](https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html)),
ZCode, Hermes, OpenClaw and the rest. Answer: yes — all four ship in the 30-agent catalog since
0.6.1 (SB-56), at the proxy level with `agents connect`/`launch`, the MCP tools and the Agents
panel; the Kimi entry was re-checked against the live docs today (install script, `kimi` binary,
`-p`, `~/.kimi-code/`, `mcp.json` all still match). Four research gaps are now closed from the
pinned sources, in `catalog/agents.json` + [research](research/agents-2026-10-05.md) +
AGENT-SUPPORT.md (regenerated): **ZCode** gained its `config_snippet` — the personal-rules shape
`{providerRules: [{providerId, config{group standard-personal, access{api-key},
api{anthropic-messages, baseUrl}, personalModelIds}}]}` per `personalProviderConfigRulesSchema`
(rule-data-schema.ts added to sources); **Kimi Code** — `/login` manages only the OAuth managed
account, a config.toml provider is an independent API source (providers.md#L20-L42), so no
`/login` is needed with the snippet, live confirmation pending; **Hermes** — the pinned adapter
never reads `ANTHROPIC_BASE_URL` (config only); **OpenClaw** — `apiKey` accepts an env-var name
as a SecretRef env marker, so the key can stay out of the file. Live acceptance with real agents
still needs an operator API-key account (below). Gate: `agent_catalog` tests, clippy and every
docs/artifacts check green; the full `check.sh` was then red only on
`owner_tests::a_foreign_lineage_under_this_name…`, from the attribution rework in the same
working tree — finished and released in 0.6.5 (above).

**Exact next task:** unchanged from 0.6.2 — release 0.6.3+ follow-ups (SB-07, SB-16, SB-52
desktop entry and MCP tool); the attribution rework owns its red test. **Operator:** the live
third-party agent run (API-key account) can now also confirm the Kimi `/login` answer and the
ZCode snippet on disk; SB-15, SB-02, SB-03, uninstall acceptance as before.

---

# Released — Switchboard v0.6.2, the first automatic update (2026-10-05)

[Release record](evidence/release-0.5.md#release-v062-2026-10-05--the-first-automatic-update).
0.6.2 ships SB-49 (the limit scan reads only what each transcript appended: 66 ms → 1.25 ms per
pass with 22 sessions, #74), SB-52 (`switchboard continue`, #73) and analytics `iid` +
`environment` (#75). **SB-55 accepted live:** the installed 0.6.1 updated itself to 0.6.2 with no
person acting. #72 (commercial licensing → passioncode.ai/business) is merged after 0.6.2 and goes
out with the next release. sshlg-growth was told to re-check its counts; Fabric (0.3.2) and Fabric
Inbox (0.10.2) carry the same props in their next releases.

**SB-57 fixed** (`agent/sb-57-config-section`): every rewrite of `~/.claude.json` counted as a changed sign-in — a fresh read through `/usr/bin/security` on most passes and the `refused` flips; the probe and capture now look at `oauthAccount` only. This is also the likely rest of SB-49's idle CPU (0.44 % → 0.33 % in 0.6.2); confirm both on the next release. A shared HTTP client for quota checks was weighed and declined: one connection carrying several accounts' tokens links them for the provider. **Exact next task:** release 0.6.3 with SB-57 and #72, then an undisturbed idle hour for SB-49. Then (sign-in
source flapping, P3), SB-07, SB-16; SB-58 waits for the Observatory owners; SB-52 next steps are a
desktop entry and the `switchboard_continue` MCP tool. **Operator:** SB-15 day of use incl. a
project with two repositories; SB-02 Windows; SB-03 Azure signing; `switchboard uninstall --yes`
acceptance; a live `switchboard continue` on a second real account after a real limit; a live
third-party agent run needs an API-key account; the Anthropic-terms question on intermediating
Claude.ai sign-ins.

---

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

**SB-52 built** (merged #73): `switchboard continue <wf> --account <id>
--dir <checkout>` hands an Observatory workflow to another account of the same provider and
launches its session ([run record](packets/n-018-continuation.md#run-record-2026-10-05),
CONTRACTS → Workflow continuation, SCN-038). Found an engine issue on the way (SB-58).

**SB-49 fixed** (`agent/sb-49-transcript-cache`): the idle CPU came from re-reading every recent
Claude transcript each 30 s pass (66 ms with 22 sessions); a per-file cache reads only what was
appended (1.25 ms). Confirm < 0.2 % on the next release.

**Exact next task:** analytics props `iid` + `environment` on every event (operator decision via
sshlg-growth 2026-10-05, ANALYTICS.md in the same change), relay to Fabric and Fabric Inbox;
then release 0.6.2 and tell sshlg-growth. Then SB-57, SB-07, SB-16; SB-52 next steps are a
desktop entry and the `switchboard_continue` MCP tool. **Waiting on the operator:** whether Fabric apps add `iid` and
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

# Historical — 0.3.2 repair (superseded; the current state is the top entry)

Operator: repair installed launch, detect existing Claude login, expose Claude Swap import, update GitHub/site and install locally. [Plan](LAUNCH-REPAIR-0.3.2.md), [evidence](evidence/release-0.3.2.md). Work branch `main` (the only branch; see [Repository layout](#repository-layout)). Current CLI detection fixed via bounded Apple security executable; real current profile captured and six Swap profiles imported without failed rows. Signing/notarization and final artifact receipts are separate remaining gates. Keep existing real provider sessions and auth untouched.

Exact next task: build/verify signed app and CLI from clean source, inspect actual Accounts/import/usage, install in Applications preserving rollback, publish matching GitHub archives and update owning website release manifest. Apple notarization requires operator-local credentials; never export a signing key or disable Gatekeeper.

---

# Historical — the 0.3.1 start page (superseded; the current state is the top entry)

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

Consolidated 2026-09-27 (historical: since 0.4 every change lands through a pull request from its own branch): `main` was the only branch, locally and on `origin`. Every other branch was an ancestor of it or carried patches already integrated (`git cherry` / `git range-diff` showed equal patches; the one `codex/cli` difference was the 0.2.0 version bump that `main` already contains). Removed: remote `codex/bootstrap`, `codex/passioncode-design-v031`; local `codex/{cli,core,research,ui,v03-core,v03-imports,v03-ui,windows,windows-ui,launch-release-032}` and their sibling worktrees. Releases point at commit SHAs or tags, not branches, so none moved. Build outputs for 0.2.0, 0.3.0 and 0.3.1 were deleted locally after their SHA-256 matched the GitHub release assets; 0.3.2 outputs, receipts and logs remain under ignored `artifacts/`. The Windows SDK cache `artifacts/xwin` is re-downloaded by `scripts/build_windows_cross.py` on the next cross-build.
