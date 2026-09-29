# Desktop repair and PassionCode 1.1.0 — 0.4 (packet P-B)

Scope: [PLAN-0.4](../PLAN-0.4.md) rows B-02, B-04, B-05, B-11, B-12, B-15, B-16 and REQ-8. Scenarios updated in the same change: SCN-003, SCN-013, SCN-021, SCN-023 ([scenarios](../ux/scenarios.md)). Account, credential, proxy, rotation and IPC contracts are unchanged; no Rust crate was touched. Branch base: `main` `40c8bdb`.

## Design decision

- Route: `sheleg-design` 1.61.1 (product UI, style-pack half only). The visual language is **locked** by the operator's 2026-09-26 request for one PassionCode system ([0.3.1 evidence](design-0.3.1.md)), so the PassionCode token file is the pack; no SHELEG style pack (`workbench` would be the default) replaces it. Mode: update — tokens change, components keep reading roles.
- Dials, announced before the first render: `DESIGN_VARIANCE 4` (existing workbench layout preserved, like the 0.3.1 Accounts screen, not a new composition), `MOTION_INTENSITY 1` (no animation added; the canonical reduced-motion rule already zeroes hover/state durations), `VISUAL_DENSITY 6` (compact account rows as before — the pack's desktop density, not the marketing hero). Held through the change.
- Theme policy: the canonical README (line 29) leaves theme choice to each application and says not to infer an OS policy *from the tokens*. The app makes that choice itself: **System** by default (follows `prefers-color-scheme`, live), with explicit **Dark** and **Light** persisted in `localStorage` under `switchboard.appearance` (every read/write in `try/catch`; failure falls back to System and About says the choice was not saved). Implementation: [ui-logic](../../src/ui-logic.ts), [interface](../../src/main.ts).
- Light fix, not shipped broken: raw gold as text or as a thin line is unreadable on white (canonical README line 33). A new adapter role `--accent-text` aliases `--pc-link` (dark: the same gold; light: `#806200`) and replaces `--accent` for the selected label, active navigation text, selection bar, usage meter fill and checkbox/radio accent. Gold remains the button fill. Focus outlines use `--focus` → `--pc-focus`; addresses use `--link` → `--pc-link` ([adapter tokens](../../src/tokens.css), [styles](../../src/style.css)). Dark rendering is visually unchanged apart from placeholder opacity (0.8 → 1) and the active navigation item keeping its fill on hover.

## Source pin (REQ-8)

- `git -C ~/DATA/passioncode-ai.github.io fetch` then `git show 6085d1073b28dc3b97fb9b029350a038d20afd3c:design-system/tokens.css | shasum -a 256` → `86866df1bec49b85e9def4132021401894483bb819dc2d3a3a70511d2e4b2a61`; the same bytes at `origin/main` `2560662`. Copied byte-for-byte to [brand/passioncode/tokens.css](../../brand/passioncode/tokens.css); [manifest](../../brand/passioncode/manifest.json) now `version 1.1.0`, commit `6085d10…`, that hash.
- Mark: `assets/switchboard-mark.svg` at `6085d10` hashes `0aca33c3…6926`, identical to the vendored mark; native icons unchanged (no regeneration needed).
- [check-brand](../../scripts/check-brand.mjs) now also asserts manifest version 1.1.0, the v1.1.0 header, `--focus`/`--link` aliases, the focus outline role, the System appearance hook, `color-scheme` "dark light" in [index.html](../../index.html), and 52 contrast pairs (below).
- Bundle metadata ([tauri.conf.json](../../src-tauri/tauri.conf.json)): `publisher`, `copyright`, `homepage`, `license`, `licenseFile`, `shortDescription`, `longDescription` — each key checked present in `node_modules/@tauri-apps/cli/config.schema.json` (`BundleConfig.properties`). Version unchanged (0.3.2; P-G bumps).
- About: version, license sentence, LICENSE and THIRD_PARTY_NOTICES addresses on GitHub (both HTTP 200 on 2026-09-29), toolkit address `https://passioncode.ai/switchboard/` (HTTP 200). `src-tauri` has no opener/shell plugin and capability `core:default` only, so the native app renders addresses as selectable text instead of adding a Rust dependency; the browser demo renders `target="_blank" rel="noopener noreferrer"` links.

## Contrast (computed, both themes)

`node scripts/check-brand.mjs --contrast`, WCAG 2.x relative luminance on the resolved role values. Text pairs are held to 1.4.3 AA 4.5:1 (all text here is ≤ 16px, not large); focus and meaningful boundaries/graphics to 1.4.11 AA 3:1. Scoped to token pairs the component layer draws, **not** a whole-app WCAG conformance claim.

| Pair (adapter roles) | Bar | Dark | Light |
|---|---|---|---|
| `--ink on --bg` | text, 4.5:1 | 19.12 | 17.30 |
| `--ink on --panel` | text, 4.5:1 | 18.45 | 15.96 |
| `--ink on --panel-2` | text, 4.5:1 | 17.18 | 17.30 |
| `--muted on --bg` | text, 4.5:1 | 9.02 | 6.73 |
| `--muted on --panel` | text, 4.5:1 | 8.71 | 6.21 |
| `--muted on --panel-2` | text, 4.5:1 | 8.11 | 6.73 |
| `--accent-ink on --accent` | text, 4.5:1 | 12.03 | 12.03 |
| `--accent-ink on --accent-hover` | text, 4.5:1 | 13.27 | 9.87 |
| `--accent-text on --panel` | text, 4.5:1 | 13.32 | 5.29 |
| `--accent-text on --accent-weak` | text, 4.5:1 | 10.19 | 5.09 |
| `--link on --panel` | text, 4.5:1 | 13.32 | 5.29 |
| `--ink on --accent-weak` | text, 4.5:1 | 14.11 | 15.36 |
| `--ink on --ok-weak` | text, 4.5:1 | 14.75 | 15.06 |
| `--ink on --warn-weak` | text, 4.5:1 | 14.42 | 15.45 |
| `--warn on --panel` | text, 4.5:1 | 11.22 | 6.27 |
| `--danger on --panel` | text, 4.5:1 | 9.30 | 6.14 |
| `--danger on --danger-weak` | text, 4.5:1 | 7.72 | 5.68 |
| `--info on --info-weak` | text, 4.5:1 | 8.48 | 5.57 |
| `--info on --panel` | text, 4.5:1 | 10.59 | 5.92 |
| `--focus on --bg` | non-text, 3:1 | 13.81 | 5.73 |
| `--focus on --panel` | non-text, 3:1 | 13.32 | 5.29 |
| `--border-strong on --panel` | non-text, 3:1 | **2.51** (reported, not asserted) | 3.40 |
| `--border-strong on --bg` | non-text, 3:1 | **2.60** (reported, not asserted) | 3.69 |
| `--accent-text on --border` (meter) | non-text, 3:1 | 9.33 | 3.97 |
| `--ok on --panel` (status dot) | non-text, 3:1 | 10.76 | 5.98 |
| `--accent-text on --panel-2` | non-text, 3:1 | 12.41 | 5.73 |

Finding for the design-system owner (P-F / website repository): the canonical **dark** `--pc-border-strong` is 2.51:1 on `--pc-panel`, below 1.4.11 for text-input boundaries; the canonical checker asserts that pair for light only. Switchboard keeps the vendored bytes unchanged and reports the pair rather than overriding a shared role locally. Buttons carry text labels, so the gap affects input fields in dark only.

Before the fix, light would have drawn `--accent` (#ffd21a) as text on `--panel` (#f7f5f8) at 1.34:1, and on `--accent-weak` (#fff2b8, active navigation) at 1.29:1 — the selected label and active navigation would have been unreadable; that is why `--accent-text` exists.

## Defects → evidence

| ID | Change | Evidence |
|---|---|---|
| B-02 | Import Claude Swap inside Add account is hidden and locked once Begin sign-in succeeds; its handler also refuses while a login id exists. Leaving is only through Cancel → `cancelLogin`. | Browser demo: before `importVisible: true`; after Begin sign-in `importHidden: true, locked: "true", disabled: true`, submit “Finish sign-in”; Cancel → 0 dialogs, focus `add-account`. Screenshot `add-signin-pending-dark-740.jpeg`. |
| B-04 | Three messages verbatim (rollback lost lock — safety critical; Claude lock unavailable — from P-A; existing sign-in) and five mapped (identity ambiguous, invalid label/pool/identity, Terminal, sign-in cleanup Keychain, rotation clock). | [adapter](../../src/adapter.ts); `scripts/test-ui-logic.mjs` case 7 asserts each string is in the verbatim set or the mapping table. Backend strings located at `crates/switchboard-runtime/src/{lib,launch,external}.rs`, `crates/switchboard-core/src/{lib,rotation}.rs`. |
| B-05 | `MutationClock`: every mutation (`mutate`, `dialogSave`, import) calls `begin/end`; background reads are stamped and applied only if no mutation began after, or was running at, the stamp. `reload()`'s late context read uses the same guard. | `scripts/test-ui-logic.mjs` case 6 (read before, during, after; overlapping mutations; no underflow). |
| B-11 | Next check shown only for accounts the monitor polls (enabled OAuth, as in `monitor.rs`); others read “Not checked automatically” with the reason. Check usage is not offered for non-OAuth kinds. | Test case 4; `accounts-light-1280.jpeg` (Personal, Experiments, Previous workspace). |
| B-12 | An observation whose `resets_at` or any window reset has passed is stale (“Reset since check”, “% used before reset”), the window reads “usage unknown since reset”, and it is excluded from rotation wording. Demo fixture: archived account reset 10 minutes ago. | Test case 5; `quota-reset-light-740.jpeg`. |
| B-15 | Every focusable control rendered has a focus key (nav, heading, Retry, Dismiss, Stop rotation, radios, links). Background renders build the tree off-DOM and skip replacement when markup is identical. | Browser: focus on `nav-accounts`, Accounts re-rendered at +33 s (relative ages changed), focus still `nav-accounts`; on Activity (no relative times) 62 s later the same DOM node remained (`sameShell: true`) with focus `nav-activity`. |
| B-16 | Dialog ids are `dialog-<n>-title/-description`; the original trigger passes from Add account to Import; a closing dialog restores focus only when no other dialog is open. | Browser: Import opened from Add → labelled by `dialog-3-title`, one `*-title` id in the document, focus inside; Escape → focus `add-account`. |

## Checks run

- `npm ci`: exit 0 (18 packages, 0 vulnerabilities; npm warned that esbuild/fsevents install scripts are not in its allowlist — the build still passed).
- `npm run build`: exit 0. `node scripts/check-brand.mjs`: exit 0 — “PassionCode 1.1.0: 2 canonical sources, 3 native assets, CSS roles, 52 contrast pairs in 2 themes and version 0.3.2 verified”.
- `node scripts/test-ui-logic.mjs`: exit 0 — 7 cases. **Not yet wired into `scripts/check.sh`** (outside this packet's ownership); integration adds one line after `test-read-deadline.mjs`.
- `./scripts/check.sh` on the final source tree (before this evidence file was committed): exit 0 — brand check, build, 5 read-deadline cases, `cargo fmt --check`, `cargo test --workspace --locked` 85 passed / 1 ignored (the opt-in synthetic Keychain test), Clippy `-D warnings`, 41 Markdown files / 256 relative links / 0 errors, third-party notices current, `git diff --check`.

## Browser review

2026-09-29, 16:57–17:05 local, managed headless Chrome 152 through chrome-devtools MCP, route `http://127.0.0.1:1420/?demo=1` (Vite dev), English, DPR 1, `prefers-color-scheme` emulated, isolated browser context, synthetic demo data only. Document width equalled the viewport at 740 and 1280 (no horizontal overflow). Console: Vite connect messages only.

| File | State |
|---|---|
| `design-0.4/accounts-dark-1280.jpeg` | Accounts, System → dark, full page |
| `design-0.4/accounts-light-1280.jpeg` | Accounts, System → light (switched live, no reload), full page |
| `design-0.4/accounts-dark-740.jpeg`, `accounts-light-740.jpeg` | minimum native window 740×560 |
| `design-0.4/about-light-1280.jpeg` | About: appearance and version/license (links, browser demo) |
| `design-0.4/about-dark-740-focus.jpeg` | Dark chosen by keyboard (Tab, Arrow) while the OS is light; focus ring on the radio; stored `dark`, announced “Dark appearance applied.” |
| `design-0.4/add-dialog-light-740-focus.jpeg` | Add account dialog, light, focus on Authentication |
| `design-0.4/add-signin-pending-dark-740.jpeg` | B-02: sign-in pending, Import hidden |
| `design-0.4/quota-reset-light-740.jpeg` | B-12: reset window disclosure, focus on summary |

Two small CSS adjustments landed after some captures: the active navigation item keeps its fill on hover (visible in `about-light-1280.jpeg` as a hovered white About item), and the Import button inside Add account gained 16px spacing above the form grid (`add-dialog-light-740-focus.jpeg` predates it). Both were re-checked by reading the rule, not by a new capture.

## Not verified

- Native rendering in WKWebView (macOS) and WebView2 (Windows), including whether `localStorage` persists across app restarts there and how `prefers-color-scheme` follows the OS inside the webview: NOT_RUN.
- Screen reader behaviour (VoiceOver, Narrator), radio-group announcement: NOT_RUN. Screenshots prove pixels in one state, not the accessibility tree.
- Selecting and copying the native address text: NOT_RUN (browser demo shows links instead).
- Bundle metadata effects (Info.plist copyright, NSIS publisher/licence page): NOT_RUN until P-G builds.
- Live provider behaviour of any B-row: NOT_RUN; all observations use synthetic fixtures.
