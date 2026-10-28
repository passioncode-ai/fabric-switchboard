# Set up Fabric Switchboard

You are a coding agent setting up Fabric Switchboard for the person you work with. Do every step
you can yourself; stop only where a step is the person's, and tell them exactly what to do there.

Switchboard keeps several Claude Code, Codex and Kimi Code accounts on this computer, switches
between them when one runs out, and gives coding agents its tools over MCP.

## How to work

1. Run `switchboard onboard --status --json` (add `--folder <absolute project folder>` when
   you work in one). It answers `steps` in order, each with an `id`, a `state`, a `summary`
   and `next`. `complete: true` means you are done.
2. For each step that is not `done`:
   - `todo`: run `next.command` yourself (or, for `proving_call`, call the MCP tool
     `next.tool`).
   - `needs_person`: tell the person `next.person` in your own words. When `next.command` is
     present, you may run it to start the step for them (for example, it opens the sign-in in a
     Terminal window), then wait for them.
   - `optional`: not required to finish; run its `next.command` only when you have not already
     completed it.
   - `next.turn_on` and `next.turn_off` are the person's switches, never a step for you: run
     one only when the person asks for it.
3. Run `switchboard onboard --status --json` again after every step. It reads the computer each
   time and writes nothing, so it is always safe to repeat. When `degraded` is not empty, part of
   what the steps read could not be read (`snapshot`, `sign_ins`, `sessions`); the step it touches
   says so in its summary.

## The steps

| Step | What it checks | Whose |
|---|---|---|
| `owner` | The desktop app or `switchboard serve` is running | the person opens the app |
| `accounts` | At least one account is saved | the person signs in; you may start `switchboard login begin` |
| `project` | The folder belongs to a project; an eligible Git folder registers on agent open | optional; automatic, or run `next.command` |
| `mcp` | The `switchboard` MCP server is registered with you | you |
| `proving_call` | A Switchboard tool answered a coding agent | you |

On a computer with an organization (added by hand, or from PassionCode.ai) more steps follow,
for each organization, newest first; each carries `organization: {id, name}`. A computer with
no organization has only the five steps above. With PassionCode.ai connected and no
organization listed for the person, one optional `organization` step says so.

| Step | What it checks | Whose |
|---|---|---|
| `organization` | At least one organization exists on this computer. One the person left on PassionCode.ai (and not linked again) gets no steps of its own: it is named under `left` (`[{id, name}]`) and in the summary, and is not set up again (SCN-079; test `onboarding::tests::an_organization_left_on_passioncode_is_not_set_up_again`) | done, or optional |
| `org_group` | The organization has exactly one group bound to it | the person: ask whether the administrator gave a settings file (then you run `next.command`) or which folders the group takes (then you run `next.alternatives`); with two groups, the person says which one stays, then you run `next.command` on the others |
| `seat` | That group holds an account of the organization that is switched on, not used on another machine and still signed in | the person signs in with the organization's address; you may start `next.command` (the sign-in opens in the group) |
| `hook` | Whether terminals in the group's folders work in the organization's context (`switchboard shell status`): on by default, and working once a start file holds the hook | optional. `next.turn_off` turns it off and `next.turn_on` turns it back on: never run either unless the person asks. `next.command` (`shell status`) only reads |
| `launch` | A session ran in the organization's context (`org:<id>`) | optional: `next.command` launches one in a folder of the group |

The window's organization card (SCN-076) reads the same steps through the app and words them in
its own language from three more fields: `org_group.groups` (the groups' names, with two or more
bound), `seat.ended` (how many of the group's accounts of the organization must sign in again)
and `hook.terminals` (`active`, `pending`, `off`, `waiting` or `unread`); test
`onboarding::tests::the_organization_steps_carry_what_the_window_words`.

## Rules

- Never ask the person to paste a token, password or key into the conversation, and never put
  one in a command line. Sign-ins happen in the provider's own sign-in, in a Terminal window.
- `switchboard accounts capture` saves the account the ordinary `claude` or `codex` is signed in
  to. Run it only when the person says to.
- An eligible unassigned Git repository becomes a project when a coding agent opens it; no account
  IDs move. For a provider with no enabled account in that project, ordinary account launches keep
  their existing fallback; automatic `--provider` selection still requires the project's selected
  account. Never move accounts during setup.
- A sign-in that lands outside its organization's group comes back with `organization_group`
  in the `login finish` (or `accounts capture`) receipt. Tell the person; move the account
  (`switchboard accounts move <id> --pool <group>`) only when they say so. With `in_cli: true`
  it is the ordinary CLI's account, which joins a group only after the CLI switches.
- Changing the ordinary Claude Code login affects every `claude` session on the computer. Never
  do it during setup.
- A server you register during a session is loaded by the next session. If `switchboard_accounts`
  is not among your tools after registering, ask the person to start a new session, and finish
  the proving call there.

When every step is `done` or `optional`, tell the person what is set up: the accounts saved, the
project (if any), and that you can now read their accounts and remaining usage through
Switchboard.
