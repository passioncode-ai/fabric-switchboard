# AGPL-3.0 or commercial, and the repository standard — 2026-09-30

## Objective

Bring Fabric Switchboard onto the PassionCode.ai repository standard (Fabric Workspace
`knowledge/repository-standard.md`, rules F1–F11 of org-index `scripts/check_format.py`) and the
licence decided in Fabric ADR-0092: `AGPL-3.0-only OR LicenseRef-PassionCode-Commercial`.

## Decision recorded here

This repository keeps its decisions in [HANDOFF](../HANDOFF.md) sections and the
[0.4 plan](../PLAN-0.4.md); there is no `docs/adr/`. The licence change is recorded as follows.

- **From 2026-09-30 the source is open source under the GNU AGPL-3.0 only, or under a commercial
  licence from PassionCode.ai** (contact@passioncode.ai). Source of the decision: Fabric
  [ADR-0092](https://github.com/passioncode-ai/fabric/blob/main/docs/adr/0092-every-repository-is-agpl-3-0-or-commercial.md)
  and the knowledge base [licensing page](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/licensing.md).
- **Released versions keep their licence.** v0.4.0-beta.1 (and the commits from 2026-09-29 until
  this change) stays PolyForm Noncommercial 1.0.0 OR PolyForm Internal Use 1.0.0; v0.3.1-beta.1
  and earlier, and commits up to and including `7c36f4a`, stay MIT
  ([facts F-009](../brand/facts.md)).
- **Third-party components keep their own licences** ([THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)
  is unchanged; it covers dependencies, not Switchboard's own code).
- **No release was cut for the licence.** The next release (after v0.4.0-beta.1) is the first
  to ship under AGPL-3.0; its archives already carry the new `LICENSE` through
  `build_macos.py` / `build_windows_cross.py`, and its About screen shows the new sentence. The
  published 0.4.0-beta.1 app still shows the PolyForm sentence, which is correct for that build.

## Done

| Area | Change |
|---|---|
| Licence files | `LICENSE` = AGPL-3.0 template byte for byte (SHA-256 `0d96a4ff…abcb0`); `COMMERCIAL-LICENSE.md` and `CLA.md` = templates |
| Manifests | `package.json` (+ root entry of `package-lock.json`), `Cargo.toml` `[workspace.package] license` (was `license-file`), every member crate `license.workspace = true`, `src-tauri/tauri.conf.json` bundle `license` and long description, `.claude-plugin/marketplace.json`, `plugins/switchboard/.claude-plugin/plugin.json`, the skill's `license:` front matter |
| Checks | `scripts/check_plugin.py` now requires the new SPDX in plugin, marketplace, package and skill; its test plants `AGPL-3.0-only AND` and is caught |
| Interface | About → License: “Open source under the GNU AGPL-3.0; a commercial license is available — contact@passioncode.ai.” (`src/main.ts`, [SCN-023](../ux/scenarios.md), [strings](../brand/strings.md)) |
| Brand | [facts](../brand/facts.md) F-008/F-009, [terminology](../brand/terminology.md) licence wording |
| Docs | README title `# Fabric Switchboard`, intro, Quick start MCP proof, `## License` in the knowledge-base wording; README.ru.md; CONTRIBUTING.md; plugin README; AGENTS.md in the template form (*Read first* → what it is → commands → local rules → organisation → *After work*) |

Dated records (`docs/evidence/*`, earlier handoffs, the 2026-09-29 licence section of HANDOFF,
PLAN-0.4) keep the wording of their day.

<a id="mcp-proof"></a>
## MCP proof with a real client

Run 2026-09-30 in a temporary directory, with the published CLI inside the installed
v0.4.0-beta.1 app (`switchboard 0.4.0`), an empty temporary `--data-dir`, the read-only tool set,
and no change to any agent configuration:

```sh
# mcp.json: {"mcpServers":{"switchboard":{"command":"<Fabric Switchboard.app>/Contents/MacOS/switchboard",
#            "args":["--data-dir","<temp>/data","mcp","--read-only"]}}}
claude -p "Call the switchboard_accounts tool once and reply with exactly its JSON result." \
  --strict-mcp-config --mcp-config mcp.json \
  --allowedTools mcp__switchboard__switchboard_accounts \
  --output-format stream-json --verbose --max-turns 3
```

Observed: exit 0; `init` → `mcp_servers: [{"name":"switchboard","status":"connected"}]`; tool
result `{"accounts":[]}`; final result `{"accounts":[]}`.

## Checks run

- `./scripts/check.sh` → exit 0 (see the PR).
- org-index `scripts/check_format.py --offline --repo fabric-switchboard` in a sibling scratch
  layout → 0 findings (before: 6 — F4, F5, F7, F8, F11 `package.json`, F11 `Cargo.toml`).
- org-index `scripts/check_names.py --offline` → 0 findings for fabric-switchboard.

## Open

- org-index `repositories.json` still describes the repository as "Source-available under
  PolyForm Noncommercial or Internal Use" — the owner of org-index updates the row.
- The site's Switchboard page (passioncode-ai.github.io) may still say source-available; its
  owner updates it.

## Exact next task

Unchanged from [HANDOFF](../HANDOFF.md): SB-10, the launcher member. The next Switchboard
release is the first AGPL one; its release record states that.
