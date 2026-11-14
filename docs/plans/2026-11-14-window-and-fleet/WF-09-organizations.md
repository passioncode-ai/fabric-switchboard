# WF-9 Organizations and PassionCode.ai (SB-371, wave 4)

> как это происходит, связь, как подконнектиться к организации, как дисконнектиться, как связь же
> нас с Вестой, с облаком, вот вся эта штука должна быть проверена, типа еще раз.

## Now

`renderOrganizations` (`src/main.ts:3074`): `passioncodePanel` (3052, Connect…, Read now, ⋯
Disconnect, Cancel while waiting), organization cards with domains, members, suggestions. Runtime:
`vesta.rs` (browser sign-in, a read every 6 hours, `vesta.json`), SB-159 open (link to Vesta),
SB-160 closed with leftovers, SB-227 released marks. CLI `org connect`, MCP
`switchboard_organization_set` (`passioncode_read`, `passioncode_cancel`, `passioncode_disconnect`).

## Sub-modules and tasks

**9.1 Connect and leave, verified.** A scripted walk on this Mac and on a clean macOS user: connect,
read, an organization appears, a group bound to it, a seat placed; disconnect; reconnect; a person
removed on PassionCode.ai shows as left (SB-160). Every step recorded with the log lines and the
`vesta.json` fields (never a token). Defects become rows here.

**9.2 States and copy.** One status line per state (not connected, waiting for the browser,
connected and read N min ago, needs signing in again, unreachable since …), each with its one action;
disconnect says what stays (the organizations, the groups' bindings) and what stops (reads).

**9.3 Cloud link.** What Switchboard sends and reads from Vesta and the organization's cloud copy, in
one diagram in CONTRACTS; a health line `org_link` in the log; the result shown under
*Organizations → PassionCode.ai*.

## MCP

`switchboard_organizations` gains the connection state block the window shows.

## Checks

The walk's evidence file under `docs/evidence/`; SCN-071, SCN-076, SCN-079 updated.

## Depends on

Vesta's side for anything 9.1 finds there (a board request to `passioncode-vesta`).
