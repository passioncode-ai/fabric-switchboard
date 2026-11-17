# 06. Groups fed from the default group (operator's order, 2026-11-17)

## The order

«хочется, чтобы появилась автоматизация ротации из дефолтной группы в папке. Она включаемая и
выключаемая для каждой папки … если для папки включаем автоматизацию ротации из дефолта,
показываем стрелочками, что это связано … эти папки начинают обмениваться аккаунтами … задача
поддерживать папки, в которых включена автоматизация дефолта, всегда в рабочем состоянии … Те,
которые выбывают, они уходят в дефолтную папку отстаиваться, а на которые активные, и аккаунты,
которые скоро станут активными, наперед мы подготавливаем их, мы размещаем по этой папке
автоматически … У папок типа есть свой приоритет, когда остается там один аккаунт, условно он
уходит ту папку, в которой более высокий приоритет.»

## The end form

- **A switch per group: fed from the default group.** Off by default. On, the group and the
  default group exchange accounts so the group always has an account with room. The window
  draws an arrow from the default group to every fed group (Groups page), and the group's row
  says "fed from Default, priority N".
- **The default group is the reservoir.** An account of a fed group that runs out (its quota
  reached, or the provider refused it for a limit) goes to the default group to rest. An account
  in the default group with room, or whose limit resets within the look-ahead, is placed into a
  fed group before that group needs it.
- **Ready ahead.** A fed group keeps `ready` usable accounts (default 2: the one in use and the
  next). An account counts as ready when it has room now or resets within the look-ahead
  (default 30 minutes); the next account is placed as the current one passes its switching
  threshold, so automatic switching (DEC-0008) always has somewhere to go.
- **Priority.** Each fed group has a priority (1 is highest). When the reservoir cannot serve
  every fed group, higher priority is served first; when the last account with room is wanted
  by two groups, it goes to the higher one, and a lower group's last account may be taken for a
  higher group that has none.
- **Never moved:** the account the ordinary Claude Code or Codex is signed in to, an account
  given to an agent (`assigned_to`), an account the person turned off (W1.2, DEC-0027), a seat
  delegated to or from another machine, an account in a machine group, an account a running
  session's request is using at that moment (a move takes effect from the next request, as a
  switch does), and an account another organization's context could not use.
- **Undo.** Turning the switch off sends every account it moved back where it came from; each
  move is a line in the journal with its reason and figures, like balancing's.
- **Surfaces with parity:** the window (Groups page switch, priority, arrows; Activity journal),
  `switchboard project feed <group> on|off [--priority N]`, `switchboard feed status`, MCP
  `switchboard_feed` (status, set). Strings through Copylot (en, ru).

## How it is built (on balancing, DEC-0012)

Balancing already lends accounts between chosen groups on a 15-minute pass (a minute after a
group runs out), keeps `balancing.json` (groups, loans, cooldowns, backup groups) and
`balancing.jsonl`. Feeding is a second policy on the same pass and the same files:

- `Settings` gains `fed: [{pool, priority}]`, `ready` and `lookahead_minutes` (unknown fields are
  ignored by older readers, so the file stays readable).
- A move into a fed group is a `Lend` from `default`; an account of the fed group that runs out
  is a `Lend` from that group to `default` (parked). Turning feed off returns both kinds.
- The pass ranks fed groups by priority, computes each one's ready count from the snapshot's
  usage figures and limit evidence, and fills the shortfall from the reservoir by most room, then
  soonest reset; it parks exhausted accounts first, so they rejoin the reservoir.
- One account moves at most once per cooldown (balancing's `COOLDOWN_SECONDS`), except a park of
  an exhausted account, which is never delayed.
- Decision record: DEC-0028.

## Gap today

Nothing of it exists: balancing knows chosen and backup groups, not a reservoir, a priority or a
look-ahead. Row SB-419.
