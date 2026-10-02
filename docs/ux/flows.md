Contract: ux-contract v4
# Flows
## FLW-01 — Account workbench
Traces: ST-001, JTBD-01
```mermaid
flowchart LR
 A[SCR-01 empty/accounts] --> B[Add or login]
 B --> C{Valid and stored?}
 C -->|yes| A
 C -->|no| E[Error and retry]
 A --> D[Select provider/pool account]
 D --> F{Launch mode}
 F -->|isolated| G[New private client]
 F -->|managed| H[Proxy session]
 H --> I[Select next-request account]
 I --> H
 A --> J[SCR-02 activity]
 A --> K[Confirm removal]
 K -->|cancel| A
 K -->|unselected| A
 K -->|selected| E
 E --> A
```

## FLW-02 — Capture and rotate (0.3)
Traces: SCN-018..022; [implementation contract](../ACCOUNTS-AND-ROTATION.md).
Current CLI observation → capture existing / official login for another / import Claude Swap → identity upsert → current CLI badge and separate managed selection → quota windows/health → opt-in policy → eligibility + cooldown → managed route or locked native Claude activation → observe current again. No eligible account → hold with reason. Stop policy → persistent disabled state.

0.3.2 recovery edge (FLW-01, SCN-024): launch → account metadata/runtime → usable workbench; current CLI observation updates independently. Native read deadline → error → Retry. Add account → Import Claude Swap → existing import confirmation/result.

## FLW-03 — One-click accounts and prompt-free switching (0.5)
Traces: SCN-003, SCN-018…SCN-020, SCN-022, SCN-028, SCN-029; [PLAN-0.5](../PLAN-0.5.md).
```mermaid
flowchart LR
 A[In use now] -->|unsaved identity| B[Add to Switchboard]
 B --> L[Account list]
 M[+ Add account menu] -->|Sign in| T[Terminal + banner]
 T -->|LoginStatus complete| L
 T -->|ended| R[Try again / Dismiss]
 M -->|Import from Claude Swap| L
 L -->|Switch| C[Claude Code uses account]
 L -->|Sign in again| T
 P[Turn on for Claude Code] --> Q[Monitor: renew inactive, check quota]
 Q -->|threshold| C
```
