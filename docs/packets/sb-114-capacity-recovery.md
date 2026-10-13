# SB-114 — remember stopped work until capacity returns

Operator request, 2026-10-13. Priority over the remaining SB-108 work. Related
tracks: RM-22 (activity), RM-28 (project/context boundaries).

## Scope and recovery

Base: `516ea45` (0.6.25). The operator's exported Claude Code conversation ends
with uncommitted SB-108 hand-over queue, runtime operations and CLI tests. Those
changes are retained and reviewed here. The export stays local: it is not a
public repository artifact. The remote integration branch was fetched and equals
the base. `pipeline.json` selects research/specification, implementation,
verification and Git handoff. Delivery includes the 0.6.26 producer release through
the existing exact-commit release guard; a pushed branch alone is not a release.

## Requirements and owners

| Requirement | Owner and check |
|---|---|
| REQ-1 Count account limits separately from registered stopped sessions; unknown host coverage is explicit | Switchboard recovery status, synthetic tests |
| REQ-2 A stop survives restart and the expiry of its account's limit | Private persistent recovery registry, reload test |
| REQ-3 Added or reset accounts become candidates only with fresh successful quota, the same provider/pool, and project-folder permission | Candidate filtering tests, including hidden holds and changed restrictions |
| REQ-4 One readiness event can be claimed once; recheck before claim; persist before returning an action; ambiguous launch is never automatically replayed | Claim/reload/concurrency tests |
| REQ-5 The session host registers exact session/account/folder/limit-event binding, notifies the session, resumes from its checkpoint and restores only unfinished children | Fabric/Observatory consumer integration; NOT_RUN until their owners connect |
| REQ-6 Finish SB-108 producer: bounded private records, one taker, expired offers never launched | Hand-over tests and CLI gate |

Switchboard owns account availability. Observatory owns workflow/checkpoint and
session activity facts. Fabric/the session host owns delivery, running-session
admission and parent/child resumption. A readiness record is data, not permission
to replay a prompt or a partially executed tool. A successful claim is not a
resumed session. Human-cancelled work must be cancelled in the registry by its host.

## Protocol

`switchboard recovery` returns account-limit evidence and registered waits,
including candidate account ids. Registration is explicit: the host supplies a
stable session id, account id, folder and stop event id. It is idempotent for the
same binding; the same event cannot be rebound. One live event per session.
Registration is a host claim of a stopped session, not independent observation.

Readiness uses a successful quota observation younger than five minutes, below
100 percent, no passed reset, no active hidden hold and no sign-in requirement.
The stopped account can become a candidate after its hold ends and a new quota
observation succeeds. Merely reaching the reset time is never readiness. A new
account in another pool/project cannot wake the session.

Claim takes an event and candidate, rechecks availability, and writes `claimed`
before returning. Repeated claims refuse, including after restart. The host
reports `resumed` or `cancelled`; a crash after claim remains `claimed` for manual
reconciliation, never an automatic duplicate. Records are bounded; a full registry
refuses new registrations rather than evicting unfinished work. Completed records
are retained within the bound for deduplication.

## Integration boundary

At intake `fabric_whoami` returned `no-project`. The operator enrolled and approved
Switchboard. On 2026-10-13 the registered Fabric MCP returned this project;
`com.status` returned `ready`, `unread: 0`, `responders: []`, and `com.list` returned
an empty page. The client initially advertised only `fabric_whoami`; a new connection
to the already-registered bundled MCP, after admission, exposed the board tools.
No alternate cross-project message or foreign-repository edit is authorized by
this packet. REQ-5 cannot be claimed complete from producer tests.

The producer owns no PTY or agent graph: it cannot safely infer that an old 429 means
a session is still stopped, or replay completed child work. The host integration must
register/cancel stops, poll readiness, claim with the selected account, deliver the
notification under its admission, restore unfinished children from the checkpoint,
and record `resumed` only after observing the resumed session. A launch with an unknown
outcome stays `claimed` until reconciled. Polling is local and must survive hidden windows.

Account evidence remains for the five-minute freshness window after its hold expires.
It is no longer reported as an active limit, but recovery refuses observations made
before that deadline, including after an owner restart. No history older than that
window is needed because an equally old quota observation cannot authorize recovery.

## Checks and next task

Run synthetic recovery/hand-over tests, CLI tests, then `./scripts/check.sh` with
lean Cargo settings (only 2.1 GiB free at intake). Keep credentials and app data
untouched. Publish the implementation and this packet together; the handoff records
actual results and the exact consumer integration task.

## Consumer handoff addresses

| Owner repository | Requirement | Observed state |
|---|---|---|
| `passioncode-ai/fabric-dashboards` | FD-62: consume `handovers` in a console PTY | [Handoff at 25f9480](https://github.com/passioncode-ai/fabric-dashboards/blob/25f9480568c4a0b51f5b043cc12d1a7fecd0a7d0/docs/HANDOFF.md) explicitly awaits Switchboard 0.6.26; no consumer acceptance claimed |
| `passioncode-ai/fabric` | REQ-5: host session/child admission, notification and actual resume | Project id unavailable to this board client; no request delivery claimed |
| `passioncode-ai/project-observatory-dashboard` | REQ-5: observed stop/checkpoint/outcome facts | Project id unavailable to this board client; no request delivery claimed |

Only `com.submit` to actual Project ids is the request channel. The adapter's
`references/project-board.md` says the directory is pending a contract decision;
repository names are not server-issued Project ids. The operator requested no more
questions during this run, so these asks remain prepared, not falsely sent.
A rejoin-only MCP attempt later returned `call budget exhausted before body admission`.
No new enrollment may be used to work around that refusal.

## Operator coordination update — 2026-10-13

The operator supplied a neighboring session's accepted hybrid plan in this run:
SB-115 owns a prompt host continuation attempt after a confirmed limit stop or route
change, followed by bounded retries with backoff. That session also owns the shared
Fabric and orchestration skill updates. Its implementation and publication have not
been verified here; do not duplicate its work from this packet.

SB-114's fresh-quota requirement governs its capacity-ready claim. It does not prohibit
an independently authorized host continuation attempt under the hybrid policy. A reset
time alone still does not establish healthy capacity. A quiet session alone does not
prove a stopped executor: the host must reconcile activity, checkpoint, cancellation,
and uncertain launch outcomes before acting. Resume only unfinished work and children;
never replay an ambiguous launch or completed task. Preserve provider, pool and project
boundaries for every attempt. REQ-5 remains open until host evidence exists.

## Delivery receipt — 0.6.27

[PR #182](https://github.com/passioncode-ai/fabric-switchboard/pull/182) merged as
[`61a2348`](https://github.com/passioncode-ai/fabric-switchboard/commit/61a2348ae2441af8d1d2403b955c0c574988fde0),
the immutable `v0.6.27` tag. Its tree equals the locally verified implementation
(`291c378`): full gate exit 0, 562 Rust tests passed, three opt-in native checks ignored.
A clean checkout of the remote tag passed documentation, plugin and release-preflight
checks without local dependencies or worktrees.

[Exact-tag nightly 38333315215](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38333315215)
finished **success** on macOS, Windows x64/arm64 and Linux x64/arm64. Both Windows
architectures passed the concurrent-taker test that rejected 0.6.26; the correction
uses permanent exclusive claim receipts. The old tag was not rewritten.

[Release 38333144066](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/38333144066)
passed preflight and the exact-commit guard. Attempt 1 built Linux x64/arm64,
notarized macOS, and signed Windows arm64. Windows x64 built successfully but its
signing action restored `ArtifactSigning-0.1.8`, skipped module installation and
failed to resolve `Invoke-ArtifactSigning` (job 115091847317). Only that derived cache
(id 8861870707) was removed; `gh run rerun 38333144066 --failed` started attempt 2.
Successful platform jobs were retained. The workflow now disables dependency caching
for future tags; the original 0.6.27 tag is unchanged. YAML parsing and the full local
gate passed for this workflow change.

Publication and installed-version readback are still pending. A `gh run watch` at
60-second cadence tracks attempt 2. The running app was observed at 0.6.20; its own
updater installed 0.6.25 into the bundle at 01:59 UTC, awaiting its normal restart.
No installed or running 0.6.27 claim yet.

The separate SB-115 owner must use a later version for its release. This receipt is
available in Git; it is not proof that the neighboring session read a notification.
