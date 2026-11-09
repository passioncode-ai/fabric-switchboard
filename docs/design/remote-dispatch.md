# Remote dispatch: a board request run on another machine (SB-317, stage A)

Status: steps 1 and 2 built 2026-11-09 (`remote_dispatch.rs`, `dispatch --target`, `dispatch status`, remote `dispatch stop`, the run's own time limit, the worktree removed once nothing is left unpushed); steps 3 and 4 open. Owner: fabric-switchboard. Consumer: Fabric CO-551 (dispatcher relay).
Board: thread `14d6cb73` (Fabric decision `32e834d4`, our acknowledgement `71c81ff5`), request
`11048978` (operator's words relayed by studio-agent: «давай агентов разработки может попробуем
уже перенести на студию чтобы они там ранились»). Deadline 2026-11-16.

## What stage A is

Fabric's board and dispatcher stay on the MacBook. `switchboard dispatch` runs a board request's
session on another machine (the Mac Studio's `sshlg` space) over ssh. That machine has no Fabric
and no `fabric-mcp`, so the session cannot answer on the board or accept the Observatory hand-over
itself: it writes a result file, Switchboard brings the file back over the same ssh, and Fabric's
dispatcher checks it and posts the answer as a relay (Fabric's four conditions, `32e834d4`).

## Decisions

- **D-1. The target is named.** `switchboard dispatch … --target <machine>` takes the machine as
  ssh knows it (the rules of `accounts send`: `account_send::target_valid`, no prompt), with the
  same `--remote` and `--remote-data-dir` as a move (`account_send::Program`). Without `--target`
  nothing changes: the session runs here as today.
- **D-2. Nobody accepts the hand-over.** Switchboard never accepts a hand-over on a session's
  behalf and never holds a lease token (`continuation.rs`, module doc), and the session there
  cannot reach this machine's Observatory. So the MacBook only reads the hand-over as today
  (`dispatch::handoff_waiting`), to check it is waiting for this workflow and to take its
  checkpoint body (the request) to the target; the offer is left as it is. Fabric, which opened
  it, withdraws it and closes the workflow after its relay (CO-551). To agree with Fabric.
- **D-3. The folder there is found by repository.** The request's folder here names a repository
  (its `origin`); the target's `switchboard --json project list` roots carry `remote`, and the root
  with the same identity (DEC-0089 normalisation) is the repository there. None: refused with
  reason `not_on_target`, nothing started. The session works in a fresh worktree from
  `origin/<default branch>` (studio-agent: the clones there lag behind), under
  `~/DATA/_worktrees/dispatch-<launch_id>`, which joins the repository's group there by itself
  (SB-117), so the target's own group account runs it.
- **D-4. What travels.** One directory on the target, `~/.fabric-dispatch/<launch_id>/` (0700):
  `request.json` (the hand-over checkpoint body as the MacBook read it: goal, constraints, plan,
  the board refs), `prompt.txt`, and, written by the run, `pid`, `log`, `exit` and `result.txt`.
  Files go over ssh stdin, never in a command line. The three `FABRIC_DISPATCH_*` values and the
  `launch_id` are in `prompt.txt` and in the session's environment.
- **D-5. The session runs headless and detached.** Over ssh: `nohup switchboard launch --provider
  <p> --working-directory <worktree> --headless --by fabric-dispatch -- -p
  "$(cat prompt.txt)"` in its own process group, stdout and stderr to `log`, its pid to `pid`,
  its exit status to `exit` when it ends. ssh returns at once; the MacBook records the launch
  (`launches::record`, `launched_by.surface = fabric`, `dispatch.target = <machine>`).
- **D-6. The result file.** The prompt tells the session to end by writing `result.txt`, whose
  first line is exactly `fabric-dispatch/1 result workflow=<wf> handoff=<id> request=<uuid>
  launch=<launch_id>`, then `outcome=answered|failed|blocked`, then `branch=<name>` and
  `commit=<sha>` when it pushed, then a blank line and the answer for the board (Markdown, up to
  64 KiB in all). Switchboard does not check its content: Fabric does (its four conditions).
- **D-7. Status and stop.** `switchboard --json dispatch status --launch-id <id>` answers
  `{launch_id, target, state: running|exited|timed_out|stopped|unknown, exit_code, result}`: for a
  remote run it reads `pid`, `exit` and `result.txt` over ssh (`result` is the file's text, or null
  until it exists). `dispatch stop --launch-id` signals the process group there (TERM, KILL after
  5 s). The background pass enforces `--timeout-minutes` the same way. An ssh that cannot reach
  the target answers `state: unknown` with the reason, never `exited`.
- **D-8. Limits.** At most two dispatched sessions per target at once (studio-agent, point 5):
  a third is refused with reason `target_busy`. The worktree is removed when the run's result has
  been read by `status` after it ended (its branch is pushed by then); a run that pushed nothing
  keeps its worktree for a person to look at, listed in `status`.
- **D-9. Choosing the machine (second step).** `--target auto --candidates <m1,m2>` asks each
  candidate's `switchboard --json usage` (and this machine's) for the group's lowest remaining
  quota and picks the one with the most; a candidate that does not answer is skipped. Built after
  D-1 to D-8 work end to end with an explicit target.

## Order of work

1. `dispatch --target` with D-2 to D-6 and a status that reads the result (D-7), with fake-ssh
   tests in the style of `account_send::tests`.
2. Stop, timeout and the two-session limit (D-7, D-8).
3. `--target auto` (D-9).
4. A live run on the Studio with a synthetic board request; Fabric CO-551 reads the same fields.
