# Fabric Switchboard — by PassionCode.ai

**English** · [Русский](README.ru.md)

Fabric Switchboard keeps your Claude Code and Codex CLI accounts in one local workbench. See
reported usage, separate work from personal accounts, and choose what handles your next
request. It is a desktop app plus the `switchboard` command-line tool, for macOS and
Windows. It is Fabric's account tool and works on its own; part of the
[PassionCode.ai](https://passioncode.ai/) toolkit.

**Status: beta. [v0.3.1-beta.1](https://github.com/passioncode-ai/fabric-switchboard/releases/tag/v0.3.1-beta.1)
is published** (prerelease, macOS universal + Windows x64); `main` carries 0.3.2 fixes that
are not released yet. Real provider login and end-to-end requests with live accounts are
not yet verified.

- **Download:** [macOS](https://passioncode.ai/switchboard/download/macos) ·
  [Windows](https://passioncode.ai/switchboard/download/windows) ·
  [product page](https://passioncode.ai/switchboard/) ·
  [all releases](https://github.com/passioncode-ai/fabric-switchboard/releases) ·
  [installation notes](docs/INSTALL.md)
- **License:** source-available under PolyForm Noncommercial or Internal Use; commercial
  license on request. See [License](#license).

Saved secrets are protected by macOS Keychain or Windows DPAPI, and work and personal
accounts live in separate pools. In a managed session the selected account changes **from
the next request**: a response already streaming keeps the identity it started with.

Version 0.3.1 brings the shared PassionCode dark design system and the yellow S icon,
explicit capture of the current CLI authorization, Claude Swap import, the active native CLI
profile, quota windows and opt-in background rotation. The interface, CLI, vault, HTTP/SSE
proxy and macOS/Windows adapters are kept. See the [0.3 contract](docs/ACCOUNTS-AND-ROTATION.md)
and the [0.3.1 checks and builds](docs/evidence/release-0.3.1.md). The
[0.2 report](docs/evidence/release-0.2.md) records tests, builds, signing and notarization
separately; the [historical 0.1 verification](docs/evidence/verification.md) is kept.

## Documentation

- [Research on four existing tools](docs/research/README.md): sources, architecture,
  storage and switching mechanics; 69 links to fixed commits.
- [Specification](docs/SPEC.md): features, compatibility axes, states, security,
  protocols, Windows and acceptance criteria.
- [Product map (Russian)](docs/PRODUCT.ru.md): what was built, which ideas were borrowed,
  limits and the order of further work.
- [Entry point for the next contributor or agent](docs/HANDOFF.md): checks, decisions and
  the exact next task.
- [CLI](docs/CLI.md): commands, JSON, stdin and the shared session owner.
- [Distribution](docs/DISTRIBUTION.md): macOS universal app/CLI, signing, Windows
  installer/CLI.

## Running from source

macOS needs macOS 14+, Xcode Command Line Tools, Rust and Node.js. Windows needs Rust
(MSVC), Visual Studio C++ Build Tools, Node.js and WebView2. The official `claude` or
`codex` CLI is needed only to sign in or launch a session, not to run the manager. The
verification status of each platform is in the current build report.

```sh
npm ci
npm run app:dev
```

Build a local app:

```sh
npm run app:build
open 'target/release/bundle/macos/Fabric Switchboard.app'
```

That command alone does not establish Developer ID signing or notarization. The signing
procedure and the separate Windows workflow are in the [distribution guide](docs/DISTRIBUTION.md).
Binaries are not stored in Git. Dependencies are pinned by `Cargo.lock` and
`package-lock.json`; byte-for-byte reproducibility is not claimed.

CLI from source:

```sh
cargo build --release --locked -p switchboard-cli
./target/release/switchboard --help
./target/release/switchboard accounts list
./target/release/switchboard serve
```

`serve`, or an open desktop app, owns the proxy and the sessions. Other CLI commands talk
to that owner automatically. Without an owner, metadata operations run under an exclusive
lock; login and launch need a running runtime.

To look at the interface with synthetic data: `npm run dev`, then
`http://127.0.0.1:1420/?demo=1`. A plain web page does not connect to the vault. The browser
demo has no real accounts, no CLI launch and no network calls to providers.

## Using it

1. **Add account** — official sign-in in a separate profile, an API key, a Claude setup
   token, or explicitly imported OAuth JSON. The app does not read the current global
   authorization on its own.
2. Set a label and a pool, for example `work` or `personal`. The pool is the routing
   boundary; accounts of different providers are not interchangeable.
3. **Select** — choose the account for the next managed requests.
4. **Launch managed** — pick a project folder and open the CLI through the local proxy.
   The app or `switchboard serve` must stay open. One managed home may run per
   provider/pool.
5. **Launch isolated** — a separate home for the selected account and a direct CLI
   connection to the provider. Later **Select** actions do not affect it.
6. **Check usage** — an explicit OAuth quota check. An API key does not report a reliable
   subscription quota; an unknown quota is never shown as zero.

The persistent credential stays in the OS vault, but isolated mode writes the working copy
of the access token that the official CLI needs into a private file (0600 on macOS, a
user-only DACL on Windows). The refresh token is not written there; when the token
expires, sign in again. Details, recovery after a failure and removal:
[operations guide](docs/OPERATIONS.md).

## Checks and layout

```sh
./scripts/check.sh
# The explicitly requested test creates and deletes only a random synthetic Keychain item:
cargo test -p switchboard-core native_vault_roundtrip_uses_only_random_app_owned_item -- --ignored --exact
```

| Path | Responsibility |
|---|---|
| `crates/switchboard-core` | accounts, Keychain/DPAPI, atomic metadata, route snapshot |
| `crates/switchboard-proxy` | loopback authentication, requests, streams, usage |
| `crates/switchboard-runtime` | shared owner, control API, official login and CLI launch |
| `crates/switchboard-cli` | the `switchboard` command, stdin and JSON |
| `src-tauri` | native window and IPC to the shared runtime |
| `src` | TypeScript interface and the clearly labelled demo |
| `docs` | research, specification, scenarios, contracts and evidence |

Full hosted CI runs nightly. The separate Windows build is started by hand for an exact
commit SHA; push and pull requests do not run the full suite. A workflow existing does not
mean it passed. Other projects' sources were studied but are not included as
dependencies.

## License

Switchboard is source-available, not open source: you may use it under the
[PolyForm Noncommercial License 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0)
or the [PolyForm Internal Use License 1.0.0](https://polyformproject.org/licenses/internal-use/1.0.0),
at your option. Commercial distribution, or building it into a product or service for
others, needs a separate commercial license: contact@passioncode.ai. The full terms are in
[LICENSE](LICENSE).

Releases up to and including v0.3.1-beta.1, and commits up to and including `7c36f4a`,
were released under the MIT License; those remain available under MIT.

The built app and CLI include third-party components under their own licenses; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Contributions are accepted under the
[CLA](CLA.md); see [CONTRIBUTING.md](CONTRIBUTING.md).
