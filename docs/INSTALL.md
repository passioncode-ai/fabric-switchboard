# Install Fabric Switchboard

Fabric Switchboard is a local account workbench for Claude Code and Codex CLI. The desktop app and optional `switchboard` CLI are included in each platform ZIP. You do not need Rust, Node.js or build tools to use these downloads.

Find the selected release and platform links on the [Switchboard download page](https://passioncode.ai/switchboard/#download), or browse [GitHub releases](https://github.com/passioncode-ai/fabric-switchboard/releases). Release publication is separate from a successful build; see the [0.4.1 release record](evidence/release-0.4.1.md) for the current checks.

## Before opening a download

Compare the archive's SHA-256 with `SHA256SUMS-0.4.1.txt` on the release, the release notes or the [0.4.1 release record](evidence/release-0.4.1.md#archive-checksums). A matching checksum identifies the downloaded bytes; it does not establish notarization or platform acceptance. If it differs, do not open the archive; download it again from the release page.

For example, from the directory containing the archive:

macOS Terminal:

```sh
shasum -a 256 -c SHA256SUMS-0.4.1.txt --ignore-missing   # or: shasum -a 256 Fabric-Switchboard-0.4.1-macos-universal.zip
```

Windows PowerShell:

```powershell
Get-FileHash .\Fabric-Switchboard-0.4.1-windows-x64.zip -Algorithm SHA256
```

## macOS

Requires macOS 14 or later. The universal archive contains both Apple silicon and Intel executables.

1. Download the macOS ZIP and compare its checksum.
2. Double-click the ZIP in Finder to extract it. Open the extracted `Fabric-Switchboard-0.4.1-macos-universal` folder.
3. Drag `Fabric Switchboard.app` to your Applications folder, then open it from there.
4. The optional `switchboard` executable is alongside the app. You can keep it in the extracted folder; installing it into your shell's PATH is optional. In Terminal, change to that folder and run `./switchboard --help` to inspect the CLI commands.
5. Updating from 0.4.0 or earlier: quit the old app, replace it, then open the new one once. It moves the accounts the old version saved into the Keychain service `ai.passioncode.fabric-switchboard.shared`, which the app and its bundled CLI both read without asking. macOS may ask at most once per account during the move; choose **Always Allow**. Accounts saved earlier stay readable until the move succeeds; the CLI and `switchboard mcp` never show a Keychain dialog and, for an account not yet moved, say to open the app ([KEYCHAIN.md](KEYCHAIN.md#moving-items-written-by-earlier-versions)).

The app and CLI are Developer ID signed with hardened runtime and **notarized by Apple**; the app carries a stapled ticket. To check before opening, in Terminal: `spctl -a -vv "Fabric Switchboard.app"` should answer `accepted` and `source=Notarized Developer ID`. If macOS blocks the app or shows a different source, do not bypass the protection: download again, compare the checksum and [report the exact warning](https://github.com/passioncode-ai/fabric-switchboard/issues).

### Connect a coding agent (MCP)

1. Put the CLI on your `PATH`, for example `mkdir -p ~/.local/bin && cp switchboard ~/.local/bin/` from the extracted folder (with `~/.local/bin` on `PATH`). The same signed CLI is inside the app at `Fabric Switchboard.app/Contents/MacOS/switchboard`.
2. Either install the plugin (the MCP server plus the `switching-accounts` skill): `claude plugin marketplace add passioncode-ai/fabric-switchboard`, then `claude plugin install switchboard@switchboard`; or register the server alone: `claude mcp add --scope user switchboard -- switchboard mcp` (Codex: `codex mcp add switchboard -- switchboard mcp`).
3. `claude mcp list` shows `switchboard … ✔ Connected`. A safe first call is the read-only `switchboard_accounts` tool; an empty vault answers `{"accounts":[]}`. Switching the ordinary Claude Code login needs an explicit `global: true`. Tools and rules: [CLI — For agents](CLI.md#for-agents).

## Windows

The ZIP contains a Windows x64 desktop installer and a separate `switchboard.exe` CLI.

1. Download the Windows ZIP, compare its checksum and use **Extract All** in File Explorer.
2. Open the extracted `Fabric-Switchboard-0.4.1-windows-x64` folder. Run `Fabric Switchboard_0.4.1_x64-setup.exe` to install the desktop app for your user account.
3. Follow the installer prompts. The desktop app requires Microsoft WebView2; the installer handles its bootstrapper when needed. An internet connection may be needed for that runtime download.
4. Open Fabric Switchboard from the Start menu. The separate CLI is optional and can stay in the extracted folder. In PowerShell, change to that folder and run `.\switchboard.exe --help` to inspect commands.

**Current beta limit:** these binaries are **unsigned and cross-built**. Execution and installation on a Windows machine have **not been verified**. SmartScreen or another Windows protection may block them. If blocked, keep the protection in place and wait for a signed, verified release or [report the exact warning](https://github.com/passioncode-ai/fabric-switchboard/issues).

## Start a session

Install the official Claude Code or Codex CLI separately before signing in or launching a coding session. Switchboard does not include a provider subscription or API credits.

Open **Add account** and explicitly capture your current CLI account, or choose the official sign-in flow for another account. Assign a label and pool, such as `work` or `personal`. **Select** sets the account for the next managed request; it does not change the native CLI account. Native Claude activation and automatic rotation are separate opt-in operations.

Keep the desktop app or `switchboard serve` running for managed sessions. The response already in progress keeps its original account. See [CLI commands](CLI.md) and [operations and recovery](OPERATIONS.md).

**Live-provider limit:** login and end-to-end requests with real provider accounts are **not yet verified**. Local synthetic tests and successful packaging do not prove that acceptance. Review the release notes before testing your accounts.

Building from source and signing artifacts are documented separately in [DISTRIBUTION.md](DISTRIBUTION.md).
