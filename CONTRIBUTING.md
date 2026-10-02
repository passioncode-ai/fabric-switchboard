# Contributing to Fabric Switchboard

This file adds to the organization's default
[CONTRIBUTING.md](https://github.com/passioncode-ai/.github/blob/main/CONTRIBUTING.md); where the
two differ, this file wins.

Thank you for helping. Issues and pull requests are welcome at
<https://github.com/passioncode-ai/fabric-switchboard>.

## License and CLA

Fabric Switchboard is open source under the [GNU AGPL-3.0](LICENSE); a
[commercial license](COMMERCIAL-LICENSE.md) is available from PassionCode.ai.

Contributions are accepted under the [Contributor License Agreement](CLA.md). By opening a
pull request you agree to it — it is what lets every contribution be offered under the
commercial license too. There is no checkbox to tick.

## Before you open a pull request

1. Run `npm ci` once after checkout, then `./scripts/check.sh`. It runs the brand check,
   the UI build, `cargo fmt`, `cargo test`, `cargo clippy`, the documentation link check,
   the third-party notices check and `git diff --check`.
2. A change to dependencies (`Cargo.lock`, `package-lock.json`) regenerates the notices:
   `python3 scripts/third_party_notices.py`, and commits `THIRD_PARTY_NOTICES.md` with it.
3. Follow [AGENTS.md](AGENTS.md): tests use synthetic Vault and upstream fixtures only; never
   real Claude or Codex credentials. No credential, provider prompt, raw upstream error body
   or environment file enters Git, an issue or a pull request.
4. A user-facing change updates [the scenarios](docs/ux/scenarios.md) and its evidence in
   the same pull request.

## Reporting a vulnerability

Do not open a public issue. Use GitHub private vulnerability reporting
(**Security → Report a vulnerability** on the repository), or write to
contact@passioncode.ai.
