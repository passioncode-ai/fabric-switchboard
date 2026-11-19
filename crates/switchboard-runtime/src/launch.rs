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
/// SB-259: names the operator's own Claude folder in a session Switchboard launched.
pub(crate) const SOURCE_DIR: &str = "SWITCHBOARD_CLAUDE_SOURCE_DIR";
/// SB-452: a managed Claude home's own route (0600): the proxy's route of the launch's group,
/// its token and the launch's group, passed as `--settings`, which Claude Code ranks above a
/// folder's `.claude/settings.local.json` (and a worktree's, which it reads from the main
/// checkout). Rewritten at each launch and repointed with `launch.command` when the proxy moves.
pub(crate) const ROUTE_FILE: &str = "switchboard-route.json";
/// SB-452: the argument that gives a Claude session its home's route file, last on its command
/// line (a later `--settings` would replace it whole).
pub(crate) fn route_arg(home: &Path) -> String {
    format!("--settings={}", home.join(ROUTE_FILE).to_string_lossy())
}
/// SB-452: writes `home`'s route file for a managed Claude session of `pool` through `base`.
fn write_route(home: &Path, base: &str, token: &str, pool: &str) -> Result<(), String> {
    let route = json!({"env": {
        "ANTHROPIC_BASE_URL": base,
        "ANTHROPIC_AUTH_TOKEN": token,
        "ANTHROPIC_API_KEY": "",
        "ANTHROPIC_CUSTOM_HEADERS": format!("X-Switchboard-Launch-Group: {pool}"),
    }});
    private_write(
        &home.join(ROUTE_FILE),
        serde_json::to_string(&route).unwrap().as_bytes(),
        false,
    )
}
pub(crate) const CONFLICTS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    // SB-533: a managed session's route header (`X-Switchboard-Launch-Group`, a folder's
    // capability) never reaches a session it starts; a session of its own gets its own route.
    "ANTHROPIC_CUSTOM_HEADERS",
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
    "SWITCHBOARD_CLAUDE_SOURCE_DIR",
    "SWITCHBOARD_CONTEXT",
    // SB-212: every launched session gets its own; one inherited from a caller never leaks.
    "SWITCHBOARD_LAUNCH_ID",
    // SB-247: a session started inside a dispatched run is not that run.
    "FABRIC_DISPATCH_WORKFLOW",
    "FABRIC_DISPATCH_HANDOFF",
    "FABRIC_DISPATCH_REQUEST",
    "FABRIC_DISPATCH_ID",
];
pub use fabric_sessions_core::terminal::LINUX_TERMINAL_MISSING;
/// SB-397 D-2: the Terminal and script helpers live in fabric-sessions-core.
pub(crate) use fabric_sessions_core::terminal::{find_program, SCRIPT_SHELL};

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
/// `.claude.json` `mcpServers.fabric` (in the person's own `CLAUDE_CONFIG_DIR` when they set one,
/// SB-260, else `~/.claude.json`), Codex's `config.toml` `[mcp_servers.fabric]`. Only a
/// stdio entry with an absolute command is carried, and only its command and arguments: an
/// `env` may hold a secret, and a secret is never copied into a launched home. Absent,
/// unreadable or half-written means no board, never a failed launch.
fn user_board_server(provider: Provider, home: &Path, own_dir: Option<&Path>) -> Option<McpServer> {
    let strings = |args: Option<Vec<Option<String>>>| -> Option<Vec<String>> {
        args.unwrap_or_default().into_iter().collect()
    };
    let (command, args) = match provider {
        Provider::Claude => {
            use std::io::Read;
            let mut bytes = Vec::new();
            let file =
                own_dir.map_or_else(|| home.join(".claude.json"), |d| d.join(".claude.json"));
            fs::File::open(file)
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
            let dir = own_dir
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
    let own_dir = match provider {
        Provider::Claude => crate::person::claude_config(),
        Provider::Codex => crate::person::codex_home(),
    };
    user_board_server(provider, Path::new(&home), own_dir.as_deref())
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
    fabric_sessions_core::cli::quoted(value)
}
fn binary(provider: Provider) -> Result<PathBuf, String> {
    find_program(provider.as_str())
        .ok_or_else(|| "Provider CLI not found. Install the official CLI and retry.".into())
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
    // SB-455: Terminal, a session host, `--in-place` and `launch --headless` run this script with
    // the PATH of whatever started it (launchd's short one for the app and `serve`, a terminal's
    // own for `--in-place`). That PATH stays first (a person's nvm, pyenv or mise shims win); the
    // login PATH comes after it, so a hook finds `node` from Homebrew.
    result.push_str(&path_export(program));
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
    older_than(written, now, RESERVATION_TTL)
}
/// SB-440: a marker handed to a caller that runs the script itself (`launch --in-place`,
/// `--headless`, `--hosted`, `continue --in-place`) is taken within moments of the answer. One no
/// script took after this long belongs to a caller that gave up (a control request that timed out
/// after the owner had prepared the session): longer than the caller's own wait for the answer
/// (`control::limit_secs`, 120 s for `Launch` and `Continue`), with a margin, so an answer that
/// arrives at the end of that wait is still run.
pub(crate) const UNCLAIMED_TTL: Duration = Duration::from_secs(5 * 60);
/// SB-440: who runs a home's script once its preparation is done, written into its marker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handed {
    /// Terminal (or PowerShell) was asked to open it, or a continuation prepared in place waits
    /// in the hand-over queue for a console (SB-108); either may take a while, so the marker holds
    /// for `RESERVATION_TTL`.
    Terminal,
    /// The caller got the script in the answer and runs it at once (`UNCLAIMED_TTL`).
    Caller,
}
impl Handed {
    /// SB-449: who runs an in-place continuation's script: the hand-over queue, where it may wait
    /// for a console (`Terminal`'s hold), or the caller that asked for it (`Caller`).
    pub fn for_continue(queued: bool) -> Self {
        if queued {
            Handed::Terminal
        } else {
            Handed::Caller
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Handed::Terminal => "terminal",
            Handed::Caller => "caller",
        }
    }
}
/// SB-440: whether a home's `.launch-pending` still holds it.
#[derive(Debug, PartialEq, Eq)]
enum Pending {
    Absent,
    Held,
    /// Nobody will run the script any more; the code says why (`expired`, `launcher_gone`,
    /// `unclaimed`).
    Stale(&'static str),
}
/// Whether `written` is more than `ttl` away from `now`, in either direction: a marker dated
/// ahead of the clock (a clock set back) expires by the same distance.
fn older_than(written: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    match now.duration_since(written) {
        Ok(age) => age > ttl,
        Err(ahead) => ahead.duration() > ttl,
    }
}
/// SB-440: reads a marker. A marker holds while its preparation runs (`{"pid": N, "token": T}`,
/// the process preparing it and its start token, released at once when that process is gone or
/// its pid now names another process), then while its script has not run
/// (`{"pid": null, "by": "terminal" | "caller"}`, for `RESERVATION_TTL` or `UNCLAIMED_TTL`). An
/// empty or unreadable marker (one an earlier version wrote) holds for `RESERVATION_TTL`; one
/// whose time cannot be read, and anything that is not a regular file, always holds, as before.
/// Reads only.
fn pending(home: &Path, now: SystemTime) -> Result<Pending, String> {
    pending_at(&home.join(".launch-pending"), now)
}
fn pending_at(path: &Path, now: SystemTime) -> Result<Pending, String> {
    let path = path.to_path_buf();
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Pending::Absent),
        Err(_) => return Err("Session state unavailable. Inspect the managed home.".into()),
    };
    if !meta.file_type().is_file() {
        return Ok(Pending::Held);
    }
    // A time that cannot be read cannot expire: the marker holds, as before.
    let Ok(written) = meta.modified() else {
        return Ok(Pending::Held);
    };
    if reservation_expired(written, now) {
        return Ok(Pending::Stale("expired"));
    }
    let marker: Value = switchboard_core::private_fs::read_private(&path, 256)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    if let Some(pid) = marker["pid"].as_u64().and_then(|p| u32::try_from(p).ok()) {
        let age = now.duration_since(written).unwrap_or_default();
        return Ok(if launcher_gone(pid, marker["token"].as_u64(), age) {
            Pending::Stale("launcher_gone")
        } else {
            Pending::Held
        });
    }
    if marker["by"] == Handed::Caller.as_str() && older_than(written, now, UNCLAIMED_TTL) {
        return Ok(Pending::Stale("unclaimed"));
    }
    Ok(Pending::Held)
}
/// SB-440: the process preparing a home is gone, or its pid now names another process: its start
/// token (`copies::process_token`, the same on every platform it can be read) differs from the one
/// the marker recorded. Without a recorded token (none could be read when it was written), Unix
/// falls back to the process's age against the marker's; a start that cannot be read holds.
fn launcher_gone(pid: u32, token: Option<u64>, marker_age: Duration) -> bool {
    if pid == 0 || !crate::copies::alive(pid) {
        return true;
    }
    if let Some(recorded) = token {
        return crate::copies::process_token(pid).is_some_and(|now| now != recorded);
    }
    #[cfg(unix)]
    {
        process_age(pid).is_some_and(|age| pid_reused(marker_age, age))
    }
    #[cfg(not(unix))]
    {
        let _ = marker_age;
        false
    }
}
/// SB-440 (review round 1): clears a stale marker without ever removing a fresh one another
/// launch wrote after it was judged: the marker is first moved aside under a name of its own (a
/// rename takes exactly the file that is there now), judged again there, and removed only when it
/// is still stale; a marker that holds goes back (a hard link, which never replaces a marker
/// written meanwhile) and the home stays busy.
fn clear_stale(home: &Path, now: SystemTime) -> Result<(), String> {
    clear_stale_with(home, now, |from, to| fs::hard_link(from, to))
}
/// `clear_stale` with the link that puts a marker back (tests make it fail).
fn clear_stale_with(
    home: &Path,
    now: SystemTime,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = home.join(".launch-pending");
    let aside = home.join(format!(
        ".launch-pending.stale-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    match fs::rename(&path, &aside) {
        Ok(()) => {}
        // Gone meanwhile: its script started or another launch cleared it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(HOME_BUSY.into()),
    }
    match pending_at(&aside, now) {
        Ok(Pending::Stale(reason)) => {
            let _ = fs::remove_file(&aside);
            crate::oplog::event(
                "launch_reservation",
                &[
                    ("outcome", crate::oplog::Field::Code("cleared")),
                    ("reason", crate::oplog::Field::Code(reason)),
                ],
            );
            Ok(())
        }
        _ => {
            match link(&aside, &path) {
                // Back in place; the copy aside goes.
                Ok(()) => {
                    let _ = fs::remove_file(&aside);
                }
                // Another marker took the name meanwhile and holds the home itself.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let _ = fs::remove_file(&aside);
                }
                // SB-448: no link here (a file system without hard links, a refused link): the
                // marker is moved back instead, never deleted while it holds.
                Err(_) => {
                    let _ = fs::rename(&aside, &path);
                }
            }
            Err(HOME_BUSY.into())
        }
    }
}
const HOME_BUSY: &str = "Close the existing session before changing its home.";
pub(crate) struct Reservation {
    path: PathBuf,
    pub(crate) committed: bool,
}
impl Reservation {
    pub(crate) fn new(home: &Path) -> Result<Self, String> {
        // SB-448: only a new reservation clears a stale marker; every other check only reads.
        if let Pending::Stale(_) = pending(home, SystemTime::now())? {
            clear_stale(home, SystemTime::now())?;
        }
        ensure_idle(home)?;
        let path = home.join(".launch-pending");
        let mut file = switchboard_core::private_fs::create_new(&path)
            .map_err(|_| "Close the existing session before changing its home.")?;
        // SB-440: the process preparing the home and its start token, so a crash while it
        // prepares frees the home.
        let reservation = Self {
            path,
            committed: false,
        };
        {
            use std::io::Write;
            let me = std::process::id();
            file.write_all(
                json!({"pid": me, "token": crate::copies::process_token(me)})
                    .to_string()
                    .as_bytes(),
            )
            .map_err(|_| "Session state unavailable. Inspect the managed home.")?;
        }
        Ok(reservation)
    }
    /// SB-440: the preparation is done and `by` runs the script, which removes the marker as it
    /// starts. The marker is rewritten in place, never created again: a script that already
    /// started (or a dispatched run that already took the home) removed it, and that stays so.
    pub(crate) fn commit(&mut self, by: Handed) {
        self.committed = true;
        if !fs::symlink_metadata(&self.path).is_ok_and(|m| m.file_type().is_file()) {
            return;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).truncate(true);
        // Never through a link put in the marker's place.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        if let Ok(mut file) = options.open(&self.path) {
            use std::io::Write;
            let _ = file.write_all(
                json!({"pid": null, "by": by.as_str()})
                    .to_string()
                    .as_bytes(),
            );
        }
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
    /// SB-358: carries the person's MCP sign-ins from their other Claude homes into a Claude home
    /// about to be launched (`root`, `home`).
    mcp_sign_ins: fn(&Path, &Path),
}
const NATIVE: Host = Host {
    binary,
    agent_cli,
    board: board_server,
    start_terminal,
    user_scope,
    mcp_sign_ins: crate::mcp_homes::carry_into,
};
/// Tests carry no MCP sign-in: nothing reads or writes a Keychain item.
#[cfg(test)]
fn no_mcp_sign_ins(_: &Path, _: &Path) {}
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
/// Each `(old, new)` secret is replaced in the same files (SB-269: the proxy's token and what is
/// derived from it, renewed when the proxy moved).
pub(crate) fn repoint_managed(
    root: &Path,
    old: u16,
    new: u16,
    secrets: &[(String, String)],
) -> usize {
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
            (ROUTE_FILE, false),
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
            let mut text = text.replace(&from, &to);
            for (old, new) in secrets {
                text = text.replace(old.as_str(), new.as_str());
            }
            if private_write(&path, text.as_bytes(), executable).is_ok() {
                changed += 1;
            }
        }
    }
    changed
}
/// SB-384: a script at a path of its own (the duty officer's resume scripts), executable, 0700.
pub(crate) fn write_script_at(path: &Path, content: &str) -> Result<(), String> {
    fabric_sessions_core::terminal::write_script(path, content)
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
/// SB-342 H-1b: `attach.command` beside the launch script `script`: it starts the session in a
/// session host and attaches this window to it (`switchboard session-open`). None when the
/// script names no launch, its data folder cannot be found, or no switchboard CLI is installed.
/// The launch a session script runs and the data folder it lies in: the script's
/// `SWITCHBOARD_LAUNCH_ID` line and the nearest of its four parents holding `accounts.json`.
pub fn script_launch(script: &Path) -> Option<(PathBuf, String)> {
    let text = std::fs::read_to_string(script).ok()?;
    let id = text
        .lines()
        .find_map(|l| {
            l.strip_prefix("export SWITCHBOARD_LAUNCH_ID='")?
                .strip_suffix('\'')
        })
        .filter(|id| crate::launches::valid_id(id))?;
    let root = script
        .ancestors()
        .skip(1)
        .take(4)
        .find(|dir| dir.join("accounts.json").is_file())?;
    Some((root.to_path_buf(), id.to_owned()))
}
#[cfg(unix)]
fn hosted_wrapper(script: &Path) -> Option<PathBuf> {
    let (root, id) = script_launch(script)?;
    let (root, id) = (root.as_path(), id.as_str());
    let cli = agent_cli()?;
    let wrapper = script.with_file_name("attach.command");
    private_write(
        &wrapper,
        attach_wrapper(&cli, root, id, script).as_bytes(),
        true,
    )
    .ok()?;
    Some(wrapper)
}
#[cfg(unix)]
fn attach_wrapper(cli: &Path, root: &Path, launch_id: &str, script: &Path) -> String {
    format!(
        "#!/bin/sh\nexec {} --data-dir {} session-open --launch-id {} --program {}\n",
        quote(&cli.to_string_lossy()),
        quote(&root.to_string_lossy()),
        quote(launch_id),
        quote(&script.to_string_lossy()),
    )
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
    headless: bool,
) -> Result<Login, String> {
    validate_fields(&label, &pool)?;
    // SB-256 N1: the owner runs the CLI itself; only Claude's sign-in reads a pasted code.
    if headless && (provider != Provider::Claude || cfg!(windows)) {
        return Err(HEADLESS_CLAUDE_ONLY.into());
    }
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
    if headless {
        crate::headless_login::start(&home, &program, &env, CONFLICTS)?;
        let _ = fs::remove_file(home.join(".launch-pending"));
    } else {
        open_terminal(&home, &script(&home, &program, &args, &env, true, &home))?;
    }
    reservation.commit(Handed::Terminal);
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
pub(crate) fn keychain_service(home: &Path) -> String {
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
pub(crate) fn login_user() -> Result<String, String> {
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
pub(crate) const HEADLESS_CLAUDE_ONLY: &str =
    "A sign-in without Terminal is available for Claude on macOS and Linux only.";
/// Where an official sign-in stands, read from its private home only — no credential.
/// `ended` means the Terminal session exited without the provider CLI succeeding.
pub fn login_state(login: &Login) -> &'static str {
    if read_regular(&login.home.join(".completed")).is_ok_and(|v| v == "complete") {
        return "complete";
    }
    // SB-256 N1: a sign-in this owner runs ended when its CLI is no longer running here (it
    // exited without success, or a previous owner started it).
    if login.home.join(crate::headless_login::MARKER).exists() {
        return if crate::headless_login::running(&login.home) {
            "pending"
        } else {
            "ended"
        };
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
/// SB-256 N1: the manual sign-in URL of a sign-in this owner runs, once the CLI printed it.
pub fn headless_url(login: &Login) -> Option<String> {
    crate::headless_login::manual_url(&login.home)
}
/// SB-256 N1: the code the sign-in page showed, to the CLI of a sign-in this owner runs.
pub fn submit_login_code(login: &Login, code: &str) -> Result<(), String> {
    if !login.home.join(crate::headless_login::MARKER).exists() {
        return Err(crate::headless_login::NOT_WAITING.into());
    }
    crate::headless_login::submit_code(&login.home, code)
}
pub fn cancel_login(login: &Login) -> Result<(), String> {
    crate::headless_login::stop(&login.home, Duration::from_secs(3));
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
    // SB-449: the same judgement as `ensure_idle`: a marker that is not a regular file holds here
    // too (CONTRACTS); only a home whose marker cannot be looked at holds no reservation.
    pending(home, SystemTime::now()) == Ok(Pending::Held)
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
/// SB-302: how long `pid` has run, read in-process (macOS `proc_pidinfo`, Linux `/proc`); `ps`
/// only where neither answers. A `sessions` read asks this once per live session, and a process
/// started per question took seconds each while the machine swapped.
fn process_age(pid: u32) -> Option<Duration> {
    process_started(pid)
        .and_then(|started| SystemTime::now().duration_since(started).ok())
        .or_else(|| process_age_from_ps(pid))
}
#[cfg(target_os = "macos")]
pub(crate) fn process_started(pid: u32) -> Option<SystemTime> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer is a zeroed proc_bsdinfo of exactly `size` bytes, which is what
    // PROC_PIDTBSDINFO writes; nothing else of this process is touched.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: proc_pidinfo filled the whole struct (checked above).
    let info = unsafe { info.assume_init() };
    Some(
        std::time::UNIX_EPOCH
            + Duration::from_secs(info.pbi_start_tvsec)
            + Duration::from_micros(info.pbi_start_tvusec),
    )
}
/// SB-445: the process's start, from `/proc/<pid>/stat` field 22 (clock ticks after boot) and the
/// boot time in `/proc/stat`; the modification time of `/proc/<pid>` is not its start.
#[cfg(target_os = "linux")]
fn process_started(pid: u32) -> Option<SystemTime> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // After the command name in parentheses, field 22 (`starttime`) is the 20th.
    let rest = &stat[stat.rfind(')')? + 1..];
    let ticks: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    // SB-449: the boot time does not change while this process runs; read it once.
    static BOOT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let boot: u64 = (*BOOT.get_or_init(|| {
        fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("btime ")?.trim().parse().ok())
    }))?;
    // SAFETY: sysconf only reads a configuration value.
    let hz = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
        .ok()
        .filter(|hz| *hz > 0)?;
    Some(
        std::time::UNIX_EPOCH
            + Duration::from_secs(boot)
            + Duration::from_micros(ticks.checked_mul(1_000_000)? / hz),
    )
}
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn process_started(_pid: u32) -> Option<SystemTime> {
    None
}
/// SB-302: whether `pid` exists and may be signalled by this process, as `kill -0` answers,
/// without starting `/bin/kill`.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists and may be signalled.
    unsafe { libc::kill(pid, 0) == 0 }
}
#[cfg(unix)]
fn process_age_from_ps(pid: u32) -> Option<Duration> {
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
/// Whether nothing holds `home`: no marker that still holds and no live session. Reads only
/// (SB-448): a stale marker counts as idle, since nothing will run its script, and is left for
/// the next `Reservation::new` to clear (SB-440: also when its launcher is gone, not only after
/// ten minutes), so a check never removes a marker another launch writes meanwhile.
pub(crate) fn ensure_idle(home: &Path) -> Result<(), String> {
    match pending(home, SystemTime::now())? {
        Pending::Absent | Pending::Stale(_) => {}
        Pending::Held => {
            return Err("Close the existing session before changing its home.".into());
        }
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
        if pid_alive(pid) {
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
    for (n, signal) in [libc::SIGHUP, libc::SIGTERM].into_iter().enumerate() {
        // Checked before each signal: the pid must still be this home's script.
        if !runs_script_of(pid, home) {
            // S2 (SB-221): after the hang-up the script may be gone already, its pid a zombie
            // its parent has not reaped yet on a loaded machine (`ps` no longer shows the
            // script); the session ended, so its home turns idle. Nothing is signalled again.
            if n > 0 && idle_within(home, wait) {
                return Ok(());
            }
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
/// SB-282: the pool a session of `account` in `folder` runs in: its home when its own group (or
/// none) holds the folder, else the first group that shares it and holds the folder.
pub(crate) fn serving_pool(
    snapshot: &switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    folder: &Path,
) -> String {
    let holds = |pool: &str| {
        snapshot
            .project_of_pool(pool)
            .is_some_and(|p| p.folders.iter().any(|f| folder.starts_with(Path::new(f))))
    };
    if holds(&account.pool) {
        return account.pool.clone();
    }
    snapshot
        .pools_of(account)
        .into_iter()
        .find(|p| holds(p))
        .unwrap_or(&account.pool)
        .to_owned()
}
/// SB-401: a pay-per-token key launched with its credential in the session's own home would
/// reach the provider directly, past the daily ceiling the proxy checks.
pub(crate) const DELEGATED_ISOLATED: &str = "A seat another machine delegated runs only through Switchboard, in its group's folders. Launch it in managed mode.";
pub(crate) const TOKENS_ISOLATED: &str = "A pay-per-token key runs only through Switchboard, where its daily ceiling is checked. Launch it in managed mode.";
/// SB-401: a Claude API key from before the pay-per-token group, outside it: no ceiling holds it.
pub(crate) use switchboard_core::KEY_OUTSIDE_TOKENS;
/// SB-401: removes a Claude API key from its own home's `settings.json` (an isolated launch of
/// 0.7.111 or older wrote it there): the key launches managed only now, through the proxy. The
/// person's other settings stay. Answers how many homes changed.
pub(crate) fn scrub_key_homes(root: &Path, snapshot: &switchboard_core::Snapshot) -> usize {
    let mut changed = 0;
    for account in snapshot
        .accounts
        .iter()
        .filter(|a| switchboard_core::Snapshot::is_claude_key(a))
    {
        let path = root.join("homes").join(&account.id).join("settings.json");
        let Ok(bytes) = switchboard_core::private_fs::read_private(&path, 1024 * 1024) else {
            continue;
        };
        let Ok(mut settings) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let removed = settings
            .get_mut("env")
            .and_then(Value::as_object_mut)
            .and_then(|env| env.remove("ANTHROPIC_API_KEY"))
            .is_some();
        if removed && private_write(&path, settings.to_string().as_bytes(), false).is_ok() {
            changed += 1;
        }
    }
    changed
}
/// SB-401: an account of a group switched off starts nothing; a Claude API key outside the
/// pay-per-token group starts in no mode; one in it starts only in managed mode (its credential never lands in a session's home), and one in
/// the pay-per-token group only while that group is on and has a daily ceiling. Fails closed: a
/// ceiling file that cannot be read is no ceiling.
pub(crate) fn group_allows_launch(
    root: &Path,
    snapshot: &switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    mode: &str,
) -> Result<(), String> {
    // D-18: a key outside the pay-per-token group launches in no mode.
    if snapshot.key_outside_tokens(account) {
        return Err(KEY_OUTSIDE_TOKENS.into());
    }
    if switchboard_core::Snapshot::is_claude_key(account) && mode != "managed" {
        return Err(TOKENS_ISOLATED.into());
    }
    // SB-403: a seat another machine delegated serves its group's managed sessions only; an
    // isolated home would hold its sign-in outside the group and outside its lease.
    if snapshot.is_delegated(&account.id) && mode != "managed" {
        return Err(DELEGATED_ISOLATED.into());
    }
    let Some(group) = snapshot.project_of_pool(&account.pool) else {
        return Ok(());
    };
    if group.off {
        return Err(switchboard_core::GROUP_OFF.into());
    }
    if group.tokens && crate::spend::group_ceiling(root, &group.pool).is_none() {
        return Err(switchboard_core::TOKENS_NO_CEILING.into());
    }
    Ok(())
}
pub const MODEL_NOT_TAKEN: &str =
    "Only Claude Code, and Hermes on the pay-per-token group, take a model at launch through the proxy. Leave out the model, or choose it in the agent itself.";
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
    // DEC-0022 (SB-403): an account runs in the folders of any group that holds it, whatever
    // its organizations; the session's context (`context_of`) keeps personal and organization
    // data apart.
    let groups: Vec<&switchboard_core::Project> = snapshot
        .pools_of(account)
        .into_iter()
        .filter_map(|p| snapshot.project_of_pool(p))
        .collect();
    if !groups.is_empty()
        && !groups
            .iter()
            .any(|g| g.folders.iter().any(|f| folder.starts_with(Path::new(f))))
    {
        return Err(PROJECT_ACCOUNT_OUTSIDE.into());
    }
    // SB-146: a folder in several groups may run on any of them; another account is refused
    // only while one of its groups has its own account for the provider.
    let here = snapshot.projects_for(folder);
    if !here.iter().any(|p| snapshot.serves(account, &p.pool)) {
        let has_own = here.iter().any(|p| {
            snapshot
                .accounts
                .iter()
                .any(|a| snapshot.serves(a, &p.pool) && a.provider == account.provider && a.enabled)
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
    model: Option<&str>,
) -> Result<Value, String> {
    launch_agent_with(
        root,
        store,
        proxy,
        agent,
        pool,
        working_directory,
        model,
        find_program,
        start_terminal,
    )
}
pub const HERMES_TOKENS_MODEL_NEEDED: &str =
    "Name the model for Hermes, an Anthropic id such as claude-sonnet-5-5: Hermes on the pay-per-token group starts on the model you name.";
/// SB-404 K3: Hermes on the pay-per-token group, through the proxy, so every answer is priced and
/// counted against the group's daily ceiling (DEC-0023). Hermes reads its provider from its own
/// config file: the launch writes one in a Hermes home of its own, pointing a provider named
/// `switchboard` at the group's route with the agents' capability (never a provider key), and
/// leaves `~/.hermes` alone. The person's own sign-ins are not adopted.
fn hermes_tokens_home(home: &Path, base: &str, model: &str) -> Result<PathBuf, String> {
    let own = home.join("agent-home");
    private_dir(&own)?;
    let mut config = format!(
        "# Written by Fabric Switchboard for this launch: Hermes on the pay-per-token group, through its proxy.\n\
providers:\n  switchboard:\n    api: \"{base}\"\n    key_env: {HERMES_KEY_ENV}\n    transport: anthropic_messages\n\
fallback_providers: []\n\
auth:\n  adopt_external_logins: false\n\
auxiliary:\n  free_only: true\n"
    );
    // Review round 1, P2-10: Hermes's side tasks (titles, compression, vision, approval and the
    // rest) pick their own provider; each is pinned to this launch's provider and model, so none
    // falls through to OpenRouter, Nous or a direct key past the meter.
    for task in HERMES_AUX_TASKS {
        config.push_str(&format!(
            "  {task}:\n    provider: switchboard\n    model: \"{model}\"\n"
        ));
    }
    switchboard_core::private_fs::private_write(&own.join("config.yaml"), config.as_bytes())
        .map_err(|_| "Managed home unavailable.".to_string())?;
    // Review round 3: Hermes 0.22.2 loads the home's `.env` first (overriding) and then its
    // install folder's `.env`, which fills only names not set yet (`env_loader.py`). The home's
    // `.env` names every provider key empty, so each is set, empty, and the install folder's
    // value is skipped: an unset key cannot come back from there.
    let dotenv: String = HERMES_PROVIDER_KEYS
        .iter()
        .map(|name| format!("{name}=\n"))
        .collect();
    switchboard_core::private_fs::private_write(&own.join(".env"), dotenv.as_bytes())
        .map_err(|_| "Managed home unavailable.".to_string())?;
    Ok(own)
}
/// Hermes's side tasks with a provider of their own (`hermes_cli/config_defaults.py`
/// `auxiliary`, Hermes 0.22.2).
const HERMES_AUX_TASKS: &[&str] = &[
    "vision",
    "compression",
    "skills_hub",
    "approval",
    "review",
    "mcp",
    "title_generation",
    "memory_query_rewrite",
    "tts_audio_tags",
    "voice_chat",
    "triage_specifier",
    "kanban_decomposer",
    "profile_describer",
    "goal_judge",
    "curator",
    "monitor",
];
/// Provider keys Hermes would find in the environment and use past the meter (its auxiliary
/// chain tries OpenRouter, Nous and direct API-key providers): unset for a metered launch. Keys a
/// session's own tools use (`GH_TOKEN`, `GITHUB_TOKEN`, `HF_TOKEN`) stay.
pub(crate) const HERMES_PROVIDER_KEYS: &[&str] = &[
    "OPENROUTER_API_KEY",
    "NOUS_API_KEY",
    "NOUS_BASE_URL",
    "NOUS_INFERENCE_BASE_URL",
    "NOUS_PORTAL_BASE_URL",
    "ANTHROPIC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "ALIBABA_CODING_PLAN_API_KEY",
    "ARCEEAI_API_KEY",
    "AZURE_FOUNDRY_API_KEY",
    "COPILOT_GITHUB_TOKEN",
    "DASHSCOPE_API_KEY",
    "DEEPSEEK_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GLM_API_KEY",
    "GMI_API_KEY",
    "KILOCODE_API_KEY",
    "KIMI_API_KEY",
    "KIMI_CN_API_KEY",
    "KIMI_CODING_API_KEY",
    "LM_API_KEY",
    "MINIMAX_API_KEY",
    "MINIMAX_CN_API_KEY",
    "NVIDIA_API_KEY",
    "OLLAMA_API_KEY",
    "OPENCODE_GO_API_KEY",
    "OPENCODE_ZEN_API_KEY",
    "STEPFUN_API_KEY",
    "TOKENHUB_API_KEY",
    "TOKENPLAN_API_KEY",
    "XAI_API_KEY",
    "XIAOMI_API_KEY",
    "Z_AI_API_KEY",
    "ZAI_API_KEY",
    "GROQ_API_KEY",
    "MISTRAL_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "CEREBRAS_API_KEY",
    "PERPLEXITY_API_KEY",
    // Review round 2: cloud credentials and the remaining providers Hermes 0.22.2 reads.
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_PROFILE",
    "AWS_BEARER_TOKEN_BEDROCK",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "NEBIUS_API_KEY",
    "NEBIUS_TOKEN_FACTORY_API_KEY",
    "UPSTAGE_API_KEY",
    "ACTUAL_API_KEY",
    "MOA_API_KEY",
    "HERMES_CUSTOM_LMSTUDIO_API_KEY",
];
/// A launch script that also clears `names` before anything runs (as it clears `CONFLICTS`).
fn with_unset(content: String, names: &[&str]) -> String {
    if names.is_empty() {
        return content;
    }
    #[cfg(not(windows))]
    {
        content.replacen("\nunset ", &format!("\nunset {} ", names.join(" ")), 1)
    }
    #[cfg(windows)]
    {
        let lines: String = names
            .iter()
            .map(|n| {
                format!(
                    "[Environment]::SetEnvironmentVariable({}, $null, 'Process')\n",
                    powershell_quote(n)
                )
            })
            .collect();
        content.replacen(
            "Set-PSDebug -Off\n",
            &format!("Set-PSDebug -Off\n{lines}"),
            1,
        )
    }
}
/// The variable the Hermes home's provider reads the agents' capability from.
const HERMES_KEY_ENV: &str = "SWITCHBOARD_AGENT_KEY";
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
    model: Option<&str>,
    find: fn(&str) -> Option<PathBuf>,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let profile = crate::agent_catalog::launchable(agent)?;
    // SB-404 K3: a model named for the launch is an Anthropic id (the route speaks Claude's API).
    if model.is_some_and(|m| !crate::anthropic_keys::model_valid(m)) {
        return Err(crate::anthropic_keys::MODEL_INVALID.into());
    }
    let working_directory = agent_folder(root, working_directory)?;
    let (account, _) = store
        .route(Provider::Claude, pool)
        .map_err(|_| "Select an API-key account in this pool first.".to_string())?;
    if account.kind != AuthKind::ApiKey {
        return Err(switchboard_proxy::AGENT_SUBSCRIPTION_REFUSED.into());
    }
    let snapshot = store.snapshot()?;
    group_allows_launch(root, &snapshot, &account, "managed")?;
    project_allows(&snapshot, &account, &working_directory)?;
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
        .serving_context(&account, pool, Some(&working_directory));
    let mut env = BTreeMap::from([
        (
            "SWITCHBOARD_SESSION".to_string(),
            format!("managed:claude:{pool}"),
        ),
        ("SWITCHBOARD_CONTEXT".to_string(), context.clone()),
    ]);
    if let (Some(base_env), Some(key_env)) = (&profile.base_env, &profile.key_env) {
        env.insert(base_env.clone(), base.clone());
        env.insert(key_env.clone(), proxy.agent_token().to_owned());
    }
    env.extend(profile.extra_env.clone());
    let mut args: Vec<String> = profile.args.clone();
    let mut unset: Vec<&str> = Vec::new();
    // SB-404 K3: Hermes on the pay-per-token group runs through the proxy, in a home of its own.
    let tokens = snapshot.project_of_pool(pool).is_some_and(|g| g.tokens);
    if profile.id == "hermes" && tokens {
        let model = model.ok_or(HERMES_TOKENS_MODEL_NEEDED)?;
        let own = hermes_tokens_home(&home, &base, model)?;
        unset.extend_from_slice(HERMES_PROVIDER_KEYS);
        env.insert("HERMES_HOME".into(), own.to_string_lossy().into_owned());
        env.insert(HERMES_KEY_ENV.into(), proxy.agent_token().to_owned());
        args.extend([
            "--provider".into(),
            "switchboard".into(),
            "-m".into(),
            model.to_owned(),
        ]);
    } else if let Some(model) = model {
        // Claude Code takes the model as `--model`; another agent picks it in its own config.
        if profile.id != "claude-code" {
            return Err(MODEL_NOT_TAKEN.into());
        }
        args.extend(["--model".into(), model.to_owned()]);
    }
    // SB-212: the session's own launch id, for the record and for what it launches.
    let launch_id = crate::launches::new_id();
    env.insert(crate::launches::ENV.into(), launch_id.clone());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let content = with_unset(
        script(&home, &program, &args, &env, false, &working_directory),
        &unset,
    );
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
        "label": model,
    });
    crate::launches::record(root, &home, record, &launch_id)?;
    start(&path)?;
    Ok(
        json!({"launched": true, "agent": profile.id, "name": profile.name, "pool": pool, "account": account.label, "model": model, "metered": tokens}),
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
/// SB-390: the agents that run on an Anthropic API key directly, with their provider flag.
const ANTHROPIC_AGENTS: &[(&str, &str)] = &[("hermes", "Hermes Agent")];
/// SB-392 K-1: whether `agent` runs on a saved Anthropic key.
pub fn takes_anthropic(agent: &str) -> bool {
    ANTHROPIC_AGENTS.iter().any(|(id, _)| *id == agent)
}
pub const ANTHROPIC_AGENT_UNSUPPORTED: &str =
    "Only Hermes runs on an Anthropic key for now. Choose Hermes, or its OpenRouter key.";
pub const ANTHROPIC_MODEL_NEEDED: &str =
    "Choose a model for this launch, or set the key's default model with switchboard agents anthropic model.";
/// SB-390: starts Hermes in a folder on the Anthropic key saved under `key` (`team-a`,
/// `personal`) and `model` (default: the key's model), as `hermes --provider anthropic -m
/// <model>`. Like an OpenRouter launch (`launch_agent_openrouter`), it runs in a Hermes home of
/// its own (`HERMES_HOME`, so `~/.hermes`'s own sign-ins do not win over the key), and the key is
/// read when the session starts (`switchboard agents anthropic key <name>`), never written. No
/// proxy, group or account is involved, and no ceiling counts it.
pub fn launch_agent_anthropic(
    root: &Path,
    agent: &str,
    key: &str,
    model: Option<&str>,
    working_directory: &Path,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let (id, name) = ANTHROPIC_AGENTS
        .iter()
        .find(|(id, _)| *id == agent)
        .copied()
        .ok_or(ANTHROPIC_AGENT_UNSUPPORTED)?;
    let saved = crate::anthropic_keys::find(root, key)?;
    let model = match model {
        Some(m) if !crate::anthropic_keys::model_valid(m) => {
            return Err(crate::anthropic_keys::MODEL_INVALID.into())
        }
        Some(m) => m.to_owned(),
        None => saved.model.clone().ok_or(ANTHROPIC_MODEL_NEEDED)?,
    };
    let working_directory = agent_folder(root, working_directory)?;
    let program = find_program(id).ok_or_else(|| {
        format!("{name} is not installed. Install it first, then launch it again.")
    })?;
    let cli = agent_cli().ok_or(OPENROUTER_CLI_MISSING)?;
    let homes = root.join("runtimes");
    private_dir(&homes)?;
    let home = homes.join(format!("agent-{id}-anthropic-{key}"));
    private_dir(&home)?;
    let own = home.join("agent-home");
    private_dir(&own)?;
    let launch_id = crate::launches::new_id();
    let env = BTreeMap::from([
        (
            "HERMES_HOME".to_string(),
            own.to_string_lossy().into_owned(),
        ),
        (crate::launches::ENV.to_string(), launch_id.clone()),
    ]);
    let args = ["--provider", "anthropic", "-m", model.as_str()];
    let key_command = vec![
        "--data-dir".to_string(),
        root.to_string_lossy().into_owned(),
        "agents".into(),
        "anthropic".into(),
        "key".into(),
        key.to_owned(),
        // SB-420: the key prints only into the pipe this script reads it from.
        "--to-pipe".into(),
    ];
    let content = script_with(
        &home,
        &program,
        &args,
        &env,
        Some(("ANTHROPIC_API_KEY", &cli, &key_command)),
        false,
        &working_directory,
    );
    let path = write_launch_script(&home, &content)?;
    // Listed under Sessions like any launch, labelled with the key's name and model; never the key.
    let record = json!({
        "account_id": format!("anthropic:{key}"), "provider": "anthropic", "agent": id,
        // The key's own name, as an OpenRouter launch writes `openrouter` (SB-240): never a group.
        "pool": "anthropic", "mode": "isolated",
        "working_directory": working_directory, "in_place": false,
        "launched_at": switchboard_proxy::now(), "continues": null,
        "label": format!("{key} · {model}"),
    });
    crate::launches::record(root, &home, record, &launch_id)?;
    start(&path)?;
    Ok(
        json!({"launched": true, "agent": id, "name": name, "via": "anthropic", "key": key, "model": model}),
    )
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
        // SB-144, SB-213: the key of the project the agent starts in, by the slot its spend gate
        // read, never by its folder when the script runs.
        let (key_command, key_slot) = key_command(root, &saved)?;
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
        // SB-212: the session's own launch id, for the record and for what it launches.
        let launch_id = crate::launches::new_id();
        env.insert(crate::launches::ENV.into(), launch_id.clone());
        let prompt = prompt.to_string_lossy().into_owned();
        let mut args = vec!["chat".to_string(), "--query-file".into(), prompt];
        args.extend(
            recipe
                .model_flag
                .iter()
                .map(|a| a.replace("{model}", &model)),
        );
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
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
            "account_id": crate::hermes::HERMES_TARGET, "provider": "hermes", "pool": switchboard_core::KEY_POOL,
            "context": context, "mode": "isolated", "working_directory": working_directory,
            "in_place": false,
            "launched_at": switchboard_proxy::now(), "continues": continuation.workflow_id,
            "label": model,
        });
        crate::launches::record(root, &session, record, &launch_id)?;
        Ok((
            path.clone(),
            json!({"launched": true, "agent": "hermes", "via": "openrouter", "model": model, "model_from": model_from, "script": path, "key_slot": key_slot}),
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
        // SB-144, SB-213: the key of the project the agent starts in, by the slot its spend gate
        // read, never by its folder when the script runs.
        let (key_command, key_slot) = key_command(root, &saved)?;
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
        // SB-212: the session's own launch id, for the record and for what it launches.
        let launch_id = crate::launches::new_id();
        env.insert(crate::launches::ENV.into(), launch_id.clone());
        let model_arg = format!("openrouter/{model}");
        let args = ["--prompt", first.as_str(), "-m", model_arg.as_str()];
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
            "account_id": OPENCODE_TARGET, "provider": "opencode", "pool": switchboard_core::KEY_POOL,
            "context": context, "mode": "isolated", "working_directory": working_directory,
            "in_place": false,
            "launched_at": switchboard_proxy::now(), "continues": continuation.workflow_id,
            "label": model,
        });
        crate::launches::record(root, &session, record, &launch_id)?;
        Ok((
            path.clone(),
            json!({"launched": true, "agent": "opencode", "via": "openrouter", "model": model, "model_from": "key", "script": path, "key_slot": key_slot}),
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
    crate::carry::link_into_home(own, &user, crate::carry::PI_LINKS);
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
/// context; the machine's key is personal outside a group and the folder's group's context
/// inside one (DEC-0022, SB-403 round 1).
fn key_context(
    store: &Store,
    key: &switchboard_core::agent_keys::AgentKey,
    folder: &Path,
) -> Result<String, String> {
    Ok(store
        .snapshot()?
        .key_context(key.pool.as_deref(), Some(folder)))
}
/// SB-213: the arguments a session script fetches its OpenRouter key with, and the slot they
/// name: `saved`, the key its folder resolves to now. The script runs later, outside the owner
/// transaction, so it names the slot (`--pool <group>` or `--machine`) rather than its folder: a
/// folder that joins another group meanwhile never makes it spend a key no gate read. Inside a
/// gated start ([`crate::fallback::pinned`]) the slot is the one the gate read, or the start is
/// refused.
fn key_command(
    root: &Path,
    saved: &switchboard_core::agent_keys::AgentKey,
) -> Result<(Vec<String>, crate::spend::KeySlot), String> {
    let slot = match &saved.pool {
        Some(pool) => crate::spend::KeySlot::Group(pool.clone()),
        None => crate::spend::KeySlot::Machine,
    };
    if let Some(pin) = crate::fallback::pinned() {
        if pin.key() != Some(&slot) {
            return Err(crate::spend::STOP_FOLDER_MOVED.into());
        }
    }
    let mut command = vec![
        "--data-dir".to_string(),
        root.to_string_lossy().into_owned(),
        "agents".into(),
        "key".into(),
        "--service".into(),
        "openrouter".into(),
    ];
    match &slot {
        crate::spend::KeySlot::Machine => command.push("--machine".into()),
        crate::spend::KeySlot::Group(pool) => {
            command.push("--pool".into());
            command.push(pool.clone());
        }
    }
    // SB-420: the key prints only into the pipe this script reads it from.
    command.push("--to-pipe".into());
    Ok((command, slot))
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
    // SB-212: the session's own launch id, for the record and for what it launches.
    let launch_id = crate::launches::new_id();
    env.insert(crate::launches::ENV.into(), launch_id.clone());
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
    // SB-144, SB-213: the key of the project the agent starts in, by the slot its spend gate read,
    // never by its folder when the script runs.
    let (key_command, key_slot) = key_command(root, &saved)?;
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
        "account_id": recipe.id, "provider": recipe.id, "agent": recipe.id, "pool": switchboard_core::KEY_POOL,
        "context": context, "mode": "isolated", "working_directory": working_directory,
        "in_place": false, "launched_at": switchboard_proxy::now(), "continues": null,
        "label": if takes_model { model.clone() } else { None },
    });
    crate::launches::record(root, &home, record, &launch_id)?;
    Ok((
        path,
        json!({
            "launched": true,
            "agent": recipe.id,
            "name": recipe.name,
            "via": "openrouter",
            "model": if takes_model { model } else { None },
            "model_choice": if takes_model { "launch" } else { "agent" },
            "key_slot": key_slot,
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
#[allow(clippy::too_many_arguments)]
pub fn continuation_here(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    mode: &str,
    working_directory: &Path,
    continuation: &Continuation,
    handed: Handed,
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
        handed,
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
    handed: Handed,
) -> Result<(bool, PathBuf), String> {
    if cfg!(windows) {
        return Err(IN_PLACE_UNSUPPORTED.into());
    }
    let (tools, script, _) = launch_core(
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
        handed,
        None,
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
    let snapshot = store.snapshot()?;
    // SB-322: an account given to an agent launches only once the person used it anyway, which
    // made it its group's selection.
    if snapshot.is_assigned(&account.id)
        && snapshot
            .routes
            .get(&format!("{}:{}", account.provider.as_str(), account.pool))
            != Some(&account.id)
    {
        return Err(switchboard_core::ASSIGNED.into());
    }
    group_allows_launch(root, &snapshot, &account, mode)?;
    project_allows(&snapshot, &account, &working_directory)?;
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
    launch_core(
        root,
        store,
        proxy,
        id,
        mode,
        working_directory,
        host,
        continuation,
        extra,
        in_place,
        if in_place {
            Handed::Caller
        } else {
            Handed::Terminal
        },
        None,
    )
    .map(|(tools, script, _)| (tools, script))
}
#[allow(clippy::too_many_arguments)]
fn launch_core(
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
    // SB-440: who runs the script once it is prepared (only when it is not opened in Terminal).
    handed: Handed,
    headless: Option<&Headless>,
) -> Result<(bool, Option<PathBuf>, Option<Dispatched>), String> {
    if !extra_valid(extra) {
        return Err(EXTRA_ARGS_INVALID.into());
    }
    let (working_directory, account) = validate(root, store, id, mode, working_directory)?;
    // SB-282: a managed session runs in the group whose folder it starts in, which may share
    // the account rather than hold it.
    let pool = serving_pool(&store.snapshot()?, &account, &working_directory);
    let program = match headless {
        Some(h) => h.program.clone(),
        None => (host.binary)(account.provider)?,
    };
    let homes = root.join(if mode == "managed" {
        "runtimes"
    } else {
        "homes"
    });
    private_dir(&homes)?;
    let name = if mode == "managed" {
        format!("{}-{}", account.provider.as_str(), pool)
    } else {
        account.id.clone()
    };
    let home = homes.join(name);
    private_dir(&home)?;
    let mut reservation = Reservation::new(&home)?;
    let session = if mode == "managed" {
        format!("managed:{}:{}", account.provider.as_str(), pool)
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
                crate::carry::link_into_home(&home, user, crate::carry::CLAUDE_LINKS);
            }
            Provider::Codex => {
                // SB-247: the person's hooks (`hooks.json`, where fabric-agent-adapter installs
                // its hook) come along as Claude Code's settings do.
                crate::carry::link_into_home(&home, user, CODEX_LINKS);
            }
        }
    }
    // SB-247: Codex runs the linked hooks only when this home's own config.toml trusts them.
    let hook_trust = match (account.provider, &user) {
        (Provider::Codex, Some(user)) => crate::carry::codex_hook_trust(&home, user),
        _ => String::new(),
    };
    if account.provider == Provider::Claude {
        crate::carry::claude_state(&home, continuation.map(|_| working_directory.as_path()))?;
        // SB-358: the person's MCP sign-ins (Figma, Sentry...) from their other Claude homes, as
        // access-token mirrors; never a failure of the launch. SB-403 S3: not while the person's
        // own sign-in is off limits, since they live in the ordinary home's credential item.
        if !crate::own_sign_in::is_off(root) {
            (host.mcp_sign_ins)(root, &home);
        }
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
        .serving_context(&account, &pool, Some(&working_directory));
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
    // SB-259 (passioncode PC-129): the operator's own Claude folder this home was built from, for
    // a tool that writes the person's settings: the home's `settings.json` is rebuilt at every
    // launch, so a write there is lost.
    if let (Provider::Claude, Some(user)) = (account.provider, &user) {
        env.insert(SOURCE_DIR.into(), user.to_string_lossy().into_owned());
    }
    if mode == "managed" {
        let (selected, _) = store.route(account.provider, &pool)?;
        if selected.id != account.id {
            return Err("Select this account before launching managed mode.".into());
        }
        let base = format!(
            "http://{}/{}/{}",
            proxy.address(),
            account.provider.as_str(),
            pool
        );
        match account.provider {
            Provider::Claude => {
                write_route(&home, &base, proxy.token(), &pool)?;
                env.insert("ANTHROPIC_BASE_URL".into(), base);
                env.insert("ANTHROPIC_AUTH_TOKEN".into(), proxy.token().into());
                env.insert("ANTHROPIC_API_KEY".into(), String::new());
            }
            Provider::Codex => {
                env.insert("SWITCHBOARD_LOCAL_TOKEN".into(), proxy.token().into());
                let config=format!("model_provider = \"switchboard\"\ncli_auth_credentials_store = \"file\"\n[model_providers.switchboard]\nname = \"Fabric Switchboard\"\nbase_url = \"{base}/v1\"\nenv_key = \"SWITCHBOARD_LOCAL_TOKEN\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n{}\n{hook_trust}", tools.codex_section);
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
                        "cli_auth_credentials_store = \"file\"\n{}\n{hook_trust}",
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
    // SB-247: a dispatched run starts no interactive session (`headless_args`).
    // SB-452: a managed Claude session takes its route from its home's own file, whatever a
    // folder's (or its main checkout's) local settings say.
    let route =
        (mode == "managed" && account.provider == Provider::Claude).then(|| route_arg(&home));
    if let Some(h) = headless {
        let mut args = headless_args(account.provider, config.as_deref(), h);
        args.extend(route.clone());
        let launch_id = crate::launches::new_id();
        env.insert(crate::launches::ENV.into(), launch_id.clone());
        env.insert(DISPATCH_ENV.into(), h.workflow.clone());
        env.insert(DISPATCH_HANDOFF_ENV.into(), h.handoff.clone());
        env.insert(DISPATCH_REQUEST_ENV.into(), h.request.clone());
        // Fabric's dispatch id (`--dispatch-id`), when it gave one: the adapter keys the run on it.
        if let Some(id) = h.by["id"].as_str() {
            env.insert(DISPATCH_ID_ENV.into(), id.to_owned());
        }
        let record = json!({
            "account_id": account.id,
            "provider": account.provider.as_str(),
            "pool": pool,
            "context": context,
            "mode": mode,
            "working_directory": working_directory,
            "in_place": false,
            "launched_at": switchboard_proxy::now(),
            "continues": h.workflow,
            "dispatch": {
                "workflow": h.workflow,
                "permissions": h.permissions.as_str(),
                "timeout_minutes": h.timeout_minutes,
                "deadline": h.deadline,
                "log": format!("{}/{}/{launch_id}.log", crate::launches::DIR, crate::launches::LOGS),
            },
        });
        crate::launches::record_by(root, &home, record, &launch_id, Some(h.by.clone()))?;
        let child = spawn_headless(
            root,
            &home,
            &program,
            &args,
            &env,
            &working_directory,
            &launch_id,
        )?;
        reservation.commit(Handed::Caller);
        journal_launch(store, id, mode);
        return Ok((
            cli.is_some(),
            None,
            Some(Dispatched {
                launch_id,
                account_id: account.id.clone(),
                child,
            }),
        ));
    }
    let mut args: Vec<&str> = config.iter().map(String::as_str).collect();
    // SB-298 (operator 2026-11-07): a session Switchboard starts runs with permissions bypassed,
    // since nobody may be there to answer; the person's `agents permissions ask` keeps the
    // prompts, and a caller that names its own permission mode keeps it.
    args.extend(bypass_args(root, account.provider, extra));
    // Both CLIs start an interactive session with a positional first message.
    if let Some(first) = &first {
        args.push(first.as_str());
    }
    args.extend(extra.iter().map(String::as_str));
    // SB-452: last, so a caller's own `--settings` cannot replace it; before a caller's `--`, after
    // which it would be read as a prompt.
    if let Some(route) = &route {
        let at = args.iter().position(|a| *a == "--").unwrap_or(args.len());
        args.insert(at, route.as_str());
    }
    // SB-212: the session's own launch id, for the record and for what it launches.
    let launch_id = crate::launches::new_id();
    env.insert(crate::launches::ENV.into(), launch_id.clone());
    let path = write_launch_script(
        &home,
        &script(&home, &program, &args, &env, false, &working_directory),
    )?;
    // SB-95: what this home's session is, for the session list — ids, folder and times only.
    let record = json!({
        "account_id": account.id,
        "provider": account.provider.as_str(),
        "pool": pool,
        "context": context,
        "mode": mode,
        "working_directory": working_directory,
        "in_place": in_place,
        "launched_at": switchboard_proxy::now(),
        "continues": continuation.map(|c| c.workflow_id.clone()),
    });
    crate::launches::record(root, &home, record, &launch_id)?;
    // In place the caller runs the script now; the reservation lapses if it never does.
    if !in_place {
        (host.start_terminal)(&path)?;
    }
    reservation.commit(if in_place { handed } else { Handed::Terminal });
    journal_launch(store, id, mode);
    Ok((cli.is_some(), in_place.then_some(path), None))
}
/// What a Codex home Switchboard launches carries from the person's own Codex folder (SB-137,
/// SB-247): links, never a copy; nothing that signs in.
pub(crate) const CODEX_LINKS: &[&str] = &["AGENTS.md", "hooks.json"];
/// SB-247: the variable a dispatched run carries: the workflow Fabric opened for its board
/// request.
pub const DISPATCH_ENV: &str = "FABRIC_DISPATCH_WORKFLOW";
/// SB-247: the hand-over Fabric opened, and the board request the run answers.
pub const DISPATCH_HANDOFF_ENV: &str = "FABRIC_DISPATCH_HANDOFF";
pub const DISPATCH_REQUEST_ENV: &str = "FABRIC_DISPATCH_REQUEST";
/// SB-247: Fabric's id for the dispatch (`--dispatch-id`), absent without the flag.
pub const DISPATCH_ID_ENV: &str = "FABRIC_DISPATCH_ID";
/// SB-298: the arguments that bypass an interactive session's permission prompts, unless the
/// person chose `ask` or the caller's own arguments already name a permission mode. Codex takes
/// them as `-c` overrides, which hold before a subcommand such as `resume` (a caller's console
/// passes one); Claude Code takes its flag.
pub(crate) fn bypass_args(root: &Path, provider: Provider, extra: &[String]) -> Vec<&'static str> {
    let (args, own): (&[&'static str], &[&str]) = match provider {
        Provider::Claude => (
            &["--dangerously-skip-permissions"],
            &[
                "--dangerously-skip-permissions",
                "--permission-mode",
                "--allow-dangerously-skip-permissions",
            ],
        ),
        Provider::Codex => (
            &[
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_mode=\"danger-full-access\"",
            ],
            &[
                "--dangerously-bypass-approvals-and-sandbox",
                "--yolo",
                "--full-auto",
                "-a",
                "--ask-for-approval",
                "-s",
                "--sandbox",
                "approval_policy",
                "sandbox_mode",
            ],
        ),
    };
    let named = extra.iter().any(|a| {
        own.iter()
            .any(|o| a == o || a.starts_with(&format!("{o}=")))
    });
    if named || !crate::agent_settings::bypass(root) {
        return Vec::new();
    }
    args.to_vec()
}
/// SB-405: a hand-over taken with `--ask` continues only on Claude Code or Codex.
pub const ASK_AGENT: &str = "Only a Claude Code or Codex hand-over can start with permission prompts on, and this one continues on another agent. It is still waiting: take it without --ask, or leave it for Terminal.";
/// SB-405: the prepared session could not be read as Switchboard wrote it.
pub const ASK_UNREADABLE: &str = "Switchboard could not prepare this hand-over's session with permission prompts on. It is still waiting: take it without --ask, or leave it for Terminal.";
/// SB-405: the script a hand-over taken with `--ask` runs, beside the prepared one.
#[cfg(unix)]
pub(crate) const ASK_SCRIPT: &str = "launch-ask.command";
/// SB-405: the agent's arguments with its permission prompts on, whatever the prepared session
/// said. Claude Code loses Switchboard's bypass flags and runs in `--permission-mode manual` (an
/// explicit mode, since the person's settings, carried into the home, may default to bypassing);
/// a mode already named becomes `manual`. Codex loses Switchboard's `approval_policy="never"` and
/// `sandbox_mode="danger-full-access"` overrides and the bypass flags, and asks on request; a
/// sandbox the prepared session names otherwise (a person's `read-only`) is kept, and none is
/// added.
#[cfg(unix)]
pub(crate) fn ask_args(agent: &str, args: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::with_capacity(args.len() + 2);
    match agent {
        "claude-code" => {
            let mut named = false;
            let mut i = 0;
            while i < args.len() {
                let a = args[i].as_str();
                if matches!(
                    a,
                    "--dangerously-skip-permissions" | "--allow-dangerously-skip-permissions"
                ) {
                    i += 1;
                    continue;
                }
                if a == "--permission-mode" {
                    named = true;
                    out.extend(["--permission-mode".into(), "manual".into()]);
                    i += 2;
                    continue;
                }
                if a.starts_with("--permission-mode=") {
                    named = true;
                    out.push("--permission-mode=manual".into());
                    i += 1;
                    continue;
                }
                out.push(args[i].clone());
                i += 1;
            }
            if !named {
                out.splice(0..0, ["--permission-mode".into(), "manual".into()]);
            }
        }
        "codex" => {
            const ASKS: &str = "approval_policy=\"on-request\"";
            let mut named = false;
            let mut i = 0;
            while i < args.len() {
                let a = args[i].as_str();
                if matches!(a, "--dangerously-bypass-approvals-and-sandbox" | "--yolo") {
                    i += 1;
                    continue;
                }
                if a == "-c" {
                    let Some(value) = args.get(i + 1) else {
                        return Err(ASK_UNREADABLE.into());
                    };
                    let key = value.split('=').next().unwrap_or_default().trim();
                    if value == "sandbox_mode=\"danger-full-access\"" {
                        i += 2;
                        continue;
                    }
                    if key == "approval_policy" {
                        named = true;
                        out.extend(["-c".into(), ASKS.into()]);
                        i += 2;
                        continue;
                    }
                }
                if matches!(a, "-a" | "--ask-for-approval") {
                    named = true;
                    out.extend([a.to_owned(), "on-request".into()]);
                    i += 2;
                    continue;
                }
                if a.starts_with("--ask-for-approval=") {
                    named = true;
                    out.push("--ask-for-approval=on-request".into());
                    i += 1;
                    continue;
                }
                out.push(args[i].clone());
                i += 1;
            }
            if !named {
                out.splice(0..0, ["-c".into(), ASKS.into()]);
            }
        }
        _ => return Err(ASK_AGENT.into()),
    }
    Ok(out)
}
/// SB-405: the words of a line written by `quote`: each a run of `'...'` and `\'` pieces, one
/// space apart. Anything else is not a line Switchboard wrote: None.
#[cfg(unix)]
fn shell_words(line: &str) -> Option<Vec<String>> {
    let bytes = line.as_bytes();
    let mut words = Vec::new();
    let mut i = 0;
    loop {
        let mut word = String::new();
        let start = i;
        while i < bytes.len() && bytes[i] != b' ' {
            match bytes[i] {
                b'\'' => {
                    let end = line[i + 1..].find('\'')? + i + 1;
                    word.push_str(&line[i + 1..end]);
                    i = end + 1;
                }
                b'\\' if bytes.get(i + 1) == Some(&b'\'') => {
                    word.push('\'');
                    i += 2;
                }
                _ => return None,
            }
        }
        if i == start {
            return None;
        }
        words.push(word);
        if i == bytes.len() {
            return Some(words);
        }
        i += 1;
    }
}
/// SB-405: a prepared session script (`script_with`, not a sign-in) with the agent's permission
/// prompts on: its `exec` command (after the home's reservation is released, to the end, a first
/// message's line breaks included) rewritten through `ask_args`, everything before it kept.
#[cfg(unix)]
pub(crate) fn ask_script_text(text: &str, agent: &str) -> Result<String, String> {
    const RELEASED: &str = "rm -f .launch-pending\n";
    if !matches!(agent, "claude-code" | "codex") {
        return Err(ASK_AGENT.into());
    }
    let after = text.find(RELEASED).ok_or(ASK_UNREADABLE)? + RELEASED.len();
    let exec = text[after..].find("\nexec ").ok_or(ASK_UNREADABLE)? + after + 1;
    let command = text[exec + "exec ".len()..]
        .strip_suffix('\n')
        .ok_or(ASK_UNREADABLE)?;
    let words = shell_words(command).ok_or(ASK_UNREADABLE)?;
    let (program, args) = words.split_first().ok_or(ASK_UNREADABLE)?;
    let mut out = text[..exec].to_owned();
    out.push_str("exec ");
    out.push_str(&quote(program));
    for arg in ask_args(agent, args)? {
        out.push(' ');
        out.push_str(&quote(&arg));
    }
    out.push('\n');
    Ok(out)
}
/// SB-405: writes the hand-over's session with permission prompts on beside its prepared script
/// (`launch-ask.command`, 0700) and answers its path; the prepared script is left as it was. Read
/// and rewritten before the hand-over is taken, so a script that cannot be read leaves it waiting.
#[cfg(unix)]
pub(crate) fn write_ask_script(script: &Path, agent: &str) -> Result<PathBuf, String> {
    if !matches!(agent, "claude-code" | "codex") {
        return Err(ASK_AGENT.into());
    }
    let bytes = switchboard_core::private_fs::read_private(script, 256 * 1024)
        .map_err(|_| ASK_UNREADABLE.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| ASK_UNREADABLE.to_string())?;
    let rewritten = ask_script_text(&text, agent)?;
    let path = script.with_file_name(ASK_SCRIPT);
    private_write(&path, rewritten.as_bytes(), true).map_err(|_| ASK_UNREADABLE.to_string())?;
    Ok(path)
}
/// SB-405: a hand-over runs in place on macOS and Linux only, so none is prepared with prompts on
/// elsewhere.
#[cfg(not(unix))]
pub(crate) fn write_ask_script(_: &Path, agent: &str) -> Result<PathBuf, String> {
    if !matches!(agent, "claude-code" | "codex") {
        return Err(ASK_AGENT.into());
    }
    Err(ASK_UNREADABLE.into())
}
/// SB-247: how much a dispatched run may do without asking (`--permissions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permissions {
    /// The agent's own defaults: it asks, and with nobody to answer it is refused.
    Ask,
    /// Claude Code `--dangerously-skip-permissions`, Codex
    /// `--dangerously-bypass-approvals-and-sandbox`.
    All,
}
impl Permissions {
    pub fn as_str(self) -> &'static str {
        match self {
            Permissions::Ask => "ask",
            Permissions::All => "all",
        }
    }
}
/// SB-247: what makes a launch a dispatched run instead of a Terminal session.
pub(crate) struct Headless {
    /// The provider CLI, as `dispatch::program` found it.
    pub program: PathBuf,
    pub workflow: String,
    pub handoff: String,
    /// The board request's id (`--request`, else the packet's `board-request:`).
    pub request: String,
    /// The prompt's first line, `dispatch::header`.
    pub header: String,
    /// The text after it: accept the hand-over, then the request.
    pub prompt: String,
    /// The exit contract, appended to the system prompt (Claude Code) or put after the header
    /// (Codex, which has no such flag).
    pub contract: String,
    /// The record's `launched_by.dispatch`: `{id, request, thread, project, workflow}`.
    pub by: Value,
    pub permissions: Permissions,
    pub timeout_minutes: u32,
    /// When the time limit stops it (Unix seconds).
    pub deadline: i64,
}
/// SB-247: a dispatched run that started. The owner keeps `child` to reap it and record its end.
pub(crate) struct Dispatched {
    pub launch_id: String,
    pub account_id: String,
    pub child: std::process::Child,
}
/// SB-247: the arguments of a non-interactive run: `claude -p` with the exit contract appended to
/// the system prompt, or `codex exec` with the contract after the header; the prompt last, its
/// first line the header and an empty line after it.
pub(crate) fn headless_args(provider: Provider, config: Option<&str>, h: &Headless) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    match provider {
        Provider::Claude => {
            args.extend(config.map(str::to_owned));
            args.push("-p".into());
            args.push("--append-system-prompt".into());
            args.push(h.contract.clone());
            if h.permissions == Permissions::All {
                args.push("--dangerously-skip-permissions".into());
            }
            args.push(format!("{}\n\n{}", h.header, h.prompt));
        }
        Provider::Codex => {
            args.push("exec".into());
            if h.permissions == Permissions::All {
                args.push("--dangerously-bypass-approvals-and-sandbox".into());
            }
            args.push(format!("{}\n\n{}\n\n{}", h.header, h.contract, h.prompt));
        }
    }
    args
}
pub const DISPATCH_FAILED: &str =
    "The dispatched session did not start: its program could not be run.";
pub const DISPATCH_WOULD_NOT_STOP: &str =
    "The dispatched session did not stop. End its process by hand, then stop it again.";
/// SB-247, SB-455: the PATH of a dispatched run (`spawn_headless`), which inherits none: the
/// program's own folder first (a `#!/usr/bin/env node` launcher finds its `node` beside it), then
/// the person's login PATH (`login_path::login_dirs`, asked by the process that builds the run),
/// then this process's, then the per-user and Homebrew folders a launchd PATH lacks.
pub(crate) fn headless_path(program: &Path) -> std::ffi::OsString {
    session_path(program, crate::login_path::login_dirs().as_deref())
}
/// SB-455: a session script's `PATH` line (a launch script, a resume script): the program's own
/// folder, then the `PATH` the script was started with, then the rest of `headless_path` after it
/// (a folder already in that `PATH` may appear twice; lookup takes the first). The inherited `PATH` stays ahead of the login one, so a terminal's
/// version-manager shims are not shadowed by `/usr/bin`.
pub(crate) fn path_export(program: &Path) -> String {
    path_export_with(program, crate::login_path::login_dirs().as_deref())
}
/// `path_export` with the login folders given.
pub(crate) fn path_export_with(program: &Path, login: Option<&[PathBuf]>) -> String {
    use fabric_sessions_core::cli::quoted;
    let first = program.parent().filter(|d| !d.as_os_str().is_empty());
    let rest: Vec<PathBuf> = std::env::split_paths(&session_path(program, login))
        .filter(|d| Some(d.as_path()) != first)
        .collect();
    let rest = std::env::join_paths(rest).unwrap_or_default();
    let rest = rest.to_string_lossy();
    let mut line = String::from("export PATH=");
    match first {
        Some(first) => {
            line.push_str(&quoted(&first.to_string_lossy()));
            line.push_str("\"${PATH:+:$PATH}\"");
            if !rest.is_empty() {
                line.push_str(&quoted(&format!(":{rest}")));
            }
        }
        None if rest.is_empty() => line.push_str("\"${PATH:-}\""),
        None => {
            line.push_str("\"${PATH:+$PATH:}\"");
            line.push_str(&quoted(&rest));
        }
    }
    line.push('\n');
    line
}
/// `headless_path` with the login folders given.
pub(crate) fn session_path(program: &Path, login: Option<&[PathBuf]>) -> std::ffi::OsString {
    let extra: Vec<PathBuf> = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/bin"))
        .into_iter()
        .collect();
    crate::login_path::merge(program.parent(), login, std::env::var_os("PATH"), &extra)
}
/// SB-247: starts a dispatched run from the owner's own process: no Terminal, no PowerShell. It
/// leads its own process group (Unix) or a new process group with no window (Windows), reads
/// nothing on stdin and writes stdout and stderr to `launches/logs/<launch_id>.log` (0600). Its
/// marker (`.session-pid`, `.session-process`) is the one a Terminal session's script writes, so
/// the session list, the one-session-per-home rule and the launch record read it as they read any
/// session; then the home's reservation goes.
fn spawn_headless(
    root: &Path,
    home: &Path,
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    working_directory: &Path,
    launch_id: &str,
) -> Result<std::process::Child, String> {
    let logs = root.join(crate::launches::DIR).join(crate::launches::LOGS);
    private_dir(&root.join(crate::launches::DIR))?;
    private_dir(&logs)?;
    let log = switchboard_core::private_fs::create_new(&logs.join(format!("{launch_id}.log")))
        .map_err(|_| DISPATCH_FAILED)?;
    let err = log.try_clone().map_err(|_| DISPATCH_FAILED)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(working_directory)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err);
    for name in CONFLICTS {
        command.env_remove(name);
    }
    command.envs(env).env("PATH", headless_path(program));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW.
        command.creation_flags(0x0000_0200 | 0x0800_0000);
    }
    let mut child = command.spawn().map_err(|_| DISPATCH_FAILED.to_string())?;
    if let Err(error) = write_marker(home, &child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let _ = fs::remove_file(home.join(".launch-pending"));
    Ok(child)
}
#[cfg(unix)]
fn write_marker(home: &Path, child: &std::process::Child) -> Result<(), String> {
    private_write(
        &home.join(".session-pid"),
        child.id().to_string().as_bytes(),
        false,
    )
}
#[cfg(windows)]
fn write_marker(home: &Path, child: &std::process::Child) -> Result<(), String> {
    let created = switchboard_core::windows::process_created(child.id())?;
    private_write(
        &home.join(".session-process"),
        json!({"pid": child.id(), "created": created})
            .to_string()
            .as_bytes(),
        false,
    )
}
/// SB-247: a dispatched run, prepared exactly as a continuation launch is (the
/// account's isolated home, its tools, the Observatory server, the record), started by
/// `spawn_headless` instead of a Terminal script.
pub(crate) fn dispatch_session(
    root: &Path,
    store: &Arc<Store>,
    proxy: &ProxyHandle,
    id: &str,
    working_directory: &Path,
    continuation: &Continuation,
    headless: &Headless,
) -> Result<Dispatched, String> {
    launch_core(
        root,
        store,
        proxy,
        id,
        "isolated",
        working_directory,
        dispatch_host(),
        Some(continuation),
        &[],
        false,
        Handed::Caller,
        Some(headless),
    )?
    .2
    .ok_or_else(|| DISPATCH_FAILED.to_string())
}
#[cfg(test)]
tokio::task_local! {
    /// A test's stand-in for the provider CLI of a dispatched run. With it set the run's host is
    /// `SANDBOX`: nothing of the operator's own (tools, board, user folder) is read.
    pub(crate) static TEST_DISPATCH_PROGRAM: PathBuf;
}
#[cfg(test)]
fn sandbox_binary(_: Provider) -> Result<PathBuf, String> {
    TEST_DISPATCH_PROGRAM
        .try_with(PathBuf::clone)
        .map_err(|_| "Provider CLI not found. Install the official CLI and retry.".into())
}
#[cfg(test)]
fn sandbox_terminal(_: &Path) -> Result<(), String> {
    panic!("a dispatched run opened Terminal")
}
#[cfg(test)]
fn sandbox_nothing() -> Option<PathBuf> {
    None
}
#[cfg(test)]
fn sandbox_no_board(_: Provider) -> Option<McpServer> {
    None
}
#[cfg(test)]
fn sandbox_no_scope(_: Provider) -> Option<PathBuf> {
    None
}
#[cfg(test)]
const SANDBOX: Host = Host {
    binary: sandbox_binary,
    agent_cli: sandbox_nothing,
    board: sandbox_no_board,
    start_terminal: sandbox_terminal,
    user_scope: sandbox_no_scope,
    mcp_sign_ins: no_mcp_sign_ins,
};
fn dispatch_host() -> &'static Host {
    #[cfg(test)]
    return &SANDBOX;
    #[cfg(not(test))]
    &NATIVE
}
/// SB-247: the provider CLI a dispatched run starts: the one a launch uses. In a unit test only
/// the stand-in, never the CLI installed on this computer.
pub(crate) fn dispatch_program(provider: Provider) -> Result<PathBuf, String> {
    (dispatch_host().binary)(provider)
}
/// SB-247: stops the dispatched run `home` holds: its process group (SIGTERM, then SIGKILL after
/// `grace`) on Unix, `taskkill /T /F` on Windows. Ok once the home is idle; a home already idle
/// is Ok at once.
#[cfg(unix)]
pub(crate) fn stop_dispatched(home: &Path, grace: Duration) -> Result<(), String> {
    if !session_running(home)? {
        return Ok(());
    }
    let pid = read_regular(&home.join(".session-pid"))
        .ok()
        .and_then(|text| text.trim().parse::<libc::pid_t>().ok())
        .filter(|pid| *pid > 0)
        .ok_or(DISPATCH_WOULD_NOT_STOP)?;
    for signal in [libc::SIGTERM, libc::SIGKILL] {
        signal_session(pid, signal);
        if !running_within(home, grace) {
            return Ok(());
        }
    }
    Err(DISPATCH_WOULD_NOT_STOP.into())
}
#[cfg(windows)]
pub(crate) fn stop_dispatched(home: &Path, grace: Duration) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if !session_running(home)? {
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_str(&read_regular(&home.join(".session-process"))?)
            .map_err(|_| DISPATCH_WOULD_NOT_STOP)?;
    let pid = value["pid"]
        .as_u64()
        .and_then(|x| u32::try_from(x).ok())
        .filter(|x| *x > 0)
        .ok_or(DISPATCH_WOULD_NOT_STOP)?;
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let _ = Command::new(system.join("System32").join("taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if running_within(home, grace + grace) {
        Err(DISPATCH_WOULD_NOT_STOP.into())
    } else {
        Ok(())
    }
}
/// Whether `home` still runs its session after waiting up to `wait` for it to end.
fn running_within(home: &Path, wait: Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if !session_running(home).unwrap_or(true) {
            return false;
        }
        if std::time::Instant::now() >= deadline {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
/// SB-342 H-0: where Switchboard keeps the Terminal window settings of the scripts it opens: a
/// folder whose path holds no space (Caches), since Terminal splits a window's command on spaces.
#[cfg(target_os = "macos")]
fn terminal_cache() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Caches/ai.passioncode.fabric-switchboard/terminal"))
}
#[cfg(not(target_os = "macos"))]
fn terminal_cache() -> Option<PathBuf> {
    None
}
pub(crate) fn start_terminal(path: &Path) -> Result<(), String> {
    // SB-342 H-1b: the window runs the session through a session host and shows it attached, so
    // closing the window leaves it running and any app can open it; the plain script where no
    // host can be prepared.
    #[cfg(unix)]
    let hosted = hosted_wrapper(path);
    #[cfg(unix)]
    let path = hosted.as_deref().unwrap_or(path);
    fabric_sessions_core::terminal::open(path, terminal_cache().as_deref(), CONFLICTS)
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
            // Perf audit item 15: a home without a record is skipped by a stat, and an ended
            // session's record, launch record and host are read only for `all`.
            if fs::symlink_metadata(home.join(SESSION_RECORD)).is_err() {
                continue;
            }
            let (state, started) = home_state(&home);
            if !all && !matches!(state, "starting" | "running") {
                continue;
            }
            let Some(mut record) = read_regular(&home.join(SESSION_RECORD))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .filter(serde_json::Value::is_object)
            else {
                continue;
            };
            record["state"] = state.into();
            record["started_at"] = started.into();
            record["home"] = folder.into();
            // SB-197: the home itself, relative to the data folder, so a host tells two sessions
            // of one category apart and can open the folder; never a path outside it.
            record["home_dir"] = entry
                .file_name()
                .to_str()
                .map(|name| format!("{folder}/{name}"))
                .into();
            crate::launches::add_outcome(root, &mut record);
            crate::launches::add_host(root, &mut record);
            found.push(record);
        }
    }
    let listed: std::collections::HashSet<String> = found
        .iter()
        .filter_map(|r| r["launch_id"].as_str().map(str::to_owned))
        .collect();
    found.extend(crate::launches::homeless(
        root,
        all,
        &listed,
        crate::monitor::now(),
    ));
    found.sort_by_key(|r| std::cmp::Reverse(r["launched_at"].as_i64().unwrap_or(0)));
    found
}
/// A home's session state now (`starting`, `running`, `ended` or `unknown`, see `sessions`)
/// and when its process started, from its marker.
pub(crate) fn home_state(home: &Path) -> (&'static str, Option<i64>) {
    let pending = reservation_open(home);
    let state = if pending {
        "starting"
    } else {
        match session_running(home) {
            Ok(true) => "running",
            Ok(false) => "ended",
            Err(_) => "unknown",
        }
    };
    #[cfg(unix)]
    let marker = home.join(".session-pid");
    #[cfg(windows)]
    let marker = home.join(".session-process");
    let started = fs::symlink_metadata(&marker)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    (state, started)
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
                shared_accounts: Vec::new(),
                pool: "alpha".into(),
                name: "Alpha".into(),
                folders: vec!["/work/web".into(), "/work/api".into()],
                created_at: 1,
                sources: vec![],
                organizations: Vec::new(),
                ..Project::default()
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
        // SB-109: an organization exists and the personal group holds a seat of Acme's.
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
        // DEC-0022 (SB-403): an account of another organization runs there all the same.
        assert!(project_allows(&with_org, &with_org.accounts[0], Path::new("/work/web")).is_ok());
        with_org.projects[0].organizations = vec!["00000000-0000-4000-8000-000000000001".into()];
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
        // SB-260: with their own CLAUDE_CONFIG_DIR, Claude Code keeps user-scope servers in that
        // folder's `.claude.json`, and the board is read there, not from `~/.claude.json`.
        let own = temp.path().join("own-claude");
        fs::create_dir_all(&own).unwrap();
        fs::write(
            own.join(".claude.json"),
            json!({"mcpServers": {"fabric": {"type": "stdio", "command": BOARD, "args": ["--own"]}}})
                .to_string(),
        )
        .unwrap();
        let mine = user_board_server(Provider::Claude, &user, Some(&own)).unwrap();
        assert_eq!(mine.args, ["--own"]);
        assert!(
            user_board_server(Provider::Claude, &temp.path().join("nowhere"), Some(&own)).is_some(),
            "the home's `.claude.json` is not needed then"
        );
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
        let unset = s.lines().find(|l| l.starts_with("unset ")).unwrap();
        assert!(
            unset.split(' ').any(|w| w == "ANTHROPIC_CUSTOM_HEADERS"),
            "SB-533: a parent session's route header is cleared"
        );
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
    #[cfg(unix)]
    #[test]
    fn a_terminal_window_runs_its_session_through_a_host_when_one_can_be_prepared() {
        // SB-342 H-1b: the window's command starts the session in a host and attaches to it.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data with 'quote");
        let home = root.join("runtimes").join("claude-default");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(root.join("accounts.json"), b"{}").unwrap();
        let id = crate::launches::new_id();
        let script = home.join("launch.command");
        std::fs::write(
            &script,
            format!("#!/bin/zsh\nexport SWITCHBOARD_LAUNCH_ID='{id}'\nexec true\n"),
        )
        .unwrap();
        let body = attach_wrapper(Path::new("/opt/sb/switchboard"), &root, &id, &script);
        assert!(body.starts_with("#!/bin/sh\nexec '/opt/sb/switchboard' --data-dir '"));
        assert!(body.contains("'\\''quote"));
        assert!(body.contains(&format!("session-open --launch-id '{id}' --program '")));
        // A script that names no launch gets no host: the plain script opens.
        let plain = home.join("plain.command");
        std::fs::write(&plain, b"#!/bin/zsh\nexec true\n").unwrap();
        assert!(hosted_wrapper(&plain).is_none());
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
        // SB-354: a file this test just wrote can be "text file busy" (ETXTBSY, exit 126) while
        // another test's fork still holds its write descriptor, so the stand-in runs the script
        // again on 126 and leaves the last status where a failure can name it.
        let status = temp.path().join("terminal-status");
        private_write(
            &terminal,
            format!(
                "#!/bin/sh\n[ \"$1\" = -e ] || exit 9\nshift\ntries=0\nwhile :; do\n  \"$@\"\n  result=$?\n  printf '%s %s' \"$result\" \"$tries\" > '{}'\n  [ \"$result\" -eq 126 ] && [ \"$tries\" -lt 50 ] || exit \"$result\"\n  tries=$((tries + 1))\n  sleep 0.1\ndone\n",
                status.display()
            )
            .as_bytes(),
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
        fabric_sessions_core::terminal::spawn_linux_terminal(
            &path,
            Some(terminal.into_os_string()),
            |_| None,
            CONFLICTS,
        )
        .unwrap();
        // 60 s as the other process tests (SB-297): the v0.7.36 nightly's arm runner needed more
        // than 10; it still ends as soon as the script completes.
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !home.join(".completed").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the script did not complete; the stand-in terminal's last status and retries: {:?}",
                fs::read_to_string(&status).ok()
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
            fabric_sessions_core::terminal::spawn_linux_terminal(&path, None, |_| None, CONFLICTS)
                .unwrap_err(),
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
        hold.commit(Handed::Terminal); // Terminal was asked to run the script, which never ran.
        drop(hold);
        age(&root.path().join(".launch-pending"), 9 * 60);
        assert!(
            ensure_idle(root.path()).is_err(),
            "a fresh launch still holds"
        );
        age(&root.path().join(".launch-pending"), 11 * 60);
        assert!(ensure_idle(root.path()).is_ok());
        // SB-448: a check only reads; the next reservation clears the stale marker.
        assert!(root.path().join(".launch-pending").exists());
        assert!(Reservation::new(root.path()).is_ok());
    }
    /// SB-448: takes a reservation on `home`, as the next launch does, and answers the reasons
    /// of the stale markers it cleared; the reservation is dropped, so no marker is left.
    fn reserve_clearing(home: &Path) -> Vec<&'static str> {
        let (hold, events) = crate::oplog::capture_fields(|| Reservation::new(home));
        drop(hold.expect("the next launch proceeds"));
        assert!(!home.join(".launch-pending").exists());
        events
            .into_iter()
            .filter(|(event, _)| *event == "launch_reservation")
            .flat_map(|(_, fields)| fields)
            .filter_map(|(name, field)| match (name, field) {
                ("reason", crate::oplog::Field::Code(code)) => Some(code),
                _ => None,
            })
            .collect()
    }
    /// SB-440: the marker's contents as `ensure_idle` reads them, with its time set back.
    fn marker(home: &Path, contents: &str, seconds: u64) {
        let path = home.join(".launch-pending");
        private_write(&path, contents.as_bytes(), false).unwrap();
        age(&path, seconds);
    }
    #[test]
    fn sb440_a_marker_no_caller_took_frees_the_home_after_five_minutes() {
        // The 2026-11-18 incident: the owner prepared an in-place session, the CLI's control
        // request had already timed out, and nothing ran the script.
        let root = tempfile::tempdir().unwrap();
        let unclaimed = r#"{"pid":null,"by":"caller"}"#;
        marker(root.path(), unclaimed, 4 * 60);
        assert!(
            reservation_open(root.path()),
            "a caller still has time to run it"
        );
        assert!(ensure_idle(root.path()).is_err());
        marker(root.path(), unclaimed, 6 * 60);
        assert!(!reservation_open(root.path()));
        assert_eq!(home_state(root.path()).0, "ended");
        let (idle, events) = crate::oplog::capture_fields(|| ensure_idle(root.path()));
        idle.unwrap();
        assert!(events.is_empty(), "a check clears nothing: {events:?}");
        assert!(root.path().join(".launch-pending").exists());
        assert_eq!(reserve_clearing(root.path()), ["unclaimed"]);
    }
    #[test]
    fn sb440_a_marker_handed_to_terminal_still_holds_for_ten_minutes() {
        let root = tempfile::tempdir().unwrap();
        marker(root.path(), r#"{"pid":null,"by":"terminal"}"#, 6 * 60);
        assert!(
            ensure_idle(root.path()).is_err(),
            "Terminal may still be opening"
        );
        marker(root.path(), "", 6 * 60);
        assert!(
            ensure_idle(root.path()).is_err(),
            "an earlier version's marker"
        );
    }
    #[cfg(unix)]
    #[test]
    fn sb440_a_marker_whose_preparing_process_is_gone_is_cleared_at_once() {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let gone = child.id();
        child.wait().unwrap();
        marker(root.path(), &format!(r#"{{"pid":{gone}}}"#), 0);
        assert!(ensure_idle(root.path()).is_ok());
        assert_eq!(reserve_clearing(root.path()), ["launcher_gone"]);
        // A live preparing process (this one) still holds the home.
        marker(
            root.path(),
            &format!(r#"{{"pid":{}}}"#, std::process::id()),
            0,
        );
        assert!(ensure_idle(root.path()).is_err());
        assert!(root.path().join(".launch-pending").exists());
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn sb440_a_preparer_is_told_by_its_start_token_not_its_pid_alone() {
        // Review round 1: a pid reused by another process frees the home on every platform.
        let root = tempfile::tempdir().unwrap();
        let me = std::process::id();
        let token = crate::copies::process_token(me).expect("this process's start token");
        marker(
            root.path(),
            &json!({"pid": me, "token": token}).to_string(),
            0,
        );
        assert!(ensure_idle(root.path()).is_err(), "the same process holds");
        marker(
            root.path(),
            &json!({"pid": me, "token": token + 1}).to_string(),
            0,
        );
        assert!(ensure_idle(root.path()).is_ok());
        assert_eq!(reserve_clearing(root.path()), ["launcher_gone"]);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn sb445_a_linux_process_started_when_it_was_spawned() {
        let before = SystemTime::now();
        let mut child = Command::new("/bin/sleep").arg("5").spawn().unwrap();
        let started = process_started(child.id());
        let _ = child.kill();
        let _ = child.wait();
        let started = started.expect("the start of a live child");
        let slack = Duration::from_secs(3);
        assert!(started + slack >= before && started <= SystemTime::now() + slack);
    }
    #[test]
    fn sb440_clearing_a_stale_marker_never_removes_a_fresh_one() {
        // Review round 1: between judging a marker stale and clearing it, another launch may have
        // cleared it and written its own; that one stays and keeps the home.
        let root = tempfile::tempdir().unwrap();
        let fresh = json!({"pid": null, "by": "caller"}).to_string();
        marker(root.path(), &fresh, 0);
        assert_eq!(
            clear_stale(root.path(), SystemTime::now()).unwrap_err(),
            HOME_BUSY
        );
        assert_eq!(
            fs::read_to_string(root.path().join(".launch-pending")).unwrap(),
            fresh
        );
        marker(root.path(), &fresh, 6 * 60);
        clear_stale(root.path(), SystemTime::now()).unwrap();
        assert!(!root.path().join(".launch-pending").exists());
        // Nothing is left aside either way, and a marker already gone is not an error.
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        clear_stale(root.path(), SystemTime::now()).unwrap();
    }
    #[test]
    fn sb448_a_marker_that_cannot_be_linked_back_is_moved_back_never_deleted() {
        let root = tempfile::tempdir().unwrap();
        let fresh = json!({"pid": null, "by": "caller"}).to_string();
        marker(root.path(), &fresh, 0);
        let refused = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        };
        assert_eq!(
            clear_stale_with(root.path(), SystemTime::now(), refused).unwrap_err(),
            HOME_BUSY
        );
        assert_eq!(
            fs::read_to_string(root.path().join(".launch-pending")).unwrap(),
            fresh
        );
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            1,
            "nothing left aside"
        );
        // Another marker took the name meanwhile: it stays, and the copy aside goes.
        let taken = |_: &Path, to: &Path| -> std::io::Result<()> {
            fs::write(to, "other")?;
            Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
        };
        assert!(clear_stale_with(root.path(), SystemTime::now(), taken).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join(".launch-pending")).unwrap(),
            "other"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn sb448_read_only_checks_never_remove_a_marker() {
        let f = tempfile::tempdir().unwrap();
        let home = f.path().join("runtimes").join("claude-tokens");
        private_dir(&f.path().join("runtimes")).unwrap();
        private_dir(&home).unwrap();
        marker(&home, r#"{"pid":null,"by":"caller"}"#, 6 * 60);
        // Stale: nothing will run its script, so no session runs, and the marker stays.
        assert!(!launched_session_running(f.path(), Provider::Claude, &[]));
        assert!(!reservation_open(&home));
        assert_eq!(home_state(&home).0, "ended");
        assert!(ensure_idle(&home).is_ok());
        assert!(home.join(".launch-pending").exists());
        // A marker that holds is a running launch.
        marker(&home, r#"{"pid":null,"by":"caller"}"#, 0);
        assert!(launched_session_running(f.path(), Provider::Claude, &[]));
        assert_eq!(home_state(&home).0, "starting");
    }
    #[cfg(unix)]
    #[test]
    fn sb449_a_marker_that_is_not_a_regular_file_holds_everywhere() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".launch-pending")).unwrap();
        assert!(ensure_idle(root.path()).is_err());
        assert!(reservation_open(root.path()));
        assert_eq!(home_state(root.path()).0, "starting");
        assert!(Reservation::new(root.path()).is_err());
        assert!(root.path().join(".launch-pending").is_dir());
    }
    #[test]
    fn sb449_an_in_place_continue_is_the_callers_unless_queued() {
        assert_eq!(Handed::for_continue(false), Handed::Caller);
        assert_eq!(Handed::for_continue(true), Handed::Terminal);
    }
    #[test]
    fn sb440_a_reservation_names_its_preparer_then_who_runs_the_script() {
        let root = tempfile::tempdir().unwrap();
        let read = || -> Value {
            serde_json::from_slice(&fs::read(root.path().join(".launch-pending")).unwrap()).unwrap()
        };
        let mut hold = Reservation::new(root.path()).unwrap();
        let me = std::process::id();
        assert_eq!(
            read(),
            json!({"pid": me, "token": crate::copies::process_token(me)})
        );
        hold.commit(Handed::Caller);
        assert_eq!(read(), json!({"pid": null, "by": "caller"}));
        drop(hold);
        assert!(
            root.path().join(".launch-pending").exists(),
            "the script removes it"
        );
        // A script that already started removed the marker; committing never writes it again.
        fs::remove_file(root.path().join(".launch-pending")).unwrap();
        let mut hold = Reservation::new(root.path()).unwrap();
        fs::remove_file(root.path().join(".launch-pending")).unwrap();
        hold.commit(Handed::Terminal);
        assert!(!root.path().join(".launch-pending").exists());
    }
    #[test]
    fn sign_in_whose_terminal_never_ran_can_be_cancelled_once_its_reservation_expires() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("login");
        private_dir(&home).unwrap();
        let mut hold = Reservation::new(&home).unwrap();
        hold.commit(Handed::Terminal);
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
        let mut command = Command::new("/bin/sh");
        command.arg(&script).process_group(0);
        // SB-221: a Terminal window starts its shell with the hang-up at its default. A test run
        // under `nohup` (or any runner that ignores SIGHUP) hands the ignore down, a
        // non-interactive shell cannot undo it, and only the terminate would end the session.
        // SAFETY: `signal` is async-signal-safe and the closure touches nothing else.
        unsafe {
            command.pre_exec(|| {
                libc::signal(libc::SIGHUP, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
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
        cancel_login(&login).unwrap();
        // S2 (SB-221): the hang-up, not the terminate after it, ended the session. Read from how
        // the script exited, not from the wall clock: `cancel_login` asks `ps` and waits for the
        // home to turn idle, and on a loaded machine that alone passed END_WAIT.
        use std::os::unix::process::ExitStatusExt;
        let status = exited.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(
            status.signal(),
            Some(libc::SIGHUP),
            "closing the window's signal ends it: {status:?}"
        );
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
    #[cfg(unix)]
    #[test]
    fn liveness_and_age_are_read_without_starting_a_process() {
        // SB-302: `sessions` asks both once per live session.
        let me = std::process::id();
        assert!(super::pid_alive(me));
        let age = super::process_age(me).expect("this process has an age");
        assert!(age < std::time::Duration::from_secs(24 * 3600), "{age:?}");
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(
            super::process_started(me).is_some(),
            "read in-process, not through ps"
        );
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let gone = child.id();
        child.wait().unwrap();
        assert!(!super::pid_alive(gone), "a reaped child is not alive");
        assert!(
            !super::pid_alive(u32::MAX),
            "a pid out of range is not alive"
        );
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
        mcp_sign_ins: no_mcp_sign_ins,
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
            text.contains("'agents' 'key' '--service' 'openrouter' '--machine'"),
            "the key slot the gate read, the machine's here (SB-144, SB-213)"
        );
        assert!(
            !text.contains("'--dir'"),
            "never the folder's key at run time"
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
        // personal outside a group; in an organization's folder, the group's (DEC-0022).
        let folder = base.join("wf_00112233445566aa");
        assert_eq!(recorded(&folder)["context"], "personal");
        assert!(text.contains("export SWITCHBOARD_CONTEXT='personal'"));
        let acme = acme_group(&f, &[]);
        session(&f).unwrap();
        // DEC-0022 (SB-403 round 1): the group owns the data, whatever key pays.
        assert_eq!(recorded(&folder)["context"], format!("org:{acme}"));
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
            text.contains("'agents' 'key' '--service' 'openrouter' '--machine'"),
            "the key slot the gate read, the machine's here (SB-144, SB-213)"
        );
        assert!(
            !text.contains("'--dir'"),
            "never the folder's key at run time"
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
        // personal outside a group; in an organization's folder, the group's (DEC-0022).
        let folder = base.join("wf_0123456789abcdef");
        assert_eq!(recorded(&folder)["context"], "personal");
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("export SWITCHBOARD_CONTEXT='personal'"));
        let acme = acme_group(&f, &[]);
        session(&f).unwrap();
        // DEC-0022 (SB-403 round 1): the group owns the data, whatever key pays.
        assert_eq!(recorded(&folder)["context"], format!("org:{acme}"));
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
                text.contains("'agents' 'key' '--service' 'openrouter' '--machine'"),
                "the key slot the gate read, the machine's here (SB-144, SB-213)"
            );
            assert!(
                !text.contains("'--dir'"),
                "never the folder's key at run time"
            );
            assert!(text.contains("export OPENROUTER_API_KEY\n"));
            assert!(text.contains(
                "exec '/opt/Agent Tools/agent' '--provider' 'openrouter' '-m' 'moonshotai/kimi-k2'"
            ));
            assert!(text.contains("unset ANTHROPIC_API_KEY"));
        }
    }
    /// SB-213, fix cycle 1, F1: the script names the key slot its start was gated on, so a folder
    /// that leaves its group between the launch and the script's first line still runs on that
    /// group's key: the key operation the script sends answers the slot, never the folder's key.
    #[cfg(not(windows))]
    #[tokio::test]
    async fn an_openrouter_script_fetches_the_slot_its_gate_read_after_its_folder_moves() {
        let f = fixture();
        openrouter_key(&f, Some("moonshotai/kimi-k2"));
        let other = f.temp.path().join("other");
        fs::create_dir(&other).unwrap();
        // As a launch resolves its folder.
        let (project, other) = (
            f.project.canonicalize().unwrap(),
            other.canonicalize().unwrap(),
        );
        let beta = f
            .store
            .save_project(None, "Beta", &[project.clone(), other.clone()], &[])
            .unwrap();
        f.store
            .set_agent_key_for(
                "openrouter",
                Some(&beta.pool),
                "sk-or-v1-synthetic-beta-key",
                Some("moonshotai/kimi-k2".into()),
            )
            .unwrap();
        let (path, answer) = openrouter_launch(&f, "hermes", None).unwrap();
        assert_eq!(answer["key_slot"], json!({ "group": beta.pool }));
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains(&format!(
                "'agents' 'key' '--service' 'openrouter' '--pool' {}",
                quote(&beta.pool)
            )),
            "the group's key by its slot"
        );
        assert!(
            !text.contains("'--dir'"),
            "never the folder's key at run time"
        );
        // After the launch the folder leaves Beta: by folder it would now get the machine's key.
        f.store
            .save_project(Some(&beta.pool), "Beta", &[other], &[])
            .unwrap();
        assert_eq!(
            f.store
                .agent_key_value_in("openrouter", Some(&project))
                .unwrap(),
            "sk-or-v1-synthetic-must-not-reach-disk"
        );
        // The operation the script's `agents key --service openrouter --pool <beta>` sends.
        let operation: crate::Operation = serde_json::from_value(json!({
            "operation": "agent_key_value",
            "service": "openrouter",
            "slot": { "group": beta.pool },
        }))
        .unwrap();
        let fetched = crate::execute(f.store.clone(), &f.root, None, operation)
            .await
            .unwrap();
        assert_eq!(fetched["key"], "sk-or-v1-synthetic-beta-key");
        // A slot whose key is gone answers that no key is saved, never another key.
        f.store
            .remove_agent_key_for("openrouter", Some(&beta.pool))
            .unwrap();
        let operation: crate::Operation = serde_json::from_value(json!({
            "operation": "agent_key_value",
            "service": "openrouter",
            "slot": { "group": beta.pool },
        }))
        .unwrap();
        assert_eq!(
            crate::execute(f.store.clone(), &f.root, None, operation)
                .await
                .unwrap_err(),
            switchboard_core::agent_keys::NO_AGENT_KEY
        );
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
        assert_eq!(goose["home_dir"], "runtimes/agent-goose-openrouter");
        // An agent that picks its model itself has no model to show.
        openrouter_launch(&f, "crush", None).unwrap();
        assert!(recorded(&f.root.join("runtimes/agent-crush-openrouter"))["label"].is_null());
        // In an organization's folder: the group's context on the machine's key and on the
        // group's own key alike (DEC-0022, SB-403 round 1).
        let acme = acme_group(&f, &[]);
        openrouter_launch(&f, "goose", None).unwrap();
        assert_eq!(recorded(&home)["context"], format!("org:{acme}"));
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
        #[cfg(windows)]
        let _ = path;
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
            None,
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
            None,
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
            None,
            goose_installed,
            terminal_opens,
        )
        .unwrap();
        let record = recorded(&f.root.join("runtimes/agent-goose-acme"));
        assert_eq!(record["context"], format!("org:{acme}"));
        assert_eq!(record["pool"], "acme");
    }
    /// SB-404 K3: Hermes on the pay-per-token group goes through the proxy, in a home of its own
    /// whose provider points at the group's route with the agents' capability, never a key; it
    /// needs a model. Claude Code takes a model as `--model`; another agent refuses one.
    #[tokio::test]
    async fn hermes_on_the_pay_per_token_group_runs_through_the_proxy() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let credential =
            Credential::parse(Provider::Claude, AuthKind::ApiKey, "synthetic-tokens-key").unwrap();
        let key = f
            .store
            .add_routed(
                "Team key".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                credential,
            )
            .unwrap();
        assert_eq!(key.pool, "tokens");
        let folder = f.project.canonicalize().unwrap();
        f.store
            .save_project(
                Some("tokens"),
                "Pay-per-token keys",
                std::slice::from_ref(&folder),
                std::slice::from_ref(&key.id),
            )
            .unwrap();
        f.store.select(Provider::Claude, "tokens", &key.id).unwrap();
        crate::spend::set_ceiling(
            &f.root,
            &f.store.snapshot().unwrap(),
            switchboard_core::ChainScope::Project {
                pool: "tokens".into(),
            },
            Some(5.0),
            crate::spend::Surface::Window,
            1,
        )
        .unwrap();
        f.store.set_group_off("tokens", false).unwrap();
        let hermes = |model: Option<&str>| {
            launch_agent_with(
                &f.root,
                &f.store,
                &proxy,
                "hermes",
                "tokens",
                &f.project,
                model,
                goose_installed,
                terminal_opens,
            )
        };
        assert_eq!(hermes(None).unwrap_err(), HERMES_TOKENS_MODEL_NEEDED);
        assert_eq!(
            hermes(Some("anthropic/claude-sonnet-5.5")).unwrap_err(),
            crate::anthropic_keys::MODEL_INVALID
        );
        let answer = hermes(Some("claude-fable-5-2")).unwrap();
        assert_eq!(answer["metered"], true);
        assert_eq!(answer["model"], "claude-fable-5-2");
        let home = f.root.join("runtimes/agent-hermes-tokens");
        let config = fs::read_to_string(home.join("agent-home/config.yaml")).unwrap();
        assert!(config.contains(&format!(
            "api: \"http://{}/claude/tokens\"",
            proxy.address()
        )));
        assert!(config.contains("key_env: SWITCHBOARD_AGENT_KEY"));
        assert!(config.contains("transport: anthropic_messages"));
        assert!(config.contains("adopt_external_logins: false"));
        // Review round 1, P2-10: side tasks pinned to the metered provider, no fallback.
        assert!(config.contains("fallback_providers: []"));
        assert!(config.contains(
            "  title_generation:\n    provider: switchboard\n    model: \"claude-fable-5-2\""
        ));
        assert!(config.contains("  compression:\n    provider: switchboard"));
        assert!(!config.contains("synthetic-tokens-key"));
        let script = fs::read_to_string(home.join(if cfg!(windows) {
            "launch.ps1"
        } else {
            "launch.command"
        }))
        .unwrap();
        assert!(script.contains("--provider") && script.contains("switchboard"));
        assert!(script.contains("claude-fable-5-2"));
        assert!(script.contains("HERMES_HOME"));
        // Provider keys in the caller's environment are cleared before Hermes starts.
        #[cfg(not(windows))]
        assert!(script.contains("unset OPENROUTER_API_KEY NOUS_API_KEY"));
        #[cfg(not(windows))]
        for name in [
            "AWS_SECRET_ACCESS_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "MOA_API_KEY",
        ] {
            assert!(script.contains(&format!(" {name} ")), "{name}");
        }
        // Its own `.env` names every provider key empty, so the install folder's `.env` fills
        // none of them.
        let dotenv = fs::read_to_string(home.join("agent-home/.env")).unwrap();
        for name in HERMES_PROVIDER_KEYS {
            assert!(dotenv.lines().any(|l| l == format!("{name}=")), "{name}");
        }
        assert_eq!(dotenv.lines().count(), HERMES_PROVIDER_KEYS.len());
        assert!(!script.contains("synthetic-tokens-key"));
        let record = recorded(&home);
        assert_eq!(record["pool"], "tokens");
        assert_eq!(record["mode"], "managed");
        assert_eq!(record["label"], "claude-fable-5-2");
        // Claude Code takes the model as its own flag; an agent that picks its model refuses one.
        launch_agent_with(
            &f.root,
            &f.store,
            &proxy,
            "claude-code",
            "tokens",
            &f.project,
            Some("claude-fable-5-2"),
            goose_installed,
            terminal_opens,
        )
        .unwrap();
        let script = fs::read_to_string(f.root.join(if cfg!(windows) {
            "runtimes/agent-claude-code-tokens/launch.ps1"
        } else {
            "runtimes/agent-claude-code-tokens/launch.command"
        }))
        .unwrap();
        assert!(script.contains("--model") && script.contains("claude-fable-5-2"));
        assert_eq!(
            launch_agent_with(
                &f.root,
                &f.store,
                &proxy,
                "goose",
                "tokens",
                &f.project,
                Some("claude-fable-5-2"),
                goose_installed,
                terminal_opens,
            )
            .unwrap_err(),
            MODEL_NOT_TAKEN
        );
        // A group switched off launches nothing on its key.
        f.store.set_group_off("tokens", true).unwrap();
        assert_eq!(
            hermes(Some("claude-fable-5-2")).unwrap_err(),
            switchboard_core::GROUP_OFF
        );
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
    #[test]
    fn a_delegated_seat_launches_only_in_managed_mode() {
        // SB-403: an isolated home would hold a guest's sign-in outside its group and lease.
        let root = tempfile::tempdir().unwrap();
        let account = switchboard_core::Account {
            id: "0b1edeee-4125-403c-9c12-44c6aadaeb2b".into(),
            label: "seat".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: "org".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        };
        let mut snapshot = switchboard_core::Snapshot {
            accounts: vec![account.clone()],
            ..Default::default()
        };
        assert!(group_allows_launch(root.path(), &snapshot, &account, "isolated").is_ok());
        snapshot.delegated.insert(
            account.id.clone(),
            switchboard_core::Delegation {
                sender_key: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=".into(),
                from: None,
                pool: "org".into(),
                granted_at: 1,
                lease_until: 2,
                ended_at: None,
                purged_at: None,
            },
        );
        assert_eq!(
            group_allows_launch(root.path(), &snapshot, &account, "isolated").unwrap_err(),
            DELEGATED_ISOLATED
        );
        assert!(group_allows_launch(root.path(), &snapshot, &account, "managed").is_ok());
    }
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
        // SB-401: a Claude API key never lands in a session's own home: managed only. Once the
        // pay-per-token group exists (the owner makes it at start), one from before, outside
        // it, launches in no mode.
        assert_eq!(
            f.launch(&proxy, &api, "isolated", &TEST_HOST).unwrap_err(),
            TOKENS_ISOLATED
        );
        assert!(f.store.ensure_tokens_group_for_keys().unwrap());
        for mode in ["isolated", "managed"] {
            assert_eq!(
                f.launch(&proxy, &api, mode, &TEST_HOST).unwrap_err(),
                KEY_OUTSIDE_TOKENS,
                "{mode}"
            );
        }
        assert!(!f
            .root
            .join("homes")
            .join(&api)
            .join("settings.json")
            .exists());

        let home = f.root.join("homes").join(&oauth);
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            settings,
            json!({"env": {"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-access"}})
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
        assert_eq!(events.iter().filter(|e| e.action == "launch").count(), 1);
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
            mcp_sign_ins: no_mcp_sign_ins,
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
    async fn with_own_sign_in_off_a_launch_carries_no_mcp_sign_in() {
        // SB-403 S3: the person's MCP sign-ins live in the ordinary Claude home's credential
        // item; with their own sign-in off limits, a launch never reads it.
        fn must_not_carry(_: &Path, _: &Path) {
            panic!("a launch read the ordinary home's MCP sign-ins");
        }
        fn terminal_must_not_open(_: &Path) -> Result<(), String> {
            panic!("an in-place launch opened Terminal");
        }
        let f = fixture();
        crate::own_sign_in::set(&f.root, false).unwrap();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let host = Host {
            binary: synthetic_program,
            agent_cli: synthetic_agent_cli,
            board: no_board,
            start_terminal: terminal_must_not_open,
            user_scope: no_user_scope,
            mcp_sign_ins: must_not_carry,
        };
        let (_, script) = launch_full(
            &f.root,
            &f.store,
            &proxy,
            &id,
            "isolated",
            &f.project,
            &host,
            None,
            &[],
            true,
        )
        .unwrap();
        assert!(script.is_some());
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
    /// SB-405: a prepared session as `script` wrote it, with the provider's bypass, and the same
    /// script rewritten for a hand-over taken with `--ask`.
    #[cfg(unix)]
    fn prepared(args: &[&str]) -> String {
        let env = BTreeMap::from([(
            crate::launches::ENV.to_string(),
            "6f1c2d3e-0000-4000-8000-000000000001".to_string(),
        )]);
        script(
            Path::new("/data/runtimes/claude-default"),
            Path::new("/opt/Provider Tools/o'cli"),
            args,
            &env,
            false,
            Path::new("/work/it's here"),
        )
    }
    #[cfg(unix)]
    #[test]
    fn a_hand_over_taken_with_ask_runs_claude_code_in_manual_mode_without_the_bypass() {
        let first = "Continue the workflow.\nIt's line two.";
        let before = prepared(&[
            "--mcp-config=/data/runtimes/claude-default/switchboard-mcp.json",
            "--dangerously-skip-permissions",
            first,
        ]);
        let after = ask_script_text(&before, "claude-code").unwrap();
        let exec = before.find("\nexec ").unwrap() + 1;
        assert_eq!(
            after[..exec],
            before[..exec],
            "everything before the command is kept"
        );
        assert!(!after.contains("dangerously"), "{after}");
        let words = shell_words(after[exec + 5..].strip_suffix('\n').unwrap()).unwrap();
        assert_eq!(
            words,
            [
                "/opt/Provider Tools/o'cli",
                "--permission-mode",
                "manual",
                "--mcp-config=/data/runtimes/claude-default/switchboard-mcp.json",
                first,
            ],
            "the mode comes first, the first message (line breaks and all) stays last"
        );
        // The person's own `ask` left no bypass: the mode is still named, since the settings the
        // home carries may default to bypassing. A mode already named becomes manual.
        let own = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            ask_args("claude-code", &own(&["--resume", "s-1"])).unwrap(),
            own(&["--permission-mode", "manual", "--resume", "s-1"])
        );
        assert_eq!(
            ask_args("claude-code", &own(&["--permission-mode", "plan", "x"])).unwrap(),
            own(&["--permission-mode", "manual", "x"])
        );
        assert_eq!(
            ask_args(
                "claude-code",
                &own(&[
                    "--allow-dangerously-skip-permissions",
                    "--permission-mode=bypassPermissions"
                ])
            )
            .unwrap(),
            own(&["--permission-mode=manual"])
        );
    }
    #[cfg(unix)]
    #[test]
    fn a_hand_over_taken_with_ask_runs_codex_asking_and_keeps_a_sandbox_the_person_set() {
        let before = prepared(&[
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "sandbox_mode=\"danger-full-access\"",
            "resume",
            "--last",
        ]);
        let after = ask_script_text(&before, "codex").unwrap();
        let exec = after.find("\nexec ").unwrap() + 1;
        let words = shell_words(after[exec + 5..].strip_suffix('\n').unwrap()).unwrap();
        assert_eq!(
            words[1..],
            ["-c", "approval_policy=\"on-request\"", "resume", "--last"],
            "Switchboard's overrides go, asking comes before the subcommand"
        );
        let own = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            ask_args(
                "codex",
                &own(&[
                    "--dangerously-bypass-approvals-and-sandbox",
                    "-s",
                    "read-only",
                    "x"
                ])
            )
            .unwrap(),
            own(&[
                "-c",
                "approval_policy=\"on-request\"",
                "-s",
                "read-only",
                "x"
            ]),
            "a read-only sandbox stays; none is added"
        );
        assert_eq!(
            ask_args("codex", &own(&["-a", "never", "--yolo"])).unwrap(),
            own(&["-a", "on-request"])
        );
        assert_eq!(
            ask_args("codex", &own(&["-c", "sandbox_mode=\"read-only\""])).unwrap(),
            own(&[
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "sandbox_mode=\"read-only\""
            ])
        );
    }
    #[cfg(unix)]
    #[test]
    fn ask_is_refused_for_another_agent_or_a_script_switchboard_did_not_write() {
        let before = prepared(&["--dangerously-skip-permissions"]);
        for agent in ["hermes", "kimi-code", "opencode", "unknown"] {
            assert_eq!(ask_script_text(&before, agent).unwrap_err(), ASK_AGENT);
        }
        let sign_in = script(
            Path::new("/data/homes/a"),
            Path::new("/bin/claude"),
            &["auth", "login"],
            &BTreeMap::new(),
            true,
            Path::new("/data/homes/a"),
        );
        assert_eq!(
            ask_script_text(&sign_in, "claude-code").unwrap_err(),
            ASK_UNREADABLE
        );
        assert_eq!(
            ask_script_text("#!/bin/sh\nexec claude\n", "claude-code").unwrap_err(),
            ASK_UNREADABLE
        );
        let unquoted = before.replace("'--dangerously-skip-permissions'", "--x");
        assert_eq!(
            ask_script_text(&unquoted, "claude-code").unwrap_err(),
            ASK_UNREADABLE
        );
        for words in [
            vec!["a b", "it's", "", "line\nbreak", "'"],
            vec!["--dangerously-skip-permissions"],
        ] {
            let line = words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ");
            assert_eq!(shell_words(&line).unwrap(), words);
        }
        for bad in ["", "'a' ", " 'a'", "'a'  'b'", "'a", "a"] {
            assert!(shell_words(bad).is_none(), "{bad:?}");
        }
    }
    /// SB-298: a session Switchboard starts bypasses its permission prompts unless the person
    /// chose `ask` or the caller names its own permission mode.
    #[test]
    fn a_started_session_bypasses_permissions_unless_asked_or_named() {
        let root = tempfile::tempdir().unwrap();
        let none: Vec<String> = Vec::new();
        assert_eq!(
            bypass_args(root.path(), Provider::Claude, &none),
            ["--dangerously-skip-permissions"]
        );
        assert_eq!(
            bypass_args(root.path(), Provider::Codex, &none),
            [
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_mode=\"danger-full-access\""
            ]
        );
        let own = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for named in [
            own(&["--permission-mode", "plan"]),
            own(&["--permission-mode=plan"]),
        ] {
            assert!(bypass_args(root.path(), Provider::Claude, &named).is_empty());
        }
        for named in [
            own(&["-a", "on-request"]),
            own(&["-c", "approval_policy=\"on-request\""]),
        ] {
            assert!(bypass_args(root.path(), Provider::Codex, &named).is_empty());
        }
        crate::agent_settings::set_permissions(root.path(), crate::agent_settings::ASK).unwrap();
        assert!(bypass_args(root.path(), Provider::Claude, &none).is_empty());
        crate::agent_settings::set_permissions(root.path(), crate::agent_settings::BYPASS).unwrap();
        assert!(!bypass_args(root.path(), Provider::Claude, &none).is_empty());
        assert_eq!(
            crate::agent_settings::set_permissions(root.path(), "never").unwrap_err(),
            crate::agent_settings::PERMISSIONS_INVALID
        );
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
            mcp_sign_ins: no_mcp_sign_ins,
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
        let mut expected = vec![
            format!("--mcp-config={}", ours.to_string_lossy()),
            "--dangerously-skip-permissions".to_string(),
        ];
        expected.extend(sent.iter().cloned());
        // SB-452: the home's own route, last (Claude Code 2.1.327 takes it after the prompt too).
        expected.push(route_arg(ours.parent().unwrap()));
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
        // SB-298: Codex's bypass as `-c` overrides, which hold before `resume`.
        let mut expected: Vec<String> = bypass_args(&f.root, Provider::Codex, &[])
            .into_iter()
            .map(str::to_owned)
            .collect();
        expected.extend(sent.iter().cloned());
        assert_eq!(
            args, expected,
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
            mcp_sign_ins: no_mcp_sign_ins,
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
            &f.root,
            &f.store,
            &proxy,
            &id,
            "isolated",
            &f.project,
            &host,
            &resumed,
            Handed::Terminal,
        )
        .unwrap();
        assert_eq!(
            script,
            f.root.join("homes").join(&id).join("launch.command")
        );
        let text = fs::read_to_string(&script).unwrap();
        assert!(text.contains("export OBSERVATORY_WORKFLOW_ID='wf_0123456789abcdef'\n"));
        assert!(text.contains(&quote(&crate::continuation::prompt(&resumed))));
        // SB-440: prepared for the hand-over queue (`Handed::Terminal`), the script may wait for
        // a console, so its home holds for ten minutes, not `UNCLAIMED_TTL`.
        let marker: Value = serde_json::from_slice(
            &fs::read(f.root.join("homes").join(&id).join(".launch-pending")).unwrap(),
        )
        .unwrap();
        assert_eq!(marker, json!({"pid": null, "by": "terminal"}));
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
            mcp_sign_ins: no_mcp_sign_ins,
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
            &f.root,
            &f.store,
            &proxy,
            &id,
            "isolated",
            &f.project,
            &host,
            &resumed,
            Handed::Terminal,
        )
        .unwrap();
        let home = f.root.join("homes").join(&id);
        assert_eq!(
            fs::read_to_string(home.join("CLAUDE.md")).unwrap(),
            "operator rules"
        );
        // SB-259: the session names the folder its home was built from.
        let script = fs::read_to_string(home.join("launch.command")).unwrap();
        assert!(
            script.contains(&format!(
                "export {SOURCE_DIR}={}",
                quote(&user.path().to_string_lossy())
            )),
            "{script}"
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
        // SB-247: a Codex home links the person's instructions and hooks (fabric-agent-adapter's
        // hook lives in `hooks.json`); its sign-in and config are Switchboard's own.
        fs::write(user.path().join("AGENTS.md"), "codex rules").unwrap();
        fs::write(user.path().join("hooks.json"), r#"{"hooks":{}}"#).unwrap();
        fs::write(user.path().join("auth.json"), "operator sign-in").unwrap();
        // The person trusts their own hook; Codex runs it only where config.toml says so.
        let theirs = user.path().join("hooks.json");
        let mut person = toml::Table::new();
        person.insert("model".into(), "never-copied".into());
        let mut state = toml::Table::new();
        let entry = |hash: &str| {
            let mut e = toml::Table::new();
            e.insert("trusted_hash".into(), hash.into());
            e.insert("enabled".into(), true.into());
            e.insert("note".into(), "never-copied".into());
            toml::Value::Table(e)
        };
        state.insert(
            format!("{}:session_start:0:0", theirs.display()),
            entry("sha256:aa"),
        );
        state.insert(
            "plugin@market:hooks/hooks.json:stop:0:0".into(),
            entry("sha256:bb"),
        );
        let mut hooks = toml::Table::new();
        hooks.insert("state".into(), toml::Value::Table(state));
        person.insert("hooks".into(), toml::Value::Table(hooks));
        fs::write(
            user.path().join("config.toml"),
            toml::to_string(&person).unwrap(),
        )
        .unwrap();
        let codex = f.add(
            Provider::Codex,
            AuthKind::ApiKey,
            "default",
            "synthetic-api-key",
        );
        continuation_here_with(
            &f.root,
            &f.store,
            &proxy,
            &codex,
            "isolated",
            &f.project,
            &host,
            &resumed,
            Handed::Terminal,
        )
        .unwrap();
        let home = f.root.join("homes").join(&codex);
        for name in CODEX_LINKS {
            assert_eq!(
                fs::read_link(home.join(name)).unwrap(),
                user.path().join(name),
                "{name} is a link"
            );
        }
        let config: toml::Table =
            toml::from_str(&fs::read_to_string(home.join("config.toml")).unwrap()).unwrap();
        let carried = config["hooks"]["state"].as_table().unwrap();
        assert_eq!(
            carried.len(),
            1,
            "only the person's own hooks.json: {carried:?}"
        );
        let ours = &carried[&format!("{}:session_start:0:0", home.join("hooks.json").display())];
        assert_eq!(ours["trusted_hash"].as_str(), Some("sha256:aa"));
        assert_eq!(ours["enabled"].as_bool(), Some(true));
        assert!(ours.get("note").is_none());
        assert!(!fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .contains("never-copied"));
        assert!(!fs::symlink_metadata(home.join("auth.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_ne!(
            fs::read_to_string(home.join("auth.json")).unwrap(),
            "operator sign-in"
        );
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
            mcp_sign_ins: no_mcp_sign_ins,
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
        // SB-298: the bypass flag sits between them, never a value of `--mcp-config`.
        assert!(text.contains(&format!(
            "{} {} {}",
            quote(&format!(
                "--mcp-config={}",
                home.join("switchboard-mcp.json").to_string_lossy()
            )),
            quote("--dangerously-skip-permissions"),
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
    #[tokio::test]
    async fn an_account_given_to_an_agent_launches_only_once_used_anyway() {
        // SB-322: no launch hands an agent's account a session, until the person used it
        // anyway, which made it its group's selection.
        let f = fixture();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        f.store.assign_to_agent(&id, Some("Frame Agent")).unwrap();
        for mode in ["isolated", "managed"] {
            assert_eq!(
                preflight(&f.root, &f.store, &id, mode, &f.project).unwrap_err(),
                switchboard_core::ASSIGNED
            );
        }
        f.store
            .select_anyway(Provider::Claude, "default", &id, 1_000)
            .unwrap();
        assert!(preflight(&f.root, &f.store, &id, "isolated", &f.project).is_ok());
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
            "cd {} || exit 1\nexec '/opt/Provider Tools/o'\\''cli' '--dangerously-skip-permissions'\n",
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
            // SB-452: the home's own route, last on the command line, so a folder's (or a
            // worktree's main checkout's) local settings cannot send the session elsewhere.
            let route = quote(&route_arg(&home));
            let exec = text.lines().find(|l| l.starts_with("exec ")).unwrap();
            assert!(exec.ends_with(&route), "{exec}");
        }
        let file = home.join(ROUTE_FILE);
        let route: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            route,
            json!({"env": {
                "ANTHROPIC_BASE_URL": format!("http://{}/claude/work", proxy.address()),
                "ANTHROPIC_AUTH_TOKEN": proxy.token(),
                "ANTHROPIC_API_KEY": "",
                "ANTHROPIC_CUSTOM_HEADERS": "X-Switchboard-Launch-Group: work",
            }})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    /// SB-452: a caller's own words come before the home's route, and a caller's `--` after it.
    #[cfg(not(windows))]
    #[tokio::test]
    async fn the_launch_route_follows_a_callers_words_and_precedes_its_end_of_options() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "work", CLAUDE_OAUTH);
        f.store.select(Provider::Claude, "work", &id).unwrap();
        let (_, script) = launch_full(
            &f.root,
            &f.store,
            &proxy,
            &id,
            "managed",
            &f.project,
            &TEST_HOST,
            None,
            &[
                "--settings".into(),
                "/theirs.json".into(),
                "--".into(),
                "-x".into(),
            ],
            true,
        )
        .unwrap();
        let home = f.root.join("runtimes").join("claude-work");
        let text = fs::read_to_string(script.unwrap()).unwrap();
        let exec = text.lines().find(|l| l.starts_with("exec ")).unwrap();
        assert!(
            exec.ends_with(&format!(
                "'--settings' '/theirs.json' {} '--' '-x'",
                quote(&route_arg(&home))
            )),
            "{exec}"
        );
    }
    #[cfg(not(windows))]
    #[tokio::test]
    async fn sb440_an_in_place_launch_nobody_ran_frees_the_group_runtime() {
        // 2026-11-18: `launch --mode managed --headless` timed out on its control request after
        // the owner had prepared `runtimes/claude-tokens`; every later launch was refused.
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "tokens", CLAUDE_OAUTH);
        f.store.select(Provider::Claude, "tokens", &id).unwrap();
        let prepare = || {
            launch_full(
                &f.root,
                &f.store,
                &proxy,
                &id,
                "managed",
                &f.project,
                &TEST_HOST,
                None,
                &[],
                true,
            )
        };
        prepare().unwrap();
        let home = f.root.join("runtimes").join("claude-tokens");
        assert_eq!(home_state(&home).0, "starting");
        assert_eq!(
            prepare().unwrap_err(),
            "Close the existing session before changing its home.",
            "a caller that may still run the script keeps the home"
        );
        age(&home.join(".launch-pending"), 6 * 60);
        assert_eq!(home_state(&home).0, "ended", "nobody will run it");
        prepare().unwrap();
        assert_eq!(home_state(&home).0, "starting");
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
        // SB-440: the failed launch took its own marker away; nothing was left to clear.
        assert!(!f
            .root
            .join("homes")
            .join(&id)
            .join(".launch-pending")
            .exists());
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
    /// SB-455: a launch script exports the program's folder, then the PATH it was started with,
    /// then the login PATH for what that PATH lacks.
    #[cfg(unix)]
    #[test]
    fn a_launch_script_exports_the_login_path() {
        let brew = PathBuf::from("/synthetic/homebrew/bin");
        let text = crate::login_path::with_login(vec![brew.clone(), "/usr/bin".into()], || {
            script(
                Path::new("/tmp/home"),
                Path::new("/opt/agent/bin/claude"),
                &[],
                &BTreeMap::new(),
                false,
                Path::new("/tmp/project"),
            )
        });
        let line = text
            .lines()
            .find(|l| l.starts_with("export PATH="))
            .unwrap();
        let prefix = format!(
            "export PATH='/opt/agent/bin'\"${{PATH:+:$PATH}}\"':{}:/usr/bin:",
            brew.display()
        );
        assert!(line.starts_with(&prefix), "{line}");
        assert!(line.contains("/opt/homebrew/bin"));
        // The program's folder is not repeated after the inherited PATH.
        assert_eq!(line.matches("/opt/agent/bin").count(), 1);
        // Exported after the scrub and before the session's own variables and its program.
        assert!(text.find("unset ").unwrap() < text.find("export PATH=").unwrap());
        assert!(text.find("export PATH=").unwrap() < text.find("exec ").unwrap());
        // The resume script of the duty officer and Continue carries the same line.
        let resume = crate::login_path::with_login(vec![brew.clone()], || {
            crate::duty::resume_script_for(
                Path::new("/tmp/config"),
                "/tmp/project",
                "s1",
                Path::new("/opt/agent/bin/claude"),
                &[],
                None,
            )
        });
        assert!(resume.contains(&format!(
            "export PATH='/opt/agent/bin'\"${{PATH:+:$PATH}}\"':{}:",
            brew.display()
        )));
    }
    /// SB-455 (review round 1): the PATH a script was started with stays ahead of the login PATH,
    /// so a terminal's shim folder wins over `/usr/bin`; with an empty PATH the session still gets
    /// the program's folder and the login folders, and never an empty entry.
    #[cfg(unix)]
    #[test]
    fn the_inherited_path_stays_ahead_of_the_login_path() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let shims = base.join("shims");
        let brew = base.join("brew");
        fs::create_dir_all(&shims).unwrap();
        fs::create_dir_all(&brew).unwrap();
        let line = path_export_with(
            Path::new("/opt/agent/bin/claude"),
            Some(&[brew.clone(), "/usr/bin".into()]),
        );
        let probe = format!("{line}printf '%s' \"$PATH\"");
        let run = |path: String| {
            let output = Command::new("/bin/sh")
                .arg("-c")
                .arg(&probe)
                .env_clear()
                .env("PATH", path)
                .output()
                .unwrap();
            String::from_utf8(output.stdout).unwrap()
        };
        let got = run(format!("{}:/usr/bin:/bin", shims.display()));
        let dirs: Vec<PathBuf> = std::env::split_paths(&got).collect();
        assert_eq!(dirs[0], PathBuf::from("/opt/agent/bin"));
        assert_eq!(dirs[1], shims);
        let at = |d: &Path| dirs.iter().position(|x| x == d).unwrap();
        assert!(at(&shims) < at(&brew));
        let got = run(String::new());
        assert!(
            got.starts_with(&format!("/opt/agent/bin:{}:", brew.display())),
            "{got}"
        );
        assert!(!got.contains("::") && !got.ends_with(':'), "{got}");
    }
    /// SB-455: a hook the session runs through `/bin/sh` (as Claude Code runs its hooks) finds a
    /// tool that only the login PATH names, although the script starts with a launchd PATH.
    #[cfg(unix)]
    #[test]
    fn a_hook_in_a_launched_session_finds_a_tool_on_the_login_path() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let brew = base.join("homebrew/bin");
        fs::create_dir_all(&brew).unwrap();
        private_write(
            &brew.join("sb455-node"),
            b"#!/bin/sh\necho synthetic-node\n",
            true,
        )
        .unwrap();
        let agent_dir = base.join("agent");
        fs::create_dir(&agent_dir).unwrap();
        let report = base.join("report");
        let agent = agent_dir.join("claude");
        private_write(
            &agent,
            format!(
                "#!/bin/sh\n/bin/sh -c 'sb455-node' > {} 2>&1\n",
                quote(&report.to_string_lossy())
            )
            .as_bytes(),
            true,
        )
        .unwrap();
        let home = base.join("home");
        private_dir(&home).unwrap();
        let project = base.join("project");
        fs::create_dir(&project).unwrap();
        let text = crate::login_path::with_login(vec![brew.clone()], || {
            script(&home, &agent, &[], &BTreeMap::new(), false, &project)
        });
        let path = home.join("launch.command");
        private_write(&path, text.as_bytes(), true).unwrap();
        let status = Command::new(SCRIPT_SHELL)
            .arg(&path)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_to_string(report).unwrap(), "synthetic-node\n");
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

    /// SB-212: the launch id a session's script exports, read back from the script as the
    /// session's shell would see it.
    #[cfg(unix)]
    fn exported_launch_id(home: &Path) -> String {
        let script = fs::read_to_string(home.join("launch.command")).unwrap();
        let line = script
            .lines()
            .find_map(|l| l.strip_prefix("export SWITCHBOARD_LAUNCH_ID="))
            .unwrap_or_else(|| panic!("no launch id exported: {script}"));
        line.trim_matches('\'').to_owned()
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn two_launches_of_one_home_keep_two_records_and_each_script_names_its_own() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let id = f.add(Provider::Claude, AuthKind::OAuth, "default", CLAUDE_OAUTH);
        let home = f.root.join("homes").join(&id);
        let mut ids = Vec::new();
        for _ in 0..2 {
            launch_with(
                &f.root, &f.store, &proxy, &id, "isolated", &f.project, &TEST_HOST, None,
            )
            .unwrap();
            ids.push(exported_launch_id(&home));
            // The script ran and its session ended: the home is free for the next launch.
            fs::remove_file(home.join(".launch-pending")).unwrap();
        }
        assert_ne!(ids[0], ids[1]);
        let current: Value =
            serde_json::from_slice(&fs::read(home.join(SESSION_RECORD)).unwrap()).unwrap();
        assert_eq!(current["launch_id"], ids[1].as_str());
        for launch in &ids {
            let file = f.root.join("launches").join(format!("{launch}.json"));
            let record: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
            assert_eq!(record["launch_id"], launch.as_str());
            assert_eq!(record["account_id"], id.as_str());
            assert_eq!(record["home_path"], format!("homes/{id}"));
            assert_eq!(record["launched_by"]["surface"], "window");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert!(!record.to_string().contains("synthetic"), "{record}");
        }
        let (history, total) = crate::launches::history(&f.root);
        assert_eq!(total, 2);
        assert_eq!(history.len(), 2);
        let state = |launch: &str| {
            history.iter().find(|r| r["launch_id"] == launch).unwrap()["state"].clone()
        };
        assert_eq!(state(&ids[0]), "ended", "the home holds a later launch");
        assert_eq!(state(&ids[1]), "ended", "no process runs it");
        // An earlier record without a launch id still lists as it did.
        fs::write(
            home.join(SESSION_RECORD),
            r#"{"account_id":"old","provider":"claude","launched_at":1}"#,
        )
        .unwrap();
        let listed = sessions(&f.root, true);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["account_id"], "old");
        assert!(listed[0]["launch_id"].is_null());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn a_chain_of_three_launches_follows_the_variable_each_session_exports() {
        // A launches B from inside its session, B launches C: each caller is read from the
        // variable its own script exported, as `switchboard launch` reads its environment.
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let mut homes = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for (n, pool) in ["one", "two", "three"].iter().enumerate() {
            let account = f.add(Provider::Claude, AuthKind::OAuth, pool, CLAUDE_OAUTH);
            let mut by = crate::launches::LaunchedBy::surface(crate::launches::Surface::Cli);
            by.caller_launch_id = ids.last().cloned();
            if n == 2 {
                by.by = Some("voc-learn".into());
            }
            let root = f.root.clone();
            let store = f.store.clone();
            let project = f.project.clone();
            let proxy = &proxy;
            let answer = crate::launches::scoped(Some(by), async move {
                launch_with(
                    &root, &store, proxy, &account, "isolated", &project, &TEST_HOST, None,
                )
                .map(|_| json!({}))
            })
            .await
            .unwrap();
            let home = f.root.join("homes").join(
                f.store
                    .snapshot()
                    .unwrap()
                    .accounts
                    .iter()
                    .find(|a| a.pool == *pool)
                    .unwrap()
                    .id
                    .clone(),
            );
            let exported = exported_launch_id(&home);
            assert_eq!(answer["launch_id"], exported.as_str());
            ids.push(exported);
            homes.push(home);
        }
        let record = |n: usize| -> Value {
            serde_json::from_slice(
                &fs::read(f.root.join("launches").join(format!("{}.json", ids[n]))).unwrap(),
            )
            .unwrap()
        };
        assert!(record(0)["launched_by"]["caller_launch_id"].is_null());
        assert_eq!(
            record(1)["launched_by"]["caller_launch_id"],
            ids[0].as_str()
        );
        assert_eq!(
            record(2)["launched_by"]["caller_launch_id"],
            ids[1].as_str()
        );
        assert_eq!(record(2)["launched_by"]["by"], "voc-learn");
        assert_eq!(record(1)["launched_by"]["surface"], "cli");
        let shown = crate::launches::show(&f.root, &ids[1]).unwrap();
        assert_eq!(shown["launch"]["launch_id"], ids[1].as_str());
        let up: Vec<&str> = shown["up"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["launch_id"].as_str().unwrap())
            .collect();
        assert_eq!(up, [ids[0].as_str()]);
        let down: Vec<&str> = shown["down"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["launch_id"].as_str().unwrap())
            .collect();
        assert_eq!(down, [ids[2].as_str()]);
        let top = crate::launches::show(&f.root, &ids[2]).unwrap();
        assert_eq!(top["up"].as_array().unwrap().len(), 2);
        assert_eq!(
            crate::launches::show(&f.root, "no-such-launch").unwrap_err(),
            crate::launches::LAUNCH_NOT_FOUND
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn a_launch_naming_its_parent_continues_it_and_the_parent_says_so() {
        let f = fixture();
        let proxy = ProxyHandle::start(f.store.clone()).await.unwrap();
        let first = f.add(Provider::Claude, AuthKind::OAuth, "one", CLAUDE_OAUTH);
        let second = f.add(Provider::Claude, AuthKind::OAuth, "two", CLAUDE_OAUTH);
        launch_with(
            &f.root, &f.store, &proxy, &first, "isolated", &f.project, &TEST_HOST, None,
        )
        .unwrap();
        let parent = exported_launch_id(&f.root.join("homes").join(&first));
        let mut by = crate::launches::LaunchedBy::surface(crate::launches::Surface::Cli);
        by.parent = Some(parent.clone());
        let (root, store, project) = (f.root.clone(), f.store.clone(), f.project.clone());
        let proxy = &proxy;
        let answer = crate::launches::scoped(Some(by), async move {
            launch_with(
                &root, &store, proxy, &second, "isolated", &project, &TEST_HOST, None,
            )
            .map(|_| json!({}))
        })
        .await
        .unwrap();
        let child = answer["launch_id"].as_str().unwrap();
        let parent_record: Value = serde_json::from_slice(
            &fs::read(f.root.join("launches").join(format!("{parent}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            parent_record["continued_as"],
            json!({"kind": "launch", "id": child})
        );
    }
}
