---
design: 2026-11-19-seat-relay
row: SB-575
decision: DEC-0037
status: design (docs only); keys and membership from Vesta (thread 14da4917, answered: directory only); the seat mailbox asked of passioncode-platform
---

# A seat for a teammate's computer through PassionCode.ai (SB-575, DEC-0037)

Entry point for the next agent: this file, then section 14 (what Vesta provides and what the
seat mailbox must guarantee) and section 16 (the build in waves). Decision: [DEC-0037](../../DECISIONS.md#dec-0037-a-teammates-computer-gets-a-seat-through-passioncodeai-never-through-the-operators-network).
Board row: [SB-575](../../evidence/backlog.md).

## 1. What was asked

The operator, 2026-11-19, in this repository's session, when a seat was about to be delegated to a
teammate's Mac under [DEC-0025](../../DECISIONS.md#dec-0025-a-subscription-is-delegated-to-a-teammates-machine-as-a-guest-under-a-lease):

> Проблема, что тем, кто приходит по tailscale, они видят вообще все, нету на уровне, нету никаких
> ограничений, поэтому мы не должны вообще это никому давать. Должны работать типа в рамках, типа
> вот правильного, как мы все настроены, смотри, чтоб ничего не наделали, ерунды никакой.

DEC-0025 needs the operator's machine to reach the teammate's machine over ssh: Remote Login on
there, a forced `switchboard ssh-serve` line in their `authorized_keys`, and a network both
machines share, in practice the operator's tailnet. A device on that tailnet reaches every other
device and service on it, and nothing in Switchboard narrows that. The operator forbids giving that
access to anyone. Earlier the same day the operator asked that the seat be the teammate's ordinary
console session, managed by the operator, chosen per project, and set up through Claude Code over
Switchboard's MCP tools. This design keeps everything DEC-0025 promised about the seat and replaces
only how it travels.

## 2. Goals and non-goals

Goals:

- G1. No inbound connection to either machine and no shared network: both Switchboards only connect
  outward, over HTTPS, to PassionCode.ai.
- G2. PassionCode.ai carries the messages and never reads a credential or a usage figure. The key
  directory and the carrier are two services: Vesta vouches for keys and membership and stores no
  message; the seat mailbox stores ciphertext only and vouches for nothing.
- G3. The seat on the teammate's computer behaves exactly as a DEC-0025 guest: one group, never
  moved on, lent, given to an agent or made the ordinary sign-in; a lease extended by the sender;
  switched off past it, credential kept 30 days, then removed; called back only by its sender.
- G4. The operator sees the seat as today: in the teammate device's machine group (DEC-0013), with
  its usage, *Call back* on its row, Needs attention when it goes quiet.
- G5. The teammate accepts it in the window or through MCP; Claude Code on the teammate's computer
  can do the whole setup except the browser consent.
- G6. Every state is typed. A read that failed is `unknown`, never an empty inbox or an absent seat.
- G7. The owner bounds what a guest may spend: a usage ceiling per window and a last-resort place
  in the teammate's rotation, both enforced on the receiving machine (section 9a).

Non-goals:

- Moving a request between machines (DEC-0020 still forbids it; only a sign-in moves).
- Stopping a teammate who reads their own Keychain. As in DEC-0025, the revocation against an
  uncooperative teammate is signing the subscription out at the provider.
- Replacing ssh between the operator's own machines (DEC-0025, DEC-0033 stay for those).

## 3. Parties

| Party | Holds | Talks to |
|---|---|---|
| Sender device | an administrator's Switchboard; the subscription; the organization's group | Vesta and the seat mailbox, outward only |
| Recipient device | a member's Switchboard; the guest seat once accepted | Vesta and the seat mailbox, outward only |
| Vesta (PassionCode.ai identity) | the per-person device registry with each device's public keys, signed membership (organization, person, role) | both, as a server; reaches neither; stores and relays no message (its ADR-0005 dropped relays) |
| The seat mailbox (passioncode-platform, recommended in section 14) | one inbox per registered device, ciphertext only, until acknowledged or expired; delivery receipts; the audit rows | both, as a server; reaches neither |
| The provider | the subscription's sign-in | each Switchboard, as today |

The administrator and the member are roles in the organization on PassionCode.ai
(`GET /v1/me/organizations` already answers `role` per organization, [CONTRACTS](../../CONTRACTS.md)
"PassionCode.ai organizations (SB-159)").

## 4. Device keys and registration

At `org connect` (CLI `org connect`, MCP, the window's PassionCode.ai panel), after the consent
that exists today, Switchboard makes one device identity for this data folder:

- An Ed25519 signing key pair and an X25519 encryption key pair, generated locally.
- The private keys go to the OS vault (`Store::put_secret`: Keychain, DPAPI, Secret Service), each
  as one item, never on disk in clear and never in a backup. `<data>/device.json` (0600) keeps the
  vault item ids, the public keys, the device id Vesta returned, the registration state and times.
- Registration in Vesta's per-person device registry (`/devices`, the one people's devices
  already use, Ed25519 or P-256 from a CSR): Switchboard writes this computer's X25519 encryption
  key and its Ed25519 signing key, bound to the person's device record, with the `switchboard` app
  token, proving possession of the signing key over a challenge; Vesta answers the `deviceId`.
  Administrators see their people's devices (`vesta_devices_list`). The exact fields and the scope
  the `switchboard` token needs for the write are Vesta's (open question V1).
- The device key is separate from the settings signing key (SB-272) and from ssh keys: one key, one
  purpose. The device record may carry the settings signer's public key, signed by the device key,
  so a later bundle can be tied to the device; nothing in this design depends on it.
- Rotation: `device rotate` makes new keys, registers them signed by the old signing key, and keeps
  the old ones until every seat thread has seen a message under the new ones. Loss of the vault
  items (a reinstall, a wiped Keychain) is a new device: the old one is revoked from the window or
  by an administrator, and its seats lapse by their lease (section 9).
- Disconnect (`org disconnect`) revokes the device at Vesta first; a failed revoke keeps the
  connection, as today's disconnect keeps the token rather than orphaning it.

Registration states, answered by `device status` and in `onboard --status` (they are about Vesta;
the seat mailbox has states of its own, section 11): `registered`,
`not_registered`, `scope_missing` (an older grant; connect again), `revoked`, `key_changed`
(Vesta lists keys for this device id other than ours), `unreachable` (Vesta did not answer; never
read as `not_registered`).

## 5. Pinning the other side's key

Vesta lists the devices of an organization's members with their public keys, and issues signed
membership: an `org-membership/2` statement (organization, person, role, `status`, revision,
expiry of at most 24 hours) carrying the extension `org.passioncode.ai/device` with `deviceId`,
`sign` (Ed25519) and `enc` (X25519), signed under the organization's key set, which chains to the
organization root. Each Switchboard pins that root at first use from Vesta
(`GET {issuer}/fabric/membership/v1/{orgId}/root`) and refuses a changed root until the person
confirms it. Each device presents its statement to the seat mailbox (`PUT /v1/mailbox/membership`)
and refreshes it before it expires; the other side reads it there
(`GET /v1/mailbox/devices/{deviceId}?org=`) and checks it offline under its own pinned root, so the
seat mailbox cannot invent a member, an administrator or a key. Envelopes carry no statement. A
statement from a server is still a claim about a person's device, so each side also pins:

- The recipient pins the sender's device signing key at the first offer it accepts. The accept
  step shows the key's short fingerprint (eight groups of four hex digits) and the organization and
  the sender device's label; the person may compare the fingerprint with the sender's window
  (*Organizations* → the device card), which shows the same string.
- The sender pins the recipient's encryption and signing keys when it makes the first offer to that
  device, after the same fingerprint is shown.
- A later list naming another key for a pinned device is `key_changed`: nothing is sent to it or
  accepted from it until the person confirms the new fingerprint (`device trust <id>`, the window,
  MCP with the person's words), as `machines trust` works for ssh keys today (SB-357).

## 6. The envelope

One JSON object per message, at most 64 KiB encoded (open question M3).

```json
{
  "v": 1,
  "id": "0c7e…",
  "org": "<organization id at Vesta>",
  "from": "<sender device id>",
  "to": "<recipient device id>",
  "kind": "seat.offer",
  "thread": "<random 128-bit seat thread id, hex>",
  "seq": 17,
  "sent_at": "2026-11-19T12:00:00Z",
  "expires_at": "2026-11-20T12:00:00Z",
  "enc": "<HPKE encapsulated key, base64url>",
  "ct": "<ciphertext, base64url>",
  "sig": "<Ed25519 signature, base64url>"
}
```

- `kind` is one of five (section 7). The seat mailbox needs `org`, `from`, `to`, `kind`, `thread`,
  `id` and `expires_at` in clear to route, authorize and expire; everything about the seat is in
  `ct`. No membership statement travels in an envelope: each side reads the other's from the
  mailbox (section 5).
- Encryption: HPKE (RFC 9180) base mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256,
  ChaCha20-Poly1305, to the recipient's X25519 key. The associated data is the canonical encoding
  (RFC 8785) of every header field except `ct` and `sig`, so a header changed in transit fails to
  open.
- Signature: Ed25519 by the sender's device key over the canonical encoding of every field except
  `sig`. The recipient checks the signature before it opens the ciphertext. The payload repeats
  `from`, `to`, `thread` and `kind`, and the recipient refuses a payload whose copies differ from
  the header: a signed envelope cannot be re-addressed.
- `seq` counts per (`from`, `to`) pair, strictly increasing, kept in `<data>/relay.json`; gaps are
  allowed (a message may expire unread).
- `id` is a random UUID, also the idempotency key at the seat mailbox: a submit whose answer was lost is sent
  again with the same bytes (section 11).

## 7. Kinds and who may send them

| Kind | From | To | Payload (inside `ct`) | Expires |
|---|---|---|---|---|
| `seat.offer` | an administrator's device | a member's device in the same organization | provider, the credential, a suggested group name, the organization, `lease_until`, the usage ceiling and the rotation place (section 9a), the sender's settings for the seat (rules the guest keeps, DEC-0025) | 7 days |
| `lease.extend` | the sender of that thread | its recipient | `lease_until`; a changed ceiling or rotation place, when the owner changed them | 1 hour |
| `usage.report` | the recipient of that thread | its sender only | seat state (`pending`, `active`, `off_lease`, `declined`, `removed`), usage windows as percentages and reset times, check time, the ceiling in force and the requests it refused since the last report, session counts by state | 1 hour |
| `seat.recall` | the sender of that thread | its recipient | a reason code | 7 days |
| `seat.surrender` | the recipient of that thread | its sender only | the live credential (the recipient's renewals rotate the refresh token), or `declined` or `removed` with no credential | 7 days |

- The seat mailbox enforces the From and To columns, from Vesta's signed membership and device
  list: a `seat.offer`, `lease.extend` or `seat.recall` only from a device whose person holds the
  administrator role in `org` to a device of a member of `org`; a
  `usage.report` or `seat.surrender` only along a thread that an offer opened, back to that offer's
  sender. Anything else is refused at submit.
- Each Switchboard checks the same rules again on receipt, and more: the thread's sender key is the
  pinned one; a `seat.surrender` is taken only on a thread this device opened and has not yet
  closed (an answer to a recall, a decline, or the teammate giving the seat back), and a second
  one on a closed thread is refused; a second `seat.offer` on a live thread is refused.
- The credential travels only in `seat.offer` and `seat.surrender`, so there is one live copy at a
  time, as with DEC-0025: the sender marks the account used elsewhere before it submits the offer,
  and the recipient marks its copy removed before it submits a surrender with the credential.

## 8. Replay and order

A recipient refuses, and logs `relay_refused {code}`:

- an `id` it already processed (kept for the longest expiry, 7 days, in `relay.json`);
- a `seq` at or below the highest seen for that (`from`, `to`) pair;
- `expires_at` in the past, with five minutes allowed for clock skew, or `sent_at` more than five
  minutes in the future;
- a `v` it does not know (no downgrade: a newer version is refused with `relay_version`, never read
  as the older one);
- a signature, a payload copy or a pinned key that does not match.

The seat mailbox refuses a second submit of an `id` with other bytes and answers the stored receipt for the
same bytes. A sender submits nothing newer to a device while an older envelope to it is
`outcome_unknown`: it sends that one again first, so a lost answer never puts a pair's `seq` out of
order.

## 9. Lease, state and the recipient's rules

The seat on the recipient is a DEC-0025 guest, with `sender_device` in place of `sender_key`:

- It joins the group the teammate chose when accepting (section 12), never `default`.
- The guest rules are the ones checked on every publication of the store today
  ([CONTRACTS](../../CONTRACTS.md#delegated-seats-and-the-own-sign-in-switch-sb-403-s2-to-s4-dec-0025-2026-11-17)):
  never moved on, handed over, given to an agent, lent, shared, marked elsewhere, made the ordinary
  sign-in or carried in a backup; only a message from the pinned sender device on its thread calls
  it back or removes it.
- The sender submits `lease.extend` every 15 minutes, to 48 hours ahead by default (1 to 168 at
  the offer for a relayed seat: the mailbox drops a thread with no envelope for 7 days, so no
  relayed lease may outlive its thread; ssh delegation between the operator's own machines keeps
  1 to 720). Each `lease.extend` and `usage.report` expires after 1 hour unread, the mailbox's
  maximum for them: a computer that was off reads the next one, never a stale one. Past the lease, once the recipient's owner has run 15 minutes past both the lease and
  its own start, the seat is switched off and its homes cleaned; its credential is kept; the next
  `lease.extend` turns it on again. Thirty days after the lease ended the credential is removed.
- The recipient submits `usage.report` every 5 minutes while the seat is `active`, and at once when
  its state changes (at most one a minute).
- Own sign-in off (`own-sign-in off`) is recommended at accept and offered in the same step, as in
  DEC-0025; it is not required.

## 9a. The seat's usage ceiling and its place in rotation

The owner sets, at delivery, how much of the subscription the seat may use, and the receiving
Switchboard enforces it. This is what keeps a shared subscription from being spent by one guest.

- The offer carries `ceiling`: a percentage per provider window, at least the 5-hour and the
  weekly window (80 % of the 5-hour window and 70 % of the week for the first teammate seat, approved by
  the operator and relayed in announcement de0ba93c, 2026-11-19), and `rotation`: `last_resort` or `normal`. The window and the CLI ask
  for both when offering; `last_resort` is the default.
- `last_resort`: in its group the seat is chosen, by hand or by automatic switching, only when no
  other account of that group has room under its own limits. The teammate's own subscriptions in
  that group always go first. A teammate's own sign-in outside Switchboard is never read (own
  sign-in off), so only accounts the group holds count.
- The receiving Switchboard treats the ceiling as the seat's limit: automatic switching moves off
  it at the ceiling, and the proxy refuses a request on it at or above the ceiling of any window
  with a clear local answer (status 429, error code `SEAT_CEILING`, the window, the ceiling, the
  reset time, and a sentence: "This seat is shared by your organization; it may use {n} % of its
  {window}, and that is reached. It resets {time}."). Nothing goes to the provider for that request.
- Unknown is not zero: the receiver checks the seat's usage before its first request and whenever
  its last check is older than 5 minutes when a request arrives; a seat whose usage could not be
  read is not used until a check works, and the refusal says so (`SEAT_USAGE_UNKNOWN`).
- `usage.report` carries every window's figure, the ceiling in force and the count of refused
  requests, so the owner's row shows the seat against its ceiling.
- The owner changes the ceiling or the rotation place on the seat's row; the change travels in the
  next `lease.extend` and the report that follows confirms it. A lower ceiling applies at once on
  receipt; nothing waits for the provider.
- An ssh-delegated seat between the operator's own machines may take the same ceiling later; it
  is not part of SB-575.

The program's path on the teammate's computer is never assumed: Switchboard may be installed under
`/Applications` or under `~/Applications` (or anywhere on Linux and Windows). The relay calls no
program on the other computer, so nothing in this design depends on where it is installed.

## 10. Polling, cadence and backoff

- Both sides fetch their inbox from the seat mailbox over HTTPS (`GET` with a cursor, section 14). Cadence: every 60
  seconds while this device has a live thread, an offer waiting either way, or a recall or
  surrender in flight; every 15 minutes otherwise (to see a new offer); and at once on *Check now*,
  `relay check`, or MCP.
- After a failed fetch: 60 s, 2 min, 4 min, 8 min, 15 min, then every 15 min, each with up to 20 %
  jitter; `Retry-After` from the mailbox wins when longer. The first fetch that works resets it.
- Fetching acknowledges nothing: a message is acknowledged after it was applied and the store
  published, so a crash between fetch and apply fetches it again and the `id` check makes the
  second apply a no-op.
- The fetch runs in a spawned task outside the owner transaction (as the Vesta read does today),
  never once the drain began, and only while PassionCode.ai is connected. The device takes the next
  revision of its own statement from Vesta and presents it to the mailbox 15 minutes before the one
  in force expires (statements live at most 24 hours; the mailbox takes 12 presentations an hour);
  the other side's statement is read from the mailbox at a thread's first envelope, before each
  offer or recall, and when a check fails. The idle budget gains one
  HTTPS request every 15 minutes on a computer with a registered device and no thread.
- Long-poll or server-sent events would cut the minute's delay; this design works without them
  (open question M5).

## 11. Failure and unknown states

Every state below is typed; the CLI, MCP and the window show the same words.

Sender, per seat thread: `offered` (submitted, no answer), `active`, `declined`, `quiet` (no
`usage.report` for 15 minutes), `unreachable` (the seat mailbox did not answer the last three fetches),
`recall_sent`, `recalled` (the surrender with its credential arrived and the provider check passed),
`expired` (the offer passed its expiry with no answer; the recipient never accepts an offer past
its expiry and then drops the credential it held, so the account comes back here ten minutes after
the expiry, past the clock tolerance, and only after a fetch that worked found no report for the
thread), `outcome_unknown` (a submit whose answer
was lost: the same bytes are sent again until the mailbox answers; until then the account stays marked
elsewhere and Needs attention names it).

Recipient, per guest seat: `pending` (an offer waits for the person), `active`, `off_lease`,
`removed`, `surrender_sent`, `outcome_unknown`. An accepted seat is used only once the mailbox took its
first `usage.report` (`active`) before the offer's expiry. An accept that did not reach the mailbox in
time is dropped; one whose answer was lost stays off and is never used until a `lease.extend` on
its thread arrives, and without one it lapses with the offer's lease and its credential is removed,
so the sender's copy stays the only one in use.

Device: the registration states of section 4; membership: `valid`, `expired`, `ended`,
`not_member`, `unknown` (Vesta or the mailbox not read). The mailbox's refusals are kept as typed
codes, never as an empty answer (ADR-0011 of passioncode-platform: `unknown_device`, `stale`,
`replayed`, `invalid`, `sender_not_member`, `recipient_not_member`,
`recipient_membership_expired`, `not_administrator`, `thread_exists`, `thread_unknown`,
`thread_direction`, `envelope_expiry`, `envelope_signature`, `envelope_conflict`, `inbox_full`,
`rate_limited`, `payload_too_large`). A `seat.recall` refused because the recipient's statement
expired (the computer is off) is `recall_waiting` and tried again every 15 minutes; the lease is
no longer extended, so the seat there lapses by itself.

An administrator who loses the role can no longer extend or recall the seats it offered (the
mailbox checks the role at every submit); those seats lapse by their lease, and the teammate can
still give them back. Another administrator offers a new seat on a new thread. Inbox: `ok`, `empty` only after a fetch that worked,
`unknown` after one that failed. A failed fetch never clears a seat, never ends a lease early and
never reads as "no offers".

Needs attention gains `relay_quiet`, `relay_unreachable`, `relay_outcome_unknown` and
`device_key_changed`, each with the act that fixes it. `agent-health/1` counts them in the check
`machines`.

## 12. What each person sees and does

The operator (sender):

- The teammate's device appears as a machine group (DEC-0013), named by the device label, marked
  *via PassionCode.ai*. The account offered or delegated to it sits there, as accounts on another
  machine do today, with its usage from the last `usage.report` and its age.
- *Offer to a teammate's computer* (the account's ⋯ menu, and a new tool) lists the member devices
  of the account's organization, with fingerprints, and takes the lease.
- *Call back* on its row submits `seat.recall`; the row says `recall_sent` until the surrender
  arrives and the provider check passes, then the account is home.

The teammate (recipient):

- Once connected to PassionCode.ai and registered, the window's *Organizations* page shows offers
  waiting: the organization, the sender device and its fingerprint, the provider, the lease.
- *Accept* asks which group serves the seat: one of the teammate's groups bound to that
  organization, or a new group with folders the teammate picks. This is the per-project choice the
  operator asked for: the seat serves only that group's folders.
- A waiting offer's credential is held in the vault, never shown, until the person accepts or
  declines or the offer expires; past its expiry it can no longer be accepted and is dropped.
- *Decline* submits `seat.surrender` with `declined` and no credential.
- *Give back* on a guest seat submits `seat.surrender` with the credential.

Claude Code over MCP, on the teammate's computer, can do every step but the browser consent: read
`switchboard_onboarding`, run the connect (the person consents in the browser), register the
device, list offers, accept into a group the person named, turn own sign-in off. Accepting lands a
credential on the person's computer, so it is done on the person's own words, recorded as the
decision (DEC-0019's parity rule: every window act has a tool).

New operations, named for the search and execute catalogue (SB-478) and served as MCP tools until it
lands: `device.status`, `device.register`, `device.trust`, `device.rotate`, `seat.offer`,
`seat.offers.list`, `seat.offers.accept`, `seat.offers.decline`, `seat.surrender`, `relay.check`.
`switchboard_account_recall` calls back a relayed seat as it calls back an ssh-delegated one.

## 13. Threat model (classes)

| Class | What it could do | What stops or bounds it |
|---|---|---|
| The seat mailbox is read or run by someone else | see who offered what kind to whom and when; drop or delay messages | it holds no key and no membership authority; payloads are end-to-end encrypted and signed; a dropped `lease.extend` lapses the seat (fails closed); a dropped recall is resent and the provider sign-out remains |
| The directory (Vesta) inserts or swaps a device key | address an offer to a device it controls, or forge a sender | keys and messages come from two services, so one alone cannot both swap a key and deliver; first-use pinning with fingerprints shown to both people; a changed key stops all traffic until a person confirms it |
| Replay of an old message | re-extend a lease, re-offer a seat, re-run a recall | `id`, per-pair `seq`, expiry, payload copies of the header (section 8) |
| Re-addressing a signed envelope | deliver one person's offer to another device | the recipient id is inside the signature and inside the payload |
| A member's device is compromised | read a seat's credential once its offer was delivered | the same as DEC-0025: the provider sign-out ends every copy; only seats offered to that device are exposed |
| An administrator's device is compromised | offer the organization's seats, recall them | Vesta's signed membership limits it to that organization; the mailbox's audit rows show every offer; revoking the device at Vesta stops it |
| A member leaves the organization | keep using a seat | the membership statement expires and is not renewed, so the mailbox and the receiving Switchboard refuse the thread's messages; the lease lapses within its length; the credential is removed 30 days later; the provider sign-out ends it at once |
| A guest spends the shared subscription | use the organization's seat up | the owner's ceiling per window, enforced before any request reaches the provider; `last_resort` keeps the teammate's own accounts first; the owner sees the figures and lowers the ceiling or calls the seat back |
| Flooding a mailbox | exhaust a device's inbox or the owner's time | the mailbox rate limits per sender device and per organization; the recipient caps envelopes per fetch and their size |
| A version downgrade | make a side accept a weaker format | `v` is signed; an unknown version is refused, never read as an older one |
| Clock skew | expire messages early or late | five minutes of tolerance; order comes from `seq`, not the clock |
| Secrets in logs or Git | leak a credential or a key | logs carry codes, ids and counts only; a planted-token test covers the relay's log lines and errors |

What this design does not change: anyone who can read a person's Keychain on their own computer can
read the seat on it. The operator's network is never part of the path.

## 14. What Vesta provides and what the seat mailbox must guarantee

### 14.1 Vesta: key directory and membership attestation

Asked on 2026-11-19 (project `passioncode-vesta`, thread 14da4917, message 446daca9); Vesta answered
(message d6656231): it will not store or relay envelopes (its ADR-0005 dropped relays, and its
vision rules out a subscription pool). It provides, and this design takes (option a, approved by
the operator, relayed in announcement de0ba93c):

1. **Per-person device registry.** Devices are already registered with Ed25519 or P-256 keys from a
   CSR; administrators see their people's devices (`/devices`, `vesta_devices_list`).
2. **Switchboard's keys on the person's device.** A new X25519 public key, and Switchboard's Ed25519
   key when it differs from the device's, bound to the person's computer, written with the
   `switchboard` app token.
3. **Signed membership.** A statement of organization, person, role and device keys with an expiry,
   signed by Vesta, which both Switchboards and the seat mailbox check without asking Vesta again.

Vesta never sees a credential, a private key, a usage figure, a group or folder name, an account
label, the provider account's e-mail or a session's content, and holds no message.

### 14.2 The seat mailbox: candidates

Envelopes travel through a store-and-forward channel that is not Vesta. Two candidates:

| | passioncode-platform (the hosting backend) | The Fabric board hub |
|---|---|---|
| Who can reach it | any PassionCode.ai member, from their own computer, with their own identity | Projects of the operator's estate; a teammate's computer would have to be admitted to that estate's board |
| Unit of authorization | can be the device and the organization role, checked against Vesta's signed membership | the Project and its grants; no organization role, no device |
| What a participant sees | only its own inbox | the estate's directory of Projects and capabilities (the directory listing shows every Project) |
| Retention | can be built to delete on acknowledgement or expiry | messages stay in their thread as history; requests expire after 7 days but are kept |
| Size | set by this ask (64 KiB proposed) | 64 KiB a message |
| Receipts | to be built (stored, fetched, acknowledged) | submit receipts; read marks per Project |
| Fit with DEC-0037 | an organization service outside the operator's estate | would bring a teammate's computer into the operator's estate, the kind of access DEC-0037 refuses |

**Chosen: passioncode-platform.** It took the ask (thread 54858223, messages 870bf008 and
9e65a34e) as its ADR-0011 "The device mailbox" (proposed, PLAT-182, passioncode-ai/passioncode-platform
PR #248, commit e8c7c85): a device signature per request (`PC-Device`, `PC-Timestamp`, `PC-Nonce`,
`PC-Signature` over `pc-device-request/1`, 300 s window, nonces kept 10 minutes), membership from
Vesta's `org-membership/2` with the device extension, API v1 under `/v1/mailbox/` (`membership`,
`devices/{id}`, `envelopes`, `inbox`, `acks`, `receipts`, `audit`), envelope `sig` checked over the
canonical header, `ct` and `enc` never parsed, 64 KiB an envelope, 256 per inbox, 30 submits and 6
fetches a minute per device, expiry at most 7 days (offer, recall, surrender) or 1 hour (extend,
report), envelopes kept at most 7 days, audit 90 days, fetch only in v1 with long-poll planned.
Why it was the recommendation: The board hub is the operator's agents' channel inside
one estate; admitting a teammate's device there repeats, at a smaller scale, the exposure DEC-0037
removed, and its units (Projects, threads kept as history) fit neither device-level authorization
nor deletion at acknowledgement. The hosting backend serves organizations and their members by
design and can be held to the guarantees below. The ask is filed on the board: project
`passioncode-platform` (fd8b81cf), capability `change.request`, thread 54858223, message 2216bf60.

### 14.3 What the seat mailbox must guarantee

1. **Authorization from Vesta's statements.** A request is signed by the device's Ed25519 key from
   Vesta's directory. A `seat.offer`, `lease.extend` or `seat.recall` is taken only from a device
   whose person holds the administrator role in the envelope's organization, to a device of a
   member of the same organization; a `usage.report` or `seat.surrender` only along a thread an offer
   opened, back to that offer's sender. A membership that ended or expired refuses the thread. Each
   refusal is a typed code.
2. **No plaintext.** It stores and forwards the envelope as given; it never holds a decryption key
   and never needs one. Nothing in its logs or metrics carries `ct`.
3. **Size and rate limits.** At most 64 KiB an envelope, a bounded inbox per device (for example 256
   envelopes), rate limits per sender device and per organization, answered with `Retry-After`.
4. **Retention.** An envelope is deleted when its recipient acknowledges it or when it expires, and
   never kept longer than 7 days; nothing of it is kept afterwards but the audit row.
5. **Idempotent submit.** By envelope `id`: the same bytes answer the stored receipt, other bytes
   under the same `id` are refused.
6. **Delivery receipts.** The sender reads, per envelope, `stored`, `fetched` and `acknowledged`
   with times, or `expired`; a receipt it cannot read is `unknown`, never `delivered`.
7. **Audit rows.** One row per submit, fetch, acknowledgement, expiry and refusal (kind, sender and
   recipient device, thread, time, size, outcome; never payload), readable by the organization's
   administrators, and the rows about a device by its person.
8. **Outbound only.** Both sides reach it over HTTPS; it never connects to a member's computer.

## 15. Open questions

For Vesta:

- V1. The exact fields of the X25519 and Ed25519 write, and the scope the `switchboard` app token
  needs for it; whether one consent covers it.
- V2. The `org.passioncode.ai/device` extension on `org-membership/2` with `{deviceId, sign, enc}`,
  addressed to `https://api.passioncode.ai`: the one thing the seat mailbox asks of Vesta (ADR-0011
  D-2), and what this design needs (confirmed to Vesta on thread 14da4917).
- V3. Does a statement turn `ended` as soon as a member leaves the organization, and is the device
  revoked at the same time?
- V4. Can the device directory be an append-only, verifiable log, so a swapped key is detectable
  beyond first-use pinning?

For the seat mailbox (answered by ADR-0011; the two changes this design asks are in the reply on
thread 54858223):

- M1 to M5 are answered (D-1 to D-7). Asked changes: (a) a member device may read, through
  `GET /v1/mailbox/devices/{deviceId}?org=`, the statement of the device that opened a thread to it,
  not only a sender reading its recipient, since envelopes carry no statement and the recipient
  needs the sender's `sign` and `enc` keys; (b) a `seat.recall` to a recipient whose statement
  expired but did not end is stored and waits in its inbox (within the 7-day expiry), so a call
  back reaches a computer that was off.

## 16. Migration from ssh delegation

- Today no seat is delegated over ssh to a computer outside the operator's own: the live run with a
  teammate's computer (SB-403 S5) never ran, and the request that would have started it is
  withdrawn by DEC-0037.
- This change (docs only): DEC-0037 recorded, DEC-0025's status narrowed, the instructions that
  told a teammate to join a network, turn on Remote Login or add an `authorized_keys` line removed
  from [ORGANIZATION.md](../../ORGANIZATION.md#a-teammates-computer), its Russian twin and
  [CLI.md](../../CLI.md); the MCP description of `switchboard_account_send` says the other machine
  is one of the operator's own.
- The build (SB-575 waves):
  - W1 device identity at connect: keys written to Vesta's registry, membership statements,
    registration states, pinning and fingerprints (needs V1, V2).
  - W2 the envelope library: seal, open, sign, check, replay state, with its tests (needs no
    service).
  - W3 the relay client against the seat mailbox (needs section 14.3 and M1 to M3), the cadence,
    receipts and the failure states.
  - W4 the seat flow: offer, accept into a group, extend, report, recall, surrender, decline,
    expiry, on the guest rules that exist today; the usage ceiling and the last-resort place
    (section 9a).
  - W5 surfaces: the window's offer and accept, the machine group *via PassionCode.ai*, Needs
    attention, MCP tools, CLI, scenarios (SCN-102 rewritten for the relay, a new one for the
    teammate's accept), strings checked with Copylot, CHANGELOG.
  - W6 the box *Delegate to a teammate* in the move dialogs becomes *Delegate to my other machine*
    (ssh, DEC-0025) beside *Offer to a teammate's computer* (relay); `accounts send --delegate` keeps
    working for the operator's own machines.
- A teammate who already turned on Remote Login or added an `authorized_keys` line for this purpose
  removes that line and turns Remote Login off; nothing in Switchboard depends on it.

## 17. Tests

- Envelope: a sealed message opens; a changed header field, a changed ciphertext, a wrong recipient
  key, a wrong signer, a payload whose copies differ from the header, an unknown `v` each fail with
  their own code.
- Replay: a repeated `id`, a lower or equal `seq`, an expired message and one dated in the future
  are refused; `seq` and seen ids survive an owner restart.
- Keys: private keys are written only through the vault (a test that greps the data folder after
  registration finds no key bytes); a changed key in the device list stops sending and receiving
  until trusted.
- Flow against an in-process fake Vesta (directory and statements) and a fake seat mailbox that
  answers ADR-0011's routes and refusals (signed requests, membership presentation, 24-hour
  statements, 1-hour extend and report expiry, thread drop after 7 days): offer, accept into a chosen group, usage report, lease
  extended, recall, surrender with the credential, provider check, account home; decline; an
  unanswered offer expires and the account comes home; a lease that lapses switches the seat off
  and the next extension turns it on; 30 days later the credential is removed.
- Failures: the mailbox unreachable reads `unreachable`, never an empty inbox; Vesta unreachable
  reads membership `unknown`, and nothing is offered or accepted on it; a submit whose answer is
  lost is resent with the same bytes and ends `recalled` once; a crash between fetch and apply
  applies the message once.
- Authorization on receipt: a `usage.report` from a device that is not the thread's recipient, a
  `seat.recall` from a device that is not the thread's sender, a second `seat.offer` on a live
  thread are refused.
- The fake mailbox asserts that no stored row contains the planted credential, label, folder or
  e-mail; the relay's log lines pass the planted-identifier test of [ANALYTICS.md](../../ANALYTICS.md).
- Ceiling: a request at or above the ceiling of any window is refused locally with `SEAT_CEILING`
  and never reaches the provider (a fake provider counts calls); a seat with unknown usage is not
  used; a `last_resort` seat is chosen only when every other account of its group is out of room;
  a lower ceiling from `lease.extend` applies at once; `usage.report` carries the ceiling and the
  refused count.
- Guest rules: the existing `delegation::tests` run over a relayed guest as over an ssh-delegated one.
