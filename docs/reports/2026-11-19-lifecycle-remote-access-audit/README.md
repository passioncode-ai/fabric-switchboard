---
report: 2026-11-19-lifecycle-remote-access-audit
title: Session lifecycle, remote machines, delegation and access, release and update: a module-by-module audit
date: 2026-11-19
read_at: origin/main c997483b (release 0.7.123 merged)
method: five read-only module audits run in parallel (code, docs, tests, workflows, gh and git reads); nothing built; security findings stated as classes
modules: m1-local-lifecycle.md, m2-cross-machine.md, m3-delegation-access.md, m4-release-update.md, m5-hanging-work.md
spec: ../../plans/2026-11-19-lifecycle-remote-access/spec.md
plan: ../../plans/2026-11-19-lifecycle-remote-access/plan.md
---

# Session lifecycle, remote machines, delegation and access: audit

The operator's order (2026-11-19, this repository's session), verbatim: «И давай еще раз аудит, лейфсайкл, асессии, ревью, вот эти, которые висят, как мы запускаем, выпускаем на одной машине, на удаленной, как мы с этой машины можем выпустить на удаленной, или с удаленной машины может выпустить на этой, по каким правилам, делегации, permission, доступ. Вот это все максимально детальный аудит, разбиваем на модули, каждый модуль отдельно анализируем, находим слабые места, либо неправильные, где-то связи битые, и все это собираем в один общий репорт. По нему пишем спеку, потом планируем, декомпозируем, все исправляем, выкатываем новую версию. И волнами заканчиваем все баги, чтобы ничего не осталось.»

## Modules and counts

| Module | File | P0 | P1 | P2 | P3 | Also |
|---|---|---|---|---|---|---|
| M1 session lifecycle on one machine | [m1-local-lifecycle.md](m1-local-lifecycle.md) | 0 | 6 | 18 | 21 | 9 already filed |
| M2 sessions and accounts across machines | [m2-cross-machine.md](m2-cross-machine.md) | 0 | 2 | 13 | 10 | |
| M3 delegation, permissions, access | [m3-delegation-access.md](m3-delegation-access.md) | 0 | 1 | 5 | 13 | 1 info |
| M4 release and update | [m4-release-update.md](m4-release-update.md) | 0 | 5 | 17 | 7 | |
| M5 work that hangs | [m5-hanging-work.md](m5-hanging-work.md) | 0 | 3 | 6 | 4 | ~30 wrong backlog statuses, 1 stale branch (deleted) |
| Found while auditing | SB-558 | 1 | | | | v0.7.123 release stopped by a Windows stack overflow |

## What is most wrong (P0 and P1)

| id | what | module |
|---|---|---|
| SB-558 | v0.7.123's nightly overflowed a Windows test thread; the app starts its owner on the Windows main thread (1 MiB) | found while auditing (PR #864) |
| M3-01 | Any process of the same user can export subscription sign-ins and saved keys through the CLI without the Keychain's prompt | M3 |
| M2-01 | A recalled seat accepts whatever sign-in the teammate's machine returns | M2 |
| M1-05 | Resuming an isolated session reuses the home's old token and skips the account checks (disabled, elsewhere, given to an agent, at its limit) | M1 |
| M1-06 | Resuming widens a session's permissions to full access | M1 |
| M1-01 | Commands of sessions in Switchboard's own homes are invisible to the duty officer, host keep and restart waves | M1 |
| M1-03 | The duty officer kills hosted sessions and Dashboards consoles and brings them back as plain Terminal windows | M1 |
| M1-04 | A resumed session writes no home marker; a new launch or a remove can rewrite or delete its home under it | M1 |
| M1-02 | Claude board runs are never extended at their deadline (Codex only) | M1 |
| M2-03 | Remote session acts the operator asked for (pause, restart, continue, output, typing, launch from the window or MCP) are missing | M2 |
| M4-01 | The release guard can pass on a nightly run that ran no tests | M4 |
| M4-12 | Every release restarts every Studio space's `serve` after 30 minutes and cuts streams in flight | M4 |
| M4-14 | Nothing says a copy is behind the feed, here or on another machine | M4 |
| M4-15, M4-16 | Linux CLI tarball installs never update; `.deb` installs need a person's `sudo` | M4 |
| M5-01, M5-02, M5-07 | PR #821 does not compile against main and lets continuation use an account turned off; #821 and #829 share SB-418 | M5 |

## Roots shared across modules

| Root | Findings | One fix |
|---|---|---|
| R1 Continuation is built from the home's present state, not from the launch it continues | M1-01, M1-03..07, M1-21..23, M1-45, M5-02 | One continuation module (SB-463) that builds every resume from the launch record: account check and fresh credential, permission mode, environment, host, home marker |
| R2 Sessions and machines are identified by fragile keys (ancestry, a command-line path, an ssh name, a label, a pid) | M1-01, M1-03, M1-27, M2-05, M2-09, M2-10, M2-24, M3-05 | Identify a session by its config folder and launch record, a machine by its `ssh -G` identity and pinned key, an account there by its recorded remote id |
| R3 An unknown or unreadable state is read as "nothing there" or "unreachable" | M1-15, M1-24, M2-02, M2-13, M3-05 | Typed unknowns: skip the act, say why, raise an attention item |
| R4 Capabilities and gates wider than the act | M3-01, M3-02, M3-06, M3-10, M3-14, M1-06 | A capability per launch and pool; value-returning operations only through restricted, journalled doors; decisions bound to a verifiable artefact |
| R5 Nobody sees a machine fall behind or a release go wrong | M4-01..08, M4-12, M4-14..19, M4-26 | Guard on jobs, not runs; retries; an `update` health check and attention item for every machine; one update policy for app and `serve`; a remote update verb |
| R6 Remote acts are partial and partly unproven | M2-03, M2-04, M2-11, M2-15, M2-18, M3-12 | W10.6 remote acts through one proved path; hosted by default; dispatch records before the start |
| R7 Pacing and deadlines on the wall clock | M1-09, M1-10, M1-43 | Monotonic or sleep-aware clocks |
| R8 Board hygiene | M5 status table, plan State table, DEC "next free" lines | One pass now; statuses edited in the same change as each merge from now on |

## How this report is used

The [spec](../../plans/2026-11-19-lifecycle-remote-access/spec.md) turns each root into requirements with acceptance checks; the [plan](../../plans/2026-11-19-lifecycle-remote-access/plan.md) orders them in waves W11.0 to W11.8 with backlog rows, and every finding id above maps to exactly one wave.
