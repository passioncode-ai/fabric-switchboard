---
design: 2026-11-19-seat-relay
row: SB-575
decision: DEC-0037
status: design (docs only); the build waits on PassionCode.ai (Vesta), board thread 14da4917
---

# A seat for a teammate's computer through PassionCode.ai (SB-575, DEC-0037)

Entry point for the next agent: this file, then section 14 (what Vesta must provide) and
section 16 (the build in waves). Decision: [DEC-0037](../../DECISIONS.md#dec-0037-a-teammates-computer-gets-a-seat-through-passioncodeai-never-through-the-operators-network).
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
- G2. PassionCode.ai carries the messages and never reads a credential or a usage figure.
- G3. The seat on the teammate's computer behaves exactly as a DEC-0025 guest: one group, never
  moved on, lent, given to an agent or made the ordinary sign-in; a lease extended by the sender;
  switched off past it, credential kept 30 days, then removed; called back only by its sender.
- G4. The operator sees the seat as today: in the teammate device's machine group (DEC-0013), with
  its usage, *Call back* on its row, Needs attention when it goes quiet.
- G5. The teammate accepts it in the window or through MCP; Claude Code on the teammate's computer
  can do the whole setup except the browser consent.
- G6. Every state is typed. A read that failed is `unknown`, never an empty inbox or an absent seat.

Non-goals:

- Moving a request between machines (DEC-0020 still forbids it; only a sign-in moves).
- Stopping a teammate who reads their own Keychain. As in DEC-0025, the revocation against an
  uncooperative teammate is signing the subscription out at the provider.
- Replacing ssh between the operator's own machines (DEC-0025, DEC-0033 stay for those).

## 3. Parties

| Party | Holds | Talks to |
|---|---|---|
| Sender device | an administrator's Switchboard; the subscription; the organization's group | Vesta only |
| Recipient device | a member's Switchboard; the guest seat once accepted | Vesta only |
| Vesta (PassionCode.ai) | the device registry, one mailbox per device, the audit log | both, as a server; reaches neither |
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
- Registration: Switchboard asks Vesta for a challenge, signs it with the signing key and sends
  both public keys, the platform, the app version and a label the person typed or confirmed
  (`POST /v1/devices`, names are proposals for Vesta, section 14). Vesta answers a `deviceId`. The
  OAuth grant needs a new scope beyond `me.organizations` (open question Q1).
- The device key is separate from the settings signing key (SB-272) and from ssh keys: one key, one
  purpose. The device record may carry the settings signer's public key, signed by the device key,
  so a later bundle can be tied to the device; nothing in this design depends on it.
- Rotation: `device rotate` makes new keys, registers them signed by the old signing key, and keeps
  the old ones until every seat thread has seen a message under the new ones. Loss of the vault
  items (a reinstall, a wiped Keychain) is a new device: the old one is revoked from the window or
  by an administrator, and its seats lapse by their lease (section 9).
- Disconnect (`org disconnect`) revokes the device at Vesta first; a failed revoke keeps the
  connection, as today's disconnect keeps the token rather than orphaning it.

Registration states, answered by `device status` and in `onboard --status`: `registered`,
`not_registered`, `scope_missing` (an older grant; connect again), `revoked`, `key_changed`
(Vesta lists keys for this device id other than ours), `unreachable` (Vesta did not answer; never
read as `not_registered`).

## 5. Pinning the other side's key

Vesta lists the devices of an organization's members with their public keys and roles. A list from
a server is a claim, so each side pins:

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

One JSON object per message, at most 64 KiB encoded (open question Q6).

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

- `kind` is one of five (section 7). Vesta needs `org`, `from`, `to`, `kind`, `thread`, `id` and
  `expires_at` in clear to route, authorize and expire; everything about the seat is in `ct`.
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
- `id` is a random UUID, also the idempotency key at Vesta: a submit whose answer was lost is sent
  again with the same bytes (section 11).

## 7. Kinds and who may send them

| Kind | From | To | Payload (inside `ct`) | Expires |
|---|---|---|---|---|
| `seat.offer` | an administrator's device | a member's device in the same organization | provider, the credential, a suggested group name, the organization, `lease_until`, the sender's settings for the seat (rules the guest keeps, DEC-0025) | 7 days |
| `lease.extend` | the sender of that thread | its recipient | `lease_until` | 1 hour |
| `usage.report` | the recipient of that thread | its sender only | seat state (`pending`, `active`, `off_lease`, `declined`, `removed`), usage windows as percentages and reset times, check time, session counts by state | 1 hour |
| `seat.recall` | the sender of that thread | its recipient | a reason code | 7 days |
| `seat.surrender` | the recipient of that thread | its sender only | the live credential (the recipient's renewals rotate the refresh token), or `declined` or `removed` with no credential | 7 days |

- Vesta enforces the From and To columns: a `seat.offer`, `lease.extend` or `seat.recall` only from
  a device whose person holds the administrator role in `org` to a device of a member of `org`; a
  `usage.report` or `seat.surrender` only along a thread that an offer opened, back to that offer's
  sender. Anything else is refused at submit.
- Each Switchboard checks the same rules again on receipt, and more: the thread's sender key is the
  pinned one; a `seat.surrender` with a credential is taken only after this device sent
  `seat.recall` on that thread or the recipient declined; a second `seat.offer` on a live thread is
  refused.
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

Vesta refuses a second submit of an `id` with other bytes and answers the stored receipt for the
same bytes.

## 9. Lease, state and the recipient's rules

The seat on the recipient is a DEC-0025 guest, with `sender_device` in place of `sender_key`:

- It joins the group the teammate chose when accepting (section 12), never `default`.
- The guest rules are the ones checked on every publication of the store today
  ([CONTRACTS](../../CONTRACTS.md#delegated-seats-and-the-own-sign-in-switch-sb-403-s2-to-s4-dec-0025-2026-11-17)):
  never moved on, handed over, given to an agent, lent, shared, marked elsewhere, made the ordinary
  sign-in or carried in a backup; only a message from the pinned sender device on its thread calls
  it back or removes it.
- The sender submits `lease.extend` every 15 minutes, to 48 hours ahead by default (1 to 720 at
  the offer). Past the lease, once the recipient's owner has run 15 minutes past both the lease and
  its own start, the seat is switched off and its homes cleaned; its credential is kept; the next
  `lease.extend` turns it on again. Thirty days after the lease ended the credential is removed.
- The recipient submits `usage.report` every 5 minutes while the seat is `active`, and at once when
  its state changes (at most one a minute).
- Own sign-in off (`own-sign-in off`) is recommended at accept and offered in the same step, as in
  DEC-0025; it is not required.

## 10. Polling, cadence and backoff

- Both sides fetch their inbox over HTTPS (`GET` with a cursor, section 14). Cadence: every 60
  seconds while this device has a live thread, an offer waiting either way, or a recall or
  surrender in flight; every 15 minutes otherwise (to see a new offer); and at once on *Check now*,
  `relay check`, or MCP.
- After a failed fetch: 60 s, 2 min, 4 min, 8 min, 15 min, then every 15 min, each with up to 20 %
  jitter; `Retry-After` from Vesta wins when longer. The first fetch that works resets it.
- Fetching acknowledges nothing: a message is acknowledged after it was applied and the store
  published, so a crash between fetch and apply fetches it again and the `id` check makes the
  second apply a no-op.
- The fetch runs in a spawned task outside the owner transaction (as the Vesta read does today),
  never once the drain began, and only while PassionCode.ai is connected. The idle budget gains one
  HTTPS request every 15 minutes on a computer with a registered device and no thread.
- Long-poll or server-sent events would cut the minute's delay; this design works without them
  (open question Q5).

## 11. Failure and unknown states

Every state below is typed; the CLI, MCP and the window show the same words.

Sender, per seat thread: `offered` (submitted, no answer), `active`, `declined`, `quiet` (no
`usage.report` for 15 minutes), `unreachable` (Vesta did not answer the last three fetches),
`recall_sent`, `recalled` (the surrender with its credential arrived and the provider check passed),
`expired` (the offer passed its expiry with no answer; the recipient never accepts an offer past
its expiry and then drops the credential it held, so the account comes back here ten minutes after
the expiry, past the clock tolerance), `outcome_unknown` (a submit whose answer
was lost: the same bytes are sent again until Vesta answers; until then the account stays marked
elsewhere and Needs attention names it).

Recipient, per guest seat: `pending` (an offer waits for the person), `active`, `off_lease`,
`removed`, `surrender_sent`, `outcome_unknown`.

Device: the registration states of section 4. Inbox: `ok`, `empty` only after a fetch that worked,
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
| The relay is read or run by someone else | see who offered what kind to whom and when; drop or delay messages | payloads are end-to-end encrypted and signed; a dropped `lease.extend` lapses the seat (fails closed); a dropped recall is resent and the provider sign-out remains |
| The relay inserts or swaps a device key | address an offer to a device it controls, or forge a sender | first-use pinning with fingerprints shown to both people; a changed key stops all traffic until a person confirms it |
| Replay of an old message | re-extend a lease, re-offer a seat, re-run a recall | `id`, per-pair `seq`, expiry, payload copies of the header (section 8) |
| Re-addressing a signed envelope | deliver one person's offer to another device | the recipient id is inside the signature and inside the payload |
| A member's device is compromised | read the seat's credential once accepted | the same as DEC-0025: the provider sign-out ends every copy; only seats offered to that device are exposed |
| An administrator's device is compromised | offer the organization's seats, recall them | Vesta's role check limits it to that organization; the audit log shows every offer; revoking the device at Vesta stops it |
| A member leaves the organization | keep using a seat | Vesta refuses the thread's messages once the membership ends; the lease lapses within its length; the credential is removed 30 days later; the provider sign-out ends it at once |
| Flooding a mailbox | exhaust a device's inbox or the owner's time | Vesta rate limits per sender device and per organization; the recipient caps envelopes per fetch and their size |
| A version downgrade | make a side accept a weaker format | `v` is signed; an unknown version is refused, never read as an older one |
| Clock skew | expire messages early or late | five minutes of tolerance; order comes from `seq`, not the clock |
| Secrets in logs or Git | leak a credential or a key | logs carry codes, ids and counts only; a planted-token test covers the relay's log lines and errors |

What this design does not change: anyone who can read a person's Keychain on their own computer can
read the seat on it. The operator's network is never part of the path.

## 14. What Vesta must provide

Asked on the board: project `passioncode-vesta`, thread 14da4917, message 446daca9 (2026-11-19).
The names below are proposals; Vesta's answer decides them.

1. **Device registry.** Register a device under the person's grant and organization membership with
   proof of possession of the signing key; list an organization's member devices with public keys,
   role and state; revoke (the person for their own device, an administrator for any device of the
   organization); rotate keys, the new set signed by the old signing key.
2. **Mailbox API.** Submit an envelope, idempotent by `id`; fetch a device's inbox from a cursor;
   acknowledge processed envelopes; envelopes kept until acknowledged or expired.
3. **Authorization.** The From and To rules of section 7, checked at submit against the current
   role and membership; a refused submit answers a typed code.
4. **Retention.** Envelopes deleted at acknowledgement or expiry, at most 7 days; nothing of an
   envelope kept afterwards but the audit row.
5. **Audit log.** One row per submit, fetch, acknowledgement, expiry and refusal: kind, sender and
   recipient device, thread, time, size, outcome; never payload. Readable by the organization's
   administrators, and the rows about a device by its person.

Vesta never sees: a credential, a private key, a usage figure, a group or folder name, an account
label or the provider account's e-mail, a session's content.

## 15. Open questions for Vesta

- Q1. The OAuth scope for devices and the relay (one scope, or `devices` and `relay` apart), and
  whether an existing `me.organizations` grant can be widened without a new consent.
- Q2. Is a device registered per person and organization, or per person across organizations?
- Q3. Which role may send `seat.offer`: any administrator, or a separate permission an
  administrator grants?
- Q4. Does Vesta revoke a member's devices and refuse their threads when the membership ends, and
  does it tell the sender (an envelope of its own, or a state in the device list)?
- Q5. Fetch only, long-poll or server-sent events; the rate limits per device and organization.
- Q6. The envelope size limit (64 KiB proposed) and the inbox size limit.
- Q7. Retention of the audit log, and whether it is exported to the organization.
- Q8. Can Vesta keep an append-only, publicly verifiable log of device keys, so a swapped key is
  detectable beyond first-use pinning?
- Q9. API versioning and the error vocabulary (typed codes, `Retry-After`).

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
  - W1 device identity at connect, registration states, pinning and fingerprints (needs Q1, Q2).
  - W2 the envelope library: seal, open, sign, check, replay state, with its tests (no Vesta needed).
  - W3 the relay client against Vesta's mailbox (needs section 14 items 2 to 4), the cadence and
    the failure states.
  - W4 the seat flow: offer, accept into a group, extend, report, recall, surrender, decline,
    expiry, on the guest rules that exist today.
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
- Flow against an in-process fake Vesta: offer, accept into a chosen group, usage report, lease
  extended, recall, surrender with the credential, provider check, account home; decline; an
  unanswered offer expires and the account comes home; a lease that lapses switches the seat off
  and the next extension turns it on; 30 days later the credential is removed.
- Failures: Vesta unreachable reads `unreachable`, never an empty inbox; a submit whose answer is
  lost is resent with the same bytes and ends `recalled` once; a crash between fetch and apply
  applies the message once.
- Authorization on receipt: a `usage.report` from a device that is not the thread's recipient, a
  `seat.recall` from a device that is not the thread's sender, a second `seat.offer` on a live
  thread are refused.
- The fake Vesta asserts that no stored row contains the planted credential, label, folder or
  e-mail; the relay's log lines pass the planted-identifier test of [ANALYTICS.md](../../ANALYTICS.md).
- Guest rules: the existing `delegation::tests` run over a relayed guest as over an ssh-delegated one.
