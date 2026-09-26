# Packet AR-01 — recommendations then opt-in rotation

**State:** planned; manual selection is the only v0.1 policy. Depends on PA-01 and SS-01 observations.
**Owns:** independent policy module, richer Usage windows, policy settings and route audit reasons.
**Shared:** [contracts](../CONTRACTS.md), [spec §§6,9](../SPEC.md).

First return recommendations without switching. Inputs: provider/pool, explicit candidate membership, auth expiry, window utilization/reset/source/freshness, recent rejection and quarantine, user pin. Unknown/stale/malformed readings exclude automatic candidates; they never count as free capacity. Preserve each source window instead of a single aggregate.

Then add per-pool opt-in with configurable threshold, hysteresis, cooldown and minimum remaining capacity. Deterministic tie-break; no cross-pool or cross-provider movement; no candidate holds the route and shows an explanation. A manual pin suspends automatic movement until explicitly released. Policy logs contain structured reason codes, not provider messages.

Switch at the next request boundary only. Existing streams are immutable. Do not warm up accounts or spend tokens merely to reset a window. No retries initially; a future proven pre-execution rejection path needs its own contract and one-attempt bound. Request timeout after transmission remains ambiguous and cannot auto-replay.

Gate: simulated clocks and traces for threshold oscillation, stale observation, restart, simultaneous manual/auto selection, disabled candidate, token expiry, all accounts exhausted, malformed usage, and active stream. Native UI clearly shows policy status, next choice and reason; user can stop it in one action. Spend/correctness claims require provider-specific evidence.
