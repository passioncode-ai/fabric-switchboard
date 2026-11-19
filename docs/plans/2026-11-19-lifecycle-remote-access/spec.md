---
spec: 2026-11-19-lifecycle-remote-access
source: ../../reports/2026-11-19-lifecycle-remote-access-audit/README.md
status: accepted (the operator's order of 2026-11-19 is the decision; the architect advises and never gates)
---

# Spec: lifecycle, remote machines, delegation and access, release and update

Each requirement names the findings it closes and the check that proves it. A requirement is met when its check passes in the gate (`./scripts/check.sh` on the Mac Studio) or, for a release requirement, on the release workflow.

## S1 Continuation (root R1)

- **S1.1** Every path that continues a session (Continue in the window, MCP or CLI; boot resume; the duty officer's restart; a restart wave; a recovery claim; the automatic fallback) builds the resume from one function that reads the launch record it continues. Closes M1-04..07, M1-21, M1-22, M1-45, M5-02. Check: one test per path asserting the same builder is used and its output.
- **S1.2** Before any resume the account the session ran on is checked through the store: disabled, used elsewhere, given to an agent, turned off (DEC-0027 once #821 lands), at its limit with a waiting stop, or guest seat ended: the resume is refused with that reason, or (for a limit) continued on the group's next account through the managed route. A Switchboard home gets a freshly renewed credential. Closes M1-05, M1-45, M5-02.
- **S1.3** The resume carries the permission mode the launch ran with (stored per launch), never the global default. Closes M1-06.
- **S1.4** The resume carries the launch's own environment (stored per launch); `OBSERVATORY_*` join `CONFLICTS`. Closes M1-07.
- **S1.5** A resumed session in a Switchboard home writes `.session-pid`; `ensure_idle` also checks the live-session table. Closes M1-04.
- **S1.6** A session running in a live host (any viewer or none) is restarted in place through its host, never stopped and reopened in Terminal; Dashboards consoles are recognised by their host, not by process ancestry. Closes M1-03.
- **S1.7** A command of a session counts when its shell snapshot lives in that session's own config folder. Closes M1-01.

## S2 Identity and unknown state (roots R2, R3)

- **S2.1** A machine is identified by its `ssh -G` identity; its key pin, its program path and its aliases hang off that identity. One resolver serves every remote call. Closes M2-05, M2-24, M3-05 (alias part).
- **S2.2** The machine proof is typed: `pinned`, `matched`, `unproven`, `key_changed`, `refused`. Acts that send a credential require `pinned` or `matched`; reads may run `unproven`; `key_changed` is propagated unchanged to the caller and to Needs attention (`machine_key_changed`). Closes M2-02, M2-10, M3-05.
- **S2.3** An account on another machine is matched by its recorded remote id; `remote-status` sends ids. Closes M2-09.
- **S2.4** A read that could not be made (process table, stops, a remote) is an unknown that skips the act and says so; it is never an empty set. Closes M1-15, M1-24.

## S3 Delegation and access (root R4)

- **S3.1** Value-returning operations (surrender, agent key values, Anthropic key values) are served only through a restricted door: a recall signed by the recalling machine's pinned key, or a one-time ticket in the environment of a process Switchboard launched; each is journalled. The CLI's env-name-plus-pipe guard is removed. Closes M3-01, M3-10.
- **S3.2** A managed launch gets a capability bound to its pool and launch; the global session token never lands in a home or an environment; the agent token is per pool. Closes M3-02.
- **S3.3** A recall reads the returned sign-in's provider profile and compares its account id with the stored identity before keeping it; a mismatch or a missing id rolls back. Closes M2-01.
- **S3.4** A guest seat carries `surrendered_at`; `stays` refuses enabling it; pending removals are retried by the sender; undo restores the previous enabled state; the moved-away guard reads `delegated_out` too; `force` needs a decision. Closes M3-03, M3-04.
- **S3.5** Operator-only loosenings take a decision bound to a verifiable artefact (an answered Fabric question or a code shown in the window) with freshness; a first ceiling above a threshold counts as a loosening; decisions are checked and journalled on the operation, not in one surface. Closes M3-06, M3-07.
- **S3.6** `ssh-serve` refuses a forced pool that is not an existing non-default, non-tokens group, cuts session rows to sessions on the caller's seats, logs each call's kind and result, caps seats per sender. Closes M3-09, M3-17, M3-18, M2-21.

## S4 Lifecycle robustness (root R7 and the rest of M1)

- **S4.1** Claude board runs are extended on their transcript writes, not only their log. Closes M1-02.
- **S4.2** A limit stop is its own field, cleared on the next turn; `ended` keeps the real end; it is booked to the account of the launch it belongs to (the launch record, else `CLAUDE_CONFIG_DIR`). Closes M1-08, M1-20.
- **S4.3** Pacing, deadlines and freshness use a monotonic or sleep-aware clock; a future-dated stamp is stale beyond a bound. Closes M1-09, M1-10, M1-43.
- **S4.4** Dispatch: remote runs count as live for their workflow; a remote stop checks the run's exit and start before signalling; gates run before the owner transaction; a run whose record or watch entry cannot be written is killed and reported. Closes M1-11..14, M2-15.
- **S4.5** Session hosts: a slow start never leaves two copies; a host whose descriptor cannot be written does not start the session; a dropped viewer's socket is shut both ways; headless restarts go through a detached host. Closes M1-16..19.
- **S4.6** The rest of M1's P3 rows (M1-25..44) are fixed or written as backlog rows with evidence.

## S5 Release and update (root R5)

- **S5.1** The release guard accepts a nightly only when its test jobs passed (never skipped); nightly concurrency is per ref; the guard's deadline covers the nightly's jobs; Linux package installs retry; release jobs do not re-run the suites nightly proved, or retry flaky ones; a failed release job on infrastructure is retried once by a watcher; publish runs in one global concurrency group and re-checks latest; a last job proves the feed serves the tag; preflight refuses a tag not newer than latest; release and nightly run on the same pinned images. Closes M4-01..09, M4-27.
- **S5.2** One update policy for the app and `serve` (pause, urgency including sticky urgency, 503 drain, `needs_migration` read one way). Closes M4-10..13.
- **S5.3** A health check `update` and an attention item `update_behind` for this machine and for every machine read over ssh (its version against the feed). Closes M4-14, M4-17, M4-19, M4-23 (lease failing).
- **S5.4** Linux: a signed CLI updater entry for tarball installs and an unattended path for `.deb` (signed APT repository or an opt-in timer); AppImage `serve` stamps `$APPIMAGE`. Closes M4-15, M4-16, M4-18.
- **S5.5** `switchboard remote update --on <machine>` and a fleet versions list. Closes M4-26.
- **S5.6** The plugin channel follows releases and updates with the app. Closes M4-22.

## S6 Remote acts (root R6)

- **S6.1** From this machine on another, and from another on this, through one proved path and in every surface (CLI, MCP, window): launch, list, attach, continue, restart, pause, read output, send a line, end, close a copy. `--target` launches are hosted by default and recorded here. Closes M2-03, M2-04, M2-18; delivers SB-523, SB-524.
- **S6.2** Moves and recalls record every outcome (left there, unknown, undo failed) with the remote id, and Needs attention shows the ones that need a person. Closes M2-12..14.
- **S6.3** The remote program is detected (`command -v`, `/usr/bin`, `~/.local/bin`, then the macOS path); Windows targets are refused with a sentence. Closes M2-06.
- **S6.4** The Sessions page and `targets` know every machine group and launch target. Closes M2-07, M2-08.

## S7 Board and docs (root R8)

- **S7.1** Every backlog status matches what shipped (M5's table); the plan's State table, DECISIONS' next-free line and plan rows without a backlog row are corrected. Closes M5 statuses.
- **S7.2** The open PRs land or close: #821 with M5-01 and M5-03..06 (its M5-02 is met by S1.2), #829 renumbered (M5-07) with M5-08, #832 with M5-09..12 and M3-13; M5-13 annotations.
- **S7.3** Docs named in M2-23, M3-19, M4-25, M1-36, M1-42 match the code; M3-14, M3-15 and M3-16 are fixed or kept as rows with evidence.
