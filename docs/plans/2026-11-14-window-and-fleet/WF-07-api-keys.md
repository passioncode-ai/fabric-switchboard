# WF-7 API keys page (SB-369, wave 2)

> API ключи дальше, типа нужно поработать над интерфейсом, сделать все такое более компактное, чтобы
> было понятно, по группам разделить, по блокам, чтобы не было все раскидано.

## Now

`renderTokens` (`src/main.ts:1299`): ceiling banners, a note, `openrouterPanel` (1465: machine key,
model, credit, Change, Replace, Remove, machine ceiling), `projectKeysPanel` (1314: every group with
own key or machine key, Add, Replace, Remove, its ceiling line), `taskCeilingsPanel` (1422).

## Sub-modules and tasks

**7.1 Blocks.** Three folding sections (WF-1.2): *This computer's key* (key, model, credit, daily
ceiling on one card), *Group keys* (only groups with their own key open; the rest one line "N groups
use this computer's key" with *Give a group its own key…*), *Task ceilings* (only when one exists).

**7.2 Compact rows.** One row per group key: group, key tail, model, spent today of its ceiling as a
bar, one ⋯ menu for Replace, Remove, ceiling. No row repeats the machine key's facts.

## MCP

Unchanged tools (`switchboard_openrouter_status`, `switchboard_ceilings`); the parity test (WF-1.3)
covers the actions.

## Checks

Demo and live screenshots; SCN-045, SCN-065, SCN-083 updated; Copylot.

## Depends on

WF-1.2.
