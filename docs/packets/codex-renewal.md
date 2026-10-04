# Packet SB-16 — renewal of inactive Codex OAuth accounts (research)

**State:** researched 2026-10-05, not built. **Board:** SB-16 (P3). Building it needs the operator's
go and an operator-assisted acceptance with two disposable-safe Codex accounts, because a wrong
grant signs a real account out.

## How the official Codex client renews (pinned `openai/codex@afb436d`)

Source: [`codex-rs/login/src/auth/manager.rs`](https://github.com/openai/codex/blob/afb436df8b70bb5bc57b86d9a3e829968988cd21/codex-rs/login/src/auth/manager.rs).

- **Endpoint and grant:** `POST https://auth.openai.com/oauth/token` (`REFRESH_TOKEN_URL`, overridable
  by `CODEX_REFRESH_TOKEN_URL_OVERRIDE`), JSON body, `client_id` `CLIENT_ID`
  (`oauth_client_id()`, overridable by its env var), `grant_type=refresh_token`
  (`request_chatgpt_token_refresh`, lines ~1626–1678).
- **Rotation:** the response may carry a new `refresh_token`, `access_token` and `id_token`
  (`RefreshResponse`); `persist_tokens` writes whichever came back and stamps `last_refresh`.
  A refresh token is single-use: reuse answers `refresh_token_reused` (`Exhausted`), and
  `refresh_token_expired` / `refresh_token_invalidated` are the other permanent failures; HTTP 401
  or `invalid_grant` with 400 are permanent too, everything else transient
  (`classify_refresh_token_failure`).
- **When:** proactively when the access token's JWT expiry is within 5 minutes
  (`CHATGPT_ACCESS_TOKEN_REFRESH_WINDOW_MINUTES`), or, without a readable expiry, when
  `last_refresh` is older than 8 days (`TOKEN_REFRESH_INTERVAL`, `should_refresh_proactively`).
- **Custody:** one refresh at a time per process (`refresh_lock` semaphore); before refreshing,
  a guarded reload of `auth.json` — it refreshes only if the stored auth is unchanged and reloads
  only when the account id matches (`reload_if_account_id_matches`, ~line 2389).

## What this means for Switchboard

The Claude renewal's rules carry over unchanged (PLAN-0.5 D-2, retro standing instructions 4 and
7): one renewer per lineage; the grant runs in its own task and records the successor keyed by the
spent token before returning; a rejected lineage is remembered by fingerprint; never the account
the ordinary Codex CLI is signed in to (its own refresher owns it — two renewers burn the token,
`refresh_token_reused`); never a row without a captured identity. Codex differences:

1. **Active-account exclusion reads `$CODEX_HOME/auth.json`** (the ordinary Codex home), not a
   Keychain item; an account whose `tokens.account_id` matches it is never renewed by Switchboard.
2. **Managed and isolated Codex homes** hold access-only copies today (SPEC §8); a renewal must not
   write into a home a running Codex process owns.
3. **Schedule:** the access token's JWT `exp` minus a margin, mirroring the 5-minute window, and
   the 8-day `last_refresh` ceiling when `exp` is unreadable.
4. **Permanent failures** map to *Sign in again* exactly as Claude's `invalid_grant` does.

## Build plan (when approved)

- `refresh::ensure_fresh` gains a Codex arm behind the same guards; a fixture token endpoint
  (`RefreshState::set_endpoint`) answers rotation, reuse, expiry, revocation, 5xx and timeouts.
- Tests (synthetic only): rotation stores the successor in every copy of the identity; a
  cancelled caller loses nothing; the ordinary Codex account is never renewed; reuse marks the
  lineage dead; a transient failure backs off; no token in logs or IPC.
- Acceptance (operator): two Codex accounts, one signed in to Codex; leave the other idle past its
  expiry; confirm Switchboard renewed it, Codex still works, and a later Codex refresh of the
  active account still succeeds.
