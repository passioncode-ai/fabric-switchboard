# Packet COM-11 (Switchboard part) — provider, account and session provenance for Fabric

**State:** researched 2026-10-05; implementation waits for Fabric's COM-01 contract and COM-03
identity/replacement/fence schema. **Canonical status:** Fabric
[execution spine](https://github.com/passioncode-ai/fabric/blob/3b2878fc9283db5fc9a81697ba8538a01630b8d9/docs/evidence/plans/2026-10-04-project-communications.md)
(COM-11, COM-12). **Owner issue:** [#36](https://github.com/passioncode-ai/fabric-switchboard/issues/36).
**Board:** SB-47. This packet is the owner's research and proposal, not a second copy of COM status.

## What Fabric asks of Switchboard

Supply provider/account/session provenance to Fabric's admitted project responder and its
explicit replacement flow. Fabric owns generation fences, delivery and the board; process
discovery and account selection grant no messaging authority; Switchboard stays a standalone
account manager.

## Why implementation waits

COM-11 depends on COM-03 (consumer identity, replacement, fences) and COM-07, both open on the
spine; COM-01 owns the message and claim schemas. The field names, the generation a provenance
record is fenced by, and the transport (MCP read, service descriptor, file) are not defined yet.
Building a shape now would be a guess Fabric then has to adopt or undo. Switchboard does not
consume `fabric-agent-contract` today (its `capabilities` are MCP's), so the CO-193 capability-name
schema needs no pin adoption here.

## What Switchboard can already supply (read-only, no credential)

| Provenance | Where it comes from today | Freshness and limits |
|---|---|---|
| Which account answers a managed session's next request | `switchboard_status.routes["<provider>:<pool>"]` (account id, label); `switchboard_switch` changes it | as of the call; a response already streaming keeps its account (SPEC §6) |
| Which mode a session runs in | `switchboard_status.session`: `managed {provider, pool}`, `isolated {provider, account_id}`, `native {provider}` — from the session's environment, never from process discovery | describes the calling session only |
| Which account the ordinary CLIs are signed in to | `switchboard_status.current_cli_accounts` (identity metadata, no token) | read on demand; may be null when unreadable |
| Remaining capacity and its age | `switchboard_usage` (windows with `scope`, `observed_at`, `age_seconds`, `fresh`, health) | cached; unknown is not zero (SB-35…40) |
| Provider limit holds | `switchboard_status.limited[]` (`kind`, `resets_at` vs `until`, `confidence`, `scope: unknown`, `event_id`, `session`) | typed evidence, persisted across restarts (SB-41) |
| Provider wait before the next quota check | `usage_health.next_check_at`; refusal *Usage checks are rate limited…* | SB-39 |
| Switches made | journal events `activation completed`, `rotation switched`, `switched_on_limit` with account ids and times | Switchboard's own actions only |

## How the acceptance criteria map

| Acceptance (issue #36) | Switchboard today |
|---|---|
| Cached quota age/error remains visible | holds: `age_seconds`, `fresh`, health and `next_check_at` are reported, never hidden |
| A stale PID/session never becomes current responder | Switchboard tracks no responder and selects none; it reports routes and accounts. A responder fence must be Fabric's generation, which Switchboard does not issue |
| Account/provider change cannot forge admission or override Fabric's generation | no Switchboard operation writes Fabric state; a switch changes only the account, and its provenance record must carry Fabric's generation unchanged (proposal below) |
| Standalone utility works without Fabric | holds: nothing in Switchboard requires Fabric |
| Credentials private | holds: no tool returns a credential (CONTRACTS; MCP tests) |

## Proposal for COM-01/COM-03 (for Fabric to accept, amend or refuse)

A read-only `switchboard_provenance` MCP tool (and `switchboard provenance --json`) returning, for
one provider and pool: the account id and identity revision (a hash of the external identity,
never a token) that serves the next managed request; the time and cause of the last change
(`manual`, `rotation`, `limit`, `rule`); the typed limit evidence of the account it replaced; and
an opaque `fabric_generation` field echoed exactly as Fabric passed it to the session (environment
or argument), so a record cannot outlive or forge Fabric's fence. Versioned with the contract's
capability-name schema once the shape is fixed. Tests: stale generation echoed unchanged, no
credential, standalone without Fabric, account replaced while a stream runs.

## Next step

When COM-03 publishes the identity/replacement schema, implement the agreed shape on a bounded
branch with fixture pins, update CONTRACTS and the plugin skill, and send the source receipt to
COM-12. Until then the row stays open with this packet as its resume point.
