# 0.6.14 release and documentation refresh

Objective: publish the fixes following 0.6.10, refresh English/Russian repository presentation and the product wiki, verify downloads, and remove obsolete Switchboard build caches.

## Presentation brief

- Surface: GitHub README and repository About, linked product wiki.
- Job: understand which accounts Switchboard manages and find the correct download; screenshot traces Accounts SCN-001 in `docs/ux/scenarios.md`.
- Constraint: existing PassionCode 1.1.0 visual system; real renderer, synthetic demo data only.
- Falsifier: a download resolves to an older release, or a screenshot presents demo accounts as a live authentication proof.
- Taste: preserve the existing workbench in its system-selected light theme, compact account rows and clear status labels; avoid decorative badges and long version-history introductions.
- Mode: update. No app layout or behavior redesign; no reference sweep needed.
- Cast: task-pipeline (delivery), agent-sync (leases), brand-voice (facts/channels), copywriting (English/Russian presentation), sheleg-design (screenshot review), evidence-docs (receipts), maintaining-fabric-workspace (wiki publication).

## Scope and source

Based on main `af71a71e2efc468f40cc226b265bed905488cbcd`, including PRs #120 and #121. Version 0.6.11 was pending protected-environment approval and was never published. The latest public download at task start was 0.6.10.

## Verification and next action

Release preparation in progress. Complete the local gate, merge the release PR, tag its merge, approve the protected release jobs on the operator's explicit release instruction, and verify published bytes before marking 0.6.12 available. Source and screenshots alone do not prove live-provider acceptance.

## Screenshots

`docs/evidence/screenshots/accounts-0.6.12.png` is a direct Chrome DevTools viewport capture of the real renderer at `http://127.0.0.1:1426/?demo=1`, 1440 × 1200 CSS pixels, DPR 2, English, system-selected light theme. Captured 2026-10-08 from the 0.6.12 candidate based on `af71a71e2efc468f40cc226b265bed905488cbcd`; only manifests, dependency lock and documentation differ from that renderer source. Accounts / SCN-001, tour dismissed. No compositing or image generation. The synthetic-data banner remains visible. Fonts, current-account panel, pool rows and quota labels were visually inspected; lower accounts continue below the viewport. This is browser-rendering evidence, not native or live-provider acceptance.

Humanization: on; own pass. Reviewed the new English/Russian introduction, feature table and release notes for concrete claims, consistent terms and unsupported superlatives. No claims of measured productivity or live-provider success were added.

Local gate: `./scripts/check.sh` passed, 480 Rust tests passed, 3 opt-in tests ignored. Brand linter clean; documentation links and v0.6.12 release preflight passed. `npm audit` reports zero vulnerabilities after the bounded source-map-js update. [Machine-readable receipt](checks.json).

## Windows fixture correction and 0.6.13

Run [37714019602](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37714019602) for 0.6.11 failed in `an_openrouter_launch_runs_without_an_owner_and_needs_a_saved_key`: the clean Windows runner had no Hermes executable, so discovery refused before reaching the expected missing-key check. The local machine had Hermes, hiding the fixture dependency. The inherited 0.6.12 run was cancelled; both tags remain unchanged. Version 0.6.13 gives that test a private synthetic discovery file and a subprocess-only PATH. No product behavior changes; no real Hermes or provider call. The screenshot remains the exact 0.6.12 renderer; 0.6.13 changes only release metadata, the fixture and documents.

## Windows ownership fixtures and 0.6.14

Run [37787672484](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37787672484) passed the corrected CLI fixture, then found two runtime fixtures whose files belonged to Administrators on the elevated Windows runner: `a_config_past_one_mebibyte_is_still_read` and `the_native_config_stamp_ignores_unrelated_rewrites`. Their writes now use `private_fs::private_write`, which explicitly creates files owned by the current user's SID. The production reader's strict owner check is unchanged. Version 0.6.13 was cancelled before publication; 0.6.14 carries the fixture corrections.

The README's final dark-theme screenshot is `docs/evidence/screenshots/accounts-0.6.13.png`, captured directly from the same demo route on 2026-10-08 at 1440 × 1200 CSS pixels, DPR 2, English, dark appearance. Source tag `v0.6.13` (`c6b24a3bb423f11658f7dcf960f70dd0c8c8d29f`). Visually reviewed after capture; no image alteration, no real accounts. The 0.6.14 renderer differs only in its version label. The preceding light capture is retained as dated evidence.
