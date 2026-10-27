Contract: brand-contract v1
# Terms
Account = one local credential record. Selected = configuration for next request. Managed = local proxy launch. Isolated = new private client home. Verified = authenticated provider evidence only. Never call a local file replacement an instantaneous live switch.

## Group, project, automatic switching, API keys (0.6.82, decisions D-1 and D-2)

Every sentence a person reads uses these words. Command names, MCP tool names and JSON fields
keep their identifiers (a stable API); the words below are what a person sees.

| Term | English | Russian | What it is | Identifiers that stay |
|---|---|---|---|---|
| Group | group, groups | группа, группы | A set of projects (repository folders) with accounts of its own. Sessions launched from its folders use only its accounts, and no other group or the ordinary Claude Code switches to them (window: [src/main.ts](../../src/main.ts) Groups page note). Operator decision SB-146, 2026-10-19: a project in the CLI and MCP is a group. | CLI `switchboard project …` ([main.rs](../../crates/switchboard-cli/src/main.rs) `enum Project`), MCP `switchboard_projects`, `switchboard_project_edit` ([mcp.rs](../../crates/switchboard-cli/src/mcp.rs)); the store's `pool` |
| Group id | group id | идентификатор группы | The store's `pool` value (lowercase letters, digits, `-`, `_`). The word `pool` is shown only where an id is needed, for example `--pool <id>` in a command; a sentence never says pool. | `pool`, `--pool` |
| Default group | the default group | группа по умолчанию | Accounts and folders in no group. A repository in no group stays in the default group and is never made a group of its own ([AGENTS.md](../../AGENTS.md) background monitor row; [mcp.rs](../../crates/switchboard-cli/src/mcp.rs) server instructions). | `default` |
| Project | project | проект | One repository with its worktrees and clones; a project may be in several groups ([README](../../README.md#what-you-can-do)). Never used for a group. | |
| Automatic switching | automatic switching; turn automatic switching on / off | автоматическое переключение; включить / выключить автоматическое переключение. The short form «автопереключение» is allowed where space is short (a label, a badge, a menu item): the window's label `Automatic switching` reads «Автопереключение» ([src/locales/ru.ts](../../src/locales/ru.ts)) | Switchboard moves off an account that nears its limit to the saved account with the most quota left. Never «ротация», never "rotation" in a sentence. | CLI `switchboard rotation …` ([main.rs](../../crates/switchboard-cli/src/main.rs) `enum Rotation`), MCP `switchboard_rotation`, `switchboard_rotation_set`, JSON `rotation` |
| API keys | API keys (the window page; renamed from Tokens in 0.6.82) | API-ключи | The page with the machine's OpenRouter key and each group's own key, which agents started in that group's folders run on (window: `renderTokens` in [src/main.ts](../../src/main.ts)). | page id `tokens` in the window code; CLI `switchboard agents openrouter …` |
| Tokens | tokens | токены | Only counted usage (Statistics: "Tokens per day", "tokens on average") and credential kinds a provider calls tokens ("Claude setup token"). Never the page of keys (CPY-14). | |
| Terminal hook | the terminal hook; Terminals in organization folders (the window section) | привязка терминалов; «Терминалы в папках организаций» (раздел окна) | The marked block Switchboard adds to each shell's start file so that new terminals in an organization's folders run claude and codex in its context (SB-183, SCN-074, SCN-075; window: `shellHookSection` in [src/main.ts](../../src/main.ts)). Never «хук» in a Russian sentence. | CLI `switchboard shell …`, MCP `switchboard_shell_hook`, `switchboard_shell_hook_set`, the marker `shell hook (SB-183)` |
| Start file | start file (`.zshrc`, `.bash_profile`, fish's own file, PowerShell's `profile.ps1`) | файл запуска | The file a shell reads when a terminal opens; Switchboard writes only between its markers and gives the file back byte for byte. | JSON `files[].path` |
| Seat | seat; “If {organization} removed your seat” | место; «Если ваше место в {organization} убрали» | An account an organization gave the person at the provider (a Claude Team or ChatGPT workspace member). It ends when the organization removes it; Switchboard reads that as an ended sign-in and names the organization (SB-160, SCN-078; `seatEndedText` in [src/ui-logic.ts](../../src/ui-logic.ts)). Never «сиденье», never «слот». | none: the code says account |

Russian sentences use the same nouns: «группа» (never «пул» in a sentence), «проект» for a
repository, «автоматическое переключение» / «автопереключение», «API-ключи». The window copy
landed in 0.6.82 (node N5): no Russian window sentence says «пул» or «ротация».

Decision D-6 (orchestrator, 0.6.82, extends D-1): «автопереключение» is allowed in interface
sentences as well as in labels. «ID группы» names the field and the label ([src/locales/ru.ts](../../src/locales/ru.ts)
“Group id”); running text says «идентификатор группы». After a preposition a group takes the
phrase form: «в группе по умолчанию», «в группе «{name}»», «в группе {id} (без папок)» (English
“in the default group”, “in group “{name}””, “in group {id} (no folders)”; `inGroup` in
[src/ui-logic.ts](../../src/ui-logic.ts)); a label shows “Default group” / “Group {id} · no folders”
(`poolName`, decision D-5).

## Product, license, appearance

Product naming: **Fabric Switchboard** is the full product name (window title, bundle, About); **Switchboard** is the short form in running text and the sidebar; the publisher is **PassionCode.ai** (the sidebar family label stays “by PassionCode”, SCN-023). Fabric alone names the CEO agent and the technical kernel, never this app.

License wording: Switchboard is **open source under the GNU AGPL-3.0**; a **commercial license** is available from PassionCode.ai (https://passioncode.ai/business/, commercial@passioncode.ai) for use that does not meet the AGPL's terms. Never call it “source-available”, “MIT” or “PolyForm” for the current version; the one permitted history sentence is [facts](facts.md) F-009.

Appearance: **System**, **Dark**, **Light** (About → Appearance). System follows the operating system; it is not called “Auto”.

## Russian glossary

Russian (since 2026-10-07, SB-76): the interface follows the PassionCode.ai glossary in
fabric-workspace [knowledge/localization.md](https://github.com/passioncode-ai/fabric-workspace/blob/main/knowledge/localization.md):
аккаунт, группа (see the table above; the glossary's «пул» stays for the group id only),
проект, правило, войти / вход, «Переключить», автоматическое переключение / автопереключение
(never «ротация»), использование / квота, сброс (a quota reset; «обновление» is only an app
update), «лимит исчерпан», управляемая / изолированная сессия, цепочка агентов, передача,
резервная копия, API-ключи, «Устанавливать обновления автоматически», «Перезапустить для
обновления», «Завершить», Связка ключей, Терминал. Plain words for technical ones: название
(not label), учётные данные (not credential), системное хранилище (Keychain, Credential Manager,
Secret Service; not OS vault), домашняя папка сессии (not home).
Formal «вы», lowercase. The full text is [src/locales/ru.ts](../../src/locales/ru.ts), keyed by the
English source; `node scripts/check-locale.mjs` (in the gate) fails on a missing entry, a changed
placeholder or a plural without its three forms.
