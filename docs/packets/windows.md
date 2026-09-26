# Packet WIN-01 — Windows native port

**State:** planned, no Windows compatibility claim. Depends on PA-01 provider contracts, not on copying macOS file semantics.
**Owns:** OS-specific persistence/vault/launcher modules and Tauri Windows distribution. Keep shared route/credential semantics unchanged.
**Shared:** [contracts](../CONTRACTS.md), [spec §12](../SPEC.md), [verification](../evidence/verification.md).

1. Split native filesystem policy behind a platform adapter. Credential Manager item size limits must be measured with the largest supported OAuth JSON; choose DPAPI CurrentUser encrypted blob with a user-only DACL if the item cannot fit. Never silently truncate or fallback to plaintext.
2. Use `%LOCALAPPDATA%/Fabric Switchboard`; reject reparse points and inherited broad ACLs. Test restrictive directory creation before writes, local-only lock semantics and ownership across two Windows user accounts.
3. Replace files with Windows-native replace/flush behavior; preserve the old valid state until success. Inject sharing violations and crashes; bounded retry must not delete the old file first.
4. Launch through Windows Terminal/PowerShell using argv-safe encoding. Scope CODEX_HOME/CLAUDE_CONFIG_DIR only to the child; clear conflicting credentials. Test spaces, apostrophes, Unicode and long paths. Login capture follows the actual Windows client storage contract.
5. Implement native process identity, not PID alone; cancellation never kills unrelated sessions. Keep the proxy bound to loopback and do not request LAN firewall exposure.
6. Package Tauri/WebView2 for Windows 11; explicit runtime dependency handling, keyboard/screen-reader review, signed installer/update chain and uninstall retention policy.

Gate: native vault roundtrip/denial/user separation, interrupted write recovery, two-instance exclusion, actual provider login and request matrix, stream switch, UI and installer checks on Windows. Record exact versions, artifact hash and signing status. A successful Linux/macOS cargo check is not this gate.
