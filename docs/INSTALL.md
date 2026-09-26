# Install Switchboard

Switchboard is a local account workbench for Claude Code and Codex CLI. The desktop app and optional `switchboard` CLI are included in each platform ZIP. You do not need Rust, Node.js or build tools to use these downloads.

Find the selected release and platform links on the [Switchboard download page](https://passioncode.ai/switchboard/#download), or browse [GitHub releases](https://github.com/passioncode-ai/fabric-switchboard/releases). Release publication is separate from a successful build; see the [0.3.1 release record](evidence/release-0.3.1.md) for the current handoff status.

## Before opening a download

Compare the archive's SHA-256 with the checksum in the release notes or the [0.3.1 release record](evidence/release-0.3.1.md#archive-checksums). A matching checksum identifies the downloaded bytes; it does not establish notarization or platform acceptance. If it differs, do not open the archive; download it again from the release page.

For example, from the directory containing the archive:

macOS Terminal:

```sh
shasum -a 256 Fabric-Switchboard-0.3.1-macos-universal.zip
```

Windows PowerShell:

```powershell
Get-FileHash .\Fabric-Switchboard-0.3.1-windows-x64.zip -Algorithm SHA256
```

## macOS

Requires macOS 14 or later. The universal archive contains both Apple silicon and Intel executables.

1. Download the macOS ZIP and compare its checksum.
2. Double-click the ZIP in Finder to extract it. Open the extracted `Fabric-Switchboard-0.3.1-macos-universal` folder.
3. Drag `Fabric Switchboard.app` to your Applications folder, then open it from there.
4. The optional `switchboard` executable is alongside the app. You can keep it in the extracted folder; installing it into your shell's PATH is optional. In Terminal, change to that folder and run `./switchboard --help` to inspect the CLI commands.

**Current beta limit:** the app and CLI are Developer ID signed, but this archive is **not notarized by Apple**. macOS may block the first launch. Signing alone is not Gatekeeper acceptance. If macOS blocks it, keep the security protection in place and wait for a notarized release or [report the exact warning](https://github.com/passioncode-ai/fabric-switchboard/issues). Do not enter credentials into a blocked or unverified copy.

## Windows

The ZIP contains a Windows x64 desktop installer and a separate `switchboard.exe` CLI.

1. Download the Windows ZIP, compare its checksum and use **Extract All** in File Explorer.
2. Open the extracted `Fabric-Switchboard-0.3.1-windows-x64` folder. Run `Fabric Switchboard_0.3.1_x64-setup.exe` to install the desktop app for your user account.
3. Follow the installer prompts. The desktop app requires Microsoft WebView2; the installer handles its bootstrapper when needed. An internet connection may be needed for that runtime download.
4. Open Fabric Switchboard from the Start menu. The separate CLI is optional and can stay in the extracted folder. In PowerShell, change to that folder and run `.\switchboard.exe --help` to inspect commands.

**Current beta limit:** these binaries are **unsigned and cross-built**. Execution and installation on a Windows machine have **not been verified**. SmartScreen or another Windows protection may block them. If blocked, keep the protection in place and wait for a signed, verified release or [report the exact warning](https://github.com/passioncode-ai/fabric-switchboard/issues).

## Start a session

Install the official Claude Code or Codex CLI separately before signing in or launching a coding session. Switchboard does not include a provider subscription or API credits.

Open **Add account** and explicitly capture your current CLI account, or choose the official sign-in flow for another account. Assign a label and pool, such as `work` or `personal`. **Select** sets the account for the next managed request; it does not change the native CLI account. Native Claude activation and automatic rotation are separate opt-in operations.

Keep the desktop app or `switchboard serve` running for managed sessions. The response already in progress keeps its original account. See [CLI commands](CLI.md) and [operations and recovery](OPERATIONS.md).

**Live-provider limit:** login and end-to-end requests with real provider accounts are **not yet verified**. Local synthetic tests and successful packaging do not prove that acceptance. Review the release notes before testing your accounts.

Building from source and signing artifacts are documented separately in [DISTRIBUTION.md](DISTRIBUTION.md).
