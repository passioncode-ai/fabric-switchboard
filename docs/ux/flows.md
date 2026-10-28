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

## FLW-01 note (0.6.82)
A failed read never replaces a list already on screen (SCN-043): the list stays, a line above it offers *Retry*. Remove of a selected account goes through *Disable and remove* in the same dialog (SCN-010), not back to the list. Restore from About asks first (SCN-030).

## FLW-04: Set up an organization's work on a computer (0.7.0)
Traces: SB-202, SB-160, SCN-076, SCN-077, SCN-074, SCN-050, SCN-073, SCN-035
Entry points: the window after the usage question and the tour; `switchboard onboard` (the person's coding agent); the runbook `docs/ORGANIZATION.md`.
Success exit: `switchboard onboard --status --json` answers `"complete": true` and the card is gone.
```mermaid
flowchart TD
 A[Usage question SCN-073] --> B[Tour SCN-035]
 B --> C{Organization on this computer?}
 C -->|no| Z[Today's first run: empty Accounts SCN-001, five onboard steps]
 C -->|no, connect| P[Organizations: Connect PassionCode.ai SCN-071]
 P -->|consent denied or cancelled| P_err[Notice, nothing changes]
 P_err --> P
 P -->|organizations read| D
 C -->|yes| D[Screen: Accounts with the organization card]
 D --> E{One group bound to it?}
 E -->|none| F[+ Group for organization: group dialog prefilled]
 F -->|refused: foreign accounts| F_err[Dialog stays open with SCN-070's reason]
 F_err --> F
 F -->|saved| G
 E -->|two or more| E_err[Step names both groups: keep one]
 E_err --> E
 E -->|one| G{Signed-in seat of it in the group?}
 G -->|no| H[Sign in with your organization address]
 H -->|sign-in failed or cancelled| H_err[SCN-003 recovery, step stays Needs you]
 H_err --> H
 H -->|signed in elsewhere| M[Notice offers Move it to group SCN-077]
 M --> G
 H -->|signed in in the group| G
 G -->|yes| I[Terminals step reads the hook SCN-074, optional]
 I --> J[First launch, optional]
 J --> K[Card gone, onboard complete]
 D -->|Hide| L[Card only on Organizations]
 L --> E
 D -->|read failed| D_err[Card keeps last steps, Refresh to retry]
 D_err --> D
```

## FLW-05: A hand-over at the switch, with the spend gate (0.7.0)
Traces: SB-73, SB-72, SB-71, SCN-040, SCN-082, SCN-083, SCN-084
Entry points: none (the background monitor finds a limited executor with an open workflow).
Success exit: a step is offered and its session opens; or every step is skipped and the reasons are on SCR-03.
```mermaid
flowchart TD
 A[Executor limited, workflow open] --> B{Chain by checkout folder: group's, else this computer's}
 B -->|none applies| N[Nothing happens]
 B --> C[Next step]
 C --> D{Usable account or key?}
 D -->|no| S[Skip with reason] --> C
 D -->|yes| E{Paid step?}
 E -->|no: subscription or Kimi account| O[Offer and launch]
 E -->|yes| F[Read the key's spend]
 F -->|less than 0.50 left on the key| S
 F -->|a ceiling covers it and the read failed| S
 F -->|a ceiling is reached| Q[Open the question SCN-084, notify] --> S
 F -->|under every ceiling, or no ceiling| O
 O -->|launch failed after the offer| X[Stop, offer lapses]
 O -->|launched| R[Record last hand-over, notify]
 C -->|no step left| T[Record last hand-over: every step skipped]
 Q --> W[Screen: banner on Agents and API keys]
 W -->|Double, window only| B2[Ceiling doubled, retry mark cleared]
 W -->|Keep| K2[Question closed, value kept]
```
Screens traversed: SCR-03 (populated, last hand-over, ceiling question banner); API keys (ceiling question banner, no SCR id yet).
