# Usage analytics

**State:** built 2026-10-05 on the operator's request; sends only from release builds that carry
an App Key, and only after the person has answered the switch in this app (issue #128, 0.6.82). Source: [`crates/switchboard-runtime/src/analytics.rs`](../crates/switchboard-runtime/src/analytics.rs).

Switchboard counts installs, days of use and connected accounts so PassionCode can see how its
apps are used, and so one person using several PassionCode.ai tools (Switchboard, Fabric, Fabric
Inbox) counts once. Events go to PassionCode's self-hosted Aptabase at
`https://analytics.passioncode.ai` (passioncode-platform `infra/analytics/README.md`, PLAT-019;
the ingestion contract is unchanged: [client contract](https://github.com/ssheleg/sshlg-analytics/blob/main/docs/client-contract.md)).
Releases up to 0.6.41 sent to the personal `https://analytics.sshlg.me`; their history to
2026-10-17 00:30 UTC was copied to the new host, and the delta after it is copied by
passioncode-platform (SB-H01).

## Nothing before the answer

Operator decision 2026-10-08 (issue [#128](https://github.com/passioncode-ai/fabric-switchboard/issues/128),
Fabric ADR-0127, audit finding A7-012): usage counts stay **on by default, but no event is queued
or sent until the person has answered the switch once in this app**. Two places ask, and both show
what is counted before the switch can be used:

- **The first-run question** above every page but About (SCN-073): what is counted, “Nothing has
  been sent yet. Nothing is sent until you answer.”, the switch *Share anonymous usage counts*
  (on) and *Continue*.
- **About → Usage analytics** (SCN-044): the same text, switch and *Continue* while unanswered;
  afterwards the switch applies at once, as before.

Either answer, on or off, records `disclosed_at` (Unix seconds) in **this app's own**
`<data>/analytics-state.json`, never in the shared `PassionCode/installation.json`: another
PassionCode.ai app's answer does not answer for Switchboard
(`the_answer_is_this_apps_own_and_never_written_to_the_shared_file`). The switch itself stays the
shared `analytics` field below.

While the switch is on and `disclosed_at` is absent:

- `app_installed`, `app_started`, `app_active`, `account_added`, `account_removed`,
  `project_changed` and `account_switched` are not queued; a flush finds nothing to send. A
  background start (the login item, the lifecycle broker) or a run whose window never opens sends
  nothing (`nothing_is_queued_or_sent_before_the_answer_not_even_from_a_background_start`).
- Accounts seen meanwhile are counted locally only, so no account change from before the answer is
  reported after it. Days are not counted locally: the day's `app_active` goes at the first
  background pass after an answer of on. The start of the run in which the person answers is
  reported after the answer: an answer of on reports that run's `app_installed` (a fresh install
  only, with `first_passioncode_app` true when this app created the shared file, kept in the state
  as `machine_first` until then) and its `app_started`, once
  (`an_answer_of_on_reports_the_held_start_once_and_nothing_from_before_it`,
  `the_first_passioncode_app_is_reported_truly_after_a_later_answer`,
  `through_the_runtime_nothing_leaves_before_the_answer_then_the_start_and_the_day`).
- An answer of off sends nothing while the shared switch stays off, and the question is not
  asked again (`an_answer_of_off_sends_nothing_then_or_after_a_restart`). The switch is shared:
  if another PassionCode.ai app turns it back on, Switchboard counts again, since it has its
  answer (Fabric ADR-0127 Amendment 2;
  `an_answer_of_off_holds_while_the_shared_switch_stays_off`).
- A question answered on after another PassionCode.ai app turned the shared switch off records
  the answer and leaves the switch off for every app; the window reads the status again every
  minute and when it is shown, so such a question usually goes away by itself first
  (`answering_on_after_another_app_turned_the_switch_off_keeps_it_off`).

An install from 0.6.81 or earlier (its state has `installed` or an `active_day`, no `disclosed_at`) is asked once, is
told “An earlier version of Switchboard may already have sent these counts from this computer.
Nothing more is sent until you answer.”, and is never counted as a new install
(`an_upgraded_install_asks_once_and_is_not_counted_as_new`,
`an_old_state_with_a_day_but_no_install_flag_is_not_a_new_install`). A state file that cannot be
read or parsed asks again and never sends `app_installed` a second time
(`an_unreadable_state_asks_again_and_never_reports_a_second_install`); one that cannot be written
logs `analytics_state_unwritable` (a code only) and the answer holds for that run only
(`a_state_that_cannot_be_written_is_reported_not_hidden`). `switchboard uninstall` removes the
state file with the rest of the data folder, so the answer is forgotten and the question returns
at the next first start (`uninstall_forgets_the_usage_answer`); backups do not carry it.

The window reads `{available, enabled, disclosed, sent_before}` (`analytics_status`); the
question shows when all of `available`, `enabled` and `disclosed: false` hold, and the first-run
tour waits for it (`src/ui-logic.ts` `usageQuestion`, `tourWaits`; `scripts/test-ui-logic.mjs`).

## What is sent

Every event carries the app version, the OS name, an SDK tag (`switchboard-analytics@<version>`),
an Aptabase session id, `props.install_id` — the shared installation id below — the same value
again as `props.iid`, and `props.environment`: `production` for a release, `sandbox` for a
debug build or a pre-release version (`-rc.N`, `-beta.N`). `iid` and `environment` are the names
`sshlg-growth` counts by (operator decision 2026-10-05); `iid` adds no new identifier. Nothing
else identifies the machine or the person.

| Event | When | Props (besides `install_id`, `iid`, `environment`) |
|---|---|---|
| `app_installed` | first start of this app on this machine after the person answered the switch with on (once, kept in `<data>/analytics-state.json`) | the counts below; `first_passioncode_app`: no PassionCode app had run here before |
| `app_started` | every start of the desktop app once answered; the start of the run in which the person answers, after the answer | `launch`: `ordinary` or `background` (login item, lifecycle broker) |
| `app_active` | once per UTC day while the app runs, window open or not | `accounts`, `enabled`, `claude`, `codex`, `oauth`, `api_key`, `setup_token`, `pools` (a count), `rotation_on` (enabled policies), `project_rules`, `projects` |
| `account_added` | an account appears | `provider`, `kind`, `method` (`sign_in`, `capture`, `import_claude_swap`, `manual`, `restore`, `sync`), `accounts` (total after) |
| `account_removed` | an account disappears | `provider`, `kind`, `accounts` |
| `project_changed` | a project created, changed or removed by the person (folders Switchboard adds or takes out by itself, SB-131, send nothing; the next person's change carries the current counts) | `change` (`created`, `updated`, `removed`), `projects`, `folders`, `project_accounts` — counts, never a name or a path |
| `account_switched` | the account in use or a pool's managed route changes | `provider`, `target` (`native`, `managed`), `cause` (`manual`, `rotation`, `limit`) |

**Never sent:** account ids, labels, e-mail addresses, organisation ids, pool names, paths,
tokens, provider responses or errors, quota figures. The test
`events_carry_counts_and_the_installation_id_never_an_identity` sends accounts whose id, label,
identity and pool are planted strings and fails if any of them reaches the server.

Accounts present when analytics first runs are counted in `app_installed`, not reported as added.
An account added or removed while analytics is off, or before the person answered, is never
reported later.

## The shared installation id

All PassionCode.ai tools share one file:

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
- **`analytics: false` turns analytics off for every PassionCode.ai tool on the machine.** About →
  *Share anonymous usage counts* and the first-run question write it; turning it off also drops
  events still waiting.
- **No app's answer is kept here.** Each app keeps its own answer timestamp in its own state file;
  Switchboard's is `disclosed_at` (above). The answer adds no field to this file
  (`the_answer_adds_no_field_to_the_shared_file`).

Other apps adopt the same file, fields and rule (Fabric and Fabric Inbox tasks below).

## Delivery

- Batches of at most 25 to `POST /api/v0/events` with the `App-Key` header, from the owner,
  never blocking an operation or a monitor pass (`a_busy_server_keeps_the_batch_a_refusing_one_drops_it_and_batches_hold_25`).
- Transport errors, `429` and `5xx` keep the batch and retry after 60 s, then 10 min; `400` and
  `404` drop it. At most 200 events wait in memory; anything older than 23 h is dropped (the
  server refuses events older than a day). Nothing is written to disk but the small state file.
- Each send is one `analytics_flush` line in the operation log with its outcome and event count.
- A shared file that cannot be read for a moment keeps what waits for a later pass; a switch that
  reads off drops it (`a_shared_file_unreadable_for_a_moment_keeps_what_waits`).

## Which builds send

`SWITCHBOARD_ANALYTICS_APP_KEY` is read at compile time (`option_env!`). Only the release
workflow sets it, from the repository secret of the same name, which holds the Aptabase App Key
of the app *Fabric Switchboard* on `analytics.passioncode.ai` (vault
`passioncode-platform/prod/APTABASE_APP_KEY_SWITCHBOARD`; set in the `release` environment on
2026-10-19 from stdin, never printed).
Source builds, forks, `npm run app:build` without the variable, tests and the smoke check send
nothing, and About shows the switch as unavailable. Only the desktop app owning the real data
folder starts analytics — not `switchboard serve`, not the CLI, not a `--data-dir` store.

## Reading the numbers

The Aptabase dashboard at `analytics.passioncode.ai` (app *Fabric Switchboard*). On the old
`analytics.sshlg.me` the app had id `sVX004YjufDn0d4BiULQpF`; a debug event from `scripts/send-test-event.sh` was stored as `sVX004YjufDn0d4BiULQpF_DEBUG` on 2026-10-05); cross-app questions —
one `install_id` across apps — through ClickHouse, which `sshlg-growth` queries
(`growth.analytics.*`). Registering the app in growth's app registry is growth's task.

From 0.6.82, installs and active days are counted from the person's answer, not from the first
start (issue #128): growth sees a step down at that release, and installs that answer off are not
counted at all.

## Cross-app tasks

| App | Task |
|---|---|
| Fabric | [passioncode-ai/fabric#12](https://github.com/passioncode-ai/fabric/issues/12) — Aptabase app *Fabric* (`TjdkrpN3etucDcZAuintAB`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC` |
| Fabric Inbox | [passioncode-ai/fabric-inbox#27](https://github.com/passioncode-ai/fabric-inbox/issues/27) — Aptabase app *Fabric Inbox* (`XnZ1VzR5qEmaf7KF1nkUVU`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC_INBOX` |
| sshlg-growth | register the three apps in growth's app registry (`apps.analytics_app_id`) so `growth.analytics.*` and the analytics agent can query them — growth's task, sent to its owner |
