# PassionCode visual alignment — 0.3.1

Scope: SCN-023 and shared chrome for SCN-001/013/020. The operator requested yellow S on black and a common PassionCode design system on 2026-09-26. Account, credential, proxy and rotation contracts are unchanged.

## Source and reproduction

- Prior Switchboard implementation: `0b415ef6f5c7b8046afe55152a730fd620afab7d` ([source](https://github.com/passioncode-ai/fabric-switchboard/tree/0b415ef6f5c7b8046afe55152a730fd620afab7d)).
- Canonical system and S mark: website commit `508e91793fcb79d6a59bd2265551dbc76f7a8f97`, exact paths and SHA-256 in [manifest](../../brand/passioncode/manifest.json). Local adapter is [src/tokens.css](../../src/tokens.css). Do not edit vendored bytes independently; refresh from a reviewed canonical website commit and update hashes.
- Native PNG/ICNS/ICO use the same geometry via [generator](../../scripts/generate-icons.mjs), locked Tauri CLI and [generated hashes](../../brand/passioncode/native-icons.json). This is vector rasterization, not generated imagery. Only desktop assets are retained.
- Existing semantic HTML controls remain. Fixed dark default, gold brand/action/next-request roles, blue current-identity role, green operational status, and existing warning/error roles. No animation added. Surface: desktop account workbench; job SCN-020/023; falsifier: selected route confused with current identity, or S identity absent from launcher/sidebar.
- Skill route used: sheleg-design (shared roles and visual review), ux-scenarios (SCN-023 and state boundaries), copywriting (two identity labels, existing action language retained). Tokens lane measured with `npx sshlg-skills pack design --lane tokens`; no new tool/kit installed. Humanization: on, own pass; identity labels require no prose rewrite.

## Checks

- `npm ci`: exit 0, 18 packages, audit 0 vulnerabilities.
- `./scripts/check.sh`: exit 0, TypeScript/Vite build, Rust formatting, 81 tests passed / 1 explicit synthetic Keychain test ignored, Clippy `-D warnings`, 35 Markdown files / 192 relative links / 0 errors at that run, diff whitespace pass.
- `node scripts/check-brand.mjs`: exit 0, 2 canonical sources / 3 native files / all CSS variable references / app and inherited workspace versions verified. `switchboard-core` independently retains 0.1.0.
- Two consecutive `node scripts/generate-icons.mjs` runs: identical native SHA-256 receipts. Initial inspection found Tauri ICNS chunk order nondeterministic; generator now sorts self-contained ICNS chunks by type. PNG/ICO already stable.
- `docs/brand/lint.py`: NOT_RUN (absent in this repository); two new identity labels reviewed against the operator's exact product/family names. Actions/errors are unchanged.

Browser visual review 2026-09-26 at 16:15–16:17 UTC, source: cua in-app browser, route `http://127.0.0.1:5178/?demo=1`, English, dark theme, DPR 1, system fonts loaded, reduced-motion false. Accounts populated state at 1280×720: S mark, gold controls, blue current identity and explicit demo notice visible. At minimum native window 740×560: Accounts and Add account dialog fit; document width exactly 740 (no horizontal overflow), dialog focus ring visible, Activity populated state readable. These are visual judgments and focused DOM observations, not a WCAG conformance claim. Native PNG visually inspected separately: yellow S on dark rounded square. Screenshot source payload digests are in [visual source receipt](design-0.3.1-visual.json). Browser evidence is synthetic only; native rendering and live-provider acceptance are NOT_RUN for this patch. No global credentials or real provider login were accessed.

## Next acceptance / publication

Root release task builds and signs version 0.3.1, verifies app icon and package hashes, publishes the repository/release and deploys the site. This implementation commit alone is not packaging or publication. macOS notarization, Windows native acceptance/signing and live-provider acceptance retain the limitations in [0.3 evidence](release-0.3.md).
