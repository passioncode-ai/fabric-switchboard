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

0.4 desktop repair and brand (Status: proposed; implemented, operator outcome unobserved) — [interface](../../src/main.ts), [safe errors](../../src/adapter.ts), [design evidence](../evidence/design-0.4.md):
- `about.appearance` = “Appearance”; options “System”, “Dark”, “Light”; help “System follows the light or dark setting of your operating system. The choice is saved on this machine.”; save failure “This choice could not be saved on this machine. It applies until Switchboard restarts.”
- `about.license` = “Open source under the GNU AGPL-3.0; a commercial license is available — contact@passioncode.ai.” (since 2026-09-30, Fabric ADR-0092; 0.4.0-beta.1 shipped the PolyForm sentence) `about.third_party` = “The app includes third-party components under their own licenses.” `about.toolkit` = “Part of the PassionCode.ai toolkit.” Native note: “Addresses are selectable text. Copy one into your browser to open it.”
- `usage.not_monitored` = “Not checked automatically · Quota checks need OAuth” (non-OAuth) / “Not checked automatically while disabled”; `usage.reset_passed` = “Reset since check”, “… % used before reset”, “usage unknown since reset”, “A quota window has reset since this check. Not used for automatic rotation until a fresh check.”
- Backend vocabulary (B-04): shown verbatim — “Claude rollback lost its account lock. Check the current Claude sign-in before retrying.”, “Claude account lock is unavailable. Check permissions of the Claude config directory.”, “Finish an existing sign-in before starting another.”; mapped — ambiguous identity → “More than one account in this pool has this identity. Remove the duplicate, then retry.”; Terminal → “Terminal could not open. Check that Terminal is available, then retry.”; sign-in cleanup → “Sign-in cleanup needs Keychain access. Unlock Keychain and allow access, then cancel again.”; rotation clock → “The system clock is earlier than the last automatic switch. Check the date and time settings, then retry.”; “Label, pool or identity is invalid” shares the label/pool recovery text.
Humanization: on, own pass; each message names cause and next action, no decorative rewrite.
