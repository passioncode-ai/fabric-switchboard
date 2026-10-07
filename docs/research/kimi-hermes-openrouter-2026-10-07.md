# Research: Kimi Code, Hermes Agent, OpenRouter for agents (2026-10-07)

Read-only research for Fabric Switchboard. No credential file, keychain item or key value on this machine was read; only `--help`/`--version` output, directory names and public source/docs. Each section below cites GitHub permalinks (commit-pinned) or doc URLs; items marked **unverified** were not confirmed from a primary source or a live run.

## Combined table: OpenRouter key supplied only via env at launch (Q3)

| Agent | Key env var | Base URL | Model selection | Main caveat |
|---|---|---|---|---|
| OpenClaw | `OPENROUTER_API_KEY` | built in | `--model openrouter/<author>/<slug>` | use `openclaw agent exec --auth-env-only`; normal mode lets config/saved login win; `openclaw agent` via Gateway daemon reads the daemon's env |
| Hermes | `OPENROUTER_API_KEY` (`OPENROUTER_BASE_URL` optional) | built in `https://openrouter.ai/api/v1` | `--provider openrouter -m <slug>` or `HERMES_INFERENCE_MODEL` | `~/.hermes/.env` and `hermes auth add` pool beat the launch env; isolate with `HERMES_HOME` |
| pi (earendil-works/pi) | `OPENROUTER_API_KEY` | built in | `--model openrouter/<id>` or `--provider openrouter --model <id>` | isolate saved `auth.json` with `PI_CODING_AGENT_DIR`; precedence vs saved key unverified |
| Kimi Code | `KIMI_MODEL_API_KEY` | `KIMI_MODEL_BASE_URL=https://openrouter.ai/api/v1` + `KIMI_MODEL_PROVIDER_TYPE=openai` | `KIMI_MODEL_NAME=<vendor>/<model>` | ignores `OPENROUTER_API_KEY`; alt: `config.toml` provider with `api_key_env` (name only) |
| OpenCode | `OPENROUTER_API_KEY` | from models.dev | `opencode run -m openrouter/<author>/<slug>` | key saved via `/connect` likely overrides env (merge order; unverified at runtime) |
| Goose | `OPENROUTER_API_KEY` | `OPENROUTER_HOST` (default `https://openrouter.ai`) | `GOOSE_PROVIDER=openrouter GOOSE_MODEL=<slug>` or `--provider/--model` | env checked before keychain/config; never run `goose configure` (saves to keychain) |
| Aider | `OPENROUTER_API_KEY` | `OPENROUTER_API_BASE` (unverified, litellm convention) | `--model openrouter/<author>/<slug>` or `AIDER_MODEL` | — |
| Kilo CLI | `OPENROUTER_API_KEY` | built in | `-m openrouter/<vendor>/<model>` | key saved by `kilo auth` overrides env; data-dir override var unverified |
| Cline CLI | `OPENROUTER_API_KEY` | built in | `-P openrouter -m <vendor>/<model>` | source-read only; avoid `-k` and `cline auth --apikey`; `--data-dir`/`CLINE_DATA_DIR` |
| Crush | `OPENROUTER_API_KEY` | built in | `crush run -m openrouter/<model>`; TUI uses picker / `models.large` | pasting a key in the picker saves it to config |
| Qwen Code | `OPENAI_API_KEY` | `OPENAI_BASE_URL=https://openrouter.ai/api/v1` | `OPENAI_MODEL` or `-m`, plus `--auth-type openai` | settings provider entries override env; `QWEN_HOME` isolates |
| omp (oh-my-pi, can1357/oh-my-pi) | `OPENROUTER_API_KEY` | built in | `--model openrouter/<pattern>` | saved login/OAuth overrides env; `.env` files lose to launch env |

Never pass the key as a CLI flag (`--api-key`, `-k`, `--openai-api-key`): it lands in argv and process listings.

---

# Part 1 — Kimi Code CLI (`kimi` 2.1.1) with a Kimi Code membership login

Researched 2026-10-07. Source: `MoonshotAI/kimi-code` shallow clone at commit
[`21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3`](https://github.com/MoonshotAI/kimi-code/tree/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3)
(main, TypeScript monorepo). Permalink prefix used below:
`https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/`
(abbreviated **`@21406fb/`**). On this machine: `kimi --version` → `2.1.1`; `kimi --help`,
`kimi login --help`, `kimi provider list --help`, `kimi doctor --help` run; `~/.kimi-code` top-level
names listed (`config.toml`, `credentials`, `oauth`, `device_id`, `region`, `server.token`, …). No
credential file was opened, listed or printed.

Caveat: the clone is today's `main`; the installed binary is 2.1.1. I did not diff 2.1.1 against
`main` — paths/fields below are **from current source, not verified against the 2.1.1 build**.

## a. How `/login` (managed Kimi Code OAuth) works

- **Device-code flow (RFC 8628), not a localhost browser callback.** Package header: "Only Device
  Code Flow (RFC 8628) is supported, against `https://auth.kimi.com`"
  ([`@21406fb/packages/oauth/src/types.ts#L1-L9`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/types.ts#L1-L9)).
  `kimi login --help`: "Authenticate with Kimi Code CLI via the device-code flow." The CLI prints the
  `verification_uri_complete` + `user_code` to **stderr** and then best-effort opens the browser
  ([`apps/kimi-code/src/cli/sub/login-flow.ts#L40-L69`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/apps/kimi-code/src/cli/sub/login-flow.ts#L40-L69));
  on success prints `Logged in to <providerName>.` and exits 0. Device-code wait timeout 15 min
  ([`oauth-manager.ts#L29`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/oauth-manager.ts#L29)).
- **Endpoints** (all `POST`, `application/x-www-form-urlencoded`, to the OAuth host)
  ([`packages/oauth/src/oauth.ts#L1-L11`, `#L123`, `#L173-L180`, `#L239-L252`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/oauth.ts#L1-L11)):
  - `/api/oauth/device_authorization` — body `client_id` → `user_code`, `device_code`,
    `verification_uri`, `verification_uri_complete`, `expires_in`, `interval` (default 5).
  - `/api/oauth/token` — `grant_type=urn:ietf:params:oauth:grant-type:device_code`, `client_id`,
    `device_code`; pending errors `authorization_pending` / `slow_down`; terminal `expired_token`,
    `access_denied`.
  - `/api/oauth/token` — `grant_type=refresh_token`, `client_id`, `refresh_token`; retried up to 3×
    on 429/5xx with 1 s/2 s backoff. Response must carry `access_token`, `refresh_token`,
    `expires_in` (+ optional `scope`, `token_type`) ([`oauth.ts#L29-L53`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/oauth.ts#L29-L53)).
  - Requests carry device headers `X-Msh-Platform`, `X-Msh-Version`, `X-Msh-Device-Name`,
    `X-Msh-Device-Model`, `X-Msh-Os-Version`, `X-Msh-Device-Id` ([`types.ts#L47-L55`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/types.ts#L47-L55));
    the device id is persisted in `<home>/device_id` (0600) ([`identity.ts#L42`, `#L62`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/identity.ts#L42)).
- **Hosts / client** ([`constants.ts#L3-L21`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/constants.ts#L3-L21),
  [`region.ts#L61-L77`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/region.ts#L61-L77)):
  public `client_id` `17e5f671-d194-4dfb-9706-5516cb48c098` (shared by both regions).
  | Region (`kimi login --region`) | OAuth host | Managed API base |
  |---|---|---|
  | `mainland-cn` (default) | `https://auth.kimi.com` | `https://api.kimi.com/coding/v1` |
  | `global` | `https://auth.kimi.ai` | `https://api.kimi.ai/coding/v1` |
  Env overrides: `KIMI_CODE_OAUTH_HOST` (or legacy `KIMI_OAUTH_HOST`), `KIMI_CODE_BASE_URL`.
- **Storage — plain files, no Keychain.** `FileTokenStorage` writes `<KIMI_CODE_HOME>/credentials/<name>.json`,
  file 0600, dir 0700, atomic tmp→fsync→rename
  ([`storage.ts#L1-L13`, `#L59-L117`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/storage.ts#L1-L13);
  credentials dir = `join(homeDir, 'credentials')`, [`toolkit.ts#L126-L127`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/toolkit.ts#L126-L127)).
  `grep -i keychain|keytar|safeStorage` over `apps/` and `packages/` source: no hit outside a
  code-signing test → **no macOS Keychain use** (only storage backend type is `'file'`, `types.ts#L11`).
  - File names: mainland login → `credentials/kimi-code.json` (oauth key `oauth/kimi-code`);
    any other (oauthHost, baseUrl) pair, including the global region → `credentials/kimi-code-env-<16 hex of sha256({oauthHost,baseUrl})>.json`
    ([`managed-kimi-code.ts#L11-L14`, `#L324-L341`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/managed-kimi-code.ts#L324-L341);
    [`toolkit.ts#L465-L479`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/toolkit.ts#L465-L479)).
    MCP-server OAuth creds live separately in `credentials/mcp/<key>-<suffix>.json` (docs below).
  - JSON field names (snake_case): `access_token`, `refresh_token`, `expires_at` (unix s),
    `scope`, `token_type`, `expires_in` ([`types.ts#L63-L71`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/types.ts#L63-L71)).
    A revoked login is kept as a tombstone with empty tokens and `expires_at: 0` ([`token-state.ts#L1-L45`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/token-state.ts)).
  - `oauth/` directory = **cross-process refresh lock**, not tokens: `proper-lockfile` creates
    `<home>/oauth/<providerName>.lock`; refresh fails closed if the lock cannot be taken
    ([`oauth-manager.ts#L73-L78`, `#L176-L233`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/oauth-manager.ts#L176-L233)).
  - `config.toml` holds only a reference: `[providers."managed:kimi-code"].oauth = { storage, key, oauthHost? }`
    ("injected automatically by the login flow", docs/en/configuration/config-files.md#L129).
- **Lifetimes: server-defined, not hard-coded — actual values UNVERIFIED.** The client stores
  `expires_in` from the server and refreshes when remaining life < max(300 s, 0.5 × expires_in)
  ([`oauth-manager.ts#L27-L36`, `#L463`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/oauth-manager.ts#L27-L36)).
  Refresh-token lifetime is not in source or docs. **Refresh tokens rotate** (source comments: "racing
  refresh_token rotation", "another process rotated the refresh_token", `oauth-manager.ts#L193`, `#L371-L374`;
  the refresh response must include a new `refresh_token`) — so two homes holding a copy of one
  credential file will invalidate each other on the first refresh. Docs also say "devices inactive for
  over 30 days are automatically unbound" ([membership page](https://www.kimi.com/code/docs/en/kimi-code/membership.html)).

## b. Several accounts / switching / home override

- **Home override: `KIMI_CODE_HOME`** (default `~/.kimi-code`) — "Once set, **all** Kimi Code data lands under
  the new path: config, sessions, logs, OAuth credentials …"; "Multiple `kimi` instances sharing the same
  `KIMI_CODE_HOME` will share config and credential files"
  (docs/en/configuration/env-vars.md#L15-L23, docs/en/configuration/data-locations.md#L13-L19 in the repo;
  source [`toolkit.ts#L481-L485`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/toolkit.ts#L481-L485), `region.ts#L173-L177`).
  `device_id`, `region` marker, `server.token` move with it too.
- **One managed account per (region) slot per home.** There is no multi-account list or `switch`
  command; the slot key is derived from (oauthHost, baseUrl) only, so a mainland and a global login can
  coexist in one home, but two mainland accounts cannot. `/login` "Select an account or platform"
  chooses between Kimi Code (OAuth) and Kimi Platform (API key), not between accounts
  (docs/en/reference/slash-commands.md#L15-L16).
- **Practical switching = one `KIMI_CODE_HOME` per account** (documented, supported). Swapping
  `credentials/kimi-code.json` in place works mechanically (load re-reads the file; no cache other than
  in-process) but is **not documented** and races with rotation and the `oauth/*.lock` refresh lock — UNVERIFIED as safe.
- Note: `KIMI_API_KEY` etc. are **not** read from the shell; only `KIMI_MODEL_*` and a provider's
  `api_key_env` read env (docs/en/configuration/env-vars.md#L6-L8).

## c. Usage / quota

- **Upstream endpoint:** `GET {base}/usages`, base = `https://api.kimi.com/coding/v1` (global
  `https://api.kimi.ai/coding/v1`, or `KIMI_CODE_BASE_URL`); headers `Authorization: Bearer <access_token>`,
  `Accept: application/json`; 8 s timeout; 401 → "try /login", 404 → "Usage endpoint not available"
  ([`managed-usage.ts#L8-L23`, `#L218-L256`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/managed-usage.ts#L218-L256)).
- **Response fields** (raw, parsed at `managed-usage.ts#L108-L185`):
  - `usages.limit_5h`, `usages.limit_7d`, `usages.limit_month_total`, `usages.limit_month_code` —
    each `{ used_ratio (0–1, number or string), reset_time (RFC3339) }`; any may be absent.
  - `boosterWallet` — `balance { type: "BOOSTER", amount, amountLeft }` (fixed-point, /1 000 000 = cents),
    `monthlyChargeLimitEnabled`, `monthlyChargeLimit { priceInCents, currency }`,
    `monthlyUsed { priceInCents, currency }`.
- **Profile ("who am I"):** `GET {base}/me`, same bearer header → `user_id`, `nickname`, `status`,
  `region`, `user_level`, `user_level_name`, `domain`, `domain_name`, optional `goods_version`,
  `global_id`, `avatar`, `username`, `email`, `phone{country_code,number}`, `created_time`,
  `last_login_time` ([`managed-userinfo.ts#L1-L48`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/oauth/src/managed-userinfo.ts#L1-L48)).
  `user_level_name` (e.g. "Vivace", "Allegretto") is the membership tier.
- **In the CLI:** slash `/usage` — "Show session tokens + context window + plan quotas"; `/status` —
  session/runtime state (version, model, cwd, permission mode), not quota
  ([`apps/kimi-code/src/tui/commands/registry.ts#L353-L366`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/apps/kimi-code/src/tui/commands/registry.ts#L353-L366)).
  Both are interactive TUI only; no `kimi usage` subcommand exists in 2.1.1 `--help`.
- **Local REST (non-interactive, via `kimi web`):** `GET /api/v1/oauth/usage`, `GET /api/v1/oauth/userinfo`,
  `GET /api/v1/oauth/region` on `http://127.0.0.1:58627`, `Authorization: Bearer <server.token>`; envelope
  `{code,msg,data,request_id}`; `data` = `{kind:"ok",quota:{usages:{limit5h,limit7d,monthTotal,monthCode:{usedRatio,resetAt}},extraUsage}}`
  or `{kind:"error",message,status?}` (docs/en/reference/server-api.md#L186-L262;
  [`packages/kap-server/src/routes/oauth.ts#L143-L180`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/kap-server/src/routes/oauth.ts#L143-L180)).
  Marked "experimental" in docs.
- Quota windows per docs: rolling **5-hour** rate window; **7-day** quota on legacy plans; **monthly**
  total shared with Kimi membership; "Requests from the CLI, VS Code, Desktop, and third-party tools all count
  toward that quota"; "All logged-in devices and API Keys share the same quota"
  ([membership](https://www.kimi.com/code/docs/en/kimi-code/membership.html)).

## d. Terms — using the membership OAuth token outside the official CLI

- Docs split two auth paths: **OAuth "for official clients"** (`/login`) vs **an API Key created in the
  Kimi Code Console** for third-party tools ([membership guide](https://www.kimi.com/en/help/kimi-code/membership-guide);
  [third-party agents](https://www.kimi.ai/help/kimi-code/third-party-agents): "An API Key (created in the Kimi Console)"
  is what third-party agents use). Documented third-party integrations (Claude Code, OpenCode, Codex,
  Hermes Agent, Roo Code) all use the **console API key** against `https://api.kimi.com/coding/` (Anthropic
  protocol) / `…/coding/v1` ([Claude Code page](https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html)).
- Quoted: "Keep the tool's real identity identifier when using it. Tampering with the client identifier
  (User-Agent) will be considered a violation and may result in suspension of membership benefits."
  ([membership guide](https://www.kimi.com/en/help/kimi-code/membership-guide); same wording on
  [kimi.com/coding/docs/en](https://www.kimi.com/coding/docs/en/)). And: "If you need to call large model
  capabilities in your own product … please visit the Kimi Platform."
- **No document found that explicitly permits or forbids reusing the `/login` OAuth access token in a
  third-party client.** The docs only ever offer OAuth for official clients and API keys for third parties;
  so reusing the OAuth token elsewhere is, at best, undocumented. A search-engine snippet attributed to
  Kimi docs reads "This benefit is for personal development only and may not be used in enterprise
  scenarios. Users must confine programming-related usage to Kimi CLI and any other coding agents we permit
  … the use of the base_url or API Key associated with this benefit in any unapproved, automated, or
  derivative products or services constitutes unauthorised use" — **UNVERIFIED: not found on any page I could
  fetch** (pages fetched: membership guide, kimi.com/coding/docs/en, third-party-agents).
- Implication for a manager: launching the **official `kimi` binary** under a per-account
  `KIMI_CODE_HOME` keeps the real client identity and is the documented model; calling `/usages` and `/me`
  with the stored token from another process is what the official client itself does, but is not
  documented as a public API (UNVERIFIED stance).

## e. Non-interactive login-status check

- **No `kimi status`/`whoami`/`auth status` command in 2.1.1** (`kimi --help` subcommands: export, fork,
  provider, session, acp, web, server, rc, login, doctor, vis, install-desktop, migrate, upgrade).
- `kimi login` is **not** a safe probe: when no valid token exists it starts the device flow and opens a browser.
  (The REST login start has an "already authenticated" fast path returning `status:"authenticated"`;
  whether the CLI `kimi login` short-circuits the same way was not verified.)
- `kimi provider list` (text) prints `id  type=…  models=N  source=…` per configured provider — shows whether
  `managed:kimi-code` is configured, not whether its token is valid, and not who. **Do not use
  `kimi provider list --json`**: it dumps the raw `providers` table, which can include plaintext `api_key`
  for API-key providers ([`apps/kimi-code/src/cli/sub/provider.ts#L144-L147`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/apps/kimi-code/src/cli/sub/provider.ts#L144-L147)).
- Token-free options: (1) check existence/`expires_at` presence of `credentials/kimi-code*.json` via
  metadata only (file present; tombstone = empty `access_token`) — requires reading the file in-process,
  never echoing it; (2) `GET {base}/me` with the access token → nickname/tier ("as whom"); (3) `kimi web`
  then `GET /api/v1/oauth/userinfo` with the local `server.token`.

---

# Part 2: Hermes Agent, model/provider config and OpenRouter

Scope: question 2 (a, b) and the Hermes row of question 3. Researched 2026-10-07.

- Installed here: `Hermes Agent v0.21.4 (2026.9.21) · upstream a928a959` (Homebrew, `/opt/homebrew/bin/hermes`), taken from `hermes --version`.
- Source citations are pinned to the installed upstream commit
  [`a928a959e33b5ed9383afb020d31eb512f1f8e3d`](https://github.com/NousResearch/hermes-agent/tree/a928a959e33b5ed9383afb020d31eb512f1f8e3d) (2026-10-05). `main` was at `13dc3a73` on 2026-10-06. Abbreviation used below: `@a928a95` = `https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d`.
- Method: CLI `--help` output on this machine, `hermes config get model --json` (key names and non-secret values only), `hermes usage --json` (printed as a type outline only), and a blob-less clone of the repo, read as data. **No** credential file was read: `~/.hermes/.env` and `~/.hermes/auth.json` were not opened.

## 2a. Where the active model/provider lives, and how to read it non-interactively

### Config keys (`~/.hermes/config.yaml`, section `model:`)

| Key | Meaning | Source |
|---|---|---|
| `model.default` (alias `model.model`) | model id, e.g. `anthropic/claude-opus-4.6`; for OpenRouter this is an OpenRouter slug | [`cli-config.yaml.example` L72-75](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/cli-config.yaml.example#L72-L75) ("Both "default" and "model" work as the key name") |
| `model.provider` | `auto` (default), `openrouter`, `anthropic`, `nous`, `openai-codex`, `kimi-coding`, `custom`, … | [`cli-config.yaml.example` L77-108](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/cli-config.yaml.example#L77-L108) |
| `model.base_url` | endpoint; `https://openrouter.ai/api/v1` for OpenRouter | [L112](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/cli-config.yaml.example#L112) |
| `model.api_key` | inline key (discouraged; "falls back to OPENROUTER_API_KEY env var") | [L110-111](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/cli-config.yaml.example#L110-L111) |
| `model.api_mode` | `chat_completions` / `anthropic_messages` / `codex_responses` | seen in this machine's config |
| `model.key_env` (alias `model.api_key_env`) | name of an env var that holds the key — **used only for custom/trusted `base_url` endpoints, not for the OpenRouter branch** | [`hermes_cli/auth.py` L361-377](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/auth.py#L361-L377), [`runtime_provider_backends.py` L177-182](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider_backends.py#L177-L182) |
| `providers.<name>.{api,key_env,api_key,key_cmd,transport,default_model,…}` | named custom providers; `key_cmd` is a command that prints a token | [`website/docs/integrations/providers.md` L1336-1381](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/integrations/providers.md#L1336-L1381) |
| `fallback_providers: []` | fallback chain | [`hermes_cli/config_defaults.py` L33-36](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/config_defaults.py#L33-L36) |

This machine (`hermes config get model --json`, values that are not secret): `default = deepseek/deepseek-v4-pro-0813`, `provider = openrouter`, `base_url = https://openrouter.ai/api/v1`, `api_mode = chat_completions`. Paths: `hermes config path` prints `~/.hermes/config.yaml`, and `hermes config env-path` prints `~/.hermes/.env`.

### Commands that print the current model and provider (verified on v0.21.4)

| Command | Output | JSON? |
|---|---|---|
| `hermes config get model --json` | the whole `model` section as one JSON object; secret-shaped values (`api_key`, `token`, …) masked unless `--raw` | **yes** (`--json`) |
| `hermes config get model.provider` / `hermes config get model.default` | the bare value on one line (`openrouter`, `deepseek/deepseek-v4-pro-0813`) | plain; `--json` also accepted |
| `hermes dump` | plain text block for support: `model:            <id>`, `provider:         <id>`, then per-provider `set`/`not set` (add `--show-keys` for first/last-4 prefixes) | no |
| `hermes status` | human block: `Model: <id>`, `Provider: <label>`, plus per-provider auth rows | no |
| `hermes usage [--provider P] --json` | account limits for the configured provider (OpenRouter: credits) | **yes** |

Notes on these commands:

- `hermes config get --help` documents `--json` ("Print value as JSON") and `--raw` ("Print credential values unmasked (default masks api_key/token/secret-shaped values)"). The secret-key mask list is [`hermes_cli/config.py` L2978-2984](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/config.py#L2978-L2984).
- **`hermes status` prints a key fragment by default.** It shows the OpenRouter key as first-4/last-4 characters (`✓ sk-o...NNNN`) with no flag. Switchboard should not log `hermes status` output; `hermes dump` without `--show-keys` prints only `set`/`not set`.
- **`hermes status` labels OpenRouter as "Custom endpoint"** whenever `model.base_url` is set, even to the canonical `openrouter.ai` URL ([`hermes_cli/status.py` L72-90](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/status.py#L72-L90)). The provider string should be read from `config get`, not from `status`.
- `hermes usage --json` has this shape (measured here; types only): `{provider, source, title, plan, fetched_at, windows:[{label, used_percent, resets_at, detail}], details:[str], unavailable_reason}`. For OpenRouter it calls `GET {base_url}/credits` and `GET {base_url}/key` with `Authorization: Bearer <key>`. It reads `total_credits`/`total_usage` and `limit`/`limit_remaining`/`limit_reset`/`usage`/`usage_daily|weekly|monthly`, then emits the window "API key quota" ([`agent/account_usage.py` L661-695](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/agent/account_usage.py#L661-L695)). It exits 1 when no credential is configured or the fetch fails (`hermes usage --help`).
- One-shot runs: `hermes -z PROMPT --usage-file PATH` writes a JSON usage report (estimated cost, tokens, model, api_calls) even when the run fails (`hermes --help`).

## 2b. Changing provider and model non-interactively

- **Persistent:** `hermes config set model.provider openrouter` and `hermes config set model.default <slug>`. A bare `hermes config set model <slug>` is redirected to `model.default` ([`hermes_cli/config.py` L3635-3641](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/config.py#L3635-L3641); docs [`configuration.md` L44-49](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/user-guide/configuration.md#L44-L49); the same pair appears in [`guides/xai-grok-oauth.md` L103-104](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/guides/xai-grok-oauth.md#L103-L104)). `hermes config set model.base_url https://openrouter.ai/api/v1` sets the endpoint. *Not executed here*, because the task is read-only. The behaviour comes from source and docs.
- `hermes model` is **interactive only**. Its flags are all for Nous login (`hermes model --help`). The `/model` slash command inside a session can switch only between providers that are already configured ([`providers.md` L724](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/integrations/providers.md#L724)).
- **Per invocation (no file write):** `hermes -z "<prompt>" -m <slug> --provider openrouter`, or `hermes chat -q … -m … --provider …`. `-m/--model` and `--provider` apply to `-z`/`--tui`/`chat` ([`hermes_cli/_parser.py` L159-167](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/_parser.py#L159-L167)). In `-z` mode `--provider` requires `--model` (or `HERMES_INFERENCE_MODEL`) ([`hermes_cli/oneshot.py` L261-264](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/oneshot.py#L261-L264)).
- **Env overrides:** `HERMES_INFERENCE_MODEL` overrides the model for the process ([env reference L821, L905](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/reference/environment-variables.md#L821)). `HERMES_INFERENCE_PROVIDER` is only a fallback: **`model.provider` in config.yaml beats it**. The order is explicit arg > config > env > `auto` ([`runtime_provider.py` L478-487](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider.py#L478-L487)). `HERMES_MODEL` is the cron scheduler's override (env ref L123).
- **Separate home:** `HERMES_HOME=<dir>` selects the whole config and data home (`config.yaml`, `.env`, `auth.json`). The default is `~/.hermes` ([env ref L127](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/reference/environment-variables.md#L127)). `hermes profile …` manages multiple isolated instances.
- **Fallback chain:** `hermes fallback list|add|remove|clear`. `add` uses the interactive picker.

### OpenRouter specifics

- Provider id: `openrouter`. Default base URL: `OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1"` ([`hermes_constants.py` L1233](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_constants.py#L1233)). The `OPENROUTER_BASE_URL` env var overrides it.
- Key env var: **`OPENROUTER_API_KEY`** ([env ref L15-16](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/reference/environment-variables.md#L15-L16); [`providers.md` L22](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/integrations/providers.md#L22)). `OPENAI_API_KEY` is a legacy fallback: it is accepted for openrouter.ai only if it starts with `sk-or-`, or if `OPENAI_BASE_URL` binds it to that exact origin ([`runtime_provider_backends.py` L156-176](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider_backends.py#L156-L176)).
- Key resolution order in the OpenRouter branch: explicit `--api-key` > `OPENROUTER_API_KEY` > a qualifying `OPENAI_API_KEY` ([L176-177](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider_backends.py#L176-L177)). **`model.key_env`, `model.api_key` and `key_cmd` are not consulted on this branch.** Those apply to custom endpoints and named `providers:` entries ([L178-182](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider_backends.py#L178-L182)).
- Before that fallback, the **credential pool** is tried for plain `openrouter`/`auto` requests with no custom endpoint (resolution ladder rung 6, [`runtime_provider.py` L578-590, L991](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/runtime_provider.py#L578-L590)). The pool comes from `hermes auth add openrouter` (an API key or OpenRouter PKCE OAuth, stored in `~/.hermes/auth.json`) and from env seeding.
- **`~/.hermes/.env` beats the process environment.** `get_env_value_prefer_dotenv` / `get_env_prefer_dotenv` read `.env` first and `os.environ` second ([`hermes_cli/config.py` L2954-2967](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/config.py#L2954-L2967); [`agent/credential_pool.py` L2854-2869](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/agent/credential_pool.py#L2854-L2869)). A launch-time `OPENROUTER_API_KEY` is therefore **ignored if `.env` also defines it**. A manual pool entry from `hermes auth add` is also tried before the env fallback.
- Env-seeded pool rows are written to `auth.json` **without the secret**: they carry only label, source (`env:OPENROUTER_API_KEY`), status and a `sha256` fingerprint, and are rehydrated from env on each load ([`credential_pool.py` L2925-2934, L2960-2967](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/agent/credential_pool.py#L2925-L2967); [`agent/credential_persistence.py` L16-22, L56-62, L99-116](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/agent/credential_persistence.py#L99-L116)). Hermes logs a one-time WARNING when it ingests an env OpenRouter key ("enables OpenRouter spend") ([`credential_pool.py` L2875-2896](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/agent/credential_pool.py#L2875-L2896)).
- `key_cmd`, for a key from a command rather than a file: available only on a **named provider** entry, `providers.<name>.key_cmd: "<cmd printing the token>"`. It is cached until expiry, and it wins over `api_key`/`key_env` on the same entry ([`providers.md` L1356-1381](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/integrations/providers.md#L1356-L1381)). It could point at OpenRouter as a custom provider (`api: https://openrouter.ai/api/v1`, `transport: chat_completions`). **Unverified:** whether such an entry keeps OpenRouter-specific features such as `provider_routing` and the usage credits view. Those are keyed on provider `openrouter`, and a named entry resolves as `custom`.
- `hermes config set OPENROUTER_API_KEY …` writes to `.env` ([`configuration.md` L44-53](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/website/docs/user-guide/configuration.md#L44-L53)). That is the path to avoid if the key must stay env-only.
- External secret sources: `hermes secrets` (Bitwarden, 1Password `op://` references in `.env`, resolved into a scoped env) ([`config.py` L2958-2966](https://github.com/NousResearch/hermes-agent/blob/a928a959e33b5ed9383afb020d31eb512f1f8e3d/hermes_cli/config.py#L2958-L2966)).

## Question 3: the Hermes row (OpenRouter key supplied only through env at launch)

| Agent | Env var(s) | Base URL | Required flag/config | Model pick |
|---|---|---|---|---|
| Hermes (`hermes`) | `OPENROUTER_API_KEY` (optionally `OPENROUTER_BASE_URL`) | `https://openrouter.ai/api/v1` (built-in default) | `--provider openrouter` per run (or `model.provider: openrouter` in config.yaml, which holds no key). **Caveat:** the `HERMES_HOME` used must have no `OPENROUTER_API_KEY` in its `.env` and no manual openrouter pool entry, or those win. The clean way is `HERMES_HOME=<isolated dir>`. | `-m <openrouter-slug>` or `HERMES_INFERENCE_MODEL=<slug>`; with `-z`, `--provider` requires `-m` |

Example (not executed): `HERMES_HOME=/path/iso OPENROUTER_API_KEY=… hermes -z "…" --provider openrouter -m deepseek/deepseek-v4-pro-0813`

## Unverified / open

- `hermes config set model.provider|model.default` was **not executed**, because the task is read-only. Its behaviour is taken from source, docs and `--help`.
- Whether `HERMES_HOME` pointing at an empty directory triggers a first-run setup wizard in `-z` mode was not tested.
- It is not confirmed which source holds the OpenRouter key Hermes sees on this machine (`.env`, the process env, or the pool). `hermes dump` shows only `openrouter set`, and `.env`/`auth.json` were deliberately not read.
- Named-provider `key_cmd` used for OpenRouter keeps or loses OpenRouter-only features (routing, `/usage` credits): unverified, see above.

---

# Part 3a + 4: OpenRouter key API, and OpenRouter-via-env for OpenClaw, pi, OpenCode, Goose, Aider

Researched 2026-10-07. Sources read at the commits named below. No credential on this machine was read.
"Verified" = read in docs or source at the cited place. "Unverified" = inferred, not read.

## 4. OpenRouter API

### 4a. Key status: `GET https://openrouter.ai/api/v1/key`

- Auth: `Authorization: Bearer <key>` (the key being inspected). Doc:
  https://openrouter.ai/docs/api/reference/limits and
  https://openrouter.ai/docs/api/api-reference/api-keys/get-current-key
- Response `{ "data": { ... } }`, fields (doc schema):
  `label`, `limit` (USD cap or null), `limit_reset` (`daily|weekly|monthly` or null),
  `limit_remaining` (or null if unlimited), `include_byok_in_limit`,
  `usage`, `usage_daily`, `usage_weekly`, `usage_monthly` (UTC day/week/month),
  `byok_usage`, `byok_usage_daily|weekly|monthly`, `is_free_tier`,
  `is_management_key`, `is_provisioning_key` (deprecated), `creator_user_id`,
  `organization_id`, `workspace_id`, `allowed_data_regions`,
  `free_model_daily_requests {used, limit, remaining}`, `rate_limit` (deprecated
  `{interval, requests}`), `expires_at`.
- `GET /api/v1/auth/key`: **live but undocumented in the current reference.** Probed
  2026-10-07 without credentials: `/api/v1/key` → 401, `/api/v1/auth/key` → 401
  (`"No cookie auth credentials found"`), unknown path → 404. So the legacy path is still
  routed; that it returns the identical body is **unverified** (needs a real key). Use
  `/api/v1/key`.
- Account-wide balance: `GET /api/v1/credits` → `data.total_credits`, `data.total_usage`;
  **management key only** ("Only management keys can perform this operation").
  https://openrouter.ai/docs/api/api-reference/credits/get-credits

### 4b. Provisioning (management) key → sub-keys

Doc: https://openrouter.ai/docs/guides/overview/auth/provisioning-api-keys and
https://openrouter.ai/docs/api/api-reference/api-keys/create-keys

| Op | Method + path |
|---|---|
| Create | `POST /api/v1/keys` |
| List | `GET /api/v1/keys` |
| Get | `GET /api/v1/keys/{hash}` |
| Update | `PATCH /api/v1/keys/{hash}` |
| Delete | `DELETE /api/v1/keys/{hash}` |

All with `Authorization: Bearer <MANAGEMENT_KEY>`.

Create body:
- `name` (string, required)
- `limit` (number|null, USD)
- `limit_reset` (`"daily" | "weekly" | "monthly"` | null = never resets)
- `include_byok_in_limit` (bool)
- `expires_at` (ISO-8601 UTC with seconds, `YYYY-MM-DDTHH:MM:SSZ`, or null)
- `workspace_id` (uuid), `creator_user_id`, `external {user, api_key?}` (optional)

Create response (201): top-level **`key`** = the secret string `sk-or-v1-…`, "only shown
once"; plus `data` = the key record (`hash`, `name`, `label` (masked), `disabled`,
`limit`, `limit_remaining`, `limit_reset`, usage fields, `created_at`, `updated_at`,
`expires_at`, …). Later calls address the key by `hash`; the secret is never returned again.

Example (daily $5 cap):
```
POST /api/v1/keys  {"name":"agent-x","limit":5,"limit_reset":"daily"}
```
Daily reset happens at UTC midnight (doc: "usage_daily: Credits used in current UTC day";
that the `daily` limit window aligns with the same UTC day is **unverified** beyond that wording).

## 3. OpenRouter key via environment variable only

| Agent | Repo / package (commit) | Key env var | Base URL | Required flag/config | Model selection |
|---|---|---|---|---|---|
| **OpenClaw** | `openclaw/openclaw` @ `2ae568e` | `OPENROUTER_API_KEY` | built in (`openrouter` provider); custom `baseUrl` only via config | none. For a strict env-only run: `openclaw agent exec … --auth-env-only` (loads **no config**, skips auth profiles and external CLI credential stores) | `--model openrouter/<author>/<slug>` (e.g. `openrouter/moonshotai/kimi-k2.6`, `openrouter/auto`); `--fallback` repeatable; persistent: `openclaw models set …` (writes config, not key) |
| **pi** | `earendil-works/pi` (old `badlogic/pi-mono` redirects) @ `eb326d2`, npm `@earendil-works/pi-coding-agent` 1.0.4, bin `pi` | `OPENROUTER_API_KEY` | built in | none; isolate with `PI_CODING_AGENT_DIR=<empty dir>` so no `auth.json` is consulted | `--provider openrouter --model <id>` or `--model openrouter/<author>/<slug>[:thinking]`; `--models` for cycling |
| **OpenCode** | `anomalyco/opencode` (was `sst/opencode`) @ `ecc4916`; installed here 1.18.10 | `OPENROUTER_API_KEY` (from models.dev `openrouter.env`) | `https://openrouter.ai/api/v1` (models.dev), SDK `@openrouter/ai-sdk-provider` | none | `opencode run -m openrouter/<author>/<slug>` (`provider/model`); or `OPENCODE_CONFIG_CONTENT='{"model":"openrouter/…"}'` |
| **Goose** | `aaif-goose/goose` (was `block/goose`) @ `067ca1e`; installed here 1.44.0 | `OPENROUTER_API_KEY` | `OPENROUTER_HOST` optional, default `https://openrouter.ai` (host only) | `GOOSE_PROVIDER=openrouter` (or `--provider openrouter`); optional `GOOSE_DISABLE_KEYRING=1` | `GOOSE_MODEL=<author>/<slug>` or `goose run --provider openrouter --model <author>/<slug>` |
| **Aider** | `Aider-AI/aider` @ `5dc9490` (last commit 2026-05-22) | `OPENROUTER_API_KEY` | litellm default; override `OPENROUTER_API_BASE` (**unverified**, litellm convention) | none | `--model openrouter/<author>/<slug>` or `AIDER_MODEL=…`; `aider --list-models openrouter/` |

### Evidence and caveats per agent

**OpenClaw**
- Docs: https://docs.openclaw.ai/providers/openrouter — key resolution "model-specific
  configuration → provider-level apiKey → `OPENROUTER_API_KEY` → saved OAuth/API-key auth
  profiles"; model refs `openrouter/<provider>/<model>`.
- Env precedence: https://docs.openclaw.ai/help/environment — process env wins ("never
  override existing values"); `OPENCLAW_HOME`, `OPENCLAW_STATE_DIR`, `OPENCLAW_CONFIG_PATH`
  relocate state/config.
- `--auth-env-only`: `docs/cli/agent.md` L48 and L315 at
  https://github.com/openclaw/openclaw/blob/2ae568e850a67b2b1d79c778c4cd833f129ae805/docs/cli/agent.md#L48
  — "restrict the run to provider keys already present in the process environment. That
  mode loads no config at all … also skips OpenClaw auth profiles and external Codex, Claude,
  or other CLI credential stores. Provider auth variables … are omitted from agent-launched
  host commands." Pairing with `--config` is rejected.
- Caveat: plain `openclaw agent` (no `--local`/`exec`) runs **through the Gateway daemon**,
  so the key must be in the *Gateway's* process env, not the launching shell. Use
  `agent exec` / `--local` for a per-launch key. A config/auth-profile key outranks env
  in the normal mode (order above).
- Not installed on this machine.

**pi**
- Env map: `packages/ai/src/env-api-keys.ts` L100 `openrouter: "OPENROUTER_API_KEY"`
  https://github.com/earendil-works/pi/blob/eb326d265ae0b88489a6d10319307780df827cdf/packages/ai/src/env-api-keys.ts#L100
- Flags: `packages/coding-agent/src/cli/args.ts` L301-303 (`--provider`, `--model` "supports
  provider/id and optional :<thinking>", `--api-key` "defaults to env vars"), L438 lists
  `OPENROUTER_API_KEY`, L462 `PI_CODING_AGENT_DIR` config dir.
- Docs: `packages/coding-agent/docs/providers.md` L20-44 "Environment variables are useful …
  anywhere Pi should not store the key"; table row `OpenRouter | OPENROUTER_API_KEY`.
- Do **not** use `--api-key` (puts the key in argv).
- Precedence between a stored `auth.json` credential and the env var is **unverified**
  (docs only say an `auth.json` entry's own `env` object beats the process env, providers.md
  L90). Pointing `PI_CODING_AGENT_DIR` at an empty dir removes the question.
- Not installed on this machine.

**OpenCode**
- Source: `packages/opencode/src/provider/provider.ts` L1633-1656
  https://github.com/anomalyco/opencode/blob/ecc4916b5a9608c30e6dd58a67f2137b594407ca/packages/opencode/src/provider/provider.ts#L1633-L1656
  — for every models.dev provider, the first set env var in `provider.env` becomes its key
  (`source: "env"`); **then** `auth.json` keys are merged after, so a key saved by `/connect`
  in `~/.local/share/opencode/auth.json` overrides the env key (inferred from merge order —
  later merge wins: **unverified** at runtime).
- models.dev `openrouter`: `env: ["OPENROUTER_API_KEY"]`, `api: https://openrouter.ai/api/v1`,
  `npm: @openrouter/ai-sdk-provider` (fetched https://models.dev/api.json, 390 models).
- CLI: `packages/web/src/content/docs/cli.mdx` L38/L83 `--model, -m` "provider/model";
  L687 `OPENCODE_CONFIG_CONTENT` "Inline json config content".
- The provider docs (https://opencode.ai/docs/providers/) only describe `/connect`; the env
  path is source-level.
- To keep it env-only, run with an isolated data dir so no `auth.json` exists
  (`XDG_DATA_HOME=<dir>`: **unverified** that OpenCode honours XDG on macOS).

**Goose**
- Repo moved: `block/goose` → `aaif-goose/goose`; docs at https://goose-docs.ai/.
- Source: `crates/goose/src/providers/openrouter_def.rs` L48-53 `get_secret("OPENROUTER_API_KEY")`,
  `get_param("OPENROUTER_HOST")` default `https://openrouter.ai`
  https://github.com/aaif-goose/goose/blob/067ca1e0b56ab692195edaa67437abd6904a7ed7/crates/goose/src/providers/openrouter_def.rs#L48-L53
- `crates/goose/src/config/base.rs` L912-915: `get_secret` "First check environment
  variables (convert to uppercase)", so env beats keyring/`secrets.yaml`; `get_param` L765-767
  likewise env first (so `GOOSE_PROVIDER`/`GOOSE_MODEL` env override config.yaml).
- Docs: `documentation/docs/getting-started/providers.md` L55 table row
  `OPENROUTER_API_KEY, OPENROUTER_HOST (optional), OPENROUTER_PARAMETERS (optional)`;
  `guides/environment-variables.md` L19-20 `GOOSE_PROVIDER`, `GOOSE_MODEL`;
  `guides/goose-cli-commands.md` L455-456 `--provider`/`--model` "overrides environment variable".
- `goose configure` would store the key in the macOS keychain (or `secrets.yaml` with keyring
  disabled), so do not run it for env-only use.

**Aider**
- Docs: https://aider.chat/docs/llms/openrouter.html — `export OPENROUTER_API_KEY=<key>`,
  `aider --model openrouter/<provider>/<model>`, `aider --list-models openrouter/`.
- Options: `aider/website/docs/config/options.md` L107-108 `--model` env `AIDER_MODEL`.
- Model ids go through litellm (`openrouter/` prefix); `aider/models.py` L263 uses a cached
  OpenRouter model DB for `openrouter/` models.
- Not installed on this machine. Repo activity: last commit 2026-05-22.

---

# Part 3b: OpenRouter key supplied only through env at launch

Agents covered: Kimi Code, Kilo CLI, Cline CLI, Crush, Qwen Code, omp. Researched 2026-10-07.

Method: shallow clones of each repo at the SHAs below, plus `--help` output of the binaries installed
on this machine (kimi 2.1.1, kilo 7.4.17, cline 3.0.46). No config or credential file was read. No
live request was sent with a real key.

| Repo | Commit read |
|---|---|
| MoonshotAI/kimi-code | `21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3` (2026-09-30) |
| Kilo-Org/kilocode | `113caf191616a10807cb7cb520d970dc197eb963` |
| cline/cline | `f832e8169d5a4a6031c285b9aefd33bed0d4a7c0` |
| charmbracelet/crush | `140e8cb9707faa6a68d87d0ecc3a85b9c65e25d5` (pins `charm.land/catwalk v0.52.49`) |
| charmbracelet/catwalk | `5155320e5ab736a658685eaa554b90918830e767` (main) |
| QwenLM/qwen-code | `a764fb9698e6e1152e0a8bc5808c30a765744152` |
| can1357/oh-my-pi | `12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723` |

## Summary table

| Agent | Env-only possible? | Key env var | Base URL | Model selection | Main caveat |
|---|---|---|---|---|---|
| Kimi Code (`kimi`) | **Yes**, with no config at all, through the `KIMI_MODEL_*` family | `KIMI_MODEL_API_KEY` (not `OPENROUTER_API_KEY`) | `KIMI_MODEL_BASE_URL=https://openrouter.ai/api/v1` + `KIMI_MODEL_PROVIDER_TYPE=openai` | `KIMI_MODEL_NAME=<openrouter model id>` | Shell `OPENROUTER_API_KEY` / `OPENAI_API_KEY` are **not** read unless a `config.toml` provider sets `api_key_env` |
| Kilo CLI (`kilo`, opencode fork) | **Yes** | `OPENROUTER_API_KEY` | none needed (models.dev `openrouter` provider) | `-m openrouter/<vendor>/<model>` (TUI and `kilo run`) | A key stored by `kilo auth login` for `openrouter` **overrides** the env value |
| Cline CLI (`cline` 3.x) | **Yes for headless runs, from source; not live-tested** | `OPENROUTER_API_KEY` | built in (`https://openrouter.ai/api/v1`) | `-P openrouter -m <vendor>/<model>` | The TUI sends you to onboarding when no provider settings are saved. `-k <key>` exists but puts the key in argv |
| Crush (`crush`) | **Yes** | `OPENROUTER_API_KEY` | built in (catwalk) | `crush run -m openrouter/<model>`; interactive TUI has no `--model` flag (model picker or `models.large` in `crush.json`) | Pasting a key in the picker stores it in config |
| Qwen Code (`qwen`) | **Yes**, through the generic OpenAI-compatible env | `OPENAI_API_KEY` (holding the OpenRouter key) | `OPENAI_BASE_URL=https://openrouter.ai/api/v1` | `OPENAI_MODEL=<vendor>/<model>` or `-m`; add `--auth-type openai` | A `modelProviders` entry in `~/.qwen/settings.json` outranks env. Docs also offer `OPENROUTER_API_KEY` + `OPENAI_BASE_URL`, but that env-only path is unverified (see below) |
| omp (oh-my-pi) | **Yes** | `OPENROUTER_API_KEY` | built in (catalog `openrouter`) | `--model openrouter/<vendor>/<model>` (or `--provider openrouter --model <pattern>`) | Stored OAuth or a key saved by `/login` **outranks** env. omp also auto-loads `.env` files |

## Kimi Code (MoonshotAI/kimi-code, `kimi` 2.1.1)

- Shell credential variables are **not** read automatically. "Credential variables such as `KIMI_API_KEY`,
  `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` are **not** read automatically from shell environment
  variables… The only exceptions are the `KIMI_MODEL_*` family and a provider's `api_key_env` field"
  ([docs/en/configuration/env-vars.md#L5-L8](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/env-vars.md#L5-L8)).
- **Route A: env only, no config.** When `KIMI_MODEL_NAME` is set, the CLI builds a temporary provider
  in memory and writes nothing back. These variables beat `default_model`, but `-m <alias>` still
  wins
  ([env-vars.md#L104-L132](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/env-vars.md#L104-L132);
  code: [`envOverlay.ts#L89`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/agent-core-v2/src/app/kosongConfig/envOverlay.ts#L89),
  [`configSection.ts#L69`](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/packages/agent-core-v2/src/app/kosongConfig/configSection.ts#L69)):
  ```sh
  KIMI_MODEL_PROVIDER_TYPE=openai \
  KIMI_MODEL_BASE_URL=https://openrouter.ai/api/v1 \
  KIMI_MODEL_API_KEY="$OR_KEY" \
  KIMI_MODEL_NAME=moonshotai/kimi-k2 \
  KIMI_MODEL_MAX_CONTEXT_SIZE=262144 \
  kimi
  ```
  Optional variables: `KIMI_MODEL_CAPABILITIES` (default `image_in,thinking`, which may be wrong for a
  given OpenRouter model), `KIMI_MODEL_REASONING_KEY` (`openai` only), `KIMI_MODEL_DISPLAY_NAME`.
- **Route B: a config entry that names the variable.** A provider in `config.toml` with
  `api_key_env = "OPENROUTER_API_KEY"` reads the key from that variable on every request.
  `api_key_env` excludes `api_key` and `oauth`, and an unset variable fails the request
  ([config-files.md#L121-L141](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/config-files.md#L121-L141)).
  This feature landed in 2.0.1
  ([changelog.md#L55](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/release-notes/changelog.md#L55)),
  so 2.1.1 has it. `kimi provider catalog` can import `openrouter` from models.dev as
  OpenAI-compatible ("guessed")
  ([kimi-command.md#L377](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/reference/kimi-command.md#L377)).
  Unverified: whether the catalog import can set `api_key_env` instead of asking for a key.
- Isolation: `KIMI_CODE_HOME` relocates the whole data root, including config and OAuth credentials
  ([env-vars.md#L15-L23](https://github.com/MoonshotAI/kimi-code/blob/21406fb4c805cc8c715e6d1f16ad3fb5f25f4fe3/docs/en/configuration/env-vars.md#L15-L23)).
- Unverified: a live OpenRouter run. The OpenAI-compatible request shape and OpenRouter's reasoning
  fields are not tested.

## Kilo CLI (Kilo-Org/kilocode, `kilo` 7.4.17, an opencode fork)

- Docs (CLI tab) say to set the key "as an environment variable": `export OPENROUTER_API_KEY=...`.
  The provider definition carries `"env": ["OPENROUTER_API_KEY"]`, and the default model is set as
  `"model": "openrouter/<vendor>/<model>"`
  ([kilo-docs/pages/ai-providers/openrouter.md#L32-L58](https://github.com/Kilo-Org/kilocode/blob/113caf191616a10807cb7cb520d970dc197eb963/packages/kilo-docs/pages/ai-providers/openrouter.md#L32-L58)).
- Code: the env load runs first, then the stored API keys (`auth.json` from `kilo auth`) are merged
  over it. **A stored openrouter key therefore wins over env**
  ([provider.ts#L1651-L1681](https://github.com/Kilo-Org/kilocode/blob/113caf191616a10807cb7cb520d970dc197eb963/packages/opencode/src/provider/provider.ts#L1651-L1681)).
  Launch with an isolated data dir (XDG dirs), or make sure no `openrouter` entry was saved with
  `kilo auth`. Unverified: the exact data-dir env name for Kilo; opencode uses `XDG_DATA_HOME`.
- Model: `kilo -m openrouter/<vendor>/<model>` (TUI) and `kilo run -m …` (local `--help`: "model to
  use in the format of provider/model"). `kilo models openrouter` lists the models.
- No base URL needed: the provider comes from models.dev.

## Cline CLI (cline/cline, `cline` 3.0.46)

- The SDK's built-in provider `openrouter` has `apiKeyEnv: ["OPENROUTER_API_KEY"]` and
  `defaults.baseUrl: "https://openrouter.ai/api/v1"`
  ([builtins.ts#L1053-L1062](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/sdk/packages/llms/src/providers/builtins.ts#L1053-L1062)).
- Key resolution: explicit `apiKey`, then `apiKeyResolver`, then each `apiKeyEnv` variable
  ([http.ts#L13-L33](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/sdk/packages/llms/src/providers/http.ts#L13-L33)).
  "the runtime no longer pre-flights credentials, so a missing key only matters when the API call
  actually runs"
  ([provider-auth.ts#L45-L52](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/apps/cli/src/utils/provider-auth.ts#L45-L52)).
  Connectors have an explicit env fallback with a test: "falls back to provider env vars when
  persisted settings have no api key"
  ([session-runtime.ts#L34-L45](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/apps/cli/src/connectors/session-runtime.ts#L34-L45)).
- Headless launch: `OPENROUTER_API_KEY=… cline -P openrouter -m anthropic/claude-sonnet-4.5 --json "<prompt>"`.
  Add `--data-dir <dir>` or `CLINE_DATA_DIR` to isolate state
  ([cli-reference.mdx#L236-L246](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/docs/cli/cli-reference.mdx#L236-L246)).
- Avoid `-k/--key <api-key>` ("API key override for this run",
  [cli-reference.mdx#L87](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/docs/cli/cli-reference.mdx#L87)):
  it puts the key in argv. Also avoid `cline auth --provider openrouter --apikey …`, which persists the
  key. The docs' own CI sample uses that command
  ([github-integration.mdx#L106](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/docs/cli/samples/github-integration.mdx#L106)).
- TUI (`-i`): with no persisted settings, `isProviderConfigured` returns false and the TUI opens
  onboarding
  ([tui/utils/provider-configured.ts](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/apps/cli/src/tui/utils/provider-configured.ts),
  [tui/root.tsx#L112](https://github.com/cline/cline/blob/f832e8169d5a4a6031c285b9aefd33bed0d4a7c0/apps/cli/src/tui/root.tsx#L112)).
  A key passed through config.apiKey counts, but that comes from `-k`. Persisting only the model
  (no key) should satisfy the check (`settings.model` counts), but that is unverified.
- **Unverified live:** the main (non-connector) CLI path picking up `OPENROUTER_API_KEY`. A sandboxed
  probe with a dummy key was planned but not run: the `cline` binary disappeared from
  `/opt/homebrew/bin` and `/opt/homebrew/lib/node_modules` during this session. I ran only `--help`
  and `--version`; something else (an updater or another session) removed it.

## Crush (charmbracelet/crush)

- README env table: `OPENROUTER_API_KEY` → OpenRouter
  ([README.md#L201-L214](https://github.com/charmbracelet/crush/blob/140e8cb9707faa6a68d87d0ecc3a85b9c65e25d5/README.md#L201-L214)).
- Catwalk provider definition: `"api_key": "$OPENROUTER_API_KEY"`,
  `"api_endpoint": "https://openrouter.ai/api/v1"`, default large model `anthropic/claude-sonnet-4.6`
  ([catwalk openrouter.json#L1-L8](https://github.com/charmbracelet/catwalk/blob/5155320e5ab736a658685eaa554b90918830e767/internal/providers/configs/openrouter.json#L1-L8)).
- Model: `crush run -m openrouter/<vendor>/<model>` ("Accepts 'model' or 'provider/model'") and
  `--small-model`
  ([internal/cmd/run.go#L170-L171](https://github.com/charmbracelet/crush/blob/140e8cb9707faa6a68d87d0ecc3a85b9c65e25d5/internal/cmd/run.go#L170-L171)).
  The interactive root command has no model flag
  ([root.go#L55-L64](https://github.com/charmbracelet/crush/blob/140e8cb9707faa6a68d87d0ecc3a85b9c65e25d5/internal/cmd/root.go#L55-L64)):
  use the Ctrl+L picker, or `models.large` in `crush.json`, which holds no key.
- Isolation: `CRUSH_GLOBAL_CONFIG`, `CRUSH_GLOBAL_DATA`, `-D/--data-dir` (seen in
  `internal/config` tests and root flags).
- Not installed locally, so there was no help check.

## Qwen Code (QwenLM/qwen-code)

- Env mapping for auth type `openai`: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, and `OPENAI_MODEL` or
  `QWEN_MODEL`
  ([packages/core/src/models/constants.ts#L74-L79](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/packages/core/src/models/constants.ts#L74-L79);
  docs table lists OpenRouter under this protocol,
  [auth.md#L233](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/docs/users/configuration/auth.md#L233)).
- **Env-only launch (verified from source):**
  `OPENAI_API_KEY="$OR_KEY" OPENAI_BASE_URL=https://openrouter.ai/api/v1 OPENAI_MODEL=qwen/qwen3-coder qwen --auth-type openai`.
  Flags: `-m/--model`
  ([top-level-options.ts#L185](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/packages/cli/src/config/top-level-options.ts#L185))
  and `--auth-type`
  ([#L444](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/packages/cli/src/config/top-level-options.ts#L444)).
  Avoid `--openai-api-key` (#L321), which puts the key in argv.
- Precedence: modelProvider selection, then CLI args, then env, then settings, then defaults
  ([modelConfigResolver.ts#L13-L19](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/packages/core/src/models/modelConfigResolver.ts#L13-L19)).
  A `modelProviders` entry in `~/.qwen/settings.json` therefore beats env. Isolate with `QWEN_HOME`
  (seen in `output-style-files.ts#L419`).
- There is a dedicated OpenRouter preset: `envKey: 'OPENROUTER_API_KEY'`,
  `baseUrl: https://openrouter.ai/api/v1`, plus HTTP-Referer and X-OpenRouter-Title headers
  ([providers/presets/openrouter.ts#L10-L40](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/packages/core/src/providers/presets/openrouter.ts#L10-L40)).
  The docs say "Use `/auth`, or set `OPENROUTER_API_KEY` and `OPENAI_BASE_URL=https://openrouter.ai/api/v1`"
  ([auth.md#L395](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/docs/users/configuration/auth.md#L395)).
  **Unverified:** `OPENROUTER_API_KEY` alone (with no `modelProviders` entry whose `envKey` is
  `OPENROUTER_API_KEY`) is used at runtime. The generic env mapping lists only `OPENAI_API_KEY`. The
  design doc says `/auth` browser OAuth "writes the exchanged API key into settings", so avoid it for
  env-only use
  ([openrouter-auth-and-models.md](https://github.com/QwenLM/qwen-code/blob/a764fb9698e6e1152e0a8bc5808c30a765744152/docs/design/openrouter-auth-and-models.md)).
- Not installed locally.

## omp = oh-my-pi (can1357/oh-my-pi)

- Identity verified: the package `@oh-my-pi/pi-coding-agent` declares `"bin": { "omp": "src/cli.ts" }`
  ([packages/coding-agent/package.json#L27-L28](https://github.com/can1357/oh-my-pi/blob/12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723/packages/coding-agent/package.json#L27-L28)).
  The README says it is a "Fork of Pi" (badlogic/pi-mono), site omp.sh.
- Key: `openrouter` → `OPENROUTER_API_KEY`
  ([docs/providers.md#L118](https://github.com/can1357/oh-my-pi/blob/12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723/docs/providers.md#L118);
  [docs/environment-variables.md#L75](https://github.com/can1357/oh-my-pi/blob/12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723/docs/environment-variables.md#L75)).
  The base URL is built in (catalog `openrouter`).
- Credential order: `--api-key` runtime override, then a `models.yml` key, then **stored OAuth**, then
  **a key saved by `/login`**, then the extension fallback, then **env / `.env`**, then other stored keys
  ([docs/providers.md#L51-L62](https://github.com/can1357/oh-my-pi/blob/12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723/docs/providers.md#L51-L62)).
  A previously saved OpenRouter login therefore beats the env variable. omp also auto-loads
  `<project>/.env`, `~/.omp/.env` and `~/.env`, but a non-empty process variable wins over them
  (#L213-L222).
- Model: `--model openrouter/<pattern>` or `--provider openrouter --model <pattern>`
  ([main.ts#L1394-L1395](https://github.com/can1357/oh-my-pi/blob/12ed7de7654d29c07d7c1c240d8e9f5cbfbe4723/packages/coding-agent/src/main.ts#L1394-L1395)).
  OpenRouter uses the Responses API by default; `PI_OPENROUTER_RESPONSES=0` switches to Chat
  Completions (env docs #L284).
- Isolation: `OMP_PROFILE`, `PI_CONFIG_DIR`, and `PI_CODING_AGENT_DIR` (default profile only); the
  auth store defaults to `~/.omp/agent/agent.db` (environment-variables.md #L533-L536).
- Not installed locally.
