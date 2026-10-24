Contract: brand-contract v1
# Brand pack
Sources:
  ui: src/*.ts
  locales: src/locales/*.ts

A local desktop app, its CLI and MCP server. The window ships in English and Russian since 0.6.10
(2026-10-07, SB-76; [CHANGELOG](../../CHANGELOG.md), "Switchboard in Russian"): every string it
shows is a `t('English source')` key with a Russian entry in [src/locales/ru.ts](../../src/locales/ru.ts),
checked by `node scripts/check-locale.mjs`. Two native surfaces carry their own English and Russian outside that check: the tray menu ([src-tauri/src/residency.rs](../../src-tauri/src/residency.rs)) and the folder notifications ([src-tauri/src/suggestions.rs](../../src-tauri/src/suggestions.rs)); strings.md lists them as native strings. Marketing pages, store listings and robots live
outside this repository (the product page is https://passioncode.ai/switchboard/), so those
source categories are omitted here.

Pack: [voice](voice.md), [terminology](terminology.md) (group, automatic switching, API keys),
[facts](facts.md), [channels](channels.md), [interface strings](strings.md), [Russian](locales/ru.md).
