---
report: fabric-switchboard audit of accounts across machines and organizations 2026-11-10
project: fabric-switchboard
date: 2026-11-10
source: operator in chat 2026-11-10 («проведем аудит по аккаунтам, переносам на студию, обратно в организацию … слинкован ли фабрик на студии со свитчбордом у меня здесь, и как информация между ними синхронизируется … где-то MCP доделать … чтобы все сессии хранились, правильно распределились, переносились, не терялись, не требовались перелоги»)
---

# Accounts across this MacBook, the Mac Studio and organizations

Read on 2026-11-10 between 20:00Z and 21:30Z with 0.7.49 on every copy. Commands are given with
what they printed; ids are shortened to 8 characters.

## 1. What runs where

| Copy | Who | Accounts | Groups (accounts in them) | Organizations | PassionCode.ai |
| --- | --- | --- | --- | --- | --- |
| this MacBook | `sshlg` | 32 (claude 31, codex 1); 11 marked used elsewhere | agents (11), apps-soft (2), passioncode (17) | PassionCode.ai `e2cb7356`, linked to PassionCode.ai (`vesta_id` set) | connected, read `ok` |
| Mac Studio | `sshlg` | 4: claude6 (off), contact@sshlg, claude13, claude14, all in `default` | agents (claude6 only, off), apps-soft (0), passioncode (0), passioncode-ai (0, bound to the organization) | a local PassionCode.ai `60968201` (another id, not linked) | not connected |
| Mac Studio | `passioncode` | 2: contact@passioncode.ai, vibecoder@passioncode.ai, in `default` | none | none | not connected |
| Mac Studio | `appvillis` (`studio-appvillis`) | 4: marketing@, sergey@, investor.relations@, billing@appvillis.com, in `default` | appvillis-agents, appvillis-platform, ng-business, nicegram-apps, esim (0 each) | none | not connected |

Sources: `switchboard org list --json`, `switchboard accounts list --json`, `switchboard project list --json`
on each copy over ssh; `switchboard_accounts`, `switchboard_organizations`, `switchboard_seats` here.

**The copies are not linked.** Each Switchboard keeps its own accounts, groups and organizations.
Nothing syncs by itself. What crosses between them is only what this MacBook starts over ssh:
`accounts send` and `accounts recall` (the sign-in, its label and group), `dispatch --target` (a run),
and by hand `settings export | settings import` (groups, organizations by name, policies). The seat
journal `seats.jsonl` here is the only record of which account went where; the Studio's copies do
not know where they came from.

## 2. What the operator asked, scenario by scenario

### S1. Accounts in a group, then placed in an organization (here)

What happened on 2026-11-10: vibecoder@, vibecoder2@ and contact@passioncode.ai were placed in
PassionCode.ai (`by: person`). This MacBook's PassionCode group is personal, and a group takes only
accounts of its own organizations once one exists (SB-109). So vibecoder2 (31 % left, enabled) left
the PassionCode group's rotation without a word: `switchboard onboard --status` said
`org_group needs_person: PassionCode.ai has no group on this computer yet`, and no answer named the
group it left.

Fixed here (live): the three accounts are now personal and also work for PassionCode.ai
(`switchboard org assign <id> personal PassionCode.ai`, answers `also: ["e2cb7356…"]`), as the
operator decided on 2026-11-09 («были на моих личных и на passion-коде остались», `ffa10a7e`).
`switchboard_accounts` now lists vibecoder2 with `organization: null`, `also_organizations:
[PassionCode.ai]` in group passioncode.

Two defects found on the way, fixed in SB-323:

- `org assign <id> PassionCode.ai personal` answered `also: []`: `personal` named second was
  dropped with no word (`assign_organizations` skipped it). It is now refused with how to say it.
- The window's *Organization…* dialog showed *Also works for* only with two organizations or more,
  so with one organization there was no way to keep an account personal and working for it. It now
  shows with one, and says before *Save* which groups the choice would leave. `org assign` and
  MCP `switchboard_organization_set` answer `refused_by` with those groups.

### S2. An account moved to the Studio, then placed in an organization here

contact@ and vibecoder@passioncode.ai were sent to `passioncode@100.69.20.19` (seats `granted`
2026-11-09 18:45Z and 2026-11-10). Placing them in PassionCode.ai here changed only this MacBook's
record. On the Studio's `passioncode` copy there is no organization at all and both are personal.

`accounts send` carried only the label and the group. Fixed in SB-323: a move now carries the
person's word on the account's organizations (adds an organization the other copy lacks by name,
then places the account there). Still open (SB-324): a word said here after the move does not
reach the copy there.

### S3. Are the organizations the same on both machines?

No. The Studio's `sshlg` copy has a PassionCode.ai made by a settings import (matched by name,
its own id, `passioncode: false`); the other two copies have none. Organizations match by name
everywhere (settings import, and now a move), so the same name is the same organization in
practice; but nothing keeps them in step, and none of the Studio copies is connected to
PassionCode.ai (connecting is a browser sign-in, a person's step). SB-326.

### S4. Do sessions on the Studio get an account?

Only when Switchboard starts them. In 2 days the Studio's `sshlg` copy launched 4 sessions, all
`dispatch` tests on `default` accounts (`switchboard sessions --history --json`: total 4); the other
two copies launched none; the proxies relayed nothing (`switchboard statistics --days 2`: empty).
Every group on the Studio holds no account, so its folders get no route
(`/Users/sshlg/DATA/fabric` has no `.claude/settings.local.json`), and the ordinary Claude Code is not
signed in on any of the three users (`credential_source claude absent` in each log). A plain `claude`
in a group folder there would ask for a sign-in. SB-325.

### S5. Are moved accounts lost or asking for a new sign-in?

| Account | Here | There | State |
| --- | --- | --- | --- |
| claude6@sshlg.me | used elsewhere, `sshlg@100.69.20.19` | off in `agents`, usage check `failed` 31 h ago | used by nobody; its sign-in there is refused, so it needs a sign-in |
| claude10@sshlg.me | used elsewhere, **no location**, no seat line | on no Studio copy | an orphan mark: nobody renews it; where it went is not recorded |
| the other 9 moved accounts | used elsewhere with their location | enabled, usage `ok` within 3 min | fine; renewed there (`renewal renewed` in the logs) |

The renewal and identity fixes of SB-311 and SB-312 hold: every moved account that is in use there
renews. SB-327 covers a check that would have found claude6 and claude10.

### S6. Is the capacity where the work is?

The Studio holds 9 enabled accounts with almost nothing running on them (every 5-hour window at
0 %; weekly use 0 % to 88 %), while on this MacBook 8 enabled accounts are at 0 % left and the
groups still have headroom (claude9 94 %, claude12 72 %, claude2 66 %, claude15 53 %). The operator's
rule (2026-11-09, `ffa10a7e`): accounts follow the work to the Studio, and the Studio keeps some to
watch. No work moved, so nothing needs to move now; when a MacBook group runs out, call back the
Studio accounts with the most left (claude14, billing@, marketing@).

## 3. Agents' doors (MCP)

Every move action has an MCP tool: `switchboard_account_send`, `_recall`, `_hand_over`,
`_receive`, `_update` (elsewhere, location, assigned_to), `switchboard_seats`,
`switchboard_organization_set`, `switchboard_settings_import`. Missing:

- reading another copy's state (accounts, groups, organizations, usage) without ssh by hand: this
  audit ran a dozen ssh commands (SB-326, SB-327);
- `settings export` (deliberately person-carried today) and so any agent-run sync of organizations
  and groups to the Studio (SB-326);
- removing an account (`accounts remove`), which the window offers: a dashboard-parity gap (SB-328).

## 4. Other findings

- The Studio's `sshlg` owner logs `proxy_refused_foreign_user` once an hour at :06: a process of
  another macOS user reaches its proxy port and is refused (as it should be). Which one is not known
  (SB-329).
- `accounts list --json` (CLI) has no `organization` or `also_organizations`; MCP
  `switchboard_accounts` has both (SB-328).

## 5. Board

| Row | P | What | State |
| --- | --- | --- | --- |
| SB-323 | P1 | a move carries the account's organizations; `personal` second refused; groups left named (CLI, MCP, window) | this change |
| SB-324 | P2 | a word on an account's organizations said after it moved reaches the copy there | open |
| SB-325 | P2 | Studio groups hold no account, so their folders have no route and no sign-in | open, decision |
| SB-326 | P2 | keep organizations and groups in step across the operator's machines (agent door for settings sync; PassionCode.ai link there) | open |
| SB-327 | P2 | read and check another copy from here: orphan marks, accounts off on both sides, copies not marked | open |
| SB-328 | P3 | MCP `accounts remove`; `accounts list --json` organizations | open |
| SB-329 | P3 | hourly `proxy_refused_foreign_user` on the Studio's `sshlg` owner | open |
