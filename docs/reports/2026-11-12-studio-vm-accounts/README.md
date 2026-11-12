---
report: fabric-switchboard accounts architecture for the Mac Studio, its spaces and the esim-studio VM 2026-11-12
project: fabric-switchboard
date: 2026-11-12
source: Fabric board request 20eeaab6 from Appvillis Anya, relaying the operator 2026-11-12 ~22:00Z («провести аудит относительно нашего вообще свитчборда, все системы, как работают эти аккаунты, как мы бежим, чтобы мы не делали то, что не нужно … мы еще добавляем новые аккаунты, от 5 до 10 штук … на студию … вопрос с архитектурой скорее больше»)
---

# Accounts on the Mac Studio, its spaces and the esim-studio VM

The case: one Mac Studio with three macOS users ("spaces": `sshlg`, `passioncode`, `appvillis`),
each running its own Switchboard; a Linux arm64 VM `esim-studio` inside it that runs three eSIM
agents and needs Claude through a Switchboard of its own; Appvillis accounts on the
organization's mail; 5 to 10 new accounts for the Studio. studio-agent proposed moving the
account of the `esim` group to the VM with `switchboard accounts send`; the operator held the
move until this audit.

## What runs now (read 2026-11-12 22:05Z, every copy on 0.7.66)

| Space | Claude accounts | Groups | Organizations |
|---|---|---|---|
| `appvillis` (`studio-appvillis`) | 5, all `appvillis.com`, one per group: `esim`, `appvillis-agents`, `nicegram-apps`, `ng-business`, `default` | the four above plus `appvillis-platform` (3 folders, no account of its own) | none (`switchboard --json organizations` is empty); no group is bound to one |
| `passioncode` | 4, `passioncode.ai`, all in `passioncode` (two marked PassionCode.ai) | `passioncode` (bound) | PassionCode.ai |
| `sshlg` | 4, `sshlg.me` (`agents` one disabled, `default`, `frame-agent`, `apps-soft`) | 5 groups, `passioncode-ai` bound | PassionCode.ai |

Week left on the `appvillis` accounts at that time: 84 %, 12 %, 53 %, 0 %, 84 %. The proxy of
every copy listens on `127.0.0.1` only (`status`: `127.0.0.1:53520` in `appvillis`).

## 1. Move an account to the VM, or let the VM use the host's pool?

**Move a seat to the VM. Sharing the host's pool through its proxy is neither built nor designed,
and the design rules it out.**

- One subscription is active on one machine at a time, each machine has its own seats, and no
  machine renews an account another holds ([two machines D-1, D-2](../../research/two-machines-2026-10-23.md)).
  `accounts send` moves the sign-in and keeps one holder ([CONTRACTS → SB-283](../../CONTRACTS.md)).
- A member's machine or VM gets its seat by `accounts send` into its group (seats S-1), leaves by
  `accounts recall` (S-3), and holds only its member's groups through `settings export --group`
  (S-6) ([seats and levels](../../research/seats-and-levels-2026-11-08.md)). SB-292 on the
  [board](../../evidence/backlog.md) is exactly "Switchboard on Linux in the member's VM".
- The proxy cannot serve another machine: it binds loopback only ([SPEC](../../SPEC.md):
  "Listen on loopback `127.0.0.1`, not LAN"; `crates/switchboard-proxy/src/lib.rs`), and since
  SB-273 and SB-330 it serves only connections whose peer is a process of the same macOS account.
  A VM's connection cannot pass that check, by design: opening it would put every account of the
  space behind a network port.
- Where the work may run on the host instead, `launch --target` and `dispatch --target` run it on
  the machine that holds the accounts ([remote dispatch](../../design/remote-dispatch.md)). That
  does not fit agents that live in the VM.

**Recommendation for esim-studio.** Do not move the `esim` group's account: it is that group's only
account, so the host's eSIM work would lose its seat. Give the VM its own seats from the new
accounts: sign them in on the `appvillis` space, put them in a group `esim-studio` there, then
`switchboard accounts send <id> --to <vm> --pool esim-studio` from that space, so the move is
journalled where the account came from (`switchboard accounts seats`). On the VM: the Linux arm64 build,
`switchboard vault use file` (no session bus in a headless VM, [CLI](../../CLI.md)),
`settings export --group esim-studio` on the host and `settings import --apply` on the VM for the
group and its folders. Prerequisites: ssh from the `appvillis` space to the VM without a prompt;
the VM's Switchboard installed from the AppImage (a `.deb` install never updates itself, SB-88).

## 2. How to spread 5 to 10 new accounts

The rules that decide it:

- The mail domain only suggests; an account belongs to Appvillis only by `org assign`, by being
  signed in to or moved into a group bound to Appvillis, or by binding its group
  ([DEC-0009](../../DECISIONS.md#dec-0009-an-e-mail-domain-suggests-an-organization-only-the-person-or-an-administrator-places-an-account)).
- A group bound to an organization takes only that organization's accounts and never falls back to
  the default group (SB-109, SB-325 in [CONTRACTS](../../CONTRACTS.md)).
- Automatic switching (on by default, [DEC-0008](../../DECISIONS.md#dec-0008-automatic-switching-of-managed-sessions-is-on-by-default))
  and the one move of a refused request ([DEC-0002](../../DECISIONS.md#dec-0002-a-refused-claude-code-or-codex-request-moves-once-to-the-groups-next-account))
  stay inside one group. **A group with one account cannot switch or fail over**: when that
  account reaches its limit, the group's sessions stop (on 2026-11-12 the `appvillis` account at
  0 % of its week stopped its group's work until 11-13).

So, for Appvillis accounts on the Studio:

1. Organization first: create Appvillis in the `appvillis` space (connect it to PassionCode.ai, or
   `settings sync --to studio-appvillis` from a copy that has it), bind the Appvillis groups to it,
   `org assign` the five existing accounts. Until then the space has no organization isolation.
2. Two accounts in every group that runs agents all day, so switching has somewhere to go:
   `appvillis-agents`, `nicegram-apps`, `esim` each get one new account (3).
3. `esim-studio` for the VM: two new accounts, sent to the VM (2), for the same reason.
4. Any further accounts (up to 5 more) go where `switchboard statistics --days 7` on the
   `appvillis` space shows the most requests or limit stops; `ng-business` and
   `appvillis-platform` come next. An account may also serve a second group (`project edit
   --add-account`) instead of a new one being bought.
5. New accounts on personal mail (`sshlg.me`) belong to the `sshlg` space, not to Appvillis groups.

No document sets a number of accounts per group; "two where work runs all day" is this report's
proposal, from DEC-0002 and DEC-0008.

## 3. What is done needlessly or diverges

| Finding | Why it matters | What to do | Owner |
|---|---|---|---|
| The `appvillis` space has no organization and no bound group | Appvillis accounts are not isolated from any personal account added there; nothing stops a group falling back to `default` | step 1 above | the `appvillis` space (studio-agent) |
| One account per Appvillis group | no switching, no failover; a limit stops the group | step 2 above | the same |
| The proposed move of `esim`'s only account to the VM | leaves the host's `esim` group with no seat | new accounts for the VM, step 3 | the same |
| [MOVING.md](../../MOVING.md) still teaches signing in again plus a manual `--elsewhere`, and says each macOS user installs into their own `~/Applications` | the decided way is `accounts send` / `recall` and `settings sync`; the Studio runs one app for three `serve`s (DEC-0011) | rewrite the runbook, with a Linux VM section | fabric-switchboard, SB-349 |
| `DECISIONS.md` said "Next free ID: DEC-0011" with DEC-0011 written | the next decision would collide | fixed in this change (DEC-0012) | fabric-switchboard |
| A seat's journal lives only on the copy that sent it | the Studio copies do not know where their accounts came from | send from the space that holds the account (step 3) | operating rule |
| The Studio copies are not connected to PassionCode.ai | organizations are copied by name, with a different id on each copy | a person's browser sign-in once per space | operator gate |

Rows the design rests on, still open: SB-178 (renewing one Claude account on two machines,
measured), SB-177 (machines publishing which identities they hold), SB-251 (two real macOS users
on one Mac), SB-287 (`serve` without a graphical login), SB-292 (the VM seat).

## Not done here

Nothing was moved, signed in or changed on any Studio copy; the move studio-agent proposed stays
held. This report changes only documents of this repository.
