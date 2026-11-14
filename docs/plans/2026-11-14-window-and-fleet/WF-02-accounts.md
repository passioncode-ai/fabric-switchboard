# WF-2 Accounts by machine (SB-364, wave 1)

> хочется, чтобы сначала у нас были аккаунты на этой машине, потом уже показывались на других, а не
> в разнобой ... изначально группировка по машинам. Вот эти фильтры, которые есть, дизайн поправить
> ... Сессия, кодекса поднимем выше. И вот InUseNow кодекс почему-то не такими ... given to agents
> ... какой-то организации, типа, где этот агент, где он находится, ... больше информации.

## Now

- Order: `groupAccounts` (`src/ui-logic.ts:458`) groups by provider (Claude, Codex), then pool
  (`default` first), then quota state. Machine pools appear inside each provider as "On {machine}"
  pool headings (`machinePoolName`), so this computer's groups and other machines interleave.
- Page order (`renderAccounts`, `src/main.ts:1936`): status stack, *In use now* (`renderCurrent`
  2361), automatic switching (`renderPolicies` 2393), `organizationsBar` (3014), `machinesBar`
  (1630), sort note, provider sections, *Given to agents* (`givenSection` 1683), Kimi (2019).
- Filters: two chip bars (organizations; all, this computer, other machines), each shown only when it
  has more than one value.
- *In use now* draws both providers with one loop; the visible difference the operator saw is to be
  measured on the live window first (2.3.1).
- *Given to agents* lists label and a "Given to {agent}" badge; no organization, machine or place.

## Sub-modules and tasks

**2.1 Machine sections.** New grouping in `ui-logic`: machine, then provider, then group, then quota.
- 2.1.1 *This computer* first: Claude, then Codex (README question 1), each with its groups.
- 2.1.2 Then one folding section per other machine (WF-1.2), named as on Groups (`studio-sshlg`),
  with its accounts and their remote usage once WF-4 lands; collapsed when it has more than 6.
- 2.1.3 *Given to agents* and Kimi after the machines.
- 2.1.4 The pool heading inside a machine says the group and which account its sessions use, as now.

**2.2 Filters.** One control row: a search field (label, e-mail), an organization select (only when
organizations exist) and *Needs attention only*. The machine chips go: machines are sections now.

**2.3 In use now.**
- 2.3.1 Screenshot Claude and Codex cards in the installed app and in the demo; list each difference.
- 2.3.2 One card layout for both: identity, saved-as, group, usage, the same actions where the
  provider allows them (Codex has no native switch, `canSwitchNative`, `src/ui-logic.ts:483`: its
  place shows why).

**2.4 Given to agents.** Each row: the agent's name, its organization (the account's attribution),
the machine it runs on (`assigned_to` plus the seat record), the group and folder of its latest
launch (`launches/`), last activity, and *Take back*. The data come from existing records; a
`switchboard_accounts` field `assigned` gains the same.

## MCP

`switchboard_accounts` gains `machine` (here or the machine group's name) on every row and the
`assigned` details of 2.4. No window-only data.

## Questions

Codex placement (README question 1).

## Checks

ui-logic cases for the new grouping and order (this computer first, every account exactly once);
demo and live screenshots before and after; SCN-001 and SCN-092 updated; design-tester pass.

## Depends on

WF-1.2. WF-4 fills other machines' usage; without it the rows say "Usage read on {machine} {age}" or
"Not read yet".
