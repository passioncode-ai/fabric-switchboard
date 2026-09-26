Contract: brand-contract v1
# Interface strings
Status: implemented, operator outcome unobserved
Canonical actions: Add account, Select, Launch isolated, Launch managed, Check usage, Edit, Remove, Cancel, Finish sign-in. They serve SCN-001..013.
Source: [interface](../../src/main.ts), [safe backend vocabulary](../../src/adapter.ts), [explicit demo messages](../../src/demo.ts).
Review decisions: selected means next request, launch requested is not authenticated success, usage unavailable is not zero, cancellation may require closing the official sign-in process first. Draft voice selected under autonomous design authorization; no operator voice calibration claimed.
