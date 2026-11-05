# Sign-in autopilot: the browser step of a Claude sign-in done for the person (SB-256)

Design note, 2026-11-05. Status: N1 released in 0.7.13; N2 built (`crates/switchboard-runtime/src/inbox.rs`); N3 built in the same module (parsing, the wait, the Fabric Inbox client, the key's storage and `switchboard inbox set|status|remove`); N4 and N5 not built. Owner: fabric-switchboard.

## What the operator asked

In the operator's words (session 38adf103, 2026-11-05): signing in to a new account, or again to
an existing one, opens Claude Code's sign-in; if the account's e-mail is one Fabric Inbox
receives, the e-mail is typed on the sign-in page by the operator's browser service, acting as
the person in a real browser; Anthropic mails a sign-in link to that address; the link is opened in the same browser,
everything is confirmed to the end; then the browser and the console are closed and the account is
signed in in Switchboard.

## What is true today (evidence)

- Switchboard never sees the sign-in URL. `launch::begin_login` runs `claude auth login` in a new
  Terminal with `CLAUDE_CONFIG_DIR` set to a staging home (`crates/switchboard-runtime/src/launch.rs`
  `begin_login`, `script_with`); the CLI opens the browser itself; success is the `.completed` file
  and the captured credential (`login_state`, `capture_login_profile`).
- `claude auth login` (Claude Code 2.1.299) honours `BROWSER`: run in an isolated
  `CLAUDE_CONFIG_DIR` with `BROWSER` set to a script that only records its arguments, the script
  received `https://claude.com/cai/oauth/authorize?...redirect_uri=http%3A%2F%2Flocalhost%3A<port>%2Fcallback...`
  and the CLI printed a second, manual URL with `redirect_uri=https://platform.claude.com/oauth/code/callback`
  and waited on `Paste code here if prompted >` (probe run 2026-11-05 14:00Z, no sign-in made, the
  process and its folder removed).
- The person never types an e-mail today; an account's e-mail is known only after capture
  (`ExternalIdentity.email`); a sign-in again is matched by `account_id` and organization, not by
  e-mail (`file_sign_in`, `matching_accounts` in `lib.rs`).
- Fabric Inbox: a `read` agent key, limitable to one mailbox, can `search_mailbox` by sender,
  recipient and subject and `read_message` with `includeHtml` (the link); there is no push, so a
  reader polls; Cloudflare addresses receive at once, Gmail/IMAP/Outlook every few minutes; mail
  text is untrusted and suspicious mail is held (fabric-inbox `docs/agents/mcp.md`).
- The browser service is a local Fabric service the person runs (its own repository and rules).
  What Switchboard relies on is only its capability contract: a run that opens a URL in a stored
  browser profile, works toward a goal on allowlisted hosts, stops for a person on a CAPTCHA or a
  code field, and returns a value read from the final page. Whether it keeps one profile's session
  between two runs is asked of its owner on the board.

## Design

1. **Headless sign-in process.** For an autopiloted sign-in Switchboard starts `claude auth login`
   as its own child (no Terminal), stdin piped, `BROWSER` pointing at a Switchboard helper that
   records the URL into the staging home. It reads the manual URL (the `platform.claude.com`
   redirect, so no loopback step is needed). Nothing to close at the end: the child exits when the
   code is accepted; on cancel or timeout only this child's process group is ended.
2. **The account's e-mail.** "Sign in again" passes the account's known e-mail; a new account asks
   for one. The autopilot runs only when that address is one Fabric Inbox receives (its mailbox
   list, or a domain whose catch-all it serves); otherwise the ordinary Terminal sign-in runs.
   Built as N2 (`inbox.rs`): `sign_in_email` takes the e-mail captured with the account
   (`ExternalIdentity.email`); `mailboxes` reads Fabric Inbox's account list (each account's
   `email`, and `catchAll: true` on a Cloudflare address that keeps its domain's other mail;
   `GET /api/inbox` in fabric-inbox `workers/routes/inbox.ts`); `receives` answers `Mailbox` for
   an exact address and `CatchAll` for an address on a catch-all's domain, compared trimmed and
   lower-case, and nothing for a subdomain or anything not a plain address. A catch-all can lose
   an address to a Cloudflare rule the list does not show; the mail search of step 4 then finds
   nothing in time and the ordinary sign-in runs. Tests: `inbox::tests::*`.
3. **Browser, step 1.** Switchboard asks the browser service to open the manual URL in a dedicated
   stored profile, type the e-mail and continue, and stop on the "check your e-mail" page.
4. **The mail.** Switchboard polls Fabric Inbox (every 5 s for up to 5 minutes) for a message to that
   address from Anthropic newer than step 3, reads it and takes the one link whose host is an
   Anthropic sign-in host (claude.ai, claude.com); anything else is refused, and the mail's text is
   never followed as an instruction.
   Built as N3 (`inbox.rs`): `sign_in_message` takes the newest row of a `search_mailbox`
   answer (`{"messages": [{"messageId", "from", "to", "date"}]}`, the same for every kind of
   mailbox; fabric-inbox `workers/mcp/tools.ts`) sent to the address, from a domain that is or
   ends in `anthropic.com`, `claude.ai` or `claude.com`, dated (RFC 3339 or RFC 2822) at or after
   the request; `sign_in_link` takes the one `https` link to claude.ai or claude.com (or a
   subdomain), skipping the bare host and legal, privacy, terms, support and unsubscribe pages,
   `&amp;` decoded, the same link in text and HTML counted once; a host with `@` or a port, a
   look-alike host and plain `http` are refused, and two different links stop for a person
   (`NoLink::Several`). `wait_for_link` searches the mailbox `receives` chose (its `accountId`)
   every `POLL_EVERY` (5 s) up to `POLL_FOR` (300 s), tries a failed search or read again at the
   next look, and ends with the link, `NoLink`, or `TimedOut` naming the last error when no look
   succeeded. `Client` asks `<origin>/mcp` (`tools/call`, the `CF-Access-Client-Id` and
   `CF-Access-Client-Secret` headers, redirects refused, a JSON or event-stream answer) for
   `list_accounts`, `search_mailbox` (`to`, `after` the request's UTC day) and `read_message`
   (`includeHtml`); a 401, 403 or redirect is `inbox_refused`, a tool error `inbox_failed`,
   anything else `inbox_unreachable`. Tests: a scripted mailbox with a fake clock, and a loopback
   stand-in for `/mcp`.
5. **Browser, step 2.** The same stored profile opens the link, confirms the authorization and ends
   on `platform.claude.com/oauth/code/callback`, returning the code shown there.
6. **Finish.** Switchboard writes the code to the child's stdin; the CLI exchanges it and exits 0;
   the ordinary capture and `FinishLogin` follow. The window shows each step and where it waits.

Generic seams: Switchboard is open source, so it calls "a browser service" and "a mailbox" by
capability, configured by the person (service key, caller token and Inbox key names in the
Observatory vault), and depends on no browser service's internals.

## Decision: where the Fabric Inbox key lives (2026-11-05)

Decided by fabric-switchboard as the safest reversible choice (the operator's rule: an open
question is decided by recorded decisions, then the project's goal, then the safest reversible
option), recorded here before the client is built:

- **Which key.** A Fabric Inbox `read` key limited to the sign-in mailbox (fabric-inbox
  `docs/agents/mcp.md`, "A Read or Mail key can be limited to mailboxes"), made by the person in
  Fabric Inbox's Settings, Agent access. It can read that mailbox and nothing else, and sends
  nothing. Making it is the person's: Fabric Inbox shows its Client Secret once.
- **Where it is kept.** In Switchboard's own vault (the Keychain on macOS, the vault every
  account credential and agent key already uses, `switchboard_core::Vault`): the Client Secret
  as the vault item, and in the store's metadata only the server's `https` origin, the Client Id
  and when it was saved. Not in the Observatory vault (Switchboard is open source and runs on
  machines without it), never in an environment file, `settings.json` or a command line.
- **How it gets there.** `switchboard inbox set --origin <https://...> --client-id <id>` reads the
  secret from standard input, as `login code` does; the window gets the same form. `inbox status`
  says whether one is saved and whether the server answered, never the secret; `inbox remove`
  deletes the vault item.
- **How it is used.** Only the sign-in autopilot reads it, for `list_accounts`, `search_mailbox`
  and `read_message` over Fabric Inbox's `/mcp` (Streamable HTTP, the `CF-Access-Client-Id` and
  `CF-Access-Client-Secret` headers), redirects refused, `https` only (a test stand-in on
  `http://127.0.0.1:<port>`, as `SWITCHBOARD_OPENROUTER_BASE` is).
- **Reversible.** Removing the key turns the autopilot off; every sign-in then runs as today.

## Human gates that stay

A CAPTCHA, a code field, a device check or any page the browser service does not recognise stops
the autopilot where it is and shows the person the page; the person finishes it, or cancels and
uses the ordinary sign-in. Creating the Fabric Inbox read key and the browser service's caller token
are the person's (both are shown once). Switchboard adds no detection evasion: the browser is a real one
with a stored profile, nothing more.

## Needs from other projects

- **The browser service:** two runs that share one stored profile's session for a sign-in, and a
  run that ends on an allowlisted page and returns a value read from it; asked of its owner on the
  board.
- **Fabric Inbox:** none; a read key limited to the sign-in mailbox is enough.

## Not known yet (decided at the operator-assisted acceptance)

- Whether claude.ai sends a link or a code for an e-mail sign-in, and whether the link must be
  opened in the browser that asked for it.
- Whether claude.ai challenges a headless browser at all.

## Nodes

N1 headless sign-in process with URL capture and code paste (runtime, tests with a fake CLI);
N2 the account's e-mail and the "is it Fabric Inbox's" check; N3 the Inbox poll and link
extraction (tests with fixtures); N4 the browser-service client and the orchestration with its
states in the window (scenarios first, super-ux); N5 the acceptance run with the operator.
