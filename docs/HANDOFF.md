> **How to read this file.** Newest entry first; only the top entry's *Exact next task* is
> current — the next tasks of older entries are history. Open work lives on the
> [board](evidence/backlog.md).

## 2026-10-12 — 0.6.21: second review of SB-107 fixed; Codex configs repaired

Second independent review of `4395263..2743749`: nothing serious; one medium (the Hermes model read
could hold the owner's operation up to 15 s, before the cheap refusals) and seven small findings, all
fixed with tests except the macOS-only cosmetic `deferred` log (backlog SB-107); nine stale doc
sentences corrected (CHANGELOG, SCN-040, CLI.md, ACCOUNTS-AND-ROTATION, AGENTS.md child processes,
CONTRACTS `deferred`, READMEs). Also found while fixing: the window would have named a Hermes session
*Codex CLI* (`providerName` knew two providers). The gate also caught a time-boundary flake in the proxy test
`a_session_never_meets_409_while_the_pool_has_a_usable_account` (a token expiring `now + 1` crossed
a second before `select`: 3 runs in 5); now `now + 2` and a 3.1 s wait, 6 runs in 6.

Outside this repository (operator's machine): Codex would not start — Orca 1.4.210 upserts folder
trust into both `~/.codex/config.toml` and its runtime home with a matcher that knows only unquoted
`[projects."…"]`; the files used the quoted form, so every trust appended a duplicate (both files were
broken by the evening). Both rewritten to the unquoted form, duplicates merged, each verified equal
block by block; backups `config.toml.bak-dupe-*`. My first repair of the runtime home alone made it
worse (Orca re-copied the quoted tables: 87 duplicates). Recorded in Observatory
(`mem:d624c6ece9e54019`). The bug is Orca's and should go upstream.

**Published:** v0.6.21, release run 38299497744, 2026-10-12T20:25:13Z, 25 files, first attempt
(guard nightly, all five builds and signing green); `latest.json` → 0.6.21.

**Exact next task:** when the installed app runs 0.6.20+
(it installed 0.6.20 at 18:37 UTC and waits for an idle restart), rehearse the Kimi step live
(`switchboard continue <wf> --account <kimi id>`); then SB-108 with Fabric Dashboards.

---

## 2026-10-12 — 0.6.20: Hermes keeps the person's model; 0.6.19's guard refusal fixed

0.6.19 (tag `v0.6.19` on `d50b46d`, release run 38272410963) was **refused by the release guard
and never published**: nightly 38272673111 failed on Linux in
`a_continuation_runs_its_first_turn_then_resumes_the_session_interactively` — the review fix put
`find_program("kimi")` into the Kimi preflight, and the CI runners have no Kimi (this Mac does).
Fixed with `continuation_preflight_with(…, program)`; 0.6.20 carries 0.6.19.

Live rehearsal before that (dev build on a temporary data folder, real Observatory and OpenRouter):
`wf_932609d34cf14593`, Claude executor → `switchboard continue --account hermes` → offered
(`handoff:9c1cc21b63c64cc2`) → Hermes accepted (executor `hermes`), wrote `e2e-accepted` and closed
the workflow in about a minute; $0.07 spent. Temporary data, its vault item and the rehearsal
checkout were removed. Operator feedback on it (2026-10-12): the model was not theirs (kimi-k2,
the default I chose when saving the key — their Hermes runs `deepseek/deepseek-v4-pro-0813` on
OpenRouter); the session had to guess `owner` and a UUID `sessionId`; hand-overs should open inside
Fabric, not in new Terminal windows. Done here: Hermes's own OpenRouter model wins
(`hermes::openrouter_model`, `modelFrom`), the prompt names `owner agent:hermes` and a UUID session
id, the key's default model set to `deepseek/deepseek-v4-pro-0813`; Fabric-embedded hand-overs filed
as SB-108. RM-28 packets CX-SB-01…03 answered and filed (SB-109…SB-111). The operator's machine
chain is `claude-code → kimi-code → hermes`. Gate `./scripts/check.sh` exit 0 (539 tests).

**Published:** v0.6.20, release run 38275250263, 2026-10-12T17:22:51Z, 25 files, `latest.json` →
0.6.20 for all seven platforms. It took three attempts, none a code change to the release: (1)
Linux arm64 failed once in `lifecycle_tests::a_restarted_owner_keeps_the_address_and_token_a_session_holds`
(a forked child holding the store lock — fixed for later releases in #172) and Windows x64 failed
in *Sign the desktop executable and the CLI* (`Invoke-ArtifactSigning` not recognized); (2) the same
signing failure again — the Actions cache entry `ArtifactSigning-0.1.8` saved for `refs/tags/v0.6.20`
earlier in this run (66095 bytes; older entries 66120–66128) restored a module PowerShell could not
load; (3) after `gh cache delete 8843982625` the module was installed afresh and every job passed.
If signing fails that way again, delete that tag's `ArtifactSigning-*` cache entry and re-run.

**Exact next task:** once the installed app
runs it (it may need one *Restart to update* — the running copy predates SB-105), rehearse the Kimi
step with a saved Kimi account (`switchboard continue <wf> --account <kimi id>`) and confirm a
near-limit account's `next_check_at` (50 s / 20 s after its check); then SB-108 with Fabric
Dashboards.

---

## 2026-10-12 — SB-107: Hermes in chains; review of everything since 0.6.18; release 0.6.19

Branch `feat/sb-107-hermes-and-review-fixes`, from the operator's request to check every account
connection and the switching pipeline (Claude → Kimi, Claude → Hermes, Claude → Kimi → Hermes) and
release it. Live state on the operator's Mac (read-only, no identifiers): 21 subscription accounts
(20 Claude, 1 Codex) enabled, `ok`, checked within 3 min; 2 Kimi accounts signed in; native
rotation on (80 %, 10 %, cooldown 1800 s); **no fallback chain set** (`chains: []`), so no
automatic hand-over could happen; no OpenRouter agent key saved.

Done: a `hermes` chain step and `switchboard continue --account hermes` (CONTRACTS → Automatic
fallback; SCN-040; CLI.md); the Observatory-issued key `fabric-switchboard-agents` ($10, daily
reset) saved as Switchboard's OpenRouter agent key with model `moonshotai/kimi-k2` (credit read
back: $10 left). Measured with Hermes 0.21.4: an empty `HERMES_HOME` starts without a wizard; with
Switchboard's `config.yaml` the session lists `observatory_handoff_accept`. An independent review
of v0.6.18..main found five defects, all fixed with tests: Kimi checks after the offer (a chain
stalled on a broken `mcp.json`), `kimi -c` after a failed `-p`, the 30 s step running every 60 s
(`NEAR_LIMIT_SLACK_SECONDS`), credential reads when choosing the next account (`next_to_check`), a
Windows updater race (`deferred`). Not fixed, recorded: a Hermes executor's limit is not detected;
Kimi's idle-token blindness (SB-100). Gate `./scripts/check.sh` exit 0 (538 Rust tests, lean
flags); the Windows and Linux checks run in the release guard's nightly on the tag.

**Exact next task:** after the 0.6.19 release is published and the installed app runs it, set the
operator's machine chain `claude-code → kimi-code → hermes` (`switchboard chain set`), then run a
live hand-over rehearsal on a test Observatory workflow (`switchboard continue <wf> --account
hermes`, then `--account <kimi id>`) and record it.

---

## 2026-10-12 — SB-106: fresher figures near a limit, “Hidden personal limit”

Branch `feat/sb-106-near-limit-checks`, from the operator's request (continued from a Kimi session
whose plan was approved): accounts at 95–98 % checked more often, the next account every minute.
Done: `near_limit_check` (the account a session works on, in a pool that can switch: 60 s from
95 %, 30 s from 98 %); `Store::pull_in_next_accounts` (the pool's failover candidate due 60 s after
its last check while the current is ≥ 95 % or held; monitor, at most once a minute); an account
refused 429 within the hour keeps the ordinary cadence (`UsageGate::refused_within`,
`Store::set_paced`); “Hidden personal limit” wording (ui-logic, ru, brand strings, SCN-052); a test
of the 2026-10-09 session-limit marker shape. CONTRACTS → Near-limit quota checks;
ACCOUNTS-AND-ROTATION, SPEC, AGENTS.md monitor row, CHANGELOG, backlog SB-106.

Measured on the operator's Mac (why it "did not switch"): 20 Claude accounts in one native pool
(threshold 80 %, hysteresis 10 %), 16 at 100 %, 3 at 99 %, 1 at 90 % — nothing under 70 % to
switch to; the hidden five-hour limit was already detected (`hidden: true`); usage checks refused
1–33 a day. A native Claude Code turn that already stopped on a limit does not resume by itself
(CONTRACTS, SB-98 "Not changed") — managed mode avoids the pause.

Also fixed (outside this repository): Codex refused to start — Orca 1.4.210's managed home
`~/Library/Application Support/orca/codex-runtime-home/home/config.toml` held a duplicate
`[projects."…/project-observatory"]`. Cause: Orca's preflight matches only unquoted
`[projects.…]`/`[hooks.state.…]` headers, the file had the equivalent quoted form, so Orca appended
a second table on every start. The headers and keys were rewritten unquoted (parsed content
unchanged, backups `config.toml.bak-dupe-*`). Not Switchboard's file.

Gate `./scripts/check.sh` exit 0 (536 Rust tests; lean `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`, ~5 GB free). Not run: the Windows cross-check (before a release).

**Exact next task:** merge the SB-106 PR; then release
0.6.19 (SB-105 + SB-99 + SB-106) and, on the installed copy, watch a near-limit account's
`next_check_at` move to 60/30 s. Still open: the live Kimi run (SB-99), the operator's one restart
by hand, the Russian copy read.

---

## 2026-10-12 — SB-99 done: Kimi Code in fallback chains

Branch `feat/sb-99-kimi-chains`, on the operator's two answers (background usage checks: yes,
every 10 min; a Kimi step: `kimi -p`, then resume). While a chain is set, `kimi::watch` asks each
saved Kimi account's `/usages` every 10 min, at most 4 a pass (`KimiWatch`, `limited_until` = the
window's rule); a workflow whose Kimi executor names a saved account found limited is handed on
(`fallback::kimi_executor_limited`); a `kimi-code` step runs on the pinned or least-used usable Kimi
account (`resolve_with`, `KimiChoice`) through `Operation::Continue`'s Kimi branch: plan with the
harness name `kimi` (`plan_named`), preflight, offer, then Terminal runs `kimi -p <prompt>` and
`exec kimi -c` (`launch::script_then`), Observatory added to the home's `mcp.json`. Core: a Kimi
account may be pinned by `kimi-code` only; removing it drops the step. Docs: CONTRACTS, CLI,
SCN-039/040, AGENTS.md (monitor row, idle budget), CHANGELOG; two errors added to the window with
Russian text. Gate `./scripts/check.sh` exit 0 (532 tests, lean flags; ~6.5 GB free).

**Exact next task:** a live run of the Kimi path — a test workflow in Observatory whose executor
is limited, a chain `claude-code → kimi-code`, a signed-in saved Kimi account — to confirm the
`-p` turn accepts the handoff and `kimi -c` resumes that session (read from `kimi --help`, not
measured); then release 0.6.19 (SB-105 + SB-99). Still open: the operator's one restart by hand
(the running app predates SB-105) and the Russian copy read.

---

## 2026-10-12 — SB-99 second part: Kimi sign-ins capped and swept

Branch `feat/sb-99-kimi-sign-ins`: at most four waiting Kimi sign-ins; an owner start removes the
Kimi sign-in homes whose `kimi login` ended (log `kimi_login_leftovers`). CONTRACTS → Kimi Code
accounts. Gate `./scripts/check.sh` exit 0 (lean: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`;
the data volume has ~3 GB free).

Measured for the rest of SB-99 (2026-10-12, `kimi` 2.1.1): `kimi -p <prompt>` without `-y`/`--auto`
ran a shell command with no prompt (`echo`, stream-json: tool call then result) and ends with a
resume hint `kimi -r <session>`; the interactive `kimi` takes no first message. A Kimi account
home has its own `mcp.json` (`mcpServers`). The engine's `handoff --to-provider` is free text.
Kimi limits are computed only in the window today (`kimiLimitUntil`), never in the background.

**Exact next task:** the operator answers the two SB-99 questions — (a) may the owner check each
saved Kimi account's usage in the background (every 10 min, only while a fallback chain is set),
so a workflow a Kimi executor runs is handed on at its limit; (b) Kimi as a chain step: a first
turn through `kimi -p` (no approvals) then `kimi -r` interactive, or skip Kimi as a target. Then
build them; the app restart by hand and the Russian copy read are still open.

---

## 2026-10-12 — SB-105 done: a waiting update never waits forever

Branch `fix/sb-105-update-waits`. While an update waits, the 6-hour checks go on and a newer
release replaces it; the idle watch logs `update_waiting {reason}` at once and then hourly; a
restart waits out a bundle swap (`Updates::swap`); a pre-SB-65 sign-in leftover is swept (test).
CONTRACTS → A waiting update never waits forever; DISTRIBUTION, OPERATIONS, AGENTS.md,
SCN (automatic updates), CHANGELOG Unreleased. Gate `./scripts/check.sh` exit 0, run with
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`: the data volume had 4.7 GB free, and the first
full run filled it (a storage test failed on a write); `cargo clean --profile dev` freed 5.1 GB
of this checkout's own cache. Not run: the Windows cross-check (retro #8, before a release).

**Exact next task:** the operator restarts the app once by hand (*Restart to update*, or Quit and
open) — the running copy predates SB-105; the version must read 0.6.18 and, on a 0.6.19+ build,
`update_waiting` must appear in the log while an update waits with the window open. Then SB-99's
rest (Kimi Code in fallback chains with SB-73 phase 2). The Russian copy pass still awaits a
native speaker's read of the window.

---

## 2026-10-12 — SB-105 filed: the installed app never left 0.6.16

Status check on the operator's Mac: `main` = `a5c1948`, v0.6.18 published, no open PRs. The
installed app runs since 2026-10-10 with a ready 0.6.16 update and has not restarted into it in
35 h; checks stop while an update is ready, so 0.6.18 is unseen. The broker refused a restart
(`in_use_by_person`, `not_started_by_broker`). Filed as SB-105 with the evidence; no code changed.

**Exact next task:** the operator presses *Restart to update* (or Quit and open) and the version
reads 0.6.18; then SB-105 (idle-watch reason log, keep checking while ready), then SB-99's rest. The Russian
copy pass below still awaits a native speaker's read of the window.

---

## 2026-10-12 — Russian copy pass

Branch `agent/ru-copy-pass`, on the operator's request to review all Russian copy («весь русский
… в целом весь надо пересмотреть — выглядит очень плохо»). 373 of the 963 entries in
`src/locales/ru.ts` rewritten (settings and setup first, then the account list, quota, automatic
switching, projects and rules, launch, Sessions, the journal and backend errors); English keys,
placeholders, commands and product names unchanged. The rules applied and the terms still missing
from the glossary are in [strings.md](brand/strings.md) ("Russian copy pass, 2026-10-12"). The
provider-terms sentence now reads «условия использования» for "terms", with the same three claims,
and README.ru and SCN-049 quote it. Exact-string tests in `scripts/test-ui-logic.mjs` follow.
Checks: every step of `./scripts/check.sh` except `cargo test` and `cargo clippy` passed locally
(locale, ui-logic, error vocabulary, build, docs, plugin, Python suites, `cargo fmt --check`); the
two cargo build steps were not run — the machine's disk was full (No space left on device) — and
the change touches no Rust. The nightly on `main` is the first full run.

Not reviewed in this pass: README.ru.md and docs/PRODUCT.ru.md (mixed English, a github-readme
surface), the tray labels in `src-tauri/src/residency.rs` (already glossary-correct). No release:
copy changes ship with the next release.

**Exact next task:** a native Russian speaker reads the window in Russian (About, + Add account,
automatic switching, an error) and reports strings that still read wrong; then the README.ru pass.
The board's open rows (idle budget with a managed policy, P3; SB-99's rest) are unchanged.

---

## 2026-10-12 — 0.6.18 published

Tag `v0.6.18` on `47ef931` (not the version-bump merge `e815d69`: the nightly there failed on
Windows, fixed by #160, then a Linux arm64 storage-test race, fixed by #161). Nightly run
38192215005 green on `47ef931`; release run
[38193536833](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38193536833).
README / README.ru name 0.6.18; fabric-workspace `knowledge/products.md` and `roadmap.md` and
the projects wiki page updated in the same run.

**Exact next task:** the board's open rows — the idle budget with a managed policy (P3) and
SB-99's rest (Kimi Code in fallback chains with SB-73 phase 2).

---

## 2026-10-12 — Release 0.6.18

Branch `agent/release-0618`: versions 0.6.18 everywhere (`release_preflight.py --tag v0.6.18
--publish true --windows-signing true` ok); CHANGELOG's 0.6.18 section carries SB-102…104, #156
and 0.6.17's notes (v0.6.17 was tagged and never published: the release guard refused it on the
nightly that #156 fixed). Release path: merge, the nightly passes on the merge commit (the guard
checks it), annotated tag `v0.6.18` on it, release run watched to the published release.

**Exact next task:** after publication — README / README.ru release section (version, run id,
what is new since 0.6.16), fabric-workspace `knowledge/products.md` Switchboard row, the projects
wiki page; the local install updates itself.

---

## 2026-10-12 — SB-104: the window keeps its word; the bug review is closed

Wave 3, third part (branch `fix/sb-104-window-docs`): the window defects and the documentation
drift from the review (see the board row SB-104). With SB-102 and SB-103 the review's P1 and P2
findings are fixed; left on the board: the idle budget with a managed policy (P3).

**Exact next task:** release 0.6.18 (SB-102…104 and #156). v0.6.17 was tagged but never
published — the guard refused it on a failed nightly — so 0.6.18 carries 0.6.17's notes too.

---

## 2026-10-12 — SB-103: sign-ins, sessions and Kimi without loose ends

Wave 3, second part (branch `fix/sb-103-logins-sessions-kimi`): a saved sign-in is never taken back
after a restart, young sign-in folders survive the start-up sweep, sessions are read off the async
workers with Kimi labels, Kimi status and client failures stay contained, `onboard --status` on
Windows. CONTRACTS → Sign-ins, sessions and Kimi without loose ends. Before it, PR #156 fixed the
nightly on Linux and Windows that made the release guard refuse v0.6.17 (nothing was published).

**Exact next task:** SB-104 (branch `fix/sb-104-window-docs`, work in progress pushed) — window
defects and documentation drift, then release 0.6.18 with SB-102…104 (the nightly must pass on the
release commit first: the guard checks it).

---

## 2026-10-12 — Bug review in waves; SB-102: failover without side effects

Wave 1: three independent readers (switching core; changes since v0.6.16; window and docs)
found ≈ 40 issues. Wave 2: verified the P1s by reading the code and chose the design (CONTRACTS →
Failover without side effects). Wave 3, first part (branch `fix/sb-102-switching-core`):
kind-compatible compare-and-move failover, one held set, candidate-only set-asides, fresher hidden
limits, project-aware fallback. Next parts: SB-103 (sign-ins, sessions, Kimi, onboarding on
Windows), SB-104 (window defects and documentation drift), then release 0.6.18.

**Exact next task:** SB-103 — see the board row for the list.

---

## 2026-10-12 — SB-101 done: a renewed sign-in is never only in memory

Branch `fix/sb-101-keep-renewed`: the idle renewal files the successor into the vault when Claude
Code's item cannot be written (the kept copy still drives the next write); no overall proxy
deadline, by decision. SB-101 closed.

**Exact next task:** SB-99's rest — Kimi accounts in fallback chains (`provider_of`, chain pins, a
fallback step launching `kimi` with `KIMI_CODE_HOME`) with SB-73 phase 2; pending Kimi sign-ins
capped and swept at start.

---

## 2026-10-12 — SB-100: Kimi status quality

Branch `fix/sb-100-kimi-status`: Kimi's 404 / 429 / unreadable / empty answers each read as what
they are; one client and parallel asks; the home's region; “checked … ago”, background refresh
every 5 min, a failed refresh shown with the last figures; ordinary-`kimi` wording in the window
and the CLI. CONTRACTS → Kimi Code accounts.

**Exact next task:** SB-101's remaining part — persist a renewed Claude sign-in before it is used
(refresh.rs stash; touches the live Claude Code lineage: design first, test with fixtures only),
then SB-99's rest (Kimi in fallback chains).

---

## 2026-10-11 — SB-101 first part: switch reliability

Branch `fix/sb-101-switch-reliability`: lock waits off the async workers; a switch carries only
the sign-in's keys into a `~/.claude.json` Claude Code wrote meanwhile, and a rollback restores
only them; a managed session's unusable selected route fails over instead of 409. CONTRACTS →
Switch reliability. Also today, outside this repo: LC-17…LC-22 merged in fabric-workspace
`knowledge/lifecycle.md` (#136); the contract took CT-23/CT-24, the adapter FAA-22/FAA-23; the
MCP gateway got `health.py`, a watchdog and `docs/AGENTS.md` (`sshlg/agentgateway-setup`,
branch `feat/health-watchdog`).

**Exact next task:** the rest of SB-101 (persist a renewed sign-in before it is used; a proxy
deadline), then SB-99's rest (Kimi in fallback chains, pending Kimi sign-ins swept at start).

---

## 2026-10-11 — SB-99 first part: Kimi safe homes and visible limits

Branch `feat/sb-99-kimi-limits`. Kimi *Launch*, *Sign in again* and *Remove* respect a running
session in the account's folder (`Reservation` on `kimi/sessions/<id>/`, `KIMI_SESSION_RUNNING`);
launches write the session record, so Kimi sessions are listed; the row shows a 100 % window as a
limit until its reset; a passed reset no longer shows the old share. CONTRACTS → Kimi Code
accounts, SCN-047.

**Exact next task:** SB-101 — switch reliability (lock waits off the async workers, `~/.claude.json`
re-read before writing, renewed sign-in persisted first, expired managed route fails over); then
the rest of SB-99 (Kimi in fallback chains, pending sign-in sweep).

---

## 2026-10-11 — SB-98: agents keep working through a limit

Branch `fix/sb-98-no-stalls`. Measured first (3 days of this machine's Claude Code transcripts):
264 sessions hit a limit, median stall 77 min. Fixed: a limited account moves on last known usage;
managed failover moves a refused session request once to the pool's next account (operator
decision; AGENTS.md rule amended); refusals are held at once; a failed switch tries the next
candidate; hidden limits (spend/credit caps) carry their reason and show on the account. Tests
and decisions: CONTRACTS → Agents keep working through a limit; SCN-052. Filed from the same
audits: SB-99 (Kimi limits and safe homes), SB-100 (Kimi status quality), SB-101 (switch
reliability).

**Exact next task:** SB-99 — Kimi limits and safe homes; then SB-101.

---

## 2026-10-11 — SB-65: sign-ins survive a restart; SB-87 merged with it

`begin_login` writes `.login.json` (provider, label, pool, time); an owner start takes back the
sign-ins Terminal still runs or that completed meanwhile (`adopt_logins`, at most four), the
SB-87 sweep removes the rest that ended (with their Claude Keychain item only when deleting it
shows no dialog) and keeps a completed one with a valid journal, and the window restores its
banner through `Operation::PendingLogins` / Tauri `pending_logins` (`restorePendingLogin`).

Checks (the machine's disk was at 1–3.7 GB free, so `./scripts/check.sh` ran in parts):
`cargo fmt --check` ok; `cargo test -p switchboard-runtime -p switchboard-cli` 319 + 22 passed,
0 failed; `cargo clippy --all-targets -D warnings` clean for switchboard-runtime, switchboard-cli
and fabric-switchboard (src-tauri); `npm run build`, `check-brand`, `test-read-deadline`,
`test-ui-logic`, `check-locale`, `check-error-vocabulary` ok. **Not run:** `cargo test -p
fabric-switchboard` (src-tauri's own unit tests; its change is two commands, compiled by clippy)
— the nightly run at 23:00 covers it; check that run.

**Exact next task:** read tonight's nightly result for this merge; then SB-65's remaining part
(acting on a session: open folder, drain, close — never by process name) or SB-07.

---

## 2026-10-11 — SB-87: forgotten sign-ins cleaned at start

Branch `fix/sb-87-login-leftovers`. A native owner's start sweeps `logins/` once, off the start
path: a sign-in home whose Terminal session ended is removed with its Claude Keychain item when
deleting the item shows no dialog; one still open, or whose item would ask, is kept
(`launch::sweep_logins`, log `login_leftovers`). OPERATIONS → recovery now says what stays manual.

**Not merged yet:** `launch::` tests pass (45/45), but the full gate was stopped by the disk guard — the machine had 1.1 GB free (other sessions filling it). Run `./scripts/check.sh` on this branch when there is room, then merge.

**Exact next task:** the rest of SB-65 — a persistent pending-login journal so a sign-in open across
a restart can still be finished (packet SS-01); or SB-07 (the intermittent `project_rules…` /
`managed_session…` MCP test failures) if CI shows it again.

---

## 2026-10-11 — SB-65 first slice: the Sessions view

Branch `feat/sb-65-sessions-view`. Activity → *Sessions* lists the sessions Switchboard launched
that start or run (SB-95's list through the Tauri command `sessions`), refreshed with the metadata;
SCN-051; checked in a WebKit demo in English and Russian.

**Exact next task:** SB-87 — clean a forgotten sign-in's staging home at owner start when no session
holds it (`session_running`), keep it when one does; then the rest of SB-65 (pending-login journal).

---

## 2026-10-11 — SB-95: the session list for other products

Branch `feat/sb-95-sessions`. Each Claude Code / Codex launch writes `<home>/.session.json`
(account, provider, pool, mode, folder, in place, launch time, continued workflow; no
credential). `switchboard sessions [--all]` and MCP `switchboard_sessions` list them with their
state from the markers that keep one session per home (`session_running`, factored out of
`ensure_idle` — reads only). CONTRACTS → Session list. Fabric Dashboards asked for it (agents-first
brief, item 3).

**Exact next task:** SB-65 — the Sessions view in the window over `Operation::Sessions` (scenario
first), then `agents launch` and Kimi sessions in the same list; or SB-92 (shared Windows signing
action) if a release is near.

---

## 2026-10-11 — SB-93 slice 2: setup in the window

Branch `feat/sb-93-window`. Agents → *Connect an agent* opens with *Set up with your coding agent*:
the prompt “Set up Fabric Switchboard on this computer: run `switchboard onboard` and follow it.”
with *Copy*, a line per agent (Claude Code, Codex CLI) saying whether its config registers the
switchboard server, and whether an agent has called it (`mcp-proof.json`). The tour's step 5
outlines that *Copy*. Backend: `agents::setup()` adds `connected` and `agent_prompt`; Tauri
`agent_setup` adds `proved`. Checked in a WebKit demo (Playwright 1.63, `?demo=1`). SB-93 done.

**Exact next task:** SB-95 with SB-65 — a live session list over CLI and MCP, then the Sessions view;
design it from packets/session-supervisor.md first.

---

## 2026-10-11 — SB-93 slice 1: `switchboard onboard` for coding agents

Branch `feat/sb-93-onboard`. The operator's agents-first strategy (2026-10-10): the coding agent
does every setup step it can. `switchboard onboard` prints [AGENT-ONBOARDING.md](AGENT-ONBOARDING.md);
`switchboard onboard --status [--json]` and the MCP tool `switchboard_onboarding` read five steps —
owner, accounts, project (optional), mcp, proving_call — each done / todo (command) /
needs_person (what to ask) / optional, and create nothing (`onboarding.rs`, SCN-050, CONTRACTS →
Agent onboarding). `switchboard mcp` writes `mcp-proof.json` after its first answered call. Read
live on this machine: the switchboard MCP server is **not** registered with Claude Code here
(`claude mcp get switchboard` agrees) — registering it is the operator's call (machine MCP
change), not done.

**Exact next task:** SB-93 slice 2 — the first-run tour (SCN-035) shows these steps (a Tauri command
over `onboarding::steps`), scenario first; then SB-95 with SB-65 (live session list).

---

## 2026-10-11 — SB-94: a caller's context through `launch --in-place`; continuation launches fixed

Branch `fix/sb-94-in-place-context`. Claude Code's `--mcp-config <configs...>` is variadic; measured
on 2.1.296 with the API pointed at a closed port (nothing spent): a plain value after
`--mcp-config <path>` is read as another config (`MCP config file not found: …/<prompt>`), so every
continuation launch with Switchboard's tools stopped before its first turn. Switchboard now passes
`--mcp-config=<path>` as one argument (launch.rs); joined or not, repeated flags add up (both read).
Fabric Dashboards switched to the same form (fabric-dashboards 47e5aa6). New test
`a_dashboards_console_launch_reaches_the_agent_as_sent` runs the generated script with a recording
agent: Dashboards' exact Claude and Codex argv arrive unchanged after Switchboard's,
`FABRIC_DASHBOARDS_CONTEXT`/`_TASK` survive the scrub. The continuation and managed-launch tests
now assert the joined form. Three sign-in-ending tests raced under load (pid file present but
empty; a signal before `trap`): `terminal_session` now waits for a `.ready` mark — `launch::` 43/43,
three runs in a row. CONTRACTS → Launch in place, CHANGELOG *Unreleased*.

**Exact next task:** SB-93 — `switchboard onboard`, scenario first (the first-run tour changes);
then SB-95 with SB-65 (live session list).

---

## 2026-10-10 — v0.6.16 published: Windows signed

[Release run 38068045608](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38068045608) published **v0.6.16** (PR #137): Windows installers, app
and CLI Authenticode-signed on x64 and arm64 — every status `Valid`, signer `CN=Siarhei Sheleh, …,
C=PL` (no street or postal code), timestamped by Microsoft's public TSA; read back from the published
receipts on an anonymous download, GPG and checksums good. The first attempt failed at `azure/login`
(AADSTS700213, GitHub's immutable OIDC subject); project-observatory's `setup-windows-signing.py`
registered both subject forms and the failed jobs were re-run. SB-03 done; SB-92 (adopt the shared
signing action) filed.

**Exact next task:** SB-92 — move the signing steps to `passioncode-ai/.github/actions/windows-signing@v1`
and rehearse on an `-rc` tag; then SB-91 (download page).

---

## 2026-10-10 — v0.6.15 published (Linux, Windows on ARM)

[Release run 38014753632](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38014753632) published **v0.6.15** from `8448624c` (PR #135): all
five platform jobs, `updater` and `publish` success after the operator's two approvals. 25 assets:
macOS universal (zip + updater archive), Windows x64 and arm64 (zip, setup, receipts), Linux x64 and
arm64 (.deb, AppImage, CLI tarball, receipts), `latest.json` with seven platform keys,
`SHA256SUMS(.asc)`. Anonymous download verified: GPG good, checksums OK, Sigstore attestation exit 0,
macOS app `spctl` Notarized Developer ID, universal. Windows still `NOT_SIGNED` (identity validation
In Progress, SB-03).

**Exact next task:** when the Azure identity validation reads Completed, run the two commands in
DISTRIBUTION.md (profile + `AZURE_SIGNING_ENABLED=true`) and cut a signed release; SB-91 (Linux and
Windows ARM on the download page).

---

## 2026-10-09 — Linux and Windows on ARM (SB-88…SB-91)

Operator decision 2026-10-09: every PassionCode.ai product runs on Apple silicon and Intel Macs,
Windows and Linux. Branch `agent/linux-port-20261009` (PR #132).

- **Linux works** (SB-88): Secret Service vault and backup key (keyring 3, D-Bus), session scripts
  in POSIX `sh`, terminal launch (`$TERMINAL`, then the common terminals), XDG log and autostart,
  AppImage self-update (a .deb copy never checks and says how it updates), `.deb` with
  `/usr/bin/switchboard`, AppImage, CLI tarball. Verified in Docker (arm64): workspace tests with a
  real GNOME Keyring, clippy clean, `smoke_native.py` PASS (ordinary + background) on the build,
  the AppImage, and the .deb installed on a clean Debian 12.
- **Windows on ARM** (SB-89): release and nightly matrices, arm64 overlay and packaging.
- **SB-90**: the control client waits for hyper's readiness before the mutation (a Linux-visible race).
- Release workflow: `linux` job (x64 + arm64), Windows matrix, `latest.json` requires all seven
  platform keys. Nightly: Linux job (tests, clippy, bundles, smoke) and Windows arm64.

Not measured here: a run on GitHub's Ubuntu and `windows-11-arm` runners (first nightly or
rehearsal), interactive use on each desktop environment, live providers.

**Exact next task:** merge PR #132, then dispatch a rehearsal (`vX.Y.Z-rc.N`,
`gh workflow run release.yml -f publish=false`) and read the `linux` and `windows (arm64)` jobs;
then SB-91 (download page). Local Linux checks: `scripts/linux/check.sh`.

---

## 2026-10-09 — Stuck sign-in, Kimi “signed out”, launch with a folder picker (SB-83…SB-87)

Operator reports 2026-10-08/09. Branch `agent/signin-stuck-20261008`.

- **SB-83** — Cancel ends Switchboard's own waiting sign-in in Terminal (identity-checked SIGHUP,
  then SIGTERM; Windows `taskkill /T /F` after pid + creation-time match) and cleans up; a refused
  Cancel no longer freezes the banner's poll or offers a finish *Retry*; Kimi's dialog keeps polling
  and its sign-in reads `ended` once Terminal closes.
- **SB-84** — the ordinary Kimi sign-in is read from the slot `config.toml` names (as `kimi` 2.1
  does); a kimi.ai login no longer reads as signed out because of an old kimi.com tombstone or a
  stale `region` marker.
- **SB-85** — a failed quota check logs the HTTP status number; the cause of the recurring
  `http_error` is still to be read from the log after the release.
- **SB-86** — one *Launch…* per account with an *Isolated session* box; every folder is chosen with
  the system picker (`pick_folder`, `tauri-plugin-dialog`, called from Rust only); Kimi rows use the
  account-row layout with small buttons. Rule recorded for all products in the operator's global
  instructions.
- **SB-87** filed: start-up cleanup of forgotten sign-in homes.

Checks: `./scripts/check.sh` exit 0; browser demo (Kimi row, launch dialog, picker, required
folder). Not in a release; Windows paths compiled only.

**Exact next task:** open the PR for this branch, include it in the next operator-approved release,
then on the installed app: cancel a waiting sign-in, confirm the ordinary Kimi line, pick a folder
natively, and read `usage_check` `status` in the log for SB-85.

---

## 2026-10-08 — Provider terms notice (SCN-049)

Operator decision 2026-10-08: a plain notice that Switchboard is used within the terms of Anthropic
and OpenAI, that breaking them can get the account blocked, and that each person is responsible for
their accounts. Accounts shows it as a dismissible banner from the first start (it is in sight while
the first account is added); the dismissal is stored as `switchboard.terms-notice`; About →
*Provider terms* shows it always. EN and RU through the locale catalog; README (EN, RU) carries the
same paragraph under *Using it*. Decisions live in `src/ui-logic.ts` (`termsNoticeOn`,
`termsNoticeDismissed`, `dismissTermsNotice`) and are covered by `scripts/test-ui-logic.mjs`;
browser demo checked in both languages. `./scripts/check.sh` exit 0. Not in a release yet.

**Exact next task:** include it in the next operator-approved release; the 0.6.14 entry's next
tasks below are unchanged.

---

## 2026-10-08 — 0.6.14 published; GitHub and wiki refresh

The operator requested current downloads, refreshed README/wiki/GitHub presentation and cleanup.
[Release run 37789186866](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37789186866)
published **v0.6.14**, source `a8618a21b2dae8f94f4de9e70b6ead49457f5395`.
Both platform archives were downloaded anonymously and verified: SHA-256, GPG, GitHub attestations;
macOS also passed codesign, Gatekeeper and staple validation. Native Windows tests and installed-CLI
checks passed. Windows remains Authenticode **NOT_SIGNED**; live-provider acceptance remains open.
The release includes PR #120's version-skew message and PR #121's MCP-only Claude Keychain recovery.
README EN/RU, product/install docs, About description/topics and a synthetic screenshot are updated.
[Run entry](runs/2026-10-08-release-0612/README.md) owns receipts and cleanup (6.806 GiB).
[Wiki handoff](https://github.com/passioncode-ai/fabric-workspace/blob/codex/switchboard-0612/docs/handoffs/2026-10-08-switchboard-0613.md)
owns the knowledge change; Fabric's workspace receipt owns the deployed identity/source pins.

**Exact next task:** finish the wiki sync after this documentation PR and workspace PR #76 merge,
then retry FD-25 in-place console acceptance and the board's operator-assisted SB-79–SB-81 checks.
SB-72 spending/key-scope decisions and the closed terminal's unexplained provider change remain open.
No real provider calls or credential changes were made during this release task.

---

# Claude signed-out recovery and installed-owner restart (2026-10-08)

Branch `codex/auth-recovery-20261008` restores the operator's previous Claude identity and
restarts the stale installed owner into 0.6.10. Codex identity and connected terminals were
preserved. The code fix recognizes a shared-MCP-only Keychain object as signed out, allowing
normal activation while preserving MCP/plugin sign-ins. Unknown envelopes remain refused.
See the [bounded run and recovery receipt](runs/2026-10-08-auth-recovery/README.md).

**Exact next task:** include the recovery fix in the next approved signed release; retry FD-25
in-place console acceptance now that LaunchAccount returns HTTP 200. A reported provider change
in a closed terminal remains unattributed; retain its terminal handle if it happens again.
The 0.6.11 approval and SB-72 decisions below were not changed by this incident recovery.

---

# SB-80 built — Hermes's model and provider (2026-10-08)

Branch `feat/sb-80-hermes-model`: Agents → *Hermes model and provider* reads `hermes config get
model --json` and changes provider/model through Hermes's own `hermes config set` (ids checked
first; Hermes accepts any string). CLI `switchboard agents hermes [set]`, MCP
`switchboard_hermes_status` / `switchboard_hermes_set`. SCN-048, CONTRACTS, CLI.md, CHANGELOG.
0.6.11 (run 37714019602) still waits for the operator's approval of the `release` environment.

**Exact next task:** when run 37714019602 publishes, verify the set and record it in
docs/evidence/release-0.5.md; with 0.6.11 installed, save the Observatory-issued key and launch
Hermes on it (command in the SB-81 entry below). Then SB-72 (spending ceilings).

---

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
