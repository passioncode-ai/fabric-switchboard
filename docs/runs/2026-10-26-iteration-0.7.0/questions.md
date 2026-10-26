# Iteration 0.7.0: questions

Each row: who owns the answer, what was tried, the recommendation, and what the run does meanwhile. Internal and
reversible answers proceed on the recommendation, marked **assumed** (iteration rule); the rest went to
fabric-workspace on the board (operator decision `a509fcc0` of 2026-10-25: questions for the operator go through it).

| id | question | sources tried | recommendation | state |
|---|---|---|---|---|
| A-Q1 | Shell homes per group instead of the managed home H-3 names | design H-3, dossier A §4 | own home per group (`<data>/shell/<provider>-<pool>`); revisit after A-0 | **assumed** |
| A-Q2 | Plain `codex` in an organization group with no Codex account | SB-143, dossier A | stays on the person's sign-in in 0.7.0, said in `shell status` and SCN-074 | **assumed** |
| A-Q3 | A symlinked rc file | dossier A | skip, state `skipped_symlink`, show `switchboard shell init <shell>` | **assumed** |
| A-Q4 | PowerShell execution policy on the Appvillis pilot's Windows machines | dossier A; no source in reach | never change it; `policy_blocked` says what IT can do | **asked** (Appvillis IT, via fabric-workspace) |
| A-Q5 | The SB-140 connectors notice in an organization's shell home | SB-140 row | accept: an organization's home has no claude.ai login to lose | **assumed** |
| A-Q6 | Kimi in an organization folder | SB-109, dossier A | record `personal`, launch, board row for attribution | **assumed** |
| A-Q7 | Windows shell homes without the person's carried links | CONTRACTS.md:68 | accept for 0.7.0, board row after SB-02 | **assumed** |
| A-Q8 | The native sign-in context (`external::context`) still reads `CLAUDE_CONFIG_DIR` and `CODEX_HOME` as the process has them, and refuses a Switchboard home for captures, activation and renewal; should its reads go through `person::` (the person's own folder) instead | A-5 fix cycle 1 (blind verifiers), CONTRACTS.md "The person's own Claude Code and Codex folders" | route `external::context` reads through `person::` and keep the refusal for writes | **assumed** |
| B-Q1 | Where Serge Khurs's work runs, Mac or Windows | call notes В-06, decision D-6 | his own machine with his own PassionCode.ai seat | **asked** (operator, via fabric-workspace) |
| B-Q2 | Is the fifth account of group `passioncode` (not on passioncode.ai) a PassionCode.ai Team seat | `switchboard --json project list` | yes: place it by the person's word; no: move it out before binding | **asked** (operator, via fabric-workspace) |
| B-Q3 | Move work off an ended seat automatically | AGENTS.md (SB-98 is the only replay) | no in 0.7.0 | **assumed** |
| B-Q4 | Runbook languages | Copylot route, global-25 | English and Russian | **assumed** |
| B-Q5 | Fabric Dashboards' setup prompt and the new `onboard` step ids | dossier B | tell Dashboards on the board when B-3 lands | **to send** with B-3 |
| B-Q6 | A Windows host | integration Q-A | the runbook's Windows column stays "not yet verified" until SB-02 | **operator queue Q-A** |
| C-Q1 | Where *double* may be answered | authority.md, dossier C | the window only | **assumed** |
| C-Q2 | Task ceiling measure | dossier C | key usage since the first paid hand-over, labelled, checked at a switch | **assumed** |
| C-Q3 | API-key Claude or Codex steps under a ceiling | dossier C | gated: `spend_unknown` | **assumed** |
| C-Q4 | Move the key's own provider ceiling when doubling | dossier C | no in 0.7.0 | **assumed** |
| C-Q5 | Which door issued the keys in use, daily or monthly | `switchboard_openrouter_status` 2026-10-26 | **answered**: one machine key, limit $10, `limit_reset: daily`, so the provider-enforced daily ceiling holds | **closed** |
| C-Q6 | Analytics for the new surfaces | ANALYTICS.md, AGENTS.md privacy rule | counts only in `app_active` (`chains`, `ceilings`) | **assumed** |
