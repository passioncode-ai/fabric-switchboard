# Switchboard 0.4 — agents, project contexts, audit repair

Run started 2026-09-29 from `main` `71298f9`. Operator request (Russian, paraphrased): audit the
project against the PassionCode ecosystem (license, brand book, design, icon), let agents switch
account context and read remaining usage through an MCP server, optionally bind projects to
accounts, add a skill to the PassionCode skill system, fix bugs and UX, update the website, and
ship. Route: `task-pipeline` (this file is its brief, spec and plan).

## Operator decisions (grill, 2026-09-29)

| # | Question | Answer |
|---|---|---|
| D-1 | What may an agent change through MCP? | Managed route freely. Native Claude Code login (global, affects every ordinary `claude` session) only with an explicit `global: true` argument and a warning in the result. |
| D-2 | What is a project context? | An **optional** machine-local rule: project folder → account. The main job of Switchboard stays rotation, so limits continue across projects. Rules must be visible at all times and easy to switch off in time. |
| D-3 | Release depth | Full cycle: 0.4.0-beta.1 signed and notarized (`fabric-notary`), GitHub release, website, local install, plugin in the `@passioncode-ai/passioncode` launcher. |
| D-4 | Visual design | In code on the PassionCode design system, verified in a browser on demo data. No Figma file. |

Consequences of D-2: a rule has `enabled` and an optional `expires_at`; the Accounts screen shows
an always-visible strip when any rule is active; every MCP answer that touches accounts reports
active rules; a rule never disables rotation — rotation still moves off an exhausted account.

## Source ledger

| Source | Read | Finding |
|---|---|---|
| org-index `RULES.md` §9, `repositories.json` | yes | Switchboard is PolyForm NC OR Internal Use, source-available |
| `passioncode` `working-in-passioncode` skill + references | yes | license file-by-file, privacy gate, plugin/launcher release order |
| `LICENSE`, `README.md` at `71298f9` | yes | relicensing already landed (other session) |
| canonical design system `passioncode-ai.github.io/design-system` | yes (audit) | v1.1.0 `86866df1…` at `6085d10`; Switchboard pins v1.0.0 |
| website `origin/main` `2560662` | yes (audit) | 8 stale/wrong claims, updater hazard, 404 license URLs |
| code, `docs/`, `docs/ux/*`, evidence | yes (audit) | 20 confirmed defects (list below) |
| `CONTEXT.md`, ADRs, `docs/evidence/retro.md`, board | none found | board seeded as `docs/evidence/backlog.md` |
| code graph (`graphify-out/`) | none found | graphify cannot build on this machine (see machine CLAUDE.md); not a gate |
| wiki `projects/` | no Switchboard page | stage 9 creates it |

## REQ table (frozen; adding is free, removing needs the operator)

| REQ | Requirement | Verified by |
|---|---|---|
| REQ-1 | Project rules in core: add/update/remove, enabled, optional expiry, longest-prefix resolution, cascade on account removal, schema 3 only when rules exist | `cargo test -p switchboard-core --test projects` |
| REQ-2 | Runtime/control operations for rules and apply; CLI `switchboard project …` | runtime unit tests, `crates/switchboard-cli/tests/commands.rs` |
| REQ-3 | `switchboard mcp` stdio server (MCP 2025-06-18) with the tool contract below; `--read-only` | `crates/switchboard-cli/tests/mcp.rs` (spawned binary, JSON-RPC transcript) |
| REQ-4 | Session detection: managed Claude/Codex session knows its provider and pool; native otherwise | unit tests on env parsing |
| REQ-5 | Managed launches give the session the Switchboard tools (Claude `--mcp-config`, Codex `[mcp_servers]`) when the CLI is available; launch never fails because of it | launch unit tests |
| REQ-6 | Desktop: Projects screen (list, add, pause, remove, expiry), always-visible active-rules strip, Agents panel with exact connect commands | browser check on demo, screenshots in evidence |
| REQ-7 | Fix the 20 audit defects (table below) or park each with a ruling | per-row test or receipt |
| REQ-8 | Brand: tokens v1.1.0 re-pinned, focus/link roles, About shows version, license and toolkit link, bundle copyright/publisher/homepage | `scripts/check-brand.mjs`, browser check |
| REQ-9 | PassionCode plugin `switchboard` (skill + `.mcp.json`) in this repo, validated `--strict`, evals; member of the launcher family | `claude plugin validate … --strict`, launcher `npm test` |
| REQ-10 | Website: Switchboard page and brand facts true for 0.4.0-beta.1, license URLs fixed, updater no longer rewrites MIT history, agent/MCP section | site `npm run check`, live byte comparison |
| REQ-11 | Release 0.4.0-beta.1: versions in step, notarized macOS universal, Windows x64 cross-build, SHA256SUMS, GitHub release, local install | build receipts, `spctl`, anonymous download hashes |
| REQ-12 | Docs in the same change: SPEC, CONTRACTS, CLI, OPERATIONS, scenarios, brand docs, HANDOFF, release evidence, wiki page | `scripts/check_docs.py`, stage 10 ladder walk |

## MCP tool contract (`switchboard mcp`, server name `switchboard`)

Transport: stdio, newline-delimited JSON-RPC 2.0. Protocol versions accepted: `2025-06-18`
(preferred), `2025-03-26`, `2024-11-05`; the server answers with the client's version when it
supports it. Methods: `initialize`, `notifications/initialized`, `ping`, `tools/list`,
`tools/call`. Every tool result carries `structuredContent` and the same JSON as one text block;
a refused operation returns `isError: true` with a fixed message, never a stack or provider body.
No tool accepts or returns a credential. The server resolves the runtime the same way the CLI
does: the running owner's control channel, else an offline store operation under the exclusive
lock; login and launch are not exposed.

| Tool | Kind | Arguments | Result |
|---|---|---|---|
| `switchboard_status` | read | — | `{runtime:{online, platform}, session:{mode, provider, pool}, current:{claude,codex}, routes, rules:{active:[…]}, rotation:[policies+last decision]}` |
| `switchboard_accounts` | read | `provider?` | accounts without secrets: id, label, provider, kind, pool, enabled, selected, current, remaining summary |
| `switchboard_usage` | read (refresh is a provider call) | `account_id?`, `refresh?: bool` | per account: windows `{name, used_percent, remaining_percent, resets_at}` (RFC 3339), `observed_at`, `age_seconds`, `fresh`, `health`; refresh refused within 60 s of the last check |
| `switchboard_switch` | write | `account_id`, `target?: "session"\|"claude_cli"`, `global?: bool` | `session` selects the account for the next request in its provider/pool; `claude_cli` activates native Claude Code and requires `global: true` |
| `switchboard_project_context` | read | `path?` (default: server cwd) | matching rule per provider (`enabled`, `expired`, `expires_at`), whether it is in effect for this session |
| `switchboard_project_set` | write | `path?`, `account_id`, `target?`, `expires_in_hours?`, `enabled?` | the saved rule |
| `switchboard_project_remove` | write | `path?`, `provider` | removed rule |
| `switchboard_project_apply` | write | `path?`, `global?: bool` | what was done per provider: `selected`, `activated`, `already_in_effect`, `no_rule`, `rule_paused`, `rule_expired`, `other_pool` (session is in another pool: relaunch in the rule's pool) |

Annotations: read tools `readOnlyHint: true`; `switchboard_switch` and `…_apply`
`destructiveHint: false, idempotentHint: true`; `--read-only` lists read tools only.

Session detection (REQ-4), first match wins: `SWITCHBOARD_SESSION=managed:<provider>:<pool>` or
`isolated:<provider>:<account id>` (set by every launch); `ANTHROPIC_BASE_URL` of the form `http://127.0.0.1:<port>/claude/<pool>`
where the port equals the running proxy; `CODEX_HOME` equal to `<app data>/runtimes/codex-<pool>`;
otherwise `native`.

## Project rules (REQ-1) — data contract

`ProjectRule { path: String, provider: Provider, account_id: String, target: "managed"|"claude_cli", enabled: bool, created_at: i64, expires_at: Option<i64> }`.
`path` is an absolute canonical directory, ≤ 1024 bytes, no control characters; unique per
(`path`, `provider`); at most 256 rules. The account must exist with the same provider;
`claude_cli` requires a Claude OAuth account with an external identity. Resolution: the enabled,
unexpired rule whose path is the longest component-wise prefix of the query path. Removing an
account removes its rules in the same metadata write. `Snapshot.rules` is omitted when empty and
the file then keeps `schema_version: 2`, so 0.3 builds still open it; with rules the file is
`schema_version: 3`, which 0.3 refuses without changing it. Rule events: `project_rule` with
detail `saved`, `removed`, `applied`, `paused`.

## Audit defects (REQ-7)

| ID | Sev | Where | Defect | Ruling |
|---|---|---|---|---|
| B-01 | high | core `lib.rs` validate | clock set back > 60 s makes every write and startup fail | fix: future bounds on new input only |
| B-02 | med | `main.ts` add dialog | Import button after Begin sign-in orphans the login | fix |
| B-03 | med | runtime `finish_login` | cleanup failure keeps the login slot forever | fix: release slot, record pending cleanup |
| B-04 | med | `adapter.ts` | seven backend messages shown as a generic error | fix |
| B-05 | med | `main.ts` refresh | stale background snapshot reverts a fresh selection | fix: sequence reads |
| B-06 | med | runtime `CurrentAccounts` | waits behind the mutation lock, UI loses identity on timeout | fix: cached fast path |
| B-07 | med | proxy headers | header usage drops model windows | fix: merge windows |
| B-08 | med | CLI `rotation set` | disabling resets custom values | fix: optional flags merged |
| B-09 | med | runtime/monitor | activation and automatic switches leave no event | fix: `rotation`/`activation` events |
| B-10 | low | monitor | every failed switch reported as native activation | fix: `switch_failed` |
| B-11 | low | `main.ts` usage | “Next check” and Check usage shown for accounts never checked | fix |
| B-12 | low | `main.ts` usage | usage with a past reset shown as observed | fix |
| B-13 | low | runtime activate | cannot activate after `claude logout` | fix |
| B-14 | low | runtime `observe_current` | matches `default` pool only | fix |
| B-15 | low | `main.ts` render | background re-render drops keyboard focus | fix |
| B-16 | low | `main.ts` dialogs | duplicate dialog ids, lost focus return | fix |
| B-17 | low | proxy events | per-request events evict account history | fix: no per-request usage events |
| B-18 | low | CLI output | raw Unix seconds, no usage health | fix |
| B-19 | low | external lock | any lock error reported as “Claude is updating” | fix: distinguish errors; stale-lock takeover parked (needs Claude's own lock semantics) |
| B-20 | low | launch | journal failure after Terminal opened reports failed launch | fix |

## Plan — packets and ownership

Packets run in parallel worktrees and integrate by merge; each owns its files.

| Packet | Owns | Implements |
|---|---|---|
| P-A backend repair | core `lib.rs` validation, proxy, monitor, external, launch B-20, runtime B-03/06/09/10/13/14, CLI B-08/B-18 | REQ-7 (backend rows) |
| P-B desktop repair + brand | `src/*` except new Projects code, `brand/`, `scripts/check-brand.mjs`, `src-tauri/tauri.conf.json` | REQ-7 (UI rows), REQ-8 |
| P-C agents | core `projects.rs`, runtime rule ops, CLI `project` + `mcp`, launch MCP injection | REQ-1..5 |
| P-D desktop Projects/Agents | after P-B lands: `src/main.ts` Projects screen, strip, Agents panel | REQ-6 |
| P-E plugin + skill | `plugins/switchboard/`, `.claude-plugin/`, `test/evals/`, validator; launcher `family.json` | REQ-9 |
| P-F website | `passioncode-ai.github.io` from `origin/main` | REQ-10 |
| P-G release | versions, builds, notarization, GitHub, install, receipts | REQ-11, REQ-12 |

Carry-over ledger: stale-lock takeover (B-19 part), native Windows acceptance, live provider
acceptance (PA-01), Windows signing — stay open with board ids in `docs/evidence/backlog.md`.
