# Set up your organization's work in Switchboard

[Русская версия](ORGANIZATION.ru.md)

This guide is for a person who works for an organization: a new employee, or someone adding a
company's work to their own computer. At the end, the organization's group holds your own
signed-in seat, terminals in its folders work in its context, and one session proved it. Your
own accounts and history stay apart from the company's.

You do the browser consent and the sign-in yourself. Everything else can be done in the window,
in a terminal, or by your coding agent (ask it to run `switchboard onboard` and follow it).

The window, your agent and this guide read the same checklist. At any step, this command shows
where you are:

```sh
switchboard --json onboard --status
```

Each step in its answer reads `done`, `todo`, `needs_person` (yours) or `optional`. When setup is
complete, `data.complete` is `true`.

**What you need first**

- An invitation to your organization's team at the provider (Claude Team, or a ChatGPT
  workspace), sent to your organization address. It reaches your own mailbox if your
  organization forwards that address to you.
- Claude Code or Codex installed. Switchboard does not include a subscription.
- For step 3, either a settings file from your organization's administrator, or the folders
  where you keep the organization's code.

## 1. Install Switchboard

Download the app for your system from the [download page](https://passioncode.ai/switchboard/#download)
and install it as [INSTALL.md](INSTALL.md) describes, or let the PassionCode.ai setup command
install it with the other PassionCode.ai tools:

```sh
npx --yes @passioncode-ai/passioncode@latest setup --yes --json
```

Open the app once. It asks whether it may share anonymous usage counts (you can change the answer
later in About), then shows a short tour. From then on it opens at login
and keeps running when you close its window.

Put the `switchboard` command on your `PATH` so you and your agent can use it: on macOS,
Agents → *Link switchboard into ~/.local/bin*; on Linux the .deb puts it in `/usr/bin`; on
Windows, `switchboard.exe` is installed beside the app and the Agents page shows its path.

**Check:** `switchboard --version` prints the version, and `switchboard --json onboard --status`
answers with its steps.

## 2. Connect PassionCode.ai

If your organization uses PassionCode.ai, connect it: *Organizations* → *Connect
PassionCode.ai…*, or in a terminal:

```sh
switchboard org connect
```

Your browser opens the PassionCode.ai consent page; approving it is yours. Your organizations then
arrive with their names and domains, and Switchboard reads them again every 6 hours.

If your organization does not use PassionCode.ai, add it by hand: *Organizations* →
*+ New organization*, or `switchboard org add "Acme" --domain acme.example`.

**Check:** `switchboard --json org passioncode` answers `"connected": true` and `"state": "ok"`;
`switchboard org list` names your organization; in `onboard --status` the step `organization`
reads `done`.

## 3. Your organization's group

A group is a set of project folders with accounts of its own. Sessions in its folders use only
its accounts. Your organization needs one group on this computer.

Once the organization exists, the window shows the card *Set up {organization} on this computer*
above Accounts and at the top of *Organizations*. It lists the five steps of this guide and offers
one action at a time.

- **With a settings file from your administrator:** it holds the group and its repositories, but
  never an account. First look at what it would do, then apply it:

  ```sh
  switchboard settings import <settings file> --under <your code folder>
  switchboard settings import <settings file> --under <your code folder> --apply
  ```

  `--under` names the folder that holds your checkouts; Switchboard finds the group's
  repositories there.
- **Without one:** on the card, *+ Group for {organization}* opens the group dialog with the
  organization chosen. Or, in a terminal, make the group and bind it:

  ```sh
  switchboard project save --name "Acme" --folder <absolute folder of a repository>
  switchboard project edit --pool <group id> --organization <organization id or name>
  ```

  `switchboard project list` shows the group id; `switchboard org list` the organization's id.

**Check:** in `onboard --status` the step `org_group` reads `done` and names the group.

## 4. Your seat

Accept the provider's invitation from your mailbox. Then sign in to that seat into the group:
on the card, *Sign in with your {organization} address*; or in a terminal:

```sh
switchboard login begin --provider claude --pool <group id>
switchboard login status <login id>
switchboard login finish <login id>
```

The provider's official sign-in opens in a terminal window; the sign-in is yours. A Codex seat
uses `--provider codex` (in the window: *+ Add account* → *Sign in to another Codex account*).
The account gets your organization's badge, for example by its address's domain.

If you signed in somewhere else first, the notice offers *Move it to “{group}”*. Switchboard never
moves an account without you.

**Check:** in `onboard --status` the step `seat` reads `done`.

## 5. Terminals in the organization's folders

When a group belongs to an organization, the installed app adds one marked block to the start
file of each shell on this computer, so that `claude` and `codex` typed in a terminal in the
group's folders run in the organization's context: its own home for Claude Code and Codex
(history, settings, sign-in) and `SWITCHBOARD_CONTEXT=org:<id>`. Leaving the folder gives your
own settings back. The app says so once, in a notification. This is on by default; nothing to do.

New terminals pick it up; a terminal that was already open does not, so open a new one.

```sh
switchboard shell status     # whether it is on, whether this terminal has it, this folder's group
switchboard shell off        # take the block out of every start file and keep it out
switchboard shell on         # put it back
```

`shell off` gives every start file back byte for byte. The window has the same switch: Groups →
*Terminals in organization folders* → *Turn off* or *Turn on*.

Good to know:

- Claude Code gets the organization's home in the group's folder itself, where Switchboard's route
  file lives; in a subfolder, `claude` keeps your own settings. Codex follows the group in
  subfolders too.
- A group with no Codex account leaves `codex` on your own sign-in there; `shell status` says so.
- A start file that is a link (a dotfile manager's), not yours or not text is left alone;
  `shell status` gives the line to add yourself (`switchboard shell init <shell>`).
- Scripts and editors that start Claude Code without an interactive shell do not load the hook;
  their account still follows the group's folder.

**Check:** `switchboard shell status` says the hook is on and, in a new terminal, "This terminal:
the hook is loaded."; in the group's folder, `echo $SWITCHBOARD_CONTEXT` prints `org:<id>`. In
`onboard --status` the step `hook` reads `optional` with `"terminals": "active"`.

## 6. Launch work

Work as usual in the group's folders. To prove the setup, launch one session from Switchboard (a
plain `claude` in a terminal is not a session Switchboard launched, so it is not recorded):

```sh
switchboard launch --provider claude --mode managed --working-directory <a folder of the group>
```

Or use *Launch…* on the seat's card in the window.

**Check:** `switchboard sessions` lists the session with your organization's name, and Activity →
*Sessions* shows its badge. In `onboard --status` the step `launch` reads `done`, and
`data.complete` is `true`.

## 7. Leaving the organization

- **Your seat is removed.** When the provider refuses the seat's sign-in for good, its card says:
  "{label}'s sign-in has ended. If {organization} removed your seat, ask its administrator; to
  keep using it, sign in again." Switchboard stops choosing it. If every seat of the group ended,
  the group's card says that sessions in its folders are refused until one is signed in.
  **Check:** in `onboard --status` the step `seat` reads `needs_person` and counts the ended seats.
- **You leave the organization on PassionCode.ai.** After the next read, its card says since when
  it is no longer among your PassionCode.ai organizations. **Check:** `switchboard --json org
  passioncode` lists it under `released` with the date.
- **What stays on your computer.** Switchboard deletes nothing when you leave: the organization,
  its group (which still refuses your other accounts), the account, its statistics, and the
  organization's homes with their history stay until you remove them.
- **Removing it.** *Organizations* → *Remove…* on its card (or `switchboard org remove
  <organization id>`): its groups become personal. `switchboard shell off` takes the terminal
  block out. `switchboard uninstall` shows what it would remove from the computer; add `--yes` to
  remove it.

## On Windows, macOS and Linux

Not yet tried on a person's Windows computer: the Windows steps below are built and pass the
nightly tests on Windows runners only.

| Step | macOS and Linux | Windows |
|---|---|---|
| 1. Install | macOS: the app in Applications, `switchboard` linked into `~/.local/bin`. Linux: the .deb, `switchboard` in `/usr/bin` | the installer for your user account; `switchboard.exe` beside the app (the Agents page shows its path) |
| 2. Connect | the default browser opens the consent page | the same |
| 3. Group | `--under` names your code folder, such as `~/code` | `--under` names your code folder, such as `C:\Users\you\code` |
| 4. Seat | sign-ins are kept in the Keychain (macOS) or the Secret Service (Linux) | sign-ins are kept in the Windows Credential Manager |
| 5. Terminals | zsh `.zshrc`, bash's login file on macOS or `.bashrc` on Linux, fish's own file in `~/.config/fish/conf.d` | the profile `Microsoft.PowerShell_profile.ps1` of Windows PowerShell 5.1 and of PowerShell 7, in your Documents folder (also when OneDrive moved it). Where the execution policy is `Restricted` or `AllSigned`, profiles do not load: Switchboard leaves the profile alone, never changes the policy, and `shell status` says `policy_blocked`. Where PowerShell runs in Constrained Language Mode, it says `constrained_language`. Command Prompt and Git Bash are not covered. In the group's folder, `$env:SWITCHBOARD_CONTEXT` prints `org:<id>` |
| 6. Launch | the session opens in Terminal (macOS) or your terminal app (Linux) | the session opens in a new PowerShell window |
| 7. Leave | the same | the same |

Behind each step: the checklist is `onboarding::steps` in
[crates/switchboard-runtime/src/onboarding.rs](../crates/switchboard-runtime/src/onboarding.rs);
the commands are in [CLI](CLI.md); the contracts in [CONTRACTS](CONTRACTS.md); the scenarios
SCN-074 to SCN-079 in [scenarios](ux/scenarios.md).
