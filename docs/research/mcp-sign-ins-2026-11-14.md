# MCP sign-ins across Claude homes: measurements (SB-358, SB-359)

Measured on the operator's MacBook on 2026-11-13 and 2026-11-14 (UTC), Claude Code as installed,
with `/usr/bin/security` and Python reading only booleans, key shapes and time differences; no
token value was printed or stored.

## What the homes held

- Every MCP entry in the 18 Switchboard homes that had a credential item was a **discovery stub**:
  `serverName`, `serverUrl`, `discoveryState` and an **empty** `accessToken`; no `refreshToken`,
  no `clientId`. 14 account homes had no item at all, and no group or shell home had one.
- The ordinary home held two real sign-ins: `plugin:figma:figma` (access and refresh token,
  client id and secret, about 1292 hours, 54 days, left) and `sentry` (about 216 hours left).
- So the 2026-11-13 reading (board `2c5b1279`) that Figma was "signed in in 9 of 18 isolated homes
  and absent in the ordinary one" counted stubs as sign-ins. The truth was the reverse.
- In the ordinary home Figma's sign-in sits under `plugin:figma:figma|a309…`, while the current
  plugin configuration's entry, in the ordinary home and in all ten homes that had one, is a stub
  under `plugin:figma:figma|3f78…`. Sentry likewise (`sentry|bc4c…` signed in, `sentry|800c…` a
  stub). The key hashes `{type, url, headers}` (Claude Code's `sd(name, config)`), so the plugin's
  configuration changed after the sign-in and Claude Code no longer finds it: the ordinary home
  reads "Needs authentication" for Figma too. The report names this `stale_key`.

## Whether a copied access token is used

A throwaway home (a fresh `CLAUDE_CONFIG_DIR` under the worktree) with one user-scope HTTP server
named `figma-probe` at Figma's MCP URL:

| Step | `claude mcp list` |
|---|---|
| No credential item | `figma-probe: … - ! Needs authentication` |
| Item with only `accessToken`, `expiresAt`, `scope`, `issuer`, `clientId`, `redirectUri`, `discoveryState`, under `figma-probe|<hash of {type:"http",url,headers:{}}>` | `figma-probe: … - ✔ Connected` |

The configuration differed from the one the token was granted under (another name and type), so
a token is not bound to the configuration's key. Claude Code left the entry as written. The item
and the folder were deleted afterwards (`security delete-generic-password`, exit 0; `find` then
44).

## Decision

Carry the access token only, by server URL, into the homes a launch starts, never the refresh
token or client secret: one home renews each grant, so a provider that rotates refresh tokens
never sees one used twice, and a copy cannot outlive its access token. A home's own sign-in is
never replaced. Figma's 54-day access token makes a copy last; a shorter one is carried again at
the next launch after the source renews.
