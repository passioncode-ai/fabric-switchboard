---
report:
  id: fabric-switchboard/2026-11-21-subscription-runway
  title: "Subscription runway: seven days of quota per group and machine, a reserve for groups with an agent on duty, and a sentry"
  kind: analysis
  project: fabric-switchboard
  domains: [accounts, reliability]
  as_of: 2026-11-21
  status: active
  valid_until: 2026-11-28
  summary: >-
    On the operator's estate (57 Claude subscription accounts in use: 43 on the MacBook, 14 on seven
    remote spaces) no group is without a live account on 2026-11-21, and at the pace of the last
    24 hours (about 5,600 list-price dollars a day) none will be for seven days. At the pace of
    17 to 19 November (about 26,500 a day) the estate is short by roughly half, and under the
    feed's old priorities the light groups that carry support agents were the ones left with
    nothing. Priorities, the reserve, the fallback chain and two remote spaces were changed
    through Switchboard's own settings, and an hourly sentry now warns 12 hours ahead.
  sources:
    - {name: "switchboard --json accounts list, here and over ssh on seven machines", path: "scripts/ops/quota_sentry.py", read_at: 2026-11-21}
    - {name: "Relay counters statistics.json (requests and list-price cost per day, account, group)", path: "docs/CONTRACTS.md", read_at: 2026-11-21}
    - {name: "Feed state and journal (switchboard feed status)", path: "crates/switchboard-runtime/src/feed.rs", read_at: 2026-11-21}
  produced_by:
    agent: claude-code
    task: "operator order relayed by the Fabric CEO, board thread 9a52f56d"
  supersedes: []
  consumers: [fabric, studio-agent]
---

# Subscription runway, 2026-11-21

The operator's order, as the Fabric CEO relayed it (2026-11-21, after a support alert that a
customer had waited 143 minutes for a repair session): check that every support agent is at its
post, check the subscriptions in every group and on every remote machine, plan them ahead by
their five-hour and seven-day resets, and spread them so that no work stands idle.

This repository is public, so this report names no account, host, private agent or project.
Groups are named by their role. The full map with names is in the sentry's private folder on the
operator's machine (`~/Library/Application Support/switchboard-quota-sentry/`) and on the board
thread named above.

## The main points

1. **No group is out of quota now.** Every group on the MacBook and every remote space had at
   least one account with room at 13:30 UTC. The alert that started this was the support bot's
   own gate (busy, daily limit or switched off), not an empty group.
2. **The displayed staleness was local.** The remote spaces' own Switchboards had figures 0 to 7
   minutes old; only this machine's copy of them was 1 to 4 days old (it shows the last
   observation taken before the account moved). 27 stale local observations were refreshed at
   the provider's pace of two a minute.
3. **At today's pace the estate holds for seven days, at last week's pace it does not.** See
   "Capacity" below. The gap at the higher pace is the operator's decision (buying
   subscriptions is a payment) and was put to him as a question.
4. **The feed took support groups' accounts for a heavy group.** Its journal of 2026-11-21 shows
   the second-priority group taking accounts from the three lowest-priority groups by
   `priority`, and returning them `spent`. Those three groups carry support agents and spend 1
   to 3 % of what the heavy groups spend. Under that order the simulation leaves them with no
   account for 140 to 158 of 168 hours at last week's pace.
5. **One support agent's folder was in no group**, so it ran on the machine's ordinary sign-in
   with everything else outside groups.
6. **One remote space had a single account that would run out about 33 hours before its weekly
   reset**, and one remote group's only account was spent until 2026-11-24 with its feed off.
7. **One sign-in is held on two machines at once** (`switchboard remote check` reports
   `held_twice`). Not changed here; backlog row SB-625.

## Data and method

- **Quota**: `switchboard --json accounts list` here, and the same command over ssh on each of
  the seven machines (their own figures, with reset times, which the one-minute machine read does
  not carry). Snapshot at 13:29 UTC, again after the refresh.
- **Spend**: Switchboard's relay counters (`statistics.json`: requests, limit refusals and the
  list-price cost of each answer per day, account and group; cost is recorded since 2026-11-17).
- **Calibration**: for 28 accounts with 15 % or more of the weekly window used, cost inside the
  window divided by the used share. Median 19.9 dollars per 1 %, quartiles about 15.6 and 23.5:
  one weekly window holds about 2,000 list-price dollars. Two accounts read 1 to 3 dollars per
  1 %: they served sessions outside the proxy, which the counters do not see.
- **Model** (`scripts/ops/quota_sentry.py`, `forecast`): hour by hour, each group spends its
  daily figure evenly, from its own accounts first (soonest reset first), then from the default
  group's lendable accounts in feed priority order. A weekly reset refills an account. One
  five-hour window is taken as 10 % of the weekly one. **That share is an assumption**: the data
  here cannot measure it. A remote space spends at the pace its accounts' weekly windows show.
- **Not modelled**: bursts (spend is spread evenly over the day), sessions on the ordinary
  sign-in outside the proxy, and the feed's rule that an account in use for 30 minutes is not
  taken.

## Capacity

Spend through the proxy on the MacBook, list-price dollars a day:

| Day (UTC) | 11-17 | 11-18 | 11-19 | 11-20 | 11-21 (14.4 h) |
|---|---|---|---|---|---|
| Spend | 18,208 | 26,034 | 27,666 | 6,397 | 3,323 |

- Supply: the 43 accounts here hold 12,300 dollars a day when every window is used in full;
  all 57 accounts hold 16,300.
- **Today's pace** (about 5,600 a day, after the operator's pause of 2026-11-21): 20 accounts'
  worth. Surplus.
- **Pace of 18 and 19 November** (about 26,500 a day): 93 accounts' worth. Short by about 36
  accounts with every account counted, by 50 on the MacBook alone.
- **Mean of 17 to 20 November** (19,600 a day): 69 accounts' worth, short by about 12.

Seven-day simulation on the MacBook, hours of 168 in which a group cannot be served:

| Group (role) | Today's pace, old order | Peak pace, old order | Peak pace, new order and reserve |
|---|---|---|---|
| Heavy development group 1 | 0 | 32 | 78 |
| Heavy development group 2 (also carries one support agent) | 0 | 111 | 146 |
| Organization group with a PM agent | 0 | 136 | 9 |
| Support group 1 | 0 | 144 | 0 |
| Support group 2 | 0 | 156 | 0 |
| Support group 3 | 0 | 158 | 0 |

The new order moves the shortage from the groups that answer customers to the groups that
develop. It does not remove it: at the peak pace only more accounts or less work does.

## What was changed, through Switchboard's settings

Every change is reversible and none touched the machine's ordinary Claude Code sign-in.

| # | Change | Before | Reason |
|---|---|---|---|
| 1 | Feed priorities | heavy groups 1 and 2, support groups 4 to 6 | A higher group may take a lower group's last account; support groups now rank 1 to 3, the organization group 4, the heavy groups 5 and 6 |
| 2 | Six accounts moved from the three support groups to the default group | four accounts each | Each support group spends under 170 dollars a day against 570 that two accounts hold; it keeps two (the routed one and one with a staggered reset), and the default group's ready accounts went from 1 to 3 |
| 3 | Machine fallback chain | Claude Code, Kimi Code, Hermes | Codex added second (preset `subscriptions-first`): its account was unused, with its window open until 2026-11-27 |
| 4 | A support agent's folder added to its product's group | in no group | It no longer shares the ordinary sign-in with every folder outside groups |
| 5 | One account sent to the remote space with a single account | one account, runway about 2.9 days, reset in 4.3 | Two accounts there now |
| 6 | Feed turned on for the remote group whose only account is spent | off | It can borrow that space's default account until 2026-11-24 |

Not changed: rotation thresholds (85 and 90 %, every group on), spending ceilings (no paid
fallback was reached), usage mode (`smart`).

## The sentry

`scripts/ops/quota_sentry.py` runs every hour as the LaunchAgent
`me.sshlg.switchboard-quota-sentry` on the operator's machine. It forecasts 72 hours for every
group and machine listed in its duty file and tells the operator through his notification
channel when a unit's runway is under 12 hours: once when it crosses the line, again after 6
hours while it stays under it, once when it recovers. A source that did not answer is named and
its units read `unknown`, never healthy. It changes nothing in Switchboard. Tests:
`python3 -m unittest scripts/test_quota_sentry.py` (13 tests).

Its first forecast (2026-11-21 13:50 UTC): every one of 13 duty units holds the 72-hour horizon
except the remote space of item 5, at 70 hours on a deliberately high estimate of its pace.

It is operator tooling, not the product: Switchboard itself sends nothing. The product's own
form of this is backlog row SB-623 (a `quota_runway` check in the health report).

## Conclusions for the project

- The feed has no floor for a group: priority alone decides whose last account is taken. A
  per-group minimum that the feed never takes (SB-624) would replace the hand-made reserve of
  item 2.
- The machines list shows an account's last local observation after it moved, for days. The
  one-minute read already brings the used share; it should bring the reset times too and the
  usage view should show them (SB-623 covers the data it needs).
- `held_twice` is found only when someone runs `remote check`; it belongs in Needs attention
  (SB-625).

## Sources

- `scripts/ops/quota_sentry.py` and `scripts/test_quota_sentry.py` at this report's commit.
- `crates/switchboard-runtime/src/feed.rs` (`View::readiness`, `ready_count`) for what the feed
  counts as ready.
- Raw snapshots, the account table, the calibration list and the action log with times are kept
  outside Git, in the sentry's private folder, because they name accounts and hosts.

## Corrections

- **2026-11-21, 14:16 UTC.** The owner set the order after this cut was written (relayed by the
  Fabric CEO at about 14:15): the group that carries the Fabric release first, the groups with
  support agents second, everything else third. The feed priorities of row 1 in "What was
  changed" were set again accordingly: the release group (heavy development group 1 here) is 1,
  the three support groups and the organization group 2 to 5, heavy development group 2 is 6.
  Two accounts with room went from the default group into the release group at once (two of its
  four were at their five-hour limit), and the folder of the session that leads the release,
  which was in no group, joined it. The simulation table above describes the order of 13:36,
  not this one: under the owner's order the shortage at the peak pace falls on the third tier
  first, then on support. One support agent's folder lies in the third-tier group; giving it a
  group of its own is the owner's decision (SB-624 covers the floor).
