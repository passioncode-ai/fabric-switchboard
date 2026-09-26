Contract: brand-contract v1
# Interface strings
Status: implemented, operator outcome unobserved
Canonical actions: Add account, Select, Launch isolated, Launch managed, Check usage, Edit, Remove, Cancel, Finish sign-in. They serve SCN-001..013.
Source: [interface](../../src/main.ts), [safe backend vocabulary](../../src/adapter.ts), [explicit demo messages](../../src/demo.ts).
Review decisions: selected means next request, launch requested is not authenticated success, usage unavailable is not zero, cancellation may require closing the official sign-in process first. Draft voice selected under autonomous design authorization; no operator voice calibration claimed.

CLI text uses the same verbs and failure distinctions (SCN-014..016). Native platform labels come from runtime metadata rather than hardcoded macOS. No Windows build is labelled provider-verified (SCN-017).

0.3 registry (Status: proposed; implemented, operator outcome unobserved): Capture current account, Import Claude Swap, Import profiles, Activate in Claude Code, Configure rotation, Save policy, Stop rotation — SCN-018..022; [interface](../../src/main.ts) and [safe errors](../../src/adapter.ts). Headroom is below the switching threshold; active CLI account is not managed-route selection. Humanization: own operational-language review; no decorative rewrites.

0.3.1 identity (Status: proposed; implemented, operator outcome unobserved): `product.name` = “Switchboard”, `product.family` = “by PassionCode” in [interface](../../src/main.ts), SCN-023. Existing action names and warning messages unchanged. Humanization: on — own pass; identity labels reviewed, no prose rewrite needed.

0.3.2 startup (Status: proposed): `startup.timeout` = “The native app did not respond. Close and reopen Switchboard, then retry.” — [read deadline](../../src/read-deadline.ts), SCN-024. Humanization: on, own pass; direct cause/recovery wording, no decorative rewrite.
