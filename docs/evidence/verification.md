<sub>ssheleg skills — task-pipeline · ux-scenarios · sheleg-design · brand-voice · copywriting</sub>

# Verification — 2026-09-26

Implementation baseline: `a80d86b9a1eded4646bdd9371c5affdce8c93895`. Host measured by `sw_vers`, `uname -m`, `rustc --version`, `node --version`: macOS 26.6.2 / arm64, Rust 1.92.0, Node 26.8.2. This is a local beta verification record, not a claim that live provider integration, native accessibility, signing or Windows are complete.

## Executed checks

| Command or observation | Result | Scope |
|---|---|---|
| `python3 docs/research/check_sources.py /path/to/account-switch-research` | PASS: 4 pinned HEADs, 69 ranges, 69 reference labels | Git objects and line bounds, not live provider behavior |
| `npm ci` | PASS | lockfile install on this host |
| `npm run build` | PASS, TypeScript + Vite | final frontend bundle after launch-dialog and quota-label changes |
| `cargo fmt --all -- --check` | PASS | workspace formatting |
| `cargo test --workspace --locked` | PASS: 8 native, 19 core, 9 proxy tests; 1 opt-in Keychain test excluded from default run | 36 passed; synthetic isolated fixtures |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS | no warnings accepted |
| Explicit synthetic native vault test | PASS: 1 passed | `cargo test -p switchboard-core native_vault_roundtrip_uses_only_random_app_owned_item -- --ignored --exact`; randomly named app-owned item created/read/deleted, no user credential read |
| `npm run app:build` | PASS | release Apple Silicon `.app`, frontend embedded, no Developer ID/notarization claim |
| Browser demo via CUA | PASS for inspected flows | five account cards; selected Personal; added synthetic API-key record; edited/disabled it; Activity showed sanitized events; final managed-launch dialog and staged-login cancellation tested in demo (five accounts retained, focus restored) |
| Brand pack strict linter | PASS | `brand_lint.py docs/brand --strict` from super-ux 0.56.2; source scan explicitly `src/*.ts` |
| Native `.app` window via CUA | BLOCKED | Mac locked; tool could not unlock it; operator unlock requested |
| Real official logins and model responses | NOT_RUN | no authentic test identities entered or provider budget used |
| VoiceOver / computed contrast / Intel / Windows | NOT_RUN | static controls and screenshot inspection are not these checks |
| Hosted nightly CI | NOT_RUN | workflow configuration alone is not a successful hosted run |

The initial combined check failed solely on document links that did not yet exist while this report and packets were being written. Rust tests/lints and frontend were green in that run. After completion, `python3 scripts/check_docs.py` passes with zero errors; strict `ux_lint.py --strict` reports consistent docs/ux; no wrapper exit code is substituted for a failed subcommand.

## Local build artifact

`artifacts/Fabric-Switchboard-0.1.0-macos-arm64.zip` is an ignored local build output, reconstructible from the implementation commit; source delivery is in Git. SHA-256: `efeb9c51b1e0f83dda13555348cc9192145610af200d0eecc8466c5260be6c94`. Executable SHA-256: `82d2d798ea400a8a75e0092388ac48a382ef6036a88c1b0c4ae4b359c20331a1`.

`codesign -dv --verbose=2` reports `Signature=adhoc`, `TeamIdentifier=not set`, `Sealed Resources=none`. This is the local linker signature, **not** Developer ID distribution signing. No notarization was attempted.

## Source and behavioral evidence

All following links address the implementation commit, so subsequent changes cannot silently rewrite the receipt.

| Claim | Receipt |
|---|---|
| Canonical vault uses native Keychain, Windows refuses | [vault.rs](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/crates/switchboard-core/src/vault.rs#L56) |
| Add/update/remove/select and route snapshot | [core store](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/crates/switchboard-core/src/lib.rs#L209); lifecycle/concurrency/fault fixtures in `crates/switchboard-core/tests/storage.rs` |
| First stream remains A, second request becomes B | [auth_is_injected_and_route_changes_only_next_request](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/crates/switchboard-proxy/src/tests.rs#L63) |
| Redirects cannot forward real credential | [redirects_do_not_exfiltrate_credentials](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/crates/switchboard-proxy/src/tests.rs#L292) |
| Incomplete stream is not reported successful | [incomplete_stream_is_aborted_instead_of_success](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/crates/switchboard-proxy/src/tests.rs#L387) |
| Official login capture and cleanup ownership | [launcher](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/src-tauri/src/launch.rs#L209); fixture coverage, real CLI acceptance still NOT_RUN |
| Codex id_token + last_refresh exported correctly | [codex_auth_snapshot](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/src-tauri/src/launch.rs#L365); `codex_export_preserves_native_auth_shape_without_refresh_lineage` |
| Project directory separate from auth home | [launch](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/src-tauri/src/launch.rs#L401); `launch_script_runs_project_without_changing_auth_home` |
| Selection/usage wording and launch dialog | [accountCard](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/src/main.ts#L133), [launchDialog](https://github.com/passioncode-ai/fabric-switchboard/blob/a80d86b9a1eded4646bdd9371c5affdce8c93895/src/main.ts#L277) |

## Scenario coverage

SCN-001: browser empty-state implementation, native observation blocked. SCN-002/004/009/011: browser interaction plus core fixtures. SCN-003/005/006: launcher/config fixtures and IPC implementation; actual official login/launch NOT_RUN. SCN-007: in-flight synthetic HTTP/SSE integration test, not real provider acceptance. SCN-008: parser/bounds/stale UI implementation; authentic usage NOT_RUN. SCN-010/012: store removal/restart/fault tests; GUI removal/restart NOT_RUN. SCN-013: semantic controls/focus implementation and visual browser review; full keyboard/VoiceOver/responsive matrix NOT_RUN. Scenario design status `validated` refers to the authorized brief, never these unobserved outcomes.

## Review findings closed before this baseline

- Codex isolated OAuth originally lacked native identity token; now retained in vault and materialized only in its private working copy.
- Native Codex also requires last_refresh. Snapshot helper now writes RFC3339 local snapshot time, explicitly not a provider refresh; pinned upstream evidence is in SPEC §15.
- Claude beta query and Connection-nominated header stripping added to proxy fixtures.
- Launch now requests a project working directory instead of opening in authentication storage.
- Synchronous launch reservation prevents rewriting a home before Terminal supplies a PID.
- Stream abort and cleanup retry are recorded correctly; login Finish retains saved-account state within the running app.

Remaining limitations and acceptance steps are in [operations](../OPERATIONS.md) and [PA-01](../packets/provider-acceptance.md), not silently promoted to passing tests.

**Made with [ssheleg skills](https://github.com/ssheleg/sshlg-skills)**
