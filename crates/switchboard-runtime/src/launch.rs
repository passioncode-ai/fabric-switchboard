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
    // Never a translocated copy beside a desktop app opened from its download folder: an MCP
    // entry pointing into it breaks once the app quits.
    let mut candidates = vec![current.parent()?.join(CLI_NAME)];
    candidates.retain(|p| !crate::agents::translocated(p));
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(CLI_NAME)));
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        candidates.push(PathBuf::from(home).join(".local/bin").join(CLI_NAME));
    }
    candidates
        .into_iter()
        .filter(|p| p.is_absolute() && p.is_file())
        .find_map(|p| p.canonicalize().ok())
}
/// What a launched session needs to reach Switchboard's tools: Claude reads an extra MCP
/// file named on its command line; Codex reads `[mcp_servers]` from its private config.
/// Isolated sessions get the read-only set, since a route change cannot reach them. A session
/// that continues an Observatory workflow (SB-52) also gets the Observatory server there: the
/// user scope is not visible in an isolated home.
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
    observatory: Option<&McpServer>,
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
    if let Some(server) = observatory {
        servers.push((
            "observatory",
            server.command.clone(),
            server.args.clone(),
            None,
        ));
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
    "Switchboard could not read the OpenRouter key. Check it under Agents, then launch again.";
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
    let mut result = format!("#!/bin/zsh\nset +x\nunset {}\n", CONFLICTS.join(" "));
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
            "{name}=\"$({call})\" && [ -n \"${name}\" ] || {{ print -u2 {}; exit 1; }}\nexport {name}\n",
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
struct Reservation {
    path: PathBuf,
    committed: bool,
}
impl Reservation {
    fn new(home: &Path) -> Result<Self, String> {
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
    start_terminal: fn(&Path) -> Result<(), String>,
}
const NATIVE: Host = Host {
    binary,
    agent_cli,
    start_terminal,
};
pub(crate) fn open_terminal(home: &Path, content: &str) -> Result<(), String> {
    start_terminal(&write_launch_script(home, content)?)
}
/// After the proxy had to move from `old` to `new` (its port was taken), points the files
/// Switchboard generated in managed homes — launch scripts and Codex's provider config — at
/// the new port. Only the exact loopback address is replaced; nothing else is touched.
/// Returns how many files changed.
pub(crate) fn repoint_managed(root: &Path, old: u16, new: u16) -> usize {
    let from = format!("http://127.0.0.1:{old}/");
    let to = format!("http://127.0.0.1:{new}/");
    let Ok(homes) = fs::read_dir(root.join("runtimes")) else {
        return 0;
    };
    let mut changed = 0;
    for home in homes.flatten() {
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
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Err("Terminal launch is currently supported on macOS and Windows only.".into())
    }
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
    ensure_idle(&login.home)
        .map_err(|_| "Finish or close sign-in in Terminal before cancelling.")?;
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
                return Err("Close the existing session before changing its home.".into());
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
                return Err("Close the existing session before changing its home.".into());
            }
        }
    }
    Ok(())
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
    "This account belongs to a project. Launch it from one of the project's folders.";
pub(crate) const PROJECT_FOLDER_FOREIGN: &str =
    "This folder belongs to a project. Launch one of the project's accounts.";
/// Projects reserve their accounts (0.6): a project's account starts only inside the project's
/// folders, and inside them a provider the project has accounts for uses only those.
pub(crate) fn project_allows(
    snapshot: &switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    folder: &Path,
) -> Result<(), String> {
    if let Some(owner) = snapshot.project_of_pool(&account.pool) {
        if !owner
            .folders
            .iter()
            .any(|f| folder.starts_with(Path::new(f)))
        {
            return Err(PROJECT_ACCOUNT_OUTSIDE.into());
        }
    }
    if let Some(here) = snapshot.project_for(folder) {
        let has_own = snapshot
            .accounts
            .iter()
            .any(|a| a.pool == here.pool && a.provider == account.provider && a.enabled);
        if has_own && account.pool != here.pool {
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
    let profile = crate::agent_catalog::launchable(agent)?;
    let working_directory = agent_folder(root, working_directory)?;
    let (account, _) = store
        .route(Provider::Claude, pool)
        .map_err(|_| "Select an API-key account in this pool first.".to_string())?;
    if account.kind != AuthKind::ApiKey {
        return Err(switchboard_proxy::AGENT_SUBSCRIPTION_REFUSED.into());
    }
    project_allows(&store.snapshot()?, &account, &working_directory)?;
    let program = find_program(&profile.binary).ok_or_else(|| {
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
    let mut env = BTreeMap::from([(
        "SWITCHBOARD_SESSION".to_string(),
        format!("managed:claude:{pool}"),
    )]);
    if let (Some(base_env), Some(key_env)) = (&profile.base_env, &profile.key_env) {
        env.insert(base_env.clone(), base);
        env.insert(key_env.clone(), proxy.agent_token().to_owned());
    }
    env.extend(profile.extra_env.clone());
    let args: Vec<&str> = profile.args.iter().map(String::as_str).collect();
    let content = script(&home, &program, &args, &env, false, &working_directory);
    open_terminal(&home, &content)?;
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
    "Choose a model for this launch, or set the default model of the OpenRouter key under Agents.";
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
/// Writes the session of an OpenRouter launch and returns its script and the answer. Separate
/// from starting Terminal so tests read the script a real launch would run.
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
        .agent_key("openrouter")?
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
    let homes = root.join("runtimes");
    private_dir(&homes)?;
    let home = homes.join(format!("agent-{}-openrouter", recipe.id));
    private_dir(&home)?;
    let mut env = BTreeMap::new();
    if let (Some(base_env), Some(base_url)) = (&recipe.base_env, &recipe.base_url) {
        env.insert(base_env.clone(), base_url.clone());
    }
    if let (Some(model_env), Some(model)) = (&recipe.model_env, &model) {
        env.insert(model_env.clone(), model.clone());
    }
    env.extend(recipe.extra_env.clone());
    if let Some(isolate) = &recipe.isolate_env {
        let own = home.join("agent-home");
        private_dir(&own)?;
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
    let tools = agent_tools(
        cli.as_deref(),
        root,
        &home,
        account.provider,
        &session,
        mode != "managed",
        continuation.map(|c| &c.observatory),
    )?;
    let mut env = BTreeMap::from([("SWITCHBOARD_SESSION".to_string(), session)]);
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
                private_write(
                    &home.join("settings.json"),
                    serde_json::to_string(&json!({"env":{name:credential.access_token}}))
                        .unwrap()
                        .as_bytes(),
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
    let mut args: Vec<&str> = match &config {
        Some(path) => vec!["--mcp-config", path.as_str()],
        None => Vec::new(),
    };
    // Both CLIs start an interactive session with a positional first message.
    if let Some(first) = &first {
        args.push(first.as_str());
    }
    args.extend(extra.iter().map(String::as_str));
    let path = write_launch_script(
        &home,
        &script(&home, &program, &args, &env, false, &working_directory),
    )?;
    // In place the caller runs the script now; the reservation lapses if it never does.
    if !in_place {
        (host.start_terminal)(&path)?;
    }
    reservation.committed = true;
    journal_launch(store, id, mode);
    Ok((cli.is_some(), in_place.then_some(path)))
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
            None,
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
            None,
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
            None,
        )
        .unwrap();
        assert!(none.claude_config.is_none() && none.codex_section.is_empty());
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
        assert!(cancel_login(&login).is_err());
        age(&home.join(".launch-pending"), 11 * 60);
        cancel_login(&login).unwrap();
        assert!(!home.exists());
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
    fn synthetic_agent_cli() -> Option<PathBuf> {
        Some(PathBuf::from("/opt/Switchboard Tools/switchboard"))
    }
    fn terminal_opens(_: &Path) -> Result<(), String> {
        Ok(())
    }
    fn terminal_fails(_: &Path) -> Result<(), String> {
        Err("Terminal could not open.".into())
    }
    const TEST_HOST: Host = Host {
        binary: synthetic_program,
        agent_cli: no_agent_cli,
        start_terminal: terminal_opens,
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
            assert!(text.contains("'agents' 'key' '--service' 'openrouter')\""));
            assert!(text.contains("export OPENROUTER_API_KEY\n"));
            assert!(text.contains(
                "exec '/opt/Agent Tools/agent' '--provider' 'openrouter' '-m' 'moonshotai/kimi-k2'"
            ));
            assert!(text.contains("unset ANTHROPIC_API_KEY"));
        }
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
            Command::new("/bin/zsh").arg(&path).output().unwrap()
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
            start_terminal: terminal_must_not_open,
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
    #[cfg(not(windows))]
    #[tokio::test]
    async fn a_continuation_launch_carries_the_ids_the_observatory_server_and_the_first_prompt() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            start_terminal: terminal_opens,
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
            assert!(text.contains("'--mcp-config'"));
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
        let mut command = Command::new("/bin/zsh");
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
