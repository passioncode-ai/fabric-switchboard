# 09. Moving accounts between machines (operator's order, 2026-11-18)

## The order

«когда я делаю муфту машин то я не вижу какие там есть группы ничего должен угадывать короче
ничего не подтягивается вот это выбор вообще давай проработаем здесь фол переноса между машинами
чтобы было удобно быстро без всяких таких заморочиваний сложных», about the dialog *Move {label}
to another machine*, the same day he asked for three subscriptions on every machine.

## As it is (0.7.117)

- The account's menu and the machine row both open a dialog with two text fields: *Machine*, an
  input with a datalist of the machine groups only (`src/main.ts` `sendDialog`, `moveHereDialog`),
  and *Group there*, a free input prefilled with the group the account has here. Nothing is read
  from the other machine, so the person guesses the group's id. A machine known only from a run
  started there (`switchboard targets`, `known_by: dispatch`) is not offered.
- One account per dialog. Three subscriptions for each of six machines took eighteen dialogs.
- The move runs the macOS app's switchboard on the other machine unless the caller names
  another (`crates/switchboard-runtime/src/lib.rs`, `Operation::SendAccount`: `program`
  defaults to `account_send::REMOTE_DEFAULT`). A Linux machine runs `/usr/bin/switchboard`
  (the .deb), so every move there failed with "The other machine's Switchboard did not answer
  over ssh. Nothing changed", while `switchboard targets --probe` read it answering, and the same
  move with the program named succeeded. Every other ssh call already takes the program the
  machine is known by (`targets::program_for`, `remote_program`).
- A move from one machine to another already exists: the move calls the account back here
  first, then sends it (`SendAccount`, SB-355).

## The end form

1. **SB-473, the machine is chosen, not typed.** *Machine* is a list of every machine this
   computer knows: its machine groups and `switchboard targets` (moves and runs), each named as
   ssh knows it, with how it is known. The machine the account is on now is left out. *Another
   machine…* at the end opens the typed field, for a machine never reached before.
2. **SB-474, the group is read from that machine.** Choosing a machine reads its groups over ssh
   (`switchboard remote status`, the same restricted call): *Group there* becomes a list of its
   groups, each with its organization and how many accounts it holds, the default group first,
   and the group with the account's own group id preselected when that machine has it. Machine
   groups there are left out. While it reads: “Reading the groups on {machine}…”. When the read
   fails: one sentence and the typed field, as before.
3. **SB-475, the program the machine runs.** A move, from the window, the CLI or MCP, runs the
   switchboard the machine is known to run (the newest move or run there), and the macOS app's
   only for a machine never reached. The same rule every other call to that machine follows.
4. **SB-476, several at once, each step reported.** The dialog lists the accounts that can move
   (a subscription sign-in that is on, here or on another machine), with the one it was opened
   for checked. *Move* sends them one after another, never two at once; under the list each
   account's line says what happened: moved, came back from {machine} then moved, or why it
   failed. A failure stops nothing else; the dialog stays open with the lines until *Done*. An
   account on another machine moves straight on (it is called back first, as today).

## Acceptance

- The dialog has no field to type into unless the person chooses *Another machine…* or the
  machine's groups could not be read.
- A move to a Linux machine whose earlier move recorded `/usr/bin/switchboard` runs that
  program with no `remote` named (test `targets::tests::a_move_runs_the_program_the_machine_is_known_by`).
- Three accounts move to one machine from one dialog; one that fails leaves the others moved
  and says why.
- Scenarios SCN-091 and SCN-092 say the same; strings checked with Copylot in en-US and ru-RU.
