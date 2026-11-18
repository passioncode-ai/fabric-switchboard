# 07. Saved launches (operator's order, 2026-11-18)

## The order

«надо может как-то такие команды дать запускать чтоы просто вызвать терминал с ней из менеджера
сессий вне окон фабрика и тд», after asking for a command to rerun the Sound Engine audit on the
pay-per-token group whenever he wants.

## The end form

- **A saved launch** is a named recipe: the account or the group whose route it takes, the
  folder, the agent (Claude Code, Codex, a catalog agent), the model, the first message (kept as
  text in the recipe or as a file path in the folder, so a long brief is not cut at 1024
  characters), how it runs, and an optional step before it.
- **How it runs:** in a Terminal window of its own (the person sees it and types into it), or
  hosted (`--in-place --hosted`: it outlives the window, and any app or machine attaches with
  `switchboard attach`), or headless with its output in a log under the data folder.
- **The step before** (optional): bring the folder to a fresh branch from the remote's default
  branch (`git fetch` then `git switch -C <branch> origin/<default>`), refused when the folder
  has uncommitted changes; never a reset, a stash or a delete.
- **One run at a time by default:** running a saved launch whose last run is still live offers
  that run (attach) instead of a second copy; "allow parallel runs" is a switch of the recipe.
- **Money is visible before the run:** a recipe on a pay-per-token group shows the group's
  ceiling and today's spend next to its Run button, and the run is refused while the group's
  spend is not metered (SB-452) or its ceiling is reached.
- **Where it is run from, all with parity:**
  - the window's Sessions page: a "Saved launches" list with Run, Edit, Duplicate, Delete, and
    the last run's state and link;
  - the menu-bar (Windows: notification-area) icon: "Run saved launch" with the recipes, so a
    run needs no window, no Fabric console and no terminal typed into;
  - `switchboard run <name>` and `switchboard presets list|add|edit|remove|show`;
  - MCP `switchboard_presets` (list, run; add and edit by the operator's words), so Fabric
    Dashboards and other agents run the same recipe the same way;
  - a double-clickable `.command` file per recipe on request (`switchboard presets export
    <name>`), for Spotlight and the Dock.
- **Stored** in `<data>/launch-presets.json` (0600): names, ids, folders, agent, model, the first
  message or its file path, mode and switches; never a credential, never a key value; carried
  in backups and in `settings export` (folders matched by repository like groups).
- **Every run is a launch** like any other: a launch record, the Sessions row, history,
  `continued_as`, capacity and the board share all apply unchanged.

## Today's stand-in

Until this ships, `~/Applications/Switchboard/Sound Engine audit.command` (on the operator's
Mac, outside Git) runs the audit recipe: fresh `audit/next` from `origin/main` in
`~/DATA/_worktrees/sound-engine-audit`, then `switchboard launch <appvillis key account>
--mode managed --in-place --hosted -- --model claude-fable-5-2 "<brief>"`.

## Row

SB-454 in [the backlog](../../evidence/backlog.md).
