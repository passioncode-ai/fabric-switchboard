---
report: fabric-switchboard estate flows 2026-11-06
project: fabric-switchboard
date: 2026-11-06
source: board thread eefa7959 (fabric-workspace, operator 2026-11-06 about 12:50 UTC)
---

# Fabric Switchboard in the estate flows (S1 to S10)

Switchboard's part of the common reconciliation. Main at `6a9df88`: v0.7.21 tagged on `6c2f6a3`
(its release run in progress at the time of writing) plus SB-284 (PR #454), merged and not yet
released. Paths are from the repository root.

## S1. Where it runs

- **One copy per macOS user.** The app and `switchboard serve` own one data folder per user
  (`~/Library/Application Support/ai.passioncode.fabric-switchboard`), its own proxy port and
  token (`proxy.json`) and a store lock (`instance.lock`); two users on one Mac never share them
  (SB-251, `docs/MOVING.md` "Another macOS user on the same Mac"). The proxy refuses connections
  from another macOS user (SB-273/274, `crates/switchboard-proxy/src/peer.rs`).
- **Residency.** The app stays resident and starts at login (`AGENTS.md` Lifecycle); `serve` is the
  headless owner. On the Studio the spaces `sshlg` and `appvillis` run `serve` started from an SSH
  session after `security unlock-keychain` (studio-agent DEC-0021), because a LaunchDaemon cannot
  open a login keychain nobody unlocked: SB-287 (open).
- **Placement record.** Switchboard writes no placement record and is not a singleton across
  machines: each machine and user is its own owner. Accounts used on another machine are marked
  `elsewhere` here (SB-175) and, once SB-284 is released, say where (`Snapshot.locations`,
  `switchboard accounts update --location`, the window's *On this computer · On other machines*).

## S2. How the operator talks to it

- Window (Accounts, Groups, Organizations), CLI `switchboard`, MCP `switchboard mcp` (every CLI
  command has a tool or a written reason, `crates/switchboard-cli/src/main.rs`
  `every_cli_command_has_an_mcp_tool_or_a_reason`).
- Fabric board: `switchboard dispatch` runs a board request headless (SB-247; exit contract
  `crates/switchboard-runtime/src/dispatch.rs` `EXIT_CONTRACT`, now with the refused-enroll branch,
  SB-288).
- Remote: over SSH the CLI and MCP work against that user's owner; a sign-in on a remote space is
  `login begin --headless` plus `login code` (SB-256 N1).

## S3. People's access

- Switchboard holds coding-agent accounts, not people. A person's reach is the groups and accounts
  on their own Switchboard. An organization's accounts serve only its groups (`context_admits`,
  `SEAT_OUTSIDE`), and since 0.7.21 a group may belong to several organizations and an account
  serve several groups (SB-282, `docs/runs/2026-11-06-many-to-many/spec.md`).
- Handing an organization's account to an employee's Switchboard: SB-283, in the backlog by the
  operator's word (thread `c7371b0c`); the first part (signed record, `hand-over`, `receive`) waits
  on branch `sb-283-handover`, unmerged. Inbox offers one mailbox per account with a `read` grant
  for the sign-in code (thread `d4c47b44`).

## S4. Tokens and keys

- Account credentials live in the macOS Keychain item of each account (vault), never in Git, a
  backup's plain part or the board (`docs/KEYCHAIN.md`). Encrypted backups seal them with a key in
  the vault.
- The proxy token and per-folder capabilities are derived and written by Switchboard
  (`proxy.json`, `.claude/settings.local.json`, SB-133); rotating the proxy port rotates the token
  (SB-269).
- The settings signing key (SB-272) is an Ed25519 key in the vault; only its public key leaves.
- OpenRouter keys for agents are vault slots, read at a paid start only (`AGENTS.md` Lifecycle).
- Observatory: Switchboard does not read or write Observatory's vault; agents launched by
  Switchboard get the Observatory MCP server.

## S5. Shared project infrastructure (Claude accounts)

- Groups hold a project's repositories and the accounts that serve them; organizations decide
  which accounts a group may use; the administrator's signed settings file carries groups by
  repository, organizations and provider teams, never an account (SB-270, SB-272,
  `docs/MOVING.md`).
- Gap: the e-mail domain still places an account in an organization by itself; the operator wants
  it to be a hint (SB-285, open).

## S6. Machines and spaces

- Ports: the proxy keeps its recorded port; DEC-0154's claim file is SB-278, waiting for the
  decision's tag (latest contract tag `v0.1.0+2026-11-06.1`).
- Context of a whole OS account from Observatory (OBS-359): SB-280, waiting.
- Machines do not tell each other which accounts they hold: SB-177 (open).

## S8. Moving and continuity

- `docs/MOVING.md`: settings export/import, a new sign-in on the new machine, `elsewhere` on the
  old one. Credentials never move (a renewal on one machine ends the other's sign-in).
- Backups: encrypted, after changes and daily, newest 10 kept (`AGENTS.md` Writes); a restore on
  the machine that wrote it.
- Format note: 0.7.21 writes schema 9 while a many-to-many link exists, and the next release schema
  10 while a location exists; an older Switchboard refuses such a file by name ("Unsupported
  account metadata version"). Update every machine before using these links.

## S9. Deploying from scratch

1. Install (`docs/INSTALL.md`, launcher or release asset); the app registers its login item.
2. `switchboard settings import <file> --under <code folder> --signer <key> --apply`: groups,
   organizations, provider teams, policies; idempotent (an existing group only gains folders).
3. Each account signs in (`login begin`, `--headless` over SSH); a person types the code.
4. `switchboard onboard` lists what is left (the same checklist as the window).
- Human steps: each sign-in's browser step or code; on a space without a graphical login, the
  keychain unlock (SB-287). Time: minutes per account.

## S10. Keys in Observatory

- Switchboard holds no organization keys in Observatory; account credentials stay in each
  machine's Keychain and do not migrate. When a person's machine leaves, their accounts are
  signed in again elsewhere and marked `elsewhere` (or handed over, SB-283 later).

## Findings and questions

| # | Finding | Row |
|---|---|---|
| 1 | `serve` for a space with no graphical login cannot store a sign-in | SB-287 |
| 2 | The e-mail domain decides an account's organization by itself | SB-285 |
| 3 | Machines do not share which accounts they hold | SB-177 |
| 4 | Agent isolation by organization (operator 13:10 UTC): Switchboard isolates accounts by group context, but a session's MCP servers and Fabric board are not checked against the organization | to file |

Question to the operator: for agent isolation, should a session launched in an organization's
group get only that organization's MCP servers (Switchboard writes the session's MCP config)?
