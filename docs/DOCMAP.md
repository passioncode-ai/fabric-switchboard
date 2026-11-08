# Doc map: Fabric Switchboard

Where settled things live in this repository, which file is the single home of each kind of
fact, what a change obliges, and which check proves it. Pointers only: where another file says
it, this one names the file. Seeded 2026-11-08 by `/task-pipeline setup`; its findings and fix
plan are in [plans/2026-11-08-doc-audit.md](evidence/plans/2026-11-08-doc-audit.md).

## Contents

- Regime
- Registers
- Single source of truth
- Propagation matrix
- Gates
- Ratchets
- Terms

## Regime

`governed`, established 2026-11-08 by the setup audit. Artifacts live in `docs/evidence/`
(registers, verification) and `docs/runs/` (one folder per run or iteration).

## Registers

| Register | File | ID scheme | Append-only? | Guarded? |
|---|---|---|---|---|
| Board (open work) | [evidence/backlog.md](evidence/backlog.md) | `SB-<n>` | rows never deleted; a status starts with its state | lease ([AGENT_SYNC.md](AGENT_SYNC.md)) |
| Handoff | [HANDOFF.md](HANDOFF.md) | dated entry, newest first | yes | lease |
| Verification | [evidence/verification.md](evidence/verification.md) | per release | yes | lease |
| Backlog sources | [backlog-sources.json](backlog-sources.json) | one source | n/a | lease |
| Scenarios | [ux/scenarios.md](ux/scenarios.md) | `SCN-<nnn>` | ids never reused | no |
| Decisions | [DECISIONS.md](DECISIONS.md) | `DEC-<nnnn>` | yes: a change is a new entry and a status line | no lease yet |

## Single source of truth

| Fact | Home | Everything else |
|---|---|---|
| Implementation contract (files, operations, refusals) | [CONTRACTS.md](CONTRACTS.md) | cites its section |
| Intended behaviour and deferred features | [SPEC.md](SPEC.md) | links to it |
| What the interface does for a person | [ux/scenarios.md](ux/scenarios.md) | cites `SCN-<nnn>` |
| How the product sounds, its words and public facts | [brand/voice.md](brand/voice.md), [brand/terminology.md](brand/terminology.md), [brand/facts.md](brand/facts.md) | link to the term |
| The window's strings and their Russian | `src/locales/ru.ts`; decision-carrying ones registered in [brand/strings.md](brand/strings.md) | cite the string |
| Processes, timers, ports, files written | `AGENTS.md`, section Lifecycle | link to it |
| What analytics sends | [ANALYTICS.md](ANALYTICS.md) | link to it |
| CLI commands and flags | [CLI.md](CLI.md) | link to it |
| How a release is built and published | [DISTRIBUTION.md](DISTRIBUTION.md) | link to it |
| What changed per version | `CHANGELOG.md` | cites the version |
| Current state and the exact next task | the top entry of [HANDOFF.md](HANDOFF.md) | link to it |
| Organisation-wide tracks | fabric-workspace `knowledge/roadmap.md` (`RM-*`) | cite the track |

## Propagation matrix

| Change type | Update these | Checked by |
|---|---|---|
| **A new document or rule** | the index a reader opens (`AGENTS.md` *Read first*, [HANDOFF.md](HANDOFF.md)), this map | `review`: no check knows which index should name a new file; `scripts/check_docs.py` only proves links resolve |
| A decision that holds | a [DECISIONS.md](DECISIONS.md) entry; every document in its *Consequences / affects* line cites the id | `review`: no check follows the affects lines yet |
| A string the window shows | `src/locales/ru.ts`, [brand/strings.md](brand/strings.md) when it carries a decision, Copylot report ids in the change | `scripts/check-locale.mjs`, `scripts/check-strings-registry.mjs` |
| A backend sentence a person can see | `src/adapter.ts` (verbatim or mapped) or the script's allowlist with its reason, `src/locales/ru.ts` | `scripts/check-error-vocabulary.mjs` |
| What the interface does | [ux/scenarios.md](ux/scenarios.md) in the same change | `scripts/check_docs.py` (coverage line present); the rest `review` until a scenario linter exists (finding F-4) |
| A CLI command or flag | [CLI.md](CLI.md), the MCP tool or its reason, the skill reference | `every_cli_command_has_an_mcp_tool_or_a_reason`, `every_mcp_tool_is_in_the_skill_reference`, `every_command_and_argument_has_help` |
| An analytics event or prop | [ANALYTICS.md](ANALYTICS.md), the planted-identifier test | `events_carry_counts_and_the_installation_id_never_an_identity` |
| A process, timer, port or file the app owns | `AGENTS.md` Lifecycle, [CONTRACTS.md](CONTRACTS.md) | `review`: behaviour, not a shape a script can read |
| A release | every version file `scripts/check-brand.mjs` compares, `CHANGELOG.md` | `scripts/check-brand.mjs` (versions agree), the release workflow's preflight |
| A board row changes | its status starts with a declared word, `done` or `closed` once its last note closes it, and names its receipt; a `\|` inside a cell is escaped | `scripts/check_docs.py` (SB-306), proven by `scripts/test_check_docs.py` |
| A plugin or skill file | `plugins/` | `scripts/check_plugin.py` |

## Gates

| Gate | Command | When | Blocking? |
|---|---|---|---|
| Everything below | `./scripts/check.sh` | before a pull request is merged | yes |
| Hosted full checks | `.github/workflows/nightly.yml` | nightly, and on a release tag | yes for a release |

`./scripts/check.sh` runs the brand check, the build, the window logic tests, the locale,
strings-registry and error-vocabulary checks, `cargo fmt`, the Rust tests (and the MCP tests
under a 1 MiB stack), clippy, the Windows cross-check when `cargo-xwin` is installed, the docs
and plugin checks and the release-script tests. It does not prove live provider behaviour,
Windows acceptance or the interface in a browser.

## Ratchets

None declared yet.

## Terms

| Term | Definition lives in | Anchor |
|---|---|---|
| Group (code: pool, project) | [brand/terminology.md](brand/terminology.md) | `#group-project-automatic-switching-api-keys-0682-decisions-d-1-and-d-2` |
| Automatic switching | [brand/terminology.md](brand/terminology.md) | same section |
| API keys | [brand/terminology.md](brand/terminology.md) | same section |
