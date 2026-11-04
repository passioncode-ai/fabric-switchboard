# Usage analytics

**State:** built 2026-10-05 on the operator's request; sends only from release builds that carry
an App Key; on by default from the first start, with Switchboard's own switch (SB-246, operator
decision 2026-11-04, replacing issue #128's "nothing before the answer" of 0.6.82). Source: [`crates/switchboard-runtime/src/analytics.rs`](../crates/switchboard-runtime/src/analytics.rs).

Switchboard counts installs, days of use and connected accounts so PassionCode can see how its
apps are used, and so one person using several PassionCode.ai tools (Switchboard, Fabric, Fabric
Inbox) counts once. Events go to PassionCode's self-hosted Aptabase at
`https://analytics.passioncode.ai` (passioncode-platform `infra/analytics/README.md`, PLAT-019;
the ingestion contract is unchanged: [client contract](https://github.com/ssheleg/sshlg-analytics/blob/main/docs/client-contract.md)).
Releases up to 0.6.41 sent to the personal `https://analytics.sshlg.me`; their history to
2026-10-17 00:30 UTC was copied to the new host, and the delta after it is copied by
passioncode-platform (SB-H01).

## On by default, Switchboard's own switch (SB-246)

Operator decision 2026-11-04, about 09:45 UTC, in Claude Code session `5d9bed98` of Fabric,
relayed verbatim on the Fabric board by fabric-32: «Включает первый раз продукт, ему показывает,
чтобы подтвердить, что мы отправляем анонимные логи для юзажа и так далее. А это по дефолту
разрешить собирать анонимные данные об использовании и об ошибках, чтобы мы улучшали наш продукт.
Все, только для этого. Это по дефолту включено, для всех уже текущих тоже по дефолту включено. В
настройках можно выключить. Выключается на уровне каждого продукта.» It replaces the rule of
2026-10-08 (issue [#128](https://github.com/passioncode-ai/fabric-switchboard/issues/128), Fabric
ADR-0127, audit finding A7-012: nothing queued or sent before the person answered a question) and
the two-tier idea of 2026-11-03, which was not built.

- **On from the first start.** `app_installed`, `app_started`, `app_active`, `account_added`,
  `account_removed`, `project_changed` and `account_switched` are queued and sent from the first
  start, a background start (the login item, the lifecycle broker) or a run whose window never
  opens included (`on_by_default_a_background_start_sends_from_the_first_start`,
  `through_the_runtime_a_background_start_sends_the_start_and_the_day`).
- **A notice, not a question.** The window shows a first-run notice above every page but About
  (SCN-073): “Anonymous usage counts are on”, what is counted, *OK* and *Turn off*, and “You can
  turn them off at any time in About. This applies to Switchboard only.” *OK* records `notice_at`
  (Unix seconds) in `<data>/analytics-state.json`; *Turn off* turns the switch off and records it
  too. Nothing waits for the notice: it only tells. An install that answered the old question
  (`disclosed_at`) sees the new notice once
  (`the_notice_waits_until_acknowledged_and_ok_changes_nothing_else`,
  `an_install_that_answered_under_the_old_rule_sees_the_notice_once`).
- **Switchboard's own switch.** The switch is `enabled` in `<data>/analytics-state.json`
  (absent = on), set by About → *Share anonymous usage counts* (SCN-044) or the notice's
  *Turn off*. It turns Switchboard off or on only: the shared `PassionCode/installation.json`
  supplies the installation id and is never written for the switch, and its `analytics` field,
  another app's, is no longer read
  (`the_switch_adds_no_field_to_the_shared_file_and_never_writes_it`,
  `an_install_whose_shared_file_says_off_still_sends_when_never_answered_here`).
- **Off stops everything.** Turning it off drops what waits and nothing more leaves, at this run or
  later; there is no reduced tier. Turning it on again later counts again under the same shared
  installation id, which Switchboard does not own and never rotates
  (`turning_off_in_switchboard_sends_nothing_after_and_leaves_the_shared_file`,
  `turning_it_off_stops_everything_and_drops_what_waits`).

**Existing installs (migration, once).** Every install that never answered in Switchboard is on
after the update, whatever the shared file says (the operator: «для всех уже текущих тоже по
дефолту включено»). The one exception, the ecosystem's reading of the same decision (the
coordinator's correction of 2026-11-04, after the sshlg-growth consent contract `a48f41e`, board
reply `cbea0b76`; fabric-inbox, PrivatePhoneBot and sms-activate.app adopted it): a person who
explicitly said no stays off. For Switchboard that is a state with `disclosed_at` (answered here
under issue #128) while the shared file reads `analytics: false`: at the first start of this
version Switchboard's `enabled` becomes false, written once; answered with the shared switch on, it
becomes true, written once, so a later change of the shared file changes nothing here
(`an_answer_of_off_under_the_old_rule_stays_off_and_only_that`). A shared file that cannot be read
at that moment leaves such an install off for that run and decides at a later start. Every state
written by this version carries `"rule": 2`; only a state without it reads the old fields as
history.

**Never twice.** `app_installed` goes once per app: an install from 0.6.81 or earlier (`installed`,
or a day without the install flag in a state never answered) is never counted as new
(`an_old_state_with_a_day_but_no_install_flag_is_not_a_new_install`,
`an_install_that_answered_under_the_old_rule_sees_the_notice_once`); an install off at its first
start reports `app_installed` at the first start after it is turned on, with
`first_passioncode_app` said truly (`an_off_start_reports_the_install_at_the_next_start_once`).
A state file that cannot be read or parsed may hold the person's "off": it reads as off for that
run and never sends `app_installed` a second time
(`an_unreadable_state_is_off_for_the_run_and_never_reports_a_second_install`); one that cannot be
written logs `analytics_state_unwritable` (a code only) and the choice holds for that run only
(`a_state_that_cannot_be_written_is_reported_not_hidden`). `switchboard uninstall` removes the
state file with the rest of the data folder, so the switch and the notice are forgotten
(`uninstall_forgets_the_usage_answer`); backups do not carry it.

**Errors.** The decision also covers anonymous error data. Switchboard sends no error event today,
and this change adds none: a fixed-code error event (where and a code, never text, as
fabric-inbox's `app_error`) is an open follow-up in the SB-246 row of
[the backlog](evidence/backlog.md), to be privacy-reviewed before it is built.

The window reads `{available, enabled, notice_pending}` (`analytics_status`); the notice shows
when all of `available`, `enabled` and `notice_pending` hold, and the first-run tour waits for it
(`src/ui-logic.ts` `usageNotice`, `tourWaits`; `scripts/test-ui-logic.mjs`). *OK* is
`analytics_notice_seen`, the switch `set_analytics`.

## What is sent

Every event carries the app version, the OS name, an SDK tag (`switchboard-analytics@<version>`),
an Aptabase session id, `props.install_id` — the shared installation id below — the same value
again as `props.iid`, and `props.environment`: `production` for a release, `sandbox` for a
debug build or a pre-release version (`-rc.N`, `-beta.N`). `iid` and `environment` are the names
`sshlg-growth` counts by (operator decision 2026-10-05); `iid` adds no new identifier. Nothing
else identifies the machine or the person.

| Event | When | Props (besides `install_id`, `iid`, `environment`) |
|---|---|---|
| `app_installed` | first start of this app on this machine with the switch on (once, kept in `<data>/analytics-state.json`) | the counts below; `first_passioncode_app`: no PassionCode app had run here before |
| `app_started` | every start of the desktop app with the switch on, a background start included | `launch`: `ordinary` or `background` (login item, lifecycle broker) |
| `app_active` | once per UTC day while the app runs, window open or not | `accounts`, `enabled`, `claude`, `codex`, `oauth`, `api_key`, `setup_token`, `pools` (a count), `rotation_on` (enabled policies), `project_rules`, `projects`; since 0.7.0 `chains`, `ceilings`, `ceiling_questions` ([below](#chains-and-ceilings-070)) |
| `account_added` | an account appears | `provider`, `kind`, `method` (`sign_in`, `capture`, `import_claude_swap`, `manual`, `restore`, `sync`), `accounts` (total after) |
| `account_removed` | an account disappears | `provider`, `kind`, `accounts` |
| `project_changed` | a project created, changed or removed by the person (folders Switchboard adds or takes out by itself, SB-131, send nothing; the next person's change carries the current counts) | `change` (`created`, `updated`, `removed`), `projects`, `folders`, `project_accounts` — counts, never a name or a path |
| `account_switched` | the account in use or a pool's managed route changes | `provider`, `target` (`native`, `managed`), `cause` (`manual`, `rotation`, `limit`) |

**Never sent:** account ids, labels, e-mail addresses, organisation ids, pool names, paths,
tokens, provider responses or errors, quota figures, and (0.7.0) a chain's or ceiling's scope,
group id, workflow id, question id, agent, key name or amount. The test
`events_carry_counts_and_the_installation_id_never_an_identity` sends accounts whose id, label,
identity and pool are planted strings, and chains and ceilings with a planted group id, workflow
id, question id, account, key name and amount, and fails if any of them reaches the server.

Accounts present when analytics first runs are counted in `app_installed`, not reported as added.
An account added or removed while analytics is off is never reported later.

## Chains and ceilings (0.7.0)

Decision C-Q6 of the 0.7.0 iteration ([questions](runs/2026-10-26-iteration-0.7.0/questions.md),
assumed): fallback chains and spending ceilings are counted in `app_active` and nowhere else, with no
new event. The event keeps its name and its earlier props; three counts join it:

| Prop | Meaning | When it is left out |
|---|---|---|
| `chains` | how many fallback chains are set, of every scope (this computer, groups, tasks) added up | never: it is read from the store's metadata |
| `ceilings` | how many spending ceilings are set, of every scope added up | `spend.json` cannot be read: unknown, never sent as 0 |
| `ceiling_questions` | how many ceiling questions wait for an answer (a reached ceiling's *double* or *keep*) | as `ceilings` |

Counts only: never a scope, a group id, a workflow id, a question id, an agent, a key name or an
amount. `ceilings` and `ceiling_questions` are counted after the same pruning the window applies
(a deleted group's ceiling, an answered question, a question past its day), so they match what
the window shows. `spend.json` is read only on the pass that sends the day's `app_active`, so the
idle budget is unchanged. `app_installed` carries the earlier counts only. Code:
`active_counts` and `chains_and_ceilings` (`crates/switchboard-runtime/src/analytics.rs:708-733`);
tests `chains_and_ceilings_are_counted_and_an_unreadable_file_is_never_zero` (`:955`) and the
planted-identifier test (`:986`), which plants a group id, a workflow id, a question id, an
account, a key name and an amount in the chains and `spend.json` and checks that none leaves the
machine. Both were shown to fail with a defect planted (the chains' scopes sent; a ceiling's
amount sent), then restored. Nothing about the spend gate, the terminal hook, the organization
checklist or launch records is sent.

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
- **Its `analytics` field is not Switchboard's switch** (SB-246: the switch is each product's
  own). Switchboard neither reads it to decide nor writes it; another app may still keep its own
  switch there. The one reading of it is the migration above, once, for an install answered here
  under issue #128.
- **No app's switch or notice is kept here by Switchboard.** Switchboard's are `enabled` and
  `notice_at` in its own state file (above); the switch adds no field to this file
  (`the_switch_adds_no_field_to_the_shared_file_and_never_writes_it`).

Other apps adopt the same file, fields and rule (Fabric and Fabric Inbox tasks below).

## Delivery

- Batches of at most 25 to `POST /api/v0/events` with the `App-Key` header, from the owner,
  never blocking an operation or a monitor pass (`a_busy_server_keeps_the_batch_a_refusing_one_drops_it_and_batches_hold_25`).
- Transport errors, `429` and `5xx` keep the batch and retry after 60 s, then 10 min; `400` and
  `404` drop it. At most 200 events wait in memory; anything older than 23 h is dropped (the
  server refuses events older than a day). Nothing is written to disk but the small state file.
- Each send is one `analytics_flush` line in the operation log with its outcome and event count.
- A shared file that cannot be read for a moment keeps what waits for a later pass; another app's
  switch going off there drops nothing (`a_shared_file_unreadable_for_a_moment_keeps_what_waits`).
  Switchboard's own switch off drops it.

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

From 0.6.82 to 0.7.6, installs and active days were counted from the person's answer, not from the
first start (issue #128): growth saw a step down at 0.6.82, and installs that answered off were not
counted at all. From the release that carries SB-246 they are counted from the first start again,
background starts included, and installs that never answered in Switchboard are counted again:
growth sees a step up at that release. Installs that answered off under issue #128 while the shared
switch read off stay uncounted.

## Cross-app tasks

| App | Task |
|---|---|
| Fabric | [passioncode-ai/fabric#12](https://github.com/passioncode-ai/fabric/issues/12) — Aptabase app *Fabric* (`TjdkrpN3etucDcZAuintAB`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC` |
| Fabric Inbox | [passioncode-ai/fabric-inbox#27](https://github.com/passioncode-ai/fabric-inbox/issues/27) — Aptabase app *Fabric Inbox* (`XnZ1VzR5qEmaf7KF1nkUVU`), key `vault:sshlg-analytics/prod/APTABASE_APP_KEY_FABRIC_INBOX` |
| sshlg-growth | register the three apps in growth's app registry (`apps.analytics_app_id`) so `growth.analytics.*` and the analytics agent can query them — growth's task, sent to its owner |
