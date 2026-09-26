Contract: brand-contract v1
# Interface strings
Status: implemented, operator outcome unobserved
Canonical actions: Add account, Select, Launch isolated, Launch managed, Check usage, Edit, Remove, Cancel, Finish sign-in. They serve SCN-001..013.
Source: [interface](../../src/main.ts), [safe backend vocabulary](../../src/adapter.ts), [explicit demo messages](../../src/demo.ts).
Review decisions: selected means next request, launch requested is not authenticated success, usage unavailable is not zero, cancellation may require closing the official sign-in process first. Draft voice selected under autonomous design authorization; no operator voice calibration claimed.

CLI text uses the same verbs and failure distinctions (SCN-014..016). Native platform labels come from runtime metadata rather than hardcoded macOS. No Windows build is labelled provider-verified (SCN-017).
