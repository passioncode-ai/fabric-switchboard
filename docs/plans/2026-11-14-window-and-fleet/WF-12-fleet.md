# WF-12 Fleet: machines, Switchboards, organizations (SB-374, waves 4 and 5)

> несколько свитчбордов, типа которые там на удаленной машине, вот я как админ организации, я должен
> видеть вот свои организации, могу перейти внутрь, если я в зависимости от уровня доступа ... я
> должен видеть эту всю связь свитчбордов на разных машинах, аккаунты, быстро иметь возможность
> перекинуть аккаунты между машинами, также подумать про автоматический роутинг между машинами

## Now

- Machine groups (SB-355, DEC-0013): every other machine is a group; `machines list --live` reads
  them over ssh; moves are send and recall (one step machine to machine since SB-355).
- `targets` (SB-332), `remote status` and `remote check` (SB-327), `settings sync` (SB-326).
- Organization-wide view: none. SB-177 (machines publish their identities through Fabric's hub) and
  SB-291 (access levels) wait for the estate.
- DEC-0013 forbids routing between machines.

## Sub-modules and tasks

**12.1 Own machines (wave 4).** A *Machines* page: this computer and each machine Switchboard knows
(machine groups, targets), each with its Switchboard version, whether it answers, its groups and
accounts with usage (WF-4), and the actions *Move here*, *Move there*, *Open its Switchboard* (a
remote launch of `switchboard status` over ssh). Groups' *Other machines* section moves here.

**12.2 Organization admin (wave 5).** Through PassionCode.ai: the organizations the person administers,
each with its members' machines and their Switchboards as each publishes (SB-177), shown by access
level (SB-291: owner and admin see all, a member sees own). Moving an account between two members'
machines asks both owners (the template's rule: another person's sessions never ride one person's
sign-in). Waits for Fabric's hub and the levels; designed here, built when they land.

**12.3 Routing between machines (wave 5).** Opt-in per machine pair: when a machine's group has no
account with room for two passes and another of the person's own machines holds an account with room
in its machine group there, Switchboard moves that sign-in here (send and recall, recorded in
`seats.jsonl`), at most once an hour per account, and moves it back when that machine needs it.
Decision DEC-0020 supersedes DEC-0013's "never routes" for this case only. Requests are never relayed
across machines.

## MCP

`switchboard_machines` (12.1), new `switchboard_fleet` (12.2), `switchboard_balance` gains
`machines` (12.3).

## Questions

Routing scope (README question 3).

## Checks

12.1: live against the Studio's three copies; 12.3: a fake second machine (the `remote_check` seam)
proving one move and one return, never two holders of one sign-in.

## Depends on

WF-4 (usage), SB-357 (pin the other machine's key before automatic moves), SB-177 and SB-291 (12.2).
