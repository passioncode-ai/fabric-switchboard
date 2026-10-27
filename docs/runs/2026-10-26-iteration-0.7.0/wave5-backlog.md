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
| SB-183 | **State** | built on `iteration/0.7.0`: A-1 map and `shell env`, A-5 the person's own folders (`b0ae360`), A-2 scripts, A-3 start files, A-4 shell homes (`fba2dbd`), A-7 window and MCP (`a884ec9`), A-8 PowerShell (`bef6cb2`), W2 Windows rules and SB-225/SB-226 (`c2d3e3e`); contracts indexed in CONTRACTS "Context binding in 0.7.0" (A-9); to close at the 0.7.0 release. Left: the Windows host's live check (SB-02), the proxy guard tests of node A-6 (row `new-1`) |
| SB-201 | **State** | built on `iteration/0.7.0` (A-10, A-11/B-2, merged `388cde9`): Kimi Code, Hermes, OpenCode, OpenRouter and proxy agent launches carry `context` and `SWITCHBOARD_CONTEXT`; tests `an_openrouter_launch_records_its_session_and_context`, `a_proxy_agent_launch_records_its_context`, core `a_session_without_an_account_takes_its_keys_group_else_personal`; to close at the 0.7.0 release |

### New rows

| id | P | Title | Source | State |
|---|---|---|---|---|
| new-1 | P3 | Proxy guard tests for the shell homes (node A-6, not built in 0.7.0): a placeholder bearer (`switchboard-shell`) with a folder's capability header reaches that folder's pool, and an `_auto` request from a folder the proxy does not map carries no sign-in upstream. Today the client side is measured ([A-0](../../evidence/shell-home-measurement-2026-10.md)) and the proxy side rests on `a_pool_capability_reaches_its_pool_only` | dossier A row A-6; `grep -n "placeholder" crates/switchboard-proxy/src/tests.rs` prints nothing | open |
| new-2 | P3 | Flaky under load: `shell::scripts::tests::a_missing_old_or_failing_cli_gives_the_held_values_back` failed once in zsh at load average 66 (passed 3 of 3 alone); likely the zsh hook's wait for the CLI under load; a sibling of SB-07 | canvas, wave 2 state | open |
| new-3 | P3 | `detect_session` and the shell homes: a plain session started in a hooked terminal runs with `CLAUDE_CONFIG_DIR` inside `<data>/shell/`; check what `detect_session` makes of it (a launched home or the person's own) and say it in CONTRACTS | canvas, wave 2 state | open |
| new-4 | P3 | A-Q8: the native sign-in context (`external::context`) still reads `CLAUDE_CONFIG_DIR` and `CODEX_HOME` as the process has them and refuses a Switchboard home; route its reads through `person::` and keep the refusal for writes | [questions](questions.md) A-Q8 (deferred) | open: after 0.7.0 |

Board message for the lead (not a backlog row): Vesta's rollout note "plain `claude` or `codex` is not bound
to a context" is Vesta's to update now that the hook ships on by default (dossier A section 5, dossier B
section 7, B-9).
