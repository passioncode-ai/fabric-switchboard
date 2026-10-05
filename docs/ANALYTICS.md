# Usage analytics

**State:** built 2026-10-05 on the operator's request; sends only from release builds that carry
an App Key. Source: [`crates/switchboard-runtime/src/analytics.rs`](../crates/switchboard-runtime/src/analytics.rs).

Switchboard counts installs, days of use and connected accounts so PassionCode can see how its
apps are used, and so one person using several PassionCode apps (Switchboard, Fabric, Fabric
Inbox) counts once. Events go to the self-hosted Aptabase at `https://analytics.sshlg.me`
(`ssheleg/sshlg-analytics`, [ingestion contract](https://github.com/ssheleg/sshlg-analytics/blob/main/docs/client-contract.md)).

## What is sent

Every event carries the app version, the OS name, an SDK tag (`switchboard-analytics@<version>`),
an Aptabase session id and `props.install_id` — the shared installation id below. Nothing else
identifies the machine or the person.

| Event | When | Props (besides `install_id`) |
|---|---|---|
| `app_installed` | first start of this app on this machine (once, kept in `<data>/analytics-state.json`) | the counts below; `first_passioncode_app` — no PassionCode app had run here before |
| `app_started` | every start of the desktop app | `launch`: `ordinary` or `background` (login item, lifecycle broker) |
| `app_active` | once per UTC day while the app runs, window open or not | `accounts`, `enabled`, `claude`, `codex`, `oauth`, `api_key`, `setup_token`, `pools` (a count), `rotation_on` (enabled policies), `project_rules` |
| `account_added` | an account appears | `provider`, `kind`, `method` (`sign_in`, `capture`, `import_claude_swap`, `manual`, `restore`, `sync`), `accounts` (total after) |
| `account_removed` | an account disappears | `provider`, `kind`, `accounts` |
| `account_switched` | the account in use or a pool's managed route changes | `provider`, `target` (`native`, `managed`), `cause` (`manual`, `rotation`, `limit`) |

**Never sent:** account ids, labels, e-mail addresses, organisation ids, pool names, paths,
tokens, provider responses or errors, quota figures. The test
`events_carry_counts_and_the_installation_id_never_an_identity` sends accounts whose id, label,
identity and pool are planted strings and fails if any of them reaches the server.

Accounts present when analytics first runs are counted in `app_installed`, not reported as added.
An account added or removed while analytics is off is never reported later.

## The shared installation id

All PassionCode apps share one file:

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/PassionCode/installation.json` |
| Windows | `%APPDATA%\PassionCode\installation.json` |

```json
{ "version": 1, "id": "<random UUID v4>", "analytics": true, "created_at": 1791165882 }
```

- **Created once** by whichever app starts first: written to a temporary file and hard-linked into
  place, which fails if another app created it at the same moment — that file is then read
  instead (`the_installation_is_shared_created_once_and_never_overwritten`).
- **Never repaired.** A file that does not parse or whose `id` is not a UUID is left as it is, and
  analytics stays off (fail closed).
- **Unknown fields are kept** when an app rewrites it, so apps can add their own.
- **`analytics: false` turns analytics off for every PassionCode app on the machine.** About →
  *Share anonymous usage counts* writes it; turning it off also drops events still waiting.

Other apps adopt the same file, fields and rule (Fabric and Fabric Inbox tasks below).

## Delivery

- Batches of at most 25 to `POST /api/v0/events` with the `App-Key` header, from the owner,
  never blocking an operation or a monitor pass (`a_busy_server_keeps_the_batch_a_refusing_one_drops_it_and_batches_hold_25`).
- Transport errors, `429` and `5xx` keep the batch and retry after 60 s, then 10 min; `400` and
  `404` drop it. At most 200 events wait in memory; anything older than 23 h is dropped (the
  server refuses events older than a day). Nothing is written to disk but the small state file.
- Each send is one `analytics_flush` line in the operation log with its outcome and event count.

## Which builds send

`SWITCHBOARD_ANALYTICS_APP_KEY` is read at compile time (`option_env!`). Only the release
workflow sets it, from the repository secret of the same name, which holds the Aptabase App Key
of the app *Fabric Switchboard* (vault `sshlg-analytics/prod/APTABASE_APP_KEY_SWITCHBOARD`).
Source builds, forks, `npm run app:build` without the variable, tests and the smoke check send
nothing, and About shows the switch as unavailable. Only the desktop app owning the real data
folder starts analytics — not `switchboard serve`, not the CLI, not a `--data-dir` store.

## Reading the numbers

The Aptabase dashboard at `analytics.sshlg.me` (app *Fabric Switchboard*, id `sVX004YjufDn0d4BiULQpF`; a debug event from `scripts/send-test-event.sh` was stored as `sVX004YjufDn0d4BiULQpF_DEBUG` on 2026-10-05); cross-app questions —
one `install_id` across apps — through ClickHouse, which `sshlg-growth` queries
(`growth.analytics.*`). Registering the app in growth's app registry is growth's task.

## Cross-app tasks

| App | Task |
|---|---|
| Fabric | [passioncode-ai/fabric#12](https://github.com/passioncode-ai/fabric/issues/12) — Aptabase app *Fabric* (`TjdkrpN3etucDcZAuintAB`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC` |
| Fabric Inbox | [passioncode-ai/fabric-inbox#27](https://github.com/passioncode-ai/fabric-inbox/issues/27) — Aptabase app *Fabric Inbox* (`XnZ1VzR5qEmaf7KF1nkUVU`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC_INBOX` |
| sshlg-growth | register the three apps in growth's app registry (`apps.analytics_app_id`) so `growth.analytics.*` and the analytics agent can query them — growth's task, sent to its owner |
