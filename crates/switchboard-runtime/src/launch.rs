use serde_json::json;
#[cfg(any(target_os = "macos", test))]
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
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
    let mut candidates = vec![current.parent()?.join(CLI_NAME)];
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
/// Isolated sessions get the read-only set, since a route change cannot reach them.
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
) -> Result<AgentTools, String> {
    let Some(cli) = cli else {
        return Ok(AgentTools {
            claude_config: None,
            codex_section: String::new(),
        });
    };
    let mut args = vec![
        "--data-dir".to_string(),
        root.to_string_lossy().into_owned(),
        "mcp".into(),
    ];
    if read_only {
        args.push("--read-only".into());
    }
    let command = cli.to_string_lossy().into_owned();
    match provider {
        Provider::Claude => {
            let path = home.join("switchboard-mcp.json");
            let config = json!({"mcpServers": {"switchboard": {"type": "stdio", "command": command, "args": args, "env": {"SWITCHBOARD_SESSION": session}}}});
            private_write(&path, config.to_string().as_bytes(), false)?;
            Ok(AgentTools {
                claude_config: Some(path),
                codex_section: String::new(),
            })
        }
        Provider::Codex => {
            let quote = |v: &str| toml::Value::String(v.into()).to_string();
            let args = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(", ");
            Ok(AgentTools {
                claude_config: None,
                codex_section: format!(
                    "\n[mcp_servers.switchboard]\ncommand = {}\nargs = [{args}]\nenv = {{ SWITCHBOARD_SESSION = {} }}\n",
                    quote(&command),
                    quote(session)
                ),
            })
        }
    }
}
fn private_dir(path: &Path) -> Result<(), String> {
    switchboard_core::private_fs::private_dir(path)
}
fn private_write(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
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
#[cfg(any(unix, test))]
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn binary(provider: Provider) -> Result<PathBuf, String> {
    let name = provider.as_str();
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|s| std::env::split_paths(&s).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
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
                return path
                    .canonicalize()
                    .map_err(|_| "Provider CLI path unavailable.".into());
            }
        }
        #[cfg(not(windows))]
        {
            let path = dir.join(name);
            if path.is_file() {
                return path
                    .canonicalize()
                    .map_err(|_| "Provider CLI path unavailable.".into());
            }
        }
    }
    Err("Provider CLI not found. Install the official CLI and retry.".into())
}
#[cfg(not(windows))]
fn script(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    login: bool,
    working_directory: &Path,
) -> String {
    let mut result = format!("#!/bin/zsh\nset +x\nunset {}\n", CONFLICTS.join(" "));
    for (key, value) in env {
        result.push_str(&format!("export {key}={}\n", quote(value)));
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
fn script(
    home: &Path,
    program: &Path,
    args: &[&str],
    env: &BTreeMap<String, String>,
    login: bool,
    working_directory: &Path,
) -> String {
    windows_script(home, program, args, env, login, working_directory)
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
fn open_terminal(home: &Path, content: &str) -> Result<(), String> {
    #[cfg(windows)]
    let path = home.join("launch.ps1");
    #[cfg(not(windows))]
    let path = home.join("launch.command");
    #[cfg(windows)]
    let content = format!("\u{feff}{content}"); // Windows PowerShell 5.1 needs a UTF-8 BOM for Unicode paths.
    private_write(&path, content.as_bytes(), true)?;
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
        Err("Terminal launch is currently supported on macOS and Windows only.".into())
    }
}
fn validate_fields(label: &str, pool: &str) -> Result<(), String> {
    if label.trim().is_empty() || label.len() > 80 || label.chars().any(char::is_control) {
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
        home,
    })
}
#[cfg(any(target_os = "macos", test))]
fn keychain_service(home: &Path) -> String {
    let raw: String = home.to_string_lossy().nfc().collect();
    format!(
        "Claude Code-credentials-{}",
        &format!("{:x}", Sha256::digest(raw.as_bytes()))[..8]
    )
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
                let user = std::env::var("USER").map_err(|_| "Local login user unavailable.")?;
                let bytes = security_framework::passwords::get_generic_password(
                    &keychain_service(&login.home),
                    &user,
                )
                .map_err(|_| {
                    "Sign-in credential unavailable. Check Keychain access and finish login."
                })?;
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
        let user = std::env::var("USER").map_err(|_| "Local login user unavailable.")?;
        if let Err(error) = security_framework::passwords::delete_generic_password(
            &keychain_service(&login.home),
            &user,
        ) {
            if error.code() != -25300 {
                return Err("Sign-in cleanup needs Keychain access.".into());
            }
        }
    }
    fs::remove_dir_all(&login.home).map_err(|_| "Sign-in cleanup failed.".into())
}
pub fn clean_login(login: &Login) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if login.provider == Provider::Claude {
        let user = std::env::var("USER").map_err(|_| "Local login user unavailable.")?;
        if let Err(error) = security_framework::passwords::delete_generic_password(
            &keychain_service(&login.home),
            &user,
        ) {
            if error.code() != -25300 {
                return Err("Account saved; isolated login cleanup needs attention.".into());
            }
        }
    }
    match fs::remove_dir_all(&login.home) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Account saved; isolated login cleanup needs attention.".into()),
    }
}
fn ensure_idle(home: &Path) -> Result<(), String> {
    if home.join(".launch-pending").exists() {
        return Err("Close the existing session before changing its home.".into());
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
            return Err("Close the existing session before changing its home.".into());
        }
    }
    Ok(())
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
pub fn launch(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
) -> Result<bool, String> {
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
    let program = binary(account.provider)?;
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
    let cli = agent_cli();
    let tools = agent_tools(
        cli.as_deref(),
        root,
        &home,
        account.provider,
        &session,
        mode != "managed",
    )?;
    let mut env = BTreeMap::from([("SWITCHBOARD_SESSION".to_string(), session)]);
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
        if credential
            .expires_at
            .is_some_and(|t| t <= switchboard_proxy::now())
        {
            return Err("Provider rejected the credential. Sign in again.".into());
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
    let args: Vec<&str> = match &config {
        Some(path) => vec!["--mcp-config", path.as_str()],
        None => Vec::new(),
    };
    open_terminal(
        &home,
        &script(&home, &program, &args, &env, false, &working_directory),
    )?;
    reservation.committed = true;
    store.record("launch", Some(id), mode)?;
    Ok(cli.is_some())
}
#[cfg(test)]
mod tests {
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
        private_write(&provider, b"if ($env:ANTHROPIC_API_KEY) { exit 9 }\nif ($env:CODEX_HOME -ne (Get-Location).Path) { exit 8 }\nexit 0\n", false).unwrap();
        let env = BTreeMap::from([("CODEX_HOME".into(), home.to_string_lossy().into_owned())]);
        let content = windows_script(&home, &provider, &[], &env, true, &home);
        let script_path = home.join("test.ps1");
        private_write(&script_path, format!("\u{feff}{content}").as_bytes(), false).unwrap();
        let mut reservation = Reservation::new(&home).unwrap();
        reservation.committed = true;
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script_path)
            .env("ANTHROPIC_API_KEY", "synthetic-conflicting-key")
            .status()
            .unwrap();
        assert!(status.success());
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
    fn rejects_invalid_fields() {
        assert!(validate_fields("x", "../../escape").is_err());
        assert!(validate_fields("\n", "default").is_err());
        assert!(validate_fields("Work", "team-a").is_ok());
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
