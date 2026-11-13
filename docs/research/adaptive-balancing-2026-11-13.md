# Adaptive balancing of accounts across chosen groups (SB-352)

Design, 2026-11-13. Source: the operator in session `59745113`, 2026-11-13 ~01:25Z, answering how
new accounts should be spread: «адаптивная система на основе данных за прошлые часы и дни работы и
если она включена то подстраиваться так чтобы перемешивать аккаунты между выбранными группами чтобы
они могли если что адаптироваться к нагрузке в одной из них, но если группу не выбрали не трогаем,
сделать такой выбор групп которые участвуют».

## The problem it solves

Automatic switching ([DEC-0008](../DECISIONS.md#dec-0008-automatic-switching-of-managed-sessions-is-on-by-default))
and the one move of a refused request ([DEC-0002](../DECISIONS.md#dec-0002-a-refused-claude-code-or-codex-request-moves-once-to-the-groups-next-account))
stay inside one group. On 2026-11-12 22:55Z twelve agents of the group Agents stopped at once on
their limit while Apps & Soft held an account with 97 % of its five hours left; an agent of this
repository had to share that account by hand (`project edit --pool agents --add-account`) and take
it back two hours later. The same happened the other way the day before. Nothing moves room
between groups by itself.

## What it does

- **Off by default; the person chooses who takes part.** A setting holds the set of groups that
  take part (`balancing.groups`, empty = off). A group not in the set is never given an account,
  never lends one and is never read for the decision. The window shows the set on the Groups page
  (a switch per group, *Shares accounts with the other chosen groups*), the CLI is
  `switchboard balance groups [--add ID] [--remove ID]` and `switchboard balance status`, MCP
  `switchboard_balance`.
- **Lending, not moving.** An account keeps its home group; balancing only adds it to another
  chosen group as a shared account (SB-282, the mechanism `project edit --add-account` already
  uses) and takes it back when the need passes. The account the ordinary Claude Code or Codex is
  signed in to is never lent, nor one given to an agent ([DEC-0010](../DECISIONS.md)), one used
  elsewhere, one disabled, one whose sign-in waits for a person, or one outside a group's
  organizations (SB-109: an organization's group takes only its organization's accounts).
- **The signal: the last hours and days.** Every 15 minutes, for each chosen group:
  - *pressure*: limit refusals (`limited`) and failovers in the last 6 hours, and whether its
    routed account is within 10 % of its group's switching threshold;
  - *demand*: requests per hour over the last 6 hours against its 7-day average
    (`statistics.json`, per group and day; the proxy's live counter for today);
  - *room*: each own account's fresh quota left in every account window
    (`Usage::account_used_percent`), counting only accounts below the group's threshold less its
    hysteresis (the same room as `rotation_decision` and SB-345).
- **The rule.** A chosen group *needs* room when its own accounts with room are none, or one while
  pressure is above zero. A chosen group *can lend* when, after lending, it still keeps at least
  two accounts with room (one when it has had no request for 6 hours). The neediest group (most
  pressure, then most demand) takes the lender account with the most room (lowest used share,
  then the latest window reset), one account per pass, so a pass never empties a group. A lent
  account goes back when the borrower has had no pressure for 2 hours, or when its home group
  starts to need it. Cooldown: an account lent or returned waits 1 hour before it moves again.
- **Every change is recorded and reversible.** Each lend and return writes a line to
  `balancing.jsonl` (0600: time, account id, from, to, the figures that decided it), the activity
  journal, and the Groups page shows *Lent by balancing* beside the account. `balance status`
  answers the last decisions with their figures. Turning balancing off returns every lent account
  at once.

## What it never does

- Touch a group the person did not choose, or change an account's home group.
- Cross an organization's boundary, lend the ordinary CLI's account, or lend an account given to
  an agent.
- Act on stale figures: a pass with no fresh quota for an account leaves that account where it is.
- Spend: balancing never starts a paid agent or an API-key account.

## Open before building

1. Decided: [DEC-0012](../DECISIONS.md#dec-0012-accounts-are-lent-between-the-groups-the-person-chose-by-their-last-hours-and-days)
   (the operator confirmed lending and every default above; on his MacBook Agents, PassionCode,
   Apps & Soft, default and every new group take part).
2. The super-ux scenario for the switch on the Groups page and its states (on, lent, returned),
   with its Russian copy through Copylot.
3. Whether a lent account's managed sessions in the borrower follow automatic switching at once
   (proposal: yes, the borrower's policy picks it like any shared account).
