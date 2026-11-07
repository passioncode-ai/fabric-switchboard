---
report: fabric-switchboard plan against the North Star 2026-11-07
project: fabric-switchboard
date: 2026-11-07
source: operator in chat 2026-11-07 («давай составим план что осталось сделать на базе долгов но смержи с виженом из north star документа в fabric workspace»)
---

# What is left in Switchboard, read against the North Star

The open board ([backlog](../../evidence/backlog.md), main at `128f80a`) merged with the target picture of the
organization: fabric-workspace `knowledge/north-star.md` at `52006d00` and its level-1 page `knowledge/system/README.md`.
Section numbers (§) and gap ids (G) are the North Star's. Release state: v0.7.22 published 2026-11-07 01:35:20Z and
running on the operator's Mac since 02:36:54Z.

## 1. Where the North Star puts Switchboard

| North Star | What it asks of Switchboard |
| --- | --- |
| §4, the administration table | coding-agent accounts and seats per group and organization; fallback chains; ceilings |
| §3, joining and leaving | the person's Claude seat is one line of the onboarding checklist, reversed at leaving with a test that the door is closed |
| §1 principle 2, §3 levels | owners, administrators and project architects command without a question; a user's request goes through the product's approval |
| §6, the task end to end | «Switchboard starts the project's default agent headless on an account with quota»; «when an account runs out, Switchboard continues the work on the next account or agent in the chain» |
| §2 and §10, places and rebuild | coding seats per space on the always-on machine survive a reboot without a person |
| §11, one host and several accounts | check who listens before sending a token; ports recorded per account (DEC-0154) |
| §8, the model map | the strongest model for coding agents; the map's roles for agents on OpenRouter |
| §0 and G1, the measure | the activity journal (OBS-378) joins agent work and spend; Switchboard holds launches, hand-overs and relayed usage |
| system README, F4 Harness | Fabric runs any agent on any runner «through Switchboard»; Linux and cloud forms (G28) |

Gaps that name Switchboard as an owner: **G2** (SB-291), **G4** (SB-292), **G14** (services at boot, thread `c7371b0c`,
SB-287).

## 2. The map: each North Star item, its rows and the next step

| North Star item | Rows | State on 2026-11-07 | Next step |
| --- | --- | --- | --- |
| Seats per group and organization (§4) | SB-282, SB-284, SB-285 | many to many released in 0.7.21, *where an account works* in 0.7.22; SB-285 open | SB-285: the e-mail domain only suggests the organization, a person confirms |
| Joining and leaving: the seat (§3, G3, G4) | SB-292, SB-283, SB-242 | SB-292 filed; SB-283 first part on branch `sb-283-handover`, parked by the operator; read-back proof named for Vesta's leave checklist (board `c3a23d20`) | one design note with SB-291, by 2026-11-13; code after the contract's architect role |
| Levels (§1.2, §3, G2) | SB-291 | the five gates named with file:line | code after CT-250 and VES-AUTHORITY-LEVELS; the caller's level from Vesta's `org-membership/1` attestation, never from a request field |
| Waking the owner (§6, F3) | SB-247, SB-294 | dispatch built on one machine; SB-294 agreed with Fabric (option 1, `dispatch_agent`) | SB-294 by 2026-11-19; SB-247's rest (console hand-over, Windows engine lookup) by 2026-11-14 |
| Continuing on the next account or agent (§6) | SB-82, SB-123, SB-72, SB-96, SB-97, SB-213 to SB-217 | chains and ceilings released in 0.7.0; runner routes designed (SB-82); metering waits for Observatory's grant registry | SB-82 against the contract's runner routes; SB-96 and SB-97 when the registry exists (G11 decides where it lives) |
| Seats that survive a reboot (§2, §10, G14) | SB-287 | headless spaces have no writable keychain; Observatory 0.35.0 ships the system-daemon route (OBS-361) | design note: a file-backed store chosen by a `serve` flag, refused in a GUI session, plus the daemon definition for Observatory's validator; then build |
| One host, several accounts (§11) | SB-278 | DEC-0154 released (`v0.1.0+2026-11-07.1`); the contract's notes in the row | build when CT-228 publishes the claim schema and the port picker vectors and CT-254 is tagged; HA-2 for our clients on the open connection, no cached listener list |
| Model map (§8) | SB-289 | machine key's default on `moonshotai/kimi-k3.1-pro` since 2026-11-06, spend read 2026-11-07: $0 new, $0.11 total | recommended models in the map's order, mimo kept for people's content, the reasoning traps handled |
| The measure (§0, G1) | **SB-295** (new), SB-203, SB-212 | launches and statistics recorded locally; nothing reaches the journal | give the activity event (fabric-workspace `knowledge/system/measure.md`) its run and `cost_usd`: the launch or dispatch behind a unit and its relayed usage, `null` for a subscription seat; after the schema (G1-a) |
| Cross-machine (§6, G5) | SB-177, SB-168 | identity digests wait for the organization hub | build with Fabric's 2026-11-21 slice |
| Organization boundary and context (§11, G12) | SB-280, SB-124, SB-184 | wait for OBS-359, a per-context profile owner, CT-251 | follow the owners' rows |
| Linux and cloud (G28) | SB-287 (store), SB-88 (built) | Linux builds ship; a headless Linux box has no Secret Service either | the same file-backed store serves a headless Linux space |
| Reliability of the owner (§10) | SB-163 | T-1 to T-3 done; 2026-11-06/07: `usage_renewal` checks held the transaction 10 to 44 s, snapshots waited up to 37 s, one MCP call timed out | T-5: read a renewal's credential before the transaction, as T-2 did for the grant |
| A seat moved to another machine is never renewed here | SB-293 | fixed and released in 0.7.22 | none |

## 3. Waves

The estate is paused for the common reconciliation (board thread `eefa7959`). Design notes are the deliverable each
project owes by 2026-11-13; code starts when the pause lifts.

| Wave | When | Work | Exit check |
| --- | --- | --- | --- |
| **W1, designs** | by 2026-11-13 | one note for SB-291, SB-292 and SB-283 (levels, the seat at joining and leaving, the hand-over); a note for SB-287 (headless store and boot route); SB-163 T-5 | notes in `docs/research/`, linked from their rows and announced on the board |
| **W2, before the organization slice** | by 2026-11-19 (slice 2026-11-21) | SB-294; SB-278 (if CT-228 and CT-254 are tagged); SB-287 build; SB-289; SB-163 T-5; SB-247's rest | release 0.7.23 green on seven platforms, installed here; Fabric runs a dispatch on a machine with no default agent |
| **W3, with the hub and the roles** | after 2026-11-21 | SB-291 and SB-292 code; SB-283 merged; SB-177; SB-295; SB-280; SB-82; SB-96 and SB-97 if the grant registry exists | a member's seat granted and taken back by the administrator agent with a test; levels read from Vesta's attestation |
| **W4, acceptance and platforms** | needs a person or a host | SB-01 and SB-15 (live acceptance), SB-178 (two machines), SB-02, SB-243, SB-224 to SB-226 (Windows) | the operator's acceptance record; a Windows host run |
| **Tail** | between waves | about thirty P3 rows left from 0.7.0 (window leftovers, hook timing, races) | closed in batches, each with its test |

## 4. What the North Star asks and the board did not have

- **G1, the measure:** no row sent Switchboard's records to the activity journal. Filed as SB-295 in this change.
- **G23, templates:** the host admin template signs coding seats per space through Switchboard. Its route is SB-287
  (headless store) and SB-292 (the seat command); no separate row until the host template names its call.
- **G19, agents finishing their own acts:** Switchboard's own gates are SB-291's five; the machine's permission rule is
  the operator's.

## 5. Not Switchboard's

Priorities and the release queue (G10, G25: Fabric), the security agent (G27: Observatory with the adapter), the
assessment (G8, G9), pricing records (G18, G24).

## 6. Updates

- 2026-11-07 afternoon: the estate's plan (fabric-workspace `docs/plans/2026-11-07-remaining-to-north-star/README.md`,
  merged `32111728`) takes this plan as Switchboard's part and found that the common backlog read 113 of this board's
  296 rows: blank lines inside the table (old lines 121 and 181) ended it. Both removed in the same change as this
  note. SB-295 now names the activity event of `knowledge/system/measure.md`, of which Switchboard supplies the run and
  the cost.
