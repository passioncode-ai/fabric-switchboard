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

- An organization created (one domain).
- Four accounts on another machine's user (in its machine group) and one account here (moved home
  into the group bound to the organization, 7 folders) work for it. Each keeps the personal
  context first (`organizations: [personal, <organization>]`): assigning the organization alone was refused
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
- **D-16 (S1 round 2).** *Add a seat* adds one account to the group as a delta (`project_add_accounts`,
  the window's door to `ProjectEdit { add_accounts }`), never a save of the whole group from the
  window's last read, which could drop a folder or account found meanwhile.
- **D-17 (S1 round 2).** The account the ordinary Claude Code or Codex is signed in to cannot join a
  group: the seat step never suggests it (`left_out`, `reason: in_cli`) and the dialog says why.
- **D-18 (S1 round 2).** With a group here and every seat on another machine, the seat step needs the
  person (a seat elsewhere does not work here) and offers the call back among its alternatives; it is
  optional only when no group exists here.
- **D-19 (S1 round 2).** An account placed in a personal group while it works for an organization gets
  its own warning wording ("the personal group … uses it anyway").
- **D-20 (S1 round 2).** Isolated homes (`homes/<account id>`) stay one per account, not per context:
  re-keying would move every existing home and sign-in. Managed launches run in each group's runtime
  and are not affected. Recorded as a known limitation in DEC-0022 and CONTRACTS; a board row follows.

- **D-21 (S2).** `ssh-serve` answers exactly the calls `accounts send`, `accounts recall`, the
  machine proof and the background read make, plus the lease: `machines prove`, `accounts list`,
  `project list`, `org list`, `org assign` (a seat of the caller), `accounts add --delegated`,
  `usage`, `accounts surrender`, `accounts lease`, `accounts remove`. Lists are cut to the caller's
  seats and to ids and names (no folder, no other account). `status`, `sessions`, `launch`,
  `settings import` and `org add` are refused, so the administrator's Sessions page,
  `machines --live` and settings sync do not reach a teammate's computer. The command is read as
  single-quoted words, never through a shell; anything a shell would expand is refused.
- **D-22 (S2).** The sender's identity is its settings signing key (SB-272), named in the forced
  command (`--sender-key`); every call that touches a seat names it too and must equal it. The same
  words work over a plain ssh (the administrator's own second machine), where they are only a claim.
  The receiving side proves itself as before (SB-357).
- **D-23 (S2).** A guest stays in the group it was delegated into (not only "never the default
  group"): no move, share or loan at all, so the rule is one line checked on every publication
  (`delegation::stays`); a backup never carries a guest.
- **D-24 (S2).** Past the lease the receiver deletes the credential first, then switches the seat off
  (a crash between ends it again). A lease that ended is never revived; the sender calls the seat
  back, which then asks the sign-in kept on the sender since the move. If the receiver had renewed it
  meanwhile that copy is spent, and the account is signed in again on the sender
  (`LEASE_ENDED_SIGN_IN`). Accepted: a revocation that works while the sender is offline cannot keep
  a refresh token alive on the receiver.
- **D-25 (S2).** A guest's call back switches it off on the receiver (it is never marked used
  elsewhere, which would break D-23) and answers its tokens after any renewal in flight settles.
- **D-26 (S2).** The lease is extended by the background read of machine groups (DEC-0017, every
  15 minutes) after a read that answered; the lease's hours and the sender's public key are kept on
  the move's `seats.jsonl` line. A failed extension is logged and tried at the next read.
- **D-27 (S3).** "Own sign-in off" masks the ordinary-sign-in sources in the owner and offline
  (`own_sign_in::mask`), skips the SB-358 MCP sign-in copy at launch, and limits the duty officer to
  sessions in Switchboard's own homes (it would otherwise close the teammate's own Claude Code
  sessions). Session transcripts (limit markers), Codex's routing file (opt-in SB-143) and the
  person's explicit `switchboard mcp-homes` are unchanged.
- **D-28 (S2).** `ssh-serve` runs on macOS and Linux; on Windows it answers `UNIX_ONLY`.
- **D-29 (S4).** The window offers *Delegate to a teammate* in both move dialogs (an account's
  *Move to another machine…* and a machine's *Move an account here…*), and *Your own sign-in* in
  Settings (DEC-0019: MCP `switchboard_account_send` and `switchboard_own_sign_in` do the same).

### Review round 1 of PR #810 (2026-11-17)

- **D-30 (P1-1).** The key line quotes the program inside `command="…"` (sshd runs it through
  `$SHELL -c`; the app's path holds a space) and adds `restrict`; the guides say to replace an
  older line for the same key (sshd takes the first match). `ssh_serve::forced_command` builds it;
  a test runs each guide's line through `/bin/sh -c`.
- **D-31 (P1-2).** A failed delegated move removes the copy there under the sender's key;
  `LEFT_THERE_DELEGATED` says the copy switches off by its lease when even that fails.
- **D-32 (P1-3).** A lease ends only once the receiver's owner ran 15 minutes past both the lease
  and its own start; it switches the seat off and cleans its homes but keeps the credential 30
  days, during which an extension revives it and a call back takes the live sign-in; then the
  credential is removed. The sender does not pull tokens at each read (one live copy, fewer
  tokens on the wire).
- **D-33 (P1-4).** The receiver's seat operations have no local CLI command and are refused unless
  they say `ssh-serve` sent them (`served`); a process holding the control token can say so too
  (round 2, P2-A, and D-40). Threat model stated in DEC-0025 and ORGANIZATION.md: the
  teammate owns the machine; the provider sign-out is the revocation against an uncooperative one.
- **D-34 (P2-1).** A delegated seat launches only in managed mode (`DELEGATED_ISOLATED`); its
  isolated home is cleaned at lease end.
- **D-35 (P2-2).** `ssh-serve --pool GROUP` pins the one group a key may delegate into and see;
  `project list` and `org list` show that group and its organizations only, and `org assign` takes
  only those (or personal).
- **D-36 (P2-3, P2-4).** With own sign-in off the bindings treat the ordinary Claude Code as signed
  out (placeholder token in the group's folders) and `mcp-homes` and a launch leave the ordinary
  home out.
- **D-37 (P2-5).** The sender keeps each delegated seat's id there, lease and key in the store
  (`Snapshot::delegated_out`), carried by backups; `seats.jsonl` is only the fallback.
- **D-38 (P2-6).** Deleting a group that holds a guest answers `GROUP_HOLDS_GUEST`.
- **D-39.** The decision is DEC-0025 (DEC-0024 is SB-404's, PR #809).
- **D-40 (round 2, P2-A).** `ssh-serve` gets no separate proof of its own. It runs as the teammate's
  user, so any secret it held to prove itself to the owner would sit beside `control.json`,
  readable by the same processes; it would add a file and a check without a trust boundary.
  The docs say instead what holds: the local CLI, MCP and window never send the receiver's seat
  operations; a process holding the control token can, with the same trust as the person's own
  Keychain; against an uncooperative teammate the revocation is signing the subscription out at
  the provider.
- **D-41 (round 2, P2-B).** A delegated move whose check failed and whose copy there could not be
  removed records the seat in `delegated_out` and a granted seat line (account there, lease, key),
  so `accounts recall` takes the sign-in back under its key. The account is not placed in the
  machine's group, so the background read does not extend its lease: left alone, the copy there
  switches off when the lease runs out. The answer says to call it back with `accounts recall`.
- **D-42 (round 3).** The record of a seat left there carries `sent`, the print of the token sent,
  so a call back after a renewal here keeps the live sign-in aside (P2-3). The adoption pass skips
  an account with a `delegated_out` record or a leased seat line outside a machine's group (P2-2;
  no distinct seat event, since five readers of `granted` would each need it). Using an account
  here again clears its `delegated_out` record whatever its group, and a call back that gives the
  sign-in back restores it, outside the machine's group when it was outside (P2-1). The seat line
  is written from the move's own answer (`SendError::left`), not from a read of the store.

## Waves

| Wave | What | State |
|---|---|---|
| S1 | Seats and onboarding: accounts that also work for the organization count; domains optional; the seat step's next steps; an organization never refuses a person's act (DEC-0022); the team follows only on request; moves and call backs place with a warning | done on branch `org-seats` (gate on the Mac Studio, see the backlog row) |
| S2 | `ssh-serve` restricted entry, `delegated` account kind and its refusals, lease and its renewal by the sender | done on branch `org-delegation` ([DEC-0025](../../DECISIONS.md#dec-0025-a-subscription-is-delegated-to-a-teammates-machine-as-a-guest-under-a-lease); review rounds 1 to 3 fixed) |
| S3 | "Own sign-in off" switch; proof that Claude Desktop and `~/.claude` are untouched (synthetic Keychain tests) | done on branch `org-delegation` |
| S4 | Teammate setup guide ([ORGANIZATION.md](../../ORGANIZATION.md#a-teammates-computer)), window and MCP for delegate and revoke, CONTRACTS | done on branch `org-delegation` |
| S5 | Live: a teammate's machine on the tailnet, one organization subscription delegated, a console session on it, Claude Desktop checked untouched, recall | needs the teammate's machine (human step: install, tailnet, ssh key) |

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
  organization's card should read *Done*); the Statistics *By organization* table still attributes a
  request to the account's first organization (unchanged by S1).

### S2 to S4 evidence

- Code: core `delegation.rs` (`Delegation`, `stays`, `add_delegated`, `extend_lease`, `end_lease`,
  `surrender_delegated`, `remove_delegated`), `lib.rs` (`Snapshot::delegated`, `remove` refusal,
  `assign_to_agent` refusal), `persistence.rs` (schema 16), `backup.rs` (guests left out); runtime
  `account_send.rs` (`send_as`, `Delegate`, `surrender_delegated`, `recall_take_as`,
  `extend_lease`), `remote_usage.rs` (`extend_leases`), `monitor.rs` (lease end), `refresh.rs`,
  `balancing.rs`, `own_sign_in.rs`, `duty.rs` (`keep_ours`), `launch.rs` (no MCP sign-in copy while
  off), `lib.rs` (operations `AddDelegated`, `SurrenderDelegated`, `LeaseDelegated`,
  `RemoveDelegated`, `OwnSignIn`, `SendAccount { delegate, lease_hours }`); CLI `ssh_serve.rs`,
  `accounts send --delegate`, `accounts lease`, `own-sign-in`, `ssh-serve`; MCP
  `switchboard_account_send` `delegate`/`lease_hours`, `switchboard_own_sign_in`; window
  `delegateFields`, `ownSignInSection`, Tauri `own_sign_in`.
- Contract: [CONTRACTS](../../CONTRACTS.md#delegated-seats-and-the-own-sign-in-switch-sb-403-s2-to-s4-dec-0025-2026-11-17).
- Scenarios: SCN-102, SCN-103 in [ux/scenarios.md](../../ux/scenarios.md).
- Deferred: S5, the live run on a teammate's computer (install, tailnet, the authorized_keys line
  are a person's steps on that computer); a Windows `ssh-serve` (D-28).
