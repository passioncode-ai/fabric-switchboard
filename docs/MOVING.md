# Move Switchboard to another Mac, or add a macOS user

[Русская версия](MOVING.ru.md)

This guide is for two moves: your work goes to another computer, or another person gets their own
macOS user on the same Mac. At the end, Switchboard on the new computer (or in the new user) has
your groups, organizations and settings, its own signed-in accounts and its own keys, and the old
one no longer uses the seats you took with you.

Switchboard moves its settings in one file. The file never holds an account, a credential, a key
or a secret, so everything that signs in is entered again on the new computer. You do the
sign-ins and type the secrets yourself; your coding agent can run every other command here.

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

Each macOS user runs their own Switchboard: its app, data folder, Keychain items, log and login
item are that user's own, and nothing is shared with the other user's copy.

1. Sign in to the new macOS user and install Switchboard there into `~/Applications` (the
   `Applications` folder in that user's home), so each user has a copy of their own.
2. Continue with "On the new computer" above, from step 2, with the settings file of the user who
   already set Switchboard up. Each user signs in to their own seats: one seat in two users'
   Switchboards renews twice and signs itself out.

The two copies never share a port or a key: each proxy takes its own port, and refuses a
connection that comes from the other user's programs, even one that carries a valid key. A folder
both users work in keeps the Switchboard route of the user who owns it; the other user's sessions
there use their own sign-in.

## If something goes wrong

- The import names each part it could not do under `refused`; nothing else is undone. Run it
  again after fixing the cause: what is already there stays as it is.
- An account that keeps asking you to sign in again may still be renewed on the old computer:
  mark it `--elsewhere true` there.
- To go back, the old computer still has everything: run
  `switchboard accounts update <account id> --elsewhere false` there for each account you marked.
