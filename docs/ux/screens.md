Contract: ux-contract v4
# Screens
**Web surfaces:** no — local authenticated desktop workbench; browser demo contains only fixtures.
Design system: PassionCode v1.0.0, vendored from the [immutable canonical source](../../brand/passioncode/manifest.json); native semantic HTML controls in Tauri. Product-specific states are our decisions, not claimed as kit validation. No cinematic motion. Variance 2 (consistent rows), motion 1 (focus/hover), density 6 (compact account rows and current identity visible before account actions; retained workbench hierarchy).
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
Scenarios: SCN-018 through SCN-022. Current CLI identity panel, capture-first add dialog, explicit official sign-in alternative, Claude Swap import result, quota window disclosure, rotation policy editor/status, and native Claude activation confirmation. Existing semantic controls retained; PassionCode tokens applied in 0.3.1. Integrated browser demo inspected with synthetic profiles; native/live 0.3 acceptance is separately declared in [release evidence](../evidence/release-0.3.md).

## Shared chrome and identity (0.3.1)
SCN-023 covers the fixed dark register, yellow S icon, Switchboard / by PassionCode sidebar, and separate information-blue current-CLI versus gold next-request status. Component structure and account behavior remain unchanged. Native icons are generated from the same canonical SVG via `node scripts/generate-icons.mjs`. Native controls remain the existing component layer; no kit migration or animation added. Browser rendering evidence and limits: [0.3.1 design evidence](../evidence/design-0.3.1.md).

## Startup and import recovery (0.3.2)
SCN-024: primary metadata loads independently of external credential observation; bounded native reads expose Retry. SCN-019: Import Claude Swap remains on Accounts and is also directly reachable from Add account. No visual theme changes.

## SCR-01 revision — compact grouped accounts (0.5)
Scenarios: SCN-003, SCN-018…SCN-020, SCN-022, SCN-028, SCN-029. Accounts are grouped by provider (Claude Code, Codex CLI) with pool sub-groups when a provider has more than one; each account is one ~48 px row: provider mark, label with badges (In Claude Code, Next managed request, Sign in again, Disabled), email or credential type, a quota cell (highest window %, reset or staleness, 3 px meter; click expands windows), one primary action (Switch / ✓ In use for Claude OAuth, Select / ✓ Selected otherwise, Sign in, Enable) and a ⋯ menu for the rest. The provider filter, the per-card action rows, the capture/import/activation dialogs and the Finish sign-in step are gone; + Add account is a menu, In use now offers Add to Switchboard, and a sign-in banner replaces the login dialog. Density 8. Inspected in the browser demo at 1280×720 and 740×560, light and dark: [design 0.5 evidence](../evidence/design-0.5.md).

### Quota order and countdown — 2026-10-04
SCN-021, SCN-032. Within provider/pool groups, fresh remaining quota precedes known waits, unknown data, sign-in and disabled rows. The quota cell is two lines (2026-10-05): meter, share used and check age; then the countdown and date of the next reset or end of wait, or the failed/stale/sign-in text. A coloured status mark on the provider icon carries the card state; the screen-reader label states it in words. Expanded windows show individual waits. Retry holds have their own label. Existing token/component system; no animation. Pause/resume stops automatic timer text updates; ticks only update text, without changing focus. [Verification](../evidence/quota-order-2026-10-04.md).
