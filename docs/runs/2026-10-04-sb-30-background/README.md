# SB-30 — unknown launch arguments and the background start (2026-10-04)

Board row [SB-30](../../evidence/backlog.md), [issue #25](https://github.com/passioncode-ai/fabric-switchboard/issues/25).
Contract: [CONTRACTS](../../CONTRACTS.md#launch-arguments-and-the-background-start-sb-30-2026-10-04);
lifecycle: [AGENTS.md → Lifecycle](../../../AGENTS.md#lifecycle).

## Answers to the issue

| Question | Answer and evidence |
|---|---|
| What does the entry point do with an unknown argument? | Ignores it; only `--smoke-test` and `--background` mean anything (`unknown_arguments_are_ignored_and_background_is_recognised`). It already ignored them before (it only looked for `--smoke-test`); now it is a tested contract. |
| Does Switchboard support `--background`? | Yes. The broker keeps it `always_on` and closing the last window quits it, so without a background start every restart brought a focused window back. |
| No window, no focus? | Window declared hidden and shown only on an ordinary start. macOS: Prohibited activation policy set before the event loop launches. **Measured** with `lsappinfo front` polled every 50 ms during a `--background` smoke of the release bundle: front app unchanged (2 runs); the same with the Accessory policy came forward — which is why Prohibited is used. |
| Quit within LC-01? | `SIGTERM`/`SIGINT` caught before the owner starts (SB-44, `serve_stops_on_sigterm_and_sigint_within_the_deadline` 12/12); Quit and the last window run the one drain. |

## Checks

- `./scripts/check.sh` → exit 0, 347 Rust tests; `scripts/test_smoke_native.py` (a window in the wrong state fails).
- Release bundle, `scripts/smoke_native.py`: ordinary → `window: visible`; `--background` + an unknown argument → `window: hidden`; `LSAppNapIsDisabled` present in the bundle's `Info.plist`.
- One seam review: Accessory set too late and still activating (fixed: Prohibited before launch, measured); "Dock click" wording impossible without a Dock icon (fixed); App Nap (fixed: `LSAppNapIsDisabled`); stale data on reveal (fixed: refresh on `visibilitychange`); Windows has no launch evidence; the broker is not yet enrolled.

## Not exercised

- The live reveal path (open the app again while it runs in the background): a second launch here would hand off to the installed app with the same bundle id. Operator acceptance after install.
- Windows start-up (the Windows jobs build the desktop but never launch it).
- Broker enrolment: `switchboard.desktop` gets `backgroundLaunch` only after a release that ships SB-30 is installed.
