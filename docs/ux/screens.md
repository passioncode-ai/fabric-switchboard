Contract: ux-contract v4
# Screens
**Web surfaces:** no — local authenticated desktop workbench; browser demo contains only fixtures.
Design system: Workbench token layer; native semantic HTML controls in Tauri. Product-specific states are our decisions, not claimed as kit validation. No cinematic motion. Variance 2 (consistent rows), motion 1 (focus/hover), density 6 (five rows and actions visible at 1100×760).
Falsifier: user cannot tell which account handles the next request, or cannot recover from a failed addition.
## SCR-01 — Accounts
Status: designed
States: empty, loading, error, populated, add dialog, login dialog, removal confirmation.
Elements: provider filter, proxy status, add action, per-account quota age, selection and launch controls. Secret inputs only in addition.
Scenarios: SCN-001 through SCN-010, SCN-012, SCN-013.
Coverage: [verification](../evidence/verification.md); browser inspected, native window inspection blocked by locked Mac.
Figma: not used
## SCR-02 — Activity
Status: designed
States: empty, populated, error.
Elements: timestamp, operation, account UUID, bounded detail, refresh.
Scenarios: SCN-011
Coverage: [verification](../evidence/verification.md); browser inspected, native window inspection blocked by locked Mac.
Figma: not used
