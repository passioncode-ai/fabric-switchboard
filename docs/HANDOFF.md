> **How to read this file.** Newest entry first; only the top entry's *Exact next task* is
> current — the next tasks of older entries are history. Open work lives on the
> [board](evidence/backlog.md).

## 2026-10-21: 0.6.59, attribution kept with the project; OpenRouter keys in Statistics (SB-156)

- **Measured.** The folderless row was 9030 requests from before 0.6.53 plus 3 on 2026-10-20;
  since 0.6.53 every request has its folder; all 40 recorded folders exist and belong to groups.
- **Done.** Counts resolve to their project at count time (cached index, names kept), so removed
  worktrees no longer become projects; *No folder recorded* label; *API keys* section with
  OpenRouter's spend per saved key; a note on what is counted where (Claude Code and Codex through
  the proxy; OpenRouter agents per key; Kimi Code on its account card). Attribution uses only the
  local groups and Git: Switchboard works without Fabric or Observatory. SB-157 filed (launched
  API-key agents carry no folder).
- **Published.** v0.6.59 release run 39606498675 green (preflight, guard, five platform builds, updater, publish), 25 assets, `latest.json` 0.6.59 (2026-10-21T14:01Z); plugin `switchboard@switchboard` updated 0.6.58 → 0.6.59 locally; announced on the board (thread c764dc65, message 193c1f32).
- **Next task.** SB-154; SB-141 when CT-64/CT-66 decide.

## 2026-10-21: verification pass, 0.6.58 (SB-155)

- **Measured on the installed 0.6.57 (2026-10-21).** Statistics by project works: since 0.6.53
  every request but 3 carries its group folder (2026-10-21: 1867 of 1867); 9048 earlier requests
  sit under "Folder not known" (0.6.52 keys). Only `claude-code` traffic is counted (the Codex
  switch is off, by design). Groups hold no husk roots. The log over a day: no errors besides two
  `owner_stopped abandoned`; `transaction_held` up to 975 s (`source_sync`, 2026-10-20 05:20)
  while every wait inside that step is bounded (Claude Code's lock 9 s, `security` 5 s), so the
  process itself stood still (swap at 27 GB then); the 0.6.57 relaunch helper keeps an update
  restart safe against it. Not fixed further: not reproducible on a machine with free memory.
- **Fixed in 0.6.58 (SB-155).** Projects are named by the main checkout's folder, not the remote
  (`project-observatory-open-source`, not `project-observatory-dashboard`); equal names get their
  owner (`research-agent (ssheleg)` / `(prowl-ai)`); the window puts a subfolder under its
  holder's project; no `claude_binding` event for a sync that changed nothing (178 a day).
- **Checks run.** `./scripts/check.sh` exit 0 (full Cargo gate, 32 GB free).
- **Published** 2026-10-21 02:18 UTC (release run 39528364422, 25 assets). Plugin, CLI and app at
  0.6.58. **SB-151 verified live:** the restart into 0.6.58 at 02:52:07 UTC logged
  `update_relaunch_helper started`, `owner_stopped drained` (74 ms) and `owner_started` one second
  later; one app process (`--background`). Wiki page updated (`obsidian-memory-vault` `93652bb0`).
- **Next task.** SB-154 (DEC-0093 catalogue); SB-141 when CT-64/CT-66 decide how Fabric's
  Project id is recorded.

## 2026-10-20: 0.6.57, an update restart always brings the app back (SB-151)

- **Published** 2026-10-20 15:52 UTC (release run 39445035684, 25 assets). Plugin at 0.6.57; the
  app takes it by its own update check. The restart into 0.6.57 still runs 0.6.56's exit path;
  the helper protects the restarts after it.

- **Cause.** The relaunch ran in `updates::finish`, after the owner's drain; at 05:31 the drain
  overran and `hard_exit` ended the process first (no `owner_stopped` between `launch` and
  `hard_exit`). Successful restarts log `owner_stopped` within 22 to 124 ms.
- **Fix.** macOS and Linux spawn `relaunch_helper` before the drain: a detached `/bin/sh` that
  waits for this pid (at most 60 s) and execs the new copy. Windows keeps its NSIS relaunch.
- **Checks run.** `cargo test -p fabric-switchboard relaunch` (2 passed), `cargo clippy -p
  fabric-switchboard --all-targets -D warnings` clean, every non-Cargo step of the gate; the full
  Cargo gate could not run (1.5 to 2.9 GB free); the release guard runs it on the tag.
- **Next task.** Verification of the installed app (thread `0abdfd6f`); then SB-141, SB-154.

## 2026-10-20: 0.6.56 published; iteration closed for Switchboard

- **Published** 2026-10-20 12:06 UTC (release run 39413879646, 25 assets; nightly on the tag
  39414127875 green). App, CLI and plugin at 0.6.56 on this machine.
- **Board, read 2026-10-20.** DEC-0092 is merged (fabric-agent-contract `0de7640`):
  `fabric.project.resolve` (read-only, `{path}` in, `resolveAnswer` out, `pending` while Git cannot
  be read). Who records a non-null answer and how `new` becomes a Project are still open (CT-64,
  CT-66), so SB-141 can now ask Fabric for the Project but not yet record its id. DEC-0093 (the
  coding-agent catalogue, `catalogue/coding-agents.json`, contract `bab2578`) is merged: SB-154.
  Contract pin for DEC-0089 to DEC-0095: `c55f2d5`.
- **Next task.** Verification phase (fabric-workspace thread `0abdfd6f`): test the installed app
  as a user would and record every problem as a backlog row; then SB-151 (an idle update restart
  left no app running), SB-141 against `fabric.project.resolve`, SB-154.

## 2026-10-20: 0.6.56, Statistics tables stacked and sortable (SB-153) with SB-152

- **Done.** Statistics tables run one under another at full width and sort by a click on a
  column title (`sortStatRows`, `nextStatSort`, ui-logic SB-153 case); billions read as B. 0.6.55
  (SB-152 alone) was cancelled at its guard and ships in 0.6.56, tag `f636220`, release run
  39413879646.
- **Checks run.** Every step of `./scripts/check.sh` except the Cargo build and tests, which ran
  out of disk (0.6 to 3.8 GB free while 40 agent sessions swap 27 GB); Rust changed only by the
  version line since the green gate on `7619bb3`. The release guard runs the full nightly on the
  tag (run 39414127875).
- **Data.** The installed app took the 30 husk folders out: groups hold 111, 21 and 120 folders,
  no husk roots left.
- **Next task.** Confirm 0.6.56 published, update the plugin and CLI, announce on the board;
  then SB-151, then SB-141.

## 2026-10-20: 0.6.55, removed worktrees leave their groups (SB-152); call to every agent

- **Found.** 26 folders shown as projects in Agents and PassionCode were removed worktrees under
  `~/DATA/_worktrees` and `~/DATA/_wt`: no `.git`, only `.claude/settings.local.json` left, which
  Switchboard's own binding writes, so the vanish pass never saw them go. Separately `~/DATA/.git`
  was an empty repository (0 objects, no refs, `git init` of 2026-09-29); removed.
- **Done.** `projects::husk` counts such a folder as deleted; `repo_root` refuses a repository
  right under home. The installed app takes the husks out within two vanish looks (10 minutes).
- **Board.** The operator's call to finish every task went to all 34 projects on threads
  `a0e418bb`, `98f4505a`, `04fdb04f`, `8947dbb5`.
- **Next task.** SB-151 (an idle update restart left no app running for 20 minutes), then SB-141.

## 2026-10-20: 0.6.54, the SessionStart hook no longer holds a session's start (SB-149)

- **Cause.** An audit of 44 session starts found the plugin's `SessionStart` hook at about
  10045 ms in 42 of them, with nothing injected. `ProjectSuggest` is not one of the metadata reads
  `Runtime::execute` answers outside the owner transaction (`crates/switchboard-runtime/src/lib.rs`,
  the `matches!` list), so the hook waited for the transaction, which a background pass on a loaded
  machine held for up to 57 s (load average 440 to 640 measured during this build). The CLI's
  control client allows 30 s (`control::request`), so Claude Code's 10 s hook timeout always won.
  Reproduced here: one run of the installed 0.6.52 hook took 29.8 s, the next 0.25 s.
- **Done.** `SESSION_HOOK_DEADLINE` (1.5 s) bounds the stdin read and the owner call together in
  `project_suggest_hook_and_exit` (`crates/switchboard-cli/src/main.rs`); on the deadline it prints
  nothing and exits 0. The plugin's hook `timeout` is 3 s. The add is not lost: the owner may finish
  it, and the background pass adds the folder within 30 s.
- **Decision.** Not moving `ProjectSuggest` out of the owner transaction: it writes
  `project-suggestions.json` and `accounts.json`, which the transaction serialises. The deadline
  is the session's guarantee; the add stays eventual.
- **Tests.** CLI `the_session_start_hook_gives_up_quickly_when_the_owner_does_not_answer` (a fake
  owner that accepts and never answers; 33.45 s before the fix), `the_session_start_hook_waits_for_no_input_it_was_not_given`
  (an open stdin; endless before the fix), `the_plugin_bounds_its_session_start_hook`.
- **Filed.** SB-150: the `StopFailure` hook has the same unbounded call.
- **Published** 2026-10-20 05:32 UTC: PR #257, squash `7bb97e1`, tag `v0.6.54`, release run
  39370706549 (guard with nightly on the tag, all builds, updater, publish: success), 25 assets.
  Installed here: plugin 0.6.54 (`claude plugin update switchboard@switchboard`, SessionStart
  `timeout` 3), app and CLI 0.6.54 by the app's own update (05:53:57 UTC). The installed hook
  command (`switchboard project suggest --from-hook 2>/dev/null || true`) took 0.07 to 0.17 s over
  five runs; the fixed build against the live owner took 0.13 to 1.02 s at load average about 490.
- **Found on the way (SB-151).** The 0.6.53 app's update restart at 05:31:28 UTC drained, logged
  `hard_exit` at 05:31:39 and its relaunch never came up: no owner until the lifecycle broker's
  `ensure_running` at 05:52. The same log shows `transaction_held` 18597 ms at `suggest_found`.
- **Exact next task.** SB-151 (why the background relaunch after an update restart did not start),
  then SB-141.

## 2026-10-20: 0.6.53, projects inside groups and Statistics by project (SB-147)

- **Published** 2026-10-20 03:49 UTC (release run 39361759501, 25 assets; nightly on the tag
  39361948284 green). Docs synced the same day: AGENTS.md lifecycle rows no longer describe REQ-002
  and name the statistics folder field; README's release status and feature table say groups,
  Statistics and Tokens (its em dashes removed); CONTRACTS and code comments no longer speak of
  account-less creation.
- **fabric-workspace knowledge** (products.md, roadmap.md rows for 0.6.53) is staged on branch
  `agent/switchboard-0.6.53-20261020` in `~/DATA/_worktrees/fw-sb053`; both files were leased by
  other runs at 03:45 and 03:57 UTC, so `~/DATA/_worktrees/fw-sb053-land.sh` lands it when the
  leases free (it gives up after 60 minutes; then run it again).

- **Decision (recorded here and in SB-147).** A project is a repository with all its checkouts
  (one `origin` identity), or a folder outside Git: DEC-0089's AT-6, AT-7 and AT-15, and what
  Fabric's Projects are. Its key is Switchboard's own (the identity or the path) until SB-141
  records Fabric's Project id against it: DEC-0089 is merged (fabric-agent-contract `ebfddce`),
  but the wire of Fabric's session-start capability (D4) is not decided.
- **Done.** Group cards list projects; the proxy places each request in its group folder;
  `Statistics` answers by group and by project; the page has a daily chart, cache share and tokens
  per request; Statistics and Activity share one width. Contract: [CONTRACTS.md](CONTRACTS.md) →
  Statistics.
- **Next task.** SB-141 against DEC-0089: run the contract's vectors in `./scripts/check.sh`, then
  replace the suggestion rules with `attributeFolder`'s answers.

## 2026-10-19: 0.6.52, project groups (SB-146), Statistics (SB-145), MCP parity (SB-134 part 3)

- **Published** 2026-10-19 22:30 UTC (release run 39325461704, 25 assets; nightly on the tag green
  on every job). Installed here: app, CLI and plugin at 0.6.52. Announced on the board, thread
  `c764dc65` (fabric, contract, dashboards, adapter, workspace, Observatory).

- **0.6.51** was tagged and not published: Rust 1.99's clippy on CI refused `sort_by` in
  `statistics.rs` (`sort_by_key` with `Reverse` now). The local toolchain was 1.92 and missed it;
  it is 1.99 now. Run `rustup update stable` before a release gate.
- **SB-134 part 3:** eight MCP tools; `mcp_parity::PARITY` has no `LATER` row.

- **Done.** Groups: a repository may be in several groups; sessions take the first group with an
  account; REQ-002 no longer makes a group per repository; hover a group's name for its projects;
  *Also add…* in the disk map. Contract: [CONTRACTS.md](CONTRACTS.md) → Groups (SB-146).
- **Operator data, 2026-10-19 (decision recorded here).** The seven account-less groups REQ-002
  had made were deleted: company-assessment-agent, dataroom (with nicegram-product-workspace and
  nicegram-app-web), hyperframes-agent, my-nicegram-app-web, nicegram-ad-manager-bot and
  nicegram-business moved into Agents (where the other Nicegram and agent repositories are);
  sshlg-skills went back to the default group. Groups now: Agents, Apps & Soft, PassionCode.
- **0.6.50** was tagged and not published: `statistics::tests` used `std::os::unix` and the
  Windows nightly did not build; fixed with `#[cfg(unix)]`, shipped in 0.6.51.
- **Checks run.** `./scripts/check.sh`; WebKit demo (tooltip, *Also add…*).
- **Next task.** Watch the 0.6.51 release, update the plugin; then SB-134 part 3 (rest of the CLI
  over MCP), SB-64/74 `switchboard_continue`, SB-07.

## 2026-10-19: 0.6.50, Statistics page (SB-145); next: project groups (SB-146)

- **Done.** The proxy counts each relayed request per UTC day, account, pool and client family
  (`crates/switchboard-proxy/src/stats.rs`), with the provider's own `usage` read from the response
  stream; the owner keeps 90 days in `statistics.json` (`crates/switchboard-runtime/src/statistics.rs`),
  `Statistics { days }`, CLI `switchboard stats`, MCP `switchboard_statistics`, page *Statistics*
  (SCN-067). Contract: [CONTRACTS.md](CONTRACTS.md) → Statistics.
- **Checks run.** `./scripts/check.sh` exit 0; WebKit demo screenshots ru/en at 1280 and 760.
- **Not counted (by design, written on the page):** off-project Codex passed through on the
  person's own sign-in, and anything outside the proxy.
- **Next task.** SB-146 (operator 2026-10-19): sessions bind to a *group* of projects, a project may
  be in several groups, a hover on a group lists its unique projects (not worktrees or subfolders),
  and the current scattered projects are regrouped.

## 2026-10-19: 0.6.48, Tokens tab and a key per project (SB-144)

- An agent on OpenRouter now runs on the key of the project its launch folder belongs to, else on
  the machine's key: `AgentKey.pool`, `Store::agent_key_in`, every launch script's key command
  carries `--dir <launch folder>` (it runs before the script's `cd`). Hermes, OpenCode and
  OpenRouter launches take the default model from the same key.
- Window: a Tokens page (machine key + Project keys with Add / Change model / Replace / Remove);
  Agents keeps a pointer. CLI `agents openrouter set|status|model|remove --pool`, `list`;
  `agents key --service openrouter --dir`. MCP status lists `keys` and `projects`.
- Deleting a project deletes its keys; backups carry the pool and restore a project key only with
  its project. Old versions (0.6.47 and before) cannot read a store holding a project key.

**Checks.** `./scripts/check.sh` exit 0; WebKit demo, English and Russian: a project key added
from Tokens, no page errors.

**Exact next task.** SB-143 (Codex per project): measure codex 0.163 against the proxy (request
body carries the folder? ChatGPT backend path? behaviour with Switchboard stopped), then build the
opt-in switch. Then the `/openrouter/<pool>` proxy route (SB-97/SB-72).

## 2026-10-19: 0.6.47, one mark per meaning in the account list (SB-142); SB-143, SB-144 designed

Operator 2026-10-19 (dictated): Codex not attributed to projects; buttons drifted; the blue bar
read as selected in one project and not in others; more agents; a Tokens tab so an agent on an
API key uses its project's key by the folder it starts in.

- **SB-142 (0.6.47).** The blue bar was `is-current`, the ordinary CLI's login (one per machine),
  so it showed in one pool only. Now the gold bar marks each pool's or project's used account, every
  pool heading says which account its sessions use, and the CLI login is named on the provider
  heading. Header buttons in one row; project and rule actions at the top right; Kimi windows one
  per line. Checked in WebKit on the demo, 760 and 1100 px, Russian.
- **SB-143 designed:** Codex ignores `model_provider`/`openai_base_url` in a project config, so the
  route is an opt-in user-level provider at the proxy that picks the project by the request's
  folder and passes other folders through. Measure first.
- **SB-144 designed:** a key per project pool, resolved by the folder the agent's key command runs
  in; a Tokens page.

**Checks.** `./scripts/check.sh` exit 0.

**Exact next task.** SB-144 (fully inside Switchboard), then SB-143 after measuring codex 0.163.

## 2026-10-19: 0.6.46, launched sessions keep the operator's setup (SB-137, SB-130)

- **SB-137.** `crates/switchboard-runtime/src/carry.rs`: a Claude Code home links the operator's
  `CLAUDE.md`, skills, commands, agents, output styles, plugins and hooks and gets their settings
  without `env`, sign-in helpers or secret-shaped keys (the account's own `env` on top when
  isolated). Pi's `agent-home` and Codex homes likewise (Pi: `AGENTS.md`, skills, extensions,
  prompts, themes, settings; Codex: `AGENTS.md`). Links, not copies: the operator's edits reach
  running homes. `Host::user_scope` keeps tests off the real folders.
- **SB-130, decided.** A continuation home starts past the first-run screen and trusts only the
  folder the workflow's previous agent worked in; a person's own launch still asks.
- **Open:** Windows (no links yet); Pi's skill listing (it reads `~/.agents/skills` from `$HOME`);
  a live console hand-over that reaches its first prompt.

**Checks.** `./scripts/check.sh` exit 0.

**Exact next task.** Watch the next console hand-over (SB-130 live). SB-141 when CT-57 lands.
Then the SB-135 rest and SB-114/115 live outcome.

## 2026-10-19: status report (operator's request, board thread bd1c9611, reply b963d929)

Measured on `origin/main` `802238d`.

- **Status:** v0.6.45 is the latest release (published 08:02 UTC); the operator's app updated itself
  to it. No open PR, no unmerged branch on origin.
- **What we worked on (since 0.6.41):** SB-H01, SB-138, SB-135 part 2, SB-132, SB-122 live check,
  board hygiene; new row SB-141.
- **Done:** v0.6.42 (PR #237: analytics host; scan reads off the owner transaction, no
  `transaction_held` 05:14 to 07:10 UTC against 15 in the 90 minutes before), v0.6.43 (PR #238:
  repositories on disk, `MoveRoot`, CLI and MCP), v0.6.44 (PR #239: Codex sessions' repositories;
  rotation and recording off the async workers), v0.6.45 (PR #240: the map walks into a chosen
  folder that is a repository). Every release run and its nightly on the tag green. SB-122 closed
  live. Board items to us answered: lifecycle (cb31a510), update check (92c37fc0), SB-H01 (0cc62467).
- **Left (priority):** SB-137 with SB-130 (a session Switchboard launches in its own home loses the
  operator's instructions, settings and folder trust); SB-141; SB-135 rest (module checkouts, the
  map opened once in the installed app); SB-114/115 live limit stop; SB-96, SB-47, SB-71,
  SB-109 to SB-111.
- **Blocked by:** SB-141 on fabric-agent-contract CT-57 (PR targeted 2026-10-22, thread 617c8cbe);
  SB-96 on Observatory's grant registry; SB-47 on Fabric COM-03. Human gates: SB-01/SB-15 live
  sign-ins and a day of switching, SB-02 a Windows host, SB-125 the operator's decision.
- **When it can ship:** every merged change ships the same day (preflight, release run, nightly on
  the tag). Next: 0.6.46 with SB-137/SB-130; SB-141 within one working day after CT-57 merges.
- **Next action now:** SB-137/SB-130.

## 2026-10-19: 0.6.45, the disk map walks into a chosen folder that is a repository

- **Released today:** 0.6.42 (04:39 UTC), 0.6.43 (05:38), 0.6.44 (06:24); each release and its
  nightly on the tag green. The owner moved from 0.6.41 to 0.6.42 by the new drain at 05:14
  (`update_restart outcome:drained`, 35 s, no request in flight) and on to 0.6.44 by itself.
- **Measured on the new owner:** no `transaction_held` from 05:14 to 07:10 UTC, against 15 in the
  90 minutes before on 0.6.41. Load had also fallen (from ~100 to 12-21), so part of it is the
  machine; `suggest_found` no longer shows at all.
- **Defect found live in 0.6.43's map:** `~/DATA` carries an empty `.git` of its own (since
  2026-09-29, not Switchboard's, left in place), so the walk stopped at the chosen folder and
  listed one row. A chosen folder is now always walked into. 185 repositories in 173 ms on this
  machine.
- **SB-H01 answered** on the board (thread 0cc62467, message cb9a714f).
- **New row SB-141** (unified folder attribution, board thread 617c8cbe): waits for
  fabric-agent-contract CT-57 (vectors, PR targeted 2026-10-22). Replied on the thread with that
  plan.

**Checks.** `./scripts/check.sh` exit 0; new test
`a_chosen_folder_that_is_a_repository_itself_is_still_walked`.

**Published:** v0.6.45 (run 39215128665 and its nightly green). Root move checked live through the
owner on 0.6.44: `nicegram-app-web` from `dataroom` to `my-nicegram-app-web` and back, and the
last-folder refusal (board SB-135).

**Exact next task.** Open *Repositories on disk* in the installed 0.6.45 app once (the section
itself was checked in WebKit on the demo only). Watch for the
first `recovery_hook registered` in a project folder (0.6.41's fix, not observed yet: no stop
since). When CT-57 lands, SB-141. Then SB-137.

## 2026-10-19: 0.6.44, Codex sessions' repositories join their project (SB-132)

- **SB-132.** `NativeSources::rollouts` (`limits::codex_rollout_files`) lists Codex rollouts of
  the last 15 minutes in the latest 31 day folders; the SB-131 scan reads them where they grew.
  `activity::codex_evidence` takes the session folder from `turn_context`/`session_meta` (on a
  first read from the tail, from the file's first line) and the changed folders from `apply_patch`
  patches and `exec_command({cmd, workdir})` git writes in code-mode scripts, or `shell` calls in
  older versions. A template with `${…}` is skipped, never guessed. Only paths are kept. On 8
  real rollouts of this machine it found 8 folders, each with its session's folder.
- **SB-138 follow-up.** On 0.6.41 the log also showed `rotate` holding the owner transaction up
  to 49,184 ms and `worked` 10,398 ms (load ~100, swap 10/11 GB). Both now run as blocking
  sections; the transaction still waits for the disk, the async workers do not.
- **Open in SB-132:** Claude sessions Switchboard launches isolated are still not read.
- **Fabric board:** the reply to passioncode-platform about SB-H01 (thread 0cc62467, key
  `sb-h01-switch-0.6.42`) could not be sent: the hub answered "cannot check credentials right
  now" and the session's Fabric tools dropped. Send it with the same key when the board answers.

**Checks.** `./scripts/check.sh` exit 0 on the branch.

**Exact next task.** Send the SB-H01 reply on the board (same idempotency key). After 0.6.44 runs:
re-measure `transaction_held` (suggest_found, rotate, worked), open *Repositories on disk* once and
time the walk, then SB-137.

## 2026-10-19: 0.6.43, repositories on disk and moving a repository between projects (SB-135 part 2)

- **0.6.42 published** 2026-10-19 04:39:27 UTC (run 39197312600, 25 assets; nightly on the tag
  green, run 39197489647).
- **Disk map.** `RepositoryMap { under }` lists the main checkouts under the chosen folders (three
  levels, not into a repository, hidden folders or dependency folders; `~/DATA` else home when none
  is chosen), each with its project and place (`folder` or `inside`). Read-only, outside the owner
  transaction.
- **Root move.** `MoveRoot { folder, pool }`: the folder and its derived folders out of their
  project and into `pool`'s, as two `ProjectEdit` saves; a refused second save restores the first.
  A loose repository joins; a project's last folder and a repository inside a project folder are
  refused with their own messages.
- **Window.** Projects → *Repositories on disk*: closed by default (the walk runs only when
  opened), chosen folders remembered (`switchboard.map-folders`), filter *Not in a project* / *All*,
  one action per row and a project dialog. SCN-064.
- **Agents.** CLI `project map`, `project move-root`; MCP `switchboard_repository_map` (also on a
  read-only server) and `switchboard_project_move_root` (write); parity table updated.

**Checks.** `./scripts/check.sh` exit 0; WebKit (Playwright 2359 build, demo `?demo=1`) in English
and Russian: the section opens, 2 loose and 4 in all, *Add to project…* added `beta-cli` to
“Alpha Web” with the notice, no page errors.

**Exact next task.** After 0.6.43 runs on the operator's machine: open *Repositories on disk*,
measure how long the `~/DATA` walk takes, add one loose repository to a project and move one
repository there and back; then SB-138 re-measure (no long `suggest_found` holds), SB-132 (Codex
rollouts), SB-137.

## 2026-10-19: 0.6.42, analytics on PassionCode's host (SB-H01) and the next owner stall

- **0.6.41 checked live.** The owner restarted into it by itself at 2026-10-18 23:44 UTC
  (`update_restart outcome:idle`), so the broker's failed restart no longer matters. Since then:
  209 folders bound, `capability-migration.json` present, no `Proxy is busy` in any transcript
  written after the start. `recovery_hook registered` is not seen yet: no session stopped in a
  project folder since the start.
- **SB-122 closed live.** `switchboard accounts move` took claude7@sshlg.me into
  `heygen-hyperframes-vr` and back to `default`; both answered `moved: true` and the project list
  followed.
- **New stall found and fixed (SB-138 follow-up).** The log named `suggest_found` holding the
  owner transaction 15 times in 90 minutes, up to 56,813 ms, and `vanish` 12,138 ms. The machine
  had a load average of about 100 and 10 of 11 GB of swap in use. `settle` read every project
  repository's Git files and each candidate's under the transaction, on an async worker. Now
  `suggest::assess_all` reads them in a blocking section before the transaction; under it only
  the suggestion list is written (`settle_assessed`), and only when the projects did not change
  meanwhile (otherwise the scanner starts over at the next pass and finds them again). `vanish`
  writes in a blocking section.
- **SB-H01.** `analytics::HOST` is `https://analytics.passioncode.ai`. The vault key
  `passioncode-platform/prod/APTABASE_APP_KEY_SWITCHBOARD` answered a debug event with HTTP 200 and
  was set as the `release` environment secret `SWITCHBOARD_ANALYTICS_APP_KEY` from stdin
  (2026-10-19 03:20 UTC), never printed. docs/ANALYTICS.md updated.
- **Board.** Rows released in 0.6.36 to 0.6.40 no longer say «unreleased»; SB-H01 added.
- **Machine.** 6 GB free on `/` and swap nearly full: noted, not this repository's work.

**Checks.** `./scripts/check.sh` exit 0 on the branch; new tests
`assessing_found_folders_writes_nothing_and_settling_records_them`,
`release_builds_send_to_passioncodes_own_host`.

**Exact next task.** After 0.6.42 runs: no `transaction_held` from `suggest_found` above a few
seconds in the log (re-measure SB-138), and answer passioncode-platform on thread 0cc62467 with the
publish time if not done. Then SB-135 part 2 (disk map of unassigned repositories), SB-132 (Codex
rollouts), SB-137.

## 2026-10-18 — 0.6.41: hot fixes from the live check (proxy slots, update drain, recovery hook)

- **Measured live on 0.6.39/0.6.40.** Proxy port had 15 to 25 connections in every one of 90
  one-second samples, never 0; recent transcripts held 154 `503 Proxy is busy` answers (16
  slots). The owner ran 0.6.39 since 15:50 although 0.6.40 was in the bundle since 18:03: the
  idle watch waited for a moment with no request in flight (`update_waiting request_in_flight`).
  `recovery_hook refused` 1,202 times: a project folder's stop named the ordinary CLI's account.
  `transaction_held step bindings` up to 12.9 s.
- **Fixed.** 64 proxy slots; after 30 min of waiting only for requests the update drains (503
  with `Retry-After: 5`, 90 s for running requests, background restart); the hook names the
  project's routed account; the bindings pass and capability rotation run under their own lock,
  off the owner transaction, on a blocking thread.
- **Not done.** The lifecycle broker's `restart` of `switchboard.desktop` timed out
  (`verification_timeout`) and its status calls time out, so the running owner could not be moved
  to 0.6.40 by an agent. 0.6.41's own drain moves it once 0.6.41 runs, but the running 0.6.39 code
  installs 0.6.41 and still waits under the old rule; the operator's tray *Restart to update* does
  it at once.

**Checks.** `./scripts/check.sh` exit 0; new tests
`a_draining_proxy_asks_new_requests_to_retry_and_reaches_the_provider_again_after`,
`an_update_waiting_only_for_requests_drains_after_half_an_hour`,
`a_stop_in_a_project_folder_is_the_projects_account`.

**Exact next task.** After the owner runs 0.6.41: check the log for no `Proxy is busy`, `recovery_hook
registered` in project folders, `capability-migration.json` present; then SB-122 live move, SB-135
part 2 (disk map), SB-132.

## 2026-10-18 — 0.6.39: move an account to another pool from its card (SB-122)

- **Behavior.** The account card's ⋯ menu has *Move to another pool…* (SCN-062): the default pool
  first, then every project by name, the current pool marked “(now)”. `Operation::MoveAccount
  { id, pool }` joins a project through `ProjectEdit` (so the old project lets the account go and
  every refusal of a project save applies), leaves one the same way, and takes an account left in
  a deleted project's pool back to `default` with `Store::return_to_default`. Any other pool is
  refused (`MOVE_ACCOUNT_NO_PROJECT`). The account the ordinary CLI is signed in to cannot join a
  project: the dialog greys projects out and says why. Agents: `accounts move <id> --pool`, MCP
  `switchboard_account_move` (parity table updated).
- **Live, 0.6.38.** `project rotate-capability --pool agents` rewrote 85 folders; all 196 bound
  folders carry the `X-Switchboard-Capability` header and none keeps `ANTHROPIC_AUTH_TOKEN`
  (SB-139/SB-140 rows updated). The dataroom session that asked had ended; nothing was sent.

**Checks.** `./scripts/check.sh` exit 0 on the branch; the move dialog checked in WebKit
(Playwright, demo `?demo=1`) in English and Russian: a loose-pool account moved to the default
pool and the notice named it. Release preflight for v0.6.39: ok.

**Exact next task.** After 0.6.39 is installed, move one account into a project and back from its
card, then close SB-122 on the board with that evidence. Then the open board rows by priority:
SB-132, SB-133, SB-135, SB-137, SB-134 part 3.

## 2026-10-18 — REQ-002: an agent's unassigned Git repository becomes a project

- **Behavior.** `ProjectSuggest`, the plugin's `SessionStart` hook and the existing background
  activity pass now create an account-less project when no existing project claims the agent's
  Git root. Creation reuses `projects::project_dir` and `Store::save_project` with an empty
  account set. Existing worktree/clone/source attachment still goes through
  `add_suggested_folder`; there is no second scanner and no broad parent scan.
- **Preserved.** Existing matches and `in_project` win; equal matches create nothing; non-Git,
  home/root and transient/tool folders are refused; `added`, `dismissed` and `shared` memory is
  not undone; `gone` may be found again. `reason: unassigned` rows give the new project the same
  one-notification and removal memory as an accepted suggestion. No account IDs move.
- **Fallback.** A provider with no enabled account in the new project keeps its existing launch
  fallback (`launch::project_allows`). Automatic `--provider` selection is a separate path and
  still returns `NO_PROJECT_SELECTION` without a selected project account.
- **Lifecycle.** The monitor stop receiver flows through `Pass::run` → `suggest_pass` →
  `Activity::collect_while`: collection stops between transcripts, returns no partial evidence
  and skips the rest of the pass. Automatic registration and the LC-01 drain deadline coexist.
- **Docs.** CONTRACTS, SPEC, AGENT-ONBOARDING, CLI, MCP instructions/tool descriptions, SCN-055,
  AGENTS lifecycle, OPERATIONS, the plugin guide and the changelog describe that behavior;
  AGENT-ONBOARDING no longer says an account-less project cannot start a provider's sessions.

**Checks.** The new tests first failed before implementation. On the final rebased commits,
`./scripts/check.sh` exited 0, including frontend build, Rust tests/clippy, Node checks,
documentation, plugin validation, packaging and third-party notices. The focused SessionStart
creation test, background creation test, stop-aware collection test and SIGTERM/SIGINT shutdown
deadline test all pass. `git diff --check` also passes.

**Lifecycle follow-up.** `serve_stops_on_sigterm_and_sigint_within_the_deadline` initially failed
at `commands.rs:927`: removing the empty-project guard also made the first background pass read
recent Claude transcripts (22 session files, about 767 MB on this machine, plus 13 subagent
files), and synchronous activity collection delayed the drain. `Pass::run` now passes its stop
receiver through `suggest_pass` into `Activity::collect_while`; a stop between transcripts returns
no partial evidence and skips the rest of the pass, preserving automatic project creation and the
LC-01 drain. `collection_stops_between_transcripts_without_returning_partial_evidence`, the
original SIGTERM/SIGINT lifecycle test, and the background project-creation test pass.

**Exact next task:** land the branch and run the isolated two-round cross-product scenario.
No live-provider or Windows acceptance is claimed here.

## 2026-10-17 (evening) — 0.6.35–0.6.37: lifecycle prompts, 5-minute updates, project delta, no stalls

Releases:
- **0.6.35** (SB-136): a «Restart to finish updating» notification and a refused-login-item notification.
- **0.6.36**:
  - update checks every 5 minutes (operator 2026-10-17, for every PassionCode product; asked of the others on the Fabric board, thread 92c37fc0);
  - SB-134 part 1: `project edit`, MCP `switchboard_projects` and `switchboard_project_edit`.
- **0.6.37** (SB-138, release run 39073078291):
  - **Stalls removed.** The owner had stalled control requests for up to 12 s, and `handovers` for up to 366 s (Fabric Dashboards FD-71). The background file work now runs in blocking sections, and subagent directories are listed every 120 s. `Handovers`/`HandoverTake` skip the owner transaction, and a transaction held ≥5 s logs `transaction_held` with the step that held it.

Live, operator's machine:
- **Folders:** 31 deleted folders left their projects and 15 found folders joined, with nothing asked.
- **Duplicate MCP registration removed:** there had been a user-scope `switchboard` beside the plugin, giving 72 processes. The plugin was updated to 0.6.36.

Accounts moved on the operator's word (2026-10-16/17):
- **Agents:** claude8 and claude6.
- **Apps & Soft:** claude5 and sergeysheleg4 (Claude).
- **PassionCode:** vibecoder3.
- **Back to default:** twingpt.
- **Stays in default:** the Codex account of sergeysheleg4. It is the ordinary Codex CLI login, which no project may hold; switching that login was left alone, since it would affect every Codex session.

**Exact next task:**
1. After 0.6.37 is installed, re-measure `agents status` and `handovers` through the owner, the same way: one probe every 5 s for 10 minutes. Read any `transaction_held` lines and fix the step they name.
2. Then SB-134 part 2: the rest of the CLI over MCP, and the CI parity check.

---

## 2026-10-17 — 0.6.34 published (SB-131); lifecycle prompts (SB-136); accounts moved

- **0.6.34**:
  - **Published:** [v0.6.34](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.34) on 2026-10-17 00:42 UTC, run 38967680225 on `8bc1964`, 25 assets.
  - **First tag refused:** the release guard refused the first tag on `c87cca6` because the Windows nightly failed. A Windows verbatim `\\?\` path was being read as a glob pattern in the SB-131 work scan; fixed in `8bc1964`, together with the move of the nightly macOS job to `macos-15`.
  - **Second run, two failures:**
    - The Linux idle-owner test failed once; it now waits up to a bounded second for the lock (`#221`).
    - Sigstore's tlog fetch failed once during publish, and re-running publish passed.
- **SB-136** (`85794df`, not released yet):
  - a «Restart to finish updating» notification, once per version;
  - a notification when the login item is refused at start.

  These answer the Fabric board's lifecycle rule (thread cb31a510; reply 089f3ac0). The row was renumbered from a duplicate SB-134.
- **Accounts (operator, 2026-10-16):**
  - claude8@sshlg.me moved into the Agents pool;
  - claude6@sshlg.me and twingpt@appvillis.com moved back to `default`.

  Saving the Agents project also dropped four worktrees that had been deleted from disk.
- **New row:** SB-137, from the hosts-contracts finding (isolated Pi and Claude homes lose the operator's setup).

**Exact next task:**
1. Watch the operator's app take 0.6.34. It may need one restart, because 0.6.31 was waiting behind an open window.
2. Release SB-136 as 0.6.35.
3. Then SB-134/SB-135 (MCP parity, root folders; [packet](packets/sb-134-135-mcp-parity-and-project-roots.md)), unless another session holds them.

---

## 2026-10-15 (day) — SB-131: projects keep their repositories by themselves, nothing is asked

Operator request 2026-10-15: «автодобавление всех репозиториев нужных в проект, когда там агент
работает … без подтверждений; в настройках проекта смотреть, удалять, добавлять, редактировать
репозитории пикерами папок; если репы удаляются — удалять и из свитчера».

- **Why questions still showed:** the running app was 0.6.31 (started 2026-10-14 12:23 UTC) with
  0.6.32 and 0.6.33 already installed. Its update was waiting behind a window that had been left
  open, logging `update_waiting window_visible` every hour.
- **Done on `feat/sb-131-auto-projects`** (`dddff8b`, `ed99f49`, `9f89d8c` and the final-review fix):
  - **Repositories join by work.** `activity.rs` reads recent Claude Code transcripts (sessions
    and subagents), only where they grew, and keeps paths only. A repository an agent of project
    P edits or commits in joins P. A repository two projects work in is `shared` and joins
    neither.
  - **Nothing is asked.** There is no banner, no tray count and no question notification. A
    folder that cannot be added is retried every 10 minutes.
  - **Deleted folders leave.** A deleted project folder leaves its project after two looks
    5 minutes apart, checked by device; an empty folder left where it was (an unmounted mount
    point) keeps it after a restart. Unmounted drives and the last folder stay.
  - **Dialog.** Each folder has *Change…* in the project dialog. Save keeps folders Switchboard
    added in the meantime (`base_folders`).
  - **Updates.** An update no longer waits behind a window that has been out of focus for
    30 minutes.
- **Checks:**
  - `./scripts/check.sh` exit 0 on `9f89d8c`;
  - `cargo xwin check` for the runtime and CLI crates;
  - three independent review rounds: seam, then unit and product, then a final pass on the
    delta. All their `breaks` findings are fixed.
- **Open:**
  - SB-132: Codex sessions and isolated launched sessions are not read.
  - SB-133 (P2): a renamed folder keeps its binding.
  - A copy already waiting behind an open window needs one manual restart to reach the release
    that carries this.

**Exact next task:** release this as 0.6.34. Then restart the operator's installed app once (About → *Restart to update*). Then watch the log for `project_suggestion worked` and `project_folder_gone`, and check that the 10 missing PassionCode folders leave the project.

---

## 2026-10-15 (morning) — 0.6.33 published: SB-129; SB-127, SB-49 closed

- **SB-129** released in [v0.6.33](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.33)
  (03:21:51Z, run 38672112470, every job green). A workflow whose executor names no account is
  handed over only while it is worked (`silentSeconds` ≤ 3600).
- **SB-127** merged (`13e0ca4`, ships with the next release): every early quota-check refusal has a
  log code. The person still sees the store's own message.
- **SB-49** closed. The installed app took 0.66 % CPU over an hour while relaying 24 session and
  17 provider connections; that is load, not idle.
- The installed app updated itself to 0.6.32 at 00:38Z; 0.6.33 follows at its next check.
- **Models** per the operator's list (`synthesis/llm-models.md`): agents' OpenRouter default and
  Hermes on mimo. Switchboard itself calls no model.

**Exact next task** (unchanged): the operator decides SB-130 (trust the continuation's checkout, or
keep the dialog). Then SB-122: confirm the pool move on 0.6.32+, then the *Move to…* action. Then
SB-82 (runner kinds: `cursor-cli` → `cursor-agent` with an alias for saved chains).

---

## 2026-10-15 — 0.6.32 published; iteration closed

**Released** [v0.6.32](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.32)
on 2026-10-14 at 22:49:50Z. Run 38644478397 succeeded on every job: guard, macOS,
Windows x64/arm64, Linux x64/arm64, updater, publish; 25 assets. The tag is on `b6cf703`.

The guard refused twice before that. Both times the fix went to the cause:
- nightly failed on Windows on Unix paths in the SB-20 and SB-121 tests (#209);
- a port-move test was flaky on Linux x64 (#210).

The guard's earlier runs published nothing, so the tag was moved.

**Also on the way.**
- **Observatory folder (operator question):** `/Users/sshlg/DATA/project-observatory` was in no
  project. Its origin is `ssheleg/project-observatory`, which matches nothing in PassionCode. It is
  now in PassionCode and bound to the pool; its session restarts to pick the route up.
- **SB-128** found and fixed: 10 folders deleted from disk blocked every save of the PassionCode
  project. This is likely CALL-07.
- **Live FD-62 hand-overs.**
  - SB-129: the scratch workflow was handed over twice by the automatic fallback; it is now
    closed.
  - SB-130: both console sessions stopped at Claude Code's folder-trust dialog.

**Waiting, with watchers.** The installed app (0.6.31) updates itself to 0.6.32 at its next
6-hourly check. A background watcher then measures idle CPU for an hour (SB-49).

**Exact next task:**
1. The operator decides SB-130: pre-accept trust for a continuation's own checkout, or keep the
   dialog.
2. Then build SB-129: hand over only workflows whose executor wrote recently or names an account.
3. Then SB-122: confirm the pool move works on 0.6.32, then the *Move to…* action.

---

## 2026-10-14 (evening) — closing iteration: everything finishable here is done; 0.6.32

**Objective** (operator: «доделай до конца … закрой все задачи … зафиксируй … задеплой … подведи
итог»). The whole repository was swept for unfinished work and one iteration closed what an agent
can close.

**Done in this iteration**
- **Repository hygiene.**
  - 23 squash-merged branches deleted (remote and local).
  - The abandoned `sb-118` worktree removed; its receipt was already on main.
  - My expired SB-118 lease released.
  - The main checkout moved back to `main`.
  - Commit `829a800` (a fabric-mcp registration for OpenCode, committed here by mistake with
    `--runner claude-code`) handed to Fabric and its branch deleted (SB-126).
- **SB-20** (orphaned PR #204: launched sessions reach the Fabric board) reviewed, given one fix
  (a board switched off in Codex stays off) and merged as `e37602d`.
- **SB-121** (operator, mid-iteration): a project's new folders join it by themselves.
  - One system notification per folder; no question.
  - Removed folders stay out.
  - Scratch and cache folders never join.
  - The 11 questions waiting on this Mac were answered by hand: 8 added, 3 scratch declined.
- **SB-112**: `serve --data-dir` logs inside its own data folder.
- **`continue --to-console`**: SB-108 by hand, for the FD-62 live check.
- **Board.** 15 released rows closed with receipts. New rows SB-121…SB-127:
  - CALL-07, CALL-08, CALL-09 and В-06 from the founders' call;
  - the fabric-mcp OpenCode runner;
  - `usage_check` `other`.
- **SB-85** closed: no `http_error` since 2026-10-09.
- **Models** per the operator's list: the agents' OpenRouter default and Hermes are now
  `xiaomi/mimo-v2.6-pro` (were `deepseek-v4-pro`).

**Checks.**
- `./scripts/check.sh` exit 0 (609 Rust tests).
- Two review rounds:
  - round 1 found three real bugs, all fixed with tests: a hand-added folder came back after
    removal; the sweep ignored scratch folders; `--to-console` without `--account` was
    "unknown";
  - round 2 checked the fixes.

**Open, with owners**
- **FD-62 live hand-over.** Workflow `wf_5e78a8c1a8ee6c19` in `/Users/sshlg/DATA/fd62-live-check`
  is ready. After 0.6.32 is installed:
  `switchboard continue wf_5e78a8c1a8ee6c19 --account opencode --dir /Users/sshlg/DATA/fd62-live-check --to-console`.
  Then tell the fabric-dashboards session the hand-over id.
- **Fabric's settings screen**: Fabric's work (shape sent).
- **SB-49 idle CPU**: re-measure on 0.6.32.
- **The operator's live checks**: SB-115 StopFailure, SB-01/SB-15 acceptance.
- **SB-122**: reproduce the pool-move conflict with the operator.
- **Hosts**: SB-02 needs a Windows host; SB-109/110/111 are the RM-28 pilot.

**Exact next task:** watch the 0.6.32 release, confirm the installed app updates, run the FD-62
hand-over above and report it to fabric-dashboards, then re-measure idle CPU (SB-49).

---

## 2026-10-14 — 0.6.31 released: SB-119/SB-120

**Released.** PR #201 (`db748f2`) and release PR #202 (`f285401`, which also added `revision`,
`updated_at`, `path` and `argv` to `agents status` at Fabric's request). Tag `v0.6.31`; release
run 38507040629 succeeded on every job (guard, macOS, Windows x64/arm64, Linux x64/arm64,
updater, publish), 25 assets. Gate at the release commit: `./scripts/check.sh` exit 0, 603 Rust
tests passed. A third review round found that `continue` without an account could choose the
workflow's own account, that the Windows system-folder check missed `\\?\` paths, and that the
tour's Agents step never loaded; all fixed with tests before the release.

The Fabric session received the `switchboard --json agents status` shape (schema 1) for its
Settings screen; Fabric falls back to its own detection when Switchboard is absent or `schema` > 1.

The installed app (0.6.30, started 02:51Z) updates itself at its next check (every 6 h, about
08:52Z) and restarts into 0.6.31 when idle; About → *Check for updates* does it now. The
lifecycle broker did not restart it: the app was not started by the broker.

**Exact next task:** the operator's live checks — one OpenCode continuation on the real
OpenRouter key (`switchboard continue <wf> --account opencode`), one SB-115 StopFailure, one FD-62
hand-over (Dashboards 0.7.9+). Then SB-116 with the Fabric session host.

---

## 2026-10-14 (later) — SB-119/SB-120: agents that take over work, OpenCode, the model picker

**Built on `feat/sb-119-agents`, release 0.6.31 next.** Operator requests 2026-10-14: OpenCode
in Switchboard and in the session-switching pipelines; OpenRouter models chosen from OpenRouter's
list with five recommended; an Orca-style Agents page; polish and finish. Packet:
[sb-119-120-agents](packets/sb-119-120-agents.md); scenarios SCN-057..SCN-060 with WebKit evidence
in `docs/evidence/design-0.6.31/`; contract: CONTRACTS → *Installed agents, switching and the
default agent*.

- The Agents page leads with the **default agent** (chips), then **Installed · N detected** (agents
  that can take over first; command, Enabled/Disabled, Default/Set default, Set up, Details), then
  **Available to install** with copyable install commands, then the OpenRouter key and *Connect an
  agent*. The Hermes model moved into its row. "Take over work" is kept apart from account
  switching in both languages.
- OpenCode continues workflows (chain step or `continue --account opencode`) on the OpenRouter key
  with `OPENCODE_CONFIG_CONTENT` only. `continue` without an account uses the default agent:
  Hermes/OpenCode on the key, Claude Code/Codex/Kimi Code on the account a one-step chain would
  choose. A disabled agent is skipped first in chains (`agent_disabled`). Unreadable settings fail
  closed.
- The model picker: recommended cards, then OpenRouter's tool-capable models as suggestions
  (public list, 10 s, cached 6 h, never on a timer), any typed id.
- Checks: `./scripts/check.sh` exit 0 (lean env); new tests named in SCN-057..060. Unit, seam and
  product verifiers each found real defects (default-agent semantics, picker failure path, search
  order before Homebrew, Sessions naming OpenCode as Codex, fail-open settings, stale scenarios,
  Russian wording); all fixed in `4f285ba`.

Also recorded here (another session's receipt, left uncommitted in `.worktrees/sb-118` when it
stopped; hashes corrected after the 2026-10-14 history purge): **0.6.30 released** — SB-117
(PR #194 `982d8bc`) and SB-118 (PR #195 `44ff10d`; review fixes squashed into it), version PR
#196 `91bb89f`, test fixes #197 `e28c060`, #199 `1b20e67`, #200 `bd60530`; release run
38498498280 succeeded and the installed app updated itself to 0.6.30.

**Exact next task:** release 0.6.31 from this branch: PR, squash merge, bump from `origin/main`
(CHANGELOG `## Unreleased` → `## 0.6.31`), preflight, tag, watch the release to publish, confirm
the installed app updates. Then send the Fabric session the `switchboard --json agents status`
shape (`schema`, `default`, `default_ready`, `agents[]`) for its settings screen. Then the
operator's live checks: one OpenCode continuation on the real key, one SB-115 StopFailure, one
FD-62 hand-over (Dashboards 0.7.9+).

---

## 2026-10-14 — SB-117 project suggestions; SB-118 found

**SB-117 (merged with this entry; release with SB-118).** A project offers its new folders:
worktrees, same-origin clones and repositories of its sources. The background pass is
stamp-driven, the plugin has a `SessionStart` hook, and `switchboard project suggest` is the
manual way. The person answers through a notification, the tray count or the window banner
(Add / Not this one). A project has no folder limit; all projects together hold up to 1024
folders.

- Checks: `./scripts/check.sh` exit 0, 581 Rust tests passed. Unit and seam verification
  each ran twice, and every finding was fixed with a test. The WebKit demo is in SCN-055.
- 0.6.29 (64 folders per project) was published, run 38474680117 succeeded.
- The project PassionCode (pool `passioncode`, vibecoder@ and contact@passioncode.ai, 16
  folders) exists on this Mac.

**SB-118, found 2026-10-14.** The operator saw the project's accounts unused. Measured: every
session runs the ordinary `claude`, signed in as the default-pool account, which serves every
folder. Project reservation reaches only sessions Switchboard launches, and those have minimal
homes (SB-20/SB-110), so nobody uses them.

Proved two ways that the per-folder `.claude/settings.local.json` `env` reroutes ordinary
Claude Code with all user configuration intact:
1. A probe server received `Bearer <capability>` from `claude -p` started in a subfolder.
2. `claude -p` against the real proxy route `/claude/passioncode` answered, and the pool
   rotated (vibecoder 91 % → contact@).

**Exact next task:** SB-118. Add a pool-scoped capability to the proxy, then a bindings sync:
write and remove only what Switchboard wrote, add the file to `info/exclude`, cover worktrees
inside project folders, and re-run on project, folder or port change. After that, docs and
release 0.6.30 with SB-117. Then add the remaining PassionCode clones (and
`project source --add github.com/passioncode-ai`) on the installed 0.6.30, and restart the
PassionCode sessions.

---

## 2026-10-13 (evening) — FD-62 consumer landed; the live hand-over check waits on two updates

Fabric Dashboards reports that FD-62 is on its main as PR #130 (`0c922cb`) and ships in 0.7.9. The console polls
`switchboard --json handovers` every 15 s while its window is shown and every 60 s while hidden. It takes each id
once with `switchboard handovers run <id>` on a PTY after LC-17 admission. A CLI newer than the running app reads as
unavailable, and nothing is taken.

**State on this Mac (20:50 UTC):**
- The installed bundle and the CLI are 0.6.28, but the running app (pid 89584) is older. Its update has waited
  since 17:27 UTC with `update_waiting reason=window_visible`.
- A restart through the lifecycle broker was refused, `in_use_by_person` / `not_started_by_broker` (operation
  `6dbc3dec…`), and is not retried. The operator's step: *Restart to update*, or close the window so the idle
  restart runs.
- Dashboards is 0.7.7. `handovers` is off.

**Watcher:** a background loop in the fabric-switchboard session checks every 2 min for up to 12 h. When the app
answers 0.6.28 and Dashboards is 0.7.9 or later, it runs `switchboard handovers set on`. The loop dies with that
session.

**Exact next task:** if `switchboard --json handovers` shows `enabled: false` while the app is current and Dashboards
is 0.7.9 or later, run `switchboard handovers set on`. Then rehearse one hand-over end to end
(`switchboard continue <wf> --account <id>` with the automatic fallback, or a synthetic queue record). Record that
the console took it once and that no Terminal opened. Then the SB-115 live check: read `recovery_hook` in the log
at the first real limit stop.

---

## 2026-10-13 — SB-115: a limit stop is not a wait

**Objective:** the operator's standing rule (relayed by nicegram-business, decision
"hybrid"): a session, lane or subagent stopped on a usage or session limit is restarted at
once, not at the stated reset. Branch `feat/sb-115-limit-stop-resume` off `b58477b`.
Packet: [SB-115](packets/sb-115-limit-stop-resume.md).

**Done:**
- **The rule:** fabric-workspace principle 10, item 6 (`0bae3875`, PR #194).
- **Skills:** fabric-agent-adapter 0.29.0 (`working-with-fabric`, PR #101, npm 0.29.0) and
  agent-stack 0.28.1 (`agent-orchestrator`, PR #46, npm 0.28.1; `v0.28.0` is a tag on the wrong
  commit, cancelled before publish).
- **Switchboard:** a stop is ready on fresh capacity at once (`mode: capacity`), else as a probe
  after 2/5/10/15 min by `attempt` (`mode: probe`, `probe_at`). Billing stops and pools that are
  fully spend-capped are never probed. One `recovery::readiness` serves both `status` and `claim`.
  `switchboard recovery hook` and the plugin's `hooks/hooks.json` (`StopFailure`, `rate_limit` and
  `billing_error`) register the session's own stop: the account comes from the session's limit
  binding, else the ordinary CLI's; the hook always exits 0. Docs: CLI.md, CONTRACTS.md, the
  plugin README, the `switching-accounts` skill §6, CHANGELOG (Unreleased); board SB-115, SB-116.

**Not done:**
- Switchboard never re-prompts a session: FD-62 and Fabric's host consume `ready` rows
  (SB-114 REQ-5).
- Live acceptance: whether a subscription session limit arrives as `StopFailure`
  `rate_limit` is undocumented and unobserved.
- SB-116: silence detection needs the host.
- **Released 0.6.28** (PR #187 `0678a04`, tag `v0.6.28`): [release run 38357003164](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38357003164) success in every job (guard, preflight, macos, windows x64/arm64, linux x64/arm64, updater, publish); [release](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.28) published 07:09 UTC with 25 assets. The launcher re-pin of the adapter rule is PassionCode 0.1.53 (PC-36), installed here.

**Checks:** `./scripts/check.sh` exited 0 with lean Cargo settings and a shared
`CARGO_TARGET_DIR`, because only 1.4 GiB of disk was free: after the rebase on 0.6.27, 569 Rust tests passed, 3 ignored.
New tests: `recovery::tests`, namely `a_stop_without_capacity_is_probed_on_the_backoff_not_at_the_reset`,
`a_billing_stop_is_never_probed`, `a_probe_claim_is_checked_by_the_same_rule_and_taken_once`,
`repeated_stops_of_one_session_raise_the_attempt`, `an_sb114_record_still_loads` and
`the_hook_names_the_account_the_sessions_limit_was_charged_to`; also CLI
`recovery_hook_never_fails_the_session`. `target/` is 2.5 GB, under the cap. The same night,
two subagents stopped on "session limit · resets 8:10am" and finished after an immediate «resume».

**Exact next task:** once the installed app has updated itself to 0.6.28 (SB-55), restart Claude Code so the plugin's `StopFailure` hook loads. At the next real limit stop, read `recovery_hook` in the log and `switchboard recovery --json`, and record whether a subscription session limit arrives as `rate_limit`. Then, with FD-62 and Fabric's host, have `mode: probe` rows consumed (SB-114 REQ-5), and SB-116.

---

## 2026-10-13 — SB-108 producer recovered; SB-114 capacity recovery

**Objective:** recover the previous Claude Code session, finish its console hand-over
producer, and let session hosts recover work after eligible account capacity returns.
Base: `516ea45` (v0.6.25). Branch: `feat/sb-108-handovers`. Start with the bounded
[SB-114 packet](packets/sb-114-capacity-recovery.md), [CLI](CLI.md) and [contracts](CONTRACTS.md).

**Implemented:** `handovers` records/claims prepared continuations once, offers the host
120 seconds before Terminal fallback and never launches an expired offer. The setting
remains off until its consumer is connected (FD-62). `recovery` retains host-reported
stops across restart, lists account/session counts and eligible same-provider/pool/project
accounts, and persists a claim before returning it. Hidden holds win over 98–99% usage;
a returning account needs a successful observation after the hold deadline. A claimed
launch with an unknown outcome is never retried automatically. Cancelled/finished events
remain deduplicated; bounded storage refuses overflow. Operational files are excluded
from backups and included in uninstall. Source exports remain local and untracked.

**Still open:** REQ-5 belongs to Fabric/Observatory/session hosts: actual stop/cancel
registration, notification delivery, resume from checkpoints and restoring unfinished
children. Producer tests do not establish this integration, live account acceptance,
Windows acceptance, installation or release. Ordinary Claude credentials were not
changed for tests; no running agent was killed or restarted.

**Fabric:** the operator approved Switchboard enrollment; `fabric_whoami` resolves it.
The already-registered bundled MCP exposes the board after admission: `com.status`
was ready, `com.list` empty. SB-108/SB-114 were linked to the canonical rows via
`fabric_work_claim`. The tool catalogue offered no Project directory (the adapter reference says its contract is pending); recipient ids
for cross-project `com.submit` were not available yet. Do not substitute session messages
or foreign-repository edits for that channel. A later rejoin-only attempt failed with
`call budget exhausted before body admission`; no new binding was used to bypass it.

**Project enrollment:** the operator separately asked to add all PassionCode projects
and skills/contracts. GitHub's live repository list contained 18 non-archived PassionCode
repositories, matching org-index. Switchboard was already present. Fabric's bulk importer
was instructed to add the other 17 canonical clones, `sshlg-skills` and its 11 canonical
member repositories (29 selected); worktrees/comparison clones were excluded. The native
consent dialog for Codex access to the `fabric` project appeared while import ran. It was
left unanswered: opening a project is distinct from granting an agent one-year access.
A later screenshot showed the consent dialog gone, added project entries (including
Fabric, Adapter, Contract and Dashboards), and the neighboring SB-115 task. UI actions
then failed with `noWindowsAvailable`; all 29 additions remain **submitted, not fully
verified**. No access consent was clicked by this run. The registered MCP reconnected
and `fabric_whoami` again resolved Switchboard.

**Checks:** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 ./scripts/check.sh`
exited 0 on the final 0.6.26 source: 561 Rust tests passed, three explicit native
checks ignored; UI, clippy, brand, error vocabulary, plugin, Python packaging,
documentation and notices checks passed. Release preflight for `v0.6.26`, publish
true and Windows signing true passed. The fixtures cover restart, duplicate events,
simultaneous owner claims, changed project permissions, duplicate identities,
stale/hidden quotas, cancellation, oversized updates and malformed data. `target/`
was below the 10 GiB cap. No full hosted suite was manually dispatched; the release
guard owns the exact-tag nightly run. Git and release receipts follow below.

**Git receipt:** [PR #181](https://github.com/passioncode-ai/fabric-switchboard/pull/181)
merged as [`b58477b`](https://github.com/passioncode-ai/fabric-switchboard/commit/b58477b1cc0108047367882cd7ee1e277d3c39bf).
The pushed implementation branch resolved to `37ad36d64cb35b1603f971f232352934aa792fba`;
a fresh remote clone of that commit passed docs, plugin and release-preflight checks.
The merge tree matched the locally checked tree. The annotated `v0.6.26` tag names
`b58477b1cc0108047367882cd7ee1e277d3c39bf`.

**Release:** [0.6.26 run 38331819706](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38331819706)
finished `failure` without publishing. Its [exact-tag nightly](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38331992391)
failed `handovers::tests::two_consoles_cannot_take_the_same_handover` on Windows x64
(job 115083614169) and arm64 (job 115083614116): two successful takes, expected one.
The release guard must remain closed. The original tag is retained unchanged.

**Correction published, 0.6.27:** exclusive private-file creation replaces rename
as the hand-over claim. A permanent `.taken` receipt prevents republishing, hides a
pending record left by a crash, and remains subject to the bounded queue. The existing
concurrent-taker test and new crash-after-claim test cover this. The final full local gate exited 0: 562 Rust tests passed, three native checks ignored,
plus clippy, UI, docs, plugin, packaging and notices checks. The plugin-layout check
now excludes isolated `.worktrees/`; its regression fixture passed. Exact-tag nightly
[38333315215](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38333315215)
passed all five platform jobs. [Release 38333144066](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38333144066)
finished success on attempt 2 and published [0.6.27](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.27)
at 2026-10-13T02:36:01Z: 25 assets, seven updater targets, five matching signature files
and five clean-source receipts for `61a2348ae2441af8d1d2403b955c0c574988fde0`.
Attempt 1's x64 signing-module cache failure and its bounded retry are recorded in the
[packet](packets/sb-114-capacity-recovery.md#delivery-receipt--0627). PR #183 (`065afb0`)
disables that cache for future tags. Neither original tag was rewritten.

**Local acceptance:** after publication, installed bundle and bundled CLI both read
0.6.25. The last observed running window showed 0.6.20. Native UI later returned
`cgWindowNotFound`; 0.6.27 installed/running acceptance remains open. Local Lifecycle
reports Switchboard ready/authenticated but started outside the broker, and Fabric
ready with zero windows. Neither was manually restarted. The existing enabled app
updater and idle watcher own activation. The packet retains the 29 import selections,
partial readback and the remaining board-channel limitation.

**Parallel owner:** the operator supplied the neighboring session's hybrid no-wait
plan; its checkout is `feat/sb-115-limit-stop-resume`. That session owns SB-115 and
shared Fabric/orchestration rule and skill updates. SB-114 readiness is not a global
prohibition on an independently authorized host retry. See the packet's operator
coordination update. No neighbor implementation or publication is claimed here.

**Exact next task:** connect the packet's REQ-5 through the owning Projects on Fabric's
board, coordinated with SB-115; FD-62 can consume the released 0.6.27 producer. Obtain
actual Project ids through the supported interface and retain message receipts.
Complete installed-version and import readback when native UI interaction works again;
do not approve new agent access or bypass lifecycle ownership as the agent.
Keep SB-114 and overall SB-108 open until actual host session evidence exists.

---

## 2026-10-12 — 0.6.22: hand-overs to saved accounts were refused by Observatory (SB-113)

The live Kimi rehearsal on the installed 0.6.20 (`wf_5473b99f80221c4f`, Kimi account
`3e7b8cf7…`) was refused by the engine before the offer: "to.accountRef looks like a credential
(uuid)". Every saved account id is a UUID, so every hand-over to a Claude, Codex or Kimi account was
refused in 0.6.20–0.6.21 — the Hermes rehearsal passed only because its reference is `hermes`. Fixed:
`continuation::account_ref` sends `sb-` + 12 hex of SHA-256 of the id; `names_account` reads both
forms. Verified against the real engine (`handoff:a6f7171bfcbcbd75`, which lapses on its own at
21:39 UTC) and its own `credential_shape` (2000 random handles). The installed app restarted at
19:56 UTC into 0.6.20.

0.6.22 (tag `v0.6.22`, run 38305858150) was refused by the guard: the Windows nightly could not
compile the tests — inserting `an_account_reaches_the_engine_as_a_handle_never_as_its_uuid` above
a Unix-only test took its `#[cfg(unix)]`. Fixed in 0.6.23; a scan of every test's `cfg` against
v0.6.18 found no other move. The nightly is dispatched once on the branch before tagging.

0.6.23 (tag `v0.6.23`, run 38309519776) was refused too: its nightly 38309742988 failed on Windows
arm64 in `external::tests::the_native_config_stamp_ignores_unrelated_rewrites` (green on the same
code an hour earlier) — two same-size configs written within one ~15.6 ms clock tick share a file
stamp, so the cached digest of the previous one came back; the test now waits 50 ms before each
write it reads back. Released as 0.6.24. Disk was down to 0 at 22:05 machine-wide (other
projects); this checkout's `target/` and scratchpad were cleared (~2.9 GB). The Kimi rehearsal
workflow `wf_5473b99f80221c4f` was closed by its creator.

**Published:** v0.6.24, release run 38313811475, 2026-10-12T22:34:29Z, 25 files, first attempt;
`latest.json` → 0.6.24.

**Exact next task:** when the installed app runs 0.6.24 (it runs 0.6.20 since 19:56 UTC; its next check is about 6 h after 19:58 UTC)
(it checks every 6 h, then restarts at an idle moment, or *Restart to update*), rerun the Kimi
rehearsal with a new workflow (`switchboard continue <wf> --account <kimi id>`) and a Claude
account hand-over, and record both; then SB-108.

---

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
