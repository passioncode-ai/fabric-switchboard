# Changelog

Release notes for Fabric Switchboard. The release workflow publishes the `## X.Y.Z` section of
this file as the notes of release `vX.Y.Z` ([DISTRIBUTION.md](docs/DISTRIBUTION.md)); the
release pull request renames `Unreleased` to the version. While Windows signing is switched off,
that section must say `windows_authenticode: NOT_SIGNED`. Notes of 0.5.3-beta.1 and earlier are
on their [GitHub releases](https://github.com/passioncode-ai/fabric-switchboard/releases).

## Unreleased

**Work in a project's folders on the project's accounts** (SB-118). The ordinary `claude` is one
sign-in for every folder, so project accounts used to serve only sessions Switchboard launched.
Now Switchboard writes the project's route into each project folder's Claude Code settings
(`.claude/settings.local.json`, kept out of Git), with a capability that opens that project's
pool only. Every Claude Code session started there, by you or an agent, spends the project's
accounts and keeps all your own settings, skills and plugins. Leaving a project, or
uninstalling, removes only what Switchboard wrote. `switchboard project bindings` lists the
folders.

**A project offers its new folders, and has no folder limit** (SB-117). When an agent creates a
worktree of a project repository, clones one again, or clones a new repository of the project's
source (`switchboard project source --add github.com/<owner>`), Switchboard finds it. It finds
the folder by itself in the background or when a Claude Code session starts there, and asks:
- a notification;
- a count next to the menu-bar icon until you answer;
- a banner in the window with **Add** and **Not this one**.

Nothing joins without a click, and a declined folder is never offered again. A project no
longer has a folder limit; all projects together hold up to 1024 folders.

## 0.6.29 — 2026-10-13

**A project holds up to 64 folders** (was 16). A product often lives in many clones and
worktrees side by side: the PassionCode project on the operator's Mac spans 35. All projects
together keep the old ceiling of 1024 folders, so the saved-data bound is unchanged.

## 0.6.28 — 2026-10-13

**A limit stop is not a wait** (SB-115). A session stopped on a usage or session limit is
ready for an immediate restart when a compatible account has room. Without one, it is
ready as a probe after 2, 5, 10, then 15 minutes, never only at the provider's reset time.
Spend and billing limits are never probed. The plugin's new `StopFailure` hook registers
such a stop by itself. Switchboard still never re-prompts a session: the host or the
orchestrator does, as fabric-workspace principle 10 now requires of every agent.

## 0.6.27 — 2026-10-13

**Prevent duplicate console hand-overs on Windows.** An exclusive claim receipt now ensures
that only one console can start each hand-over. The receipt survives a crash and prevents
the same hand-over from being offered again. The setting remains off by default.

This includes the capacity-recovery and console-queue features described below for 0.6.26.
The 0.6.26 release was stopped by its mandatory Windows checks and was not published.

## 0.6.26 — 2026-10-13

**Prepare stopped sessions for recovery** (SB-114). A host can register a session stopped by
an account limit, see counts and compatible accounts, and claim its continuation once.
Readiness requires fresh quota after a hold ends, including hidden limits; project and pool
restrictions still apply. Fabric/Observatory must connect their session lifecycle before this
resumes sessions automatically.

**Queue automatic hand-overs for an embedded console** (SB-108 producer). With
`switchboard handovers set on`, a console can take a prepared hand-over once. Unclaimed live
offers fall back to Terminal after two minutes; expired offers never launch. The setting is
off by default, pending the host integration.

## 0.6.25 — 2026-10-13

**Continue a workflow inside another app's console** (SB-108, first half). `switchboard continue
--in-place` runs the continuing session in the terminal that called it — an embedded console such
as Fabric Dashboards' — instead of opening a new Terminal window; Claude Code, Codex, Kimi Code and
Hermes alike. Automatic hand-overs still open Terminal until Fabric can take them.

## 0.6.24 — 2026-10-12

0.6.22 and 0.6.23 were tagged but refused by the release guard and never published, each for a
test, not the product: 0.6.22's Windows test build failed (a Unix-only test lost its `#[cfg(unix)]`
when a test was added above it), and 0.6.23's Windows on ARM run read a config file's stamp twice
within one clock tick in a timing-sensitive test. This release carries 0.6.22.

**Hand-overs to saved accounts work again** (found by the live Kimi rehearsal). Project Observatory
refuses an account id that looks like a key, and Switchboard's account ids are UUIDs, which it
counts as one — so since the engine started checking, every hand-over to a saved Claude, Codex or
Kimi Code account was refused before anything started, by hand and in a fallback chain alike;
only Hermes went through. Switchboard now names an account to Observatory by a short handle derived
from its id, and still recognises workflows that named it the old way.

## 0.6.21 — 2026-10-12

A second review of the hand-overs to Kimi Code and Hermes, finished.

**Hand-overs to Hermes, finished** (second review, SB-107). A Hermes that does not answer can no
longer hold Switchboard up: its model is read within five seconds, after the quick checks, and
whatever it started is stopped with it. Hermes hand-overs appear under Activity → *Sessions* as
*Hermes Agent* with their model, and their folders go a week after they end. Asking for
`--mode managed` with Hermes or Kimi Code says it does not apply instead of ignoring it. The next
account near a limit is checked every minute, not every 90 seconds, and the Hermes settings file
stays readable whatever characters a path holds.

## 0.6.20 — 2026-10-12

Work keeps going when the accounts run out: a fallback chain now reaches Kimi Code and Hermes, the
account in use is checked more often near its limit and the next one is ready, and a waiting update
no longer waits forever. 0.6.19 was tagged but never published — the release guard refused it
because a test assumed Kimi Code was installed on the build machine (fixed below) — so this release
carries 0.6.19's changes too.

**Hermes keeps your model** (SB-107, operator feedback on the live rehearsal). A hand-over to
Hermes runs on the model your own Hermes is set to on OpenRouter (`switchboard agents hermes
status`), not on the key's default; only a Hermes set to another provider falls back to the key's
model. The handoff prompt now names the session's owner and a session id Observatory accepts, so
Hermes no longer has to work them out before accepting.

**Hermes takes over when the subscriptions run out** (SB-107). A fallback chain such as
`claude-code → kimi-code → hermes` now ends on Hermes Agent: when no Claude or Kimi account is
left, Hermes picks the workflow up on the OpenRouter key saved under Agents — on the model your
Hermes uses on OpenRouter, else the key's default model — inside a home Switchboard keeps for it — the handoff is its first turn in an ordinary interactive
session, and it reaches Project Observatory to accept it. Spending stops at the key's own
OpenRouter limit. By hand: `switchboard continue <workflow> --account hermes --dir <checkout>`.

**A chain never stalls on a broken step** (found by review). A Kimi Code step whose `kimi` is
missing or whose `mcp.json` cannot be read is now passed over before anything is offered, so the
next step runs instead of the same offer lapsing every quarter of an hour; a Kimi first turn that
fails no longer opens the folder's previous Kimi session; the 30-second check near a limit now
really runs every 30 seconds; choosing the account checked next reads no credentials; on Windows a
newer release found while a restart is already under way waits for the next start.

**Fresher figures near a limit** (SB-106). The account a session works on is checked every minute
from 95 % of its usage and every 30 seconds from 98 %, and the account a switch would move to is
checked every minute meanwhile, so the switch at the limit acts on current figures. Other
accounts keep their three- and ten-minute schedules, and an account whose usage checks the
provider refused within the hour is not checked faster. A limit the usage figures did not show now
reads *Hidden personal limit* on the account.

**A waiting update never waits forever** (SB-105). While an update waits to start, Switchboard
keeps checking every six hours, and a newer release takes the waiting one's place — before, a copy
that never found an idle moment stayed on the waiting version and never saw a later release. The
log now says why a waiting update has not started (`update_waiting`: the window is open, a request
or a sign-in is running, updates are off), at once and then at most hourly. *Restart to update*
waits for an install in progress instead of quitting in the middle of it.

**Kimi Code in fallback chains** (SB-99). A chain step `kimi-code` now takes a workflow on a
saved Kimi Code account — the pinned one, else the least used that is signed in, not limited and
not busy: Terminal runs the first turn with `kimi -p` (Kimi asks for no approvals in that mode)
and then resumes the same session interactively with `kimi -c`. A workflow Kimi Code runs on a
saved account is handed on when that account reaches its limit: while a chain is set, Switchboard
checks each saved Kimi account's usage every ten minutes.

**Kimi sign-ins without loose ends** (SB-99). At most four Kimi Code sign-ins wait at once, as
for Claude and Codex, and a start removes the sign-in folders whose `kimi login` ended without
signing in; a completed sign-in stays until it is saved.

## 0.6.18 — 2026-10-12

The first published release since 0.6.16: 0.6.17 was tagged but never published — the release
guard refused it because the nightly checks had failed on its commit (a Windows test and a Linux
lint, fixed below) — so this release carries 0.6.17's changes too, listed after this release's own.

**Failover never makes things worse** (SB-102). A session moved off a limited account goes only
to an account of the same kind — a subscription never onto a paid key, a key shared with agents
never onto a subscription — and only while the pool still points at the refused account, so
parallel requests and the person's own choice are never overridden. A refused account is never
picked again in the same moment; an account set aside after a failed switch is never treated as
limited once it is in use; a model's weekly limit no longer holds the whole account; automatic
hand-over never picks another project's account.

**Sign-ins and sessions tie up their loose ends** (SB-103). A sign-in whose account was saved is
never offered again after a restart; a sign-in that has just begun is never swept away as a
leftover; the Sessions list names Kimi accounts and never stalls other requests; a Kimi re-sign-in
no longer shows the previous session, one Kimi account that cannot be read no longer blanks the
others, and `switchboard onboard --status` finds the home on Windows.

**The window keeps its word** (SB-104). *Check for updates* runs in the background and says how it
went, also with automatic updates off; a menu closes before its action runs; on Linux the window
names the Secret Service keyring and the notification area; when limits and sign-in states cannot
be read the window says so instead of showing every account as available; a backup made on
another computer is listed but not offered to restore; the Kimi *Add* dialog can start again after
a sign-in ended and retry a save that failed; the Sessions list refreshes when Activity opens.
Docs: the ten read tools, Kimi sessions, a limit's `reason` and `hidden`, Linux CLI paths.

**Nightly on Linux and Windows** (#156): Claude Code on Windows keys a folder with forward
slashes — onboarding finds the registration either way.

**Agents keep working when an account hits a limit** (SB-98). A managed session's request the
provider refuses for a limit goes once to the pool's next account, so the agent does not pause;
the refused account is held at once until its reset. The automatic switch of the ordinary Claude
Code no longer waits for fresh figures when the account in use is limited, and tries the next
account when one fails to activate. Limits the provider's usage figures do not show — spend and
credit caps — appear on the account as a hidden limit, with the reason in words.

**Switching is steadier** (SB-101). A switch no longer overwrites what Claude Code wrote into its
config in the same moment; waiting for Claude Code's lock no longer slows other requests; a
managed session whose selected account's token expired moves to the next account instead of
failing. A renewed Claude sign-in is saved at once, so quitting at the wrong moment no longer
signs the account out.

**Kimi Code accounts are safer and say when they are spent** (SB-99). A second `kimi`, a sign-in
again or a removal on an account whose session still runs is refused instead of sharing or
deleting its login; Kimi sessions appear in Sessions; a plan window at 100 % marks the account
as limited until its reset, and a window past its reset no longer shows its old share.

**Kimi Code says exactly what it knows** (SB-100). Each answer from Kimi reads as what it is —
no usage service, too many requests, an unreadable answer, no figures — instead of one “did not
answer”; figures show how old they are and refresh in the background; a failed refresh is said
while the last figures stay; several accounts are read at once.

**A coding agent can set Switchboard up** (SB-93). `switchboard onboard` prints the guide an agent
follows; `switchboard onboard --status` (and the MCP tool `switchboard_onboarding`) says where
setup stands — the app running, accounts saved, the folder's project, the MCP server registered
with the agent, a first answered tool call — with the command for each step the agent can take
and what to ask the person for the rest (opening the app, signing in). It only reads.
In the window, Agents → *Set up with your coding agent* gives the one-line prompt to copy into
Claude Code or Codex and shows whether each agent has Switchboard's tools registered and whether an
agent has called them; the tour's last step points at it.

**See which sessions run** (SB-95). `switchboard sessions` and the MCP tool `switchboard_sessions`
list the Claude Code and Codex sessions Switchboard launched — account, provider, pool, mode,
folder, when it started — and whether each is starting, running or ended, so another product or
agent can read them; Activity stays the journal. In the window, Activity → *Sessions* shows the same
list above the journal.

**A sign-in open when Switchboard quit or restarted is no longer lost** (SB-65). Each sign-in keeps a
small journal (provider, label, pool); the next start takes it back when Terminal still runs it
or it finished meanwhile, the banner returns, and the account is added as usual.

**A sign-in left open when Switchboard quit or crashed is cleaned up at the next start** (SB-87),
when its Terminal session has ended: its staging folder and, for Claude, its Keychain item —
the item only when deleting it shows no dialog. A sign-in still open in Terminal is left for a
later start.

**A Claude session continued from another agent starts again** (SB-94). Switchboard passed its
own MCP config as `--mcp-config <path>`, and Claude Code reads every plain value after that flag
as one more config file, so the first prompt of a continuation was taken for a file and the
session stopped with *MCP config file not found*. The config now goes as one argument,
`--mcp-config=<path>`. Fabric Dashboards' console hands its own MCP config, brief and first
prompt through `launch --in-place` the same way; a test runs that exact shape.

## 0.6.16 — 2026-10-10

**Windows installers and the Windows CLI are Authenticode-signed** (SB-03), x64 and arm64: signed in
CI through Azure Artifact Signing with the PassionCode.ai Public Trust profile, each signature
checked `Valid` before the release is published. SmartScreen no longer meets an unknown publisher.
Nothing else changes from 0.6.15.

## 0.6.15 — 2026-10-10

Fabric Switchboard on Linux (x64 and arm64) and on Windows on ARM, beside macOS (Apple silicon and
Intel) and Windows x64. Launching a session now picks its folder with the system picker and an
*Isolated session* box, a sign-in left waiting in Terminal ends with one Cancel, and the ordinary
Kimi Code sign-in on kimi.ai is read correctly.

**Windows:** windows_authenticode: NOT_SIGNED. Built natively for x64 and arm64; the installers are
not yet Authenticode-signed (the signing identity validation is in progress), so SmartScreen may warn.

### Added

- **Linux, x64 and arm64** (SB-88; operator decision 2026-10-09: every PassionCode.ai product runs
  on Apple silicon and Intel Macs, Windows and Linux). A .deb (app and `switchboard` on `PATH`), an
  AppImage that updates itself, and the CLI alone. Credentials live in the Secret Service (GNOME
  Keyring, KWallet, KeePassXC), never in a plain file; sessions open in your terminal app
  (`$TERMINAL`, then the common ones); the session scripts no longer need zsh. Tested in the release
  and nightly workflows against a real GNOME Keyring, with the app's smoke check under Xvfb.
- **Windows on ARM (arm64)**: a native installer and CLI beside the x64 ones (SB-88).

- Launch any account in a folder you pick (operator decision 2026-10-09, SB-86): the row menu's
  *Launch…* replaces *Launch isolated…* and *Launch managed…*; an *Isolated session* box chooses the
  mode, ticked and locked when only an isolated session is possible. Every folder Switchboard asks
  for — launches, Kimi Code, agents, projects, project rules — is chosen with the system folder
  picker, never typed; the picker opens at the last folder chosen, remembered across restarts.

- A provider terms notice (operator decision 2026-10-08, SCN-049): Accounts shows it from the first
  start, while the first account is added, until you dismiss it; About shows it always. It says to
  use Switchboard in line with the terms of Anthropic and OpenAI, that breaking a provider's terms
  can get the account blocked, and that you are responsible for how you use your accounts.

### Changed

- Kimi Code accounts use the same compact row as Claude Code and Codex: a small *Launch…* (or
  *Sign in*) and a ⋯ menu with *Sign in again* and *Remove…*; *Add Kimi Code account* and *Refresh*
  are small buttons (SB-86).
- A failed quota check logs the provider's HTTP status as a number (`usage_check` `http_error`
  `status`), so a refusal (403) and an outage (502) are told apart (SB-85).

### Fixed

- **A CLI or agent request to the running app could fail as "Control request did not complete"**
  with nothing sent (SB-90): the second request on the control connection went out before the
  connection was ready again. Seen on Linux in 12 of 44 requests; macOS's timing mostly hid it.
- Cancel ends a sign-in still waiting in Terminal instead of refusing until its window is closed;
  a refused Cancel no longer stops the banner from noticing Terminal, nor offers a *Retry* that
  meant finishing. The same for Kimi Code, whose sign-in now notices a closed Terminal (SB-83).
- The ordinary Kimi Code sign-in on kimi.ai read as signed out when an older kimi.com login or a
  stale `region` marker remained: Switchboard now reads the login slot `config.toml` names, as
  `kimi` does (SB-84).

## 0.6.14 — 2026-10-08

Kimi Code accounts, Hermes model settings and a shared OpenRouter key for supported agents.
This release also restores Claude account activation when Claude Code leaves only MCP sign-ins
in its Keychain entry, and explains when a running Switchboard needs a restart after an update.
It includes the changes prepared for 0.6.11, 0.6.12 and 0.6.13, which were superseded before publication.

**Windows:** windows_authenticode: NOT_SIGNED. Built natively; the installer is not yet
Authenticode-signed, so SmartScreen may warn.

### Added

- Kimi Code subscription accounts (SB-81): *Accounts → Kimi Code* signs an account in with the
  official `kimi login` (in Terminal), shows its membership tier and 5-hour, 7-day and monthly
  usage, launches `kimi` on it in a folder, and shows the ordinary `kimi`'s sign-in. Each login
  stays in its own folder; Switchboard never copies or renews it. CLI `switchboard kimi`, MCP
  `switchboard_kimi_accounts`.
- Hermes's model and provider on the Agents screen, changed on request through Hermes's own
  `hermes config set` (SB-80). CLI `switchboard agents hermes [set --provider --model]`, MCP
  `switchboard_hermes_status` / `switchboard_hermes_set`.
- One OpenRouter key for agents, saved once: `switchboard agents openrouter set --key-stdin` (the
  key is piped on stdin, never an argument) keeps it in the OS vault; `status` shows what the key
  may still spend, asked of OpenRouter itself; `model` changes the model agents start on;
  `remove` deletes it. Agents read it with `switchboard agents key --service openrouter`, and an
  agent over MCP sees the status and sets the model (`switchboard_openrouter_status`,
  `switchboard_openrouter_model`) — never the key. Encrypted backups carry it, so it returns
  after a reinstall. First slice of SB-79.
- Agents launch on the OpenRouter key (SB-79b): `switchboard agents launch <agent> --openrouter
  [--model <id>]`, and in the app *Agents → OpenRouter key for agents* (add, replace, change the
  model, remove; the balance OpenRouter reports) with *Launch on OpenRouter* in each agent's
  setup. Ten agents: Hermes, Kilo, omp, pi, Kimi Code, Aider, OpenCode, Goose, Qwen Code, Crush.
  The key reaches the agent only through its environment, read by the session script when it
  starts.

### Fixes

- Switching to a saved Claude account works when Claude Code has left only shared MCP or
  plugin sign-ins in its Keychain entry. Those sign-ins are preserved. An unrecognized or
  malformed entry is still refused without changing it.
- After an update is installed while an earlier Switchboard still runs, a `switchboard` command it
  does not know (for example `launch --in-place`) now says to restart Switchboard to finish the
  update, instead of “Control request refused”. The running Switchboard names its version to the
  command, and every refused control request is logged with its reason.
- `switchboard uninstall` also removes the Kimi Code login folders (`kimi`).
- `switchboard --data-dir <folder> backup list` no longer lists the real backups, and
  `backup restore` there refuses instead of loading the real accounts into the scratch store
  (SB-61).
- Every `switchboard` command and argument explains itself in `--help`, and
  `switchboard accounts update` takes `--label` or `--enabled` alone; the other keeps its value
  (SB-68).
- A busy machine's `~/.claude.json` over 1 MiB no longer silently stops Switchboard from
  noticing a new Claude Code sign-in, and a file caught half-written is not taken for a changed
  sign-in (SB-78).
- On Windows, a quit that starts the update installer drains for 5 seconds instead of 8, so the
  installer always has time to start (SB-78).
- macOS: a failed install at *Restart to update* says the administrator password was refused
  only when one was asked for (SB-78).
- Signing in again to an account saved in several pools renews every copy in one step: if a
  credential cannot be written, all copies keep the previous sign-in instead of some moving on
  (SB-78).
- Deleting a project and removing a project rule now ask first, like removing an account; a
  rule's *Pause* stays the one-step choice (SB-69).
- The rule dialog no longer offers “Claude Code login” for an account reserved by a project,
  which was refused after saving (SB-69).
- A failed read of the Projects or Activity screen is titled for that screen, not “Unable to
  load accounts” (SB-69).

### Build maintenance

- Native config-reader tests create user-owned files on Windows, including elevated CI, while retaining the strict production ownership check.
- The OpenRouter CLI test provides its own synthetic Hermes executable, so a clean Windows build no longer depends on an installed third-party agent.
- Update the build-time `source-map-js` dependency to 1.2.2, fixing GHSA-68fv-2mgg-jv7q.

## 0.6.10 — 2026-10-07

Switchboard speaks Russian, updates behave like every PassionCode.ai product, and the fixes of
the 2026-10-07 audit. 0.6.9 was never published — its Windows build failed — so this release also
carries 0.6.9's changes (below). If you run 0.6.1 or later, this version arrives on its own.
Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Switchboard in Russian

- The whole interface is available in Russian: every screen, dialog, notice, error the window
  shows, the first-run tour and the menu-bar (Windows: notification-area) menu. Switchboard follows
  the language of the operating system; About → *Language* overrides it (Same as system, English,
  Русский) and reloads the window. Dates and times follow the language. Terms follow the
  PassionCode.ai glossary (fabric-workspace knowledge/localization.md, track RM-25).
- The journal names its events in the interface language (“Account added · done”) instead of the
  recorded codes.

### Updates behave like every PassionCode.ai product (LC-16)

- A ready update now starts on its own at an idle moment — window hidden, no managed request
  running, no sign-in waiting — instead of only at the next start or *Restart to update*. Nothing
  running in Terminal is stopped.
- About → *Check for updates* checks at once, also with automatic updates off.
- A release marked as needing a manual step is held, with the reason shown.
- The log records downloads (`update_download`) and why a copy never checks.

### Fixes

- “Turn on for Claude Code” reports the threshold and headroom it actually saved, not always
  90 % / 10 points (SB-67).
- API-key and setup-token rows say “Not checked automatically · Quota checks need OAuth” instead of
  leaving the second line empty (SB-67).
- *Try again* after a sign-in no longer starts a new sign-in when cancelling the old one failed and
  left its staging folder behind; the error is shown instead (SB-67).
- On Windows the app no longer says “Mac”, “menu-bar icon” or “Keychain”: it names the
  notification area and Windows' own key protection (SB-66).
- The Windows build compiles its tests again: a test added in 0.6.9 lost its macOS-only mark,
  which stopped the 0.6.9 Windows release job.
- The automatic hand-over (SB-73) no longer hands work to another saved copy of the same spent
  login, never blames the ordinary CLI's limit on a workflow whose executor is an account
  Switchboard does not hold or may be a session Switchboard launched, waits five minutes after a
  scan that found nothing instead of starting Project Observatory every minute, gives the engine
  15 seconds off the monitor's thread, and kills everything the engine started when it hangs.
- Windows: turning automatic updates off discards an installer already waiting for the quit, and
  an ordinary quit installs nothing while updates are off.
- An agent that is not installed ("Aider is not installed. Install it first…") is named in the
  error instead of a generic message, in both languages.
- A session that could not start in place releases its account at once instead of blocking it for
  ten minutes.
- A macOS update install that stalls says to reopen Switchboard instead of promising a retry that
  would not come.
- Russian: reset times, the paused countdown, the native app's start-up timeout and the estimate
  check read correctly in Russian.
- Interface copy matches the brand pack: “Automatic switching” everywhere (no “rotation”), “Switch”
  instead of “Native Claude activation”, “Terminal launch requested” rather than “launched”,
  “PassionCode.ai”, and the tour's first two steps describe what the buttons do (SB-66).

## 0.6.9 — 2026-10-06 (not published; shipped in 0.6.10)

A Switchboard session can now run inside another app's console. If you run 0.6.1 or later, this
version arrives on its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Sessions in another app's console

- `switchboard launch --in-place` runs a session in the terminal it was started from, instead of
  opening a Terminal window — the same account, private folder, tools and protections — so an app
  with its own console (Fabric Dashboards) can host it. `--provider claude|codex` picks the account
  Switchboard would use for the folder; arguments after `--` reach the agent (for example
  `-- --resume <id>`).

## 0.6.8 — 2026-10-06

You choose which agents take over a task when an account runs out, and Switchboard follows that
order on its own. Nothing changes until you set a chain. If you run 0.6.1 or later, this version
arrives on its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Fallback chains

- `switchboard chain` and the MCP tools `switchboard_chain_get` / `switchboard_chain_set` keep your
  order of agents for continuing work when an account runs out — for the whole machine, a project
  or a single task; the narrowest one applies, and none applies until you set one. A ready-made
  order: Claude Code → Codex → Kimi Code → Hermes. An agent can be pinned to an account or to a
  paid key named in Project Observatory's vault; other agents never get a Claude or ChatGPT
  subscription.
- **When an agent's account runs out and a chain is set, its work moves on by itself.** For a task
  recorded as a Project Observatory workflow, Switchboard hands it to the next agent of the chain
  that has an account with quota — Claude Code or Codex for now — and opens that session in the
  task's folder, starting from the last checkpoint. Without a chain nothing happens. Kimi Code and
  Hermes in a chain are skipped until paid-key ceilings are in place.

## 0.6.7 — 2026-10-06

An agent task that ran out of its limit in Claude Code can now continue in Codex, and back, from
the same point. If you run 0.6.1 or later, this version arrives on its own. Signed by the same
team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Continuing work on another agent

- `switchboard continue` hands a workflow from Claude Code to Codex, or back, with its context:
  the new session starts from the same checkpoint and constraints, told that the work comes from
  another agent. Before, it accepted only an account of the same provider. A workflow left by an
  agent Switchboard does not run itself (Hermes, Kimi Code) can be taken over the same way.

## 0.6.6 — 2026-10-06

Signing in again to an account you already saved no longer renames it or copies it into another
pool. If you run 0.6.1 or later, this version arrives on its own. Signed by the same team
(`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Signing in

- **Try again in the sign-in banner repeats the same sign-in.** Before, it lost the account's name
  and pool and started over in the default pool: finishing it renamed a saved account to its email,
  or added a second copy of an account saved in another pool — including an account reserved for a
  project, which then showed up for switching Claude Code.
- **Signing in to an account you already saved updates it where it is**, from its row, from
  **+ Add account** or from Try again: its name and pool stay, every saved copy gets the new
  sign-in, and the notice says it is signed in again. A new account is added only when it is not
  saved anywhere yet.

### Under the hood

- The local proxy checks an agent's key in constant time, so response timing cannot reveal which
  key matched.
- The MCP tool descriptions for `switchboard_usage` and `switchboard_project_apply` now describe
  what the tools return.

## 0.6.5 — 2026-10-06

The tag `v0.6.4` was never published; 0.6.5 carries its changes (below) and this fix. If you run
0.6.1 or 0.6.2, this version arrives on its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Switching accounts

- **Switch works again when Claude Code's sign-in and settings disagree.** After an interrupted
  switch or a half-finished `/login`, Claude Code's settings can name one account while it is
  signed in to another one you saved in Switchboard. Every switch then stopped with *The Claude
  Code sign-in does not match the account named in its settings*. Now Switchboard asks Anthropic
  whose sign-in it is, keeps it under that account and switches.
- Switchboard trusts Anthropic's answer over its own saved copies. A copy saved under the wrong
  account no longer blocks a switch, and it can no longer overwrite another account's sign-in:
  when Anthropic cannot be asked, the switch stops with *Switchboard could not confirm which
  account Claude Code is signed in to* and changes nothing.

### Agents and MCP

- `switchboard mcp --read-only` no longer asks the provider when an agent sets `refresh: true`
  on `switchboard_usage`: it answers with the stored observation and says so. Before, a
  read-only server could still run a quota check, store its answer and renew a sign-in.
- The plugin's tool reference now gives the real freshness limit without a rotation policy:
  900 seconds, not 300.

### Diagnostics

- The log (`~/Library/Logs/Fabric Switchboard/switchboard.log`) records why a background quota
  check failed, as a short code such as `unreachable` or `rejected`. It never records the
  provider's reply or a token.

### Agent catalog

- Catalog notes for ZCode, Kimi Code, Hermes and OpenClaw were checked against their sources:
  ZCode gained a ready config snippet, and OpenClaw can read the key from an environment
  variable.

## 0.6.4 — 2026-10-05

The tag `v0.6.3` was never published: a test of this change failed on Windows before the
release step, and 0.6.4 carries the same change with the test fixed.

Switchboard no longer re-reads your Claude Code sign-in every time Claude Code saves its settings,
which it does several times a minute when many sessions are open. If you run 0.6.1 or 0.6.2, this
version arrives on its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Quieter in the background

- Switchboard watches only the account part of Claude Code's `~/.claude.json`. Before, any change
  to that file made it read your sign-in again, and a read that overlapped Claude Code's own
  write was briefly reported as refused.

### License

- About → Version and license now points a commercial license request to the form at
  https://passioncode.ai/business/ instead of an email address.

## 0.6.2 — 2026-10-05

Switchboard uses far less of your Mac in the background, and an agent that runs out of its limit
mid-task can be picked up by another of your accounts. If you run 0.6.1, this version arrives on
its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Lighter in the background

- Switchboard watches Claude Code's session logs for limit errors. It used to read the end of
  every active log twice a minute; now it reads only what each log added since. On a Mac with
  22 active sessions, one background pass went from 66 ms to about 1 ms.

### Continue a task on another account

- `switchboard continue <workflow> --account <id> --dir <checkout>` hands a Project Observatory
  workflow whose agent hit its limit to another account of the same provider, and opens that
  account's session in the checkout. The session takes the work over from the last checkpoint.
  Switchboard refuses before changing anything when the workflow cannot be continued here, and
  reports Observatory's own reason when it declines. macOS only for now.
  [CLI.md](docs/CLI.md).

### Usage analytics

- Each anonymous usage event also carries the installation id under a second name and whether
  the build is a release, so PassionCode's own reports count installs correctly. Nothing new
  identifies you. [ANALYTICS.md](docs/ANALYTICS.md).

## 0.6.1 — 2026-10-05

Switchboard now updates itself: from this version on, new releases arrive and install without
you doing anything (turn it off in About). It also works with the popular coding agents beyond
Claude Code and Codex, and a reinstall gets everything back. Install this one by hand; every
later release arrives on its own. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Other agents

- Switchboard now works with the popular coding agents beyond Claude Code and Codex — Hermes,
  Kilo Code, Cline, Goose, OpenCode, Qwen Code, Aider and 23 more (Agents → Other agents,
  `switchboard agents`). Each gets Switchboard's tools; agents that accept a custom endpoint can
  send their requests through Switchboard, which switches accounts for them, and some launch from
  Switchboard directly. Subscription sign-ins stay with Claude Code and Codex, as the providers
  require; other agents use API-key accounts. The proxy now also speaks Chat Completions for
  OpenAI API keys. [AGENT-SUPPORT.md](docs/AGENT-SUPPORT.md).

### Backups and reinstall

- Backups now also keep your projects, project rules, which account each pool's managed sessions
  use, and the open-at-login and auto-update settings. A change to a project rule triggers a
  backup too.
- A restore now also gives back the credential of an account that is still listed but lost it
  (after `switchboard uninstall --keep-data` or a removed Keychain item).
- `switchboard uninstall` keeps the Claude Code and Codex history in Switchboard's session
  folders and removes only the sign-in files in them; `--purge` removes those folders too.
- After a reinstall or `switchboard uninstall`, the first start restores the newest backup on its
  own and says so; an empty account list with a backup beside it offers *Restore from backup*.

### Automatic updates

- Switchboard now updates itself. About 90 seconds after it starts, and every six hours, it
  checks for a new release, downloads it in the background, verifies its signature and installs
  it; the new version runs from the next start, or at once with *Restart to update* in the
  menu-bar menu or About. On by default; About → *Install updates automatically* turns it off.
- An update never touches your accounts, settings, connections or backups: on Windows the
  installer runs in update mode, which never uninstalls and never deletes the app data.
- This version is the first that can update itself; installing it is the last manual download.

## 0.6.0 — 2026-10-05

The first stable release. A project can now keep its own accounts: only sessions from its folders
use them, and no other project switches to them. A short tour on first start shows what
Switchboard does and where to press. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The installer is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Projects

- A project is one or more folders — related repositories — with accounts of its own. Create it
  under Projects → *New project*: its accounts serve only sessions launched from its folders,
  automatic switching stays inside them, and no other project — nor the ordinary Claude Code,
  which every folder shares — switches to them. Inside its folders, sessions use its accounts.
  Also `switchboard project save|delete`.

### First start

- A five-step tour on first start shows what Switchboard does and where to press; About →
  *Show the tour again*.

- Windows: the installer now puts `switchboard.exe` beside the desktop app, so the Agents panel
  can show its path and the commands that connect Claude Code and Codex to it.

## 0.5.5-beta.1 — 2026-10-05

Switchboard now stays running with its window closed and opens at login, its account list is
compact again with each account's state readable at a glance, idle accounts are checked less
often, and release builds count anonymous usage. Signed by the same team (`KJ35UYYL22`).

**Windows:** windows_authenticode: NOT_SIGNED. The archive is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Usage analytics

- Release builds count installs, days of use and connected accounts — numbers and kinds only,
  never account names, e-mail addresses, sign-ins or pool names — with PassionCode's own
  analytics server. A random installation number shared by PassionCode.ai tools on this computer
  lets one person using several of them count once. Turn it off in About → *Share anonymous
  usage counts*; that switch applies to every PassionCode.ai tool. What is sent:
  [ANALYTICS.md](docs/ANALYTICS.md).

### Stays running

- Closing the window no longer quits Switchboard: quota checks, automatic switching, sign-in
  renewal and backups keep running. A menu-bar icon (on Windows, in the notification area)
  opens the window again or quits Switchboard.
- Switchboard opens at login, in the background, without a window. Turn it off in About →
  *Open at login*. `switchboard uninstall` removes the login item.

### Quota checks

- Accounts nobody is using are checked every ten minutes instead of every three, and again just
  after one of their limits resets; the account in use, the one your managed sessions use and
  every account in a pool that switches automatically stay on three minutes. Without automatic
  switching, numbers now count as current for 15 minutes, so idle rows no longer read stale.

### Interface

- The account list is compact again: each row's quota fits in two lines — the meter with the
  share used and how long ago it was checked, then the countdown and date of the next reset (or
  when a limited account is back, or when the next check runs). A coloured mark on the provider
  icon shows the account's state at a glance: available, running low, limited, stale or failed.
  The limit details moved into the row's quota disclosure, which now lists each window on one line.

## 0.5.4-beta.3 — 2026-10-05

Switchboard now stays quiet while nothing changes, and it respects every limit the providers
report: no quota check before the time a provider asked for, no feature limit mistaken for the
whole account, no estimated hold shown as a reset. Signed by the same team (`KJ35UYYL22`) as
0.5.3, so the Keychain's trust carries over.
(The `v0.5.4-beta.1` and `v0.5.4-beta.2` tags never became releases: their runs stopped in the
Windows job — first on the encoding of these notes, then on a Windows build error — and both
are fixed here.)

**Windows:** windows_authenticode: NOT_SIGNED. The archive is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

### Quota, limits and accounts

- `switchboard uninstall` shows what Switchboard created on this Mac and, with `--yes`, removes
  it: saved accounts' credentials, the old unused vault key, the `~/.local/bin` link and the data
  folder's own files. Backups and the Claude Code and Codex sign-ins are never touched.
- Finishing a sign-in whose temporary folder could not be removed now reports the account as
  added (with a note) instead of an error, and finishing it again returns the same account; the
  folder is removed on the next try.
- An account held after a limit error shows whether the time is the provider's own reset
  (*Limit resets*) or an estimate (*Retry hold until*). Holds survive a restart, and a new
  Claude Code session's limit is charged to the account it runs on even when its reset time
  happens to match another account's.
- Codex accounts show the limits the Codex app itself reads beyond the two usage windows: a
  personal spend limit, a workspace out of credits or at its usage limit, and each feature's
  own limit. A limit on one feature no longer marks the whole account as used up; spend and
  workspace limits block it whatever the percentages say.
- When a provider answers a quota check with “too many requests”, nothing checks that account
  again before the time it asked for — not the background, not Check usage, not
  `switchboard usage` or MCP. Its `Retry-After` is read in seconds or as a date and kept whole
  (up to seven days; 15 minutes when it gives none). The wait survives a restart and ends at
  once with a new sign-in. A row whose check failed shows when the next check runs. A quota
  check no longer holds up other actions while the provider answers, and two checks of one
  account share one request.
- Account rows show fresh remaining quota first within each pool, followed by the shortest
  known wait. Reset dates include a countdown in days, hours and minutes; Pause countdown
  keeps the displayed duration still. Unknown or stale quota stays labelled.
- Partial quota responses no longer make unobserved windows appear fresh. Backups created
  in the same second keep separate copies, and session attribution reads timestamp metadata
  without materializing conversation content.

### Lifecycle

Version 0.5.4: the organization's [lifecycle contract](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/lifecycle.md)
applied to Switchboard ([PR #23](https://github.com/passioncode-ai/fabric-switchboard/pull/23);
[AGENTS.md → Lifecycle](AGENTS.md#lifecycle)).

- **No Keychain read on a timer.** The background looks at Claude Code's and Codex's sign-in
  sources without decrypting anything or starting a process — file stamps, the Keychain item's
  attributes and access list — and reads only when one of them changed. A locked keychain, a
  refusal or an item that would ask is remembered and not tried again until it changes or a
  person acts (a locked keychain is looked at again after 1, then up to 30 minutes), so no
  background loop can raise a Keychain dialog. Capture, a person's action, still reads directly.
- **Idle means idle.** One background pass every 30 s instead of 10 s; renewals are rescheduled
  only when credentials change; Claude Swap is found in the process table without running `ps`;
  a quota check that finds nothing new is not written (flushed within 15 minutes and at stop);
  the window polls a sign-in only while one waits.
- **Managed sessions survive a restart or an update.** The proxy's port and token are kept in
  `proxy.json` and reused; if another program took the port, the proxy moves, keeps the token
  and repoints the managed homes.
- **A clean stop.** Quit, the last window, `SIGTERM` and `SIGINT` (app and `switchboard serve`)
  run one drain with an 8 s deadline and a hard exit at 10 s; `control.json` names its owner's
  pid and is removed at stop, and one left by a dead owner counts as stale.
- **A bounded log** of codes and numbers only in `~/Library/Logs/Fabric Switchboard/`
  (5 × 5 MB, 0600).
- `/usr/bin/security` calls and the backup write no longer hold the async workers (SB-23).
- Local builds keep the current and the previous release in `artifacts/` and unregister removed
  app bundles.
- `--background` starts the app without showing its window or taking focus (and, on macOS,
  without a Dock icon), as the local lifecycle broker does for apps it keeps running; opening it
  again shows the window. Unknown launch arguments are ignored, and App Nap no longer slows the
  account service while no window is open.
- A `SIGTERM` or `SIGINT` that arrives while the app or `switchboard serve` is still starting
  now runs the normal drain instead of ending the process with its control file left behind.

## 0.5.3-beta.2 — 2026-10-04

The first release built, signed and published by GitHub Actions instead of a laptop, with the
Windows fixes its rehearsals found. The app is the same 0.5.3, signed by the same team
(`KJ35UYYL22`), so the Keychain's trust in it carries over.

**Windows:** windows_authenticode: NOT_SIGNED. The archive is built natively but not
Authenticode-signed until the organization's Azure signing account exists; SmartScreen may warn.

- **Windows: an elevated session's own files count as its own.** An elevated process owns new
  files as `BUILTIN\Administrators`, so storage it had just created read as foreign and was
  refused.
- **Windows: a stale control file falls back as it should.** Windows refuses a connection to a
  port nobody listens on only after about two seconds; the bound is now five, so a left-over
  `control.json` leads to the lock-guarded fallback instead of *Control connection did not
  complete*.
- **Windows builds are checked every night.** Native tests, the unsigned release build, and a
  clean-tree check after each. Text checks out as LF on every platform, so a Windows build no
  longer looks like a changed source tree.
- **Releases are built and signed only in GitHub Actions.** A `vX.Y.Z` tag starts
  `.github/workflows/release.yml` in the protected `release` environment; a member of
  `release-approvers` approves (whoever pushed the tag may). macOS: the
  universal app and CLI are signed with the organization's CI Developer ID, the app is notarized
  and stapled, and the standalone CLI is notarized on its own (a bare executable cannot be
  stapled; the receipt names both submissions). Windows: built natively on `windows-latest`, with Azure Artifact Signing
  ready behind `AZURE_SIGNING_ENABLED`, which is off for now. Every file is attested (Sigstore),
  summed in `SHA256SUMS` and signed with the organization's release key (`SHA256SUMS.asc`).
  Building and signing on a laptop remain for debugging and are never published.
- **No team id in the code.** The Keychain trust check reads the team only from
  `SWITCHBOARD_SIGNING_TEAM` at build time (the release environment's `APPLE_TEAM_ID`); a build
  without it is a development build ([KEYCHAIN.md](docs/KEYCHAIN.md)).
- The manual `build-windows.yml` workflow is replaced by the release workflow; a Windows build
  of a commit is a rehearsal on an `-rc` tag.
