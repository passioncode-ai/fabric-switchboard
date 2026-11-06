# SB-282: groups, organizations and accounts, many to many

Operator, 2026-11-06 (chat): «группы могут быть привязаны к организации, а могут и нет, или к
нескольким организациям», then «аккаунт может быть перевязан к нескольким группам или к
нескольким организациям. Короче, это должно быть ко многим связи».

Before this change each link was one to one: a group (`Project`) had one `organization`, an
account one `pool` (its group) and one attribution (`Snapshot::attribution`), and
`organization_group` treated two groups of one organization as a mistake to resolve.

## Model

A **context** is `personal` or one organization. Each side of a link is a set of contexts.

| Link | Stored as | Compatible form |
|---|---|---|
| group to organizations | `Project.organizations` (ordered; empty: personal) | one is still written as `organization` (schema 6); several as `organizations` (schema 9) |
| account to groups | the account's own `pool` (its home group, unchanged) plus `Project.shared_accounts` of every other group it also serves | written only while not empty (schema 9) |
| account to organizations | the rules' answer (`attribution`: the person's word, the provider organization, the e-mail domain) plus `Snapshot.also_organizations` (the person's word that the account also works for these) | written only while not empty (schema 9) |

Schema 9 is written only while one of the three many-links is in use, so a file without them
stays readable by 0.7.20 and older; a 0.7.20 reader refuses a version-9 file by name
("Unsupported account metadata version"), never as corrupt.

## Rules

- **R1 admission.** An account serves a group when their context sets meet: a personal group
  takes accounts whose set holds `personal`; a group of organizations takes accounts of any of
  them (`Snapshot::context_admits`, `Project::admits_any`). Placement (`save_project`,
  `set_project_organizations`, sharing) refuses an account outside it with `SEAT_CHOSEN`, and a
  change of the group's organizations that would leave one of its accounts outside with
  `SEATS_ELSEWHERE`, as before.
- **R2 session context.** A session runs in the first of its group's contexts the serving
  account also holds; with no group, in the account's own first context; personal when they
  share none (`context_for`). The group's own first organization is its context where no
  account decides: a terminal in its folder (`SWITCHBOARD_CONTEXT` in the folder map) and its
  saved key (`key_context`).
- **R3 serving.** `Snapshot::serves(account, pool)`: the account's home pool or a group that
  shares it. Selection, routing, the 429 move (SB-98), rotation, seats, Claude and Codex
  bindings, shell homes and launches use it, so one login serves several groups with one
  credential and one quota record (no copies).
- **R4 placement.** Adding an account to a group (`SaveProject` account set, `ProjectEdit
  add_accounts`): an account in the default pool moves there as before; an account whose home
  is another group is shared, keeping its home. Taking a shared account out unshares it; taking
  its home account out returns it to the default pool as before. `MoveAccount` still moves the
  home and drops a share into the new home. Removing a group drops its shares.
- **R5 several groups per organization.** No organization has to keep one group:
  `organization_groups` lists them, `organization_seats` counts its accounts in any of them,
  the checklist's group step is done with one or more, and a new seat is offered a move only
  when its organization has exactly one group.
- **R6 removing an organization** takes it off every group and every account's extra list; a
  group or account left with none is personal.

## Surfaces

- CLI: `project edit --organization` repeatable (each one id or name; `personal` alone clears);
  `accounts organization <id> --organization` repeatable.
- MCP: `switchboard_project_edit organizations: [..]`; `switchboard_organization_set assign`
  with `organizations: [..]`.
- Window: a group's organizations and an account's organizations are chosen with check boxes;
  an account's card lists every group it serves.
- Views carry the lists (`organizations`, `organization_names`, `shared_accounts`, account
  `groups`, attribution `also`) beside the old single fields, which keep the first.
