# Move Switchboard to another Mac, or add a macOS user

[Русская версия](MOVING.ru.md)

This guide is for two moves: your work goes to another computer, or another person gets their own
macOS user on the same Mac. At the end, Switchboard on the new computer (or in the new user) has
your groups, organizations and settings, its own signed-in accounts and its own keys, and the old
one no longer uses the seats you took with you.

Switchboard moves its settings in one file, which never holds an account, a credential, a key or a
secret. Accounts go one of two ways: each sign-in is moved over ssh with `accounts send` (no new
sign-in, and only one computer holds it afterwards), or it is signed in again on the new computer.
You do the sign-ins and type the secrets yourself; your coding agent can run every other command
here.

When the old computer can reach the new one over ssh without a prompt, the short way is
[Over ssh](#over-ssh-no-new-sign-in) below; the file and the sections after it are the way without
ssh.

## What the settings file carries

- Groups, by the repositories they hold (`host/owner/name`), so the new computer finds its own
  checkouts of them, and each group's GitHub sources.
- Organizations with their domains and the Claude and ChatGPT teams they follow.
- The agents' settings (the default agent, the ones you turned off) and automatic switching.
- The Fabric Inbox key's address and Client Id, never its secret.
- The terminal hook's on or off, when you chose one.
- The spending ceilings of the computer and of its groups.

What it does not carry, and you set up again: accounts and their sign-ins, the OpenRouter key and
other agent keys, the Fabric Inbox secret, the PassionCode.ai connection, folder rules and
fallback chains (they name accounts of the old computer), and the history and statistics.

## Over ssh, no new sign-in

From the computer that holds the accounts, with the new one reachable as `user@host` (an ssh key,
no password prompt):

1. Carry the groups, organizations and settings, look at what it would do, then apply it:

   ```sh
   switchboard settings sync --to user@host
   switchboard settings sync --to user@host --apply
   ```

   `--group <group id>` (repeat it) carries only those groups and what they need. The file is
   signed with this computer's key and imported there; no account goes with it.

2. Move each account into its group there:

   ```sh
   switchboard accounts send <account id> --to user@host --pool <group id there>
   ```

   The account is marked used elsewhere here first, so this computer never renews it again; it is
   saved there, checked with a quota read, and both steps are undone if anything fails. Only a
   Claude or Codex subscription sign-in that no group has selected moves this way.
   `switchboard accounts seats` lists what this computer sent, where and when; the record stays
   on the computer that sent it, so send from the one that holds the account.

3. To take one back: `switchboard accounts recall <account id> --from user@host`. It stops being
   used there, its tokens come back over ssh, the provider is checked here, then its copy there is
   removed.

Then continue with steps 4 to 6 of "On the new computer" (keys, PassionCode.ai, the check), run
on the new computer.

## Machine groups

Every other machine your accounts live on is a group of its own here, one per ssh destination
([DEC-0013](DECISIONS.md#dec-0013-an-account-on-another-machine-belongs-to-that-machines-group-and-to-no-group-here)).
When `switchboard accounts send` moves an account and its copy there answers, the account joins
that machine's group and leaves every group on this computer: it is never selected, switched to,
lent or renewed here. The Groups page lists each machine under *Other machines* with its accounts;
*Read their state* reads each machine's Switchboard over ssh.

```sh
switchboard machines list
switchboard machines list --live
switchboard accounts send <account id> --to studio-sshlg --pool agents
switchboard accounts recall <account id> --pool agents
switchboard accounts send <account id> --to passioncode@100.69.20.19
```

The first two list the machines and their accounts (`--live` reads each one, 20 seconds per ssh
call at most, all at once). `send` moves an account there; `recall` brings it back into the group
named here (by default the group it left, else the default group; `--from` defaults to the
machine the move named). A `send` of an account that lives on another machine calls it back
first, then moves it, so it goes from one machine to the other in one step; if the move fails it
stays here. Accounts marked as used elsewhere before this version join their machine once, at the
first start (or by hand: `switchboard machines adopt`, then `switchboard machines adopt --apply`),
by the move recorded in `seats.jsonl`, else by their location naming a known machine; one whose
machine cannot be told stays where it is and is named. To record where an account lives
without ssh, and to remove an empty machine group:

```sh
switchboard machines place <account id> --on studio-sshlg
switchboard machines forget studio-sshlg
```

### How the machines are linked, and what Switchboard knows about them

- **ssh is the only link.** Every step on another machine is `ssh <name> -- <its switchboard>
  --json …`, run with `BatchMode=yes` and `ConnectTimeout=15`, so a machine that would ask for a
  password is never used ([account_send.rs:652](../crates/switchboard-runtime/src/account_send.rs#L652),
  [account_send.rs:101](../crates/switchboard-runtime/src/account_send.rs#L101)). The name is what
  your `~/.ssh/config` and keys make of it: an alias such as `studio-sshlg`, or `user@host`; on the
  operator's machines the host is a Tailscale address (100.64.0.0/10), which ssh reaches like any
  other. Switchboard knows nothing of Tailscale or keys: it passes the name to ssh, refusing only a
  name ssh would read as an option ([account_send.rs:89](../crates/switchboard-runtime/src/account_send.rs#L89)).
- **One machine, several names.** Two names are one machine when `ssh -G` resolves them to the same
  user, host and port ([targets.rs:126](../crates/switchboard-runtime/src/targets.rs#L126)), so
  `studio-sshlg` and `sshlg@100.69.20.19` are one group. Two macOS users on one Mac Studio are two
  machines: two Switchboards, two data folders, two groups.
- **Switchboard does not prove a machine is yours.** It trusts whatever ssh reaches without a
  prompt and the answers that machine's `switchboard` prints
  ([remote_check.rs:54](../crates/switchboard-runtime/src/remote_check.rs#L54)). That you own it is
  what your ssh keys and your Tailscale network say, nothing more. A settings file it sends is
  signed with this computer's key ([settings_signing.rs:112](../crates/switchboard-runtime/src/settings_signing.rs#L112)),
  but the other machine's own key is not checked. What would prove it: pin the other machine's
  settings key (`switchboard settings signer` there) at the first move and refuse a later ssh call
  that answers with another key (board row SB-356).
- **Nobody else sees it.** Machine groups, the moves (`seats.jsonl`) and the record of the one
  pass (`machine-groups.json`) live only in this user's data folder, readable by this user alone:
  the folder is 0700 and every file 0600
  ([persistence.rs:97](../crates/switchboard-core/src/persistence.rs#L97),
  [persistence.rs:128](../crates/switchboard-core/src/persistence.rs#L128),
  [seats.rs:111](../crates/switchboard-runtime/src/seats.rs#L111)). Nothing is sent to a server:
  PassionCode.ai is asked only for your organizations, with a token whose one scope is
  `me.organizations` ([vesta.rs:22](../crates/switchboard-runtime/src/vesta.rs#L22),
  [vesta.rs:420](../crates/switchboard-runtime/src/vesta.rs#L420)), and the usage analytics carry
  counts and kinds only ([ANALYTICS.md](ANALYTICS.md)). On the other machine the account is an
  ordinary account of that macOS user's Switchboard, seen by whoever signs in as that user there.
- **Delegation is a move, recorded.** `accounts send` marks the account here as used on that
  machine before anything leaves, saves it there with `accounts add --secret-stdin` (the tokens on
  stdin, never in a command line), checks it there with a quota read and undoes both on any failure
  ([account_send.rs:149](../crates/switchboard-runtime/src/account_send.rs#L149),
  [account_send.rs:237](../crates/switchboard-runtime/src/account_send.rs#L237)); `accounts recall`
  has that machine give the sign-in back over ssh, checks it here and removes the copy there
  ([account_send.rs:498](../crates/switchboard-runtime/src/account_send.rs#L498)). Each move and
  call back is a line in `seats.jsonl` with who ordered it: `operator` with their words verbatim,
  else `agent` ([seats.rs:19](../crates/switchboard-runtime/src/seats.rs#L19),
  [seats.rs:42](../crates/switchboard-runtime/src/seats.rs#L42)). Giving an account to an agent on
  this machine (`switchboard accounts assign <account id> --to 'Frame Agent'`) is the other kind of
  delegation: the account stays here, renewed and observed, and nothing here chooses it by itself
  ([lib.rs:2267](../crates/switchboard-core/src/lib.rs#L2267)).

## On the old computer

1. Write the settings file:

   ```sh
   switchboard settings export > switchboard-settings.json
   ```

   Take it to the new computer any way you like: it holds no secret.

2. When you are moving to another computer for good, mark each account you will sign in to there
   as used elsewhere, so this computer never renews it or switches to it:

   ```sh
   switchboard accounts list
   switchboard accounts update <account id> --elsewhere true
   ```

   Two computers renewing one Claude sign-in sign each other out. `--elsewhere false` brings an
   account back here.

## On the new computer

1. Install Switchboard as [INSTALL.md](INSTALL.md) describes and open it once, then check:

   ```sh
   switchboard --version
   ```

2. See what the settings file would do, then apply it. `--under` names the folder that holds your
   checkouts (by default `~/DATA`); with `--clone`, repositories on GitHub that are not checked out
   yet are cloned into it with your GitHub CLI (`gh`):

   ```sh
   switchboard settings import switchboard-settings.json --under ~/code
   switchboard settings import switchboard-settings.json --under ~/code --apply
   switchboard settings import switchboard-settings.json --under ~/code --apply --clone
   ```

   The preview names each part: the groups and the folders found for them, what is missing, the
   organizations, the ceilings and what happens to each, and the Fabric Inbox command. Import
   never raises a spending ceiling, and sets the terminal hook only where this computer has no
   choice yet; the app applies the hook at its next start.

3. Sign in to your accounts, each into its group:

   ```sh
   switchboard project list
   switchboard login begin --provider claude --pool <group id>
   switchboard login status <login id>
   switchboard login finish <login id>
   ```

   A Codex account uses `--provider codex`. An account already signed in here moves into a group
   with `switchboard accounts move <account id> --pool <group id>`.

4. Enter your keys again. Each is read from standard input, never from the command line:

   ```sh
   pbpaste | switchboard agents openrouter set --key-stdin
   pbpaste | switchboard agents openrouter set --key-stdin --pool <group id>
   ```

   The second form is a group's own key; a group's spending ceiling waits for it. For the Fabric
   Inbox key, copy its secret and pipe it into the command the import printed, then check it:

   ```sh
   pbpaste | switchboard inbox set --origin https://inbox.example.com --client-id <client id>
   switchboard inbox status --check
   ```

5. If you use PassionCode.ai, connect it again; approving the browser page is yours:

   ```sh
   switchboard org connect
   ```

6. Check where the setup stands:

   ```sh
   switchboard --json onboard --status
   ```

   Every step reads `done` or `optional` when the new computer is ready.

## A settings file from your organization's administrator

An administrator can sign the settings file, so each member's computer can check that the file is
the one the administrator wrote. On the administrator's computer, print the key once and publish
it to the members, then sign each file:

```sh
switchboard settings signer
switchboard settings export --sign > switchboard-settings.json
```

The private half of the key stays in that computer's vault. A member imports with the published
key; a file that key did not sign, or one changed after signing, is refused and nothing is
applied:

```sh
switchboard settings import switchboard-settings.json --signer <published key> --apply
```

## Another macOS user on the same Mac

Each macOS user runs their own Switchboard: its data folder, Keychain items, log and login item are
that user's own, and nothing is shared with the other user's copy.

1. Either each user installs the app into `~/Applications` (the `Applications` folder in their own
   home), or one copy in `/Applications` serves every user, as on a Mac where each user runs
   `switchboard serve` under its own LaunchAgent: a `serve` updates the shared app when its user
   may replace it, and every other `serve` restarts onto the new program by itself
   ([DEC-0011](DECISIONS.md#dec-0011-serve-keeps-itself-current-and-follows-an-app-another-account-replaced)).
2. Continue with "Over ssh" above (the other user as ssh knows it, such as `other@localhost` with
   Remote Login on), or with "On the new computer" from step 2. Each user holds their own seats:
   one seat in two users' Switchboards renews twice and signs itself out, so move an account with
   `accounts send` rather than signing in to it twice.

The two copies never share a port or a key: each proxy takes its own port, and refuses a
connection that comes from the other user's programs, even one that carries a valid key. A folder
both users work in keeps the Switchboard route of the user who owns it; the other user's sessions
there use their own sign-in.

## A Linux VM

A virtual machine (for example a Linux arm64 VM on a Mac Studio) runs a Switchboard of its own and
holds its own seats: one sign-in is active on one machine at a time, and the host's proxy serves
only the host user's own programs, never a connection from the VM. Give the VM accounts of its own
rather than moving the only account of a group the host still uses.

1. On the VM, without a desktop: the CLI from `Fabric-Switchboard-X.Y.Z-linux-<arch>-cli.tar.gz`,
   or the AppImage ([INSTALL.md → Linux](INSTALL.md#linux)). With no session bus there is no
   keyring, so keep sign-ins in Switchboard's own 0600 files:

   ```sh
   switchboard vault use file
   ```

2. Run `switchboard serve` as a systemd user service with `Restart=on-failure`: it checks for
   updates every five minutes and restarts onto a new program by itself (DEC-0011);
   `switchboard update --app <path to the AppImage>` updates it by hand.
3. On the host, in the user that holds the accounts: give the VM's work a group of its own there,
   then `settings sync --to user@vm --group <group id> --apply` and
   `accounts send <account id> --to user@vm --pool <group id>` as in "Over ssh". The host keeps the
   record of what it sent.

## If something goes wrong

- The import names each part it could not do under `refused`; nothing else is undone. Run it
  again after fixing the cause: what is already there stays as it is.
- An account that keeps asking you to sign in again may still be renewed on the old computer:
  mark it `--elsewhere true` there (an account moved with `accounts send` is marked already).
- To go back, the old computer still has everything: run
  `switchboard accounts update <account id> --elsewhere false` there for each account you marked.
