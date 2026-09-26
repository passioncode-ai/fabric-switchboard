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
Coverage: [verification](../evidence/verification.md); browser inspected, historical 0.1 state; native 0.2 inspection is recorded in [0.2 evidence](../evidence/release-0.2.md).
Figma: not used
## SCR-02 — Activity
Status: designed
States: empty, populated, error.
Elements: timestamp, operation, account UUID, bounded detail, refresh.
Scenarios: SCN-011
Coverage: [verification](../evidence/verification.md); browser inspected, historical 0.1 state; native 0.2 inspection is recorded in [0.2 evidence](../evidence/release-0.2.md).
Figma: not used

## SCR-01 extension — current accounts and rotation (0.3)
Scenarios: SCN-018 through SCN-022. Current CLI identity panel, capture-first add dialog, explicit official sign-in alternative, Claude Swap import result, quota window disclosure, rotation policy editor/status, and native Claude activation confirmation. Workbench tokens and semantic controls retained. Integrated browser demo inspected with synthetic profiles; native/live 0.3 acceptance is separately declared in [release evidence](../evidence/release-0.3.md).
