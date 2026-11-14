# WF-6 Backup group (SB-368, wave 3)

> добавить возможно группу backup аккаунтов это аккаунты которые не используются пока ... когда
> ... не заканчивают аккаунты тогда они туда переключаются и поддерживают эту группу пока там не
> появится свой аккаунт потом возвращается обратно в бэкап

## Now

Balancing (SB-352, DEC-0012, `crates/switchboard-runtime/src/balancing.rs`): every 15 minutes a
chosen group out of room borrows the account with the most room from another chosen group that keeps
two accounts with room; the loan returns after 2 hours without pressure or when the lender needs it.
Every lender is a working group; no group exists only to lend.

## Sub-modules and tasks

**6.1 Decision (DEC-0016).** A group can be marked *backup*: it holds no folders, starts no session,
is never selected for a launch, and is always a lender to the chosen groups. Its accounts keep their
home there. It keeps no minimum (DEC-0012's "two with room" does not apply to it). A loan from it
returns when the borrowing group has an account of its own with room for two passes in a row.
Organization boundaries hold (a backup group of personal accounts lends to personal groups).

**6.2 Core.** `Project.backup: bool` (schema version 15), refusals (`backup` with folders), the
balancing pass reads backup groups first as lenders, loan records say `from_backup`.

**6.3 Window and MCP.** Settings → *Balancing and backup* (WF-1.1): mark a group backup, see where
each backup account is lent now. Accounts page: a backup group's section says "Backup: 2 of 3 lent".
MCP `switchboard_balance` gains `backup` (add, remove, status).

## Questions

Lending across organization contexts: assumed no (README question 4).

## Checks

Core tests: a backup account lent and returned, never selected for a launch, never lent across
contexts; ui-logic; SCN-090 extended; release note.

## Depends on

WF-1.1 (Settings section).
