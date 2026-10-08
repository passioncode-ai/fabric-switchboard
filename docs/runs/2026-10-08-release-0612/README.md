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

## Published release

**[v0.6.14](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.6.14) published 2026-10-08 at 14:26:14 UTC**, source `a8618a21b2dae8f94f4de9e70b6ead49457f5395`. All jobs in [run 37789186866](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37789186866) succeeded. The release environment was approved on the operator's explicit instruction to provide the current build.

All 11 public assets downloaded anonymously. Every SHA256SUMS entry matched; GPG verified against the organization's published release key. `gh attestation verify` passed for both ZIPs with signer `passioncode-ai/.github`. The downloaded Mac archive passed `codesign --verify --deep --strict`, `spctl --assess` (Notarized Developer ID) and `xcrun stapler validate`; bundled/standalone CLI both report 0.6.14 and contain arm64 + x86_64. The updater manifest names only 0.6.14 assets with matching signatures. Native Windows tests, installation and installed CLI checks passed in the workflow. [Machine-readable publication receipt](publication.json).

Windows Authenticode remains NOT_SIGNED. Interactive Windows and live-provider acceptance remain separate open checks. No real credentials were read or changed in this release task.

**Exact next action:** merge the documentation receipt and workspace PR #76, then run the dedicated Fabric workspace sync and verify its deployed identity and Switchboard source pin. After publication, the next product work is FD-25 in-place acceptance and the board's remaining operator-assisted checks.

## Screenshots

`docs/evidence/screenshots/accounts-0.6.12.png` is a direct Chrome DevTools viewport capture of the real renderer at `http://127.0.0.1:1426/?demo=1`, 1440 × 1200 CSS pixels, DPR 2, English, system-selected light theme. Captured 2026-10-08 from the 0.6.12 candidate based on `af71a71e2efc468f40cc226b265bed905488cbcd`; only manifests, dependency lock and documentation differ from that renderer source. Accounts / SCN-001, tour dismissed. No compositing or image generation. The synthetic-data banner remains visible. Fonts, current-account panel, pool rows and quota labels were visually inspected; lower accounts continue below the viewport. This is browser-rendering evidence, not native or live-provider acceptance.

Humanization: on; own pass. Reviewed the new English/Russian introduction, feature table and release notes for concrete claims, consistent terms and unsupported superlatives. No claims of measured productivity or live-provider success were added.

Local gate: `./scripts/check.sh` passed, 480 Rust tests passed, 3 opt-in tests ignored. Brand linter clean; documentation links and v0.6.12 release preflight passed. `npm audit` reports zero vulnerabilities after the bounded source-map-js update. [Machine-readable receipt](checks.json).

## Windows fixture correction and 0.6.13

Run [37714019602](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37714019602) for 0.6.11 failed in `an_openrouter_launch_runs_without_an_owner_and_needs_a_saved_key`: the clean Windows runner had no Hermes executable, so discovery refused before reaching the expected missing-key check. The local machine had Hermes, hiding the fixture dependency. The inherited 0.6.12 run was cancelled; both tags remain unchanged. Version 0.6.13 gives that test a private synthetic discovery file and a subprocess-only PATH. No product behavior changes; no real Hermes or provider call. The screenshot remains the exact 0.6.12 renderer; 0.6.13 changes only release metadata, the fixture and documents.

## Windows ownership fixtures and 0.6.14

Run [37787672484](https://github.com/passioncode-ai/fabric-switchboard/actions/runs/37787672484) passed the corrected CLI fixture, then found two runtime fixtures whose files belonged to Administrators on the elevated Windows runner: `a_config_past_one_mebibyte_is_still_read` and `the_native_config_stamp_ignores_unrelated_rewrites`. Their writes now use `private_fs::private_write`, which explicitly creates files owned by the current user's SID. The production reader's strict owner check is unchanged. Version 0.6.13 was cancelled before publication; 0.6.14 carries the fixture corrections.

The README's final dark-theme screenshot is `docs/evidence/screenshots/accounts-0.6.13.png`, captured directly from the same demo route on 2026-10-08 at 1440 × 1200 CSS pixels, DPR 2, English, dark appearance. Source tag `v0.6.13` (`c6b24a3bb423f11658f7dcf960f70dd0c8c8d29f`). Visually reviewed after capture; no image alteration, no real accounts. The 0.6.14 renderer differs only in its version label. The preceding light capture is retained as dated evidence.

## Build cleanup

Removed obsolete local build outputs, the old xwin SDK cache, the unused temporary app bundle and the completed task worktrees' generated files. Allocated size measured immediately before each removal: **7307567104 bytes (6.806 GiB)**. [Itemized receipt](cleanup.json). The temporary app was unregistered from LaunchServices before removal; the task-owned preview server was stopped. The primary checkout's active Vite dependency tree, app data, credentials, live app caches and historical build receipts were preserved. Shared machine-wide Cargo/npm caches and other projects are outside this cleanup.

Fresh source check: an archive of tag `v0.6.14` (`a8618a21b2dae8f94f4de9e70b6ead49457f5395`) passed `check_docs.py` (87 files, 695 links), `check_plugin.py` and release preflight without relying on the worktree's generated files. Native Windows storage/runtime/CLI fixtures passed in run 37789186866; packaging and publication subsequently passed; see the published release above.

Windows artifact check (before publication): `release-windows` from run 37789186866 contains a clean-source receipt for `a8618a21b2dae8f94f4de9e70b6ead49457f5395`, native tests PASS and Authenticode NOT_SIGNED. ZIP integrity passed; SHA-256 `4bf62562c216100b4c3adf4864fa9260764b20301f952345f38aca79c34a7eac` matches that receipt. The workflow's silent installation and installed CLI steps passed. The public download was subsequently verified and has the same hash.
