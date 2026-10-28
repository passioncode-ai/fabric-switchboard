# Iteration 0.7.0, wave 5: backlog changes proposed for the lead

`docs/evidence/backlog.md` and `docs/HANDOFF.md` are guarded registers ([AGENT_SYNC](../../AGENT_SYNC.md)):
the lead edits them under a lease. The docs nodes A-9, B-9 and C-9 write here what they propose; the lead
copies it in. Row numbers and texts are read from `origin/main` at `47cb173` (the backlog's home; the
iteration branch's copy stops at SB-211). New ids are left as `new-1`, `new-2`, … for the lead to reserve
(the last id on `main` is SB-233).

## A-9: context binding

### Existing rows

| Row | Change | New state text |
|---|---|---|
| SB-183 | **Title** (the word "opt-in" and the shell list are out of date: D-3 made the hook on by default, and PowerShell joined) | SB-110 phase 2: the terminal hook (`switchboard shell init zsh\|bash\|fish\|pwsh`, written into the start files by the installed app on a computer that holds a group bound to an organization, off by `switchboard shell off`) gives plain `claude` and `codex` in an organization's group folders that group's shell home (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) and `SWITCHBOARD_CONTEXT`, and restores the person's own on leaving; scenario first |
| SB-183 | **State** | built on `iteration/0.7.0`: A-1 map and `shell env`, A-5 the person's own folders (`b0ae360`), A-2 scripts, A-3 start files, A-4 shell homes (`fba2dbd`), A-7 window and MCP (`a884ec9`), A-8 PowerShell (`bef6cb2`), W2 Windows rules and SB-225/SB-226 (`c2d3e3e`); contracts indexed in CONTRACTS "Context binding in 0.7.0" (A-9); to close at the 0.7.0 release. Left: the Windows host's live check (SB-02). The proxy guard tests of node A-6 (row `new-1`) are built on `w5/a6-cq6` |
| SB-201 | **State** | built on `iteration/0.7.0` (A-10, A-11/B-2, merged `388cde9`): Kimi Code, Hermes, OpenCode, OpenRouter and proxy agent launches carry `context` and `SWITCHBOARD_CONTEXT`; tests `an_openrouter_launch_records_its_session_and_context`, `a_proxy_agent_launch_records_its_context`, core `a_session_without_an_account_takes_its_keys_group_else_personal`; to close at the 0.7.0 release |

### New rows

| id | P | Title | Source | State |
|---|---|---|---|---|
| new-1 | P3 | Proxy guard tests for the shell homes (node A-6): a placeholder bearer (`switchboard-shell`) with a folder's capability header reaches that folder's pool, and an `_auto` request from a folder the proxy does not map carries no sign-in upstream. The client side is measured ([A-0](../../evidence/shell-home-measurement-2026-10.md)) | dossier A row A-6 | done on `w5/a6-cq6` (2026-10-28): `a_placeholder_bearer_with_a_folder_header_reaches_its_pool`, `an_auto_request_from_an_unmapped_folder_carries_no_sign_in_upstream` in `crates/switchboard-proxy/src/tests.rs`, each shown to fail with its defect planted; CONTRACTS "Context binding in 0.7.0". Reserve an id only if the lead wants the record as a closed row |
| new-2 | P3 | Flaky under load: `shell::scripts::tests::a_missing_old_or_failing_cli_gives_the_held_values_back` failed once in zsh at load average 66 (passed 3 of 3 alone); likely the zsh hook's wait for the CLI under load; a sibling of SB-07 | canvas, wave 2 state | open |
| new-3 | P3 | `detect_session` and the shell homes: a plain session started in a hooked terminal runs with `CLAUDE_CONFIG_DIR` inside `<data>/shell/`; check what `detect_session` makes of it (a launched home or the person's own) and say it in CONTRACTS | canvas, wave 2 state | open |
| new-4 | P3 | A-Q8: the native sign-in context (`external::context`) still reads `CLAUDE_CONFIG_DIR` and `CODEX_HOME` as the process has them and refuses a Switchboard home; route its reads through `person::` and keep the refusal for writes | [questions](questions.md) A-Q8 (deferred) | open: after 0.7.0 |

Board message for the lead (not a backlog row): Vesta's rollout note "plain `claude` or `codex` is not bound
to a context" is Vesta's to update now that the hook ships on by default (dossier A section 5, dossier B
section 7, B-9).

## B-9: the organization's person

### Existing rows

| Row | Change | New state text |
|---|---|---|
| SB-202 | **State** | built on `iteration/0.7.0`: the organization checklist in `switchboard onboard` and MCP `switchboard_onboarding` (B-3, merged `a6701ad`), the window's organization card and the empty Accounts page leading with the seat (B-8, `e466ad7`), the runbook [ORGANIZATION.md](../../ORGANIZATION.md) and [ORGANIZATION.ru.md](../../ORGANIZATION.ru.md) with each step's proof command and a Windows column (B-9; Copylot en-US `82a68bc7-2279-489d-9289-3357ceb77231`, ru-RU `faec18a8-11a7-4cae-adbe-683cd6cc7dcc`), linked from README *Quick start* and INSTALL.md; to close at the 0.7.0 release. The Windows column stays "not yet tried on a person's Windows computer" until SB-02 |
| SB-160 | **State** | Switchboard's part built on `iteration/0.7.0`: joining, a new seat's receipt offers its organization's group (B-4, `a6701ad`); leaving, an ended seat read from the provider's refusal (B-5, `f7d145f`), named with its organization on the card and the group (B-6), PassionCode.ai leaving recorded with its date (B-7, both `bdf4c04`); SCN-077 to SCN-079; to close at the 0.7.0 release. The playbook's run stays Vesta's and Fabric's |
| SB-125 | **State** (add) | 2026-10-28: the runbook it walks exists (B-9). Node B-10 runs after 0.7.0 is installed on both machines; human gates: the browser consent and Serge Khurs's own sign-in |

## C-9: chains and ceilings

### Existing rows

| Row | Change | New state text |
|---|---|---|
| SB-71 | **State: done** at the 0.7.0 release | **done** in 0.7.0: core, CLI and MCP released in `v0.6.8` (state as before); the window's *Fallback chains* panel on Agents and the Groups card line (C-3, merged `de077ff`), readiness and the last hand-over in `Operation::Chains`, `switchboard chain list` and `switchboard_chain_get` (C-2, merged `1cd483b`), a group's chain found through the workflow's checkout (C-8, `1cd483b`); scenarios SCN-080 to SCN-082; CONTRACTS → Fallback chains. Follow-ups SB-229, SB-232 |
| SB-72 | **Split** | **done** for its shipped part in 0.7.0: this computer's daily ceiling (every saved OpenRouter key added up per UTC day), a group's daily ceiling on its own key, a task ceiling counted since the task's first paid hand-over (C-Q2, labelled), the key's own limit read at every paid start with a $0.50 floor, the double-or-keep question answered in the window only (C-Q1, C-Q7, C-Q9), the gate on every paid start (C-Q8) with one budget (SB-214) and the key slot it read (SB-213): C-4 and C-2 (`1cd483b`), C-5 and C-6 (`dc734fd`), C-7 (`cc2c3e5`), S2 (`25d58d6`), S5 (`76d1a21`); CONTRACTS → Spending ceilings; SCN-083, SCN-084. **Moved out:** ceilings enforced at the provider through a key Switchboard issues per scope from the Observatory door (XA-01's first enforcement; the door lives only on the operator's estate, dossier C section 4) and grant-backed project ceilings read from the project's policy go to **SB-96** (grant check) and **SB-97** (metering through the proxy, which also replaces the task measure). Follow-ups SB-216, SB-217, SB-230, SB-233 |
| SB-73 | **State** | the status part is **done** in 0.7.0: readiness of every chain step and the last hand-over with the steps it passed over, in the window (C-3), `switchboard chain list` and MCP `switchboard_chain_get` (C-2); the spend gate before each paid step (C-5). **Left (phase 2):** steps pinned to project paid keys (SB-96), a Hermes executor's own limit, Kimi Code on a paid key, live acceptance on a capped key |
| SB-96 | **State** (add) | 2026-10-28: 0.7.0 ships Switchboard's own ceilings (SB-72's shipped part); keyed pins still skip as `agent_not_automatic_yet` until this row's grant check exists; the grant-backed per-project ceiling moved here from SB-72 |
| SB-97 | **State** (add) | 2026-10-28: when metering lands it replaces 0.7.0's task measure (the key's `usage` since the first paid hand-over, which over-counts when agents share a key); provider-side keys per scope moved here from SB-72 |

### New rows

| id | P | Title | Source | State |
|---|---|---|---|---|
| new-5 | P3 | Analytics for chains and ceilings (decision C-Q6): `app_active` gains the counts `chains` and `ceilings` (counts only), with a group id and a workflow id planted in `events_carry_counts_and_the_installation_id_never_an_identity` | [questions](questions.md) C-Q6; [ANALYTICS](../../ANALYTICS.md#chains-and-ceilings-070) | done on `w5/a6-cq6` (2026-10-28): `chains`, `ceilings` and `ceiling_questions` (open questions) in `app_active`, the last two left out when `spend.json` cannot be read; `chains_and_ceilings_are_counted_and_an_unreadable_file_is_never_zero` and the planted-identifier test (group id, workflow id, question id, account, key name, amount), each shown to fail with a defect planted. Reserve an id only if the lead wants the record as a closed row |
| new-6 | P2 | A group's managed sessions share one route, so they all stop on one account at once: six managed sessions of the PassionCode group stopped together (about 05:33 UTC 2026-10-26) while one account still held quota. Spread a group's sessions over its accounts | fabric-workspace report, recorded in the [canvas](canvas.md) wave 2 state | open |
| new-7 | P3 | SB-204's second source: sessions started before the plugin's 0.6.83 update have no `Stop` hook, so their stops wait for the one-day horizon (7 stops seen 2026-10-26); settle a stop also on the session's first successful request through the proxy | [canvas](canvas.md) wave 2 state | open |
| new-8 | P3 | The agent-settings unreadable sentence says "restore it from a backup", but a restore never writes over an existing setting: name the order (delete it first), as spend.json's sentence does (`spend::UNREADABLE`) | w2/context round 1, [canvas](canvas.md) | open |
| new-9 | P3 | One session per agent home, and the `openrouter` pool sentinel in launch records: launches on the OpenRouter key write `pool: openrouter` in their record (`crates/switchboard-runtime/src/launch.rs:1879`, `:2022`, `:2245`); check whether a group id `openrouter` is refused and reserve it if not, and say what a second launch of the same agent home does | w2/context round 1, [canvas](canvas.md) | open |
| new-10 | P3 | About's usage text (`USAGE_COUNTED`, `src/main.ts:110`) names installs, days of use and saved accounts but not the configuration counts `app_active` carries (`pools`, `rotation_on`, `project_rules`, `projects`, and since C-Q6 `chains`, `ceilings`, `ceiling_questions`); decide whether the disclosure lists them or links ANALYTICS.md, then the string in every locale through Copylot | `crates/switchboard-runtime/src/analytics.rs` `counts`, `active_counts`; [ANALYTICS](../../ANALYTICS.md#what-is-sent) | open |
