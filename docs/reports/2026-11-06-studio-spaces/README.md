---
report: studio-spaces
project: fabric-switchboard
date: 2026-11-06
kind: plan
sources: fabric-switchboard main 7ca4811; fabric-workspace main 6cb6cbb6 (docs/plans/2026-11-04-studio-spaces, knowledge/plans.md, knowledge/integration.md)
---

# Moving to another machine, several macOS users, tokens and organizations (2026-11-06)

Operator, relayed on the board (thread `1fe66e6e`, said 2026-11-05 in project-observatory session 456ad510):
«…что нам нужно сделать, чтобы все было качественно и настраиваемо, и переносилось, выдавались токены… пиши
детальные спеки по реализации на каждого… запускай волнами все до конца автономно и выполняй.»

## What the Workspace plan expects of Switchboard

- studio-spaces README: each macOS user runs its own Switchboard (Q1 a), a copy per user in `~/Applications`;
  rows SB-251 (several users on one Mac, released 0.7.12, two-user acceptance open) and SB-242 (groups and accounts
  per user, D-1).
- runbook 2.7, 2.9, 3.2: apps per user; the Switchboard accounts of both spaces are signed in by the operator.
- access.md: Fabric's on-demand sessions start through `switchboard dispatch` (0.7.8).
- integration.md I1-3 (accounts placed on both Macs), I2-1 (`org connect` on every member's Mac).

## Findings (code at 7ca4811)

1. **Moving.** Encrypted backups carry accounts with credentials, groups, rules, agent keys and settings, sealed with
   a Keychain key of this Mac (`crates/switchboard-core/src/backup.rs` 112-157, 733-753): they restore on the same
   Mac only. The account-free settings file (`switchboard-settings/1`, `crates/switchboard-runtime/src/settings_bundle.rs`)
   moves groups, organizations, agents and policies, but `apply` never writes a group's `sources` (exported at 85,
   ignored in 225-299), skips a group with no checkout found (around 239), names a new group's pool from its name while
   imported policies keep the bundle's pool, and leaves out the Fabric Inbox origin and Client Id,
   `provider_organizations`, the shell-hook choice and spending ceilings. No runbook lists what must be entered again.
2. **Several users.** Ports are per user already: the proxy reuses the port its `proxy.json` recorded or binds an
   ephemeral one (`crates/switchboard-proxy/src/lib.rs` 206-222), the control port is ephemeral
   (`crates/switchboard-runtime/src/control.rs` 66); folders of another user are left alone (SB-251). Open: the
   two-real-users acceptance; Keychain trust names `/Applications` only (`keychain_macos.rs` 64, 345-360).
3. **Tokens on loopback.** The CLI, hooks and MCP check the control listener before sending their token (a hello
   with an HMAC proof, `control.rs` 141-152, 456-517; test `reused_port_never_receives_bearer_or_secret`).
   Sessions reach the proxy with no such check: Claude Code sends its folder or pool capability, a managed home its
   session token, Codex the auto capability, with the prompt, to the recorded port. While a user's owner is down,
   another user's process on that port receives them, and since the token is kept across restarts (LC-11) they stay
   valid on the owner's new port.
4. **Organizations.** A new member's Mac gets the administrator's settings file (SCN-076 step 2): its GitHub owner
   (`sources`, SB-265) is lost, a group with nothing checked out is not made and nothing is cloned, and the file is
   not authenticated.

## Specs (backlog rows)

| Row | P | Spec | Acceptance |
| --- | --- | --- | --- |
| SB-267 | P1 | `settings import --apply` writes each group's `sources` and gives a new group the bundle's pool when it is free (else the name's, with the group's policies moved to it). | A round trip of a group with sources and a renamed pool keeps both; policies point at an existing pool (test). |
| SB-268 | P1 | Import creates every organization-bound group even when nothing is checked out; repositories of a GitHub source not on this Mac are listed in the plan as `missing`, and `--clone` clones them with `github::clone` into `--under` and adds each to its group. | Plan names missing repositories; `--clone` against a stand-in `gh` adds them; a group with no checkout exists with its sources (tests). |
| SB-269 | P1 | Capabilities a stopped owner may have leaked are renewed: when the proxy comes up on another port than `proxy.json` recorded, the session token, every pool salt and the auto capability are rotated and every route rewritten, logged `proxy_rotated` with the reason; the port another user holds is named in the log by uid. | A start whose recorded port is held rotates every capability and the old ones are refused (test); a start on the recorded port keeps them. |
| SB-270 | P2 | The settings file carries the Inbox origin and Client Id (never the secret), `provider_organizations`, the shell-hook choice and group spending ceilings (a ceiling is only ever lowered by import); the preview names each. | Round-trip test per field; import never raises a ceiling. |
| SB-271 | P2 | Runbook `docs/MOVING.md` (English and Russian): moving to another Mac and adding a macOS user: the settings file, what is entered again (sign-ins, OpenRouter and agent keys, the Inbox secret, PassionCode.ai connect), marking moved seats `elsewhere` on the old Mac, `~/Applications` per user. | `check_docs`; every named command exists (`onboard_commands`-style test). |
| SB-272 | P3 | An administrator's settings file can be signed (an Ed25519 key the organization publishes) and the preview names the signer; an unsigned file says so. | Signed, unsigned and tampered files (tests). |
| SB-251 | P2 | (existing) the two-real-users acceptance on the Studio, with the Keychain trust of a `~/Applications` copy recorded. | Operator-assisted. |
| SB-177 | P3 | (existing) marks shared between machines through the hub replace manual `elsewhere`. | After Fabric's hub on the Studio. |

## Waves

1. SB-267, SB-268 (settings import for a new member's Mac).
2. SB-269 (capability renewal).
3. SB-270, SB-271.
4. SB-272; SB-251 with the operator; SB-177 after Fabric.

## Proposed to the contract (thread `1fe66e6e`)

Switchboard needs no fixed port per uid: both of its listeners record or draw their own. What it asks of the port
norm is the rule SB-269 implements: a product whose listener may be taken while it is down renews every credential
it handed out once it comes up on another port.
