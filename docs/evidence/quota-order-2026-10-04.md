# Quota ordering and system repairs — verification

Source baseline: `46ee4b9a249b3da9612e7ca40308ea12178b92a6`; reviewed change is `codex/quota-order-countdown`, SCN-021/032. Model inherited across root/research/review. [Plan](../runs/2026-10-04-quota-review/PLAN.md), [research](../reports/2026-10-04-system-review/README.md).

## Reproduction and local scope

- `node scripts/test-ui-logic.mjs`: new order regression first failed with creation order placing available after exhausted/unknown rows; after implementation, exit 0, 17 cases. Covers multi-window latest reset, missing reset, hold+quota maximum, failed/stale/future/NaN/disabled/sign-in/API-key, policy max age, input immutability/determinism, negative/past/month-long waits and minute boundaries.
- `partial_headers_never_freshen_an_unobserved_low_quota_window`: failed before age repair (`threshold_reached` versus `no_eligible_account`); delayed-response checks then exposed chronology after successful and failed checks. Rotation+storage after repair: 38 passed, one native acceptance ignored. Authoritative JSON recovery and credential-generation checks preserved.
- Backup same-second regressions failed before generation names/reservation. Full→partial and cross-store copies survive afterward; legacy names restore, existing destination/symlink refused, private permissions preserved. `cargo test -p switchboard-core backup::tests --locked`: exit 0, 14 passed.
- `opaque_conversation_content_cannot_hide_an_old_sessions_start`: failed on content materialization before timestamp-only parser; now old timestamp remains attributable and reset-less old-session marker is excluded. `cargo test -p switchboard-runtime limits`: exit 0, 13 passed.
- Root read the combined diffs; independent session agent reviewed age/credential, backup publication/legacy/pruning, order/hold arithmetic and pause/date handling. A later failed-health chronology issue was found and regressed before final verification. Review is judgment, not provider measurement.

## Rendered inspection

Native Safari, through `cua_repl`; in-app browser unavailable. Explicit browser `?demo=1` supplies synthetic data, with `quota-review=1` adding exhausted multi-window and failed-check states. Viewport fixture [quota-review-viewport.html](quota-review-viewport.html) creates fixed 740 and 1280 CSS-pixel iframes (900px high) without native storage access. The fixture is served by the ordinary dev server; source URL is relative.

Inspected 2026-10-04, en, light/system-light, Safari desktop screenshot (DPR not independently measured), no animation added; revision is the working change from `46ee4b9` (Git records the exact implementation with this evidence). At 740 CSSpx rows use their existing narrow layout; local date and duration wrap in the quota cell. In the large Safari window the date, remaining hours, stale labels, current/selected badges and primary actions remain legible. Pause→Resume retains focus; expanded windows reveal individual date/time and a newly disclosed countdown reads Countdown paused until resume. Metadata refresh and wall-clock ticks are distinct; countdown tick touches text only. No user data is stored in screenshots; screenshots containing unrelated browser chrome are not committed.

Executed browser interactions: 740px fixture, populated/failed/exhausted order, Pause→Resume label and focus, expanded per-window dates, and new disclosure while paused. Large-window screenshot inspected before final accessibility wrapper; 1280px fixture and dark theme were not exercised in this run. Further browser actions stopped when the operator used another tab; no unrelated UI was changed. Final wrapper exposes dates/timers outside the quota button so assistive tools can inspect them individually; final full screenshot after that small semantic change is NOT_RUN. Screenshots are qualitative visual inspection, not screen-reader or native packaged/provider acceptance. Arithmetic tests exercise reset boundaries; a real provider reset was not awaited.

## Not exercised

Real provider login/renewal/rotation/hot adoption/stopped-session continuation, day-long macOS consent/CPU, Windows install/DPAPI interactive acceptance, real client MCP, screen reader, downloaded artifact signatures, release/site/install. Existing SB-01/02/06/15/25/34 remain distinct. No full hosted workflow dispatched for this branch; nightly default-branch CI does not prove it.

## Final local gate

`npm ci` ran once in the isolated checkout. Final `./scripts/check.sh` exited 0:
306 Rust tests passed, two opt-in native tests ignored; 17 UI logic cases passed;
64 Python checks passed; TypeScript/Vite, cargo fmt/clippy, brand/error vocabulary,
plugin/docs/notices and whitespace checks passed. [Captured output](../runs/2026-10-04-quota-review/check-output.txt)
replaces only the checkout's absolute machine path with `<checkout>`.

Additional `brand_lint.py docs/brand --strict`, `ux_lint.py` and `reports.py check`
exited 0. The installed brand linter does not implement `--fail-on B063`; the attempted
flag returned usage error, so the actual check was its supported strict mode.
Final presentation review separates retry hold and quota reset even when the hold is later.
One focused TypeScript build initially caught an unused variable in that adjustment; it was
removed and the build and 17 cases passed before the final full gate.

Final `npm run app:build` exited 0 and produced one local macOS `.app` bundle from the
final source. [Build output](../runs/2026-10-04-quota-review/app-build-output.txt) uses the same
path normalization. This is a development packaging receipt; the bundle was not launched,
installed, notarized or released in this run. The earlier cold build also passed; the final
warm build repeated after the last UI change to ensure the bundled assets matched it.
