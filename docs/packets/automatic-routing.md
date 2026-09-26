# Packet AR-01 — quota rotation

**State:** 0.3 implementation, subject to release verification. Shared contracts and packet boundaries are [PLAN-0.3](../PLAN-0.3.md); behavior and limitations are [account capture and rotation](../ACCOUNTS-AND-ROTATION.md).

This supersedes the original future-only packet under the operator’s explicit request to implement quota monitoring and account switching. Same-provider/pool membership, current identity, fresh quota, expiry, threshold, hysteresis, cooldown and deterministic eligibility live in the core policy. Runtime applies managed or native Claude selection without replay. Manual selection starts cooldown; disabling the policy is the persistent pause. A separate indefinite per-account pin/quarantine engine remains a follow-on.

The source-pinned comparator research is [Claude Swap delta](../RESEARCH-0.3-IMPORTS.md). Synthetic policy, monitor, import and native lock/rollback fixtures are the engineering gate; actual CLI adoption and provider requests still require [provider acceptance](provider-acceptance.md).
