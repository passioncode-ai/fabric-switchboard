# switchboard CLI

Reference for the `switchboard` CLI as of 0.6.82: the same account store, vault, request proxy and launcher the desktop app uses. The original [CLI release plan](CLI-RELEASE-PLAN.md) fixed the 0.2 behaviour; release records: [0.5.x–0.6.x](evidence/release-0.5.md), [0.2](evidence/release-0.2.md). This file is an operational contract, not a claim of real-provider acceptance.

## Install and first use

Use the `switchboard` executable beside the macOS app in the signed archive, or `switchboard.exe` from the Windows artifact. Building only the CLI from source:

```sh
cargo install --path crates/switchboard-cli --locked
switchboard --help
switchboard --json accounts list
```

Help/version do not open account storage. Default data root is shared with GUI: `~/Library/Application Support/ai.passioncode.fabric-switchboard` on macOS, `%LOCALAPPDATA%/ai.passioncode.fabric-switchboard` on Windows, `~/.local/share/ai.passioncode.fabric-switchboard` on Linux (`$XDG_DATA_HOME/ai.passioncode.fabric-switchboard` when `XDG_DATA_HOME` is set; `default_root` in `crates/switchboard-runtime/src/lib.rs`, `dirs::data_dir` everywhere but Windows). Encrypted backups sit beside it, never inside: `~/Library/Application Support/Fabric Switchboard Backups`, `%APPDATA%\Fabric Switchboard Backups`, `~/.local/share/Fabric Switchboard Backups` (`$XDG_DATA_HOME/Fabric Switchboard Backups` when set; `backup_dir` in the same file; `SWITCHBOARD_BACKUP_DIR` overrides it). `switchboard --help` and `switchboard backup --help` name the same folders.

Words (decision D-1 of the 0.6.82 iteration): help and output say **group** for what the commands, flags and JSON fields call a project, and **group id** for its `pool`; **automatic switching** for what they call rotation. Command names, flags and fields stay as they are. A command group run without its command (`switchboard accounts`) prints that group's help and exits 2; with `--json` it prints `{"ok": false, "error": "Name a command. Run switchboard accounts --help."}`. Empty cells of text output show `-`. `--data-dir ABSOLUTE_PATH` selects a deliberately separate metadata root. Credentials stay in the native vault by UUID; never mix metadata roots casually or treat this flag as account export.

A refusal prints the sentence the window shows for it (SB-189): what happened and what to do, in English, on stderr with exit 1. With `--json` it is `{"ok":false,"error":<that sentence>,"raw":<the backend's own text>}`, `raw` present only when the two differ; a script that matches refusals reads `raw`.

## Account management

```sh
switchboard accounts list
switchboard accounts add --provider claude --kind api-key --label Work --pool work --secret-stdin < /private/path/to/credential
# SB-401: a Claude API key lands in the pay-per-token group whatever --pool says (made when needed, switched off);
# `accounts move <key> --pool <other>` is refused: a Claude API key moves only into that group
# (`--pool tokens` names that group whatever its id, and makes it when missing); a key from before, outside it,
# is never selected or launched once an owner has started (it makes the group)
switchboard accounts select ACCOUNT_UUID --provider claude --pool work
switchboard accounts update ACCOUNT_UUID --label Work                  # or --enabled false, or both
switchboard accounts update ACCOUNT_UUID --elsewhere true              # used on another machine: never renewed, rotated to or switched to here (SB-175); false: used here again
switchboard accounts update ACCOUNT_UUID --location "Mac Studio, user alex"   # SB-284: says where it works (and marks it used elsewhere); "" clears it
switchboard accounts assign ACCOUNT_UUID --to "Frame Agent on Studio"   # SB-322: an agent signs in with it now: usage still checked, never selected or switched to by itself; clears --elsewhere
switchboard accounts unassign ACCOUNT_UUID                          # SB-322: take it back
switchboard accounts select ACCOUNT_UUID --provider claude --force   # SB-322: use an account given to an agent anyway (journaled); also `accounts activate --force`
switchboard accounts send ACCOUNT_UUID --to user@studio.local [--pool agents]     # SB-283: move the sign-in over ssh; no new sign-in there, undone on failure
switchboard targets [--probe]                            # SB-332: machines known from moves and remote runs; --probe asks each over ssh for its version
switchboard launch --target user@studio.local --provider claude --working-directory /Users/me/DATA/app --in-place   # SB-332: that machine's own launch, in this terminal over ssh (ssh -t); --headless for no terminal
switchboard accounts remote --on user@studio.local      # SB-327: that machine's accounts, groups and organizations over ssh (no credential); reads only
switchboard accounts check --on user@studio.local       # SB-327: findings against the moves recorded here (missing_there, off_there, refused_there, not_marked_here, held_twice, no_record)
# SB-323: the person's word on its organizations goes with it (org add by name there when missing, then org assign); answer.organizations says whether it was carried
switchboard accounts hand-over ACCOUNT_UUID --to "Anna, MacBook" > handover.json   # SB-283: signed record, no credential; marks it used elsewhere
switchboard accounts receive handover.json --signer KEY [--apply] [--headless]    # on the receiving machine: shows the plan; --apply starts the sign-in
switchboard accounts recall ACCOUNT_UUID --from member@vm   # SB-292: take a moved sign-in back; its copy there is removed and proved gone (Claude and Codex)
switchboard accounts send ACCOUNT_UUID --to user@studio.local --remote /usr/local/bin/switchboard --remote-data-dir /srv/switchboard   # SB-315: a Switchboard there that runs with --data-dir; a recall uses the same by default
switchboard accounts seats [--limit 50]                     # SB-312: the moves, hand-overs and call backs recorded here, with the id there
switchboard agents anthropic set team-a --key-stdin --model claude-sonnet-5-5   # SB-390: an Anthropic API key under a name (piped, never echoed); list | model | remove | key
switchboard agents launch hermes --anthropic personal --dir /abs/folder     # SB-390: Hermes on the Anthropic key saved as personal
switchboard agents launch-keys hermes --folder /abs/folder   # SB-392: the keys this agent can start on, by name: the folder's OpenRouter key first, then the others and the Anthropic keys
use_secret.py run --vault-only --env prod team-a ANTHROPIC_API_KEY -- sh -c 'printf %s "$ANTHROPIC_API_KEY" | switchboard agents anthropic set team-a --key-stdin'   # SB-392 K-0: save a key from Observatory without seeing it (an Admin key is refused)
switchboard keys observatory                          # SB-404 K1: the Anthropic, OpenAI and OpenRouter keys Project Observatory holds, by project/env/NAME, and where each is saved here; never a value
switchboard keys observatory add alpha-web/prod/ANTHROPIC_API_KEY [--to account|agent|both] [--as NAME] [--model M]   # SB-404 K2: an Anthropic key into the pay-per-token group (and/or a named key for Hermes); the value moves through a pipe
switchboard keys observatory add alpha-web/prod/OPENROUTER_API_KEY [--pool POOL] [--model M] [--replace]   # SB-404 K2: an OpenRouter key as this computer's (or a group's) agents' key; an OpenAI key is refused for now
switchboard agents launch hermes --pool tokens --model claude-sonnet-5-5 --dir /abs/folder   # SB-404 K3: Hermes on the pay-per-token group's selected key, through the proxy, counted against its daily ceiling
switchboard agents launch claude-code --pool tokens --model claude-sonnet-5-5 --dir /abs/folder   # SB-404 K3: Claude Code on the same key, managed, with its model
switchboard bugs                                       # SB-379 W3: crashes found at the next start (exception type and signal from macOS's report) and, with usage counts on, the week's error counts
switchboard attention                                  # SB-365: what needs a person (sign-in, refused renewal, ended seat, failing usage checks, a lost move, a ceiling question, a session copy, a stuck or gone board run), each with the tool that fixes it; the accounts stay in their groups
switchboard mcp-homes                                  # SB-359: which Claude home is signed in to which MCP server (names, URLs, booleans; never a token)
switchboard sessions --tree                            # SB-379: live sessions with who started them and for which task, the day's launches, counts, the watch's findings
switchboard sessions --rows [--target studio]           # SB-395: every session in four groups (needs you, running, waiting, ended) with the acts it has; here or there
switchboard sessions continue SESSION_ID              # SB-396: continue an ended session as itself (claude --resume in its own config folder)
switchboard sessions continue SESSION_ID --in-place [--hosted] [--surface recovery]  # SB-400: the same, in this terminal; --hosted: in a session host under a launch of its own, attached here
switchboard sessions restart plan                     # SB-397: which running sessions a restart wave restarts and skips, and its digest
switchboard sessions restart apply --digest D --reason T  # SB-397: restart them, each at the end of its turn, as itself; this session last; needs the owner running
switchboard sessions restart status [WAVE]            # SB-397: a wave's state per session
switchboard sessions restart cancel WAVE              # SB-397: sessions still waiting for their turn stay as they are
switchboard monitor hold --session ID --until UNIX [--reason fabric_claim]    # SB-399: the session holds live work (a Fabric task's claim) until then; 0 releases; the reason is one short word, echoed and logged as a code
switchboard sessions close-copy SESSION_ID [--pid N] [--target M]  # SB-388, SB-389 (--pid: the copy attention offers when both work) (--target: the copy on another machine, closed by its Switchboard): one session in two processes: close the copy quiet longest (SIGTERM, SIGKILL after 10 s); never a busy one or one running a command
switchboard duty [on | off | set … | run [--dry-run]]   # SB-384: the session duty officer: closes a finished session, restarts a stalled one, never one still working
switchboard capacity [set N | clear]                   # SB-378: this computer's cap on working coding sessions (quiet ones not counted); dispatch and launch --hosted wait at it
switchboard machines list [--live]                     # SB-355: each machine group with its accounts, and accounts used elsewhere in none; --live reads each machine over ssh (20 s per call)
switchboard machines adopt [--apply]                   # SB-355: place accounts already used elsewhere in their machines' groups (recorded move, else location); never guessed
switchboard machines place ACCOUNT_UUID --on studio-sshlg   # SB-355: record where it lives without ssh; it leaves every group here
switchboard machines forget studio-sshlg               # SB-355: remove a machine group that holds no account
switchboard machines keys                             # SB-357: the keys trusted for other machines (public keys only)
switchboard machines trust studio-sshlg --key KEY     # SB-357: trust the key `settings signer` printed on that machine
switchboard accounts recall ACCOUNT_UUID --pool agents   # SB-355: back from its machine group into a group here (default: the group it left); --from defaults to the recorded move's machine
switchboard accounts send ACCOUNT_UUID --to member@vm --pool acme --operator-said "…" --said-at … --said-in …   # SB-292: the grant is recorded in seats.jsonl with the operator's words
switchboard accounts move ACCOUNT_UUID --pool agents                  # a project's pool, or default (SB-122)
switchboard accounts remove ACCOUNT_UUID                # MCP switchboard_account_remove (SB-328); refused while its group has it selected
switchboard --json events
switchboard events --kind errors --account ACCOUNT_UUID --limit 50   # SB-373: one kind (switches, sign_ins, groups, moves, errors, other) and one account
switchboard events --before 1763100000                 # SB-373: older events; a page that leaves older ones prints the next --before on stderr
```

### Organizations (SB-158)

```sh
switchboard org add "Acme" --domain acme.example --domain acme.dev
switchboard org add "Beta"                             # SB-403: an organization needs no domain
switchboard org list                                   # organizations, then each account, why, and what its address suggests
switchboard org assign ACCOUNT_UUID ORGANIZATION        # an id or a name, or personal, or auto
switchboard org assign ACCOUNT_UUID auto Acme Beta      # SB-282: the rules decide its first; it also works for Acme and Beta
switchboard org assign ACCOUNT_UUID personal Acme      # SB-323: personal, and it also works for Acme (personal comes first or is refused)
switchboard org assign ACCOUNT_UUID --also Acme        # SB-403: Acme added to the organizations it works for; its own stays
switchboard org assign ACCOUNT_UUID Acme --team        # SB-403: the other seats of its Claude Team or ChatGPT workspace follow too
switchboard org edit ORGANIZATION_ID --name "Acme Inc." --domain acme.example
switchboard org remove ORGANIZATION_ID
```

An account belongs to an organization by your word (`assign`) or by its team at the provider
(`assign --team` on one seat of a Claude Team or ChatGPT workspace records the team, so its other
seats follow; without `--team` only that seat is placed and the answer says `team: not_claimed`
with the number of `teammates`); otherwise it is personal. An organization is a label, never a lock
(SB-403, [DEC-0022](DECISIONS.md#dec-0022-an-organization-is-a-label-never-a-lock-on-where-an-account-works)): no `assign`, move, share, sign-in or call back is refused because of
organizations, and every group serves the accounts it holds. The answer's `outside` (and
`warnings`) names the groups it serves whose organizations it does not hold, as information;
`project save`/`edit`, `accounts move` and `accounts recall` answer `warnings` the same way. An address on an organization's domain only suggests it
(SB-285, [DEC-0009](DECISIONS.md#dec-0009-an-e-mail-domain-suggests-an-organization-only-the-person-or-an-administrator-places-an-account)): `org list` shows `(its address suggests Acme)` beside such an account, and `assign`, a
sign-in into that organization's group, or moving the account into it confirms the suggestion.
Nothing changes until an organization exists. `--json` gives `{schema: 1, organizations, accounts,
passioncode}`; each account carries `organization`, `organization_name`, `by` (`person`,
`provider` or null), `also` (the organizations it also works for, SB-403), `suggested` and
`suggested_name` (null when nothing is suggested); an organization's `accounts` counts every
account that works for it, first or also. The text view joins an account's organizations with
` + `.

```sh
switchboard org connect        # SB-159: sign in to PassionCode.ai in the browser (waits up to 5 minutes)
switchboard org read           # read your PassionCode.ai organizations again now
switchboard org passioncode    # where the connection stands; no token is shown
switchboard org disconnect     # delete the token; the organizations stay as local ones
switchboard org cancel         # stop a connection still waiting for the browser
```

What About and the tray do, through the running app (SB-299): `switchboard app updates [on|off]`
(the update state, or *Install updates automatically*), `app check-updates`, `app restart-to-update`
(installs a downloaded update and restarts through the drain) and `app login-item [on|off]` (*Open at
login*). Anywhere the installed app does not run the actions and the switches answer that only it can;
`app updates` alone answers what it knows instead (SB-414): `{available: false, reason: not_owner|no_owner, owner:
serve|null, owner_version, owner_updates_copy, window_version, window_older, windows, serve_update?, message}`, where
`owner_version` is the `serve` that owns the data folder, `owner_updates_copy` whether it keeps its copy current (an
installed copy with automatic updates on whose update check succeeded within 15 minutes, or whose first check is still
due within 15 minutes of its start; never on Windows), `serve_update` its own update
loop's last pass (`{state, latest, reason, checked_at, auto_update}`) and `window_version` the oldest app window open
beside it that does not own the data folder (it says the same in Settings → Updates, and when it runs from the same
copy as a `serve` that keeps that copy current it leaves updating the copy to `serve`).

`switchboard link` puts this `switchboard` into `~/.local/bin`, as the window's *Link the CLI* does
(SB-299); run it by its full path, for example
`"/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard" link`.

`switchboard analytics on|off` sets Switchboard's own usage analytics as About does, and without an
argument reads the state (SB-299, [ANALYTICS](ANALYTICS.md)).

GitHub through your own `gh`, as the window's GitHub page (SB-299): `switchboard github owners`
lists you and your organizations, `switchboard github repos OWNER` that owner's repositories with
where each is on this computer and its group, and `switchboard github clone OWNER NAME --into
/absolute/folder` clones one into a folder that does not exist yet. MCP: `switchboard_github`,
`switchboard_github_clone`.

`connect` opens the PassionCode.ai consent page in the default browser and listens on
`127.0.0.1` for its answer (RFC 8252 loopback, PKCE S256). The organizations you are a member of
arrive with their names and domains; one you typed with the same name or a shared domain is linked,
not duplicated. They are read again at start and every 6 hours while the app or `serve` runs.
The refresh token is kept in the OS vault, never in a file; revoke the grant itself in My data on
PassionCode.ai. Without a connection nothing here runs.

`--secret-stdin` is mandatory for add; a secret value is never accepted as an argument. Redirect from a private credential file or a trusted secret manager. Do not put tokens in shell history. Provider/kind are explicit; supported kind spellings are `api-key`, `setup-token` and `oauth`. OAuth input is provider credential JSON. UUID identifies an account; labels are display text. Removal has the same selected-account/active-home guards as the GUI.

Without a running owner, metadata operations acquire the Store’s exclusive lock and release it after the command. While another process holds that lock (an owner draining or starting during an update restart, or another command), a command asks again every 200 ms, the running owner first, for up to 8 s (SB-181); “Another Switchboard window owns the account storage. Close that instance, then retry.” after that means the lock is still held (`--json` `raw`: *Another Switchboard instance owns this account storage*). With GUI or `serve` running, they use the owner’s authenticated loopback control server. Both paths run core validation. The CLI must not convert an authentication failure into a local-write fallback.

### Moving to another machine (SB-176)

Accounts never move: the other machine signs in to its own seats ([two machines](research/two-machines-2026-10-23.md), D-1). Settings do:

```sh
switchboard settings export > switchboard-settings.json          # on this machine: no account, no credential
switchboard settings import switchboard-settings.json             # on the other one: what it would do
switchboard settings import switchboard-settings.json --apply     # do it
switchboard settings import switchboard-settings.json --under ~/code --under ~/work   # where its checkouts are (default ~/DATA)
switchboard settings import switchboard-settings.json --apply --clone   # also clone what is on GitHub but not here (SB-268)
switchboard settings signer                                      # the public key you sign settings files with, to publish (SB-272)
switchboard settings export --sign > switchboard-settings.json   # signed with that key
switchboard settings export --sign --group <pool> > member.json  # a member's machine: only that group, its organizations, ceilings and rotation settings (repeat --group; SB-292)
switchboard settings import switchboard-settings.json --signer <published key> --apply   # only a file that key signed
switchboard settings sync --to user@studio.local [--group <pool>] [--apply]   # export, sign and import there over ssh in one step: a preview, or the import (SB-326)
```

The file (`switchboard-settings/1`) holds each group's name, organization, sources, the identities of its repositories (`host/owner/name`) and its plain folders; the organizations' names and domains; the default and disabled agents; the rotation policies. Import finds this machine's checkouts of each repository under `--under`, lists what it found and what has no checkout here, and on `--apply` adds the organizations it lacks, makes the missing groups with no accounts (give them this machine's own), adds found folders to existing groups without changing their accounts, keeps each group's sources (the GitHub owners whose new repositories join it, SB-267), and sets the agents' settings and the policies: a policy follows its group to the pool it landed in here, one whose group is not here is left out and named, and a policy for the ordinary Claude Code (`claude_cli`) arrives switched off. With `--clone` (only with `--apply`), the repositories on GitHub that have no checkout here are first cloned with your GitHub CLI (`gh`) into the first `--under` folder, so a new member's computer with nothing checked out yet gets the organization's groups (SB-268); a clone that fails is named and the rest goes on. The file also carries your Fabric Inbox key's address and Client Id (never its secret: the preview prints the `switchboard inbox set` command to type it again), the Claude and ChatGPT teams your organizations follow, the terminal hook's on or off when you chose one, and the spending ceilings of the computer and its groups (SB-270). Import sets the hook only where this computer has no choice yet, and a ceiling only where there is none or the file's is lower: it never raises one, and a group's ceiling waits until that group has its own key here. Rules and fallback chains name accounts and stay behind. An agent carries it with `settings sync` (MCP `switchboard_settings_sync`, SB-326): the same signed file goes over ssh to the other machine's `settings import`, as a preview, or imported with `--apply`; `settings export` itself stays the person's.

### A seat delegated to a teammate's machine (SB-403, DEC-0025)

```sh
# On the teammate's computer
switchboard own-sign-in off                 # Switchboard never reads or changes your own sign-in here (own-sign-in on, or no argument to read it)
# ~/.ssh/authorized_keys, the one line for the administrator's ssh key (replace an older one; keep the quotes):
#   command="'/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard' ssh-serve --sender-key <its settings signer key> --pool <group id>",restrict ssh-ed25519 AAAA… administrator
# On the administrator's computer
switchboard accounts send ACCOUNT_UUID --to teammate@host --pool <group there> --delegate [--lease-hours 48]
switchboard accounts recall ACCOUNT_UUID    # take it back, a seat whose lease ran out included (its sign-in is kept there 30 days)
```

`ssh-serve` answers only what a delegation into the group its line names (`--pool`) needs:
`machines prove`, `accounts list` (only the caller's seats there), `project list` (that group:
id, name, organizations; no folder), `org list` (that group's organizations), `org assign` of a
seat (to those organizations or personal), and the delegation calls `accounts add --delegated`,
`usage`, `accounts surrender`, `accounts lease` and `accounts remove` with `--sender-key KEY`,
read from `SSH_ORIGINAL_COMMAND` without a shell. Only `ssh-serve` sends these delegation calls:
the local CLI, MCP and window never send them; a process holding the control token can, with the
same trust as the person's own Keychain; against an uncooperative teammate the real revocation is
signing the subscription out at the provider. A plain move there is refused with “That machine takes seats only as delegated ones.
Move it again as a delegated seat (accounts send --delegate).”. The seat is a guest there (never
moved on, lent, given to an agent, made the ordinary sign-in, launched in an isolated home, or
removed before its sign-in is); this machine's background read extends its lease every 15
minutes. Past it, the seat there is switched off but keeps its sign-in for 30 days: the next read
turns it on again, and a call back takes the live sign-in.

## Running sessions and official sign-in

Keep the desktop open **or** run this in a separate terminal:

```sh
switchboard serve
```

Then use another terminal:

```sh
switchboard status
switchboard backup list
switchboard backup now
switchboard backup restore switchboard-backup-1790000000-<id>.json   # a name from backup list
switchboard login begin --provider codex --label Work --pool work
switchboard login status LOGIN_UUID
switchboard login pending                 # every sign-in waiting, oldest first, as the window lists them (SB-299)
switchboard login finish LOGIN_UUID
switchboard accounts select ACCOUNT_UUID --provider codex --pool work
switchboard launch ACCOUNT_UUID --mode managed --working-directory /absolute/project/path
switchboard launch --provider claude --working-directory /absolute/project/path --in-place -- --resume SESSION_ID
switchboard launch --provider claude --working-directory /absolute/project/path --headless -- -p "…"   # SB-317: from a launchd job or a scheduler: no terminal, nothing on stdin, the agent's exit status
switchboard launch ACCOUNT_UUID --working-directory /absolute/project/path --by voc-learn --parent LAUNCH_ID
switchboard continue wf_0123456789abcdef --account ACCOUNT_UUID --dir /absolute/checkout
switchboard --json dispatch --dir /absolute/project --workflow wf_0123456789abcdef --handoff handoff:0123456789abcdef --request REQUEST_UUID --dispatch-id DISPATCH_ID --reason request --permissions all --timeout-minutes 30
switchboard --json dispatch --dir /absolute/project --workflow wf_0123456789abcdef --handoff handoff:0123456789abcdef --reason inbox --timeout-minutes 30   # SB-338: the project's unread board messages, settled by read marks; never with --target
switchboard --json dispatch --dir /absolute/project --workflow wf_0123456789abcdef --handoff handoff:0123456789abcdef --request TASK_UUID --reason resume --timeout-minutes 60   # SB-398: continue a Fabric task a session let go of; the run claims and releases the task, answers no request
switchboard --json dispatch stop --launch-id LAUNCH_ID
switchboard --json dispatch --dir /absolute/project --workflow wf_0123456789abcdef --handoff handoff:0123456789abcdef --request REQUEST_UUID --target sshlg@studio   # SB-317: the session runs on that machine in a fresh worktree of the repository there
switchboard --json dispatch --dir /absolute/project --workflow wf_0123456789abcdef --handoff handoff:0123456789abcdef --target auto --candidates sshlg@studio   # SB-317: here or on the candidate whose accounts have the most room
switchboard --json dispatch status --launch-id LAUNCH_ID   # SB-317: running, exited (exit_code), stopped, timed_out, lost or unknown; a remote run's result file in `result`
```

Backups belong to the default data folder: with `--data-dir`, `backup list` shows none and `backup restore` refuses (“Backups belong to the default data folder; this data folder has none.”), so a scratch store never sees or restores the operator's accounts (SB-61, `a_scratch_data_folder_never_sees_the_real_backups`).

On Windows use an absolute drive or UNC project path, quoted if it contains spaces. CLI login/launch need a live owner because its proxy and pending-login registry must outlive the command. Missing owner returns an instruction to start GUI or serve. Managed launch requires that account selected in its provider/pool; isolated mode launches a private credential snapshot and is not changed by later selection.

`login begin` opens the installed official provider CLI in a private home; `--label` is optional and defaults to the email of the account that signs in; signing in to an account already saved updates it where it is (label and pool kept, every copy in every pool) instead of adding another (SB-62). `login status` answers `pending`, `complete` (ready to finish) or `ended` (the Terminal session exited without a successful sign-in) and reads no credential; the desktop app polls it and finishes on its own. Complete its login, then `finish` imports only that new credential into the OS vault and cleans staging. If the account is saved but its staging home cannot be removed, `finish` reports *The account was saved. A temporary sign-in folder was left behind; Switchboard removes it before the next sign-in.* (`raw`: `Account saved; isolated login cleanup needs attention.`), releases the sign-in slot and retries the cleanup before the next `login begin` (`failed_login_cleanup_releases_the_slot_after_the_account_is_saved`). To cancel, close/finish that Terminal process, then `switchboard login cancel LOGIN_UUID`. It never kills an unrelated process or invokes global logout. A sign-in survives an app or `serve` restart: the next owner takes back the sign-ins whose journal it can read and that completed in Terminal or are still open there, newest first and at most four (`MAX_LOGINS`; `adopt_logins` in `crates/switchboard-runtime/src/launch.rs`, test `a_sign_in_open_across_a_restart_is_taken_back_and_kept_from_the_sweep`), so its login id still answers `status`, `finish` and `cancel`; the rest are swept. Recovery limits are in [operations](OPERATIONS.md).

`login begin --provider claude --headless` (SB-256 N1, macOS and Linux) runs the official sign-in
without Terminal, as the owner's own child in the private home: `login status` then also answers
`manual_url`, the sign-in page whose code goes to `platform.claude.com`; open it, sign in, and pipe
the code that page shows to `switchboard login code --login-id LOGIN_UUID` (stdin only, never an
argument). The status reads `complete` once the CLI accepted it, `ended` when it refused it; `login
cancel` ends the CLI's process group. The sign-in autopilot (SB-256) drives the same steps.

`launch` opens the session in a new Terminal window. **`--in-place`** (0.6.9, SB-75) prepares exactly the same session (the private home, its tools and `switchboard mcp`, the scrubbed environment, the one-session-per-home marker) and runs it in the terminal the command was started in, for a host with its own console (Fabric Dashboards' panel); the agent's exit status is the command's, its launch record gets its end (0.7.97: before, an in-place launch read `gone`), Ctrl-C and a closed window reach the agent, a SIGTERM sent to `switchboard` is passed on to it, and the host never sees a token. It needs a terminal on stdin and stdout and, like every launch, a running owner; macOS and Linux for now. **`--provider claude|codex`** instead of an account id uses the account Switchboard would use for that folder: its group's selected account, else the account of the folder's rule in force, else the default group's selected account. Arguments after `--` go to the agent after Switchboard's own, each quoted (at most 16, up to 1024 characters, no control characters), for example `-- --resume <id>` for Claude Code, `-- resume <id>` for Codex; they need `--in-place`. For the launch record (SB-212): **`--by NAME`** names the runner that launches (up to 64 letters, digits, dots, hyphens or underscores; otherwise refused with “Name the launcher with up to 64 letters, digits, dots, hyphens or underscores.”), **`--parent LAUNCH_ID`** names the launch this one continues, whose record then says its work went here (a malformed id is refused with “Give a launch id from switchboard sessions --history as the parent.”), and **`--surface dashboards|fabric|recovery`** says the launch comes from Fabric Dashboards' console, from Fabric's own console (0.7.84, Fabric CO-584; recorded as `fabric`, like a run `switchboard dispatch` starts) or from a host resuming a stopped session (default `cli`). The answer carries `launch_id`.

`continue` hands an [Observatory](CONTRACTS.md#workflow-continuation-sb-52-2026-10-05) workflow whose executor ran out of its limit to another account: of the same provider, or since 0.6.7 of the other one, Claude Code ↔ Codex, with the same checkpoint and constraints (SB-70) (`--mode isolated` by default, or `managed`). Since 0.6.31 `--account` also takes `opencode`: OpenCode on the saved OpenRouter key and its default model, with its own configuration for that session only (`OPENCODE_CONFIG_CONTENT`), the handoff as its first message (`opencode --prompt`) and never a change to your own OpenCode (SB-119); without `--account` the default agent continues (`switchboard agents default`, SB-120). Since 0.6.20 `--account` also takes a saved Kimi Code account's id (SB-99) or `hermes`: Hermes Agent on the saved OpenRouter key, on the model your Hermes is set to when it runs on OpenRouter (`switchboard agents hermes status`), else the key's default model (`switchboard agents openrouter model`); in a home Switchboard owns, its first turn the handoff (`hermes chat --query-file`, SB-107). The answer's `modelFrom` says which (`hermes` or `key`). With `--in-place` (0.6.25, SB-108) the continuing session runs in the calling terminal instead of a new Terminal window (the same session, home and first prompt), for an embedded console such as Fabric Dashboards'; it needs a terminal on stdin and stdout and refuses before anything is offered without one. It reads the workflow, refuses before anything changes when it cannot be continued here (closed, no checkpoint, a handoff already waiting, a declared key not in the vault, a workflow that names no executor, the same account, a folder outside the workflow's checkouts), offers it with reason `limit` and opens the account's session in Terminal. That session accepts the handoff itself, through the Observatory tools Switchboard declares in its config. The engine refuses an offer while the executor wrote a checkpoint less than two minutes ago; Switchboard reports that sentence and launches nothing. Set `SWITCHBOARD_OBSERVATORY_BIN` when `project-observatory` is not in `~/.local/bin`, Homebrew or `PATH`. macOS only for now.

`dispatch` (SB-247, next release) is Fabric's command for a board request nobody answered: Fabric writes the request's workflow and offers it to the default agent's provider, and passes the folder, the workflow, the hand-over and, with `--request`, the board request (without it, the workflow's `board-request:` artifact names it; the flag wins). `--dispatch-id` is Fabric's own id for the dispatch, kept in the launch record. Switchboard chooses the account as `continue` without `--account` does (the default agent, the folder's group, the spending ceilings) and starts the agent with no person and no Terminal: Claude Code as `claude -p`, Codex as `codex exec`, in the account's isolated home, with `FABRIC_DISPATCH_WORKFLOW`, `FABRIC_DISPATCH_HANDOFF`, `FABRIC_DISPATCH_REQUEST` and, with `--dispatch-id`, `FABRIC_DISPATCH_ID` set; the prompt's first line is `fabric-dispatch/1 workflow=… handoff=… request=…`. The session accepts the hand-over itself. `--permissions all`, the default since 0.7.24, adds `--dangerously-skip-permissions` (Claude Code) or `--dangerously-bypass-approvals-and-sandbox` (Codex); `ask` keeps the agent's own. The answer is `{launch_id, agent, account_id, state}`: `started`, `running` (a run of the same workflow already runs: its id, nothing new starts) or `refused` with a `reason` code and a `message` (`no_agent`, `not_in_group`, `ceiling_reached`, `spend_unknown`, `agent_not_installed`, `permission_mode`, `handoff_refused`, `at_capacity`: this computer's session limit is reached, a wait rather than a failure). A refusal is an answer, exit 0; a malformed argument or a missing owner is an error, exit 1. The run writes its output to `launches/logs/<launch_id>.log` in the data folder and is stopped by `dispatch stop --launch-id` (`stopped` or `already_ended`; also MCP `switchboard_dispatch_stop` and Needs attention's *Stop the run* for a run quiet for 30 minutes or gone without an end; `--if-gone` only records the end of a run whose process is gone and refuses a live one) or after `--timeout-minutes` (default 60), by the running owner, never before (Fabric passes a limit already cut to the board request's deadline); a refusal `at_capacity` is counted, and `switchboard capacity` reports the last hour's; each such stop is written to the duty officer's journal (`switchboard duty` lists it under its recent actions); `sessions show <launch_id>` reads its end (`ended`: `{kind: exited | stopped | timed_out, exit_code?, at}`) and `launched_by.dispatch` (`{id, request, thread, project, workflow}`). Only Claude Code and Codex take a dispatch for now; on Windows it answers `handoff_refused` until the Observatory engine is found there. The fields: [CONTRACTS](CONTRACTS.md#dispatch-for-a-board-request-sb-247-2026-11-04).

Select another same-pool account while the managed response streams: the current response keeps its snapshot; the next request sees the new account. `Ctrl-C` or `SIGTERM` in `serve` stops the owner within ten seconds. Since 0.5.4 the proxy keeps its port and token across restarts, so a managed client launched earlier works again once an owner is back. Only one owner may hold a data root.

## A machine with no graphical login

```sh
switchboard vault use file     # keep this data folder's sign-ins in 0600 files
switchboard vault use native   # back to this computer's own store (the default)
switchboard vault use file --move-from-native   # in a session where the keychain opens: move its sign-ins into files
```

A `serve` under a LaunchDaemon on a Mac with no graphical login, or in a Linux VM with no session bus, has no unlocked store for its sign-ins (`Credential storage unavailable`). `vault use file` keeps them as 0600 files in `<data>/vault-files/`, protected as well as the user account is, as Claude Code's own `.credentials.json` is there. It is refused while this computer's own store works, unless `--move-from-native` says to move from it on purpose (SB-301: a Mac Studio space opens its login keychain only in a session of its own, so the move runs there and the daemon then reads the files; run it again to move what it could not read), and while Switchboard runs (the store is opened exclusively). The choice is the data folder's (`vault.json`), so the app, `serve` and every command read the same store; each switch moves the sign-ins it can read and names the rest in `not_moved`. `status` shows it as `vault` (SB-287, [design](research/headless-vault-2026-11-07.md)).

On a Mac Studio space the owner runs as a LaunchDaemon that Project Observatory's validator installs (`observatory-space-daemons`, OBS-361). Stop any `serve` started by hand, in a session of the space where its keychain opens run `switchboard vault use file --move-from-native` (its sign-ins move into files; `not_moved` names any it could not read), then put this definition in the space's `~/.local/share/project-observatory/background/LaunchAgents/ai.passioncode.fabric-switchboard.serve.SPACE.plist` (folder 0700, file 0600, owned by the space's account) and let the validator `plan` and `apply` it (`remove` first when an older one is loaded). The validator writes `UserName`, `GroupName`, `InitGroups` and `HOME` itself and moves the logs to `/Library/Logs/ProjectObservatory/<account>/`, so the definition names neither; `ProgramArguments[0]` is the app's own binary, not the `~/.local/bin` link; `SPACE` is the space's account and `/Users/SPACE` its home: a label lives in the system domain, one per machine, so each space's daemon has its own (SB-301).

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>ai.passioncode.fabric-switchboard.serve.SPACE</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard</string>
    <string>serve</string>
  </array>
  <key>WorkingDirectory</key><string>/Users/SPACE</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
</dict>
</plist>
```

`serve` stops with status 0 on a signal, so `KeepAlive {SuccessfulExit: false}` starts it again only after a crash.

Such a computer never opens the app, so it never checks for updates (SB-305). The account that owns the app updates it
from the CLI, then restarts the daemons:

```sh
"/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard" --json update --check   # {current, latest, action: up_to_date|available}
"/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard" --json update            # installs a newer version; {action: installed, restart_needed: true}
switchboard update --app /opt/fabric-switchboard/Fabric-Switchboard.AppImage             # Linux: the AppImage to replace (default $APPIMAGE)
switchboard update --feed latest.json --archive Fabric-Switchboard-X.Y.Z-macos-universal.app.tar.gz   # files fetched with `gh release download` where github.com refuses anonymous downloads; checked the same way
```

It reads the release feed the app reads, offers only a newer version, refuses a release that needs a person first
(`needs_migration`), checks the package's minisign signature against the key the app ships and the version signed
into it, and replaces the app with one rename (the old one is put back if the move fails). A `serve` already running
keeps the old binary until it is restarted (`launchctl kickstart -k system/<label>` or the validator's `remove` and
`apply`). Windows is not covered: there the app updates itself.

A copy installed from the .deb package is never replaced (SB-408): the package manager owns its files. There
`switchboard update`, with or without `--check`, reads the feed and answers the installed and the latest version and
the commands that download, verify and install the newer package, installing nothing:

```sh
switchboard --json update --check   # {package: deb, current, latest, action: up_to_date|available, release, commands, command, release_key_fingerprint}
```

`command` is one command for version X.Y.Z on x64 (`arm64` on ARM): each step runs only when the one before
succeeded, the key goes into a keyring of its own (its armour taken off with `sed` and `base64`, so no `gpg` is
needed), and the checksums count only with a good signature whose primary key is the PassionCode.ai release key
`63B30DC324BD697487AA31944FAFB8AEC803B6A7` (`release_key_fingerprint`). It needs `curl` and `gpgv` besides coreutils
and apt (`prerequisites`); `gpgv` comes with apt, and a minimal Debian may lack `curl`: `sudo apt install curl gpgv`
adds whichever is missing.

```sh
mkdir -p ~/Downloads/fabric-switchboard-X.Y.Z && cd ~/Downloads/fabric-switchboard-X.Y.Z && \
curl -fLO https://github.com/passioncode-ai/fabric-switchboard/releases/download/vX.Y.Z/Fabric-Switchboard-X.Y.Z-linux-x64.deb && \
curl -fLO https://github.com/passioncode-ai/fabric-switchboard/releases/download/vX.Y.Z/SHA256SUMS && \
curl -fLO https://github.com/passioncode-ai/fabric-switchboard/releases/download/vX.Y.Z/SHA256SUMS.asc && \
curl -fsSL https://raw.githubusercontent.com/passioncode-ai/.github/main/release-signing/passioncode-release-signing.asc -o release-key.asc && \
sed -e '1,/^$/d' -e '/^=/,$d' release-key.asc | base64 -d > release.gpg && \
gpgv --status-fd 1 --keyring ./release.gpg SHA256SUMS.asc SHA256SUMS | grep -Eq '^\[GNUPG:\] VALIDSIG .* 63B30DC324BD697487AA31944FAFB8AEC803B6A7$' && \
sha256sum -c SHA256SUMS --ignore-missing && \
sudo apt install ./Fabric-Switchboard-X.Y.Z-linux-x64.deb
```

A `serve` from the .deb logs `update_waiting deb` once per newer version. Once its program was replaced (by apt
here; for an installed app or AppImage, by another account's update or `switchboard update`), any `serve` restarts
onto it (exit status 75) only under a supervisor that starts it again: a systemd unit that runs `serve` itself, so
`SYSTEMD_EXEC_PID` is its own pid (systemd 248 or later; set `Restart=on-failure`), or a launchd job, whose
`XPC_SERVICE_NAME` is the job's label and whose parent is launchd (set `KeepAlive {SuccessfulExit: false}`). Started
by hand (a shell, `nohup … &`, `setsid -f`, cron `@reboot`, a unit that runs it through `sh -c`), it logs
`update_restart unsupervised` once and keeps running on the old program until you restart it; an inherited
`INVOCATION_ID` or init as the parent does not count.

## Uninstall

```sh
switchboard uninstall              # the plan; nothing changes
switchboard uninstall --yes        # remove credentials, the CLI link and the data folder's own entries
switchboard uninstall --keep-data --yes
switchboard uninstall --purge --yes    # also the session homes and the CLIs' history in them
```

Quit the app and `serve` first (the store is opened exclusively). What is and is not removed: [OPERATIONS → uninstall](OPERATIONS.md#corrupt-metadata-backup-and-uninstall) (SB-29).

## Usage, output and failures

```sh
switchboard usage
switchboard usage ACCOUNT_UUID
switchboard --json status
switchboard --json activity
```

Without an ID, usage reads cached observations; an ID requests a provider check, refused without a request while a wait the provider asked for lasts (SB-39; [ACCOUNTS-AND-ROTATION](ACCOUNTS-AND-ROTATION.md)). Human output prints times as RFC 3339 UTC, marks an observation older than 900 seconds as stale (the idle quota cadence plus slack, SB-48; rotation judges by its policy's `--max-age`) and shows the last check's health (`ok`, `failed`, `unavailable` or `not checked`) with the next check time; `events` prints RFC 3339 times too, oldest first; `--kind`, `--account`, `--before` (Unix seconds, exclusive) and `--limit` (1 to 500; without it every event the journal holds) page it as the window and `switchboard_events` do ([CONTRACTS](CONTRACTS.md#activity-by-kind-and-page-sb-373)). `--json` output keeps Unix seconds (`human_usage_and_events_print_utc_times_and_health`). Unsupported/unknown quota is an error or unknown state, never zero. The human percentage is the account's own; a Codex feature limit is listed in `--json` and MCP (`scope: feature`) but does not count toward it (SB-40). API billing is not subscription utilization.

`--json` writes `{ "ok": true, "data": ... }` to stdout on success and `{ "ok": false, "error": "..." }` to stderr on failure. Exit status is 0 for success, 1 for an operational failure and 2 for invalid arguments. `--help` succeeds without touching the vault. Do not log stdin or treat parser diagnostics as a place to print credentials. Output never includes the control capability, provider token, credential JSON or provider request/response payload. The [CLI implementation and integration fixtures](../crates/switchboard-cli/src/main.rs) define this envelope.

The local `control.json` contains a capability and is private runtime state. It is not an API discovery file to share. It does not grant authority on LAN: clients require exact loopback host/port, no redirects/proxies, bounded bodies and a per-runtime bearer. Browser Origins are refused. Same-user malicious processes are outside the protection boundary, just as for the official CLIs they could inspect.

## Capture, import and automatic rotation

```sh
switchboard current
switchboard accounts capture --provider claude --pool default
switchboard accounts import-claude-swap --pool default
switchboard accounts activate ACCOUNT_UUID
switchboard rotation set --provider claude --pool default --target claude-cli --enabled true
switchboard rotation status
switchboard rotation set --provider claude --pool default --target claude-cli --enabled false
```

Capture reuses existing local authorization without another login. When the saved account belongs to an organization that has exactly one group and it landed outside that group, the `login finish` and `accounts capture` receipts carry `organization_group: {pool, name, in_cli?}` (SB-160, SCN-077): the group to move it to with `accounts move`, which nothing does on its own. `in_cli: true` marks the account the ordinary `claude` or `codex` is signed in to, which joins a group only after the CLI switches (always so for `accounts capture`; for `login finish` when the current accounts list its id). A repeated `login finish` reads the account where it is now, so after a move it carries no offer; nor does an account whose identity the group already holds (`a_new_seat_the_cli_is_signed_in_to_says_so_in_its_receipt`, `a_repeated_finish_reads_where_the_account_is_now` in `crates/switchboard-runtime/src/lib.rs`). Import reads Claude Swap and reports imported/failed/skipped counts without credentials. Activate changes the ordinary Claude Code account with identity checks and compatible locks; only captured/imported Claude OAuth profiles qualify. Account selection for the proxy stays separate.

Rotation targets are `managed` and `claude-cli` (serialized `claude_cli`). `rotation set` changes only the flags it is given and keeps the saved values of the others, so `--enabled false` stops rotation without resetting custom settings. A new policy starts off, with `--threshold` 90, `--hysteresis` 10, `--cooldown` 1800 seconds and `--max-age` 300 seconds (`rotation_set_changes_only_the_given_flags`). The saved policy is read and the merged one written in two steps; a concurrent change between them is overwritten. Hysteresis is headroom below the threshold. Only one native Claude policy may be enabled across all pools because the ordinary native account is one shared target. Existing streams are never replayed. Configuration persists offline; status explicitly reports whether the owner/monitor runs. Full behavior and limitations: [account and rotation contract](ACCOUNTS-AND-ROTATION.md).

## For agents

Version 0.4 serves Switchboard to coding agents over the Model Context Protocol and adds optional project rules. The [tool reference](../plugins/switchboard/skills/switching-accounts/references/tools.md) and the [server source](../crates/switchboard-cli/src/mcp.rs) define the tools (PLAN-0.4's table is the 0.4 design, kept for history); the [plugin](../plugins/switchboard/README.md) and its [skill](../plugins/switchboard/skills/switching-accounts/SKILL.md) tell an agent how to use them.

```sh
switchboard mcp               # stdio server: status, accounts, usage, switch, project rules
switchboard mcp --read-only   # only the 38 read tools; usage refresh shows the stored observation
switchboard --data-dir /path/to/app-data mcp
```

`switchboard mcp` speaks newline-delimited JSON-RPC 2.0 on stdin/stdout (MCP `2025-06-18`, also `2025-03-26` and `2024-11-05`; clients asking for `2025-11-25` (MCP Inspector 2.9.0, the TypeScript SDK 1.32.0 and client 2.3.0, the Python SDK 2.3.0) accept `2025-06-18` and work: [matrix](runs/2026-10-04-sb-43-mcp-matrix/README.md), `scripts/mcp_compat.py`); stdout carries protocol messages only. It uses the running owner's control channel when the desktop app or `serve` runs, else an offline store operation under the exclusive lock. No tool accepts or returns a credential, and launch is not exposed; `switchboard_login` and `switchboard_kimi_login` begin a sign-in that the person finishes. `switchboard_switch` with `target: "claude_cli"` changes the ordinary Claude Code login for every `claude` session and is refused unless the call carries `global: true`. The server lists 76 tools; `--read-only` lists the 33 that only read and none of the 43 that write (the tool reference has one section per tool, and `every_mcp_tool_is_in_the_skill_reference` fails when one is missing). A managed session that Switchboard launches gets the server automatically when the CLI is found (Claude through `--mcp-config`, Codex through `[mcp_servers]` in its private config); an isolated session gets the read-only set. Both also get the Fabric project board (`fabric`) when the operator registered it in that provider's user scope (SB-20).

A project reserves its accounts to its folders and their subfolders. A project has no folder limit; all projects together hold at most 1024 folders, and no folder may sit inside another project's (`a_project_has_no_folder_limit_and_all_projects_hold_1024`).

### Ordinary Claude Code in a project's folders uses the project's accounts (SB-118)

The ordinary `claude` is signed in once for every folder. So that sessions in a project's folders spend the project's accounts, Switchboard writes two keys into each folder's `.claude/settings.local.json`:

- `env.ANTHROPIC_BASE_URL`: the proxy route of the project's pool;
- `env.ANTHROPIC_CUSTOM_HEADERS`: `X-Switchboard-Capability: <capability>`, a capability for that pool only, sent as a header of its own (SB-140). It is not an auth source, so the session keeps its claude.ai login and the claude.ai connectors. Versions up to 0.6.37 wrote `ANTHROPIC_AUTH_TOKEN`, which turned the connectors off; the next sync replaces it.

This covers worktrees inside a project folder too. Everything else in the file stays as it was, and the file is kept out of Git through the repository's `info/exclude`. Your skills, plugins, MCP servers and `CLAUDE.md` stay in force. A session already running keeps its account until it restarts.

```sh
switchboard project bindings    # the folders bound, with their pool and route
```

Leaving a project removes only what Switchboard wrote, and `switchboard uninstall` removes every binding. Never print that file in a chat: its token opens the pool on this computer. See the [SB-118 packet](packets/sb-118-claude-bindings.md).

### The Fabric Inbox key (SB-256)

```sh
pbpaste | switchboard inbox set --origin https://inbox.example.com --client-id CLIENT_ID
switchboard inbox status            # saved, the server and since when; never the key
switchboard inbox status --check    # and whether the server answers it, with how many mailboxes
switchboard inbox remove
```

The key the Claude sign-in autopilot reads its sign-in mail with: a Fabric Inbox `read` key limited
to the sign-in mailbox, made in Fabric Inbox's Settings, Agent access (its Client Secret is shown
once: copy it, then run `inbox set`, which reads it from standard input, so it stays out of your
shell history). It is kept in the vault; `inbox.json` beside the store names the server and the
Client Id. No MCP tool touches it.

### Terminals in an organization's folders (SB-183, 0.7.0)

In a group bound to an organization, a terminal with Switchboard's shell hook runs plain `claude`
and `codex` in that group's shell homes, with `SWITCHBOARD_CONTEXT=org:<id>`, and gives the
person's own values back when it leaves the folder ([SCN-074](ux/scenarios.md), design
[dossier A](runs/2026-10-26-iteration-0.7.0/dossier-a-context-binding.md) part A). The owner
(the app or `serve`) writes each organization group's shell homes, `<data>/shell/claude-<pool>`
and, for a group with a Codex account, `<data>/shell/codex-<pool>`, and the folder map
`<data>/shell/map.json`; a computer with no group bound to an organization gets none of them, and
every folder there answers `none`.

```sh
switchboard shell status            # the hook's state, this terminal, the folder's group and homes
switchboard shell status --dir /w/acme --json
switchboard shell env --shell zsh --dir "$PWD"   # what the hook asks on every change of folder
switchboard shell init zsh          # the hook script itself (also bash, fish, pwsh)
switchboard shell off               # take the hook out of every start file and keep it out
switchboard shell on                # put it back, with or without an organization's group
```

- `shell env` prints one line: `none`, or the group's pool, its context, the Claude shell home and
  the Codex shell home (`-` for none), separated by tabs. It reads only `<data>/shell/map.json` and
  `<data>/shell-hook`: no store lock, no owner, so a busy or stopped owner never holds a terminal.
  It always exits 0 and never writes to the error stream; anything it cannot read (no map, a map
  of another version or over 1 MiB, the hook turned off, a relative or unknown folder, a bad
  argument) answers `none`. The Claude home is answered only in the group's folder itself: Claude
  Code reads Switchboard's route only from the folder it starts in, so in a subfolder the Claude
  field is `-` (a group with no Codex account answers `none` there). Budget 25 ms
  (`shell_env_answers_without_the_owner_or_the_store_lock_within_25_ms`).
- `shell init <zsh|bash|fish|pwsh>` prints the hook script, naming this executable and the data
  folder, for people who keep their own start files: `eval "$(switchboard shell init zsh)"` in
  `.zshrc` (bash likewise), `switchboard shell init fish | source` in fish's,
  `switchboard shell init pwsh | Out-String | Invoke-Expression` in a PowerShell profile (the
  PowerShell script carries the two paths as base64, so Windows PowerShell 5.1's console code
  page cannot garble a user name that is not ASCII). On every change of folder it
  asks `shell env` and exports the group's `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
  `SWITCHBOARD_CONTEXT`, keeping your own values in shell variables it never exports and giving
  them back exactly when you leave; a variable you change while inside stays yours. It prints
  nothing, never fails the prompt, and does nothing in a terminal Switchboard launched. A CLI that
  is gone, too old for `shell env` or failing reads as `none`, so the values it set are given back;
  zsh waits at most ten seconds for an answer. fish runs the hook in interactive shells only;
  PowerShell asks as it loads only in a shell you type in (not under `-Command`, `-File`,
  `-EncodedCommand` or `-NonInteractive` without `-NoExit`), and does nothing in Constrained
  Language Mode.
- On a computer with a group bound to an organization, the installed app (in `/Applications` or
  `~/Applications` on macOS) puts one marked block
  (`# >>> Fabric Switchboard: shell hook (SB-183) >>>` … `# <<< Fabric Switchboard <<<`) at the end
  of the start file of each shell present: `.zshrc` (in `ZDOTDIR` when set; never created when
  there is none and `~/.zshenv` exists, or `~/.zprofile` names `ZDOTDIR`, since the app cannot
  see where zsh reads it: `shell status` then gives `eval "$(switchboard shell init zsh)"` to add
  yourself), bash's login file on
  macOS (`.bash_profile`, `.bash_login` or `.profile`) and `.bashrc` on Linux, and fish's own file
  `~/.config/fish/conf.d/fabric-switchboard.fish`; on Windows, `Microsoft.PowerShell_profile.ps1`
  of each PowerShell edition installed (`WindowsPowerShell\` and `PowerShell\` under Documents,
  wherever Documents was redirected), except where the execution policy is `Restricted` or
  `AllSigned`: that profile is not changed and `shell status` says so (`policy_blocked`);
  Switchboard never changes the policy. Where PowerShell is held to Constrained Language Mode
  (Application Control holds both editions; `__PSLockdownPolicy` holds Windows PowerShell 5.1
  only), that edition's profile is not changed and `shell status` says so
  (`constrained_language`). A profile saved in UTF-16 (with its byte order mark) is written in
  UTF-16 and given back byte for byte. It reads the hook script only when it is there.
  A start file that is a link (a dotfile manager's), another user's or not text is left alone, and
  `shell status` says so; add `shell init`'s line yourself there. A block you delete by hand is not
  written back. `shell off` gives every file back byte for byte (a start file Switchboard created
  and that holds nothing else is deleted), and `switchboard uninstall` does the same.
- `shell on` and `shell off` act for Switchboard's own data folder only: with another
  `--data-dir` they answer “Only Switchboard's own data folder can change your terminals' start
  files. Run this command without --data-dir.” and change nothing, and `uninstall` of another
  data folder leaves the start files alone. The hook names the installed `switchboard` (a
  development build never names itself; with none installed, `shell on` says so).
- `shell status` reads the same files and the installer's record and never fails: `hook` is `on`,
  `off`, `default_on` (no choice made and a group belongs to an organization) or `default_off`;
  `terminal.hook_loaded` says whether this terminal exported `SWITCHBOARD_SHELL_HOOK=1`; `here` is
  what `shell env` answers in the folder; `files` lists each start file with its state. A group
  with no Codex account reads “codex: your own sign-in (this group has no Codex account)”.

Programs started in such a terminal see a shell home as `CLAUDE_CONFIG_DIR` or `CODEX_HOME`.
Switchboard never takes a folder inside its own data folder for the person's: what a launch links
from, the limit scan's transcripts, where *Codex in group folders* writes its provider and the user
scope a launch copies its MCP servers from all fall back to `~/.claude` and `~/.codex` then. An
offline switch of the ordinary Claude Code from such a terminal is refused: “This session runs in a
Switchboard home or a terminal in an organization's folder. Run this from another terminal.”

### Accounts lent between chosen groups (SB-352)

```sh
switchboard balance status                                  # chosen groups, accounts lent now, the last pass, the newest lends and returns
switchboard balance groups --add agents --add apps-soft --all-new true
switchboard balance groups --remove apps-soft               # its lent accounts go back at once
switchboard balance groups --backup-add spare               # Spare only lends, first and with no minimum (DEC-0016)
switchboard balance groups --backup-remove spare            # Spare takes part like the others again
```

Off until you choose groups ([DEC-0012](DECISIONS.md#dec-0012-accounts-are-lent-between-the-groups-the-person-chose-by-their-last-hours-and-days)).
Every 15 minutes a chosen group that had requests in the last 6 hours and has no account with
room (or one, while the provider refused it for a limit in those hours) borrows one account from
another chosen group: the account with the most room of a group that keeps at least two with room
after it (one when it had no request for 6 hours). Room is the group's switching threshold less
its hysteresis (80 % by default) on fresh figures. The account keeps its home group and serves
the borrower as a shared account (`project list` shows it under both); it goes back after 2 hours
without a limit refusal in the borrower, or when its home group needs it, and waits an hour
between moves. A group not chosen is never touched, no account crosses an organization's
boundary, and the ordinary Claude Code's or Codex's account, an account given to an agent, used
on another machine, disabled or waiting for a sign-in is never lent. Taking a group out, or the
last one, returns every account lent from or to it at once. `--all-new true` lets a group made
later join by itself. `default` lends and borrows like any group you choose (since wave 2), but it
borrows only a personal account while organizations exist. The choice and the loans are in `balancing.json` (0600), each lend and return
with its figures in `balancing.jsonl` (0600); each is also an activity entry, `balancing lent` or
`balancing returned` (`switchboard events`). The window has the same choice on the Groups page.
MCP: `switchboard_balance`.

### A project's folders follow its work and the disk (SB-117, SB-121, SB-131)

Switchboard keeps a project's folder list by itself and never asks (SB-121, SB-131; operator
2026-10-14 and 2026-10-15). One system notification tells the person about each change.

A folder joins its existing project when it is:

- a git worktree of a project repository;
- a clone with the same `origin`;
- a repository of one of the project's sources (`host/owner`);
- a repository an agent working in the project changes (SB-131). An agent changes it when it edits or writes a file there (Claude Code `Edit`/`Write`), or commits, pushes, stages, merges, rebases, runs `git init` or makes a worktree there. Reading it does not count.

When no group matches, the repository stays in the default group: since SB-146 Switchboard never makes a group of its own for it (REQ-002 did, and scattered repositories that belonged together). Put it in a group with `project move-root`.

Where the signals come from:

- **Background pass.** It looks at the parent folder of every project folder and at every project repository's worktree list, re-reading either only when its modification time changed. It does not scan for unrelated new projects.
- **Recent Claude Code transcripts.** These are the sessions and their subagents of the last 15 minutes. They are read only where they grew, and only the paths are kept. Work in an unassigned session's Git root creates its project through the same path.
- **The plugin's `SessionStart` hook.** It reports the folder a Claude Code session starts in.

Folders that never join or create:

- a folder or project the person removed;
- a folder in a temporary or cache location (`/tmp`, `$TMPDIR`, `~/.cache`, `~/Library/Caches`);
- a folder that is not a git repository, the home folder or a filesystem root;
- a folder two projects match equally.

Codex sessions and sessions Switchboard launches isolated are not read yet (SB-132).

- **Tools' own folders.** A folder of a tool (`~/Library`, a hidden folder in the home folder such as `~/.claude`) never joins because an agent worked there. A worktree of a project repository kept there (`~/.codex/worktrees/…`) still joins as a worktree.
- **Shared repositories.** Suppose agents of two projects work in one repository, an organisation wiki for example. If it joined one project by work, it leaves that project and is never added by work again.
- **Effect of joining or creating.** For a provider with an enabled account in the project, accounts of other pools are refused there (SB-118). A provider with no enabled project account keeps its existing launch fallback; automatic `--provider` selection still requires the project's selected account.
- **When adding fails.** If the folder overlaps another project or the 1024-folder total is reached, nothing is asked. Switchboard tries again every ten minutes and logs `project_suggestion add: refused`.

A project folder deleted from disk leaves its project once it is still missing five minutes later (SCN-061).

- **Stays:** a folder on a drive that may only be unmounted (`/Volumes`, `/media`, `/mnt`), a folder behind a share that went away, and the project's last folder.
- **Renamed or moved:** a renamed or moved folder counts as deleted; add the new place with *Add folder…*.
- **Comes back:** a deleted folder that comes back is found again.

```sh
switchboard project suggest --folder /work/app-review   # attach to the group it belongs to (worktree, clone, worked in); a repository in no group stays in the default group (default: the current folder)
switchboard project suggestions                         # {suggestions: still being added, added: last 24 h, removed: taken out as deleted, last 24 h}
switchboard project accept /work/app-review             # add a folder still being added now
switchboard project dismiss /work/app-review            # never add it
switchboard project source --pool app --add github.com/example   # also adopt new repositories of this owner
```

`suggest --from-hook` reads Claude Code's hook input on stdin. It prints one line, which joins the session's context, only when the folder was added to its group, or an add could not finish yet, and it always exits 0. It gives up after 1.5 s, reading its input included, and then prints nothing: a busy owner (its transaction held by a background pass on a loaded machine) no longer holds the session's start, and the background pass adds the folder within 30 s anyway (SB-149; the plugin's hook timeout is 3 s). See the [SB-117 packet](packets/sb-117-project-suggestions.md), SCN-055 and SCN-061; `suggest::tests`, `activity::tests`, `an_unassigned_git_repository_stays_in_the_default_group_and_no_group_is_made`, `a_repository_may_be_in_several_groups_and_sessions_take_the_one_with_accounts`, `a_projects_new_folders_join_it_by_themselves_and_a_removed_one_stays_out`, `a_repository_a_projects_agent_changes_joins_it_and_a_shared_one_joins_neither`, `a_deleted_project_folder_leaves_its_project_once_it_stays_gone`, `an_unassigned_repository_stays_in_the_default_group_through_the_session_hook` and `project_suggestions_work_from_the_cli_and_the_hook_never_fails`.

Folder rules (the `project set|remove|apply` commands) are optional: a folder and its subfolders start on a chosen account, rules never stop automatic switching, and the desktop app lists them under Groups, where each can be paused.

```sh
switchboard project list                      # {projects, rules}: projects with folders, roots (each with its worktrees and clones), accounts (own, then shared from another group, `shared: true`) and organizations, then rules
switchboard project save --name "Client Alpha" --folder /work/alpha-web --folder /work/alpha-api --account <id>
switchboard project save --pool client-alpha --name "Client Alpha" --folder /work/alpha-web   # update; omitted accounts go back to default
switchboard project save --name "Acme web" --folder /work/acme-web --organization Acme   # SB-299: a new group bound to Acme (repeat --organization for several; --move-accounts moves listed accounts here)
switchboard project edit --pool client-alpha --add-account <id> --remove-folder /work/old   # SB-134: change by delta; the rest stays
switchboard project edit --pool client-alpha --organization Acme   # SB-109: the group runs only on Acme's accounts (`personal` for personal ones)
switchboard project edit --pool shared --organization Acme --organization Beta   # SB-282: a group of two organizations, Acme first
switchboard project edit --pool client-alpha --add-account <id>   # SB-282: an account of another group keeps its home and serves this one too
switchboard project edit --pool client-alpha --add-account <id> --move-accounts   # moves it here instead
switchboard project rotate-capability --pool client-alpha   # the folders' capability leaked: the old one is refused, a new one is written; `skipped` and `not_rewritten` name each folder still holding an old value (SB-244)
switchboard project codex [on|off]     # SB-143: Codex sessions in project folders use the projects' Codex accounts (one provider in Codex's config.toml)
switchboard stats [--days 30]            # SB-145/SB-147: requests and the provider's token counts by group, project, account, agent and day (1-90 days)
switchboard project map [--under ~/DATA]   # SB-135: Git repositories on disk, each with its project or none, and their modules (`module_of`) (default ~/DATA, else home)
switchboard project move-root --folder /work/bot --pool client-beta   # SB-135: a repository with its worktrees and clones into another group, in one step
switchboard project move-root --folder /work/bot --pool tools --also   # SB-146: into one more group; it stays in its others (--from <group> picks the one it leaves)
switchboard project delete --pool client-alpha  # its accounts go back to the default group (SB-314)
switchboard project switch --pool client-alpha off   # SB-401: no folder is routed to it and its accounts serve no request; `on` again
switchboard ceiling set --scope project:tokens --usd 5 && switchboard project switch --pool tokens on   # SB-401: the pay-per-token group turns on only with a daily ceiling
switchboard project edit --pool tokens --add-folder /work/paid   # SB-401: its folders are added one by one; nothing joins it by itself
# a group holding a spending ceiling is deleted only in the window (its ceiling goes with it)
switchboard project show --path /path/to/project
switchboard project set --path /path/to/project --account ACCOUNT_UUID --expires-in-hours 8
switchboard project set --path /path/to/project --account ACCOUNT_UUID --target claude-cli --paused
switchboard project remove --path /path/to/project --provider claude
switchboard project apply --path /path/to/project
switchboard project apply --path /path/to/project --global
```

`--path` defaults to the current folder and must be absolute. `--target` is `managed` (default) or `claude-cli`; `--expires-in-hours` accepts 1 to 720; `--paused` saves the rule switched off. `apply` detects the calling session (managed, isolated or native) and reports one action per provider: `selected`, `activated`, `already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool`, `other_session`, `needs_global` or `failed`. A `claude_cli` rule changes the ordinary Claude Code login only with `--global`.

Connecting an agent. The `switchboard` executable must be on `PATH`; on macOS the desktop app's Agents panel links the bundled CLI to `~/.local/bin/switchboard`, and on Windows the installer puts `switchboard.exe` beside the desktop app (since 0.6.0, SB-05), where the Agents panel finds it and prints the registration commands with its full path; to type `switchboard` in a shell, add that folder to `PATH`. On Linux the .deb installs it as `/usr/bin/switchboard`, already on `PATH`; the AppImage's files exist only while it runs, so connect agents from a .deb install or the `-cli.tar.gz` ([INSTALL → Linux](INSTALL.md#linux)).

| Agent | Command |
|---|---|
| Claude Code, plugin (server plus the `switching-accounts` skill) | `claude plugin marketplace add passioncode-ai/fabric-switchboard`, then `claude plugin install switchboard@switchboard` |
| Claude Code, server only | `claude mcp add --scope user switchboard -- switchboard mcp` |
| PassionCode launcher, once the plugin is released as a member | `npx @passioncode-ai/passioncode@latest update` |
| Codex | `codex mcp add switchboard -- switchboard mcp` |

Restart the agent after installing; it loads servers and skills at session start. The plugin is validated by `python3 scripts/check_plugin.py`, `python3 -m unittest scripts/test_check_plugin.py` and `claude plugin validate ./plugins/switchboard --strict` plus `claude plugin validate . --strict`.

### Sessions (SB-95)

```sh
switchboard sessions          # sessions Switchboard launched (Claude Code, Codex, Kimi Code, Hermes, OpenCode, catalog agents) that are starting or running
switchboard sessions --all    # also the ended ones (each home keeps its last launch's record; a session resumed as itself, with no home, for 90 days)
switchboard --json sessions   # {sessions: [...]}
switchboard sessions --history          # every launch of the last 90 days, newest first (SB-212)
switchboard sessions --history --limit 50 --before 1794700000   # one page; next_before starts the next
switchboard sessions show LAUNCH_ID     # one launch, who launched it and where its work went
switchboard sessions --machine          # every Claude Code and Codex process of this user, Switchboard's or not (SB-353)
```

Each launch writes `.session.json` beside its `launch.command` (0600): `account_id`, `provider`, `pool`, `context` (`personal` or `org:<id>`, SB-110: the context the session runs in, fixed at its start, also exported to it as `SWITCHBOARD_CONTEXT`; since 0.7.0 also Kimi Code sessions, Hermes and OpenCode continuations, agents launched on the OpenRouter key (`provider` and `agent` the agent id, `label` its model) and agents launched through the proxy (`provider` `claude`, the route's provider, and `agent` the catalog id), SB-201), `mode` (`managed`/`isolated`), `working_directory`, `in_place`, `launched_at` and `continues` (the workflow id a continuation picks up, or null); no credential. The list adds `agent` when the record names none (from the provider: `claude-code`, `codex`, `kimi-code`, `hermes`, `opencode`; null otherwise; the text output's second column shows it, the provider only when it is null), `label`, `context_name` (the organization’s name for an `org:` context; the text output shows it after the account), `home` (`homes` or `runtimes`), `home_dir` (the home itself relative to the data folder, such as `homes/<account id>`, SB-197), `started_at` (when the script recorded its process) and `state`: `starting` (Terminal has not run the script yet), `running` (the recorded process is still that session: the pid alive and at least as old as its marker; on Windows, the pid with its creation time), `ended`, or `unknown` (the marker cannot be read). Only Switchboard's own homes are read (`homes`, `runtimes` and `kimi/sessions`) and no link is followed; a session started outside Switchboard is not listed. A session resumed as itself (`sessions continue --in-place --hosted`) has a launch record and no home: it is listed from that record with `home: null`, without `--all` while its session host or its process runs, with `--all` also once ended (90 days); a launch is never listed twice. The MCP tool `switchboard_sessions` (`all?`) answers the same.

`sessions --machine` (SB-353, MCP `switchboard_sessions` with `machine: true`) answers every Claude Code and Codex process of this user instead, read from the process table in the CLI's own process, without the owner (macOS `proc_listpids`, `proc_pidpath`, `PROC_PIDTBSDINFO`, `PROC_PIDVNODEPATHINFO`; Linux `/proc`; Windows lists none yet): `pid`, `agent` (`claude-code` for an executable named `claude` or under `…/claude/versions/`, `codex`), `folder` (its working directory), `started_at`, `launch_id` (the launch whose home's `.session-pid` names the process, its parent or its grandparent; null when it is not Switchboard's), `managed`, `last_write_at` (the newest `.jsonl` of the folder's transcripts in `~/.claude/projects` and in Switchboard's homes, null for Codex), `quiet_minutes` and `quiet` (30 minutes or more), with `total`, `managed`, `unmanaged` and `quiet`. It serves the [session lifecycle plan](plans/2026-11-13-session-lifecycle.md) SL-8. Tests `machine_sessions::tests`. A managed session follows its pool's selected account from the next request, so `account_id` is the account selected at launch.

Since 0.7.0 (SB-212) every launch also keeps a record of its own, `launches/<launch_id>.json`, because a home is reused and its `.session.json` names only its latest launch. `--history` lists them (`{launches: [...], total}`, at most 500): the same fields plus `launch_id`, `launched_by` (`surface`: `window`, `cli`, `mcp`, `fallback`, `recovery`, `dashboards` or `fabric` for `dispatch` (SB-247); `caller_launch_id`, `caller_project`, `caller_program`, `by`, `parent`), what the session's hooks reported (`provider_session_id`, `transcript`, `turns`, `last_turn_at`, `ended: limit`; `nested_sessions`, the sessions a launched session started inside itself, SB-218) and `continued_as` (where its work went: a later launch or a hand-over), with `continued_from` on the launch that took the work over (SB-219). Each line shows the id, state, agent, who launched it (`cli:voc-learn` with `--by`), folder, time, a limit stop and where the work went. `show LAUNCH_ID` adds the account, group, provider session, transcript path and turns, then *Launched by* (up the chain to the first launch) and *Then* (what it launched or what took its work over). Every launched session carries `SWITCHBOARD_LAUNCH_ID`, so a `switchboard launch` run inside it names it as the caller with no flag. Records are kept 90 days. The fields: [CONTRACTS](CONTRACTS.md#launch-records-and-their-chain-sb-212-2026-10-26).

### Provenance (SB-47)

`switchboard --json provenance --provider <claude|codex> [--pool <id>]` answers which account
serves that group's managed requests (the default group when `--pool` is left out), and
`switchboard --json provenance --launch-id <id>` the account and session of one launch. Fabric
reads it (COM-11; board thread `82241ad5`). Fields: `provider`, `pool`, `account_ref` (the
account's handle `sb-…`, never its id), `identity_revision`, `mode` (`managed` for a group, null
for a launch), `changed_at`, `cause` (`manual`, `rotation`, `limit`, `failover` or `unknown`),
`replaced` (`{account_ref, limit}` or null) and `session` (`{launch_id, dispatch_id, workflow}` for
a launch, else null). MCP: `switchboard_provenance`. Reads only; no credential
([CONTRACTS](CONTRACTS.md#provenance-sb-47-2026-11-05)).

### Setting up with a coding agent (SB-93)

`switchboard onboard` prints the guide a coding agent follows ([docs/AGENT-ONBOARDING.md](AGENT-ONBOARDING.md), built into the CLI). `switchboard onboard --status [--folder <absolute dir>] [--host claude|codex] [--json]` reads where setup stands and writes nothing (it creates no data folder):

```sh
switchboard onboard                         # the guide
switchboard onboard --status                # one line per step, with the command or the person's part
switchboard --json onboard --status --host codex --folder /work/app
```

The answer is `{complete, degraded, host, folder, steps[]}`; each step is `{id, state, summary, next}` in this order: `owner` (the app or `serve` answers), `accounts` (claude, codex and kimi accounts that are switched on), `project` (the folder's project; always optional), `mcp` (a `switchboard` server in the agent's config: Claude Code's `~/.claude.json` user or per-folder scope, the folder's `.mcp.json`, or the enabled Switchboard plugin; Codex's `config.toml` under `CODEX_HOME` or `~/.codex`) and `proving_call` (`mcp-proof.json` in the data folder, written by `switchboard mcp` after its first answered tool call: the client's name and the time). `state` is `done`, `todo` (`next.command` for the agent, `next.tool` for the proving call), `needs_person` (`next.person`, sometimes with a `next.command` the agent may run to start it) or `optional`. `--host` defaults to the agent the command runs inside (`CLAUDECODE=1`, or Codex's `CODEX_SANDBOX`/`CODEX_THREAD_ID`); unknown, any registration counts. Those are the five steps of a computer with no organization. With PassionCode.ai connected and listing no organization, one optional `organization` step follows. With organizations (SB-202), `organization` follows, then per organization, newest first, `org_group` (exactly one group bound to it; `needs_person` otherwise, with `next.command` and `next.alternatives` the agent runs once the person agreed), `seat`, `hook` and `launch`, each carrying `organization: {id, name}`. `next.turn_on` and `next.turn_off` on `hook` are the person's switches (`shell on`, `shell off`), never in `next.command`. `degraded` names reads that failed or were partial (`snapshot`: an owner of another version answered and part of its snapshot could not be read; `sign_ins`; `sessions`); the step each touches says so, and the checklist still answers. Every command the steps name parses as a command of this CLI (`onboard_commands::every_command_the_checklist_names_exists` in `crates/switchboard-cli/src/main.rs`). The MCP tool `switchboard_onboarding` (`path?`, `host?`) answers the same.

### Fallback chains

Which agents continue a workflow, in what order, when its executor's accounts run out (SB-71,
[XA-01](packets/cross-agent-continuation.md)). You set them per machine, per project (its pool) or
per task (one workflow); the narrowest set chain applies, and **none applies until you set one**.

```sh
switchboard chain set --preset subscriptions-first            # claude-code → codex → kimi-code → hermes; after a Claude Code or Codex executor the fallback skips kimi-code and hermes, which ask for no approval (SB-82 D-6)
switchboard chain set --scope project:alpha-web codex claude-code
# an agent may be named by the contract's runner kind (cursor-agent); it is stored as Switchboard's id (cursor-cli), SB-82
switchboard chain set --scope workflow:wf_0123456789abcdef codex@ACCOUNT_ID kimi-code#my-project/prod/OPENROUTER_API_KEY
switchboard chain list [--workflow wf_…] [--pool …]          # every chain, presets, the one that applies
switchboard chain clear --scope workflow:wf_0123456789abcdef
```

An agent is a catalog id (`switchboard agents list`) that loads MCP servers and takes a prompt
without a person; Aider, for example, does not and is refused. `@ACCOUNT_ID` pins an account,
`#project/env/NAME` a paid key by its Observatory vault name, never a key value. Without a pin
Switchboard chooses an account at the switch. Claude Code and Codex run on their own provider's
accounts, Kimi Code on a saved Kimi Code account (`switchboard kimi`, SB-99), Hermes also on the
saved OpenRouter key (SB-107); any other agent on an API-key account or a paid key, never on a
subscription sign-in.
Removing an account or a project drops what pinned it. When a workflow's executor hits its limit,
the automatic fallback walks the chain that applies (0.6.8, SB-73: Claude Code and Codex;
Kimi Code on saved Kimi accounts since SB-99: its first turn runs as `kimi -p`, without approvals,
then `kimi -c` resumes it only when that first turn succeeded; Hermes on the saved OpenRouter key since
SB-107; OpenCode on the same key and its default model since SB-119; an agent disabled on the Agents
page is skipped as `agent_disabled` (SB-120); other agents and keyed pins are skipped until the
project grant they need can be checked, SB-96); `switchboard continue` still hands a workflow over by hand.
`--mode managed` applies to Claude Code and Codex only; with Hermes, OpenCode or Kimi Code it is refused.

### Spending ceilings

Before every paid step of an automatic hand-over (Hermes or OpenCode on an OpenRouter key, Claude
Code or Codex on an API-key account) Switchboard reads the key's spend from OpenRouter and skips
the step when the key has less than $0.50 left of its own limit, when a spending ceiling is
reached (a question opens: double it or keep it), or when a ceiling covers it and its spend cannot
be read (0.7.0, SB-72; [contract](CONTRACTS.md#spending-ceilings-sb-72-070)). A ceiling is a daily dollar amount for this
computer (every saved OpenRouter key added up per UTC day) or for a group on its own key, or one
for a task counted on its key since the task's first paid hand-over.

```sh
switchboard ceiling list                                     # ceilings, open questions, the last hand-over
switchboard ceiling set 5                                    # this computer: $5 a UTC day
switchboard ceiling set --scope project:alpha-web 2          # a group on its own key
switchboard ceiling set --scope workflow:wf_0123456789abcdef 1
switchboard ceiling keep q_0123456789abcdef                  # keep a reached ceiling as it is
switchboard ceiling set 10 --operator-said "подними до 10" --said-at 2026-11-07T22:00:00Z --said-in "session 38adf103"
switchboard ceiling double q_0123456789abcdef --operator-said "…" --said-at … --said-in …
switchboard ceiling clear --scope machine --operator-said "…" --said-at … --said-in …
switchboard continue wf_0123456789abcdef --account hermes --dir /absolute/checkout --ceiling 1
```

From the CLI (and MCP) a first ceiling for a scope or a lower one is accepted; raising, doubling
or clearing one is done in the Switchboard window (*Raising or clearing a ceiling is done in the
Switchboard window. From here, set a first ceiling or a lower one.*; *Doubling a ceiling is
answered in the Switchboard window.*), unless the command carries the operator's own words
(SB-299): `--operator-said` (verbatim, at most 500 characters), `--said-at` (RFC 3339) and
`--said-in` (the session or board thread) open raising, clearing and doubling a ceiling, deleting
a group that holds one and replacing or removing a key one counts. The words are recorded in
`spend.json` (`loosened`, the newest five) and each loosening is logged (`ceiling_loosened`). On
`accounts send` and `accounts hand-over` the same flags record who ordered the seat: every grant is a
line in `seats.jsonl` (SB-292), with the words when the command carries them. `continue --ceiling`
sets the task's ceiling before anything is offered (an amount out of range, or higher than the
one set, refuses the continue); the continue is then gated by it like a hand-over (C-Q8): every
paid start through Switchboard (`continue`, also `--to-console` and on the default agent, onto
Hermes, OpenCode or an API-key account; `agents launch --openrouter`; `agents launch` through the
proxy on an API-key account; `launch <API-key account>`) is refused before anything is offered or
started when a ceiling is reached (*A spending ceiling is reached, so no paid agent starts. Its
question waits in the Switchboard window.*), when a covering ceiling's spend cannot be read (*A
spending ceiling covers this start and its spend could not be read, so no paid agent starts.*) or
when the key has less than $0.50 left of its own limit. Removing or replacing a capped group's
own key (`agents openrouter remove|set --pool <group>`) is the window's (*This group has a
spending ceiling. Remove or replace its own key in the Switchboard window.*, C-Q9). Tests
`ceilings_are_set_lowered_listed_and_kept_from_the_cli_and_never_raised`,
`a_paid_continue_by_hand_past_a_reached_machine_ceiling_is_refused_and_offers_nothing`,
`a_capped_groups_own_key_is_removed_or_replaced_only_in_the_window`.

### Other coding agents

Switchboard's catalog of 30 coding agents ([AGENT-SUPPORT.md](AGENT-SUPPORT.md)) says what each
can use: its MCP tools (`mcp`), the local proxy with an API-key account (`proxy`), or a launch
configured by environment alone (`launch`).

```sh
switchboard agents list                                # id, level (mcp|proxy|launch), name
switchboard agents connect hermes --pool work          # MCP registration and, when it can, proxy settings
switchboard agents key                                 # the agents' proxy key, for an agent's key command
switchboard agents launch goose --pool work --dir /abs/folder
```

The agent id comes from `agents list`. `agents key` prints a local proxy key, not a provider
credential: the proxy accepts it only for the pool's API-key account, never for a Claude
subscription. Keep it out of shared configuration files.

### Installed agents and the default agent (SB-119/SB-120)

```sh
switchboard agents status                 # what is installed, which take over work, the default, the command run
switchboard agents disable kimi-code      # keep an agent out of fallback chains and continue
switchboard agents enable kimi-code
switchboard agents default opencode       # the machine's default agent; `auto` clears it
switchboard agents permissions ask        # sessions Switchboard starts keep their permission prompts; `bypass` is the default
switchboard agents models                 # the recommended OpenRouter models, then every model that can call tools
```

Installed means the agent's program is found the way launches find it: on `PATH`, in the usual
per-user folders (`~/.local/bin`, `~/.bun/bin`, …), Homebrew's, then the agents' own install
folders (`~/.opencode/bin`, `~/.kimi-code/bin`, `~/.hermes/bin`, …); version-manager shims are not
searched. Claude Code, Codex, Kimi Code, Hermes and OpenCode can take over a
workflow; each is enabled for taking over work until you disable it (account switching is not affected), and a disabled one is skipped in
fallback chains (`agent_disabled`) and refused by `continue`. The default agent is *Auto* or one
installed, enabled agent of those five; disabling it returns the machine to *Auto*.
`switchboard continue` without `--account` hands work to the default agent: OpenCode and Hermes on
the OpenRouter key, Claude Code, Codex and Kimi Code on whichever of their accounts has room now.
Fallback chains keep their own order. Fabric can read `default` (and `default_ready`) from
`switchboard --json agents status` unless a project sets its own; `dispatch_agent` names the agent
`switchboard dispatch` would start now (with *Auto*: Claude Code, else Codex, else null). The choices are saved in
`agents-settings.json` (0600) and carried in encrypted backups.
`agents models` asks OpenRouter's public model list (no key, 10 s, kept 6 hours); when OpenRouter
does not answer, the five recommended models and any id you type still work. MCP:
`switchboard_agents` (read).

### Hermes's model and provider (SB-80)

```sh
switchboard agents hermes                       # model, provider and endpoint Hermes runs on
switchboard agents hermes set --provider openrouter --model moonshotai/kimi-k2
```

`set` runs Hermes's own `hermes config set`; the new provider needs its own key inside Hermes.

### Kimi Code accounts (SB-81)

Kimi Code subscription accounts, each signed in by the official `kimi` in a folder of its own.

```sh
switchboard kimi login [--label Work] [--region mainland-cn|global]   # kimi login in Terminal
switchboard kimi finish <id>          # once Terminal says you are logged in
switchboard kimi status <id>          # pending, complete or ended (SB-299)
switchboard kimi list                 # tier and 5-hour / 7-day / monthly usage, asked of Kimi
switchboard kimi launch <id> --dir ~/src/app
switchboard kimi relogin <id>         # when Kimi refused the saved login
switchboard kimi rename <id> "New name" ; switchboard kimi remove <id>
```

`list` also shows the ordinary `kimi`'s sign-in (`~/.kimi-code`), never changed. Switchboard
never copies or renews a Kimi login: when `list` says Kimi renews it on its next run, launch
the account once. Backups do not carry these logins.

### The OpenRouter key for agents (SB-79)

One OpenRouter key, saved once in the OS vault, for agents that can run on OpenRouter. The value
never touches a file, an argument or a log.

```sh
switchboard agents openrouter set --key-stdin [--model moonshotai/kimi-k2]   # key piped on stdin
switchboard agents openrouter status       # saved? default model, what the key may still spend
switchboard agents openrouter model anthropic/claude-sonnet-4.5
switchboard agents openrouter remove
switchboard agents openrouter set --key-stdin --pool client-alpha   # SB-144: the key agents in that project's folders run on
switchboard agents openrouter list         # every saved key with its project, and the projects without one
switchboard agents key --service openrouter --pool client-alpha   # exactly that group's own key (launch scripts call this, SB-213)
switchboard agents key --service openrouter --machine   # exactly the machine's key (launch scripts call this, SB-213)
switchboard agents key --service openrouter --dir /work/app   # the key of that folder's group, else the machine's (scripts written before 0.7.0)
switchboard agents key --service openrouter   # the value, for a session script's export only
```

A session script names the key slot its start's spending check read (`--pool` or `--machine`),
never its folder: a folder that joins another group between the launch and the script's first
line still runs on the key that was checked. A slot whose key was removed meanwhile answers that
no key is saved, and the script stops before the agent starts.

`set` refuses interactive input: pipe the key (`< key.txt`, `pbpaste |`). `status` asks
OpenRouter itself (`GET /api/v1/key`) and shows `limit_remaining` and `usage_daily`; a refused
key stays saved and is named as refused. An agent over MCP gets the same status and sets the
model (`switchboard_openrouter_status`, `switchboard_openrouter_model`), never the key.
Encrypted backups carry the key, so it returns after a reinstall.

Launch an agent on the key in Terminal (no owner needed; the pool does not apply):

```sh
switchboard agents launch hermes --openrouter --dir ~/src/app [--model moonshotai/kimi-k2]
```

`--model` defaults to the key's model. The session script reads the key from `switchboard agents
key --service openrouter` when it starts and exports it under the agent's own variable
(`OPENROUTER_API_KEY`, `KIMI_MODEL_API_KEY` for Kimi Code, `OPENAI_API_KEY` for Qwen Code).
Hermes, pi, omp and Qwen Code start in their own home under Switchboard's data folder, so a key
they saved themselves cannot win. Which agents launch this way, and their caveats:
[AGENT-SUPPORT.md](AGENT-SUPPORT.md) ("OpenRouter key" lines). OpenClaw and Cline cannot.

### Keys from Project Observatory (SB-404)

When Project Observatory holds a provider key (an Anthropic, OpenAI or OpenRouter API key in its
vault), Switchboard lists it by name and saves it without anyone seeing the value:

```sh
switchboard keys observatory                 # every such key by project/env/NAME, and where each is saved here
switchboard keys observatory add alpha-web/prod/ANTHROPIC_API_KEY                  # an account in the pay-per-token group
switchboard keys observatory add alpha-web/prod/ANTHROPIC_API_KEY --to both --as alpha --model claude-sonnet-5-5   # and a named key for Hermes
switchboard keys observatory add beta/prod/OPENROUTER_API_KEY --pool client-alpha   # that group's OpenRouter key
```

The list leaves out organization admin, management, provisioning and BYOK keys and archived
`_PREVIOUS` copies: no agent runs on one. Without Observatory it says so (`state: absent`).

`add` runs Observatory's own `use_secret.py run --vault-only` with a small shell that moves the
value into an unexported variable, removes it from the environment and writes it to the stdin of
the ordinary save (`accounts add --secret-stdin`, `agents anthropic set --key-stdin`, `agents
openrouter set --key-stdin`). The value never appears in an argument, a file, a log or the
answer. `observatory-keys.json` (0600) then records which slot each saved key came from, never a
value.

Where a key goes:

- **Anthropic**: by default an account in the pay-per-token group (DEC-0023), which starts off
  and runs only under its daily ceiling; `--to agent` or `--to both` also saves it under a name
  for Hermes's direct launch (`--anthropic NAME`), which no ceiling counts. A slot already added
  as an account is refused.
- **OpenRouter**: this computer's agents' key, or a group's with `--pool`, under its ceiling. A
  key already saved there is kept unless `--replace`.
- **OpenAI**: listed, not added yet: no ceiling in Switchboard counts an OpenAI key.

Once the pay-per-token group has a daily ceiling, is turned on and holds the folder, Claude Code
and Hermes start on its selected key through the proxy, and every answer is priced against that
ceiling:

```sh
switchboard agents launch claude-code --pool tokens --model claude-sonnet-5-5 --dir ~/src/app
switchboard agents launch hermes --pool tokens --model claude-sonnet-5-5 --dir ~/src/app
```

Hermes needs `--model` there: it starts in a Hermes home of its own whose provider points at the
group's route with the agents' capability, so `~/.hermes` and its sign-ins are left alone.
`agents launch-keys` lists these keys as `Pay-per-token group` with why one cannot start yet.

## Capacity recovery (SB-114)

The running desktop app or `switchboard serve` owns the registry. The host that
knows a session stopped registers its exact binding; Switchboard does not infer
an instruction to resume from arbitrary transcript text.

```sh
switchboard recovery --json
switchboard recovery register limit-1 --session session-1 --account <id> --directory /work/project
switchboard recovery claim limit-1 --account <candidate-id>
switchboard recovery finish limit-1 resumed
# Or when the person cancelled the work:
switchboard recovery finish limit-1 cancelled
```

Status separates `limited_accounts`, `waiting_sessions`, `ready_sessions` and
`claimed_sessions`. `coverage: registered_sessions_only` and
`source: host_reported` are part of the contract: zero registered waits is not
proof that no external session is stopped. `limits` includes hidden-limit evidence.
Each ready row carries its stable stop event and compatible candidates. The host
polls status, notifies the session owner, claims immediately before acting, and
reports resumed only after observing that session continue. Child-agent state
belongs to the host's checkpoint. No command here sends a prompt or restarts a client.

Same-event registration and same-result finish are idempotent. Claim is deliberately
not replayable: after a lost response inspect the existing claim and reconcile the
host's receipt. The registry keeps stops of the last 7 days and at most 512 rows
(D-4 of 0.6.82): an older stop leaves every answer at once and the file at the next
write; past 512, finished stops (resumed, cancelled) go first, then the oldest live
ones, and the stop being written is always kept, so a new stop is never refused for
room. The ids of pruned events are remembered (`recovery-pruned.json`, ids only, the
newest 2048): `register` of a pruned event answers as a replay and stores nothing,
`finish` of one is a no-op, and `claim` of one answers that the stop is not ready.
Each write that dropped rows logs `recovery_pruned` with the count.
See [SB-114 packet](packets/sb-114-capacity-recovery.md) and `recovery::tests`.

Recovery status separates `limited_accounts` (active holds) from
`awaiting_post_limit_probe_accounts` (ended holds awaiting a successful observation
made after the deadline). `limit_unavailable_accounts` includes both. A quota sample
made during a hidden hold cannot make the account ready when its timer ends.

### A limit stop is not a wait (SB-115)

A stop is ready in one of two modes, both in each status row as `mode`:

- `capacity`: a compatible account has a fresh successful quota observation (as above);
- `probe`: no such account, but the stop's backoff has passed. The backoff is 2, 5, 10, then
  15 minutes after the stop, by its `attempt`. Its `probe_at` says when. The provider's
  reset time is never waited for. `candidates` lists the accounts a probe may use, those
  without an active hold first.

A `billing` stop is never probed, and neither is a pool whose every compatible account is
held by a spend or credit cap: a payment is a person's step. A claim records its `mode` and
stays claim-once.

```sh
# Claude Code's StopFailure hook (shipped in the plugin) runs:
switchboard recovery hook < stop-failure.json
```

`hook` reads the hook's JSON on stdin and registers only `rate_limit` and
`billing_error` stops. The account is the one Switchboard charged that session's limit
to, else the ordinary Claude Code's. A second stop of a waiting session keeps its event,
one attempt later. A stop after a claim closes that claim as `resumed`, because the
session ran, and opens a new stop. It always exits 0: `registered: false` with
`reason` `malformed_input`, `not_a_limit` or `refused` (unknown account, folder not
allowed). Since SB-397 W0.4 a stop reported while no owner answers (the app quit, updating or
starting) is not refused: it answers `registered: false, spooled: true` and waits in
`<data>/stop-spool/<session>.json` (0600: session id, folder, billing, time; at most 256, a day at
most), and the owner's next background pass registers it as if the hook had just run, removing the file
only once it answered (a quit in between registers it again, never loses it)
(`a_stop_reported_with_no_owner_is_kept_and_registered_by_the_next_owner`, `stop_spool::tests`).
A stop is kept only when no owner could be asked (`owner_unreachable`: none runs, its store is locked
or its control is busy, the recovery file cannot be read, its control metadata is missing or invalid,
or the request was not attempted); a refusal by an owner that answered is reported as `refused`. A
temporary file in `stop-spool/` that a crash left (`.private-*`, older than 60 s) is removed.
The owner logs `recovery_hook` with a result code (`spooled` for a kept stop). The rule for
agents is fabric-workspace
[principle 10, item 6](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/principles.md#10-a-free-agent-takes-the-next-task).
See [SB-115 packet](packets/sb-115-limit-stop-resume.md); `recovery::tests` and
`recovery_hook_never_fails_the_session`.

Since 0.6.83 the plugin also runs the same command on `Stop`, after every finished turn
(SB-204): a session that went on by itself (its traffic moved to another account, it
continued at the reset, the person sent the prompt again) has its waiting or claimed stop
marked `resumed`. The command reads the file first and asks the owner only when that
session has a stop, and prints `{}`. A stop older than a day reads as
`cancelled` (SB-199). A stop follows its account into another group, and a waiting stop
that is not ready says why in `reason`: `billing`, `account_removed`, `no_account` or
`backoff` (SB-205).

## Console hand-overs (SB-108)

```sh
switchboard handovers --json
switchboard handovers set on
switchboard handovers run ho-0123456789ab
switchboard handovers run ho-0123456789ab --ask
switchboard handovers set off
switchboard continue wf_0123456789abcdef --account opencode --dir ~/src/app --to-console
```

Off by default. With the setting on, automatic fallback prepares a continuation
and queues metadata for the host console; `continue … --to-console` (0.6.32) does the same for one
workflow and account a person names, and is refused before anything is offered while the setting
is off. `run` requires a terminal on stdin and
stdout and takes the record once. After 120 seconds without a taker, the monitor
requests a Terminal launch while the offer is live. Expired offers never launch.
The private queue stores no credential or prompt. Fabric Dashboards' consumer
(FD-62) is separate from this CLI producer; an available command is not evidence
that the installed console has consumed it. Checks: `handovers::tests` and
`hand_overs_are_listed_and_set_offline_and_run_only_in_a_terminal`.

`run --ask` (SB-405, asked by Fabric Dashboards for a console that holds its folder) starts the
session with the agent's permission prompts on, whatever `agents permissions` says: Claude Code
runs with `--permission-mode manual` and without `--dangerously-skip-permissions`; Codex runs
without Switchboard's `approval_policy="never"` and `sandbox_mode="danger-full-access"` overrides
(or `--dangerously-bypass-approvals-and-sandbox`) and with `-c approval_policy="on-request"`, and
a sandbox the prepared session names otherwise (a person's `read-only`) is kept, none added. The
session runs from `launch-ask.command` beside the prepared script, which is left as it was, and its
launch record and its `sessions --all` row carry `"permissions": "ask"`. Only a Claude Code or Codex
hand-over can be taken this way: another (Kimi Code, Hermes, OpenCode) is refused and stays
waiting, as is one whose script cannot be read as Switchboard wrote it. Without `--ask` nothing
changes. Checks: `a_hand_over_taken_with_ask_starts_with_prompts_on_and_its_record_says_so`,
`launch::tests::a_hand_over_taken_with_ask_runs_claude_code_in_manual_mode_without_the_bypass`,
`a_hand_over_taken_with_ask_runs_codex_asking_and_keeps_a_sandbox_the_person_set`,
`ask_is_refused_for_another_agent_or_a_script_switchboard_did_not_write`.
