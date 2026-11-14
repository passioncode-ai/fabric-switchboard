# WF-3 Attention (SB-365, wave 1)

> Подписки, которые у нас с ошибками, нужно залогиниться, их сразу поднимаем вверх, в отдельную
> категорию, типа "Attention" ... после залогинивания они возвращаются туда же, где были, в ту
> группу, где находились. В той группе они тоже видны, просто они как бы задизеплены, а в
> "Attention" это просто как дополнительная информация.

## Now

- `signInRequired` (`src/main.ts:1623`) reads `monitor.sign_in_required`; a row shows *Sign in again*,
  "Sign in to check usage" and a *Sign in* button (2154, 2278, 2194). There is no list of them.
- Signing in again keeps label and group (SB-62); nothing moves an account out of its group on a
  failure, so "back where it was" already holds and only needs to be proved.

## Sub-modules and tasks

**3.1 What needs attention.** One function in the runtime, `attention(snapshot, monitor)`, answering
each item with its reason: sign-in required, a seat ended in an organization, a renewal refused
(`invalid_grant`), a usage check failing for over an hour, a move whose outcome is unknown
(`seats.jsonl`), a remote machine's copy refused there (`remote check refused_there`, WF-4), a
spending ceiling's open question. Each with the one action that fixes it.

**3.2 The section.** At the top of Accounts, above *In use now*, only when something needs a person:
"Needs attention (N)", each row the account, the reason in one line, and its action (*Sign in*,
*Answer*, *Check again*). The account also stays in its group, disabled, with its badge (DEC-0015).

**3.3 Back in place.** After the action, the item leaves the list on the next refresh; a test proves
the account is in the same group with the same label and selection after a sign-in again.

## MCP

New tool `switchboard_attention` (and CLI `switchboard attention`): the same list with reasons and
the action's tool name. The window reads the same answer (no window-only logic).

## Decisions

DEC-0015, Attention is a view, not a group.

## Checks

Runtime tests for each reason; the window test that an account appears in both places; SCN for
Attention; Copylot for each reason line.

## Depends on

WF-1.2 (section), WF-1.3 (tool lands with the command).
