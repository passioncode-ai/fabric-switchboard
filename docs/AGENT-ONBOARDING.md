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
   - `todo` — run `next.command` yourself (or, for `proving_call`, call the MCP tool
     `next.tool`).
   - `needs_person` — tell the person `next.person` in your own words. When `next.command` is
     present, you may run it to start the step for them (for example, it opens the sign-in in a
     Terminal window), then wait for them.
   - `optional` — offer it once; do it only if the person wants it.
3. Run `switchboard onboard --status --json` again after every step. It reads the computer each
   time and writes nothing, so it is always safe to repeat.

## The steps

| Step | What it checks | Whose |
|---|---|---|
| `owner` | The desktop app or `switchboard serve` is running | the person opens the app |
| `accounts` | At least one account is saved | the person signs in; you may start `switchboard login begin` |
| `project` | The folder belongs to a project (accounts reserved for its folders) | optional; ask first |
| `mcp` | The `switchboard` MCP server is registered with you | you |
| `proving_call` | A Switchboard tool answered a coding agent | you |

## Rules

- Never ask the person to paste a token, password or key into the conversation, and never put
  one in a command line. Sign-ins happen in the provider's own sign-in, in a Terminal window.
- `switchboard accounts capture` saves the account the ordinary `claude` or `codex` is signed in
  to. Run it only when the person says to.
- A project's accounts serve only sessions started in its folders, and a folder in a project
  with no account of a provider cannot start that provider's sessions. Create one only when the
  person wants accounts kept for it, and name the accounts (`switchboard accounts list`).
- Changing the ordinary Claude Code login affects every `claude` session on the computer. Never
  do it during setup.
- A server you register during a session is loaded by the next session. If `switchboard_accounts`
  is not among your tools after registering, ask the person to start a new session, and finish
  the proving call there.

When every step is `done` or `optional`, tell the person what is set up: the accounts saved, the
project (if any), and that you can now read their accounts and remaining usage through
Switchboard.
