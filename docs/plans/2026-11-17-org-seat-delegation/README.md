# A subscription delegated to a teammate's machine, through an organization (SB-403, 2026-11-17)

Board row: [SB-403](../../evidence/backlog.md). Entry point for the next agent: this file, then the
wave table at the end.

## What was asked (the operator's words, 2026-11-17)

> Следующее, что я хочу сделать, это подключение машин сотрудников к организациям, которые есть.
> Мы создадим организацию Апфилис. Подключим у неё все аккаунты, которые у нас на машине Апфилис,
> и один аккаунт или два каких-нибудь свободных, которые будут стоять тут. И один из этих аккаунтов
> мы подключим так, чтобы можно было … связь между машинами: у Ростика … он хочет себе подключить
> вторую подписку, значит мы сделаем, чтобы он на машине конвертнул свой софт, подключил к
> организации, у него добавится подписки организации, которые делегированы ему, и я могу
> делегировать его на машину Ростика … и отзыв этой подписки … чтобы Ростик на своей машине мог
> ей пользоваться, но по факту авторизоваться он типа не будет с ней.

> Ростик может использовать Claude Desktop для своей обычной стандартной подписки, а вот эту
> использовать уже подключенную к свитчборду. Если этой будет мало, мы будем просто в свитчборд
> больше подписок ему добавлять … консольное на свитчборде, а десктопное на десктопе. А также
> убедиться, что мы не трогаем сессии для десктопного Claude Code, и он будет работать никак не
> связанно со свитчбордом.

> У нас может быть несколько e-mail на организацию, не обязательно только один … в организации
> может и не быть имейлов, мы можем вообще не привязываться к имейлу.

## Done on this Mac (2026-11-17, through MCP)

- Organization **Appvillis** (`90f2ff9f…`, domain `appvillis.com`) created.
- sergey@, twingpt@, marketing@, billing@ (on the Studio's `appvillis` user, machine group
  `studio-appvillis`) and investor.relations@ (moved home into the group `appvillis`, 7 folders,
  bound to Appvillis) work for Appvillis. Each keeps the personal context first (`organizations:
  [personal, Appvillis]`): assigning Appvillis alone is refused ("a group this account or its
  provider team serves would then refuse it"), and the groups that use them keep working.
- Every Claude account here was at 0 % of its weekly window (resets 2026-11-20) except three that
  serve Apps & Soft and Agents now; no account was taken from a group. The second "free" account is
  chosen when the windows reset.

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
  Claude Code sign-in, organization seats counting accounts that also work for the organization,
  and an onboarding seat step that never demands an organization e-mail.

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
  it (first or also) in its group; the seat step offers assigning an account already here, signing
  in with any address, or calling one back, never only "sign in with your address".

## Waves

| Wave | What | State |
|---|---|---|
| S1 | Seats and onboarding: accounts that also work for the organization count; domains optional; the seat step's next steps | next (live defect: Appvillis reads "no signed-in account") |
| S2 | `ssh-serve` restricted entry, `delegated` account kind and its refusals, lease and its renewal by the sender | after S1 |
| S3 | "Own sign-in off" switch; proof that Claude Desktop and `~/.claude` are untouched (synthetic Keychain tests) | with S2 |
| S4 | Teammate setup guide (`docs/ORGANIZATION.md` §4b), window and MCP for delegate and revoke, CONTRACTS | after S2 |
| S5 | Live: Rostislav's machine on the tailnet, one Appvillis subscription delegated, a console session on it, Claude Desktop checked untouched, recall | needs his machine (human step: install, tailnet, ssh key) |
