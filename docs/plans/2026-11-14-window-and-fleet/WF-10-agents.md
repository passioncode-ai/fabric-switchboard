# WF-10 Agents: the central place (SB-372, waves 2 and 4)

> настройки агентов, которые здесь вот у нас есть, ... глобальные настройки, какими пермишенами они
> запускаются, фоллбеки и так далее ... мы в первую очередь говорим что нужно поставить switchboard
> а потом уже делать какие-то настройки ... это централизованное место switchboard ... темплейты
> ... скиллы, возможно, какие-нибудь подгрузим, типа общие плагины.

## Now

- `renderAgents` (`src/main.ts:1007`): default agent (1015), installed agents (1028), fallback chains
  (1116), available agents (1073), a pointer to API keys, the MCP connect panel (1253).
- Permissions (bypass or ask for the sessions Switchboard starts) exist in MCP
  (`switchboard_agent_set permissions`, SB-298) and the CLI, not in the window.
- Launched homes carry the person's `CLAUDE.md`, skills, plugins and hooks by links (SB-137,
  `crate::carry::CLAUDE_LINKS`); there is no choice of what is carried and no template.

## Sub-modules and tasks

**10.1 Global settings in the window (wave 2).** An *Agent settings* block on the Agents page:
permissions (bypass, ask), default agent, which agents may take over work, the machine's fallback
chain, each with what it applies to ("every session Switchboard starts on this computer").

**10.2 Templates (wave 4).** An agent template: a named, versioned set (permissions, default model
per agent, carried files, MCP servers, environment names without values) applied to a group or the
machine. Versions are add-only copies (operator rule 2026-11-13). Stored in `agent-templates.json`;
a launch records which template version it used.

**10.3 Shared skills and plugins (wave 4).** A list of what the person's Claude folder offers
(skills, plugins, `CLAUDE.md`, hooks) with a switch per item for the homes Switchboard makes, and
"also install for everyone on this computer" through the family updater (`sshlg-skills update`)
when the item is a family member. Writes only links in Switchboard's homes, never the person's own
folder.

## MCP

`switchboard_agents` and `switchboard_agent_set` gain the template and carry lists; new
`switchboard_agent_template` (list, show, create, apply). Every window control lands with its tool.

## Decisions

DEC-0018: Switchboard is the machine's place for agent settings. The knowledge base page that names
products' roles (fabric-workspace `knowledge/`) gains the line, by a board request.

## Questions

Whether Fabric and Dashboards stop offering their own permission choices and link here: asked on
the board to `fabric` and `fabric-dashboards` with this module.

## Checks

Launch tests that a template's settings reach the home; carry tests for the switches; SCN-026,
SCN-057, SCN-058, SCN-080 updated, new SCN for templates.

## Depends on

WF-1.3 (tools). The architect's base for agent templates (`fabric-agents-database`) is consulted
before 10.2.
