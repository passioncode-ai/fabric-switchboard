# Two machines, one estate: Switchboard on the MacBook and the Mac Studio

Design, 2026-10-23, for SB-168 (RM-21 stage 1, the Mac Studio move). Status: decided here,
built in the phases of §6. Source of the question: passioncode-vesta readiness report
`docs/reports/2026-10-22-ecosystem-readiness/README.md` §4 item 6 ("One subscription account on
two machines"), board thread `b2511678`.

## 1. What is true today

- **Accounts do not move.** An encrypted backup is sealed with a key the OS keeps for this
  machine (`crates/switchboard-core/src/backup.rs:1-4`: a Keychain item through
  `/usr/bin/security` on macOS, a DPAPI file on Windows, `backup.rs:706`); OPERATIONS.md
  (*Backing up app data*) says moving accounts to another machine is not supported. The Studio
  starts with no accounts, groups, rules or settings.
- **Every machine renews its own inactive Claude accounts** (`crates/switchboard-runtime/src/refresh.rs:1`,
  PLAN-0.5 D-2). A Claude refresh issues a new refresh token and ends the old one, so one account
  signed in on two machines is renewed from two places, and the machine that renews second finds
  its token spent: that machine's sessions on that account sign out (SB-21 history, CO-324 note).
- **Limits are the account's, not the machine's.** Two machines on one subscription share its
  five-hour and weekly windows; neither sees the other's use.
- **Groups name folders on this disk** (`Project.folders`, absolute paths). The Studio's checkouts
  sit at other paths or not at all, so a group copied as written binds nothing.
- **Accounts already carry a stable identity** (`Account::external_identity`,
  `crates/switchboard-core/src/lib.rs:238`: provider account id, organization id, e-mail), which
  says when two machines hold the same subscription.

## 2. Decisions

| # | Decision | Why |
|---|---|---|
| D-1 | **Each machine has its own seats.** The Studio signs in to accounts of its own (a separate pool of subscriptions, or separate seats of a Team), the readiness report's own recommendation. One subscription is active on one machine at a time. | Two machines on one subscription halve its limits for both and race on its refresh lineage; no client-side design removes either. |
| D-2 | **Never renew an account another machine holds.** An account marked *in use on another machine* is not renewed, not rotated to and not switched to on this machine; a session already on it keeps its token until that expires. | A renewal here signs the other machine out (§1). Failing closed costs a sign-in on this machine at worst; failing open costs the other machine's running sessions. |
| D-3 | **The mark is explicit first, shared later.** Phase 1: the person marks an account *Used on another machine* (account ⋯ menu, CLI, MCP). Phase 3: each machine publishes the identities it holds (provider + account id digest, never a token or an e-mail) to the estate through Fabric's hub, and the mark follows them. | No channel between the two machines exists yet (readiness report §4 items 1 and 2: the hub and the relay are undecided or not live). An explicit mark works now and stays correct later. |
| D-4 | **Credentials never travel; settings do.** Moving to the Studio is a fresh official sign-in per account (a human gate, recorded as such). Groups, rules, fallback chains, agent settings and organizations move as a settings bundle with no credential, and a group's folders are matched on the Studio by repository identity (DEC-0089 item 2, `suggest::normalize`): a folder joins where a checkout of the same repository exists, the rest are listed as not found. | A fresh sign-in gives the Studio its own refresh lineage (D-1, D-2). Matching by identity is the rule the three products already share (SB-141). |
| D-5 | **What the window shows.** An account marked in use elsewhere reads *In use on another machine* on its card, is skipped by rotation with that reason, and a manual switch to it is refused with “This account is in use on another machine. Use one of this machine's accounts, or clear the mark if it moved here.” | The person sees why nothing happens, and the one way out. |
| D-6 | **Prove the risk before S1.** One operator-assisted acceptance with a spare Claude account signed in on both machines: renew on one, observe the other (401 within one request, or not). The result goes in `docs/evidence/` and either confirms D-2 or relaxes it. | §1's lineage claim comes from the SB-21 history on one machine; it has not been measured across two. |

## 3. Not decided here

- Which Fabric holds the estate's database and hub when the Studio runs (readiness report §4
  item 1): Fabric's ADR. Phase 3 waits for it.
- Seats bought for the Studio: the operator's spending decision.
- Codex: a ChatGPT sign-in on two machines has its own token rules (SB-16 packet); D-2 applies to
  Codex the day Switchboard renews Codex accounts.

## 4. Data

- `Snapshot.elsewhere`: account id → when it was marked, in `accounts.json` (schema version 8
  while one exists); backups carry it, so a restore on the same machine keeps the mark. Phase 3
  adds who marked it (the person or the estate).
- Settings bundle `switchboard-settings/1`: `{groups: [{name, pool, organization?, repositories:
  [identity], plain_folders: [path]}], rules, chains, agents_settings, organizations}`; no account
  id, no credential, no capability. Import matches repositories by identity under the folders the
  person names (default `~/DATA`), lists what it could not place, and creates groups empty of
  accounts (the person assigns this machine's seats).
- Phase 3 publication: `{machine, identities: [sha256(provider + ":" + account_id)]}` on the hub;
  read every 10 minutes while the hub answers; an identity seen on another machine marks the
  account `by: estate`; a stale hub never clears a mark.

## 5. Risks

- A person forgets to mark a shared account: phase 1 catches the sign-in case only. Phase 3
  closes it; D-6 tells how much it costs meanwhile.
- A settings import creates groups the person did not want on the Studio: import is a preview
  first, applied only on confirm, like `switchboard uninstall` without `--yes`.

## 6. Phases and rows

| Phase | What | Row |
|---|---|---|
| 1 | The *Used on another machine* mark: `Snapshot.elsewhere` (schema 8), the account disabled here (so no renewal, rotation or switch), a refused switch with its reason, the card's badge and *Use here*, CLI `accounts update --elsewhere`, MCP `switchboard_account_update.elsewhere`, kept by backups (SCN-072); built in 0.6.74 | SB-175 |
| 2 | Settings bundle export and import with repository matching and a preview | SB-176 |
| 3 | Identities published and read through Fabric's hub | SB-177 (waits for Fabric's hub ADR) |
| 4 | D-6 acceptance on two machines | SB-178 (operator-assisted) |
