# Shell home measurement (A-0, iteration 0.7.0, SB-183)

Measured 2026-10-26 on the operator's MacBook with `scripts/measure_shell_home.sh` (fresh temporary homes and
folders, `env -i`, a local stub as the provider on 127.0.0.1; no real home, Keychain item or sign-in read; the
stub records header names and whether values are the expected placeholder and capability, never a value).

```text
claude 2.1.299 (Claude Code); codex codex-cli 0.163.0
claude/placeholder: requests=2 first={"path": "/v1/messages", "authorization": "placeholder", "x-api-key": "absent", "capability": "expected"} output="API Error: 400 a0 stub "
claude/no-placeholder: requests=0 first= output="Not logged in · Please run /login "
codex/auth-required: requests=1 first={"path": "/codex/_auto/v1/responses", "authorization": "absent", "x-api-key": "absent", "capability": "expected"} output="ERROR: {"type": "error", "error": {"type": "invalid_request_error", "message": "a0 stub"}} ERROR: {"type": "error", "error": {"type": "invalid_request_error", ""
codex/no-auth: requests=1 first={"path": "/codex/_auto/v1/responses", "authorization": "absent", "x-api-key": "absent", "capability": "expected"} output="ERROR: {"type": "error", "error": {"type": "invalid_request_error", "message": "a0 stub"}} ERROR: {"type": "error", "error": {"type": "invalid_request_error", ""
```

**What it settles for dossier A.**
- **Claude Code 2.1.299.** A shell home holding only a placeholder `ANTHROPIC_AUTH_TOKEN` in its `settings.json`,
  in a folder whose `.claude/settings.local.json` carries Switchboard's route (`ANTHROPIC_BASE_URL`) and
  capability header (`ANTHROPIC_CUSTOM_HEADERS`, SB-118), sends `Authorization: Bearer <placeholder>` and
  `X-Switchboard-Capability` to the route, with no `x-api-key`. The home and the folder combine as the design
  assumed (A-4). Without the placeholder the client stops before any request: "Not logged in".
- **Codex 0.163.0.** A `CODEX_HOME` with the `switchboard-auto` provider block and no `auth.json` sends its
  request to `/codex/_auto/v1/responses` with the capability header and no `Authorization`, whether
  `requires_openai_auth` is true or false. The shell home needs no Codex sign-in (A-4).
- **Not measured here.** The SB-140 connectors notice appears only in an interactive session, not in `-p`;
  it is read when A-7's scenario is walked (question A-Q5, assumed: accept). The cold timing of
  `switchboard shell env` waits for A-1.

## Below the bound folder (fix cycle 1 of A-2/A-3/A-4, 2026-10-26)

The same script, with two cases added (`claude_case subfolder yes src no` and
`claude_case subfolder-git yes src yes`): the route stays in `<folder>/.claude/settings.local.json` and
`claude -p` starts in `<folder>/src`, the folder a plain directory or a fresh `git init` repository.
Run on the operator's MacBook, the same day and versions:

```text
claude 2.1.299 (Claude Code); codex codex-cli 0.163.0
claude/placeholder: requests=2 first={"path": "/v1/messages", "authorization": "placeholder", "x-api-key": "absent", "capability": "expected"} output="API Error: 400 a0 stub "
claude/no-placeholder: requests=0 first= output="Not logged in · Please run /login "
claude/subfolder: requests=0 first= output="Failed to authenticate. API Error: 401 Invalid bearer token "
claude/subfolder-git: requests=0 first= output="Failed to authenticate. API Error: 401 Invalid bearer token "
codex/auth-required: requests=1 first={"path": "/codex/_auto/v1/responses", "authorization": "absent", "x-api-key": "absent", "capability": "expected"} output="ERROR: {"type": "error", "error": {"type": "invalid_request_error", "message": "a0 stub"}} ERROR: {"type": "error", "error": {"type": "invalid_request_error", ""
codex/no-auth: requests=1 first={"path": "/codex/_auto/v1/responses", "authorization": "absent", "x-api-key": "absent", "capability": "expected"} output="ERROR: {"type": "error", "error": {"type": "invalid_request_error", "message": "a0 stub"}} ERROR: {"type": "error", "error": {"type": "invalid_request_error", ""
```

**What it settles.** Claude Code 2.1.299 does not read `.claude/settings.local.json` from a parent folder,
git repository or not: started in a subfolder it sent nothing to the route (`requests=0`) and its request
with the shell home's placeholder went to its default provider, which refused it (401). So `switchboard
shell env` answers the Claude home only in a row's own folder and `-` below it
(`shell::map::tests::the_claude_home_is_answered_in_the_bound_folder_itself_only`); Codex reads its route
from its home, wherever it starts, and keeps it below the folder. The placeholder is a fixed word, not a
credential (dossier A), so the 401 carried nothing of the person's or the organization's.
