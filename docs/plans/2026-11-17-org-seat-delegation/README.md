# A subscription delegated to a teammate's machine, through an organization (SB-403, 2026-11-17)

Board row: [SB-403](../../evidence/backlog.md). Entry point for the next agent: this file, then the
wave table at the end. Decision: [DEC-0022](../../DECISIONS.md#dec-0022-an-organization-is-a-label-never-a-lock-on-where-an-account-works).

## What was asked (the operator's words, 2026-11-17)

> Следующее, что я хочу сделать, это подключение машин сотрудников к организациям, которые есть.
> Мы создадим организацию Апфилис. Подключим у неё все аккаунты, которые у нас на машине Апфилис,
> и один аккаунт или два каких-нибудь свободных, которые будут стоять тут. И один из этих аккаунтов
> мы подключим так, чтобы можно было … связь между машинами: у [a teammate] … он хочет себе подключить
> вторую подписку, значит мы сделаем, чтобы он на машине конвертнул свой софт, подключил к
> организации, у него добавится подписки организации, которые делегированы ему, и я могу
> делегировать его на машину [a teammate] … и отзыв этой подписки … чтобы [a teammate] на своей машине мог
> ей пользоваться, но по факту авторизоваться он типа не будет с ней.

> [a teammate] может использовать Claude Desktop для своей обычной стандартной подписки, а вот эту
> использовать уже подключенную к свитчборду. Если этой будет мало, мы будем просто в свитчборд
> больше подписок ему добавлять … консольное на свитчборде, а десктопное на десктопе. А также
> убедиться, что мы не трогаем сессии для десктопного Claude Code, и он будет работать никак не
> связанно со свитчбордом.

> У нас может быть несколько e-mail на организацию, не обязательно только один … в организации
> может и не быть имейлов, мы можем вообще не привязываться к имейлу.

> давай вот здесь вот правильно настроим, чтобы подписки, которые у нас привязываются к
> организации, они не блочились, чтобы мы могли их все равно передавать между группами, чтобы,
> если нужно, мы могли другую организацию поставить, типа через делегацию, типа подписки на другую
> машину. все избирать, а не ограничивая всю локальную работу. Локальную работу подписки на машине
> ограничивается папками и PassionCode-ами, которые там уже создают сами пользователи.

## Done on the operator's Mac (2026-11-17, through MCP)

- Organization **Appvillis** created (one domain).
- Four accounts on another machine's user (in its machine group) and one account here (moved home
  into the group bound to Appvillis, 7 folders) work for Appvillis. Each keeps the personal
  context first (`organizations: [personal, Appvillis]`): assigning Appvillis alone was refused
  then ("a group this account or its provider team serves would then refuse it"), and the groups
  that use them kept working. S1 removed that refusal.
- Every Claude account here was at 0 % of its weekly window (resets 2026-11-20) except three that
  serve other groups now; no account was taken from a group. The second "free" account is chosen
  when the windows reset.
- 2026-11-17 03:40 UTC: two organization accounts called back into personal groups were left in
  `default` with `placed_error` (the group refused them); worked around by hand, then fixed in S1.

## What exists (research of 2026-11-17, file:line on main `a49bc2f7`)

- `accounts send` / `recall` move a sign-in over ssh (`account_send.rs:212`, `:604`): this Mac
  stops renewing it, the other machine renews it, recall takes it back and proves the copy there is
  gone; a machine signs a challenge with its pinned key first (`machine_proof.rs`).
- On the receiving machine an account in a project group serves only sessions in that group's
  folders (`bindings.rs:16-31`, the proxy replaces the client's token, `switchboard-proxy/src/lib.rs`)
  and can never become the ordinary Claude Code login (`PROJECT_NOT_NATIVE`). Claude Desktop is
  outside Switchboard: no code names its app, data folder or Keychain item (`docs/SPEC.md:12`).
- Missing: a restricted ssh entry, a "delegated" account the receiver cannot pass on, a lease for
  revocation while the receiver is offline, a switch so Switchboard never reads the person's own
  Claude Code sign-in.

## Decisions (under the operator's delegation)

- **D-1.** The delegation rides `accounts send` over ssh into a group of the organization on the
  teammate's machine; no server path until PassionCode.ai's vault-to-vault door exists.
- **D-2.** The teammate's machine gets a restricted ssh entry (`switchboard ssh-serve`, forced
  command), never a shell.
- **D-3.** A delegated account carries the sender's pinned key: it cannot be sent on, given to an
  agent, lent, moved to the default group or made the ordinary login, and only the sender recalls it.
- **D-4.** A lease: the receiver renews a delegated sign-in only while the sender's background read
  keeps extending it (default 48 h); past it, the copy is switched off and removed.
- **D-5.** "Own sign-in off": a teammate's Switchboard never reads or writes his own Claude Code or
  Claude Desktop sign-in (background sync and MCP sign-in copying skip it).
- **D-6.** An organization may have several domains or none; a seat is any account that works for
  it (first or also) or that its group holds; the seat step offers assigning an account already
  here, signing in with any address, or calling one back, never only "sign in with your address".
- **D-7 (S1, DEC-0022).** An organization is a label, never a lock. A person's act (assign, add an
  organization, place, move, share, bind a group, sign in into a group, call back) is never refused
  because of organizations; the answer warns (`outside_organization`) and the group serves the
  account. Local work is bounded by group folders only.
- **D-8 (S1).** Automatic acts prefer and never refuse: automatic switching and the SB-98 move take
  the group's own organizations' accounts first, another only when none has room. Balancing keeps
  its organization boundary (an automatic move between groups, DEC-0012).
- **D-9 (S1).** One seat of a provider team is placed alone; its team follows only with `team`
  (`org assign --team`, MCP `team`, the window's box), and the answer says `team: claimed` or
  `not_claimed` with `teammates`.
- **D-10 (S1).** Data stays apart: a session's context is unchanged (the first context its group
  and account share, else personal). The org-seats data (SB-381) and the Organizations counts take
  every account that works for the organization, with `context: first|also`.
- **D-11 (S1).** `org assign --also ORG` (MCP `also`) adds an organization and keeps the account's
  own; it is the window's *Add a seat* for an account already here, followed by sharing it into
  the group.

- **D-12 (S1 round 1).** The group owns the data, the account only pays: a session in a group's
  folder runs in the group's context whatever account or key serves it, so a launch and a terminal
  there agree.
- **D-13 (S1 round 1).** With a group, the seat step is done only when an account one of its groups
  holds is signed in (only those route there); an account that works for the organization in none
  of its groups is offered first with `project edit --add-account`.
- **D-14 (S1 round 1).** *Add a seat* saves the group first, then adds the organization. An account
  of the default group moves into the group (a person's word cannot lend from the default group
  without balancing taking it back), and the dialog says so before it happens.
- **D-15 (S1 round 1).** Automatic picks outside the group's organizations are journaled
  (`failover_outside`) and counted (`FailoverTally.outside`, `outside` on a rotation decision);
  statistics count an account once, under its first context.

## Waves

| Wave | What | State |
|---|---|---|
| S1 | Seats and onboarding: accounts that also work for the organization count; domains optional; the seat step's next steps; an organization never refuses a person's act (DEC-0022); the team follows only on request; moves and call backs place with a warning | done on branch `org-seats` (gate on the Mac Studio, see the backlog row) |
| S2 | `ssh-serve` restricted entry, `delegated` account kind and its refusals, lease and its renewal by the sender | next |
| S3 | "Own sign-in off" switch; proof that Claude Desktop and `~/.claude` are untouched (synthetic Keychain tests) | with S2 |
| S4 | Teammate setup guide (`docs/ORGANIZATION.md` §4b), window and MCP for delegate and revoke, CONTRACTS | after S2 |
| S5 | Live: a teammate's machine on the tailnet, one Appvillis subscription delegated, a console session on it, Claude Desktop checked untouched, recall | needs the teammate's machine (human step: install, tailnet, ssh key) |

### S1 evidence

- Code: `crates/switchboard-core/src/organizations.rs` (`assign` with `team`, `groups_outside`,
  `outside_warnings`), `lib.rs` (`route`, `select_transaction`, `SaveRequest::plan`), `projects.rs`,
  `rotation.rs`, `machines.rs`, `backup.rs`; runtime `onboarding.rs` (`read_snapshot`, the seat
  step), `lib.rs` (`organizations_view`, `organization_seats`, `OrganizationAssign`, `MoveAccount`,
  `RecallAccount`), `projects.rs` (`project_view` warnings), `launch.rs` (`project_allows`); CLI
  `org assign --also --team`; MCP `switchboard_organization_set` `also`, `team`; the window's
  *Add a seat* (`seatDialog`, `seatChoices`), the team box and `assignNotice`.
- Contract: [CONTRACTS](../../CONTRACTS.md#an-organization-is-a-label-sb-403-dec-0022-2026-11-17).
- Scenarios: SCN-062, SCN-069, SCN-070, SCN-076, SCN-077 in [ux/scenarios.md](../../ux/scenarios.md).
- Deferred to later waves or rows: a live check on the operator's Mac after the release (the
  Appvillis card should read *Done*); the Statistics *By organization* table still attributes a
  request to the account's first organization (unchanged by S1).
