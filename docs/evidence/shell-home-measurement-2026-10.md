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
