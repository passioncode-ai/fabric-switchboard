use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use switchboard_core::{AuthKind, Credential, Provider, Store};
use switchboard_proxy::ProxyHandle;
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
    "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
    "SWITCHBOARD_LOCAL_TOKEN",
];
fn private_dir(path: &Path) -> Result<(), String> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err("Unsafe managed home. Check app-data permissions.".into());
        }
    }
    fs::create_dir_all(path).map_err(|_| "Managed home unavailable.")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Managed home permissions unavailable.")?;
    }
    Ok(())
}
fn private_write(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Unsafe managed file.".into());
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if executable { 0o700 } else { 0o600 });
    }
    let result = (|| {
        let mut f = options
            .open(&temporary)
            .map_err(|_| "Managed file unavailable.")?;
        f.write_all(bytes)
            .map_err(|_| "Managed file could not be written.")?;
        f.sync_all()
            .map_err(|_| "Managed file could not be saved.")?;
        fs::rename(&temporary, path).map_err(|_| "Managed file could not be installed.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
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
    for dir in dirs {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("Provider CLI not found. Install the official CLI and retry.".into())
}
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
struct Reservation {
    path: PathBuf,
    committed: bool,
}
impl Reservation {
    fn new(home: &Path) -> Result<Self, String> {
        ensure_idle(home)?;
        let path = home.join(".launch-pending");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
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
    let path = home.join("launch.command");
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
    #[cfg(not(target_os = "macos"))]
    {
        Err("Terminal launch is currently supported on macOS only.".into())
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
        Provider::Codex => vec!["-c", "cli_auth_credentials_store=\"file\"", "login"],
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
fn keychain_service(home: &Path) -> String {
    let raw: String = home.to_string_lossy().nfc().collect();
    format!(
        "Claude Code-credentials-{}",
        &format!("{:x}", Sha256::digest(raw.as_bytes()))[..8]
    )
}
fn read_regular(path: &Path) -> Result<String, String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| "Sign-in is not complete. Finish in Terminal, then try again.")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 * 1024 {
        return Err("Sign-in credential is unsupported.".into());
    }
    fs::read_to_string(path).map_err(|_| "Sign-in credential is unavailable.".into())
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
    let marker = home.join(".session-pid");
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
) -> Result<(), String> {
    if !working_directory.is_absolute() {
        return Err("Choose an existing project directory.".into());
    }
    let working_directory = working_directory
        .canonicalize()
        .map_err(|_| "Choose an existing project directory.")?;
    if !working_directory.is_dir() || working_directory.starts_with(root) {
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
    let mut env = BTreeMap::new();
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
                let config=format!("model_provider = \"switchboard\"\ncli_auth_credentials_store = \"file\"\n[model_providers.switchboard]\nname = \"Fabric Switchboard\"\nbase_url = \"{base}/v1\"\nenv_key = \"SWITCHBOARD_LOCAL_TOKEN\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n");
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
                    b"cli_auth_credentials_store = \"file\"\n",
                    false,
                )?;
            }
        }
    }
    open_terminal(
        &home,
        &script(&home, &program, &[], &env, false, &working_directory),
    )?;
    reservation.committed = true;
    store.record("launch", Some(id), mode)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
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
