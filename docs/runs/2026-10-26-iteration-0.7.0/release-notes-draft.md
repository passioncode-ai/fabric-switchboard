# Release 0.7.0: notes draft

Draft for the release pull request of iteration 0.7.0 ([canvas](canvas.md)). The published notes are
the `## 0.7.0` section of [CHANGELOG.md](../../../CHANGELOG.md): the release pull request renames
`## Unreleased` to `## 0.7.0 (<date>)` (target 2026-11-14); its first paragraph and the eight entries
after it are the release's headline, written to read unchanged under that heading. The summary below
is for a person, in plain words; it can open the GitHub release or a post.

## What you get in 0.7.0

**Switchboard now knows when you work for an organization.**

- **Your work and the company's stay apart.** In a company's folders, `claude` and `codex` typed in
  a terminal use the company's own setup: its accounts, its history, its settings. Step out of the
  folder and your own come back. Switchboard sets this up by itself once your computer has a
  company group, and one command turns it off: `switchboard shell off`. It works in zsh, bash, fish
  and, on Windows, in PowerShell.
- **One checklist to get a new computer ready.** Connect PassionCode.ai, get the company's group,
  sign in to your seat, check your terminals, start a first session. The window shows the next step
  on a card, your coding agent reads the same list, and a guide walks you through it in English and
  Russian.
- **Joining and leaving are clear.** A new seat is offered its company's group. When a company
  removes your seat, its card says so and tells you whom to ask, and Switchboard stops using it.
  When you leave a company on PassionCode.ai, nothing is deleted: you decide what to remove.
- **Paid agents stay inside a budget you set.** Give this computer, a group or one task a spending
  ceiling. Before a paid agent starts, Switchboard checks what its key has spent; past the ceiling it
  does not start the agent and asks you to double the ceiling or keep it. Only you, in the window,
  can raise one.
- **See your fallback chains.** The Agents page shows which agents take a task over when its
  accounts run out, in what order, and whether each could take it now.

Not yet tried on a person's Windows computer: the Windows parts pass the nightly tests on Windows
runners only.

## For the release pull request

- Rename `## Unreleased` to `## 0.7.0 (<date>)`; nothing else in the section changes. No line of it
  may start with `Urgent:` unless the release is meant to be urgent
  ([updater_artifacts.py](../../../scripts/updater_artifacts.py) `URGENT_LINE`).
- Copylot `copylot_check` (register editorial, voice "PassionCode.ai site voice v2", en-US): the
  CHANGELOG's first paragraph and eight new entries `2358e52a-629d-45cb-b41f-6353206d415b` (pass, 95.2,
  three advisory rule-of-three notes); this summary `a903757b-f870-49c6-aa1d-0f3b1832dda7` (pass,
  100, no findings).
- Live checks still owed: the Windows host (SB-02), the organization's first real setup (node B-10,
  SB-125), the operator's acceptance ([checklist](../../reports/2026-10-26-operator-acceptance/README.md)).
