use crate::continuation::{Continuation, McpServer};
use serde_json::{json, Value};
#[cfg(any(target_os = "macos", test))]
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, SystemTime},
};
use switchboard_core::{AuthKind, Credential, Provider, Store};
use switchboard_proxy::ProxyHandle;
#[cfg(any(target_os = "macos", test))]
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

pub struct Login {
    pub id: String,
    pub provider: Provider,
    pub label: String,
    pub pool: String,
    pub saved: Option<switchboard_core::Account>,
    /// The signed-in identity was already saved: the sign-in updated it in place (SB-62).
    pub signed_in_again: bool,
    home: PathBuf,
}
const CONFLICTS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_BASE_URL",
    "ANTHROPIC_FOUNDRY_RESOURCE",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "CODEX_API_KEY",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CONFIG_PATH",
    "CLAUDE_SECURESTORAGE_CONFIG_DIR",
    "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
    "SWITCHBOARD_LOCAL_TOKEN",
    "SWITCHBOARD_SESSION",
    "SWITCHBOARD_CONTEXT",
];
#[cfg(windows)]
const CLI_NAME: &str = "switchboard.exe";
#[cfg(not(windows))]
const CLI_NAME: &str = "switchboard";
/// The executable that serves `switchboard mcp`: this process when it is the CLI, the
/// copy bundled beside the desktop executable, or one on the user's PATH.
pub fn agent_cli() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?.canonicalize().ok()?;
    if current.file_name().is_some_and(|n| n == CLI_NAME) {
        return Some(current);
    }
    cli_candidates(&current)
        .into_iter()
        .filter(|p| p.is_absolute() && p.is_file())
        .find_map(|p| p.canonicalize().ok())
}
/// An installed CLI other than `current` (a development build): beside the desktop executable,
/// on the user's PATH or in `~/.local/bin`, found as [`agent_cli`] finds one.
pub(crate) fn installed_cli(current: &Path) -> Option<PathBuf> {
    cli_candidates(current)
        .into_iter()
        .filter(|p| p.is_absolute() && p.is_file())
        .filter_map(|p| p.canonicalize().ok())
        .find(|p| p != current)
}
fn cli_candidates(current: &Path) -> Vec<PathBuf> {
    // Never a translocated copy beside a desktop app opened from its download folder: an MCP
    // entry pointing into it breaks once the app quits.
    let mut candidates: Vec<PathBuf> = current
        .parent()
        .map(|dir| dir.join(CLI_NAME))
        .into_iter()
        .collect();
    candidates.retain(|p| !crate::agents::translocated(p));
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(CLI_NAME)));
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        candidates.push(PathBuf::from(home).join(".local/bin").join(CLI_NAME));
    }
    candidates
}
/// What a launched session needs to reach Switchboard's tools: Claude reads an extra MCP
/// file named on its command line; Codex reads `[mcp_servers]` from its private config.
/// Isolated sessions get the read-only set, since a route change cannot reach them. A session
/// that continues an Observatory workflow (SB-52) also gets the Observatory server there: the
/// user scope is not visible in an isolated home. Every session also gets the Fabric project
/// board (`fabric`, SB-20) when the operator registered it for this provider in their user
/// scope: a runtime or isolated home has a scope of its own, so without it a launched
/// session never sees the board.
struct AgentTools {
    claude_config: Option<PathBuf>,
    codex_section: String,
}
fn agent_tools(
    cli: Option<&Path>,
    root: &Path,
    home: &Path,
    provider: Provider,
    session: &str,
    read_only: bool,
    // Servers beside Switchboard's own, by name: Observatory's for a continuation, the board.
    extra: &[(&str, &McpServer)],
) -> Result<AgentTools, String> {
    // (name, command, args, the session variable when the server is Switchboard's own)
    let mut servers: Vec<(&str, String, Vec<String>, Option<&str>)> = Vec::new();
    if let Some(cli) = cli {
        let mut args = vec![
            "--data-dir".to_string(),
            root.to_string_lossy().into_owned(),
            "mcp".into(),
        ];
        if read_only {
            args.push("--read-only".into());
        }
        servers.push((
            "switchboard",
            cli.to_string_lossy().into_owned(),
            args,
            Some(session),
        ));
    }
    for (name, server) in extra {
        servers.push((name, server.command.clone(), server.args.clone(), None));
    }
    if servers.is_empty() {
        return Ok(AgentTools {
            claude_config: None,
            codex_section: String::new(),
        });
    }
    match provider {
        Provider::Claude => {
            let path = home.join("switchboard-mcp.json");
            let mut entries = serde_json::Map::new();
            for (name, command, args, session) in servers {
                let mut entry = json!({"type": "stdio", "command": command, "args": args});
                if let Some(session) = session {
                    entry["env"] = json!({"SWITCHBOARD_SESSION": session});
                }
                entries.insert(name.into(), entry);
            }
            let config = json!({ "mcpServers": entries });
            private_write(&path, config.to_string().as_bytes(), false)?;
            Ok(AgentTools {
                claude_config: Some(path),
                codex_section: String::new(),
            })
        }
        Provider::Codex => {
            let quote = |v: &str| toml::Value::String(v.into()).to_string();
            let mut section = String::new();
            for (name, command, args, session) in servers {
                let args = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(", ");
                section.push_str(&format!(
                    "\n[mcp_servers.{name}]\ncommand = {}\nargs = [{args}]\n",
                    quote(&command)
                ));
                if let Some(session) = session {
                    section.push_str(&format!(
                        "env = {{ SWITCHBOARD_SESSION = {} }}\n",
                        quote(session)
                    ));
                }
            }
            Ok(AgentTools {
                claude_config: None,
                codex_section: section,
            })
        }
    }
}
/// The name the operator registers Fabric's board server under (`fabric-mcp`, ADR-0134).
const FABRIC_BOARD: &str = "fabric";
/// The board server as the operator registered it in the provider's user scope: Claude Code's
/// `~/.claude.json` `mcpServers.fabric`, Codex's `config.toml` `[mcp_servers.fabric]`. Only a
/// stdio entry with an absolute command is carried, and only its command and arguments: an
/// `env` may hold a secret, and a secret is never copied into a launched home. Absent,
/// unreadable or half-written means no board, never a failed launch.
fn user_board_server(
    provider: Provider,
    home: &Path,
    codex_home: Option<&Path>,
) -> Option<McpServer> {
    let strings = |args: Option<Vec<Option<String>>>| -> Option<Vec<String>> {
        args.unwrap_or_default().into_iter().collect()
    };
    let (command, args) = match provider {
        Provider::Claude => {
            use std::io::Read;
            let mut bytes = Vec::new();
            fs::File::open(home.join(".claude.json"))
                .ok()?
                .take(32 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .ok()?;
            let user: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            let entry = user.get("mcpServers")?.get(FABRIC_BOARD)?;
            if entry.get("type").is_some_and(|t| t != "stdio") {
                return None;
            }
            let args = entry.get("args").map(|a| {
                a.as_array()
                    .map(|a| a.iter().map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_else(|| vec![None])
            });
            (entry.get("command")?.as_str()?.to_string(), strings(args)?)
        }
        Provider::Codex => {
            let dir = codex_home
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".codex"));
            let config: toml::Table = fs::read_to_string(dir.join("config.toml"))
                .ok()?
                .parse()
                .ok()?;
            let entry = config.get("mcp_servers")?.get(FABRIC_BOARD)?;
            // A remote server, or one the operator switched off, is not carried.
            if entry.get("url").is_some()
                || entry.get("enabled").and_then(toml::Value::as_bool) == Some(false)
            {
                return None;
            }
            let args = entry.get("args").map(|a| {
                a.as_array()
                    .map(|a| a.iter().map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_else(|| vec![None])
            });
            (entry.get("command")?.as_str()?.to_string(), strings(args)?)
        }
    };
    Path::new(&command)
        .is_absolute()
        .then_some(McpServer { command, args })
}
/// The board server of this machine's operator, read from their real user scope.
pub(crate) fn board_server(provider: Provider) -> Option<McpServer> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    let codex_home = crate::person::codex_home();
    user_board_server(provider, Path::new(&home), codex_home.as_deref())
}
pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    switchboard_core::private_fs::private_dir(path)
}
pub(crate) fn private_write(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    switchboard_core::private_fs::private_write(path, bytes)?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Managed file permissions unavailable.")?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}
/// The shell a session script runs in: zsh, macOS's own, on macOS; POSIX `sh` elsewhere, since
/// many Linux systems have no zsh (SB-88). The script uses only POSIX constructs in either.
#[cfg(target_os = "macos")]
pub(crate) const SCRIPT_SHELL: &str = "/bin/zsh";
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const SCRIPT_SHELL: &str = "/bin/sh";
#[cfg(unix)]
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn binary(provider: Provider) -> Result<PathBuf, String> {
    find_program(provider.as_str())
        .ok_or_else(|| "Provider CLI not found. Install the official CLI and retry.".into())
}
/// An executable named `name` on PATH or in the usual per-user and Homebrew folders.
pub(crate) fn find_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|s| std::env::split_paths(&s).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
        ]);
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    // After Homebrew, so a program found before keeps being the one launched: where agents' own
    // installers put theirs, which the desktop app's PATH lacks (SB-120: OpenCode's is
    // ~/.opencode/bin, Kimi Code's ~/.kimi-code/bin). Version-manager shims (Volta, mise, asdf)
    // are left out: they resolve to the manager, not the agent.
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".opencode/bin"),
            home.join(".kimi-code/bin"),
            home.join(".hermes/bin"),
            home.join(".deno/bin"),
            home.join("Library/pnpm"),
            home.join(".yarn/bin"),
        ]);
    }
    #[cfg(windows)]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            dirs.push(PathBuf::from(home).join(".local/bin"));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    for dir in dirs {
        #[cfg(windows)]
        for extension in ["exe", "cmd", "ps1"] {
            let path = dir.join(format!("{name}.{extension}"));
            if path.is_file() {
                return path.canonicalize().ok();
            }
        }
        #[cfg(not(windows))]
        {
            let path = dir.join(name);
            if path.is_file() {
                return path.canonicalize().ok();
            }
        }
    }
    None
}
/// A variable the session script fills from a command's output when it starts — a secret the
/// script must not hold (SB-79, XA-02 A-2): `(name, program, arguments)`.
type FromCommand<'a> = Option<(&'a str, &'a Path, &'a [String])>;
/// Printed by a session script that could not read its key; names the fix, never the key.
const KEY_UNREADABLE: &str =
    "Switchboard could not read the OpenRouter key. Check it on the API keys page, then launch again.";
#[cfg(not(windows))]
pub(crate) fn script(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    login: bool,
    working_directory: &Path,
) -> String {
    script_with(home, program, args, env, None, login, working_directory)
}
#[cfg(not(windows))]
fn script_with(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    from_command: FromCommand,
    login: bool,
    working_directory: &Path,
) -> String {
    let mut result = format!("#!{SCRIPT_SHELL}\nset +x\nunset {}\n", CONFLICTS.join(" "));
    for (key, value) in env {
        result.push_str(&format!("export {key}={}\n", quote(value)));
    }
    if let Some((name, command, arguments)) = from_command {
        let mut call = quote(command.to_string_lossy().as_ref());
        for argument in arguments {
            call.push(' ');
            call.push_str(&quote(argument));
        }
        result.push_str(&format!(
            "{name}=\"$({call})\" && [ -n \"${name}\" ] || {{ printf '%s\\n' {} >&2; exit 1; }}\nexport {name}\n",
            quote(KEY_UNREADABLE)
        ));
    }
    result.push_str(&format!(
        "cd {} || exit 1\n",
        quote(home.to_string_lossy().as_ref())
    ));
    result.push_str("(umask 077; printf '%s' \"$$\" > .session-pid)\nrm -f .launch-pending\n");
    if !login {
        result.push_str(&format!(
            "cd {} || exit 1\nexec ",
            quote(working_directory.to_string_lossy().as_ref())
        ));
    }
    result.push_str(&quote(program.to_string_lossy().as_ref()));
    for arg in args {
        result.push(' ');
        result.push_str(&quote(arg));
    }
    result.push('\n');
    if login {
        result.push_str("result=$?\nif [ \"$result\" -eq 0 ]; then\n  (umask 077; printf 'complete' > .completed)\nfi\nexit \"$result\"\n");
    }
    result
}
/// A launch script that runs `program first…` to its end, then — only when it succeeded — hands
/// the shell to `program then…` (SB-99: a Kimi continuation's `kimi -p <prompt>`, then `kimi -c`).
/// A first turn that failed resumes nothing: `kimi -c` would open the folder's previous session,
/// which may belong to other work.
#[cfg(not(windows))]
pub(crate) fn script_then(
    home: &Path,
    program: &Path,
    first: &[&str],
    then: &[&str],
    env: &BTreeMap<String, String>,
    working_directory: &Path,
) -> String {
    let script = script(home, program, then, env, false, working_directory);
    // The `exec` follows the change into the working directory, which `script` writes exactly
    // so: found by that whole line, not by a word a quoted path could also contain.
    let marker = format!(
        "cd {} || exit 1\nexec ",
        quote(working_directory.to_string_lossy().as_ref())
    );
    let at = script
        .rfind(&marker)
        .map_or(script.len(), |at| at + marker.len() - "exec ".len());
    let mut call = quote(program.to_string_lossy().as_ref());
    for arg in first {
        call.push(' ');
        call.push_str(&quote(arg));
    }
    format!(
        "{}if ! {call}; then\n  printf '%s\\n' {} >&2\n  exit 1\nfi\n{}",
        &script[..at],
        quote(FIRST_TURN_FAILED),
        &script[at..]
    )
}
#[cfg(any(windows, test))]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
#[cfg(any(windows, test))]
fn windows_script(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    login: bool,
    working_directory: &Path,
) -> String {
    windows_script_with(home, program, args, env, None, login, working_directory)
}
#[cfg(any(windows, test))]
fn windows_script_with(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    from_command: FromCommand,
    login: bool,
    working_directory: &Path,
) -> String {
    let q = |p: &Path| powershell_quote(&p.to_string_lossy());
    let mut result = String::from("$ErrorActionPreference = 'Stop'\nSet-PSDebug -Off\n");
    for variable in CONFLICTS {
        result.push_str(&format!(
            "[Environment]::SetEnvironmentVariable({}, $null, 'Process')\n",
            powershell_quote(variable)
        ));
    }
    for (key, value) in env {
        result.push_str(&format!(
            "[Environment]::SetEnvironmentVariable({}, {}, 'Process')\n",
            powershell_quote(key),
            powershell_quote(value)
        ));
    }
    if let Some((name, command, arguments)) = from_command {
        let arguments = arguments
            .iter()
            .map(|a| powershell_quote(a))
            .collect::<Vec<_>>()
            .join(", ");
        result.push_str(&format!(
            "$global:LASTEXITCODE = 0\n$switchboardSecret = & {} @({arguments})\nif ($LASTEXITCODE -ne 0 -or -not $switchboardSecret) {{ [Console]::Error.WriteLine({}); exit 1 }}\n[Environment]::SetEnvironmentVariable({}, [string]$switchboardSecret, 'Process')\nRemove-Variable switchboardSecret\n",
            q(command),
            powershell_quote(KEY_UNREADABLE),
            powershell_quote(name)
        ));
    }
    result.push_str(&format!("$homePath = {}\n$marker = Join-Path $homePath '.session-process'\n$process = Get-Process -Id $PID\n@{{ pid = $PID; created = $process.StartTime.ToUniversalTime().ToFileTimeUtc() }} | ConvertTo-Json -Compress | Set-Content -LiteralPath $marker -Encoding ASCII\nRemove-Item -LiteralPath (Join-Path $homePath '.launch-pending') -Force\nSet-Location -LiteralPath {}\n",q(home),q(working_directory)));
    let arguments = args
        .iter()
        .map(|a| powershell_quote(a))
        .collect::<Vec<_>>()
        .join(", ");
    result.push_str(&format!("$arguments = @({arguments})\n$global:LASTEXITCODE = 0\n& {} @arguments\n$result = $LASTEXITCODE\n",q(program)));
    if login {
        result.push_str("if ($result -eq 0) { [IO.File]::WriteAllText((Join-Path $homePath '.completed'), 'complete') }\n");
    }
    result.push_str("exit $result\n");
    result
}
#[cfg(windows)]
pub(crate) fn script(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    login: bool,
    working_directory: &Path,
) -> String {
    windows_script(home, program, args, env, login, working_directory)
}
#[cfg(windows)]
fn script_with(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    from_command: FromCommand,
    login: bool,
    working_directory: &Path,
) -> String {
    windows_script_with(
        home,
        program,
        args,
        env,
        from_command,
        login,
        working_directory,
    )
}
/// A reservation guards only the window between writing a home and its script removing
/// the marker; a Terminal that never ran the script must not hold the home forever.
const RESERVATION_TTL: Duration = Duration::from_secs(10 * 60);
/// A marker dated ahead of the clock (a clock set back) has no knowable age, so it expires
/// by the same distance rather than holding the home until the clock catches up.
fn reservation_expired(written: SystemTime, now: SystemTime) -> bool {
    match now.duration_since(written) {
        Ok(age) => age > RESERVATION_TTL,
        Err(ahead) => ahead.duration() > RESERVATION_TTL,
    }
}
pub(crate) struct Reservation {
    path: PathBuf,
    pub(crate) committed: bool,
}
impl Reservation {
    pub(crate) fn new(home: &Path) -> Result<Self, String> {
        ensure_idle(home)?;
        let path = home.join(".launch-pending");
        switchboard_core::private_fs::create_new(&path)
            .map_err(|_| "Close the existing session before changing its home.")?;
        Ok(Self {
            path,
            committed: false,
        })
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// What a launch needs from outside its private root: the provider CLI, the Switchboard
/// CLI its tools run from, and a Terminal for the written script. Tests substitute all
/// three, so no real CLI is resolved and no window opens.
struct Host {
    binary: fn(Provider) -> Result<PathBuf, String>,
    agent_cli: fn() -> Option<PathBuf>,
    board: fn(Provider) -> Option<McpServer>,
    start_terminal: fn(&Path) -> Result<(), String>,
    /// The operator's own user folder for the provider (`~/.claude`, `~/.codex`), carried into a
    /// launched home (SB-137); None: nothing is carried.
    user_scope: fn(Provider) -> Option<PathBuf>,
}
const NATIVE: Host = Host {
    binary,
    agent_cli,
    board: board_server,
    start_terminal,
    user_scope,
};
fn user_scope(provider: Provider) -> Option<PathBuf> {
    match provider {
        Provider::Claude => crate::carry::claude_user_dir(),
        Provider::Codex => crate::person::codex_dir(),
    }
}
pub(crate) fn open_terminal(home: &Path, content: &str) -> Result<(), String> {
    start_terminal(&write_launch_script(home, content)?)
}
/// After the proxy had to move from `old` to `new` (its port was taken), points the files
/// Switchboard generated in managed homes (launch scripts and Codex's provider config) and in
/// the organizations' Codex shell homes (SB-183) at the new port, before the first bindings pass
/// rewrites the shell homes anyway. Only the exact loopback address is replaced; nothing else is
/// touched. Returns how many files changed.
pub(crate) fn repoint_managed(root: &Path, old: u16, new: u16) -> usize {
    let from = format!("http://127.0.0.1:{old}/");
    let to = format!("http://127.0.0.1:{new}/");
    let homes = ["runtimes", crate::shell::map::DIR]
        .iter()
        .filter_map(|dir| fs::read_dir(root.join(dir)).ok())
        .flat_map(|entries| entries.flatten());
    let mut changed = 0;
    for home in homes {
        if !home.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for (name, executable) in [
            ("launch.command", true),
            ("launch.ps1", true),
            ("config.toml", false),
        ] {
            let path = home.path().join(name);
            let Ok(bytes) = switchboard_core::private_fs::read_private(&path, 256 * 1024) else {
                continue;
            };
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            if !text.contains(&from) {
                continue;
            }
            if private_write(&path, text.replace(&from, &to).as_bytes(), executable).is_ok() {
                changed += 1;
            }
        }
    }
    changed
}
pub(crate) fn write_launch_script(home: &Path, content: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let path = home.join("launch.ps1");
    #[cfg(not(windows))]
    let path = home.join("launch.command");
    #[cfg(windows)]
    let content = format!("\u{feff}{content}"); // Windows PowerShell 5.1 needs a UTF-8 BOM for Unicode paths.
    private_write(&path, content.as_bytes(), true)?;
    Ok(path)
}
pub(crate) fn start_terminal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/open")
            .args(["-a", "Terminal"])
            .arg(path)
            .status()
            .map_err(|_| "Terminal could not open.")?;
        if !status.success() {
            return Err("Terminal could not open.".into());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let shell = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or("Windows system directory unavailable.")?,
        )
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = Command::new(shell);
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(path)
            .creation_flags(0x00000010); // CREATE_NEW_CONSOLE; script waits for provider.
        for variable in CONFLICTS {
            command.env_remove(variable);
        }
        command.spawn().map_err(|_| "Terminal could not open.")?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        spawn_linux_terminal(path, std::env::var_os("TERMINAL"), find_program)
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        let _ = path;
        Err("Terminal launch is currently supported on macOS, Windows and Linux only.".into())
    }
}
/// Opens `path` in the Linux terminal `linux_terminal` chooses; the seam the Linux test drives.
#[cfg(target_os = "linux")]
fn spawn_linux_terminal(
    path: &Path,
    preferred: Option<std::ffi::OsString>,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<(), String> {
    {
        use std::os::unix::process::CommandExt;
        let (program, arguments) = linux_terminal(preferred, find)?;
        let mut command = Command::new(program);
        command
            .args(arguments)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // Its own process group: the Terminal outlives Switchboard, as on macOS.
            .process_group(0);
        for variable in CONFLICTS {
            command.env_remove(variable);
        }
        let mut child = command.spawn().map_err(|_| "Terminal could not open.")?;
        // Reaped when it exits, so no zombie stays behind for a server-style terminal either.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
/// No terminal emulator could be found on Linux (SB-88). Shown verbatim (src/adapter.ts).
pub const LINUX_TERMINAL_MISSING: &str = "No terminal app was found. Install one, such as GNOME Terminal, Konsole or xterm, or set TERMINAL to yours, then retry.";
/// Linux terminal emulators and how each is told to run one program: `$TERMINAL` first (a known
/// name gets its own arguments, an unknown one the common `-e`), then Debian's
/// `x-terminal-emulator` alternative, then the common desktops' own terminals.
#[cfg(any(target_os = "linux", test))]
const LINUX_TERMINALS: [(&str, &[&str]); 11] = [
    ("x-terminal-emulator", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("mate-terminal", &["-x"]),
    ("tilix", &["-e"]),
    ("kitty", &[]),
    ("alacritty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("foot", &[]),
    ("xterm", &["-e"]),
];
#[cfg(any(target_os = "linux", test))]
fn linux_terminal(
    preferred: Option<std::ffi::OsString>,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<(PathBuf, &'static [&'static str]), String> {
    if let Some(preferred) = preferred.filter(|p| !p.is_empty()) {
        let preferred = PathBuf::from(preferred);
        let name = preferred
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let arguments = LINUX_TERMINALS
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(&["-e"][..], |(_, arguments)| *arguments);
        let program = if preferred.is_absolute() {
            preferred.is_file().then_some(preferred)
        } else {
            find(&name)
        };
        if let Some(program) = program {
            return Ok((program, arguments));
        }
    }
    LINUX_TERMINALS
        .iter()
        .find_map(|(name, arguments)| find(name).map(|program| (program, *arguments)))
        .ok_or_else(|| LINUX_TERMINAL_MISSING.into())
}
/// An empty sign-in label is allowed: the captured email names the account at finish.
fn validate_fields(label: &str, pool: &str) -> Result<(), String> {
    if label.len() > 80 || label.chars().any(char::is_control) {
        return Err("Enter a label of up to 80 characters.".into());
    }
    if pool.is_empty()
        || pool.len() > 32
        || !pool
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err("Pool must use lowercase letters, digits, hyphens or underscores.".into());
    }
    Ok(())
}
pub fn begin_login(
    root: &Path,
    provider: Provider,
    label: String,
    pool: String,
) -> Result<Login, String> {
    validate_fields(&label, &pool)?;
    let program = binary(provider)?;
    private_dir(&root.join("logins"))?;
    let id = Uuid::new_v4().to_string();
    let home = root.join("logins").join(&id);
    private_dir(&home)?;
    let mut reservation = Reservation::new(&home)?;
    let env = BTreeMap::from([(
        match provider {
            Provider::Claude => "CLAUDE_CONFIG_DIR",
            Provider::Codex => "CODEX_HOME",
        }
        .into(),
        home.to_string_lossy().into_owned(),
    )]);
    let args = match provider {
        Provider::Claude => vec!["auth", "login"],
        Provider::Codex => {
            // A private config file avoids shell-dependent quote handling of TOML CLI arguments.
            private_write(
                &home.join("config.toml"),
                b"cli_auth_credentials_store = \"file\"\n",
                false,
            )?;
            vec!["login"]
        }
    };
    // SB-65: what this sign-in is for, so an owner started after a quit or crash can take it
    // back (`adopt_logins`) — the provider, label and pool only.
    let journal = json!({
        "provider": provider.as_str(),
        "label": label,
        "pool": pool,
        "started_at": switchboard_proxy::now(),
    });
    private_write(
        &home.join(LOGIN_JOURNAL),
        journal.to_string().as_bytes(),
        false,
    )?;
    open_terminal(&home, &script(&home, &program, &args, &env, true, &home))?;
    reservation.committed = true;
    Ok(Login {
        id,
        provider,
        label,
        pool,
        saved: None,
        signed_in_again: false,
        home,
    })
}
#[cfg(test)]
pub(crate) fn fixture_login(home: PathBuf, saved: switchboard_core::Account) -> Login {
    Login {
        id: Uuid::new_v4().to_string(),
        provider: Provider::Codex,
        label: saved.label.clone(),
        pool: saved.pool.clone(),
        saved: Some(saved),
        signed_in_again: false,
        home,
    }
}
#[cfg(any(target_os = "macos", test))]
fn keychain_service(home: &Path) -> String {
    let raw: String = home.to_string_lossy().nfc().collect();
    format!(
        "Claude Code-credentials-{}",
        &format!("{:x}", Sha256::digest(raw.as_bytes()))[..8]
    )
}
/// The Keychain account Claude Code files its login item under. This MUST match
/// `crate::external::username()` — `$USER` when set and non-empty, then the passwd entry
/// of the effective user — or a sign-in is captured under one name and cleaned under another.
#[cfg(target_os = "macos")]
fn login_user() -> Result<String, String> {
    pick_user(std::env::var("USER").ok(), passwd_user)
}
#[cfg(any(target_os = "macos", test))]
fn pick_user(
    environment: Option<String>,
    passwd: impl FnOnce() -> Option<String>,
) -> Result<String, String> {
    environment
        .filter(|user| !user.is_empty())
        .or_else(|| passwd().filter(|user| !user.is_empty()))
        .ok_or_else(|| "Local login user unavailable.".into())
}
#[cfg(target_os = "macos")]
fn passwd_user() -> Option<String> {
    // getpwuid returns null or static storage valid until the next passwd call; the name
    // is copied out before returning.
    let pw = unsafe { libc::getpwuid(libc::geteuid()) };
    (!pw.is_null()).then(|| {
        unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
            .to_string_lossy()
            .into_owned()
    })
}
fn read_regular(path: &Path) -> Result<String, String> {
    let bytes = switchboard_core::private_fs::read_private(path, 1024 * 1024).map_err(|_| {
        "Sign-in is not complete or its private file is unsafe. Finish in Terminal, then try again."
    })?;
    String::from_utf8(bytes).map_err(|_| "Sign-in credential is unsupported.".into())
}
pub fn capture_login(login: &Login) -> Result<Credential, String> {
    if read_regular(&login.home.join(".completed"))? != "complete" {
        return Err("Sign-in is not complete. Finish in Terminal, then try again.".into());
    }
    let material = match login.provider {
        Provider::Codex => read_regular(&login.home.join("auth.json"))?,
        Provider::Claude => {
            #[cfg(target_os = "macos")]
            {
                let user = login_user()?;
                // Claude Code wrote this item with /usr/bin/security; reading it the same
                // way is silent (PLAN-0.5 C-3).
                let bytes = crate::external_keychain::read(&keychain_service(&login.home), &user)
                    .ok()
                    .flatten()
                    .ok_or(
                        "Sign-in credential unavailable. Check Keychain access and finish login.",
                    )?;
                String::from_utf8(bytes).map_err(|_| "Sign-in credential is unsupported.")?
            }
            #[cfg(not(target_os = "macos"))]
            {
                read_regular(&login.home.join(".credentials.json"))?
            }
        }
    };
    Credential::parse(login.provider, AuthKind::OAuth, &material)
}
/// Where an official sign-in stands, read from its private home only — no credential.
/// `ended` means the Terminal session exited without the provider CLI succeeding.
pub fn login_state(login: &Login) -> &'static str {
    if read_regular(&login.home.join(".completed")).is_ok_and(|v| v == "complete") {
        return "complete";
    }
    #[cfg(unix)]
    let marker = login.home.join(".session-pid");
    #[cfg(windows)]
    let marker = login.home.join(".session-process");
    if marker.exists() && ensure_idle(&login.home).is_ok() {
        return "ended";
    }
    "pending"
}
pub fn capture_login_profile(login: &Login) -> Result<crate::external::CapturedProfile, String> {
    if read_regular(&login.home.join(".completed"))? != "complete" {
        return Err("Sign-in is not complete. Finish in Terminal, then try again.".into());
    }
    crate::external::capture_at(login.provider, &login.home)
}
pub fn cancel_login(login: &Login) -> Result<(), String> {
    end_sign_in(&login.home)?;
    // A cancelled/failed sign-in may never have created a vault item.
    #[cfg(target_os = "macos")]
    if login.provider == Provider::Claude {
        let user = login_user()?;
        crate::external_keychain::delete(&keychain_service(&login.home), &user)
            .map_err(|_| "Sign-in cleanup needs Keychain access.")?;
    }
    fs::remove_dir_all(&login.home).map_err(|_| "Sign-in cleanup failed.".into())
}
pub fn clean_login(login: &Login) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if login.provider == Provider::Claude {
        let user = login_user()?;
        crate::external_keychain::delete(&keychain_service(&login.home), &user)
            .map_err(|_| "Account saved; isolated login cleanup needs attention.")?;
    }
    match fs::remove_dir_all(&login.home) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Account saved; isolated login cleanup needs attention.".into()),
    }
}
/// Written into a sign-in's home before Terminal opens (SB-65): provider, label, pool, time.
pub(crate) const LOGIN_JOURNAL: &str = ".login.json";
/// How many sign-ins one owner holds at once (`begin_login` refuses a fifth).
pub(crate) const MAX_LOGINS: usize = 4;
fn login_completed(home: &Path) -> bool {
    read_regular(&home.join(".completed")).is_ok_and(|v| v == "complete")
}
pub(crate) fn reservation_open(home: &Path) -> bool {
    fs::symlink_metadata(home.join(".launch-pending")).is_ok_and(|meta| {
        meta.file_type().is_file()
            && !meta
                .modified()
                .is_ok_and(|t| reservation_expired(t, SystemTime::now()))
    })
}
/// A sign-in's journal, when it names a known provider and a valid label and pool.
fn login_journal(home: &Path) -> Option<(Provider, String, String, i64)> {
    let journal: Value =
        serde_json::from_str(&read_regular(&home.join(LOGIN_JOURNAL)).ok()?).ok()?;
    let provider = match journal["provider"].as_str()? {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return None,
    };
    let (label, pool) = (journal["label"].as_str()?, journal["pool"].as_str()?);
    validate_fields(label, pool).ok()?;
    Some((
        provider,
        label.to_owned(),
        pool.to_owned(),
        journal["started_at"].as_i64().unwrap_or(0),
    ))
}
/// SB-103: once a sign-in's account is saved its journal goes first, so no later start can
/// take it back — even when the rest of its folder cannot be removed yet.
pub(crate) fn forget_login_journal(login: &Login) {
    let _ = fs::remove_file(login.home.join(LOGIN_JOURNAL));
}
/// When a sign-in began, from its journal (0 when it has none), to order them.
pub(crate) fn login_started(login: &Login) -> i64 {
    login_journal(&login.home).map_or(0, |(.., at)| at)
}
/// The sign-ins an earlier owner started that can still be finished: their journal is readable
/// and the sign-in either completed in Terminal or is still open there. Newest first, at most
/// `MAX_LOGINS`. The rest stay for `sweep_logins`.
pub fn adopt_logins(root: &Path) -> Vec<Login> {
    let Ok(entries) = fs::read_dir(root.join("logins")) else {
        return Vec::new();
    };
    let mut found: Vec<(i64, Login)> = Vec::new();
    for entry in entries.flatten() {
        let Some(id) = entry
            .file_name()
            .to_str()
            .filter(|name| Uuid::parse_str(name).is_ok())
            .map(str::to_owned)
        else {
            continue;
        };
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let home = entry.path();
        let Some((provider, label, pool, started_at)) = login_journal(&home) else {
            continue;
        };
        let open = login_completed(&home)
            || reservation_open(&home)
            || matches!(session_running(&home), Ok(true));
        if !open {
            continue;
        }
        found.push((
            started_at,
            Login {
                id,
                provider,
                label,
                pool,
                saved: None,
                signed_in_again: false,
                home,
            },
        ));
    }
    found.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    found
        .into_iter()
        .take(MAX_LOGINS)
        .map(|(_, login)| login)
        .collect()
}
/// What an owner start did with sign-in homes a previous owner left behind (SB-87).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sweep {
    /// Removed: no session ran in them and no Keychain item needed a dialog to delete.
    pub removed: usize,
    /// Kept: their sign-in is still open in Terminal; a later start removes them once it ends.
    pub running: usize,
    /// Kept: the sign-in completed in Terminal and its journal lets this owner finish it (SB-65).
    pub finishable: usize,
    /// Kept: a Keychain item that could not be deleted silently (locked, or not created through
    /// `/usr/bin/security`), or a marker that could not be read. OPERATIONS → recovery.
    pub kept: usize,
}
/// The Claude Code Keychain item a sign-in home may have left. Only macOS finds one; elsewhere
/// the item lives inside the home, so `Silent` and `Asks` are built by tests alone.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum LeftoverItem {
    Absent,
    /// Present and deletable through `/usr/bin/security` without a dialog.
    Silent,
    /// Locked, unavailable, or an access list that would ask.
    Asks,
}
/// Removes the sign-in homes an earlier owner forgot (it crashed or quit with a sign-in open):
/// only UUID-named folders directly under `logins/`, never a link, never one whose sign-in still
/// runs in Terminal, and never one whose Keychain item would ask to be deleted. Runs once, at an
/// owner's start, before any sign-in of this owner exists.
pub fn sweep_logins(root: &Path) -> Sweep {
    sweep_logins_with(
        root,
        &native_leftover_item,
        &native_delete_item,
        Duration::from_secs(60),
    )
}
pub(crate) fn sweep_logins_with(
    root: &Path,
    item: &dyn Fn(&Path) -> LeftoverItem,
    delete: &dyn Fn(&Path) -> Result<(), String>,
    min_age: Duration,
) -> Sweep {
    let mut sweep = Sweep::default();
    let Ok(entries) = fs::read_dir(root.join("logins")) else {
        return sweep;
    };
    for entry in entries.flatten() {
        let home = entry.path();
        let named = entry
            .file_name()
            .to_str()
            .is_some_and(|name| Uuid::parse_str(name).is_ok());
        if !named || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        // SB-103: a folder made in the last minute is a sign-in just beginning (the sweep runs
        // beside a starting owner), not a leftover.
        if fs::symlink_metadata(&home)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_none_or(|age| age < min_age)
        {
            continue;
        }
        let pending = reservation_open(&home);
        match session_running(&home) {
            _ if pending => sweep.running += 1,
            Ok(true) => sweep.running += 1,
            Err(_) => sweep.kept += 1,
            Ok(false) if login_completed(&home) && login_journal(&home).is_some() => {
                sweep.finishable += 1
            }
            Ok(false) => {
                let cleared = match item(&home) {
                    LeftoverItem::Absent => true,
                    LeftoverItem::Silent => delete(&home).is_ok(),
                    LeftoverItem::Asks => false,
                };
                if cleared && fs::remove_dir_all(&home).is_ok() {
                    sweep.removed += 1;
                } else {
                    sweep.kept += 1;
                }
            }
        }
    }
    sweep
}
fn native_leftover_item(home: &Path) -> LeftoverItem {
    #[cfg(target_os = "macos")]
    {
        use switchboard_core::external_keychain::{probe_external, ItemProbe};
        let Ok(user) = login_user() else {
            return LeftoverItem::Asks;
        };
        match probe_external(
            &keychain_service(home),
            &user,
            Some(Path::new("/usr/bin/security")),
        ) {
            ItemProbe::Absent => LeftoverItem::Absent,
            ItemProbe::Present {
                tool_trusted: true, ..
            } => LeftoverItem::Silent,
            _ => LeftoverItem::Asks,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Elsewhere a sign-in's credentials live inside its home and go with it.
        let _ = home;
        LeftoverItem::Absent
    }
}
fn native_delete_item(home: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::external_keychain::delete(&keychain_service(home), &login_user()?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        Ok(())
    }
}
/// A session marker is written as its shell starts, so a live process younger than the
/// marker by more than this margin cannot be that shell: its pid was reused.
#[cfg(any(unix, test))]
const PID_REUSE_MARGIN: Duration = Duration::from_secs(60);
#[cfg(any(unix, test))]
fn pid_reused(marker_age: Duration, process_age: Duration) -> bool {
    process_age + PID_REUSE_MARGIN < marker_age
}
/// Parses `ps -o etime=`, `[[dd-]hh:]mm:ss`; macOS `ps` has no `etimes` keyword.
#[cfg(any(unix, test))]
fn parse_elapsed(text: &str) -> Option<Duration> {
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u64>().ok())
            .flatten()
    };
    let text = text.trim();
    let (days, clock) = match text.split_once('-') {
        Some((days, clock)) => (number(days)?, clock),
        None => (0, text),
    };
    let parts = clock.split(':').map(number).collect::<Option<Vec<u64>>>()?;
    let (hours, minutes, seconds) = match parts[..] {
        [m, s] => (0, m, s),
        [h, m, s] => (h, m, s),
        _ => return None,
    };
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    let total = days
        .checked_mul(86_400)?
        .checked_add(hours.checked_mul(3600)?)?
        .checked_add(minutes * 60 + seconds)?;
    Some(Duration::from_secs(total))
}
/// How long ago `pid` started; None when `ps` cannot say, which keeps the home busy.
#[cfg(unix)]
fn process_age(pid: u32) -> Option<Duration> {
    let output = Command::new("/bin/ps")
        .args(["-o", "etime=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_elapsed(&String::from_utf8_lossy(&output.stdout)))
        .flatten()
}
pub(crate) fn ensure_idle(home: &Path) -> Result<(), String> {
    let pending = home.join(".launch-pending");
    match fs::symlink_metadata(&pending) {
        Ok(meta) => {
            let expired = meta.file_type().is_file()
                && meta
                    .modified()
                    .is_ok_and(|t| reservation_expired(t, SystemTime::now()));
            // The script never ran: releasing the abandoned marker frees the home.
            if !expired || fs::remove_file(&pending).is_err() {
                return Err("Close the existing session before changing its home.".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Session state unavailable. Inspect the managed home.".into()),
    }
    if session_running(home)? {
        return Err("Close the existing session before changing its home.".into());
    }
    Ok(())
}
/// Whether a live session holds `home`: its marker names a process that is still the session
/// (the pid alive and at least as old as the marker; on Windows, the pid with its creation
/// time). Reads only. An unreadable or invalid marker is an error, never "not running".
pub(crate) fn session_running(home: &Path) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let marker = home.join(".session-process");
        if marker.exists() {
            let value: serde_json::Value = serde_json::from_str(&read_regular(&marker)?)
                .map_err(|_| "Session state invalid. Inspect the managed home.")?;
            let pid = value["pid"]
                .as_u64()
                .and_then(|x| u32::try_from(x).ok())
                .filter(|x| *x > 0)
                .ok_or("Session state invalid.")?;
            let created = value["created"].as_u64().ok_or("Session state invalid.")?;
            if switchboard_core::windows::process_matches(pid, created)? {
                return Ok(true);
            }
        }
    }
    #[cfg(unix)]
    let marker = home.join(".session-pid");
    #[cfg(unix)]
    if marker.exists() {
        let text = read_regular(&marker)?;
        let pid = text
            .trim()
            .parse::<u32>()
            .map_err(|_| "Session state invalid. Inspect the managed home.")?;
        if pid == 0 {
            return Err("Session state invalid.".into());
        }
        #[cfg(unix)]
        if Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            // A live pid may since belong to another process; only a process at least as
            // old as the marker can be the session that wrote it. Unknown ages stay busy.
            let reused = fs::symlink_metadata(&marker)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .zip(process_age(pid))
                .is_some_and(|(marker_age, age)| pid_reused(marker_age, age));
            if !reused {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
/// Cancel found the sign-in's Terminal session but could not end it, or could not confirm that
/// the process its marker names is that session (SB-83).
pub(crate) const SIGN_IN_WOULD_NOT_END: &str =
    "Switchboard could not end the sign-in in Terminal. Close its Terminal window, then cancel again.";
/// Terminal was asked to run the sign-in script and has not started it yet (its reservation
/// holds the home for up to ten minutes); there is no process to end.
pub(crate) const SIGN_IN_STARTING: &str =
    "The sign-in is still opening in Terminal. Cancel again in a moment.";
/// How long each signal gets to end the session before the next one is tried.
const END_WAIT: Duration = Duration::from_secs(3);
/// Ends the Terminal session a sign-in runs in `home`, the way closing its window would, so
/// Cancel never sends the person looking for that window (SB-83). Only Switchboard's own sign-in
/// script for this home is touched: on Unix the live shell its `.session-pid` names, at least as
/// old as the marker and with this home's `launch.command` on its command line, together with the
/// provider CLI in its process group (SIGHUP, then SIGTERM); on Windows the PowerShell process its
/// `.session-process` names, by pid and creation time, with its children. Ok once the home is idle.
pub(crate) fn end_sign_in(home: &Path) -> Result<(), String> {
    end_sign_in_within(home, END_WAIT)
}
fn end_sign_in_within(home: &Path, wait: Duration) -> Result<(), String> {
    if ensure_idle(home).is_ok() {
        return Ok(());
    }
    #[cfg(unix)]
    let marker = home.join(".session-pid");
    #[cfg(windows)]
    let marker = home.join(".session-process");
    if !marker.exists() {
        return Err(if home.join(".launch-pending").exists() {
            SIGN_IN_STARTING.into()
        } else {
            SIGN_IN_WOULD_NOT_END.into()
        });
    }
    end_session(home, &marker, wait)
}
/// Polls until the home is idle or `wait` passes.
fn idle_within(home: &Path, wait: Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if ensure_idle(home).is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(unix)]
fn end_session(home: &Path, marker: &Path, wait: Duration) -> Result<(), String> {
    let pid = read_regular(marker)
        .ok()
        .and_then(|text| text.trim().parse::<libc::pid_t>().ok())
        .filter(|pid| *pid > 0)
        .ok_or(SIGN_IN_WOULD_NOT_END)?;
    for signal in [libc::SIGHUP, libc::SIGTERM] {
        // Checked before each signal: the pid must still be this home's script.
        if !runs_script_of(pid, home) {
            return Err(SIGN_IN_WOULD_NOT_END.into());
        }
        signal_session(pid, signal);
        if idle_within(home, wait) {
            return Ok(());
        }
    }
    Err(SIGN_IN_WOULD_NOT_END.into())
}
/// Whether `pid` is the shell running this home's own launch script.
#[cfg(unix)]
fn runs_script_of(pid: libc::pid_t, home: &Path) -> bool {
    let script = home.join("launch.command");
    Command::new("/bin/ps")
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains(script.to_string_lossy().as_ref())
        })
}
/// Terminal's shell starts the script as its own job, so its process group holds the script and
/// the provider CLI; a script that does not lead its group is signalled with its children.
#[cfg(unix)]
fn signal_session(pid: libc::pid_t, signal: libc::c_int) {
    // SAFETY: getpgid and kill take plain integers and touch no memory of this process.
    if unsafe { libc::getpgid(pid) } == pid {
        unsafe { libc::kill(-pid, signal) };
        return;
    }
    let children = Command::new("/usr/bin/pgrep")
        .args(["-P", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    unsafe { libc::kill(pid, signal) };
    for child in children
        .lines()
        .filter_map(|line| line.trim().parse::<libc::pid_t>().ok())
    {
        unsafe { libc::kill(child, signal) };
    }
}
#[cfg(windows)]
fn end_session(home: &Path, marker: &Path, wait: Duration) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let value: serde_json::Value =
        serde_json::from_str(&read_regular(marker).map_err(|_| SIGN_IN_WOULD_NOT_END)?)
            .map_err(|_| SIGN_IN_WOULD_NOT_END)?;
    let pid = value["pid"]
        .as_u64()
        .and_then(|x| u32::try_from(x).ok())
        .filter(|x| *x > 0)
        .ok_or(SIGN_IN_WOULD_NOT_END)?;
    let created = value["created"].as_u64().ok_or(SIGN_IN_WOULD_NOT_END)?;
    if switchboard_core::windows::process_matches(pid, created)
        .map_err(|_| SIGN_IN_WOULD_NOT_END)?
    {
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        // Closing a console window ends every process attached to it; /T /F does the same for
        // the PowerShell script and the provider CLI it started. No console window flashes.
        let _ = Command::new(system.join("System32").join("taskkill.exe"))
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    if idle_within(home, wait + wait) {
        Ok(())
    } else {
        Err(SIGN_IN_WOULD_NOT_END.into())
    }
}
/// Whether a session Switchboard launched for `provider` may be running: an isolated home of one
/// of `account_ids`, or a managed home of the provider, that is not idle. A state that cannot be
/// read counts as running. The automatic fallback uses it: a workflow executor that names no
/// account could be such a session, so the ordinary CLI's limit is not blamed on it (SB-73).
pub(crate) fn launched_session_running(
    root: &Path,
    provider: Provider,
    account_ids: &[String],
) -> bool {
    let busy = |home: &Path| home.is_dir() && ensure_idle(home).is_err();
    if account_ids
        .iter()
        .any(|id| busy(&root.join("homes").join(id)))
    {
        return true;
    }
    let prefix = format!("{}-", provider.as_str());
    fs::read_dir(root.join("runtimes"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry.file_name().to_string_lossy().starts_with(&prefix) && busy(&entry.path())
        })
}
pub fn clean_account(root: &Path, id: &str) -> Result<(), String> {
    Uuid::parse_str(id).map_err(|_| "Invalid account.")?;
    let home = root.join("homes").join(id);
    if home.exists() {
        ensure_idle(&home)?;
        let meta = fs::symlink_metadata(&home).map_err(|_| "Account home unavailable.")?;
        switchboard_core::private_fs::check_path(&home)?;
        if meta.file_type().is_symlink() {
            return Err("Unsafe account home.".into());
        }
        fs::remove_dir_all(home)
            .map_err(|_| "Account home cleanup failed. Close its sessions and retry.")?;
    }
    Ok(())
}
fn codex_auth_snapshot(
    kind: AuthKind,
    credential: &Credential,
    written_at: time::OffsetDateTime,
) -> Result<serde_json::Value, String> {
    match kind {
        AuthKind::ApiKey => Ok(json!({
            "OPENAI_API_KEY": credential.access_token,
            "auth_mode": "apikey"
        })),
        AuthKind::OAuth => {
            let identity = credential.id_token.as_deref().ok_or(
                "Captured Codex login has no identity token. Sign in again or use managed mode.",
            )?;
            // Codex requires last_refresh to expose token-backed authentication:
            // https://github.com/openai/codex/blob/e72da2b53805894878023d01949a25a082e0a5cb/codex-rs/login/src/auth/manager.rs#L580-L598
            // This is the local snapshot-write time, NOT an OAuth refresh observation.
            // The access-token expiry remains unchanged and no refresh lineage is copied.
            let written_at = written_at
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| "Credential snapshot timestamp unavailable.")?;
            Ok(json!({
                "auth_mode": "chatgpt",
                "OPENAI_API_KEY": null,
                "last_refresh": written_at,
                "tokens": {
                    "access_token": credential.access_token,
                    "refresh_token": "",
                    "id_token": identity,
                    "account_id": credential.account_id
                }
            }))
        }
        AuthKind::SetupToken => Err("Codex does not support setup tokens.".into()),
    }
}
pub(crate) const PROJECT_ACCOUNT_OUTSIDE: &str =
    "This account belongs to a group. Launch it from one of the group's folders.";
pub(crate) const PROJECT_FOLDER_FOREIGN: &str =
    "This folder belongs to a group. Launch one of the group's accounts.";
/// Projects reserve their accounts (0.6): a project's account starts only inside the project's
/// folders, and inside them a provider the project has accounts for uses only those.
pub(crate) fn project_allows(
    snapshot: &switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    folder: &Path,
) -> Result<(), String> {
    // SB-109: a group runs only on accounts of its own organization (or personal ones).
    if !snapshot.context_admits(&account.pool, account) {
        return Err(switchboard_core::organizations::SEAT_OUTSIDE.into());
    }
    if let Some(owner) = snapshot.project_of_pool(&account.pool) {
        if !owner
            .folders
            .iter()
            .any(|f| folder.starts_with(Path::new(f)))
        {
            return Err(PROJECT_ACCOUNT_OUTSIDE.into());
        }
    }
    // SB-146: a folder in several groups may run on any of them; another account is refused
    // only while one of its groups has its own account for the provider.
    let here = snapshot.projects_for(folder);
    if !here.iter().any(|p| p.pool == account.pool) {
        let has_own = here.iter().any(|p| {
            snapshot
                .accounts
                .iter()
                .any(|a| a.pool == p.pool && a.provider == account.provider && a.enabled)
        });
        if has_own {
            return Err(PROJECT_FOLDER_FOREIGN.into());
        }
    }
    Ok(())
}
/// Starts a third-party agent the catalog marks `launch` (configured by environment alone) in
/// `working_directory`, pointed at the proxy's pool with the agents' capability. Only an API-key
/// account may serve it, and projects keep their folders (operator request 2026-10-05).
pub fn launch_agent(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    agent: &str,
    pool: &str,
    working_directory: &Path,
) -> Result<Value, String> {
    launch_agent_with(
        root,
        store,
        proxy,
        agent,
        pool,
        working_directory,
        find_program,
        start_terminal,
    )
}
/// `launch_agent` with the agent's program found by `find` and Terminal started by `start`, so
/// tests need neither an installed agent nor Terminal.
#[allow(clippy::too_many_arguments)]
fn launch_agent_with(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    agent: &str,
    pool: &str,
    working_directory: &Path,
    find: fn(&str) -> Option<PathBuf>,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let profile = crate::agent_catalog::launchable(agent)?;
    let working_directory = agent_folder(root, working_directory)?;
    let (account, _) = store
        .route(Provider::Claude, pool)
        .map_err(|_| "Select an API-key account in this pool first.".to_string())?;
    if account.kind != AuthKind::ApiKey {
        return Err(switchboard_proxy::AGENT_SUBSCRIPTION_REFUSED.into());
    }
    project_allows(&store.snapshot()?, &account, &working_directory)?;
    let program = find(&profile.binary).ok_or_else(|| {
        format!(
            "{} is not installed. Install it first, then launch it again.",
            profile.name
        )
    })?;
    let homes = root.join("runtimes");
    private_dir(&homes)?;
    let home = homes.join(format!("agent-{}-{pool}", profile.id));
    private_dir(&home)?;
    let base = format!("http://{}/claude/{pool}{}", proxy.address(), profile.suffix);
    // SB-201: the account's context, as a Claude Code launch on it would have.
    let context = store
        .snapshot()?
        .session_context(&account, Some(&working_directory));
    let mut env = BTreeMap::from([
        (
            "SWITCHBOARD_SESSION".to_string(),
            format!("managed:claude:{pool}"),
        ),
        ("SWITCHBOARD_CONTEXT".to_string(), context.clone()),
    ]);
    if let (Some(base_env), Some(key_env)) = (&profile.base_env, &profile.key_env) {
        env.insert(base_env.clone(), base);
        env.insert(key_env.clone(), proxy.agent_token().to_owned());
    }
    env.extend(profile.extra_env.clone());
    let args: Vec<&str> = profile.args.iter().map(String::as_str).collect();
    let content = script(&home, &program, &args, &env, false, &working_directory);
    let path = write_launch_script(&home, &content)?;
    // Listed under Sessions like any launch (`sessions`): ids, folder and times only.
    // `provider` is the provider the proxy route serves (the agent speaks Claude's protocol on
    // `/claude/<pool>`), so a reader comparing it with the route sees no false move; the catalog
    // agent is its own field.
    let record = json!({
        "account_id": account.id, "provider": Provider::Claude.as_str(), "agent": profile.id,
        "pool": pool, "context": context,
        "mode": "managed", "working_directory": working_directory, "in_place": false,
        "launched_at": switchboard_proxy::now(), "continues": null,
    });
    private_write(
        &home.join(SESSION_RECORD),
        record.to_string().as_bytes(),
        false,
    )?;
    start(&path)?;
    Ok(
        json!({"launched": true, "agent": profile.id, "name": profile.name, "pool": pool, "account": account.label}),
    )
}
/// The folder a third-party agent starts in: an existing absolute folder outside Switchboard's
/// own data.
pub(crate) fn agent_folder(root: &Path, working_directory: &Path) -> Result<PathBuf, String> {
    if !working_directory.is_absolute() {
        return Err("Choose an existing project directory.".into());
    }
    let working_directory = working_directory
        .canonicalize()
        .map_err(|_| "Choose an existing project directory.")?;
    let private_root = root
        .canonicalize()
        .map_err(|_| "Managed home unavailable.")?;
    if !working_directory.is_dir() || working_directory.starts_with(&private_root) {
        return Err("Choose an existing project directory.".into());
    }
    Ok(working_directory)
}
pub const OPENROUTER_MODEL_NEEDED: &str =
    "Choose a model for this launch, or set the key's default model on the API keys page or with switchboard agents openrouter model.";
/// Starts an agent with an OpenRouter recipe (catalog `openrouter`, SB-79) in a folder on the
/// operator's saved OpenRouter key and `model` (default: the key's model). The key never touches
/// disk: the session script reads it from `switchboard agents key --service openrouter` when it
/// starts (XA-02 A-2). No proxy, pool or account is involved.
pub fn launch_agent_openrouter(
    root: &Path,
    store: &Store,
    agent: &str,
    model: Option<&str>,
    working_directory: &Path,
) -> Result<Value, String> {
    let recipe = crate::agent_catalog::openrouter(agent)?;
    let program = find_program(&recipe.binary).ok_or_else(|| {
        format!(
            "{} is not installed. Install it first, then launch it again.",
            recipe.name
        )
    })?;
    let cli = agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    let (path, result) = openrouter_session(
        root,
        store,
        &recipe,
        &program,
        &cli,
        model,
        working_directory,
    )?;
    start_terminal(&path)?;
    Ok(result)
}
pub const OPENROUTER_CLI_MISSING: &str =
    "The switchboard command was not found. Install it from Agents, then launch again.";
/// Printed when a continuation's first turn ended in an error: nothing was resumed.
#[cfg(not(windows))]
const FIRST_TURN_FAILED: &str = "The first turn did not finish, so the handoff may not have been accepted. Nothing was resumed; run switchboard continue again or open the workflow by hand.";
/// What a Hermes continuation needs before its model is even read (SB-107): not Windows, Hermes
/// installed, an OpenRouter key saved, the `switchboard` command present for the session script.
/// Cheap — no process is started — so it comes first.
pub(crate) fn hermes_continuation_basics(store: &Store) -> Result<(), String> {
    if cfg!(windows) {
        return Err(crate::continuation::UNIX_ONLY.into());
    }
    find_program("hermes").ok_or(crate::hermes::HERMES_NOT_INSTALLED)?;
    store
        .agent_key("openrouter")?
        .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
    agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    Ok(())
}
/// Whether a `hermes` chain step can be tried: `hermes_continuation_basics`. The model is settled
/// at the hand-over (`hermes_continuation_preflight`), which refuses before the offer, and the
/// chain then moves on.
pub(crate) fn hermes_continuation_ready(store: &Store) -> bool {
    hermes_continuation_basics(store).is_ok()
}
/// What refuses a Hermes continuation before anything is offered (SB-107): the basics, then no
/// model — neither Hermes's own OpenRouter model (`own_model`) nor the key's default.
pub(crate) fn hermes_continuation_preflight(
    store: &Store,
    own_model: Option<&str>,
) -> Result<(), String> {
    hermes_continuation_basics(store)?;
    let saved = store
        .agent_key("openrouter")?
        .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
    if own_model.is_none() && saved.model.is_none() {
        return Err(OPENROUTER_MODEL_NEEDED.into());
    }
    Ok(())
}
/// Continues an Observatory workflow on Hermes Agent with the saved OpenRouter key (SB-107), on
/// the model Hermes itself runs on OpenRouter (`own_model`), else the key's default model. Hermes runs in a home Switchboard owns (`runtimes/hermes-continue/agent-home`, its
/// `HERMES_HOME`): its `config.yaml` is Switchboard's — Observatory's MCP server, so the session
/// can accept the handoff, and no adoption of other tools' logins. Terminal runs
/// `hermes chat --query-file <prompt>`, which on a terminal seeds an interactive session with the
/// handoff prompt as its first turn (Hermes 0.21.4). The prompt names the owner and a session id
/// Switchboard made (Observatory wants a UUID; Hermes's own ids are not one). The key is read when the session starts,
/// never written (as `launch_agent_openrouter`).
pub(crate) fn hermes_continuation(
    root: &Path,
    store: &Store,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
    own_model: Option<&str>,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    hermes_continuation_preflight(store, own_model)?;
    let program = find_program("hermes").ok_or(crate::hermes::HERMES_NOT_INSTALLED)?;
    let cli = agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    let (path, result) = hermes_continuation_session(
        root,
        store,
        &program,
        &cli,
        working_directory,
        continuation,
        own_model,
    )?;
    start(&path)?;
    Ok(result)
}
#[cfg_attr(windows, allow(unused_variables))]
fn hermes_continuation_session(
    root: &Path,
    store: &Store,
    program: &Path,
    cli: &Path,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
    own_model: Option<&str>,
) -> Result<(PathBuf, Value), String> {
    #[cfg(windows)]
    {
        Err(crate::continuation::UNIX_ONLY.into())
    }
    #[cfg(not(windows))]
    {
        let working_directory = agent_folder(root, working_directory)?;
        if !crate::continuation::valid_workflow_id(&continuation.workflow_id) {
            return Err(crate::continuation::BAD_WORKFLOW_ID.into());
        }
        let saved = store
            .agent_key_in("openrouter", Some(&working_directory))?
            .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
        let context = key_context(store, &saved, &working_directory)?;
        let (model, model_from) = match own_model {
            Some(m) if switchboard_core::agent_keys::model_valid(m) => (m.to_owned(), "hermes"),
            _ => (saved.model.ok_or(OPENROUTER_MODEL_NEEDED)?, "key"),
        };
        let recipe = crate::agent_catalog::openrouter("hermes")?;
        let base = root.join("runtimes").join("hermes-continue");
        private_dir(&root.join("runtimes"))?;
        private_dir(&base)?;
        let own = base.join("agent-home");
        private_dir(&own)?;
        private_write(
            &own.join("config.yaml"),
            hermes_config(&continuation.observatory).as_bytes(),
            false,
        )?;
        prune_hermes_sessions(&base, SystemTime::now());
        // One folder per workflow: two hand-overs never share a script or a prompt.
        let session = base.join(&continuation.workflow_id);
        private_dir(&session)?;
        let prompt = session.join("handoff-prompt.txt");
        let session_id = uuid::Uuid::new_v4();
        let first = format!(
            "{} In every Observatory call use owner agent:hermes and sessionId {session_id}.",
            crate::continuation::prompt(continuation)
        );
        private_write(&prompt, first.as_bytes(), false)?;
        let mut env = BTreeMap::from([("SWITCHBOARD_CONTEXT".to_string(), context.clone())]);
        if let Some(isolate) = &recipe.isolate_env {
            env.insert(isolate.clone(), own.to_string_lossy().into_owned());
        }
        env.extend(recipe.extra_env.clone());
        let prompt = prompt.to_string_lossy().into_owned();
        let mut args = vec!["chat".to_string(), "--query-file".into(), prompt];
        args.extend(
            recipe
                .model_flag
                .iter()
                .map(|a| a.replace("{model}", &model)),
        );
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let key_command = [
            "--data-dir".to_string(),
            root.to_string_lossy().into_owned(),
            "agents".into(),
            "key".into(),
            "--service".into(),
            "openrouter".into(),
            // SB-144: the key of the project the agent starts in (the command runs before `cd`).
            "--dir".into(),
            working_directory.to_string_lossy().into_owned(),
        ];
        let content = script_with(
            &session,
            program,
            &args,
            &env,
            Some((&recipe.key_env, cli, &key_command)),
            false,
            &working_directory,
        );
        let path = write_launch_script(&session, &content)?;
        // Listed under Sessions like any launch (`sessions`); its label is the model.
        let record = json!({
            "account_id": crate::hermes::HERMES_TARGET, "provider": "hermes", "pool": "openrouter",
            "context": context, "mode": "isolated", "working_directory": working_directory,
            "in_place": false,
            "launched_at": switchboard_proxy::now(), "continues": continuation.workflow_id,
            "label": model,
        });
        private_write(
            &session.join(SESSION_RECORD),
            record.to_string().as_bytes(),
            false,
        )?;
        Ok((
            path.clone(),
            json!({"launched": true, "agent": "hermes", "via": "openrouter", "model": model, "model_from": model_from, "script": path}),
        ))
    }
}
/// The continuation target name for OpenCode on the saved OpenRouter key (SB-119).
pub const OPENCODE_TARGET: &str = "opencode";
pub const OPENCODE_NOT_INSTALLED: &str =
    "OpenCode is not installed. Install it from opencode.ai, then try again.";
/// What an OpenCode continuation needs before anything is offered (SB-119): not Windows,
/// OpenCode installed, an OpenRouter key and model saved, the `switchboard` command present for
/// the session script.
pub(crate) fn opencode_continuation_preflight(store: &Store) -> Result<(), String> {
    if cfg!(windows) {
        return Err(crate::continuation::UNIX_ONLY.into());
    }
    find_program("opencode").ok_or(OPENCODE_NOT_INSTALLED)?;
    let saved = store
        .agent_key("openrouter")?
        .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
    if saved.model.is_none() {
        return Err(OPENROUTER_MODEL_NEEDED.into());
    }
    agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    Ok(())
}
/// Whether an `opencode` chain step can be tried now.
pub(crate) fn opencode_continuation_ready(store: &Store) -> bool {
    opencode_continuation_preflight(store).is_ok()
}
/// OpenCode's configuration for one continuation (SB-119), given as `OPENCODE_CONFIG_CONTENT`,
/// the layer OpenCode merges last: the key from the session's environment wins over a key the
/// person saved with `/connect`; the chosen model is declared, so an id OpenCode's own list
/// lacks is not swapped for another; Observatory's MCP server lets the session accept the
/// handoff. Nothing of the person's OpenCode — `auth.json`, sessions, `opencode.json` — is
/// written (OpenCode 1.19.0 provider.ts and config.ts, researched 2026-10-14).
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn opencode_config(model: &str, observatory: &crate::continuation::McpServer) -> String {
    let mut command = vec![Value::String(observatory.command.clone())];
    command.extend(observatory.args.iter().cloned().map(Value::String));
    json!({
        "provider": {"openrouter": {
            "options": {"apiKey": "{env:OPENROUTER_API_KEY}"},
            "models": {model: {}},
        }},
        "mcp": {"observatory": {"type": "local", "command": command, "enabled": true}},
    })
    .to_string()
}
/// Continues an Observatory workflow on OpenCode with the saved OpenRouter key and its default
/// model (SB-119), the handoff prompt as the session's first message (`--prompt`).
pub(crate) fn opencode_continuation(
    root: &Path,
    store: &Store,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    opencode_continuation_preflight(store)?;
    let program = find_program("opencode").ok_or(OPENCODE_NOT_INSTALLED)?;
    let cli = agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    let (path, result) = opencode_continuation_session(
        root,
        store,
        &program,
        &cli,
        working_directory,
        continuation,
    )?;
    start(&path)?;
    Ok(result)
}
#[cfg_attr(windows, allow(unused_variables))]
pub(crate) fn opencode_continuation_session(
    root: &Path,
    store: &Store,
    program: &Path,
    cli: &Path,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
) -> Result<(PathBuf, Value), String> {
    #[cfg(windows)]
    {
        Err(crate::continuation::UNIX_ONLY.into())
    }
    #[cfg(not(windows))]
    {
        let working_directory = agent_folder(root, working_directory)?;
        if !crate::continuation::valid_workflow_id(&continuation.workflow_id) {
            return Err(crate::continuation::BAD_WORKFLOW_ID.into());
        }
        let saved = store
            .agent_key_in("openrouter", Some(&working_directory))?
            .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
        let context = key_context(store, &saved, &working_directory)?;
        let model = saved.model.ok_or(OPENROUTER_MODEL_NEEDED)?;
        if !switchboard_core::agent_keys::model_valid(&model) {
            return Err(switchboard_core::agent_keys::MODEL_INVALID.into());
        }
        let base = root.join("runtimes").join("opencode-continue");
        private_dir(&root.join("runtimes"))?;
        private_dir(&base)?;
        prune_hermes_sessions(&base, SystemTime::now());
        let session = base.join(&continuation.workflow_id);
        private_dir(&session)?;
        let session_id = uuid::Uuid::new_v4();
        let first = format!(
            "{} In every Observatory call use owner agent:opencode and sessionId {session_id}.",
            crate::continuation::prompt(continuation)
        );
        private_write(&session.join("handoff-prompt.txt"), first.as_bytes(), false)?;
        let mut env = BTreeMap::from([("SWITCHBOARD_CONTEXT".to_string(), context.clone())]);
        env.insert(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            opencode_config(&model, &continuation.observatory),
        );
        let model_arg = format!("openrouter/{model}");
        let args = ["--prompt", first.as_str(), "-m", model_arg.as_str()];
        let key_command = [
            "--data-dir".to_string(),
            root.to_string_lossy().into_owned(),
            "agents".into(),
            "key".into(),
            "--service".into(),
            "openrouter".into(),
            // SB-144: the key of the project the agent starts in (the command runs before `cd`).
            "--dir".into(),
            working_directory.to_string_lossy().into_owned(),
        ];
        let content = script_with(
            &session,
            program,
            &args,
            &env,
            Some(("OPENROUTER_API_KEY", cli, &key_command)),
            false,
            &working_directory,
        );
        let path = write_launch_script(&session, &content)?;
        let record = json!({
            "account_id": OPENCODE_TARGET, "provider": "opencode", "pool": "openrouter",
            "context": context, "mode": "isolated", "working_directory": working_directory,
            "in_place": false,
            "launched_at": switchboard_proxy::now(), "continues": continuation.workflow_id,
            "label": model,
        });
        private_write(
            &session.join(SESSION_RECORD),
            record.to_string().as_bytes(),
            false,
        )?;
        Ok((
            path.clone(),
            json!({"launched": true, "agent": "opencode", "via": "openrouter", "model": model, "model_from": "key", "script": path}),
        ))
    }
}
#[cfg_attr(windows, allow(dead_code))]
/// How long a finished Hermes hand-over's folder (its script, prompt and record) is kept.
#[cfg(not(windows))]
const HERMES_SESSION_KEEP: Duration = Duration::from_secs(7 * 86_400);
/// Removes the folders of Hermes hand-overs that ended more than a week ago; a running one, the
/// shared `agent-home` and anything not named like a workflow are never touched.
#[cfg(not(windows))]
fn prune_hermes_sessions(base: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let named = entry
            .file_name()
            .to_str()
            .is_some_and(crate::continuation::valid_workflow_id);
        let old = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| {
            now.duration_since(t)
                .is_ok_and(|age| age > HERMES_SESSION_KEEP)
        });
        if named
            && old
            && entry.file_type().is_ok_and(|t| t.is_dir())
            && matches!(session_running(&path), Ok(false))
        {
            let _ = fs::remove_dir_all(&path);
        }
    }
}
/// Hermes's `config.yaml` in the home Switchboard owns: JSON strings are YAML double-quoted
/// scalars, so every value is written escaped.
fn hermes_config(server: &crate::continuation::McpServer) -> String {
    // A YAML double-quoted scalar takes JSON's escapes; anything outside printable ASCII is
    // written as `\uXXXX` too, since YAML readers reject DEL and fold NEL, LS and PS.
    let string = |s: &str| {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                ' '..='~' => out.push(c),
                c if (c as u32) <= 0xFFFF => out.push_str(&format!("\\u{:04X}", c as u32)),
                c => out.push_str(&format!("\\U{:08X}", c as u32)),
            }
        }
        out.push('"');
        out
    };
    let args = server
        .args
        .iter()
        .map(|a| string(a))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "# Written by Fabric Switchboard for workflow continuations; rewritten at each one.\n\
auth:\n  adopt_external_logins: false\n\
mcp_servers:\n  observatory:\n    command: {}\n    args: [{args}]\n",
        string(&server.command)
    )
}
/// Writes the session of an OpenRouter launch and returns its script and the answer. Separate
/// from starting Terminal so tests read the script a real launch would run.
/// SB-137: Pi's own folder gets the operator's `AGENTS.md`, skills and extensions as links and
/// their settings without anything that signs in; the launch's key still wins (`auth.json` is
/// never carried).
fn carry_pi(own: &Path) -> Result<(), String> {
    let Some(user) = crate::carry::pi_user_dir() else {
        return Ok(());
    };
    crate::carry::link_into(own, &user, crate::carry::PI_LINKS);
    if let Some(settings) = crate::carry::pi_settings(&user) {
        private_write(
            &own.join("settings.json"),
            serde_json::to_string(&settings).unwrap().as_bytes(),
            false,
        )?;
    }
    Ok(())
}
/// SB-201: the context of an agent on a saved key: the group's own key gives that group's
/// context, the machine's key is personal (also in an organization's folder).
fn key_context(
    store: &Store,
    key: &switchboard_core::agent_keys::AgentKey,
    folder: &Path,
) -> Result<String, String> {
    Ok(store
        .snapshot()?
        .key_context(key.pool.as_deref(), Some(folder)))
}
fn openrouter_session(
    root: &Path,
    store: &Store,
    recipe: &crate::agent_catalog::OpenrouterRecipe,
    program: &Path,
    cli: &Path,
    model: Option<&str>,
    working_directory: &Path,
) -> Result<(PathBuf, Value), String> {
    let working_directory = agent_folder(root, working_directory)?;
    let saved = store
        .agent_key_in("openrouter", Some(&working_directory))?
        .ok_or(switchboard_core::agent_keys::NO_AGENT_KEY)?;
    let model = match model {
        Some(m) if !switchboard_core::agent_keys::model_valid(m) => {
            return Err(switchboard_core::agent_keys::MODEL_INVALID.into())
        }
        Some(m) => Some(m.to_owned()),
        None => saved.model.clone(),
    };
    let takes_model = !recipe.model_flag.is_empty() || recipe.model_env.is_some();
    if takes_model && model.is_none() {
        return Err(OPENROUTER_MODEL_NEEDED.into());
    }
    let context = key_context(store, &saved, &working_directory)?;
    let homes = root.join("runtimes");
    private_dir(&homes)?;
    let home = homes.join(format!("agent-{}-openrouter", recipe.id));
    private_dir(&home)?;
    let mut env = BTreeMap::from([("SWITCHBOARD_CONTEXT".to_string(), context.clone())]);
    if let (Some(base_env), Some(base_url)) = (&recipe.base_env, &recipe.base_url) {
        env.insert(base_env.clone(), base_url.clone());
    }
    if let (Some(model_env), Some(model)) = (&recipe.model_env, &model) {
        env.insert(model_env.clone(), model.clone());
    }
    // `{model}` inside a value names the chosen model there too (OpenCode declares it in its
    // config, SB-119); ids are validated, so they are safe inside the JSON it sits in.
    env.extend(recipe.extra_env.iter().map(|(k, v)| {
        let v = match &model {
            Some(m) => v.replace("{model}", m),
            None => v.clone(),
        };
        (k.clone(), v)
    }));
    if let Some(isolate) = &recipe.isolate_env {
        let own = home.join("agent-home");
        private_dir(&own)?;
        if isolate == "PI_CODING_AGENT_DIR" {
            carry_pi(&own)?;
        }
        env.insert(isolate.clone(), own.to_string_lossy().into_owned());
    }
    let mut args = recipe.args.clone();
    if let Some(model) = &model {
        args.extend(
            recipe
                .model_flag
                .iter()
                .map(|a| a.replace("{model}", model)),
        );
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let key_command = [
        "--data-dir".to_string(),
        root.to_string_lossy().into_owned(),
        "agents".into(),
        "key".into(),
        "--service".into(),
        "openrouter".into(),
        // SB-144: the key of the project the agent starts in (the command runs before `cd`).
        "--dir".into(),
        working_directory.to_string_lossy().into_owned(),
    ];
    let content = script_with(
        &home,
        program,
        &args,
        &env,
        Some((&recipe.key_env, cli, &key_command)),
        false,
        &working_directory,
    );
    let path = write_launch_script(&home, &content)?;
    // SB-201: listed under Sessions like any launch, labelled with its model (none when the
    // agent picks it itself); ids, folder and times only, never the key.
    let record = json!({
        "account_id": recipe.id, "provider": recipe.id, "agent": recipe.id, "pool": "openrouter",
        "context": context, "mode": "isolated", "working_directory": working_directory,
        "in_place": false, "launched_at": switchboard_proxy::now(), "continues": null,
        "label": if takes_model { model.clone() } else { None },
    });
    private_write(
        &home.join(SESSION_RECORD),
        record.to_string().as_bytes(),
        false,
    )?;
    Ok((
        path,
        json!({
            "launched": true,
            "agent": recipe.id,
            "name": recipe.name,
            "via": "openrouter",
            "model": if takes_model { model } else { None },
            "model_choice": if takes_model { "launch" } else { "agent" },
        }),
    ))
}
pub fn launch(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
) -> Result<bool, String> {
    launch_with(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        &NATIVE,
        None,
    )
}
/// Launches the account's session to continue an Observatory workflow (SB-52): the same launch,
/// plus the workflow and handoff ids, the Observatory server and a first prompt to accept.
pub fn launch_continuation(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    continuation: &Continuation,
) -> Result<bool, String> {
    launch_with(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        &NATIVE,
        Some(continuation),
    )
}
/// `launch_continuation` prepared for the caller's own terminal (SB-108): the same session, home
/// and first prompt, its script returned instead of Terminal opened.
pub fn continuation_here(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    continuation: &Continuation,
) -> Result<(bool, PathBuf), String> {
    continuation_here_with(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        &NATIVE,
        continuation,
    )
}
#[allow(clippy::too_many_arguments)]
fn continuation_here_with(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    host: &Host,
    continuation: &Continuation,
) -> Result<(bool, PathBuf), String> {
    if cfg!(windows) {
        return Err(IN_PLACE_UNSUPPORTED.into());
    }
    let (tools, script) = launch_full(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        host,
        Some(continuation),
        &[],
        true,
    )?;
    Ok((tools, script.ok_or(IN_PLACE_UNSUPPORTED)?))
}
/// Everything `launch` refuses before it touches a home: the folder, the mode, the account, its
/// project and, for an isolated session, a credential that has not expired. A continuation runs
/// it before offering the workflow, so a launch that would be refused never leaves an offer.
pub fn preflight(
    root: &Path,
    store: &Store,
    id: &str,
    mode: &str,
    working_directory: &Path,
) -> Result<(), String> {
    let (_, account) = validate(root, store, id, mode, working_directory)?;
    if mode == "managed" {
        let (selected, _) = store.route(account.provider, &account.pool)?;
        if selected.id != account.id {
            return Err("Select this account before launching managed mode.".into());
        }
    } else {
        // Store::credential refuses an expired credential.
        store.credential(id)?;
    }
    Ok(())
}
/// The checks every launch makes before it touches a home.
fn validate(
    root: &Path,
    store: &Store,
    id: &str,
    mode: &str,
    working_directory: &Path,
) -> Result<(PathBuf, switchboard_core::Account), String> {
    if !working_directory.is_absolute() {
        return Err("Choose an existing project directory.".into());
    }
    let working_directory = working_directory
        .canonicalize()
        .map_err(|_| "Choose an existing project directory.")?;
    let private_root = root
        .canonicalize()
        .map_err(|_| "Managed home unavailable.")?;
    if !working_directory.is_dir() || working_directory.starts_with(&private_root) {
        return Err("Choose an existing project directory.".into());
    }

    if mode != "isolated" && mode != "managed" {
        return Err("Choose isolated or managed launch.".into());
    }
    let account = store
        .snapshot()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found.")?;
    if !account.enabled {
        return Err("Enable the account before launch.".into());
    }
    project_allows(&store.snapshot()?, &account, &working_directory)?;
    Ok((working_directory, account))
}
/// At most this many arguments an in-place launch hands to the agent, each this long (SB-75).
const MAX_EXTRA_ARGS: usize = 16;
const MAX_EXTRA_ARG: usize = 1024;
pub const EXTRA_ARGS_INVALID: &str =
    "Agent arguments are at most 16 plain values of up to 1024 characters each.";
pub const IN_PLACE_UNSUPPORTED: &str = "Launching in place runs on macOS and Linux for now.";

/// Prepares the session exactly as `launch` does — the home, its tools, the one-session marker
/// and the script — but opens no Terminal: the caller runs the returned script in its own
/// terminal (SB-75: an embedded console such as Fabric Dashboards'). `extra` goes to the agent
/// after Switchboard's own arguments, each quoted by the script builder.
pub fn launch_here(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    extra: &[String],
) -> Result<(bool, PathBuf), String> {
    if cfg!(windows) {
        return Err(IN_PLACE_UNSUPPORTED.into());
    }
    let (tools, script) = launch_full(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        &NATIVE,
        None,
        extra,
        true,
    )?;
    Ok((tools, script.ok_or(IN_PLACE_UNSUPPORTED)?))
}
#[allow(clippy::too_many_arguments)]
fn launch_with(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    host: &Host,
    continuation: Option<&Continuation>,
) -> Result<bool, String> {
    launch_full(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        host,
        continuation,
        &[],
        false,
    )
    .map(|(tools, _)| tools)
}
fn extra_valid(extra: &[String]) -> bool {
    extra.len() <= MAX_EXTRA_ARGS
        && extra
            .iter()
            .all(|a| a.len() <= MAX_EXTRA_ARG && !a.chars().any(char::is_control))
}
#[allow(clippy::too_many_arguments)]
fn launch_full(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    host: &Host,
    continuation: Option<&Continuation>,
    extra: &[String],
    in_place: bool,
) -> Result<(bool, Option<PathBuf>), String> {
    if !extra_valid(extra) {
        return Err(EXTRA_ARGS_INVALID.into());
    }
    let (working_directory, account) = validate(root, store, id, mode, working_directory)?;
    let program = (host.binary)(account.provider)?;
    let homes = root.join(if mode == "managed" {
        "runtimes"
    } else {
        "homes"
    });
    private_dir(&homes)?;
    let name = if mode == "managed" {
        format!("{}-{}", account.provider.as_str(), account.pool)
    } else {
        account.id.clone()
    };
    let home = homes.join(name);
    private_dir(&home)?;
    let mut reservation = Reservation::new(&home)?;
    let session = if mode == "managed" {
        format!("managed:{}:{}", account.provider.as_str(), account.pool)
    } else {
        format!("isolated:{}:{}", account.provider.as_str(), account.id)
    };
    let cli = (host.agent_cli)();
    let board = (host.board)(account.provider);
    // SB-137/SB-130: the operator's instructions, skills and settings come along as links; a
    // Claude Code home skips the first-run screens and, for a continuation, trusts the folder its
    // previous agent worked in. Nothing that signs in is carried.
    let user = (host.user_scope)(account.provider);
    if let Some(user) = &user {
        match account.provider {
            Provider::Claude => {
                crate::carry::link_into(&home, user, crate::carry::CLAUDE_LINKS);
            }
            Provider::Codex => {
                crate::carry::link_into(&home, user, &["AGENTS.md"]);
            }
        }
    }
    if account.provider == Provider::Claude {
        crate::carry::claude_state(&home, continuation.map(|_| working_directory.as_path()))?;
        let settings_path = home.join("settings.json");
        let settings = crate::carry::claude_settings(user.as_deref(), None);
        // Managed: the account comes through the proxy, so the file holds only the operator's
        // carried settings, and exists only when there are some (or an earlier launch wrote it).
        if mode == "managed"
            && (settings.as_object().is_some_and(|s| !s.is_empty()) || settings_path.exists())
        {
            private_write(
                &settings_path,
                serde_json::to_string(&settings).unwrap().as_bytes(),
                false,
            )?;
        }
    }
    let tools = agent_tools(
        cli.as_deref(),
        root,
        &home,
        account.provider,
        &session,
        mode != "managed",
        &continuation
            .map(|c| ("observatory", &c.observatory))
            .into_iter()
            .chain(board.as_ref().map(|b| (FABRIC_BOARD, b)))
            .collect::<Vec<_>>(),
    )?;
    // SB-110: the session's context, fixed for the session (CX-26), for whatever reads it.
    let context = store
        .snapshot()?
        .session_context(&account, Some(&working_directory));
    let mut env = BTreeMap::from([
        ("SWITCHBOARD_SESSION".to_string(), session),
        ("SWITCHBOARD_CONTEXT".to_string(), context.clone()),
    ]);
    if let Some(c) = continuation {
        // The Stop hook records the workflow on the session; the handoff id is what it accepts.
        env.insert("OBSERVATORY_WORKFLOW_ID".into(), c.workflow_id.clone());
        env.insert("OBSERVATORY_HANDOFF_ID".into(), c.handoff_id.clone());
    }
    env.insert(
        match account.provider {
            Provider::Claude => "CLAUDE_CONFIG_DIR",
            Provider::Codex => "CODEX_HOME",
        }
        .into(),
        home.to_string_lossy().into_owned(),
    );
    if mode == "managed" {
        let (selected, _) = store.route(account.provider, &account.pool)?;
        if selected.id != account.id {
            return Err("Select this account before launching managed mode.".into());
        }
        let base = format!(
            "http://{}/{}/{}",
            proxy.address(),
            account.provider.as_str(),
            account.pool
        );
        match account.provider {
            Provider::Claude => {
                env.insert("ANTHROPIC_BASE_URL".into(), base);
                env.insert("ANTHROPIC_AUTH_TOKEN".into(), proxy.token().into());
                env.insert("ANTHROPIC_API_KEY".into(), String::new());
            }
            Provider::Codex => {
                env.insert("SWITCHBOARD_LOCAL_TOKEN".into(), proxy.token().into());
                let config=format!("model_provider = \"switchboard\"\ncli_auth_credentials_store = \"file\"\n[model_providers.switchboard]\nname = \"Fabric Switchboard\"\nbase_url = \"{base}/v1\"\nenv_key = \"SWITCHBOARD_LOCAL_TOKEN\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n{}", tools.codex_section);
                private_write(&home.join("config.toml"), config.as_bytes(), false)?;
            }
        }
    } else {
        let credential = store.credential(id)?;
        // Store::credential already refuses an expired credential; this guards the second
        // between that check and the copy. No provider was asked, so it is not a rejection.
        if credential
            .expires_at
            .is_some_and(|t| t <= switchboard_proxy::now())
        {
            return Err("Credential expired; reauthenticate this account".into());
        }
        match account.provider {
            Provider::Claude => {
                // Native working copy contains access only. No duplicate refresh-token writer.
                let name = if account.kind == AuthKind::ApiKey {
                    "ANTHROPIC_API_KEY"
                } else {
                    "CLAUDE_CODE_OAUTH_TOKEN"
                };
                // SB-137: the operator's settings (hooks, permissions, plugins), without what
                // signs in, then this account's own credential.
                let settings = crate::carry::claude_settings(
                    user.as_deref(),
                    Some(json!({name: credential.access_token})),
                );
                private_write(
                    &home.join("settings.json"),
                    serde_json::to_string(&settings).unwrap().as_bytes(),
                    false,
                )?;
            }
            Provider::Codex => {
                let auth = codex_auth_snapshot(
                    account.kind,
                    &credential,
                    time::OffsetDateTime::now_utc(),
                )?;
                private_write(
                    &home.join("auth.json"),
                    serde_json::to_vec(&auth).unwrap().as_slice(),
                    false,
                )?;
                private_write(
                    &home.join("config.toml"),
                    format!(
                        "cli_auth_credentials_store = \"file\"\n{}",
                        tools.codex_section
                    )
                    .as_bytes(),
                    false,
                )?;
            }
        }
    }
    let config = tools
        .claude_config
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    let first = continuation.map(crate::continuation::prompt);
    // One argument, `--mcp-config=<path>`: Claude's `--mcp-config <configs...>` is variadic and
    // would read the first prompt or a caller's plain value after a separate flag as another
    // config file; a caller's own `--mcp-config` still adds to it (SB-94, measured on 2.1.296).
    let config = config.map(|path| format!("--mcp-config={path}"));
    let mut args: Vec<&str> = config.iter().map(String::as_str).collect();
    // Both CLIs start an interactive session with a positional first message.
    if let Some(first) = &first {
        args.push(first.as_str());
    }
    args.extend(extra.iter().map(String::as_str));
    let path = write_launch_script(
        &home,
        &script(&home, &program, &args, &env, false, &working_directory),
    )?;
    // SB-95: what this home's session is, for the session list — ids, folder and times only.
    let record = json!({
        "account_id": account.id,
        "provider": account.provider.as_str(),
        "pool": account.pool,
        "context": context,
        "mode": mode,
        "working_directory": working_directory,
        "in_place": in_place,
        "launched_at": switchboard_proxy::now(),
        "continues": continuation.map(|c| c.workflow_id.clone()),
    });
    private_write(
        &home.join(SESSION_RECORD),
        record.to_string().as_bytes(),
        false,
    )?;
    // In place the caller runs the script now; the reservation lapses if it never does.
    if !in_place {
        (host.start_terminal)(&path)?;
    }
    reservation.committed = true;
    journal_launch(store, id, mode);
    Ok((cli.is_some(), in_place.then_some(path)))
}
/// Written beside `launch.command` by every Claude Code or Codex launch (SB-95).
pub(crate) const SESSION_RECORD: &str = ".session.json";
/// The sessions Switchboard launched, from the homes it owns (`homes/`, `runtimes/`): each
/// launch's record plus its state now — `starting` (the script has not run yet), `running` (its
/// process is the session) or `ended`; `unknown` when the marker cannot be read. Without `all`,
/// only sessions that start or run. Reads only; follows no link out of the owned folders.
pub fn sessions(root: &Path, all: bool) -> Vec<serde_json::Value> {
    let mut found = Vec::new();
    for folder in [
        "homes",
        "runtimes",
        "kimi/sessions",
        "runtimes/hermes-continue",
        "runtimes/opencode-continue",
    ] {
        let Ok(entries) = fs::read_dir(root.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let home = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some(mut record) = read_regular(&home.join(SESSION_RECORD))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .filter(serde_json::Value::is_object)
            else {
                continue;
            };
            let pending = fs::symlink_metadata(home.join(".launch-pending")).is_ok_and(|meta| {
                meta.file_type().is_file()
                    && !meta
                        .modified()
                        .is_ok_and(|t| reservation_expired(t, SystemTime::now()))
            });
            let state = if pending {
                "starting"
            } else {
                match session_running(&home) {
                    Ok(true) => "running",
                    Ok(false) => "ended",
                    Err(_) => "unknown",
                }
            };
            if !all && !matches!(state, "starting" | "running") {
                continue;
            }
            #[cfg(unix)]
            let marker = home.join(".session-pid");
            #[cfg(windows)]
            let marker = home.join(".session-process");
            let started = fs::symlink_metadata(&marker)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            record["state"] = state.into();
            record["started_at"] = started.into();
            record["home"] = folder.into();
            found.push(record);
        }
    }
    found.sort_by_key(|r| std::cmp::Reverse(r["launched_at"].as_i64().unwrap_or(0)));
    found
}
/// Terminal is already open: a journal failure must not report the launch as failed.
fn journal_launch(store: &Store, id: &str, mode: &str) {
    let _ = store.record("launch", Some(id), mode);
}
#[cfg(test)]
mod tests {
    #[test]
    fn a_project_account_starts_only_in_its_folders_and_its_folders_use_only_its_accounts() {
        use switchboard_core::{Account, Project, Snapshot};
        let account = |id: &str, pool: &str, provider: Provider| Account {
            id: id.into(),
            label: id.into(),
            provider,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        };
        let snapshot = Snapshot {
            accounts: vec![
                account("own", "alpha", Provider::Claude),
                account("other", "default", Provider::Claude),
                account("codex", "default", Provider::Codex),
            ],
            projects: vec![Project {
                pool: "alpha".into(),
                name: "Alpha".into(),
                folders: vec!["/work/web".into(), "/work/api".into()],
                created_at: 1,
                sources: vec![],
                organization: None,
            }],
            ..Default::default()
        };
        let a = |i: usize| snapshot.accounts[i].clone();
        // Its own account: inside any of its folders, never outside.
        assert!(project_allows(&snapshot, &a(0), Path::new("/work/api/src")).is_ok());
        assert_eq!(
            project_allows(&snapshot, &a(0), Path::new("/work/other")).unwrap_err(),
            PROJECT_ACCOUNT_OUTSIDE
        );
        // Inside, another pool's Claude account is refused while the project has one.
        assert_eq!(
            project_allows(&snapshot, &a(1), Path::new("/work/web")).unwrap_err(),
            PROJECT_FOLDER_FOREIGN
        );
        // A provider the project has no account for keeps working there.
        assert!(project_allows(&snapshot, &a(2), Path::new("/work/web")).is_ok());
        // Outside every project, ordinary accounts are unaffected.
        assert!(project_allows(&snapshot, &a(1), Path::new("/elsewhere")).is_ok());
        // `/work/webapp` is not inside `/work/web`.
        assert!(project_allows(&snapshot, &a(1), Path::new("/work/webapp")).is_ok());
        // SB-109: once an organization exists, the personal group refuses its seat of Acme's.
        let mut with_org = snapshot.clone();
        with_org.organizations.push(switchboard_core::Organization {
            id: "00000000-0000-4000-8000-000000000001".into(),
            name: "Acme".into(),
            domains: vec![],
            provider_organizations: vec![],
            created_at: 1,
            vesta_id: None,
        });
        with_org
            .memberships
            .insert("own".into(), "00000000-0000-4000-8000-000000000001".into());
        assert_eq!(
            project_allows(&with_org, &with_org.accounts[0], Path::new("/work/web")).unwrap_err(),
            switchboard_core::organizations::SEAT_OUTSIDE
        );
        with_org.projects[0].organization = Some("00000000-0000-4000-8000-000000000001".into());
        assert!(project_allows(&with_org, &with_org.accounts[0], Path::new("/work/web")).is_ok());
    }

    use super::*;
    #[test]
    fn launched_sessions_get_switchboard_tools_with_exact_quoting() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        private_dir(&home).unwrap();
        let cli = temp.path().join("O'Brien \"x\" \\ δ").join("switchboard");
        let tools = agent_tools(
            Some(&cli),
            temp.path(),
            &home,
            Provider::Claude,
            "managed:claude:work",
            false,
            &[],
        )
        .unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(tools.claude_config.unwrap()).unwrap()).unwrap();
        let server = &config["mcpServers"]["switchboard"];
        assert_eq!(server["command"], cli.to_string_lossy().as_ref());
        assert_eq!(
            server["args"],
            json!(["--data-dir", temp.path().to_string_lossy(), "mcp"])
        );
        assert_eq!(server["env"]["SWITCHBOARD_SESSION"], "managed:claude:work");

        let tools = agent_tools(
            Some(&cli),
            temp.path(),
            &home,
            Provider::Codex,
            "isolated:codex:x",
            true,
            &[],
        )
        .unwrap();
        let parsed: toml::Value =
            toml::from_str(&format!("model = \"x\"\n{}", tools.codex_section)).unwrap();
        let server = &parsed["mcp_servers"]["switchboard"];
        assert_eq!(server["command"].as_str().unwrap(), cli.to_string_lossy());
        assert_eq!(
            server["args"].as_array().unwrap().last().unwrap().as_str(),
            Some("--read-only")
        );
        assert_eq!(
            server["env"]["SWITCHBOARD_SESSION"].as_str(),
            Some("isolated:codex:x")
        );

        let none = agent_tools(
            None,
            temp.path(),
            &home,
            Provider::Claude,
            "managed:claude:work",
            false,
            &[],
        )
        .unwrap();
        assert!(none.claude_config.is_none() && none.codex_section.is_empty());
    }
    /// An absolute program path on this OS: the board is carried only with an absolute command.
    #[cfg(windows)]
    const BOARD: &str = r"C:\Board\fabric-mcp.exe";
    #[cfg(not(windows))]
    const BOARD: &str = "/Applications/Board.app/bin/fabric-mcp";
    #[test]
    fn the_operators_fabric_board_reaches_a_launched_session_without_its_env() {
        // SB-20: a runtime or isolated home has its own scope, so the board the operator
        // registered in theirs is carried in; its env is not (it may hold a secret).
        let temp = tempfile::tempdir().unwrap();
        let user = temp.path().join("user");
        fs::create_dir_all(user.join(".codex")).unwrap();
        fs::write(
            user.join(".claude.json"),
            json!({"mcpServers": {
                "fabric": {"type": "stdio", "command": BOARD,
                           "args": ["--runner", "claude-code"], "env": {"TOKEN": "never-copied"}},
                "other": {"type": "stdio", "command": "/opt/other", "args": []}
            }})
            .to_string(),
        )
        .unwrap();
        fs::write(
            user.join(".codex/config.toml"),
            format!("[mcp_servers.fabric]\ncommand = '{BOARD}'\nargs = [\"--runner\", \"codex\"]\nenv = {{ TOKEN = \"never-copied\" }}\n"),
        )
        .unwrap();
        let claude = user_board_server(Provider::Claude, &user, None).unwrap();
        assert_eq!(claude.command, BOARD);
        assert_eq!(claude.args, ["--runner", "claude-code"]);
        let codex = user_board_server(Provider::Codex, &user, None).unwrap();
        assert_eq!(codex.args, ["--runner", "codex"]);
        let off = temp.path().join("off");
        fs::create_dir_all(off.join(".codex")).unwrap();
        fs::write(
            off.join(".codex/config.toml"),
            format!("[mcp_servers.fabric]\ncommand = '{BOARD}'\nenabled = false\n"),
        )
        .unwrap();
        assert!(
            user_board_server(Provider::Codex, &off, None).is_none(),
            "a board the operator switched off stays off"
        );

        let home = temp.path().join("home");
        private_dir(&home).unwrap();
        let cli = temp.path().join("switchboard");
        let tools = agent_tools(
            Some(&cli),
            temp.path(),
            &home,
            Provider::Claude,
            "managed:claude:work",
            false,
            &[("fabric", &claude)],
        )
        .unwrap();
        let written = fs::read_to_string(tools.claude_config.unwrap()).unwrap();
        let config: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(
            config["mcpServers"]["fabric"],
            json!({"type": "stdio", "command": claude.command, "args": claude.args})
        );
        assert!(
            config["mcpServers"]["switchboard"].is_object(),
            "Switchboard's own tools stay"
        );
        assert!(
            config["mcpServers"].get("other").is_none(),
            "only the board is carried"
        );
        assert!(!written.contains("never-copied"));
        let tools = agent_tools(
            Some(&cli),
            temp.path(),
            &home,
            Provider::Codex,
            "isolated:codex:x",
            true,
            &[("fabric", &codex)],
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&tools.codex_section).unwrap();
        assert_eq!(
            parsed["mcp_servers"]["fabric"]["args"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(!tools.codex_section.contains("never-copied"));
        // The board alone is enough to write a config: a session without Switchboard's CLI still gets it.
        let alone = agent_tools(
            None,
            temp.path(),
            &home,
            Provider::Claude,
            "managed:claude:work",
            false,
            &[("fabric", &claude)],
        )
        .unwrap();
        assert!(alone.claude_config.is_some());
    }
    #[test]
    fn no_registered_board_or_a_doubtful_entry_means_no_board_never_a_failed_launch() {
        let temp = tempfile::tempdir().unwrap();
        let user = temp.path();
        assert!(
            user_board_server(Provider::Claude, user, None).is_none(),
            "no ~/.claude.json"
        );
        assert!(
            user_board_server(Provider::Codex, user, None).is_none(),
            "no config.toml"
        );
        for entry in [
            json!({"type": "http", "url": "https://board.example/mcp"}),
            json!({"type": "stdio", "command": "fabric-mcp", "args": []}),
            json!({"type": "stdio", "command": BOARD, "args": [1]}),
            json!({"type": "stdio", "args": []}),
        ] {
            fs::write(
                user.join(".claude.json"),
                json!({"mcpServers": {"fabric": entry}}).to_string(),
            )
            .unwrap();
            assert!(
                user_board_server(Provider::Claude, user, None).is_none(),
                "{entry}"
            );
        }
        fs::write(user.join(".claude.json"), "{\"mcpServers\": {\"fab").unwrap();
        assert!(
            user_board_server(Provider::Claude, user, None).is_none(),
            "half-written"
        );
        fs::write(
            user.join(".claude.json"),
            json!({"mcpServers": {"fabric": {"command": BOARD}}}).to_string(),
        )
        .unwrap();
        assert_eq!(
            user_board_server(Provider::Claude, user, None)
                .unwrap()
                .args,
            Vec::<String>::new(),
            "no type is stdio, no args is none"
        );
        let codex_home = user.join("elsewhere");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("config.toml"),
            "[mcp_servers.fabric]\nurl = \"https://board.example/mcp\"\n",
        )
        .unwrap();
        assert!(
            user_board_server(Provider::Codex, user, Some(&codex_home)).is_none(),
            "CODEX_HOME is read, and a URL server is not carried"
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_native_script_scopes_env_and_completes_synthetic_login() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("O'Brien δοκιμή");
        private_dir(&home).unwrap();
        let provider = home.join("synthetic-provider.ps1");
        // The provider reports what it was started with; the paths are compared below
        // as the file system resolves them, because the same directory has more than
        // one spelling on Windows (an 8.3 short name in TEMP on a hosted runner).
        private_write(
            &provider,
            b"if ($env:ANTHROPIC_API_KEY) { exit 9 }\n\
              [IO.File]::WriteAllText((Join-Path $env:CODEX_HOME 'seen-home'), $env:CODEX_HOME)\n\
              [IO.File]::WriteAllText((Join-Path $env:CODEX_HOME 'seen-cwd'), (Get-Location).ProviderPath)\n\
              exit 0\n",
            false,
        )
        .unwrap();
        let env = BTreeMap::from([("CODEX_HOME".into(), home.to_string_lossy().into_owned())]);
        let content = windows_script(&home, &provider, &[], &env, true, &home);
        let script_path = home.join("test.ps1");
        private_write(&script_path, format!("\u{feff}{content}").as_bytes(), false).unwrap();
        let mut reservation = Reservation::new(&home).unwrap();
        reservation.committed = true;
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script_path)
            .env("ANTHROPIC_API_KEY", "synthetic-conflicting-key")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{:?}\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let resolved = |name: &str| {
            let seen = read_regular(&home.join(name)).unwrap();
            std::fs::canonicalize(&seen).unwrap_or_else(|e| panic!("{name} {seen:?}: {e}"))
        };
        let expected = std::fs::canonicalize(&home).unwrap();
        assert_eq!(
            resolved("seen-home"),
            expected,
            "CODEX_HOME is the managed home"
        );
        assert_eq!(
            resolved("seen-cwd"),
            expected,
            "the provider runs in the working directory"
        );
        assert_eq!(read_regular(&home.join(".completed")).unwrap(), "complete");
        assert!(ensure_idle(&home).is_ok());
    }
    #[test]
    fn windows_script_quotes_literals_and_tracks_process_incarnation() {
        let env = BTreeMap::from([("CODEX_HOME".into(), r"C:\Users\O'Brien\δοκιμή".into())]);
        let content = windows_script(
            Path::new(r"C:\private"),
            Path::new(r"C:\Program Files\codex.cmd"),
            &["-c", "cli_auth_credentials_store=\"file\"", "login"],
            &env,
            true,
            Path::new(r"C:\my project"),
        );
        assert!(content.contains("O''Brien"));
        assert!(content.contains("ToFileTimeUtc()"));
        assert!(content.contains("Set-Location -LiteralPath 'C:\\my project'"));
        assert!(content.contains("'cli_auth_credentials_store=\"file\"'"));
        assert!(content.contains("$null, 'Process'"));
        assert!(!content.contains("Invoke-Expression"));
        assert_eq!(powershell_quote("x'$()`;&"), "'x''$()`;&'");
    }
    #[test]
    fn codex_export_preserves_native_auth_shape_without_refresh_lineage() {
        let credential = Credential::parse(
            Provider::Codex,
            AuthKind::OAuth,
            r#"{"tokens":{"access_token":"synthetic-access","refresh_token":"synthetic-refresh-must-not-export","id_token":"header.eyJzdWIiOiJzeW50aGV0aWMifQ.signature","account_id":"synthetic-account"}}"#,
        ).unwrap();
        let written_at = time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let value = codex_auth_snapshot(AuthKind::OAuth, &credential, written_at).unwrap();
        assert_eq!(value["auth_mode"], "chatgpt");
        assert!(value["OPENAI_API_KEY"].is_null());
        assert_eq!(value["tokens"]["access_token"], "synthetic-access");
        assert_eq!(value["tokens"]["account_id"], "synthetic-account");
        assert_eq!(
            value["tokens"]["id_token"],
            credential.id_token.as_deref().unwrap()
        );
        assert_eq!(value["tokens"]["refresh_token"], "");
        let timestamp = time::OffsetDateTime::parse(
            value["last_refresh"].as_str().unwrap(),
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!(timestamp, written_at);
        assert!(!serde_json::to_string(&value)
            .unwrap()
            .contains("synthetic-refresh-must-not-export"));

        let api_key =
            Credential::parse(Provider::Codex, AuthKind::ApiKey, "synthetic-api-key").unwrap();
        assert_eq!(
            codex_auth_snapshot(AuthKind::ApiKey, &api_key, written_at).unwrap(),
            json!({
                "auth_mode":"apikey", "OPENAI_API_KEY":"synthetic-api-key"
            })
        );
        let mut missing_identity = credential;
        missing_identity.id_token = None;
        assert!(codex_auth_snapshot(AuthKind::OAuth, &missing_identity, written_at).is_err());
        assert!(codex_auth_snapshot(AuthKind::SetupToken, &api_key, written_at).is_err());
    }
    #[test]
    fn codex_login_cleanup_is_retry_safe_after_home_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("synthetic-login");
        fs::create_dir(&home).unwrap();
        fs::write(home.join("auth.json"), "synthetic-test-file").unwrap();
        let login = Login {
            id: Uuid::new_v4().to_string(),
            provider: Provider::Codex,
            label: "Synthetic account".into(),
            pool: "default".into(),
            saved: None,
            signed_in_again: false,
            home,
        };
        clean_login(&login).unwrap();
        assert!(!login.home.exists());
        clean_login(&login).unwrap();
    }
    #[cfg(not(windows))]
    #[test]
    fn script_quotes_paths_and_clears_credentials() {
        let env = BTreeMap::from([("CODEX_HOME".into(), "/tmp/a'b".into())]);
        let s = script(
            Path::new("/tmp/a'b"),
            Path::new("/tmp/cli space"),
            &["login"],
            &env,
            true,
            Path::new("/tmp/project"),
        );
        assert!(s.contains("'/tmp/a'\\''b'"));
        assert!(s.contains("unset ANTHROPIC_API_KEY"));
        assert!(s.contains(".completed"));
        assert!(!s.contains("set -x"));
    }
    #[test]
    fn pending_launch_excludes_rewrite_and_is_released_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let hold = Reservation::new(root.path()).unwrap();
        assert!(Reservation::new(root.path()).is_err());
        assert!(ensure_idle(root.path()).is_err());
        drop(hold);
        assert!(ensure_idle(root.path()).is_ok());
    }
    #[cfg(not(windows))]
    #[test]
    fn launch_script_runs_project_without_changing_auth_home() {
        let env = BTreeMap::from([("CODEX_HOME".into(), "/tmp/private home".into())]);
        let text = script(
            Path::new("/tmp/private home"),
            Path::new("/bin/true"),
            &[],
            &env,
            false,
            Path::new("/tmp/project"),
        );
        assert!(text.contains("export CODEX_HOME='/tmp/private home'"));
        assert!(text.contains("cd '/tmp/project' || exit 1"));
        assert!(text.contains("rm -f .launch-pending"));
    }
    #[test]
    fn service_matches_derived_name() {
        let digest = format!("{:x}", Sha256::digest(b"/tmp/example"));
        assert_eq!(
            keychain_service(Path::new("/tmp/example")),
            format!("Claude Code-credentials-{}", &digest[..8])
        );
    }
    #[test]
    fn journal_failure_after_terminal_opened_is_not_a_launch_failure() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(
            root.path().to_owned(),
            Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap();
        let id = Uuid::new_v4().to_string();
        journal_launch(&store, &id, "managed");
        assert_eq!(store.snapshot().unwrap().events.len(), 1);
        let metadata = root.path().join("accounts.json");
        fs::remove_file(&metadata).unwrap();
        fs::create_dir(&metadata).unwrap();
        let (): () = journal_launch(&store, &id, "isolated");
        assert_eq!(store.snapshot().unwrap().events.len(), 1);
    }
    #[test]
    fn login_state_reads_completion_and_an_exited_terminal() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("login");
        std::fs::create_dir_all(&home).unwrap();
        let account = switchboard_core::Account {
            id: Uuid::new_v4().to_string(),
            label: "x".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        };
        let login = fixture_login(home.clone(), account);
        assert_eq!(login_state(&login), "pending");
        // A session process that no longer runs, and no completion marker: the sign-in
        // ended. Each platform's script leaves its own marker.
        #[cfg(unix)]
        private_write(&home.join(".session-pid"), b"999999", false).unwrap();
        #[cfg(windows)]
        private_write(
            &home.join(".session-process"),
            br#"{"pid":999999,"created":1}"#,
            false,
        )
        .unwrap();
        assert_eq!(login_state(&login), "ended");
        private_write(&home.join(".completed"), b"complete", false).unwrap();
        assert_eq!(login_state(&login), "complete");
    }
    #[test]
    fn a_linux_terminal_is_chosen_by_terminal_then_by_the_known_list() {
        let only = |names: &'static [&'static str]| {
            move |name: &str| {
                names
                    .contains(&name)
                    .then(|| PathBuf::from(format!("/usr/bin/{name}")))
            }
        };
        let none = only(&[]);
        assert_eq!(
            linux_terminal(None, none).unwrap_err(),
            LINUX_TERMINAL_MISSING
        );
        // Debian's alternative wins over a desktop's own terminal; each gets its own arguments.
        let (program, arguments) =
            linux_terminal(None, only(&["konsole", "x-terminal-emulator"])).unwrap();
        assert_eq!(
            (program, arguments),
            (PathBuf::from("/usr/bin/x-terminal-emulator"), &["-e"][..])
        );
        let (program, arguments) =
            linux_terminal(None, only(&["gnome-terminal", "xterm"])).unwrap();
        assert_eq!(
            (program, arguments),
            (PathBuf::from("/usr/bin/gnome-terminal"), &["--"][..])
        );
        // TERMINAL is honoured first: a known name keeps its arguments, an unknown one gets `-e`.
        let (program, arguments) =
            linux_terminal(Some("kitty".into()), only(&["kitty", "xterm"])).unwrap();
        assert_eq!(program, PathBuf::from("/usr/bin/kitty"));
        assert!(arguments.is_empty());
        let (_, arguments) = linux_terminal(Some("myterm".into()), only(&["myterm"])).unwrap();
        assert_eq!(arguments, &["-e"][..]);
        // A TERMINAL that is not installed falls back to the list instead of failing.
        let (program, _) = linux_terminal(Some("missing-term".into()), only(&["xterm"])).unwrap();
        assert_eq!(program, PathBuf::from("/usr/bin/xterm"));
        assert!(linux_terminal(Some("".into()), only(&["foot"]))
            .unwrap()
            .1
            .is_empty());
    }
    /// SB-88: on Linux a sign-in's script runs in the terminal end to end — `/bin/sh`, its session
    /// marker, the provider program, the completion marker — through a stand-in terminal that runs
    /// what it is given the way `xterm -e` does.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_terminal_runs_the_generated_script_to_completion() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("login home");
        private_dir(&home).unwrap();
        let terminal = temp.path().join("stand-in-term");
        private_write(
            &terminal,
            b"#!/bin/sh\n[ \"$1\" = -e ] || exit 9\nshift\nexec \"$@\"\n",
            true,
        )
        .unwrap();
        let provider = temp.path().join("provider");
        private_write(
            &provider,
            b"#!/bin/sh\n[ \"$1\" = auth ] && [ \"$2\" = login ] || exit 3\n",
            true,
        )
        .unwrap();
        let text = script(
            &home,
            &provider,
            &["auth", "login"],
            &BTreeMap::new(),
            true,
            &home,
        );
        assert!(text.starts_with("#!/bin/sh\n"));
        let path = write_launch_script(&home, &text).unwrap();
        spawn_linux_terminal(&path, Some(terminal.into_os_string()), |_| None).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !home.join(".completed").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the script did not complete"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            fs::read_to_string(home.join(".completed")).unwrap(),
            "complete"
        );
        assert!(home.join(".session-pid").exists());
        // No terminal at all: the person is told what to install.
        assert_eq!(
            spawn_linux_terminal(&path, None, |_| None).unwrap_err(),
            LINUX_TERMINAL_MISSING
        );
    }
    #[test]
    fn rejects_invalid_fields() {
        assert!(validate_fields("x", "../../escape").is_err());
        assert!(validate_fields("\n", "default").is_err());
        assert!(validate_fields("Work", "team-a").is_ok());
        assert!(
            validate_fields("", "default").is_ok(),
            "email names it at finish"
        );
        assert!(validate_fields(&"x".repeat(81), "default").is_err());
    }
    fn age(path: &Path, seconds: u64) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(seconds))
            .unwrap();
    }
    #[test]
    fn reservation_expires_after_ten_minutes_in_either_direction() {
        let now = std::time::SystemTime::now();
        let minutes = |m: u64| Duration::from_secs(m * 60);
        assert!(!reservation_expired(now - minutes(9), now));
        assert!(!reservation_expired(now, now));
        assert!(reservation_expired(now - minutes(11), now));
        // A clock set back leaves the marker's age unknowable; it must not hold the home forever.
        assert!(!reservation_expired(now + minutes(9), now));
        assert!(reservation_expired(now + minutes(11), now));
    }
    #[test]
    fn stale_reservation_no_longer_holds_the_home() {
        let root = tempfile::tempdir().unwrap();
        let mut hold = Reservation::new(root.path()).unwrap();
        hold.committed = true; // Terminal was asked to run the script, which never ran.
        drop(hold);
        age(&root.path().join(".launch-pending"), 9 * 60);
        assert!(
            ensure_idle(root.path()).is_err(),
            "a fresh launch still holds"
        );
        age(&root.path().join(".launch-pending"), 11 * 60);
        assert!(ensure_idle(root.path()).is_ok());
        assert!(!root.path().join(".launch-pending").exists());
        assert!(Reservation::new(root.path()).is_ok());
    }
    #[test]
    fn sign_in_whose_terminal_never_ran_can_be_cancelled_once_its_reservation_expires() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("login");
        private_dir(&home).unwrap();
        let mut hold = Reservation::new(&home).unwrap();
        hold.committed = true;
        drop(hold);
        let login = Login {
            id: Uuid::new_v4().to_string(),
            provider: Provider::Codex,
            label: "Synthetic".into(),
            pool: "default".into(),
            saved: None,
            signed_in_again: false,
            home: home.clone(),
        };
        assert_eq!(cancel_login(&login).unwrap_err(), SIGN_IN_STARTING);
        age(&home.join(".launch-pending"), 11 * 60);
        cancel_login(&login).unwrap();
        assert!(!home.exists());
    }
    /// A sign-in home whose `launch.command` runs `body` the way Terminal runs it: a shell leading
    /// its own process group that records `.session-pid` first. A thread reaps it, as Terminal's
    /// login shell would, and reports when it exited. Returns once the script reached the last
    /// line of `body` (a `.ready` mark before it): every `trap` is set and the pid is written in
    /// full, so a signal never lands in the gap — that gap made these tests fail under load.
    #[cfg(unix)]
    fn terminal_session(
        root: &Path,
        body: &str,
    ) -> (Login, std::sync::mpsc::Receiver<std::process::ExitStatus>) {
        use std::os::unix::process::CommandExt;
        let home = root.join("logins").join(Uuid::new_v4().to_string());
        private_dir(&home).unwrap();
        let (setup, run) = body.rsplit_once('\n').unwrap_or(("", body));
        let script = write_launch_script(
            &home,
            &format!(
                "cd {} || exit 1\n(umask 077; printf '%s' \"$$\" > .session-pid)\n{setup}\n: > .ready\n{run}\n",
                quote(home.to_string_lossy().as_ref())
            ),
        )
        .unwrap();
        let mut child = Command::new("/bin/sh")
            .arg(&script)
            .process_group(0)
            .spawn()
            .unwrap();
        let (sent, exited) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sent.send(child.wait().unwrap());
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !home.join(".ready").exists() {
            assert!(std::time::Instant::now() < deadline, "the script never ran");
            std::thread::sleep(Duration::from_millis(20));
        }
        let login = Login {
            id: Uuid::new_v4().to_string(),
            provider: Provider::Codex,
            label: "Synthetic".into(),
            pool: "default".into(),
            saved: None,
            signed_in_again: false,
            home,
        };
        (login, exited)
    }
    #[cfg(unix)]
    #[test]
    fn cancelling_a_waiting_sign_in_ends_its_terminal_session_and_cleans_up() {
        let root = tempfile::tempdir().unwrap();
        // The provider CLI waits for a code that never comes, as `claude auth login` does.
        let (login, exited) = terminal_session(root.path(), "sleep 60 & wait $!");
        assert_eq!(login_state(&login), "pending");
        let started = std::time::Instant::now();
        cancel_login(&login).unwrap();
        assert!(
            started.elapsed() < END_WAIT,
            "closing the window's signal ends it"
        );
        exited.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!login.home.exists());
    }
    #[cfg(unix)]
    #[test]
    fn a_session_that_ignores_the_hang_up_is_terminated() {
        let root = tempfile::tempdir().unwrap();
        let (login, exited) =
            terminal_session(root.path(), "trap '' HUP\nwhile :; do sleep 1; done");
        end_sign_in_within(&login.home, Duration::from_millis(500)).unwrap();
        exited.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(ensure_idle(&login.home).is_ok());
    }
    #[test]
    fn a_sign_in_folder_made_a_moment_ago_is_never_swept() {
        // SB-103: the sweep runs beside a starting owner; a sign-in begun right then is young.
        let root = tempfile::tempdir().unwrap();
        let young = root.path().join("logins").join(Uuid::new_v4().to_string());
        private_dir(&young).unwrap();
        let sweep = sweep_logins_with(
            root.path(),
            &|_| LeftoverItem::Absent,
            &|_| Ok(()),
            Duration::from_secs(60),
        );
        assert!(young.exists());
        assert_eq!(sweep, Sweep::default());
    }
    #[cfg(unix)]
    #[test]
    fn a_sign_in_home_from_before_its_journal_is_swept_once_its_terminal_ended() {
        // SB-105 (4): the leftover found on the operator's Mac, 2026-10-12 — a sign-in begun on
        // 2026-10-08 by a build without `.login.json` (pre-SB-65), its Terminal shell gone.
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("logins").join(Uuid::new_v4().to_string());
        private_dir(&home).unwrap();
        private_dir(&home.join("backups")).unwrap();
        private_write(&home.join(".session-pid"), b"999999", false).unwrap();
        private_write(&home.join("launch.command"), b"#!/bin/sh\n", false).unwrap();
        private_write(&home.join(".claude.json"), b"{}", false).unwrap();
        assert!(login_journal(&home).is_none());
        assert!(adopt_logins(root.path()).is_empty(), "nothing to take back");
        let sweep = sweep_logins_with(
            root.path(),
            &|_| LeftoverItem::Absent,
            &|_| Ok(()),
            Duration::ZERO,
        );
        assert_eq!(sweep.removed, 1);
        assert!(!home.exists());
    }
    #[cfg(unix)]
    #[test]
    fn a_start_removes_forgotten_sign_ins_and_keeps_what_still_runs_or_would_ask() {
        // SB-87: an earlier owner's sign-in homes, as a crash or a quit leaves them.
        let root = tempfile::tempdir().unwrap();
        let logins = root.path().join("logins");
        let home = |name: &str| {
            let home = logins.join(name);
            private_dir(&home).unwrap();
            home
        };
        let idle = home(&Uuid::new_v4().to_string());
        let with_item = home(&Uuid::new_v4().to_string());
        let asks = home(&Uuid::new_v4().to_string());
        let starting = home(&Uuid::new_v4().to_string());
        fs::write(starting.join(".launch-pending"), "").unwrap();
        let not_ours = home("notes");
        let (running, exited) = terminal_session(root.path(), "sleep 60 & wait $!");
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), logins.join(Uuid::new_v4().to_string()))
            .unwrap();

        let deleted = std::cell::RefCell::new(Vec::new());
        let item = |h: &Path| {
            if h == with_item {
                LeftoverItem::Silent
            } else if h == asks {
                LeftoverItem::Asks
            } else {
                LeftoverItem::Absent
            }
        };
        let delete = |h: &Path| {
            deleted.borrow_mut().push(h.to_path_buf());
            Ok(())
        };
        let sweep = sweep_logins_with(root.path(), &item, &delete, Duration::ZERO);
        assert_eq!(
            sweep,
            Sweep {
                removed: 2,
                running: 2,
                finishable: 0,
                kept: 1
            }
        );
        assert!(!idle.exists() && !with_item.exists());
        assert_eq!(
            *deleted.borrow(),
            std::slice::from_ref(&with_item),
            "only the silent item is deleted"
        );
        assert!(asks.exists(), "an item that would ask stays, with its home");
        assert!(
            starting.exists() && running.home.exists(),
            "a sign-in still open is kept"
        );
        assert!(not_ours.exists(), "only UUID-named homes are Switchboard's");
        assert!(outside.path().exists(), "a link is never followed");

        // A failed Keychain delete keeps the home, so its item is never orphaned.
        let failing = home(&Uuid::new_v4().to_string());
        let sweep = sweep_logins_with(
            root.path(),
            &|h| {
                if h == failing {
                    LeftoverItem::Silent
                } else {
                    item(h)
                }
            },
            &|_| Err("Keychain unavailable.".into()),
            Duration::ZERO,
        );
        assert!(failing.exists());
        assert_eq!(sweep.removed, 0);

        let pid: i32 = fs::read_to_string(running.home.join(".session-pid"))
            .unwrap()
            .parse()
            .unwrap();
        unsafe { libc::kill(-pid, libc::SIGKILL) };
        exited.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(sweep_logins_with(root.path(), &item, &delete, Duration::ZERO).removed >= 1);
        assert!(
            !running.home.exists(),
            "removed by a later start once it ended"
        );
    }
    #[cfg(unix)]
    #[test]
    fn a_sign_in_open_across_a_restart_is_taken_back_and_kept_from_the_sweep() {
        // SB-65: an earlier owner's sign-ins, with the journal begin_login writes.
        let root = tempfile::tempdir().unwrap();
        let journal = |home: &Path, provider: &str, label: &str, at: i64| {
            fs::write(
                home.join(LOGIN_JOURNAL),
                json!({"provider": provider, "label": label, "pool": "work", "started_at": at})
                    .to_string(),
            )
            .unwrap();
        };
        let home = |name: String| {
            let home = root.path().join("logins").join(name);
            private_dir(&home).unwrap();
            home
        };
        let completed = home(Uuid::new_v4().to_string());
        journal(&completed, "codex", "Team", 10);
        fs::write(completed.join(".completed"), "complete").unwrap();
        let (running, exited) = terminal_session(root.path(), "sleep 60 & wait $!");
        journal(&running.home, "claude", "Studio", 20);
        let ended = home(Uuid::new_v4().to_string());
        journal(&ended, "claude", "Gone", 30);
        let no_journal = home(Uuid::new_v4().to_string());
        fs::write(no_journal.join(".completed"), "complete").unwrap();
        let bad = home(Uuid::new_v4().to_string());
        fs::write(
            bad.join(LOGIN_JOURNAL),
            r#"{"provider":"gemini","label":"x","pool":"work"}"#,
        )
        .unwrap();
        fs::write(bad.join(".completed"), "complete").unwrap();

        let adopted = adopt_logins(root.path());
        let names: Vec<(&str, Provider, &str)> = adopted
            .iter()
            .map(|l| (l.label.as_str(), l.provider, l.pool.as_str()))
            .collect();
        assert_eq!(
            names,
            [("Studio", Provider::Claude, "work"), ("Team", Provider::Codex, "work")],
            "newest first; an ended one, one without a journal and an unknown provider are not taken"
        );
        assert_eq!(login_state(&adopted[1]), "complete");
        assert_eq!(login_state(&adopted[0]), "pending");
        // SB-103: the owner orders them by when they began, from the journal.
        assert_eq!(
            (login_started(&adopted[1]), login_started(&adopted[0])),
            (10, 20)
        );
        // SB-103: once its account is saved, a sign-in is never taken back again, even while
        // its folder is still there.
        forget_login_journal(&adopted[1]);
        assert_eq!(
            adopt_logins(root.path())
                .iter()
                .map(|l| l.label.as_str())
                .collect::<Vec<_>>(),
            ["Studio"]
        );
        journal(&completed, "codex", "Team", 10);

        let sweep = sweep_logins_with(
            root.path(),
            &|_| LeftoverItem::Absent,
            &|_| Ok(()),
            Duration::ZERO,
        );
        assert!(
            completed.exists(),
            "a completed sign-in with its journal waits for Finish"
        );
        assert!(running.home.exists());
        assert!(!ended.exists() && !no_journal.exists() && !bad.exists());
        assert_eq!((sweep.finishable, sweep.running, sweep.removed), (1, 1, 3));

        let pid: i32 = fs::read_to_string(running.home.join(".session-pid"))
            .unwrap()
            .parse()
            .unwrap();
        unsafe { libc::kill(-pid, libc::SIGKILL) };
        exited.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn a_session_that_will_not_end_keeps_its_home_and_says_so() {
        let root = tempfile::tempdir().unwrap();
        let (login, exited) =
            terminal_session(root.path(), "trap '' HUP TERM\nwhile :; do sleep 1; done");
        assert_eq!(
            end_sign_in_within(&login.home, Duration::from_millis(300)).unwrap_err(),
            SIGN_IN_WOULD_NOT_END
        );
        assert!(login.home.join(".session-pid").exists());
        let pid: i32 = fs::read_to_string(login.home.join(".session-pid"))
            .unwrap()
            .parse()
            .unwrap();
        unsafe { libc::kill(-pid, libc::SIGKILL) };
        exited.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn a_live_process_that_is_not_this_sign_in_is_never_signalled() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("login");
        private_dir(&home).unwrap();
        // The marker names a live process of the same age that is not this home's script.
        let mut other = Command::new("/bin/sleep").arg("60").spawn().unwrap();
        private_write(
            &home.join(".session-pid"),
            other.id().to_string().as_bytes(),
            false,
        )
        .unwrap();
        let login = Login {
            id: Uuid::new_v4().to_string(),
            provider: Provider::Codex,
            label: "Synthetic".into(),
            pool: "default".into(),
            saved: None,
            signed_in_again: false,
            home: home.clone(),
        };
        assert_eq!(cancel_login(&login).unwrap_err(), SIGN_IN_WOULD_NOT_END);
        assert!(
            other.try_wait().unwrap().is_none(),
            "another process was signalled"
        );
        assert!(home.exists());
        other.kill().unwrap();
        other.wait().unwrap();
    }
    #[test]
    fn keychain_user_prefers_a_set_user_then_the_passwd_entry() {
        let unused = || -> Option<String> { panic!("passwd must not be read when USER is set") };
        assert_eq!(pick_user(Some("alice".into()), unused).unwrap(), "alice");
        assert_eq!(
            pick_user(Some(String::new()), || Some("bob".into())).unwrap(),
            "bob"
        );
        assert_eq!(pick_user(None, || Some("bob".into())).unwrap(), "bob");
        assert!(pick_user(None, || Some(String::new())).is_err());
        assert!(pick_user(Some(String::new()), || None).is_err());
    }
    #[test]
    fn ps_elapsed_time_forms_parse_and_malformed_ones_do_not() {
        let s = Duration::from_secs;
        assert_eq!(parse_elapsed("00:05"), Some(s(5)));
        assert_eq!(parse_elapsed("  12:34\n"), Some(s(12 * 60 + 34)));
        assert_eq!(parse_elapsed("01:02:03"), Some(s(3723)));
        assert_eq!(parse_elapsed("2-01:02:03"), Some(s(2 * 86_400 + 3723)));
        for bad in [
            "", "5", "1:2:3:4", "aa:bb", "-01:00", "01:60", "x-01:00", "+1:00", "1-2",
        ] {
            assert_eq!(parse_elapsed(bad), None, "{bad:?}");
        }
    }
    #[test]
    fn a_process_younger_than_its_marker_is_a_reused_pid() {
        let s = Duration::from_secs;
        assert!(pid_reused(s(3600), s(10)));
        assert!(!pid_reused(s(3600), s(3590)), "within the margin");
        assert!(
            !pid_reused(s(10), s(3600)),
            "the session shell predates its marker"
        );
        assert!(!pid_reused(s(60), s(0)));
        assert!(pid_reused(s(61), s(0)));
    }
    #[cfg(unix)]
    #[test]
    fn live_pid_holds_the_home_only_while_it_can_be_the_session() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join(".session-pid");
        // This test process is alive and seconds old: a marker as fresh is its session.
        private_write(&marker, std::process::id().to_string().as_bytes(), false).unwrap();
        assert!(ensure_idle(root.path()).is_err());
        // A month-old marker cannot name a process started seconds ago: the pid was reused.
        age(&marker, 30 * 86_400);
        assert!(ensure_idle(root.path()).is_ok());
    }

    fn native_never(_: Provider) -> Result<PathBuf, String> {
        panic!("tests must not resolve a real provider CLI")
    }
    fn synthetic_program(_: Provider) -> Result<PathBuf, String> {
        Ok(PathBuf::from("/opt/Provider Tools/o'cli"))
    }
    fn no_agent_cli() -> Option<PathBuf> {
        None
    }
    fn no_board(_: Provider) -> Option<McpServer> {
        None
    }
    fn synthetic_agent_cli() -> Option<PathBuf> {
        Some(PathBuf::from("/opt/Switchboard Tools/switchboard"))
    }
    fn terminal_opens(_: &Path) -> Result<(), String> {
        Ok(())
    }
    fn terminal_fails(_: &Path) -> Result<(), String> {
        Err("Terminal could not open.".into())
    }
    fn no_user_scope(_: Provider) -> Option<PathBuf> {
        None
    }
    const TEST_HOST: Host = Host {
        binary: synthetic_program,
        agent_cli: no_agent_cli,
        board: no_board,
        start_terminal: terminal_opens,
        user_scope: no_user_scope,
    };
    fn openrouter_key(f: &Fixture, model: Option<&str>) {
        f.store
            .set_agent_key(
                "openrouter",
                "sk-or-v1-synthetic-must-not-reach-disk",
                model.map(str::to_owned),
            )
            .unwrap();
    }
    fn openrouter_launch(
        f: &Fixture,
        agent: &str,
        model: Option<&str>,
    ) -> Result<(PathBuf, Value), String> {
        openrouter_session(
            &f.root,
            &f.store,
            &crate::agent_catalog::openrouter(agent).unwrap(),
            Path::new("/opt/Agent Tools/agent"),
            Path::new("/opt/Switchboard Tools/switch'board"),
            model,
            &f.project,
        )
    }
    /// SB-201: an organization whose group holds the fixture's folder (pool `acme`), with
    /// `accounts` assigned to it and moved into the group; its id.
    fn acme_group(f: &Fixture, accounts: &[String]) -> String {
        let acme = f.store.add_organization("Acme", &[]).unwrap();
        for id in accounts {
            f.store.assign_organization(id, Some(&acme.id)).unwrap();
        }
        let group = f
            .store
            .save_project_in(
                None,
                "Acme",
                &[f.project.canonicalize().unwrap()],
                accounts,
                Some(Some(&acme.id)),
            )
            .unwrap();
        assert_eq!(group.pool, "acme");
        acme.id
    }
    /// The launch record a session folder holds.
    fn recorded(session: &Path) -> Value {
        serde_json::from_slice(&fs::read(session.join(SESSION_RECORD)).unwrap()).unwrap()
    }
    #[cfg(not(windows))]
    #[test]
    fn finished_hermes_hand_overs_go_after_a_week_and_nothing_else_does() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        for name in [
            "wf_0000000000000001",
            "wf_0000000000000002",
            "agent-home",
            "notes",
        ] {
            fs::create_dir(base.join(name)).unwrap();
        }
        let now = SystemTime::now() + Duration::from_secs(8 * 86_400);
        // A hand-over whose session still runs is kept, however old.
        fs::write(
            base.join("wf_0000000000000002/.session-pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        prune_hermes_sessions(base, now);
        assert!(!base.join("wf_0000000000000001").exists());
        assert!(base.join("wf_0000000000000002").exists());
        assert!(base.join("agent-home").exists() && base.join("notes").exists());
        // A recent one stays.
        fs::create_dir(base.join("wf_0000000000000003")).unwrap();
        prune_hermes_sessions(base, SystemTime::now());
        assert!(base.join("wf_0000000000000003").exists());
    }
    #[test]
    fn hermes_config_escapes_every_value_into_plain_ascii() {
        let config = hermes_config(&crate::continuation::McpServer {
            command: "/opt/a \"b\"\\c".into(),
            args: vec!["x\u{7f}y\u{2028}z\u{1F600}\nw".into()],
        });
        assert!(config
            .bytes()
            .all(|b| b == b'\n' || (b' '..=b'~').contains(&b)));
        assert!(config.contains(r#"command: "/opt/a \"b\"\\c""#));
        assert!(config.contains(r#"args: ["x\u007Fy\u2028z\U0001F600\u000Aw"]"#));
    }
    #[cfg(not(windows))]
    #[test]
    fn a_first_turn_goes_before_the_final_exec_even_when_a_path_reads_like_one() {
        let dir = tempfile::tempdir().unwrap();
        let program = Path::new("/opt/my\nexec tools/kimi");
        let script = script_then(
            dir.path(),
            program,
            &["-p", "hi"],
            &["-c"],
            &BTreeMap::new(),
            dir.path(),
        );
        let guard = script.find("if ! ").unwrap();
        let resume = script.rfind("' '-c'").unwrap();
        assert!(guard < resume, "{script}");
        assert!(script[guard..].starts_with(&format!(
            "if ! {} '-p' 'hi'; then",
            quote("/opt/my\nexec tools/kimi")
        )));
        let path = dir.path().join("check.command");
        fs::write(&path, &script).unwrap();
        let shell = script.lines().next().unwrap().trim_start_matches("#!");
        assert!(std::process::Command::new(shell)
            .arg("-n")
            .arg(&path)
            .status()
            .unwrap()
            .success());
    }
    #[cfg(not(windows))]
    #[test]
    fn an_opencode_continuation_runs_on_its_own_config_and_never_holds_the_key() {
        // SB-119: OpenCode takes a workflow on the saved OpenRouter key and its model, through
        // OPENCODE_CONFIG_CONTENT only; the person's own OpenCode files are never written.
        let f = fixture();
        let continuation = crate::continuation::Continuation {
            workflow_id: "wf_00112233445566aa".into(),
            handoff_id: "ho_synthetic".into(),
            observatory: crate::continuation::McpServer {
                command: "/opt/Engine \"Tools\"/python".into(),
                args: vec!["/opt/engine/mcp/server.py".into()],
            },
            previous_provider: Some("claude".into()),
        };
        let session = |f: &Fixture| {
            opencode_continuation_session(
                &f.root,
                &f.store,
                Path::new("/opt/Agent Tools/opencode"),
                Path::new("/opt/Switchboard Tools/switch'board"),
                &f.project,
                &continuation,
            )
        };
        assert_eq!(
            session(&f).unwrap_err(),
            switchboard_core::agent_keys::NO_AGENT_KEY
        );
        openrouter_key(&f, None);
        assert_eq!(session(&f).unwrap_err(), OPENROUTER_MODEL_NEEDED);
        openrouter_key(&f, Some("xiaomi/mimo-v2.6-pro"));
        let (path, answer) = session(&f).unwrap();
        assert_eq!(answer["agent"], "opencode");
        assert_eq!(answer["model"], "xiaomi/mimo-v2.6-pro");
        let base = f.root.join("runtimes/opencode-continue");
        assert!(path.starts_with(base.join("wf_00112233445566aa")));
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("sk-or-"), "the key reached the script");
        assert!(text.contains("exec '/opt/Agent Tools/opencode' '--prompt' "));
        assert!(text.contains("'-m' 'openrouter/xiaomi/mimo-v2.6-pro'"));
        assert!(text.contains("export OPENCODE_CONFIG_CONTENT="));
        assert!(
            text.contains("'agents' 'key' '--service' 'openrouter' '--dir' "),
            "the key of the folder's project (SB-144)"
        );
        let shell = text.lines().next().unwrap().trim_start_matches("#!");
        assert!(std::process::Command::new(shell)
            .arg("-n")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        // The config the script exports: the key by reference, the model declared, Observatory.
        let config: Value = serde_json::from_str(&opencode_config(
            "xiaomi/mimo-v2.6-pro",
            &continuation.observatory,
        ))
        .unwrap();
        assert_eq!(
            config["provider"]["openrouter"]["options"]["apiKey"],
            "{env:OPENROUTER_API_KEY}"
        );
        assert!(config["provider"]["openrouter"]["models"]["xiaomi/mimo-v2.6-pro"].is_object());
        assert_eq!(
            config["mcp"]["observatory"]["command"],
            json!(["/opt/Engine \"Tools\"/python", "/opt/engine/mcp/server.py"])
        );
        assert_eq!(config["mcp"]["observatory"]["type"], "local");
        let first =
            fs::read_to_string(base.join("wf_00112233445566aa/handoff-prompt.txt")).unwrap();
        assert!(first.contains("ho_synthetic") && first.contains("owner agent:opencode"));
        let id = first
            .split("sessionId ")
            .last()
            .unwrap()
            .trim_end_matches('.');
        assert!(uuid::Uuid::parse_str(id).is_ok());
        // Listed under Sessions with its model.
        let listed = sessions(&f.root, true);
        let opencode = listed
            .iter()
            .find(|r| r["provider"] == "opencode")
            .expect("the hand-over is listed");
        assert_eq!(opencode["continues"], "wf_00112233445566aa");
        assert_eq!(opencode["label"], "xiaomi/mimo-v2.6-pro");
        // No OpenCode file of the person's was made: nothing outside the session folder.
        assert!(!f
            .root
            .join("runtimes/opencode-continue/agent-home")
            .exists());
        // SB-201: the hand-over records and exports its context. The machine's key is
        // personal, also in an organization's folder; the group's own key is the group's.
        let folder = base.join("wf_00112233445566aa");
        assert_eq!(recorded(&folder)["context"], "personal");
        assert!(text.contains("export SWITCHBOARD_CONTEXT='personal'"));
        let acme = acme_group(&f, &[]);
        session(&f).unwrap();
        assert_eq!(recorded(&folder)["context"], "personal");
        f.store
            .set_agent_key_for(
                "openrouter",
                Some("acme"),
                "sk-or-v1-synthetic-group-key",
                Some("xiaomi/mimo-v2.6-pro".into()),
            )
            .unwrap();
        let (path, _) = session(&f).unwrap();
        assert_eq!(recorded(&folder)["context"], format!("org:{acme}"));
        assert!(fs::read_to_string(path)
            .unwrap()
            .contains(&format!("export SWITCHBOARD_CONTEXT='org:{acme}'")));
    }
    #[cfg(not(windows))]
    #[test]
    fn a_hermes_continuation_seeds_its_session_with_the_handoff_and_never_holds_the_key() {
        // SB-107: Hermes takes a workflow on the saved OpenRouter key and its model.
        let f = fixture();
        let continuation = crate::continuation::Continuation {
            workflow_id: "wf_0123456789abcdef".into(),
            handoff_id: "ho_synthetic".into(),
            observatory: crate::continuation::McpServer {
                command: "/opt/Engine \"Tools\"/python".into(),
                args: vec![
                    "/opt/engine/mcp/server.py".into(),
                    "--home".into(),
                    "/opt/h".into(),
                ],
            },
            previous_provider: Some("claude".into()),
        };
        let session = |f: &Fixture| {
            hermes_continuation_session(
                &f.root,
                &f.store,
                Path::new("/opt/Agent Tools/hermes"),
                Path::new("/opt/Switchboard Tools/switch'board"),
                &f.project,
                &continuation,
                None,
            )
        };
        assert_eq!(
            session(&f).unwrap_err(),
            switchboard_core::agent_keys::NO_AGENT_KEY
        );
        openrouter_key(&f, None);
        assert_eq!(session(&f).unwrap_err(), OPENROUTER_MODEL_NEEDED);
        openrouter_key(&f, Some("moonshotai/kimi-k2"));
        let (path, answer) = session(&f).unwrap();
        assert_eq!(answer["model"], "moonshotai/kimi-k2");
        assert_eq!(answer["model_from"], "key");
        assert_eq!(
            answer["script"],
            json!(path),
            "for a continuation in place (SB-108)"
        );
        let base = f.root.join("runtimes/hermes-continue");
        assert!(path.starts_with(base.join("wf_0123456789abcdef")));
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("sk-or-"), "the key reached the script");
        let prompt = base.join("wf_0123456789abcdef/handoff-prompt.txt");
        assert!(text.contains(&format!(
            "exec '/opt/Agent Tools/hermes' 'chat' '--query-file' {} '--provider' 'openrouter' '-m' 'moonshotai/kimi-k2'",
            quote(prompt.to_string_lossy().as_ref())
        )));
        assert!(text.contains(&format!(
            "export HERMES_HOME={}",
            quote(base.join("agent-home").to_string_lossy().as_ref())
        )));
        assert!(
            text.contains("'agents' 'key' '--service' 'openrouter' '--dir' "),
            "the key of the folder's project (SB-144)"
        );
        let shell = text.lines().next().unwrap().trim_start_matches("#!");
        assert!(std::process::Command::new(shell)
            .arg("-n")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        let first = fs::read_to_string(&prompt).unwrap();
        assert!(first.contains("ho_synthetic") && first.contains("wf_0123456789abcdef"));
        // The owner and a UUID session id are named: Hermes's own ids are not UUIDs.
        let id = first
            .split("sessionId ")
            .last()
            .unwrap()
            .trim_end_matches('.');
        assert!(first.contains("owner agent:hermes") && uuid::Uuid::parse_str(id).is_ok());
        // Hermes's own OpenRouter model wins over the key's default.
        let (path, answer) = hermes_continuation_session(
            &f.root,
            &f.store,
            Path::new("/opt/Agent Tools/hermes"),
            Path::new("/opt/Switchboard Tools/switch'board"),
            &f.project,
            &continuation,
            Some("deepseek/deepseek-v4-pro-0813"),
        )
        .unwrap();
        assert_eq!(answer["model_from"], "hermes");
        // Listed under Sessions, labelled with its model.
        let listed = sessions(&f.root, true);
        let hermes = listed
            .iter()
            .find(|r| r["provider"] == "hermes")
            .expect("the hand-over is listed");
        assert_eq!(hermes["continues"], "wf_0123456789abcdef");
        assert_eq!(hermes["label"], "deepseek/deepseek-v4-pro-0813");
        assert_eq!(hermes["home"], "runtimes/hermes-continue");
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("'-m' 'deepseek/deepseek-v4-pro-0813'"));
        let config = fs::read_to_string(base.join("agent-home/config.yaml")).unwrap();
        assert!(config.contains("adopt_external_logins: false"));
        assert!(config.contains("command: \"/opt/Engine \\\"Tools\\\"/python\""));
        assert!(config.contains("args: [\"/opt/engine/mcp/server.py\", \"--home\", \"/opt/h\"]"));
        {
            use std::os::unix::fs::PermissionsExt;
            for file in [&prompt, &base.join("agent-home/config.yaml")] {
                let mode = fs::metadata(file).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600, "{}", file.display());
            }
        }
        // Never in Switchboard's own data, never a malformed workflow id.
        let mut bad = continuation.clone();
        bad.workflow_id = "../../etc".into();
        assert!(hermes_continuation_session(
            &f.root,
            &f.store,
            Path::new("/bin/true"),
            Path::new("/bin/true"),
            &f.project,
            &bad,
            None
        )
        .is_err());
        assert!(hermes_continuation_session(
            &f.root,
            &f.store,
            Path::new("/bin/true"),
            Path::new("/bin/true"),
            &f.root,
            &continuation,
            None
        )
        .is_err());
        // SB-201: the hand-over records and exports its context. The machine's key is
        // personal, also in an organization's folder; the group's own key is the group's.
        let folder = base.join("wf_0123456789abcdef");
        assert_eq!(recorded(&folder)["context"], "personal");
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("export SWITCHBOARD_CONTEXT='personal'"));
        let acme = acme_group(&f, &[]);
        session(&f).unwrap();
        assert_eq!(recorded(&folder)["context"], "personal");
        f.store
            .set_agent_key_for(
                "openrouter",
                Some("acme"),
                "sk-or-v1-synthetic-group-key",
                Some("moonshotai/kimi-k2".into()),
            )
            .unwrap();
        let (path, _) = session(&f).unwrap();
        assert_eq!(recorded(&folder)["context"], format!("org:{acme}"));
        assert!(fs::read_to_string(path)
            .unwrap()
            .contains(&format!("export SWITCHBOARD_CONTEXT='org:{acme}'")));
    }
    #[test]
    fn an_openrouter_launch_needs_a_saved_key_and_a_valid_model() {
        let f = fixture();
        assert_eq!(
            openrouter_launch(&f, "hermes", Some("moonshotai/kimi-k2")).unwrap_err(),
            switchboard_core::agent_keys::NO_AGENT_KEY
        );
        openrouter_key(&f, None);
        assert_eq!(
            openrouter_launch(&f, "hermes", None).unwrap_err(),
            OPENROUTER_MODEL_NEEDED
        );
        assert_eq!(
            openrouter_launch(&f, "hermes", Some("no model; rm -rf")).unwrap_err(),
            switchboard_core::agent_keys::MODEL_INVALID
        );
        // An agent that picks its model itself starts without one.
        let (_, answer) = openrouter_launch(&f, "crush", None).unwrap();
        assert_eq!(answer["model"], Value::Null);
        assert_eq!(answer["model_choice"], "agent");
        // Nothing outside an existing folder outside Switchboard's data.
        let inside = openrouter_session(
            &f.root,
            &f.store,
            &crate::agent_catalog::openrouter("goose").unwrap(),
            Path::new("/bin/true"),
            Path::new("/bin/true"),
            Some("a/b"),
            &f.root,
        );
        assert!(inside.is_err());
    }
    #[test]
    fn an_openrouter_session_reads_the_key_when_it_starts_and_never_holds_it() {
        let f = fixture();
        openrouter_key(&f, Some("moonshotai/kimi-k2"));
        let (path, answer) = openrouter_launch(&f, "hermes", None).unwrap();
        assert_eq!(answer["via"], "openrouter");
        assert_eq!(answer["model"], "moonshotai/kimi-k2");
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("sk-or-"), "the key reached the script");
        let home = f.root.join("runtimes/agent-hermes-openrouter");
        assert!(path.starts_with(&home));
        assert!(home.join("agent-home").is_dir());
        let (_, answer) = openrouter_launch(&f, "kimi-code", Some("qwen/qwen3-coder")).unwrap();
        assert_eq!(answer["model"], "qwen/qwen3-coder");
        let kimi = fs::read_to_string(f.root.join("runtimes/agent-kimi-code-openrouter").join(
            if cfg!(windows) {
                "launch.ps1"
            } else {
                "launch.command"
            },
        ))
        .unwrap();
        for expected in [
            "KIMI_MODEL_BASE_URL",
            "https://openrouter.ai/api/v1",
            "KIMI_MODEL_NAME",
            "qwen/qwen3-coder",
            "KIMI_MODEL_PROVIDER_TYPE",
        ] {
            assert!(kimi.contains(expected), "{expected}");
        }
        assert!(!kimi.contains("sk-or-"));
        #[cfg(not(windows))]
        {
            assert!(text.contains(&format!(
                "export HERMES_HOME={}",
                quote(home.join("agent-home").to_string_lossy().as_ref())
            )));
            assert!(text.contains(
                "OPENROUTER_API_KEY=\"$('/opt/Switchboard Tools/switch'\\''board' '--data-dir' "
            ));
            assert!(
                text.contains("'agents' 'key' '--service' 'openrouter' '--dir' "),
                "the key of the folder's project (SB-144)"
            );
            assert!(text.contains("export OPENROUTER_API_KEY\n"));
            assert!(text.contains(
                "exec '/opt/Agent Tools/agent' '--provider' 'openrouter' '-m' 'moonshotai/kimi-k2'"
            ));
            assert!(text.contains("unset ANTHROPIC_API_KEY"));
        }
    }
    /// SB-201: an agent launched on the OpenRouter key writes a session record, so Sessions
    /// lists it with its model and context; the context follows the key it runs on.
    #[test]
    fn an_openrouter_launch_records_its_session_and_context() {
        let f = fixture();
        openrouter_key(&f, Some("moonshotai/kimi-k2"));
        let (path, _) = openrouter_launch(&f, "goose", None).unwrap();
        let home = f.root.join("runtimes/agent-goose-openrouter");
        let record = recorded(&home);
        assert_eq!(record["provider"], "goose");
        // Readers name the agent from one field, whatever route it runs on.
        assert_eq!(record["agent"], "goose");
        assert_eq!(record["account_id"], "goose");
        assert_eq!(record["pool"], "openrouter");
        assert_eq!(record["label"], "moonshotai/kimi-k2");
        assert_eq!(record["mode"], "isolated");
        assert_eq!(record["context"], "personal");
        assert_eq!(
            record["working_directory"],
            f.project.canonicalize().unwrap().to_string_lossy().as_ref()
        );
        assert!(!fs::read_to_string(home.join(SESSION_RECORD))
            .unwrap()
            .contains("sk-or-"));
        let script = fs::read_to_string(&path).unwrap();
        assert!(script.contains("SWITCHBOARD_CONTEXT"));
        #[cfg(not(windows))]
        assert!(script.contains("export SWITCHBOARD_CONTEXT='personal'"));
        let listed = sessions(&f.root, true);
        let goose = listed
            .iter()
            .find(|r| r["provider"] == "goose")
            .expect("the launch is listed");
        assert_eq!(goose["home"], "runtimes");
        // An agent that picks its model itself has no model to show.
        openrouter_launch(&f, "crush", None).unwrap();
        assert!(recorded(&f.root.join("runtimes/agent-crush-openrouter"))["label"].is_null());
        // In an organization's folder: the machine's key is personal, the group's key the group's.
        let acme = acme_group(&f, &[]);
        openrouter_launch(&f, "goose", None).unwrap();
        assert_eq!(recorded(&home)["context"], "personal");
        f.store
            .set_agent_key_for(
                "openrouter",
                Some("acme"),
                "sk-or-v1-synthetic-group-key",
                Some("qwen/qwen3-coder".into()),
            )
            .unwrap();
        let (path, _) = openrouter_launch(&f, "goose", None).unwrap();
        let record = recorded(&home);
        assert_eq!(record["context"], format!("org:{acme}"));
        assert_eq!(record["label"], "qwen/qwen3-coder");
        #[cfg(not(windows))]
        assert!(fs::read_to_string(path)
            .unwrap()
            .contains(&format!("export SWITCHBOARD_CONTEXT='org:{acme}'")));
    }
    fn goose_installed(_: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/opt/Agent Tools/goose"))
    }
    /// SB-201: an agent launched through the proxy on an API-key account records its session
    /// and the account's context, as a Claude Code launch does.
    #[tokio::test]
    async fn a_proxy_agent_launch_records_its_context() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let api = f.add(
            Provider::Claude,
            AuthKind::ApiKey,
            "default",
            "synthetic-api-key",
        );
        f.store.select(Provider::Claude, "default", &api).unwrap();
        let answer = launch_agent_with(
            &f.root,
            &f.store,
            &proxy,
            "goose",
            "default",
            &f.project,
            goose_installed,
            terminal_opens,
        )
        .unwrap();
        assert_eq!(answer["agent"], "goose");
        let home = f.root.join("runtimes/agent-goose-default");
        let record = recorded(&home);
        // `provider` is the provider the proxy route serves, so a reader comparing it with the
        // route never sees a move; the agent is its own field.
        assert_eq!(record["provider"], "claude");
        assert_eq!(record["agent"], "goose");
        assert_eq!(record["account_id"], api.as_str());
        assert_eq!(record["pool"], "default");
        assert_eq!(record["mode"], "managed");
        assert_eq!(record["context"], "personal");
        assert!(!fs::read_to_string(home.join(SESSION_RECORD))
            .unwrap()
            .contains("synthetic"));
        let script = fs::read_to_string(home.join(if cfg!(windows) {
            "launch.ps1"
        } else {
            "launch.command"
        }))
        .unwrap();
        assert!(script.contains("SWITCHBOARD_CONTEXT"));
        #[cfg(not(windows))]
        assert!(script.contains("export SWITCHBOARD_CONTEXT='personal'"));
        assert!(sessions(&f.root, true)
            .iter()
            .any(|r| r["provider"] == "claude"
                && r["agent"] == "goose"
                && r["account_id"] == api.as_str()));
        // Codex through the proxy: its id must not pass for the Codex provider.
        launch_agent_with(
            &f.root,
            &f.store,
            &proxy,
            "codex",
            "default",
            &f.project,
            goose_installed,
            terminal_opens,
        )
        .unwrap();
        let record = recorded(&f.root.join("runtimes/agent-codex-default"));
        assert_eq!(record["provider"], "claude");
        assert_eq!(record["agent"], "codex");
        // The account is an organization's seat in its group: the group's context.
        let acme = acme_group(&f, std::slice::from_ref(&api));
        f.store.select(Provider::Claude, "acme", &api).unwrap();
        launch_agent_with(
            &f.root,
            &f.store,
            &proxy,
            "goose",
            "acme",
            &f.project,
            goose_installed,
            terminal_opens,
        )
        .unwrap();
        let record = recorded(&f.root.join("runtimes/agent-goose-acme"));
        assert_eq!(record["context"], format!("org:{acme}"));
        assert_eq!(record["pool"], "acme");
    }
    #[test]
    fn a_windows_session_reads_the_key_by_command_and_exits_when_it_cannot() {
        let env = BTreeMap::from([("GOOSE_PROVIDER".into(), "openrouter".into())]);
        let arguments = [
            "agents".to_string(),
            "key".into(),
            "--service".into(),
            "openrouter".into(),
        ];
        let text = windows_script_with(
            Path::new("C:/Data/runtimes/agent-goose-openrouter"),
            Path::new("C:/Tools/goose.exe"),
            &[],
            &env,
            Some((
                "OPENROUTER_API_KEY",
                Path::new("C:/Program Files/Switchboard/switch'board.exe"),
                &arguments,
            )),
            false,
            Path::new("C:/Projects/app"),
        );
        assert!(text.contains(
            "$switchboardSecret = & 'C:/Program Files/Switchboard/switch''board.exe' @('agents', 'key', '--service', 'openrouter')"
        ));
        assert!(text.contains("if ($LASTEXITCODE -ne 0 -or -not $switchboardSecret) {"));
        assert!(text.contains(
            "[Environment]::SetEnvironmentVariable('OPENROUTER_API_KEY', [string]$switchboardSecret, 'Process')"
        ));
        assert!(text.contains("Remove-Variable switchboardSecret"));
        // The key is read after the conflicting variables are cleared, before the agent runs.
        let read = text.find("$switchboardSecret = &").unwrap();
        assert!(text.find("'OPENAI_API_KEY', $null").unwrap() < read);
        assert!(read < text.find("& 'C:/Tools/goose.exe'").unwrap());
    }
    /// Runs the script a launch writes under zsh with a stand-in CLI and agent: the agent sees the
    /// key in its environment, and a CLI that cannot answer stops the session before the agent.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_session_script_hands_the_key_to_the_agent_and_stops_without_it() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let tool = |name: &str, body: &str| {
            let path = temp.path().join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            path
        };
        let seen = temp.path().join("seen");
        let agent = tool(
            "agent",
            &format!(
                "printf '%s|%s' \"$OPENROUTER_API_KEY\" \"$1\" > {}",
                quote(seen.to_string_lossy().as_ref())
            ),
        );
        let run = |cli: &Path| {
            let arguments = ["agents".to_string(), "key".into()];
            let text = script_with(
                &home,
                &agent,
                &["--model"],
                &BTreeMap::new(),
                Some(("OPENROUTER_API_KEY", cli, &arguments)),
                false,
                temp.path(),
            );
            let path = home.join("launch.command");
            private_write(&path, text.as_bytes(), true).unwrap();
            Command::new(SCRIPT_SHELL).arg(&path).output().unwrap()
        };
        let answers = tool("cli-ok", "printf 'sk-or-v1-synthetic\\n'");
        assert!(run(&answers).status.success());
        assert_eq!(
            fs::read_to_string(&seen).unwrap(),
            "sk-or-v1-synthetic|--model"
        );
        fs::remove_file(&seen).unwrap();
        for failing in [tool("cli-fails", "exit 3"), tool("cli-silent", "exit 0")] {
            let output = run(&failing);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains(KEY_UNREADABLE));
            assert!(!seen.exists(), "the agent ran without a key");
        }
    }
    struct Fixture {
        temp: tempfile::TempDir,
        root: PathBuf,
        project: PathBuf,
        vault: Arc<switchboard_core::MemoryVault>,
        store: Arc<Store>,
    }
    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        private_dir(&root).unwrap();
        // Windows file names cannot hold a double quote.
        let project = temp.path().join(if cfg!(windows) {
            "My project o'neil"
        } else {
            "My \"project\" o'neil"
        });
        fs::create_dir(&project).unwrap();
        let vault = Arc::new(switchboard_core::MemoryVault::default());
        let store = Arc::new(Store::open(root.clone(), vault.clone()).unwrap());
        Fixture {
            temp,
            root,
            project,
            vault,
            store,
        }
    }
    impl Fixture {
        fn add(&self, provider: Provider, kind: AuthKind, pool: &str, material: &str) -> String {
            let credential = Credential::parse(provider, kind, material).unwrap();
            self.store
                .add("Synthetic".into(), provider, kind, pool.into(), credential)
                .unwrap()
                .id
        }
        fn launch(
            &self,
            proxy: &ProxyHandle,
            id: &str,
            mode: &str,
            host: &Host,
        ) -> Result<bool, String> {
            launch_with(
                &self.root,
                &self.store,
                proxy,
                id,
                mode,
                &self.project,
                host,
                None,
            )
        }
        /// Every file below the fixture's temporary directory, so a test can prove where a
        /// launch wrote.
        fn files(&self) -> std::collections::BTreeSet<PathBuf> {
            fn walk(dir: &Path, out: &mut std::collections::BTreeSet<PathBuf>) {
                for entry in fs::read_dir(dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        walk(&path, out);
                    } else {
                        out.insert(path);
                    }
                }
            }
            let mut out = std::collections::BTreeSet::new();
            walk(self.temp.path(), &mut out);
            out
        }
    }
    const CLAUDE_OAUTH: &str = r#"{"claudeAiOauth":{"accessToken":"synthetic-access","refreshToken":"synthetic-refresh-must-not-export"}}"#;
    #[tokio::test]
    async fn isolated_claude_launch_writes_only_the_access_token_into_its_home() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let oauth = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let api = f.add(
            Provider::Claude,
            AuthKind::ApiKey,
            "default",
            "synthetic-api-key",
        );
        let before = f.files();
        assert!(!f.launch(&proxy, &oauth, "isolated", &TEST_HOST).unwrap());
        assert!(!f.launch(&proxy, &api, "isolated", &TEST_HOST).unwrap());

        let home = f.root.join("homes").join(&oauth);
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            settings,
            json!({"env": {"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-access"}})
        );
        let settings: serde_json::Value = serde_json::from_slice(
            &fs::read(f.root.join("homes").join(&api).join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            settings,
            json!({"env": {"ANTHROPIC_API_KEY": "synthetic-api-key"}})
        );
        // The reservation stays until the script itself removes it.
        assert!(home.join(".launch-pending").exists());
        assert!(f.launch(&proxy, &oauth, "isolated", &TEST_HOST).is_err());

        let private_root = f.root.canonicalize().unwrap();
        for path in f.files().difference(&before) {
            let path = path.canonicalize().unwrap();
            assert!(
                path.starts_with(&private_root),
                "{path:?} is outside the root"
            );
            if path.file_name().is_some_and(|n| n != "accounts.json") {
                assert!(!fs::read_to_string(&path)
                    .unwrap()
                    .contains("synthetic-refresh-must-not-export"));
            }
        }
        assert_eq!(fs::read_dir(&f.project).unwrap().count(), 0);
        let events = f.store.snapshot().unwrap().events;
        assert_eq!(events.iter().filter(|e| e.action == "launch").count(), 2);
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn an_in_place_launch_prepares_the_same_session_and_opens_no_terminal() {
        // SB-75: the host (an embedded console) runs the returned script itself. Terminal must
        // not open; the agent's extra arguments come after Switchboard's, quoted.
        fn terminal_must_not_open(_: &Path) -> Result<(), String> {
            panic!("an in-place launch opened Terminal");
        }
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: terminal_must_not_open,
            user_scope: no_user_scope,
        };
        let extra = vec!["--resume".to_string(), "it's; rm -rf ~".to_string()];
        let (_, script) = launch_full(
            &f.root, &f.store, &proxy, &id, "isolated", &f.project, &host, None, &extra, true,
        )
        .unwrap();
        let script = script.expect("an in-place launch returns its script");
        let home = f.root.join("homes").join(&id);
        assert_eq!(script, home.join("launch.command"));
        let text = fs::read_to_string(&script).unwrap();
        assert!(
            text.contains(".session-pid"),
            "the one-session marker is written as before"
        );
        assert!(
            text.contains("export SWITCHBOARD_SESSION="),
            "the same environment"
        );
        assert!(
            text.contains(&format!(
                " {} {}\n",
                quote("--resume"),
                quote("it's; rm -rf ~")
            )),
            "extra arguments last, each quoted: {text}"
        );
        // Bad arguments are refused before a home is touched.
        for bad in [
            vec!["line\nbreak".to_string()],
            vec!["x".repeat(MAX_EXTRA_ARG + 1)],
            vec!["a".to_string(); MAX_EXTRA_ARGS + 1],
        ] {
            assert_eq!(
                launch_full(
                    &f.root, &f.store, &proxy, &id, "isolated", &f.project, &host, None, &bad, true
                )
                .unwrap_err(),
                EXTRA_ARGS_INVALID
            );
        }
    }
    /// Runs an in-place session script with the provider program replaced by a recorder, and
    /// returns the arguments and environment the agent would have started with.
    #[cfg(not(windows))]
    fn run_as_the_agent(script: &Path, extra_env: &[(&str, &str)]) -> (Vec<String>, String) {
        let temp = tempfile::tempdir().unwrap();
        let args = temp.path().join("args");
        let env = temp.path().join("env");
        let recorder = temp.path().join("agent");
        private_write(
            &recorder,
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\0' \"$a\"; done > {}\nenv > {}\n",
                quote(&args.to_string_lossy()),
                quote(&env.to_string_lossy())
            )
            .as_bytes(),
            true,
        )
        .unwrap();
        let text = fs::read_to_string(script).unwrap();
        let program = quote(
            &synthetic_program(Provider::Claude)
                .unwrap()
                .to_string_lossy(),
        );
        assert!(text.contains(&program), "{text}");
        let runnable = temp.path().join("launch.command");
        private_write(
            &runnable,
            text.replace(&program, &quote(&recorder.to_string_lossy()))
                .as_bytes(),
            true,
        )
        .unwrap();
        let status = std::process::Command::new(SCRIPT_SHELL)
            .arg(&runnable)
            .envs(extra_env.iter().copied())
            .status()
            .unwrap();
        assert!(status.success());
        let args = fs::read(&args).unwrap();
        let args = args
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8(a.to_vec()).unwrap())
            .collect();
        (args, fs::read_to_string(&env).unwrap())
    }
    /// Claude's `--mcp-config <configs...>` is variadic (measured on Claude Code 2.1.296: a
    /// plain value after it is read as one more config file). A bare value may follow a
    /// separate `--mcp-config` flag only as its one config, then an option must come next.
    #[cfg(not(windows))]
    fn no_value_is_read_as_a_config(args: &[String]) {
        for (i, arg) in args.iter().enumerate() {
            if arg == "--mcp-config" {
                if let Some(next) = args.get(i + 2) {
                    assert!(
                        next.starts_with('-'),
                        "{next:?} would be read as a config: {args:?}"
                    );
                }
            }
        }
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn a_dashboards_console_launch_reaches_the_agent_as_sent() {
        // SB-94: Fabric Dashboards' console (FD-39) hands its context through `launch --in-place
        // --` — its own MCP config and brief for Claude, `-c` overrides and a first prompt for
        // Codex. Both must reach the agent unchanged after Switchboard's own arguments, both MCP
        // configs must load, and its context variables must survive the environment scrub.
        fn terminal_must_not_open(_: &Path) -> Result<(), String> {
            panic!("an in-place launch opened Terminal");
        }
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: terminal_must_not_open,
            user_scope: no_user_scope,
        };
        let context = [
            (
                "FABRIC_DASHBOARDS_CONTEXT",
                "/Users/x/Library/Application Support/fabric-dashboards/consoles/svc-1",
            ),
            ("FABRIC_DASHBOARDS_TASK", "fix"),
        ];

        let claude = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        f.store
            .select(Provider::Claude, "default", &claude)
            .unwrap();
        let theirs =
            "/Users/x/Library/Application Support/fabric-dashboards/consoles/svc-1/mcp.json";
        let brief = format!("You are the console of 'svc-1'. {}", "b".repeat(960));
        // fabric-dashboards 47e5aa6 (`src/core/handoff.ts`): the joined form, as Switchboard's.
        let theirs = format!("--mcp-config={theirs}");
        let sent: Vec<String> = [
            theirs.as_str(),
            "--append-system-prompt",
            &brief,
            "--continue",
            "Fix the failing health check",
        ]
        .map(str::to_owned)
        .to_vec();
        let (_, script) = launch_full(
            &f.root, &f.store, &proxy, &claude, "managed", &f.project, &host, None, &sent, true,
        )
        .unwrap();
        let (args, env) = run_as_the_agent(&script.unwrap(), &context);
        let ours = f
            .root
            .join("runtimes")
            .join("claude-default")
            .join("switchboard-mcp.json");
        let mut expected = vec![format!("--mcp-config={}", ours.to_string_lossy())];
        expected.extend(sent.iter().cloned());
        assert_eq!(args, expected);
        no_value_is_read_as_a_config(&args);
        for (key, value) in context {
            assert!(
                env.contains(&format!("{key}={value}\n")),
                "{key} was scrubbed"
            );
        }

        let codex = f.add(
            Provider::Codex,
            AuthKind::ApiKey,
            "team",
            "synthetic-api-key",
        );
        f.store.select(Provider::Codex, "team", &codex).unwrap();
        let sent: Vec<String> = [
            "-c",
            "mcp_servers.fabric-dashboards.command=\"/Applications/Fabric Dashboards.app/Contents/MacOS/Fabric Dashboards\"",
            "-c",
            "mcp_servers.fabric-dashboards.args=[\"/Applications/Fabric Dashboards.app/Contents/Resources/app/mcp/server.js\"]",
            "-c",
            "mcp_servers.fabric-dashboards.env.ELECTRON_RUN_AS_NODE=\"1\"",
            "resume",
            "--last",
            "Read $FABRIC_DASHBOARDS_CONTEXT/context.md, then fix it",
        ]
        .map(str::to_owned)
        .to_vec();
        let (_, script) = launch_full(
            &f.root, &f.store, &proxy, &codex, "managed", &f.project, &host, None, &sent, true,
        )
        .unwrap();
        let (args, env) = run_as_the_agent(&script.unwrap(), &context);
        assert_eq!(
            args, sent,
            "Codex gets Switchboard's server from its config.toml"
        );
        assert!(env.contains("FABRIC_DASHBOARDS_TASK=fix\n"));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn a_launch_records_its_session_and_the_list_follows_its_process() {
        // SB-95: the list reads each owned home's record and asks the process, never a name.
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        assert!(sessions(&f.root, true).is_empty());
        launch_with(
            &f.root, &f.store, &proxy, &id, "isolated", &f.project, &TEST_HOST, None,
        )
        .unwrap();
        let home = f.root.join("homes").join(&id);
        let record = fs::read_to_string(home.join(SESSION_RECORD)).unwrap();
        assert!(
            !record.contains("synthetic"),
            "no credential in the record: {record}"
        );
        let listed = sessions(&f.root, false);
        assert_eq!(listed.len(), 1);
        let session = &listed[0];
        assert_eq!(
            session["state"], "starting",
            "Terminal has not run the script yet"
        );
        assert_eq!(session["account_id"], id.as_str());
        assert_eq!(session["provider"], "claude");
        // SB-110: the context is recorded and exported; no organization here.
        assert_eq!(session["context"], "personal");
        assert!(fs::read_to_string(home.join("launch.command"))
            .unwrap()
            .contains("export SWITCHBOARD_CONTEXT='personal'"));
        assert_eq!(session["mode"], "isolated");
        assert_eq!(session["home"], "homes");
        assert_eq!(session["in_place"], false);
        assert_eq!(
            session["working_directory"],
            f.project.canonicalize().unwrap().to_string_lossy().as_ref()
        );
        assert!(session["started_at"].is_null());

        // The script runs: it clears the reservation and records its own pid.
        fs::remove_file(home.join(".launch-pending")).unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "printf '%s' \"$$\" > .session-pid; exec sleep 30"])
            .current_dir(&home)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while fs::read_to_string(home.join(".session-pid")).map_or(true, |t| t.is_empty()) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        let listed = sessions(&f.root, false);
        assert_eq!(listed[0]["state"], "running");
        assert!(listed[0]["started_at"].as_i64().unwrap() > 0);

        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            sessions(&f.root, false).is_empty(),
            "an ended session leaves the default list"
        );
        assert_eq!(sessions(&f.root, true)[0]["state"], "ended");

        // A link inside the owned folders is never followed, and a home without a record is
        // not a session.
        let outside = tempfile::tempdir().unwrap();
        fs::write(
            outside.path().join(SESSION_RECORD),
            r#"{"account_id":"planted"}"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), f.root.join("homes").join("planted")).unwrap();
        fs::create_dir(f.root.join("homes").join("no-record")).unwrap();
        let all = sessions(&f.root, true);
        assert_eq!(all.len(), 1, "{all:?}");
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn a_continuation_in_place_returns_its_script_and_opens_no_terminal() {
        // SB-108: an embedded console (Fabric) runs the continuing session itself.
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        fn never(_: &Path) -> Result<(), String> {
            panic!("Terminal was opened for an in-place continuation")
        }
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: never,
            user_scope: no_user_scope,
        };
        let resumed = Continuation {
            workflow_id: "wf_0123456789abcdef".into(),
            handoff_id: "handoff:fedcba9876543210".into(),
            observatory: McpServer {
                command: "/opt/Observatory/bin/python".into(),
                args: vec!["/opt/Observatory/engine/mcp/server.py".into()],
            },
            previous_provider: None,
        };
        let (_, script) = continuation_here_with(
            &f.root, &f.store, &proxy, &id, "isolated", &f.project, &host, &resumed,
        )
        .unwrap();
        assert_eq!(
            script,
            f.root.join("homes").join(&id).join("launch.command")
        );
        let text = fs::read_to_string(&script).unwrap();
        assert!(text.contains("export OBSERVATORY_WORKFLOW_ID='wf_0123456789abcdef'\n"));
        assert!(text.contains(&quote(&crate::continuation::prompt(&resumed))));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn a_launched_home_carries_the_operators_setup_and_a_continuation_trusts_its_folder() {
        // SB-137/SB-130: the operator's instructions and settings come along; their sign-in does
        // not; a continuation does not stop at the first-run or trust screens.
        static USER: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let user = tempfile::tempdir().unwrap();
        fs::write(user.path().join("CLAUDE.md"), "operator rules").unwrap();
        fs::create_dir(user.path().join("skills")).unwrap();
        fs::write(
            user.path().join("settings.json"),
            json!({"hooks": {"Stop": []}, "env": {"OPERATOR_SECRET": "synthetic"}}).to_string(),
        )
        .unwrap();
        USER.set(user.path().to_path_buf()).unwrap();
        fn fixture_scope(_: Provider) -> Option<PathBuf> {
            USER.get().cloned()
        }
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        fn never(_: &Path) -> Result<(), String> {
            panic!("Terminal was opened for an in-place continuation")
        }
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: never,
            user_scope: fixture_scope,
        };
        let resumed = Continuation {
            workflow_id: "wf_0123456789abcdef".into(),
            handoff_id: "handoff:fedcba9876543210".into(),
            observatory: McpServer {
                command: "/opt/Observatory/bin/python".into(),
                args: vec!["/opt/Observatory/engine/mcp/server.py".into()],
            },
            previous_provider: None,
        };
        continuation_here_with(
            &f.root, &f.store, &proxy, &id, "isolated", &f.project, &host, &resumed,
        )
        .unwrap();
        let home = f.root.join("homes").join(&id);
        assert_eq!(
            fs::read_to_string(home.join("CLAUDE.md")).unwrap(),
            "operator rules"
        );
        assert!(home.join("skills").is_dir());
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert_eq!(settings["hooks"], json!({"Stop": []}));
        assert_eq!(
            settings["env"],
            json!({"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-access"}),
            "the operator's env is never carried"
        );
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        let trusted: Vec<&String> = state["projects"].as_object().unwrap().keys().collect();
        assert_eq!(trusted.len(), 1, "only the workflow's folder: {trusted:?}");
        assert!(Path::new(trusted[0]).ends_with(f.project.file_name().unwrap()));
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: terminal_opens,
            user_scope: no_user_scope,
        };
        let resumed = Continuation {
            workflow_id: "wf_0123456789abcdef".into(),
            handoff_id: "handoff:fedcba9876543210".into(),
            observatory: McpServer {
                command: "/opt/Observatory/bin/python".into(),
                args: vec!["/opt/Observatory/engine/mcp/server.py".into()],
            },
            previous_provider: None,
        };
        let before = f.files();
        launch_with(
            &f.root,
            &f.store,
            &proxy,
            &id,
            "isolated",
            &f.project,
            &host,
            Some(&resumed),
        )
        .unwrap();
        let home = f.root.join("homes").join(&id);
        let text = fs::read_to_string(home.join("launch.command")).unwrap();
        assert!(text.contains("export OBSERVATORY_WORKFLOW_ID='wf_0123456789abcdef'\n"));
        assert!(text.contains("export OBSERVATORY_HANDOFF_ID='handoff:fedcba9876543210'\n"));
        assert!(text.contains(&quote(&crate::continuation::prompt(&resumed))));
        // The first prompt follows Switchboard's MCP config: with a separate `--mcp-config` flag
        // Claude would read it as a second config file and never start.
        assert!(!text.contains("'--mcp-config' "), "{text}");
        assert!(text.contains(&format!(
            "{} {}",
            quote(&format!(
                "--mcp-config={}",
                home.join("switchboard-mcp.json").to_string_lossy()
            )),
            quote(&crate::continuation::prompt(&resumed))
        )));
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("switchboard-mcp.json")).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["observatory"],
            json!({"type": "stdio", "command": "/opt/Observatory/bin/python", "args": ["/opt/Observatory/engine/mcp/server.py"]})
        );
        assert_eq!(
            config["mcpServers"]["switchboard"]["env"]["SWITCHBOARD_SESSION"],
            format!("isolated:claude:{id}")
        );
        // Switchboard never holds a lease token or the old session's transcript, so nothing it
        // writes for the new session can carry one.
        for path in f.files().difference(&before) {
            let content = fs::read_to_string(path).unwrap_or_default();
            assert!(!content.contains("wl_"), "{path:?}");
            assert!(!content.contains(".jsonl"), "{path:?}");
            assert!(
                !content.contains("synthetic-refresh-must-not-export"),
                "{path:?}"
            );
        }

        // Codex declares the same server in its private config, beside Switchboard's.
        let codex = f.add(
            Provider::Codex,
            AuthKind::ApiKey,
            "default",
            "synthetic-codex-key",
        );
        launch_with(
            &f.root,
            &f.store,
            &proxy,
            &codex,
            "isolated",
            &f.project,
            &host,
            Some(&resumed),
        )
        .unwrap();
        let config: toml::Value = toml::from_str(
            &fs::read_to_string(f.root.join("homes").join(&codex).join("config.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            config["mcp_servers"]["observatory"]["command"].as_str(),
            Some("/opt/Observatory/bin/python")
        );
        assert!(config["mcp_servers"]["observatory"].get("env").is_none());
        assert!(config["mcp_servers"]["switchboard"].get("env").is_some());
    }
    #[tokio::test]
    async fn preflight_refuses_what_a_launch_would_refuse_without_writing() {
        let f = fixture();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let before = f.files();
        assert!(preflight(&f.root, &f.store, &id, "isolated", &f.project).is_ok());
        assert_eq!(
            preflight(&f.root, &f.store, &id, "sideways", &f.project).unwrap_err(),
            "Choose isolated or managed launch."
        );
        assert_eq!(
            preflight(&f.root, &f.store, "nobody", "isolated", &f.project).unwrap_err(),
            "Account not found."
        );
        switchboard_core::Vault::delete(&*f.vault, &id).unwrap();
        assert!(preflight(&f.root, &f.store, &id, "isolated", &f.project).is_err());
        assert_eq!(f.files(), before);
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn isolated_launch_script_exports_the_home_and_runs_in_the_project() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        f.launch(&proxy, &id, "isolated", &TEST_HOST).unwrap();
        let home = f.root.join("homes").join(&id);
        let script_path = home.join("launch.command");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&script_path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let text = fs::read_to_string(script_path).unwrap();
        assert!(text.contains(&format!(
            "export CLAUDE_CONFIG_DIR={}\n",
            quote(&home.to_string_lossy())
        )));
        assert!(text.contains(&format!(
            "export SWITCHBOARD_SESSION='isolated:claude:{id}'\n"
        )));
        let project = f.project.canonicalize().unwrap();
        assert!(text.contains(&format!(
            "cd {} || exit 1\nexec '/opt/Provider Tools/o'\\''cli'\n",
            quote(&project.to_string_lossy())
        )));
        assert!(
            !text.contains("synthetic-access"),
            "the token stays in settings.json"
        );
    }
    #[tokio::test]
    async fn expired_credential_is_refused_and_releases_the_home() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let mut expired =
            Credential::parse(Provider::Claude, AuthKind::OAuth, CLAUDE_OAUTH).unwrap();
        expired.expires_at = Some(1);
        switchboard_core::Vault::put(f.vault.as_ref(), &id, &expired).unwrap();
        assert_eq!(
            f.launch(&proxy, &id, "isolated", &TEST_HOST).unwrap_err(),
            "Credential expired; reauthenticate this account"
        );
        let home = f.root.join("homes").join(&id);
        assert!(!home.join("settings.json").exists());
        assert!(!home.join("launch.command").exists());
        assert!(ensure_idle(&home).is_ok());
    }
    #[tokio::test]
    async fn managed_launch_requires_the_selected_account_and_routes_through_the_proxy() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let selected = f.add(Provider::Claude, AuthKind::OAuth, "work", CLAUDE_OAUTH);
        let other = f.add(
            Provider::Claude,
            AuthKind::OAuth,
            "work",
            &CLAUDE_OAUTH.replace("synthetic-access", "synthetic-other"),
        );
        assert_eq!(
            f.launch(&proxy, &other, "managed", &TEST_HOST).unwrap_err(),
            "No account selected for this provider and pool"
        );
        f.store.select(Provider::Claude, "work", &selected).unwrap();
        assert_eq!(
            f.launch(&proxy, &other, "managed", &TEST_HOST).unwrap_err(),
            "Select this account before launching managed mode."
        );
        let home = f.root.join("runtimes").join("claude-work");
        assert!(
            ensure_idle(&home).is_ok(),
            "a refused launch releases the home"
        );

        let host = Host {
            agent_cli: synthetic_agent_cli,
            board: no_board,
            ..TEST_HOST
        };
        assert!(f.launch(&proxy, &selected, "managed", &host).unwrap());
        assert!(
            !home.join("settings.json").exists(),
            "no credential is copied"
        );
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("switchboard-mcp.json")).unwrap()).unwrap();
        let args = &config["mcpServers"]["switchboard"]["args"];
        assert!(!args.as_array().unwrap().iter().any(|a| a == "--read-only"));
        #[cfg(not(windows))]
        {
            let text = fs::read_to_string(home.join("launch.command")).unwrap();
            assert!(text.contains(&format!(
                "export ANTHROPIC_BASE_URL='http://{}/claude/work'\n",
                proxy.address()
            )));
            assert!(text.contains(&format!(
                "export ANTHROPIC_AUTH_TOKEN='{}'\n",
                proxy.token()
            )));
            assert!(text.contains("export ANTHROPIC_API_KEY=''\n"));
            assert!(text.contains(&quote(&format!(
                "--mcp-config={}",
                home.join("switchboard-mcp.json").to_string_lossy()
            ))));
        }
    }
    #[tokio::test]
    async fn managed_codex_launch_points_codex_at_the_local_proxy() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(
            Provider::Codex,
            AuthKind::ApiKey,
            "team",
            "synthetic-api-key",
        );
        f.store.select(Provider::Codex, "team", &id).unwrap();
        f.launch(&proxy, &id, "managed", &TEST_HOST).unwrap();
        let home = f.root.join("runtimes").join("codex-team");
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(home.join("config.toml")).unwrap()).unwrap();
        assert_eq!(config["model_provider"].as_str(), Some("switchboard"));
        let provider = &config["model_providers"]["switchboard"];
        assert_eq!(
            provider["base_url"].as_str().unwrap(),
            format!("http://{}/codex/team/v1", proxy.address())
        );
        assert_eq!(
            provider["env_key"].as_str(),
            Some("SWITCHBOARD_LOCAL_TOKEN")
        );
        assert!(!home.join("auth.json").exists(), "no credential is copied");
    }
    #[tokio::test]
    async fn isolated_codex_launch_writes_the_native_auth_snapshot() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let oauth = f.add(
            Provider::Codex,
            AuthKind::OAuth,
            "default",
            r#"{"tokens":{"access_token":"synthetic-access","refresh_token":"synthetic-refresh-must-not-export","id_token":"header.eyJzdWIiOiJzeW50aGV0aWMifQ.signature","account_id":"synthetic-account"}}"#,
        );
        let api = f.add(
            Provider::Codex,
            AuthKind::ApiKey,
            "default",
            "synthetic-api-key",
        );
        let host = Host {
            agent_cli: synthetic_agent_cli,
            board: no_board,
            ..TEST_HOST
        };
        assert!(f.launch(&proxy, &oauth, "isolated", &host).unwrap());
        f.launch(&proxy, &api, "isolated", &TEST_HOST).unwrap();

        let home = f.root.join("homes").join(&oauth);
        let auth: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["auth_mode"], "chatgpt");
        assert!(auth["OPENAI_API_KEY"].is_null());
        assert_eq!(auth["tokens"]["access_token"], "synthetic-access");
        assert_eq!(auth["tokens"]["refresh_token"], "");
        assert_eq!(
            auth["tokens"]["id_token"],
            "header.eyJzdWIiOiJzeW50aGV0aWMifQ.signature"
        );
        assert_eq!(auth["tokens"]["account_id"], "synthetic-account");
        assert!(auth["last_refresh"].is_string());
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(home.join("config.toml")).unwrap()).unwrap();
        assert_eq!(config["cli_auth_credentials_store"].as_str(), Some("file"));
        let server = &config["mcp_servers"]["switchboard"];
        assert_eq!(
            server["command"].as_str(),
            Some("/opt/Switchboard Tools/switchboard")
        );
        assert_eq!(
            server["args"].as_array().unwrap().last().unwrap().as_str(),
            Some("--read-only"),
            "an isolated session cannot change routes"
        );
        for entry in fs::read_dir(&home).unwrap() {
            let text = fs::read_to_string(entry.unwrap().path()).unwrap_or_default();
            assert!(!text.contains("synthetic-refresh-must-not-export"));
        }
        let auth: serde_json::Value = serde_json::from_slice(
            &fs::read(f.root.join("homes").join(&api).join("auth.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            auth,
            json!({"OPENAI_API_KEY": "synthetic-api-key", "auth_mode": "apikey"})
        );
    }
    #[tokio::test]
    async fn launch_refuses_bad_input_before_touching_a_home() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let never = Host {
            binary: native_never,
            ..TEST_HOST
        };
        let refuse = |wd: &Path, id: &str, mode: &str| {
            launch_with(&f.root, &f.store, &proxy, id, mode, wd, &never, None).unwrap_err()
        };
        let project = "Choose an existing project directory.";
        assert_eq!(refuse(Path::new("relative"), &id, "isolated"), project);
        assert_eq!(refuse(&f.project.join("missing"), &id, "isolated"), project);
        assert_eq!(refuse(&f.root, &id, "isolated"), project);
        assert_eq!(
            refuse(&f.project, &id, "elsewhere"),
            "Choose isolated or managed launch."
        );
        assert_eq!(
            refuse(&f.project, &Uuid::new_v4().to_string(), "isolated"),
            "Account not found."
        );
        f.store.update(&id, "Synthetic".into(), false).unwrap();
        assert_eq!(
            refuse(&f.project, &id, "isolated"),
            "Enable the account before launch."
        );
        assert!(!f.root.join("homes").exists());
    }
    #[tokio::test]
    async fn terminal_failure_releases_the_home_and_records_no_launch() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let host = Host {
            start_terminal: terminal_fails,
            ..TEST_HOST
        };
        assert_eq!(
            f.launch(&proxy, &id, "isolated", &host).unwrap_err(),
            "Terminal could not open."
        );
        assert!(ensure_idle(&f.root.join("homes").join(&id)).is_ok());
        let events = f.store.snapshot().unwrap().events;
        assert!(!events.iter().any(|e| e.action == "launch"));
        f.launch(&proxy, &id, "isolated", &TEST_HOST).unwrap();
    }
    #[cfg(not(windows))]
    #[test]
    fn script_unsets_every_conflicting_variable() {
        let text = script(
            Path::new("/tmp/home"),
            Path::new("/bin/true"),
            &[],
            &BTreeMap::new(),
            false,
            Path::new("/tmp/project"),
        );
        let unset = text.lines().find(|l| l.starts_with("unset ")).unwrap();
        let names: Vec<&str> = unset.split(' ').skip(1).collect();
        assert_eq!(names, CONFLICTS);
        assert!(text.find("unset ").unwrap() < text.find("export ").unwrap_or(usize::MAX));
    }
    /// Runs the real generated script with zsh against hostile paths and a polluted
    /// environment. Terminal launches only exist on macOS, which is where zsh is guaranteed.
    #[cfg(target_os = "macos")]
    #[test]
    fn generated_zsh_script_survives_hostile_paths_and_scrubs_the_environment() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let home = base.join("it's \"a\" $(touch pwned) `touch pwned` dir");
        let project = base.join("my project; touch pwned");
        private_dir(&home).unwrap();
        fs::create_dir(&project).unwrap();
        let report = base.join("report");
        let provider = base.join("provider's cli");
        private_write(
            &provider,
            format!(
                "#!/bin/sh\n{{ pwd; printf '%s\\n' \"$@\"; env; }} > {}\n",
                quote(&report.to_string_lossy())
            )
            .as_bytes(),
            true,
        )
        .unwrap();
        let mut hold = Reservation::new(&home).unwrap();
        hold.committed = true;
        let env = BTreeMap::from([
            (
                "CODEX_HOME".to_string(),
                home.to_string_lossy().into_owned(),
            ),
            (
                "SWITCHBOARD_SESSION".to_string(),
                "isolated:codex:x'y".into(),
            ),
        ]);
        let path = home.join("launch.command");
        let text = script(&home, &provider, &["a b", "c'd"], &env, false, &project);
        private_write(&path, text.as_bytes(), true).unwrap();
        let mut command = Command::new(SCRIPT_SHELL);
        command.arg(&path).env_clear().env("PATH", "/usr/bin:/bin");
        for variable in CONFLICTS {
            command.env(variable, "synthetic-conflict");
        }
        assert!(command.status().unwrap().success());

        let report = fs::read_to_string(report).unwrap();
        let mut lines = report.lines();
        assert_eq!(lines.next(), Some(project.to_string_lossy().as_ref()));
        assert_eq!(lines.next(), Some("a b"));
        assert_eq!(lines.next(), Some("c'd"));
        let env: BTreeMap<&str, &str> = lines.filter_map(|l| l.split_once('=')).collect();
        assert_eq!(
            env.get("CODEX_HOME").copied(),
            Some(home.to_string_lossy().as_ref())
        );
        assert_eq!(
            env.get("SWITCHBOARD_SESSION").copied(),
            Some("isolated:codex:x'y")
        );
        for variable in CONFLICTS {
            if !matches!(*variable, "CODEX_HOME" | "SWITCHBOARD_SESSION") {
                assert!(!env.contains_key(variable), "{variable} leaked");
            }
        }
        assert!(!home.join(".launch-pending").exists());
        assert!(read_regular(&home.join(".session-pid")).is_ok());
        let mut found = Vec::new();
        for dir in [&base, &home, &project] {
            found.extend(fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()));
        }
        assert!(!found.iter().any(|n| n == "pwned"), "a path was evaluated");
    }
    #[test]
    fn private_write_and_symlink_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("auth.json");
        private_write(&file, b"synthetic", false).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let link = dir.path().join("link");
            symlink(&file, &link).unwrap();
            assert!(private_write(&link, b"changed", false).is_err());
            assert_eq!(fs::read(&file).unwrap(), b"synthetic");
        }
    }
}
