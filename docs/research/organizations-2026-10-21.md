# Organizations in Switchboard: accounts, groups and employee onboarding

Design, 2026-10-21. Status: phases 1 (0.6.60, SB-158), 2 (0.6.65, SB-109; the badge on
groups of SB-111) and 3 (0.6.66, SB-159: Vesta's read carries name and domains, no member list)
built; phase 4 open (operator request of 2026-10-21; builds on the RM-28
decisions of 2026-10-12). Rows: SB-158, SB-159, SB-160, with
SB-109 and SB-111 (backlog).

## 1. What the operator asked

A person works for several organizations (PassionCode.ai, Appvillis, clients). Switchboard
should show which of its accounts belong to which organization, keep an organization's work on
its own accounts, and without organizations stay the plain Switchboard it is today. For an
employee the organization issues an address on one of its domains in Fabric Inbox, forwards it
to the employee's own mail, invites that address to its Claude Code (and Codex) team, and the
employee signs in with it.

## 2. Two meanings of "organization", kept apart

| Word | What it is | Where it lives | Who sees it |
|---|---|---|---|
| **Provider organization** | Claude Team/Enterprise organization, ChatGPT Business/Enterprise workspace; a personal Pro/Max/Plus plan is also an "organization" of one at the provider | at the provider; Switchboard already records its id on each account (`ExternalIdentity::organization_id`, Claude profile `/organization/uuid`; Codex `account_id` is the ChatGPT workspace) | the provider and its admins (they see their seats' usage regardless of us, RM-28 MC-56) |
| **Organization** (Fabric) | the company the work is for: the RM-28 context `org:<id>` | Vesta record (`org.passioncode.ai`: name, domains, members), or a local label in Switchboard when there is no Vesta | the person; an organization sees only its own membership (RM-28 CX-22) |

Switchboard maps the first onto the second. One provider organization belongs to at most one
Fabric organization; one Fabric organization may have several provider organizations (a Claude
Team and a ChatGPT workspace).

## 3. How an account is attributed

Deterministic, in this order; the first that answers wins, and the account's badge says which:

1. **Placed by the person** (Accounts → card menu → *Organization…*). Always wins, *Personal*
   included.
2. **Provider organization known**: the account's provider organization (`claude:<uuid>`,
   `codex:<ChatGPT workspace id>`) is recorded on an organization. Placing one Claude Team seat
   records the team there, so every other seat of it follows without asking.
3. **Domain**: the account's address is on one of an organization's domains.
4. Otherwise: personal. With no organization defined at all, nothing of this is shown.

Built 2026-10-21 (0.6.60): an earlier draft let a personal plan on a company address stay only
"suggested". It was dropped: an address on the organization's domain is a mailbox the organization
holds, so the sign-in is the organization's whatever the plan, and *Personal* in the card menu
covers the exception in one step (section 10).

Inputs are local: the identity Switchboard already reads at sign-in and at its 30 s source pass,
plus the organization list. No Fabric, Vesta or Observatory call is needed to attribute
(same independence rule as statistics, SB-156).

## 4. Data

- `Snapshot.organizations[{id, name, domains[], provider_organizations[], created_at}]` and
  `Snapshot.memberships{account id → organization id | "personal"}` in `accounts.json`
  (omitted while empty) and therefore in encrypted backups.
- Nothing is added to the account itself: attribution is computed from these two and the
  identity already stored (`Snapshot::attribution`).
- Group (today's "project"): `organization: id | null` (SB-109, 0.6.65); the default pool is not
  a group and stays unrestricted (it serves the ordinary Claude Code login's rotation). Built
  2026-10-22: organizations, memberships and a group's organization also travel in backups,
  which the phase-1 build had left out.

## 5. What changes for the person

- **No organization defined:** nothing; the Organization menu item is the only trace.
- **Accounts:** a badge with the organization's name on each attributed card (its tooltip says
  why) and a filter (All, each organization, Personal) once one exists; organizations are added,
  edited and removed on their own page in the sidebar (*Organizations*), and nothing about them
  shows on *Accounts* until one exists (operator 2026-10-22).
- **Groups:** a group gets an organization; its folders then use only that organization's
  accounts (SB-109). A group without one takes personal accounts only, never an organization's
  seat; a folder in groups of different contexts uses the more private one, fixed for the
  session (RM-28 CX-26).
- **Statistics:** a By organization table above By group (counts only), with phase 2.
- **CLI and MCP:** `switchboard org list|add|domain|assign`, `switchboard_accounts` and
  `switchboard_projects` carry the organization id and name (schema bump, additive).

## 6. Employee onboarding (the operator's flow)

| Step | Who does it | Tool | Gate |
|---|---|---|---|
| 1. Add the member to the organization | org admin or its agent | Vesta (member, role) | none |
| 2. Issue `first.last@<org domain>` | agent | Fabric Inbox MCP `create_address` on a domain of the organization's Cloudflare account | the domain must be one of the organization's (Vesta record) |
| 3. Forward it to the employee's own mail | agent | Inbox `add_forward_destination` and the address's forward copy | **the employee clicks Cloudflare's verification link once** in their own mail (Email Routing verifies every destination); the mailbox keeps its own copy, so the organization keeps the identity |
| 4. Invite the address to the provider team | org admin | Claude Team: Settings → Members → Invite (no API on Team); ChatGPT Business: Workspace settings → Members (no API on Business). Enterprise plans: SCIM (Claude), Admin API invitations or SCIM (ChatGPT, since 2026-08-20) can do it from an agent | on Team/Business the invite is a click in the provider's console |
| 5. Accept and sign in | employee | invitation and sign-in codes reach their own mail through the forward; `switchboard login claude` (or *Add account*) on their machine | the employee signs in (sign-in code) |
| 6. Attribution | Switchboard on the employee's machine | rule 3: the organization's domain | none |
| Leaving | org admin | remove the seat at the provider; Inbox `pause_address` (forward stops, mail stays with the organization); Vesta removes the member | Switchboard reads the removed seat as an ended sign-in (Claude: a renewal refused, or a 401 that survives a fresh renewal; Codex: a 401 on two checks in a row), names the organization on the account's card, stops choosing it, and keeps its history (built in 0.7.0, nodes B-5 and B-6; [SCN-078](../ux/scenarios.md)) |

Why a forwarded company address rather than the employee's own: the sign-in identity belongs to
the organization; on leaving, pausing the address ends every new sign-in code, and nobody has to
ask the employee to give anything back. The seat is the employee's own: Switchboard never shares
one seat between two people, and an admin never signs in in an employee's name (provider terms
assign a seat to one person).

**Remote authorization, honestly:** a new sign-in always needs its code typed by someone. The
code reaches both the organization's mailbox and the employee's forward, so the employee signs
in on their own machine. Automating the code (reading it from Inbox on the employee's behalf)
would hand the identity to whoever runs the agent and is out of scope.

## 7. Standalone

Switchboard keeps working without Fabric, Vesta, Inbox or Observatory: organizations are a local
list the person types (name and domains). Linking to Vesta (SB-159) only fills the same list
(name, domains, members' addresses) and keeps it current; a Vesta that cannot be reached leaves
the last copy in force and says so.

## 8. Privacy

An organization learns nothing of the person's other organizations from Switchboard: no list of
memberships leaves the machine, statistics stay local, analytics sends only the count of
organizations and attributed accounts if it ever sends anything about them; 0.6.60 sends nothing
new (ANALYTICS.md unchanged).

## 9. Phases

| Phase | Row | Content | Needs |
|---|---|---|---|
| 1 | SB-158 | local organizations, attribution rules 1 to 4, badge and filter, CLI and MCP fields (done in 0.6.60) | nothing outside Switchboard |
| 2 | SB-109, SB-111 | group organization, refusal of other contexts' seats, badge on groups and sessions, Statistics by organization | phase 1 |
| 3 | SB-159 | link an organization to Vesta (domains, members) | Vesta read API for the member's own organizations |
| 4 | SB-160 | onboarding playbook (section 6) run by an agent: Vesta member, Inbox address and forward, provider invite checklist; Enterprise automation where the provider has an API | Vesta and Fabric Inbox owners; provider console clicks on Team/Business |

## 10. Decisions taken here

- An address on an organization's domain belongs to it whatever the plan (section 3); the
  person keeps an exception personal from the card menu.
- Switchboard never reads a sign-in code from a mailbox (section 6).
- Organization ids follow RM-28 (`org:<id>`), local ids until a Vesta link replaces them; the
  replacement keeps every attribution.

## Sources

- RM-28 org contexts, operator decisions of 2026-10-12: fabric-workspace
  `docs/reports/2026-10-12-org-contexts/README.md` §0, §5 to §7, §15 (CX-07, CX-22, CX-26, CX-28).
- Vesta organizations with recorded domains: passioncode-vesta `docs/backlog.md` row VES-SELFSERVE
  (commit `776846c`).
- Fabric Inbox address and forwarding tools: fabric-inbox `README.md` (Domains and addresses),
  MCP tools `create_address`, `add_forward_destination`, `pause_address` (commit `f328d9b`).
- Claude Team: invitations by hand, SCIM only on Enterprise, SSO with JIT on Team:
  [Anthropic support, JIT or SCIM provisioning](https://support.claude.com/en/articles/13133195-set-up-jit-or-scim-provisioning).
- ChatGPT Business members by hand; Enterprise Admin API invitations and SCIM:
  [Business members](https://help.openai.com/en/articles/8542216-managing-members-seat-types-and-roles-in-chatgpt-business),
  [Enterprise release notes](https://help.openai.com/en/articles/10128477-chatgpt-enterprise-and-edu-release-notes).
- Provider organization already stored per account: `crates/switchboard-core/src/lib.rs`
  (`ExternalIdentity::organization_id`), `crates/switchboard-runtime/src/refresh.rs`
  (`/organization/uuid`), `crates/switchboard-runtime/src/external.rs` (Codex identity).
