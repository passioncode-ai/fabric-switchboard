//! Explicit, read-only import of native CLI stores; native activation is separate.
//! Secret-bearing capture and batch values must never implement Debug or Serialize.
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use switchboard_core::{AuthKind, Credential, ExternalIdentity, Provider};
use unicode_normalization::UnicodeNormalization;

const CAP: usize = 1024 * 1024;
const SECRET_CAP: usize = 64 * 1024;
const UNAVAILABLE: &str = "External sign-in unavailable or its files are unsafe.";
pub struct CapturedProfile {
    pub provider: Provider,
    pub kind: AuthKind,
    pub credential: Credential,
    pub identity: ExternalIdentity,
    pub label: String,
}
pub struct ImportBatch {
    pub profiles: Vec<CapturedProfile>,
    pub failed: usize,
    pub skipped: usize,
}

trait Reader {
    fn read(&self, path: &Path, cap: usize) -> Result<Option<Vec<u8>>, String>;
    fn keychain(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String>;
}
struct Native;
fn checked_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(UNAVAILABLE.into());
    }
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(m) => {
                if m.file_type().is_symlink() {
                    return Err(UNAVAILABLE.into());
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if m.file_attributes() & 0x400 != 0 {
                        return Err(UNAVAILABLE.into());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UNAVAILABLE.into()),
        }
    }
    Ok(())
}
#[cfg(windows)]
fn check_windows_file(file: &fs::File) -> Result<(), String> {
    use std::{os::windows::io::AsRawHandle, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            EqualSid, GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION, TOKEN_QUERY,
            TOKEN_USER,
        },
        Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0
            || info.nNumberOfLinks != 1
        {
            return Err(UNAVAILABLE.into());
        }
        let mut owner = null_mut();
        let mut descriptor = null_mut();
        if GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        ) != 0
        {
            return Err(UNAVAILABLE.into());
        }
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            LocalFree(descriptor);
            return Err(UNAVAILABLE.into());
        }
        let mut needed = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
        // A Vec<usize> guarantees TOKEN_USER alignment.
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let okay = needed > 0
            && GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            ) != 0;
        let matches = okay
            && !owner.is_null()
            && EqualSid(owner, (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid) != 0;
        CloseHandle(token);
        LocalFree(descriptor);
        if !matches {
            return Err(UNAVAILABLE.into());
        }
        Ok(())
    }
}
impl Reader for Native {
    fn read(&self, path: &Path, cap: usize) -> Result<Option<Vec<u8>>, String> {
        checked_path(path)?;
        let mut o = fs::OpenOptions::new();
        o.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.custom_flags(0x00200000);
        }
        let f = match o.open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(UNAVAILABLE.into()),
        };
        let m = f.metadata().map_err(|_| UNAVAILABLE)?;
        if !m.is_file() || m.len() > cap as u64 {
            return Err(UNAVAILABLE.into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if m.uid() != unsafe { libc::geteuid() } || m.nlink() != 1 {
                return Err(UNAVAILABLE.into());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err(UNAVAILABLE.into());
            }
            switchboard_core::private_fs::check_path(path)?;
            check_windows_file(&f)?;
        }
        let mut bytes = Vec::new();
        f.take((cap + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| UNAVAILABLE)?;
        if bytes.len() > cap {
            return Err(UNAVAILABLE.into());
        }
        Ok(Some(bytes))
    }
    fn keychain(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        #[cfg(target_os = "macos")]
        {
            crate::external_keychain::read(service, account)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (service, account);
            Ok(None)
        }
    }
}
struct Context {
    home: PathBuf,
    config: PathBuf,
    service: String,
    user: String,
    mac: bool,
}
fn service(raw: &str) -> String {
    let normalized: String = raw.nfc().collect();
    format!(
        "Claude Code-credentials-{}",
        &format!("{:x}", Sha256::digest(normalized.as_bytes()))[..8]
    )
}
fn username() -> Result<String, String> {
    if let Ok(user) = std::env::var("USER") {
        if !user.is_empty() {
            return Ok(user);
        }
    }
    #[cfg(unix)]
    {
        let pw = unsafe { libc::getpwuid(libc::geteuid()) };
        if !pw.is_null() {
            let user = unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
                .to_string_lossy()
                .into_owned();
            if !user.is_empty() {
                return Ok(user);
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(user) = std::env::var("USERNAME") {
            return Ok(user);
        }
    }
    Err("Local user unavailable.".into())
}
fn context(provider: Provider, explicit: Option<&Path>) -> Result<Context, String> {
    let user_home = dirs::home_dir().ok_or("User home unavailable.")?;
    context_from(
        provider,
        explicit,
        &|name| std::env::var(name).ok(),
        &user_home,
        &Native,
        username()?,
    )
}
/// Where Claude Code or Codex keeps its sign-in for this environment, resolved the way the CLI
/// itself resolves it. Inputs are explicit so every branch is testable without the real home.
fn context_from(
    provider: Provider,
    explicit: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
    user_home: &Path,
    reader: &dyn Reader,
    user: String,
) -> Result<Context, String> {
    let variable = env(if provider == Provider::Claude {
        "CLAUDE_CONFIG_DIR"
    } else {
        "CODEX_HOME"
    })
    .filter(|s| !s.is_empty());
    let home = explicit
        .map(Path::to_owned)
        .or_else(|| variable.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| {
            user_home.join(if provider == Provider::Claude {
                ".claude"
            } else {
                ".codex"
            })
        });
    if !home.is_absolute() {
        return Err("CLI home must be an absolute path.".into());
    }
    let raw = explicit
        .map(|p| p.to_string_lossy().into_owned())
        .or(variable);
    if provider == Provider::Claude && explicit.is_none() {
        if let Some(secure) = env("CLAUDE_SECURESTORAGE_CONFIG_DIR") {
            if secure != raw.clone().unwrap_or_default() {
                return Err("Claude secure storage points at another profile. Use a shell with matching Claude settings.".into());
            }
        }
    }
    let config = if provider == Provider::Codex {
        home.join("config.toml")
    } else if reader.read(&home.join(".config.json"), CAP)?.is_some() {
        home.join(".config.json")
    } else if raw.is_some() {
        home.join(".claude.json")
    } else {
        user_home.join(".claude.json")
    };
    Ok(Context {
        home,
        config,
        service: raw
            .as_deref()
            .map(service)
            .unwrap_or_else(|| "Claude Code-credentials".into()),
        user,
        mac: cfg!(target_os = "macos"),
    })
}
fn parse(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|_| "External profile format is invalid.".into())
}
fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .map(str::to_owned)
}
fn identity(config: &Value) -> Result<ExternalIdentity, String> {
    let oauth = config
        .get("oauthAccount")
        .and_then(Value::as_object)
        .ok_or("Claude account identity is missing.")?;
    let v = Value::Object(oauth.clone());
    for key in ["accountUuid", "organizationUuid", "emailAddress"] {
        if let Some(value) = v.get(key) {
            if !value.is_null()
                && (!value.is_string()
                    || (!value.as_str().unwrap_or_default().is_empty() && text(&v, key).is_none()))
            {
                return Err("Claude account identity is invalid.".into());
            }
        }
    }
    let result = ExternalIdentity {
        account_id: text(&v, "accountUuid"),
        organization_id: text(&v, "organizationUuid"),
        email: text(&v, "emailAddress"),
    };
    if result.account_id.is_none() && result.email.is_none() {
        return Err("Claude account identity is missing.".into());
    }
    Ok(result)
}
fn claude_profile(
    auth: &[u8],
    config: &[u8],
    label: Option<String>,
) -> Result<CapturedProfile, String> {
    let config = parse(config)?;
    let identity = identity(&config)?;
    let raw = std::str::from_utf8(auth).map_err(|_| "External credential format is invalid.")?;
    let mut credential = Credential::parse(Provider::Claude, AuthKind::OAuth, raw)?;
    if credential
        .account_id
        .as_ref()
        .zip(identity.account_id.as_ref())
        .is_some_and(|(a, b)| a != b)
    {
        return Err("Claude credential and config identities differ.".into());
    }
    credential.native_context =
        Some(json!({"auth":parse(auth)?,"oauth_account":config["oauthAccount"]}));
    Ok(CapturedProfile {
        provider: Provider::Claude,
        kind: AuthKind::OAuth,
        credential,
        label: label
            .or_else(|| identity.email.clone())
            .unwrap_or_else(|| "Claude account".into()),
        identity,
    })
}
fn current_auth(reader: &dyn Reader, c: &Context) -> Result<Option<Vec<u8>>, String> {
    if c.mac {
        if let Some(bytes) = reader.keychain(&c.service, &c.user)? {
            return Ok(Some(bytes));
        }
    }
    reader.read(&c.home.join(".credentials.json"), SECRET_CAP)
}
fn codex_auth(reader: &dyn Reader, c: &Context) -> Result<Option<Vec<u8>>, String> {
    let config = reader.read(&c.home.join("config.toml"), CAP)?;
    let cfg: toml::Value = config
        .as_deref()
        .map(|b| {
            std::str::from_utf8(b)
                .map_err(|_| "Codex config is invalid.")
                .and_then(|s| toml::from_str(s).map_err(|_| "Codex config is invalid."))
        })
        .transpose()?
        .unwrap_or_else(|| toml::Value::Table(Default::default()));
    let mode = cfg
        .get("cli_auth_credentials_store")
        .and_then(toml::Value::as_str)
        .unwrap_or("file");
    if mode == "file" {
        return reader.read(&c.home.join("auth.json"), SECRET_CAP);
    }
    if !matches!(mode, "auto" | "keyring") {
        return Err("This Codex credential store cannot be imported. Use official sign-in to add a profile.".into());
    }
    let secrets = cfg
        .get("features")
        .and_then(|f| f.get("secret_auth_storage"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(cfg!(windows));
    if secrets || !c.mac {
        return Err("This Codex keyring backend cannot be imported yet. Use official sign-in to add a profile.".into());
    }
    let canonical = c.home.canonicalize().unwrap_or_else(|_| c.home.clone());
    let key = format!(
        "cli|{}",
        &format!(
            "{:x}",
            Sha256::digest(canonical.to_string_lossy().as_bytes())
        )[..16]
    );
    if let Some(value) = reader.keychain("Codex Auth", &key)? {
        return Ok(Some(value));
    }
    if mode == "auto" {
        reader.read(&c.home.join("auth.json"), SECRET_CAP)
    } else {
        Ok(None)
    }
}
fn capture(
    reader: &dyn Reader,
    provider: Provider,
    c: &Context,
) -> Result<CapturedProfile, String> {
    if provider == Provider::Codex {
        let auth = codex_auth(reader, c)?.ok_or("No current Codex sign-in found.")?;
        let raw =
            std::str::from_utf8(&auth).map_err(|_| "External credential format is invalid.")?;
        let parsed = parse(&auth)?;
        let kind = if parsed
            .get("OPENAI_API_KEY")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            AuthKind::ApiKey
        } else {
            AuthKind::OAuth
        };
        let credential = Credential::parse(
            provider,
            kind,
            if kind == AuthKind::ApiKey {
                parsed["OPENAI_API_KEY"].as_str().unwrap_or_default()
            } else {
                raw
            },
        )?;
        let claims = credential
            .id_token
            .as_deref()
            .and_then(|s| s.split('.').nth(1))
            .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
            .and_then(|s| parse(&s).ok())
            .unwrap_or(Value::Null);
        let identity = ExternalIdentity {
            account_id: credential.account_id.clone(),
            organization_id: None,
            email: text(&claims, "email"),
        };
        if codex_auth(reader, c)?.as_deref() != Some(auth.as_slice()) {
            return Err("Current sign-in changed during capture. Try again.".into());
        }
        return Ok(CapturedProfile {
            provider,
            kind,
            credential,
            label: identity
                .email
                .clone()
                .unwrap_or_else(|| "Codex account".into()),
            identity,
        });
    }
    let config = reader
        .read(&c.config, CAP)?
        .ok_or("No current Claude sign-in found.")?;
    let auth = current_auth(reader, c)?.ok_or("No current Claude sign-in found.")?;
    // Claude Code empties its tokens after `invalid_grant`: that is a signed-out state, not a
    // credential to store or to block a switch away from.
    if wiped(&auth) {
        return Err("No current Claude sign-in found.".into());
    }
    let result = claude_profile(&auth, &config, None)?;
    if reader.read(&c.config, CAP)?.as_deref() != Some(config.as_slice())
        || current_auth(reader, c)?.as_deref() != Some(auth.as_slice())
    {
        return Err("Current sign-in changed during capture. Try again.".into());
    }
    Ok(result)
}
pub fn capture_current(provider: Provider) -> Result<CapturedProfile, String> {
    capture(&Native, provider, &context(provider, None)?)
}
pub fn capture_at(provider: Provider, home: &Path) -> Result<CapturedProfile, String> {
    capture(&Native, provider, &context(provider, Some(home))?)
}

fn segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.contains(['/', '\\', ':'])
        && !s.chars().any(char::is_control)
        && s != "."
        && s != ".."
}
fn import(reader: &dyn Reader, root: &Path, mac: bool) -> Result<ImportBatch, String> {
    let sequence = reader
        .read(&root.join("sequence.json"), CAP)?
        .ok_or("No Claude Swap profiles found.")?;
    let parsed = parse(&sequence)?;
    let accounts = parsed
        .get("accounts")
        .and_then(Value::as_object)
        .filter(|a| a.len() <= 256)
        .ok_or("Claude Swap account index is invalid.")?;
    let mut batch = ImportBatch {
        profiles: vec![],
        failed: 0,
        skipped: 0,
    };
    for (slot, row) in accounts {
        let result = (|| {
            let email = row
                .get("email")
                .and_then(Value::as_str)
                .filter(|s| segment(s))
                .ok_or("Invalid import identity.")?;
            if slot.is_empty() || slot.len() > 8 || !slot.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Invalid import slot.".to_string());
            }
            let path = root
                .join("credentials")
                .join(format!(".creds-{slot}-{email}.enc"));
            let baseline = reader.read(&path, SECRET_CAP * 2)?;
            let secret = if let Some(encoded) = &baseline {
                STANDARD
                    .decode(
                        encoded
                            .iter()
                            .copied()
                            .filter(|b| !b.is_ascii_whitespace())
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|_| "Invalid backup credential.")?
            } else if mac {
                reader
                    .keychain("claude-swap", &format!("account-{slot}-{email}"))?
                    .ok_or("Backup credential missing.")?
            } else {
                return Err("Backup credential missing.".into());
            };
            if secret.len() > SECRET_CAP {
                return Err("Backup credential too large.".into());
            }
            let stable = || -> Result<(), String> {
                if reader.read(&path, SECRET_CAP * 2)? != baseline
                    || (baseline.is_none()
                        && reader
                            .keychain("claude-swap", &format!("account-{slot}-{email}"))?
                            .as_deref()
                            != Some(secret.as_slice()))
                {
                    return Err("Backup changed during import.".into());
                }
                Ok(())
            };
            let label = text(row, "alias")
                .filter(|s| s.len() <= 80)
                .unwrap_or_else(|| email.to_owned());
            if row
                .get("kind")
                .or_else(|| row.get("accountKind"))
                .and_then(Value::as_str)
                == Some("api_key")
                || secret.starts_with(b"sk-ant-api")
            {
                let credential = Credential::parse(
                    Provider::Claude,
                    AuthKind::ApiKey,
                    std::str::from_utf8(&secret).map_err(|_| "Invalid backup credential.")?,
                )?;
                stable()?;
                return Ok(CapturedProfile {
                    provider: Provider::Claude,
                    kind: AuthKind::ApiKey,
                    credential,
                    identity: ExternalIdentity {
                        account_id: None,
                        organization_id: None,
                        email: None,
                    },
                    label,
                });
            }
            let cfg = reader
                .read(
                    &root
                        .join("configs")
                        .join(format!(".claude-config-{slot}-{email}.json")),
                    CAP,
                )?
                .ok_or("Backup config missing.")?;
            // A `cswap run` profile can hold a newer generation than the backup (Claude Swap
            // `session.py:272-286`); the one that expires later is the live lineage.
            let account = parse(&cfg)
                .ok()
                .and_then(|c| text(&c["oauthAccount"], "accountUuid"));
            let newest = account
                .and_then(|account| session_credential(reader, root, slot, &account, mac))
                .filter(|session| expires(session) > expires(&secret));
            let profile = claude_profile(newest.as_deref().unwrap_or(&secret), &cfg, Some(label))?;
            if profile.identity.email.as_deref() != Some(email)
                || text(row, "organizationUuid")
                    .as_ref()
                    .is_some_and(|org| profile.identity.organization_id.as_ref() != Some(org))
            {
                return Err("Backup identity mismatch.".into());
            }
            stable()?;
            if reader
                .read(
                    &root
                        .join("configs")
                        .join(format!(".claude-config-{slot}-{email}.json")),
                    CAP,
                )?
                .as_deref()
                != Some(cfg.as_slice())
            {
                return Err("Backup changed during import.".into());
            }
            Ok(profile)
        })();
        match result {
            Ok(profile) => batch.profiles.push(profile),
            Err(_) => batch.failed += 1,
        }
    }
    if reader.read(&root.join("sequence.json"), CAP)?.as_deref() != Some(sequence.as_slice()) {
        return Err("Claude Swap profiles changed during import. Try again.".into());
    }
    Ok(batch)
}
/// What Claude Swap is doing on this Mac.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwapActivity {
    /// A Claude Swap process runs (its TUI, `cswap auto`, its menu bar): it renews the
    /// accounts it holds, and two renewers of one lineage spend each other's refresh token.
    pub running: bool,
    /// It switches Claude Code by itself (`cswap auto`, or the menu bar with automatic
    /// switching on): a second automatic switcher would fight it.
    pub switching: bool,
}
/// Reads the process list once. A leftover LaunchAgent plist proves nothing: the menu bar
/// runs as `cswap menubar` and is seen in the process list like every other mode.
pub fn claude_swap_activity() -> SwapActivity {
    #[cfg(unix)]
    {
        let processes = std::process::Command::new("/bin/ps")
            .args(["-axo", "args="])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let settings = dirs::home_dir()
            .map(|h| h.join(".claude-swap-backup/menubar_settings.json"))
            .and_then(|path| Native.read(&path, CAP).ok().flatten())
            .and_then(|bytes| String::from_utf8(bytes).ok());
        swap_activity(&processes, settings.as_deref())
    }
    #[cfg(not(unix))]
    {
        SwapActivity::default()
    }
}
pub fn claude_swap_running() -> bool {
    claude_swap_activity().running
}
fn swap_activity(processes: &str, menubar_settings: Option<&str>) -> SwapActivity {
    let menubar_switches = menubar_settings
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|v| v.get("auto_switch_enabled").and_then(Value::as_bool))
        .unwrap_or(false);
    let mut activity = SwapActivity::default();
    for line in processes.lines().filter(|l| running_swap_line(l)) {
        activity.running = true;
        match swap_subcommand(line) {
            Some("auto") => activity.switching = true,
            Some("menubar") if menubar_switches => activity.switching = true,
            _ => {}
        }
    }
    activity
}
/// The first word after Claude Swap's own program word that is not a flag: its subcommand.
fn swap_subcommand(line: &str) -> Option<&str> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let program = words.iter().take(3).position(|word| is_swap_word(word))?;
    words[program + 1..]
        .iter()
        .find(|word| !word.starts_with('-'))
        .copied()
}
fn is_swap_word(word: &str) -> bool {
    let name = word.rsplit('/').next().unwrap_or(word);
    name == "cswap"
        || name == "claude-swap"
        || word == "claude_swap"
        || word.ends_with("claude_swap/__main__.py")
}
/// A process line that is Claude Swap itself, not something merely mentioning it.
fn running_swap_line(line: &str) -> bool {
    // The program and its script or `-m` module: never a word later on the command line.
    line.split_whitespace().take(3).any(is_swap_word)
}
/// `claudeAiOauth.expiresAt` of a credential, 0 when absent.
fn expires(auth: &[u8]) -> i64 {
    parse(auth)
        .ok()
        .and_then(|v| {
            v.pointer("/claudeAiOauth/expiresAt")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0)
}
/// The credential of Claude Swap's session profile for `slot` (`sessions/<slot>-<slug>/`): its
/// plaintext seed or, once Claude Code took it over, the Keychain item for that config dir.
/// Only while the session's own config still names `account`: a `/login` inside the session
/// to another account leaves that account's token there, never the slot's.
fn session_credential(
    reader: &dyn Reader,
    root: &Path,
    slot: &str,
    account: &str,
    mac: bool,
) -> Option<Vec<u8>> {
    let sessions = root.join("sessions");
    let entries = fs::read_dir(&sessions).ok()?;
    let prefix = format!("{slot}-");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&prefix) || !segment(&name) {
            continue;
        }
        let home = sessions.join(&name);
        // Claude Code's config for a custom dir: a legacy `.config.json` wins, as in `context`.
        let same = [".config.json", ".claude.json"]
            .iter()
            .find_map(|file| reader.read(&home.join(file), CAP).ok().flatten())
            .and_then(|bytes| parse(&bytes).ok())
            .and_then(|c| text(&c["oauthAccount"], "accountUuid"))
            .is_some_and(|uuid| uuid == account);
        if !same {
            continue;
        }
        if mac {
            if let Ok(Some(bytes)) =
                reader.keychain(&service(&home.to_string_lossy()), &username().ok()?)
            {
                return Some(bytes);
            }
        }
        if let Ok(Some(bytes)) = reader.read(&home.join(".credentials.json"), SECRET_CAP) {
            return Some(bytes);
        }
    }
    None
}
/// A cheap fingerprint of Claude Swap's files — their count, total size and newest change — so
/// the monitor reads them again only when something there moved. None: no backup folder.
pub fn claude_swap_signature() -> Option<(u64, u64, u128)> {
    let root = dirs::home_dir()?.join(".claude-swap-backup");
    swap_signature_at(&root)
}
fn swap_signature_at(root: &Path) -> Option<(u64, u64, u128)> {
    if !root.is_dir() {
        return None;
    }
    let mut files = vec![root.join("sequence.json")];
    for dir in ["credentials", "configs"] {
        if let Ok(entries) = fs::read_dir(root.join(dir)) {
            files.extend(entries.flatten().map(|e| e.path()));
        }
    }
    if let Ok(entries) = fs::read_dir(root.join("sessions")) {
        for entry in entries.flatten() {
            files.push(entry.path().join(".credentials.json"));
            files.push(entry.path().join(".claude.json"));
        }
    }
    let (mut count, mut size, mut newest) = (0u64, 0u64, 0u128);
    for file in files {
        // symlink_metadata: a link is not followed out of Claude Swap's folder.
        let Ok(meta) = fs::symlink_metadata(&file) else {
            continue;
        };
        count += 1;
        size += meta.len();
        if let Ok(modified) = meta.modified() {
            if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
                newest = newest.max(since.as_nanos());
            }
        }
    }
    Some((count, size, newest))
}
pub fn read_claude_swap() -> Result<ImportBatch, String> {
    let root = dirs::home_dir()
        .ok_or("User home unavailable.")?
        .join(".claude-swap-backup");
    import(&Native, &root, cfg!(target_os = "macos"))
}

fn same_identity(a: &ExternalIdentity, b: &ExternalIdentity) -> bool {
    a.account_id == b.account_id && a.organization_id == b.organization_id && a.email == b.email
}
trait Writer: Reader {
    fn auth_write(&self, c: &Context, value: Option<&[u8]>) -> Result<(), String>;
    fn config_write(&self, path: &Path, value: Option<&[u8]>) -> Result<(), String>;
}
fn atomic_write(path: &Path, value: Option<&[u8]>) -> Result<(), String> {
    checked_path(path)?;
    if let Some(bytes) = value {
        // Do not chmod the user's home or third-party directory.
        if fs::symlink_metadata(path).is_ok() {
            switchboard_core::private_fs::check_path(path)?;
        }
        let tmp = path.with_file_name(format!(".switchboard-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut f = switchboard_core::private_fs::create_new(&tmp)?;
            f.write_all(bytes).map_err(|_| UNAVAILABLE)?;
            f.sync_all().map_err(|_| UNAVAILABLE)?;
            drop(f);
            checked_path(path)?;
            switchboard_core::private_fs::replace(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(UNAVAILABLE.into()),
        }
    }
}
impl Writer for Native {
    fn auth_write(&self, c: &Context, value: Option<&[u8]>) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        if c.mac {
            // Claude Code creates and reads this item with /usr/bin/security; writing it
            // from any other executable would make macOS ask for consent (PLAN-0.5 C-2).
            return match value {
                Some(bytes) => crate::external_keychain::write(&c.service, &c.user, bytes)
                    .map_err(|_| "Claude credential write needs Keychain access.".into()),
                None => crate::external_keychain::delete(&c.service, &c.user)
                    .map_err(|_| "Claude credential rollback needs Keychain access.".into()),
            };
        }
        atomic_write(&c.home.join(".credentials.json"), value)
    }
    fn config_write(&self, path: &Path, value: Option<&[u8]>) -> Result<(), String> {
        atomic_write(path, value)
    }
}
#[derive(Clone)]
struct OwnedLock {
    path: PathBuf,
    identity: (u64, u64),
}
fn directory_file(path: &Path) -> Result<fs::File, String> {
    checked_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .custom_flags(0x02000000 | 0x00200000)
            .access_mode(0x80 | 0x100);
    }
    options
        .open(path)
        .map_err(|_| "Claude account lock was lost.".into())
}
fn file_identity(file: &fs::File) -> Result<(u64, u64), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(|_| UNAVAILABLE)?;
        Ok((m.dev(), m.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(UNAVAILABLE.into());
        }
        Ok((
            info.dwVolumeSerialNumber as u64,
            ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        ))
    }
}
impl OwnedLock {
    fn owned_file(&self) -> Result<fs::File, String> {
        let f = directory_file(&self.path)?;
        if file_identity(&f)? != self.identity {
            return Err("Claude account lock was lost.".into());
        }
        Ok(f)
    }
    fn heartbeat(&self) -> Result<(), String> {
        self.owned_file()?
            .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
            .map_err(|_| "Claude account lock heartbeat failed.".into())
    }
}
struct Locks {
    paths: Vec<OwnedLock>,
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Locks {
    fn acquire(c: &Context) -> Result<Self, String> {
        Self::acquire_with(c, true)
    }
    /// `config: false` takes only the two credential locks — a renewal writes no config, and
    /// holding `~/.claude.json.lock` through a network grant would stall Claude Code's writes.
    fn acquire_with(c: &Context, config: bool) -> Result<Self, String> {
        let mut locks = Self {
            paths: vec![],
            stop: None,
            thread: None,
            lost: Default::default(),
        };
        let paths = [
            c.home.join(".oauth_refresh.lock"),
            c.home.with_file_name(format!(
                "{}.lock",
                c.home.file_name().ok_or(UNAVAILABLE)?.to_string_lossy()
            )),
            c.config.with_file_name(format!(
                "{}.lock",
                c.config.file_name().ok_or(UNAVAILABLE)?.to_string_lossy()
            )),
        ];
        for (index, path) in paths
            .into_iter()
            .enumerate()
            .take(if config { 3 } else { 2 })
        {
            checked_path(&path)?;
            // Claude Code's proper-lockfile protocol (Claude Swap `claude_locks.py:46-129`,
            // against the 2.1.218 bundle): credential locks are stale after 60 s, the config
            // lock after 10 s; holders refresh their mtime, so an older one has no holder.
            let stale = Duration::from_secs(if index == 2 { 10 } else { 60 });
            acquire_lock(&path, stale, LOCK_WAIT)?;
            let identity = file_identity(&directory_file(&path)?)?;
            locks.paths.push(OwnedLock { path, identity });
        }
        let paths = locks.paths.clone();
        let lost = locks.lost.clone();
        let (stop, receiver) = std::sync::mpsc::channel();
        locks.stop = Some(stop);
        // Config lock lease is ten seconds. Keep every held lock fresh even
        // while macOS presents a Keychain consent dialog.
        locks.thread = Some(std::thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(std::time::Duration::from_secs(1)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                for path in &paths {
                    if path.heartbeat().is_err() {
                        lost.store(true, std::sync::atomic::Ordering::SeqCst);
                        return;
                    }
                }
            }
        }));
        Ok(locks)
    }
    fn ensure(&self) -> Result<(), String> {
        if self.lost.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Claude account lock was lost.".into());
        }
        for path in &self.paths {
            path.owned_file()?;
        }
        Ok(())
    }
}
/// How long a live holder is waited for; tests do not sit out the full nine seconds.
const LOCK_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(9)
};
/// Creates one lock directory, waiting up to `wait` for a live holder and taking over one whose
/// last heartbeat is older than `stale`. Only one process can win the re-creation.
fn acquire_lock(path: &Path, stale: Duration, wait: Duration) -> Result<(), String> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        match fs::create_dir(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let age = fs::symlink_metadata(path)
                    .ok()
                    .filter(|m| m.is_dir() && !m.file_type().is_symlink())
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.elapsed().ok());
                if age.is_some_and(|age| age > stale) {
                    // A crashed holder: remove only the empty directory, then race to create.
                    let _ = fs::remove_dir(path);
                    continue;
                }
                if std::time::Instant::now() >= deadline {
                    return Err("Claude is updating its account. Wait for login or refresh to finish, then retry.".into());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(_) => {
                return Err("Claude account lock is unavailable. Check permissions of the Claude config directory.".into())
            }
        }
    }
}
impl Drop for Locks {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        for path in self.paths.iter().rev() {
            if path.owned_file().is_ok() {
                let _ = fs::remove_dir(&path.path);
            }
        }
    }
}
/// Credential keys Claude Code shares across every account on the machine: MCP-server and plugin
/// OAuth. They rotate on their own, so on a switch the live copy wins — presence and absence
/// alike — and the target's snapshot of them is discarded (Claude Swap `credentials.py:191-267`).
/// Every other key, `trustedDeviceToken` included, belongs to the account and stays the target's.
const SHARED_KEYS: [&str; 5] = [
    "mcpOAuth",
    "mcpOAuthClientConfig",
    "mcpXaaIdp",
    "mcpXaaIdpConfig",
    "pluginSecrets",
];
/// `claudeAiOauth` emptied by Claude Code after `invalid_grant`: the sign-in has ended.
fn wiped(auth: &[u8]) -> bool {
    parse(auth).is_ok_and(|v| {
        v.get("claudeAiOauth").is_some_and(|o| {
            o.get("accessToken")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        })
    })
}
/// The live credential and config read under Claude Code's locks, handed to the caller before
/// anything is written so it can keep the outgoing account's newest generation.
pub struct Outgoing<'a> {
    pub auth: &'a [u8],
    pub config: &'a [u8],
}
/// A capture of exactly the bytes read under the locks (for the outgoing account).
pub fn profile_of(outgoing: &Outgoing<'_>) -> Result<CapturedProfile, String> {
    claude_profile(outgoing.auth, outgoing.config, None)
}
#[allow(clippy::too_many_arguments)]
fn activate(
    writer: &dyn Writer,
    c: &Context,
    credential: &Credential,
    target: &ExternalIdentity,
    expected: Option<&ExternalIdentity>,
    preserve: &mut dyn FnMut(&Outgoing<'_>) -> Result<(), String>,
    ensure_lock: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let native = credential
        .native_context
        .as_ref()
        .ok_or("Capture this Claude profile again before native activation.")?;
    let oauth = native
        .get("oauth_account")
        .ok_or("Claude profile identity is missing.")?;
    let declared = identity(&json!({"oauthAccount":oauth}))?;
    if !same_identity(&declared, target) {
        return Err("Stored Claude identity differs from its native profile.".into());
    }
    let mut target_auth = native
        .get("auth")
        .cloned()
        .ok_or("Claude native credential is missing.")?;
    let tokens = target_auth
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or("Claude native credential is invalid.")?;
    tokens.insert("accessToken".into(), json!(credential.access_token));
    if let Some(refresh) = &credential.refresh_token {
        tokens.insert("refreshToken".into(), json!(refresh));
    } else {
        tokens.remove("refreshToken");
    }
    // A captured expiry must never sit next to a different access token.
    match credential.expires_at {
        Some(expiry) => {
            tokens.insert("expiresAt".into(), json!(expiry.saturating_mul(1000)));
        }
        None => {
            tokens.remove("expiresAt");
        }
    }
    let old_config = writer.read(&c.config, CAP)?;
    let mut config = old_config
        .as_deref()
        .map(parse)
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let current = if config.get("oauthAccount").is_some() {
        Some(identity(&config)?)
    } else {
        None
    };
    let old_auth = if c.mac {
        writer.keychain(&c.service, &c.user)?
    } else {
        writer.read(&c.home.join(".credentials.json"), SECRET_CAP)?
    };
    let ended = old_auth.as_deref().is_some_and(wiped);
    if match (expected, current.as_ref()) {
        (Some(a), Some(b)) => !same_identity(a, b),
        (None, None) => false,
        // Claude Code still names an account whose sign-in it has wiped: signed out.
        (None, Some(_)) => !ended,
        (Some(_), None) => true,
    } {
        return Err("Current Claude account changed. Refresh the account list, then retry.".into());
    }
    // An existing config with no readable credential must not be overwritten.
    if current.is_some() && old_auth.is_none() {
        return Err("Current Claude credential is unavailable; activation was cancelled.".into());
    }
    let live = old_auth
        .as_deref()
        .map(parse)
        .transpose()?
        .filter(Value::is_object);
    let composed = target_auth
        .as_object_mut()
        .ok_or("Claude native credential is invalid.")?;
    for key in SHARED_KEYS {
        match live.as_ref().and_then(|l| l.get(key)) {
            Some(value) => {
                composed.insert(key.into(), value.clone());
            }
            None => {
                composed.remove(key);
            }
        }
    }
    let new_auth = serde_json::to_vec(&target_auth).map_err(|_| UNAVAILABLE)?;
    if new_auth.len() > SECRET_CAP {
        return Err("Claude credential exceeds size limit.".into());
    }
    Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        std::str::from_utf8(&new_auth).map_err(|_| UNAVAILABLE)?,
    )?;
    let object = config
        .as_object_mut()
        .ok_or("Claude config format is invalid.")?;
    object.insert("oauthAccount".into(), oauth.clone());
    // A config Switchboard creates from nothing would otherwise send Claude Code through its
    // first-run onboarding although an account is signed in (Claude Swap `session.py:911-921`).
    // An existing config keeps whatever it says.
    if old_config.is_none() {
        object.insert("hasCompletedOnboarding".into(), json!(true));
    }
    // A managed API key would keep billing per token over the OAuth sign-in (Claude Swap
    // `credentials.py:797-937`); the whole old config comes back on rollback.
    object.remove("primaryApiKey");
    let new_config = serde_json::to_vec_pretty(&config).map_err(|_| UNAVAILABLE)?;
    if new_config.len() > CAP {
        return Err("Claude config exceeds size limit.".into());
    }
    if writer.read(&c.config, CAP)? != old_config
        || (if c.mac {
            writer.keychain(&c.service, &c.user)?
        } else {
            writer.read(&c.home.join(".credentials.json"), SECRET_CAP)?
        }) != old_auth
    {
        return Err("Current Claude account changed during activation. Try again.".into());
    }
    ensure_lock()?;
    // The outgoing account's newest generation is kept before the first write, under the
    // same locks Claude Code refreshes under; failing to keep it cancels the switch.
    if let (Some(auth), Some(config), false) = (old_auth.as_deref(), old_config.as_deref(), ended) {
        preserve(&Outgoing { auth, config })?;
    }
    ensure_lock()?;
    // A stale plaintext copy on macOS would hand the old account back if the Keychain ever
    // became unreadable; rewrite it when, and only when, it already exists.
    let shadow = c.home.join(".credentials.json");
    let old_shadow = if c.mac {
        writer.read(&shadow, SECRET_CAP)?
    } else {
        None
    };
    let rollback = |writer: &dyn Writer| -> bool {
        let auth = writer.auth_write(c, old_auth.as_deref()).is_ok();
        let shadow = old_shadow
            .as_deref()
            .is_none_or(|bytes| writer.config_write(&shadow, Some(bytes)).is_ok());
        let config = writer
            .config_write(&c.config, old_config.as_deref())
            .is_ok();
        auth && shadow && config
    };
    if writer.auth_write(c, Some(&new_auth)).is_err() {
        // A timed-out write may already have committed: put the old item back.
        ensure_lock().map_err(|_| "Claude account lock was lost after credential write. Sign in through Claude before retrying.")?;
        return Err(if writer.auth_write(c, old_auth.as_deref()).is_ok() {
            "Claude activation failed; previous account restored."
        } else {
            "Claude activation failed and rollback needs attention. Sign in through Claude before retrying."
        }
        .into());
    }
    // Losing ownership forbids both continuation and rollback: another writer
    // may now own the live generation. Surface the partial result explicitly.
    ensure_lock().map_err(|_|"Claude account lock was lost after credential write. Sign in through Claude before retrying.")?;
    let shadow_written =
        old_shadow.is_none() || writer.config_write(&shadow, Some(&new_auth)).is_ok();
    if !shadow_written || writer.config_write(&c.config, Some(&new_config)).is_err() {
        ensure_lock().map_err(|_| "Claude account lock was lost after credential write. Sign in through Claude before retrying.")?;
        let restored = rollback(writer);
        ensure_lock().map_err(|_| "Claude rollback lost its account lock. Check the current Claude sign-in before retrying.")?;
        return Err(if restored { "Claude activation failed; previous account restored." } else { "Claude activation failed and rollback needs attention. Sign in through Claude before retrying." }.into());
    }
    Ok(())
}
/// Switchboard's own homes are never the ordinary Claude Code (Claude Swap
/// `switcher.py:6661-6690`): a session Switchboard launched must not switch "the" account.
fn refuse_own_home(home: &Path) -> Result<(), String> {
    refuse_home_inside(home, crate::default_root().ok())
}
fn refuse_home_inside(home: &Path, root: Option<PathBuf>) -> Result<(), String> {
    let inside = root.is_some_and(|root| {
        let root = root.canonicalize().unwrap_or(root);
        let home = home.canonicalize().unwrap_or_else(|_| home.to_owned());
        home.starts_with(root)
    });
    if inside {
        return Err("This session runs in a Switchboard home. Switch the ordinary Claude Code from the app or a normal terminal.".into());
    }
    Ok(())
}
/// Claude Code's own credential locks, held for a renewal of the account it is signed in to
/// while it is idle — the only way two refreshers of the live lineage cannot race (Claude Swap
/// refreshes the active account the same way, `switcher.py:4072-4300`).
pub struct LiveLock {
    context: Context,
    locks: Locks,
}
/// The access and refresh token of a Claude credential item.
pub fn claude_auth_tokens(auth: &[u8]) -> Option<(String, Option<String>)> {
    let value = parse(auth).ok()?;
    let oauth = value.get("claudeAiOauth")?;
    Some((
        oauth.get("accessToken")?.as_str()?.to_owned(),
        oauth
            .get("refreshToken")
            .and_then(Value::as_str)
            .map(str::to_owned),
    ))
}
/// The live Claude credential item while Claude Code's locks are held. A trait so that
/// synthetic owners never reach the real sign-in.
pub trait LiveItem: Send {
    /// The live credential item, read under the locks.
    fn read(&self) -> Result<Option<Vec<u8>>, String>;
    /// Replaces the live item; only while every lock is still held.
    fn write(&self, auth: &[u8]) -> Result<(), String>;
}
pub fn lock_live() -> Result<Box<dyn LiveItem>, String> {
    let context = context(Provider::Claude, None)?;
    checked_path(&context.home)?;
    refuse_own_home(&context.home)?;
    let locks = Locks::acquire_with(&context, false)?;
    Ok(Box::new(LiveLock { context, locks }))
}
impl LiveItem for LiveLock {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        self.locks.ensure()?;
        current_auth(&Native, &self.context)
    }
    fn write(&self, auth: &[u8]) -> Result<(), String> {
        self.locks.ensure()?;
        write_live(&Native, &self.context, auth)?;
        self.locks.ensure()
    }
}
/// Writes a renewed live item, and on macOS the plaintext copy too when one exists, so it never
/// keeps a spent token (activation does the same).
fn write_live(writer: &dyn Writer, c: &Context, auth: &[u8]) -> Result<(), String> {
    writer.auth_write(c, Some(auth))?;
    let shadow = c.home.join(".credentials.json");
    if c.mac && matches!(writer.read(&shadow, SECRET_CAP), Ok(Some(_))) {
        // The Keychain item is what Claude Code reads; a failed copy is not a failed renewal.
        let _ = writer.config_write(&shadow, Some(auth));
    }
    Ok(())
}
pub fn activate_claude(
    credential: &Credential,
    identity: &ExternalIdentity,
    expected_current: Option<&ExternalIdentity>,
    preserve: &mut dyn FnMut(&Outgoing<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let c = context(Provider::Claude, None)?;
    checked_path(&c.home)?;
    refuse_own_home(&c.home)?;
    if !c.home.is_dir() {
        return Err("Open Claude Code once before activating a profile.".into());
    }
    let locks = Locks::acquire(&c)?;
    activate(
        &Native,
        &c,
        credential,
        identity,
        expected_current,
        preserve,
        || locks.ensure(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
    };
    struct Fixture {
        files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
        keys: RefCell<BTreeMap<(String, String), Vec<u8>>>,
        fail_config: Cell<bool>,
        /// The next credential write commits, then reports failure (a `security` timeout).
        fail_auth_after_commit: Cell<bool>,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                files: RefCell::new(BTreeMap::new()),
                keys: RefCell::new(BTreeMap::new()),
                fail_config: Cell::new(false),
                fail_auth_after_commit: Cell::new(false),
            }
        }
        fn put(&self, path: &Path, bytes: &[u8]) {
            self.files.borrow_mut().insert(path.into(), bytes.into());
        }
    }
    impl Reader for Fixture {
        fn read(&self, p: &Path, cap: usize) -> Result<Option<Vec<u8>>, String> {
            let v = self.files.borrow().get(p).cloned();
            if v.as_ref().is_some_and(|v| v.len() > cap) {
                return Err(UNAVAILABLE.into());
            }
            Ok(v)
        }
        fn keychain(&self, s: &str, a: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.keys.borrow().get(&(s.into(), a.into())).cloned())
        }
    }
    impl Writer for Fixture {
        fn auth_write(&self, c: &Context, v: Option<&[u8]>) -> Result<(), String> {
            if c.mac {
                let key = (c.service.clone(), c.user.clone());
                match v {
                    Some(v) => self.keys.borrow_mut().insert(key, v.into()),
                    None => self.keys.borrow_mut().remove(&key),
                };
            } else {
                let mut files = self.files.borrow_mut();
                if let Some(v) = v {
                    files.insert(c.home.join(".credentials.json"), v.into());
                } else {
                    files.remove(&c.home.join(".credentials.json"));
                }
            }
            if self.fail_auth_after_commit.replace(false) {
                return Err("fixture timeout after commit".into());
            }
            Ok(())
        }
        fn config_write(&self, p: &Path, v: Option<&[u8]>) -> Result<(), String> {
            if self.fail_config.replace(false) {
                return Err("fixture failure".into());
            }
            let mut files = self.files.borrow_mut();
            if let Some(v) = v {
                files.insert(p.into(), v.into());
            } else {
                files.remove(p);
            }
            Ok(())
        }
    }
    fn config(email: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"oauthAccount":{"emailAddress":email,"accountUuid":email,"organizationUuid":"org"},"projects":{"keep":true}})).unwrap()
    }
    fn auth(token: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":token,"refreshToken":"fixture-refresh","expiresAt":4102444800000i64,"scopes":["user:profile"]},"shared":"keep"})).unwrap()
    }
    fn ctx() -> Context {
        Context {
            home: "/fixture/.claude".into(),
            config: "/fixture/.claude.json".into(),
            service: "Claude Code-credentials".into(),
            user: "fixture".into(),
            mac: false,
        }
    }
    #[test]
    fn current_capture_preserves_native_fields_and_does_not_write() {
        let f = Fixture::new();
        let c = ctx();
        f.put(&c.config, &config("a@example.test"));
        f.put(&c.home.join(".credentials.json"), &auth("synthetic-token"));
        let before = f.files.borrow().clone();
        let p = capture(&f, Provider::Claude, &c).unwrap();
        assert_eq!(p.identity.email.as_deref(), Some("a@example.test"));
        assert_eq!(
            p.credential.native_context.unwrap()["auth"]["shared"],
            "keep"
        );
        assert_eq!(*f.files.borrow(), before);
    }
    #[test]
    fn partial_import_rejects_traversal_and_bad_identity() {
        let f = Fixture::new();
        let root = Path::new("/fixture/swap");
        f.put(&root.join("sequence.json"),br#"{"accounts":{"1":{"email":"a@example.test"},"2":{"email":"../escape"},"3":{"email":"bad@example.test"}}}"#);
        f.put(
            &root.join("credentials/.creds-1-a@example.test.enc"),
            STANDARD.encode(auth("synthetic-token")).as_bytes(),
        );
        f.put(
            &root.join("configs/.claude-config-1-a@example.test.json"),
            &config("a@example.test"),
        );
        f.put(
            &root.join("credentials/.creds-3-bad@example.test.enc"),
            b"not-base64!",
        );
        let result = import(&f, root, true).unwrap();
        assert_eq!(result.profiles.len(), 1);
        assert_eq!(result.failed, 2);
    }
    #[test]
    fn activation_cas_and_rollback_preserve_original() {
        let f = Fixture::new();
        let c = ctx();
        f.put(&c.config, &config("old@example.test"));
        f.put(&c.home.join(".credentials.json"), &auth("old-token"));
        let old = capture(&f, Provider::Claude, &c).unwrap();
        let new = claude_profile(&auth("new-token"), &config("new@example.test"), None).unwrap();
        let before = f.files.borrow().clone();
        assert!(activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            None,
            &mut |_| Ok(()),
            || Ok(())
        )
        .is_err());
        assert_eq!(*f.files.borrow(), before);
        f.fail_config.set(true);
        assert!(activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old.identity),
            &mut |_| Ok(()),
            || Ok(())
        )
        .unwrap_err()
        .contains("restored"));
        assert_eq!(*f.files.borrow(), before);
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old.identity),
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        let cfg = parse(&f.files.borrow()[&c.config]).unwrap();
        assert_eq!(cfg["projects"]["keep"], true);
        assert_eq!(cfg["oauthAccount"]["emailAddress"], "new@example.test");
    }
    fn mac_ctx() -> Context {
        Context { mac: true, ..ctx() }
    }
    fn live(f: &Fixture, c: &Context) -> Value {
        let key = (c.service.clone(), c.user.clone());
        parse(&f.keys.borrow()[&key]).unwrap()
    }
    fn put_live(f: &Fixture, c: &Context, value: Value) {
        f.keys.borrow_mut().insert(
            (c.service.clone(), c.user.clone()),
            serde_json::to_vec(&value).unwrap(),
        );
    }
    fn target(token: &str, extra: Value) -> CapturedProfile {
        let mut auth = json!({"claudeAiOauth":{"accessToken":token,"refreshToken":format!("{token}-refresh"),"expiresAt":4102444800000i64}});
        for (k, v) in extra.as_object().unwrap() {
            auth[k] = v.clone();
        }
        claude_profile(
            &serde_json::to_vec(&auth).unwrap(),
            &config("new@example.test"),
            None,
        )
        .unwrap()
    }
    #[test]
    fn shared_keys_come_from_the_live_item_and_account_keys_from_the_target() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"},"mcpOAuth":{"live":1},"pluginSecrets":{"live":2},"trustedDeviceToken":"old-device"}),
        );
        let new = target(
            "new",
            json!({"mcpOAuth":{"stale":1},"mcpXaaIdp":{"stale":3},"trustedDeviceToken":"new-device"}),
        );
        let old = identity(&parse(&config("old@example.test")).unwrap()).unwrap();
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        let item = live(&f, &c);
        assert_eq!(item["claudeAiOauth"]["accessToken"], "new");
        assert_eq!(
            item["mcpOAuth"],
            json!({"live":1}),
            "live MCP OAuth survives the switch"
        );
        assert_eq!(item["pluginSecrets"], json!({"live":2}));
        assert!(
            item.get("mcpXaaIdp").is_none(),
            "a shared key the machine lacks is not resurrected"
        );
        assert_eq!(
            item["trustedDeviceToken"], "new-device",
            "account-bound keys stay the target's"
        );
    }
    #[test]
    fn the_outgoing_generation_is_preserved_under_the_lock_before_any_write() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old-newest","refreshToken":"old-newest-r"}}),
        );
        let new = target("new", json!({}));
        let old = identity(&parse(&config("old@example.test")).unwrap()).unwrap();
        let locked = Cell::new(false);
        let mut seen = Vec::new();
        let mut preserve = |out: &Outgoing<'_>| {
            assert!(locked.get(), "preserve runs under the locks");
            seen.push(profile_of(out).unwrap().credential.refresh_token.unwrap());
            Ok(())
        };
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut preserve,
            || {
                locked.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(seen, ["old-newest-r"]);
        // A failure to keep it cancels the switch with nothing written.
        let f = Fixture::new();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"}}),
        );
        let before = (f.files.borrow().clone(), f.keys.borrow().clone());
        let error = activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Err("vault refused".into()),
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(error, "vault refused");
        assert_eq!((f.files.borrow().clone(), f.keys.borrow().clone()), before);
    }
    #[test]
    fn a_credential_write_that_fails_after_committing_is_rolled_back() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"}}),
        );
        let before = f.keys.borrow().clone();
        let new = target("new", json!({}));
        let old = identity(&parse(&config("old@example.test")).unwrap()).unwrap();
        f.fail_auth_after_commit.set(true);
        let error = activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Claude activation failed; previous account restored."
        );
        assert_eq!(*f.keys.borrow(), before);
    }
    #[test]
    fn a_config_created_by_a_switch_skips_onboarding_and_an_existing_one_is_kept() {
        let f = Fixture::new();
        let c = mac_ctx();
        let new = target("new", json!({}));
        // Signed out and no config at all: the switch creates it.
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            None,
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        let cfg = parse(&f.files.borrow()[&c.config]).unwrap();
        assert_eq!(cfg["hasCompletedOnboarding"], true);
        assert_eq!(
            cfg["oauthAccount"]["accountUuid"],
            new.identity.account_id.clone().unwrap()
        );
        // An existing config never gains the key.
        let f = Fixture::new();
        f.put(
            &c.config,
            &serde_json::to_vec(&json!({"theme":"dark"})).unwrap(),
        );
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            None,
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        let cfg = parse(&f.files.borrow()[&c.config]).unwrap();
        assert!(cfg.get("hasCompletedOnboarding").is_none());
        assert_eq!(cfg["theme"], "dark");
    }
    #[test]
    fn a_wiped_live_sign_in_is_signed_out_and_does_not_block_a_switch() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"","refreshToken":""}}),
        );
        assert_eq!(
            capture(&f, Provider::Claude, &c).err().unwrap(),
            "No current Claude sign-in found."
        );
        let new = target("new", json!({}));
        let mut called = false;
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            None,
            &mut |_| {
                called = true;
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        assert!(!called, "a wiped item is not a generation to keep");
        assert_eq!(live(&f, &c)["claudeAiOauth"]["accessToken"], "new");
        // An absent item under a named account is still refused: it may be unreadable.
        let f = Fixture::new();
        f.put(&c.config, &config("old@example.test"));
        assert!(activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            None,
            &mut |_| Ok(()),
            || Ok(())
        )
        .is_err());
    }
    #[test]
    fn a_managed_api_key_is_removed_and_returns_on_rollback() {
        let f = Fixture::new();
        let c = mac_ctx();
        let mut cfg = parse(&config("old@example.test")).unwrap();
        cfg["primaryApiKey"] = json!("sk-ant-api-fixture");
        f.put(&c.config, &serde_json::to_vec(&cfg).unwrap());
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"}}),
        );
        let new = target("new", json!({}));
        let old = identity(&cfg).unwrap();
        f.fail_config.set(true);
        assert!(activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Ok(()),
            || Ok(())
        )
        .is_err());
        assert_eq!(
            parse(&f.files.borrow()[&c.config]).unwrap()["primaryApiKey"],
            "sk-ant-api-fixture"
        );
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        assert!(parse(&f.files.borrow()[&c.config])
            .unwrap()
            .get("primaryApiKey")
            .is_none());
    }
    #[test]
    fn a_plaintext_copy_on_macos_is_rewritten_only_when_it_exists() {
        let c = mac_ctx();
        let new = target("new", json!({}));
        let old = identity(&parse(&config("old@example.test")).unwrap()).unwrap();
        let shadow = c.home.join(".credentials.json");
        for exists in [false, true] {
            let f = Fixture::new();
            f.put(&c.config, &config("old@example.test"));
            put_live(
                &f,
                &c,
                json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"}}),
            );
            if exists {
                f.put(&shadow, &auth("old"));
            }
            activate(
                &f,
                &c,
                &new.credential,
                &new.identity,
                Some(&old),
                &mut |_| Ok(()),
                || Ok(()),
            )
            .unwrap();
            let file = f.files.borrow().get(&shadow).cloned();
            if exists {
                assert_eq!(
                    parse(&file.unwrap()).unwrap()["claudeAiOauth"]["accessToken"],
                    "new"
                );
            } else {
                assert!(file.is_none(), "never created");
            }
        }
    }
    #[test]
    fn a_captured_expiry_never_outlives_its_access_token() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("old@example.test"));
        put_live(
            &f,
            &c,
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r"}}),
        );
        let mut new = target("new", json!({}));
        new.credential.expires_at = None;
        let old = identity(&parse(&config("old@example.test")).unwrap()).unwrap();
        activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old),
            &mut |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        assert!(live(&f, &c)["claudeAiOauth"].get("expiresAt").is_none());
    }
    #[test]
    fn a_live_lock_is_waited_for_and_a_stale_one_is_taken_over() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".claude.lock");
        fs::create_dir(&path).unwrap();
        let start = std::time::Instant::now();
        assert_eq!(
            acquire_lock(&path, Duration::from_secs(60), Duration::from_millis(300)).unwrap_err(),
            "Claude is updating its account. Wait for login or refresh to finish, then retry."
        );
        assert!(
            start.elapsed() >= Duration::from_millis(250),
            "waited before giving up"
        );
        // A holder that stopped heartbeating long ago left an empty directory: taken over.
        let old = std::time::SystemTime::now() - Duration::from_secs(120);
        fs::File::open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        acquire_lock(&path, Duration::from_secs(60), Duration::from_millis(300)).unwrap();
        assert!(path.is_dir());
        // A holder released during the wait lets us in.
        fs::remove_dir(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let release = path.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            fs::remove_dir(release).unwrap();
        });
        acquire_lock(&path, Duration::from_secs(60), Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
    }
    #[test]
    fn only_claude_swap_itself_counts_as_running() {
        assert!(running_swap_line(
            "/Users/x/.local/share/uv/tools/claude-swap/bin/python3 /Users/x/.local/bin/cswap"
        ));
        assert!(running_swap_line("cswap auto"));
        assert!(running_swap_line(
            "/usr/bin/python3 -m claude_swap/__main__.py"
        ));
        assert!(!running_swap_line("vim notes-about-cswap.md"));
        assert!(!running_swap_line(
            "/Applications/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard"
        ));
    }
    #[test]
    fn a_newer_session_profile_generation_wins_over_the_backup() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let f = Fixture::new();
        f.put(
            &root.join("sequence.json"),
            br#"{"accounts":{"8":{"email":"a@example.test"}}}"#,
        );
        let older = serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":"backup","refreshToken":"backup-r","expiresAt":1000i64}})).unwrap();
        let newer = serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":"session","refreshToken":"session-r","expiresAt":2000i64}})).unwrap();
        f.put(
            &root.join("credentials/.creds-8-a@example.test.enc"),
            STANDARD.encode(&older).as_bytes(),
        );
        f.put(
            &root.join("configs/.claude-config-8-a@example.test.json"),
            &config("a@example.test"),
        );
        // The session folder must exist on disk for discovery; its file lives in the fixture.
        let session = root.join("sessions/8-a_example.test");
        fs::create_dir_all(&session).unwrap();
        f.put(&session.join(".credentials.json"), &newer);
        // Without the session's own config naming the slot's account it is not trusted.
        assert_eq!(
            import(&f, root, false).unwrap().profiles[0]
                .credential
                .access_token,
            "backup"
        );
        f.put(&session.join(".claude.json"), &config("someone-else"));
        assert_eq!(
            import(&f, root, false).unwrap().profiles[0]
                .credential
                .access_token,
            "backup",
            "a /login to another account inside the session"
        );
        f.put(&session.join(".claude.json"), &config("a@example.test"));
        let batch = import(&f, root, false).unwrap();
        assert_eq!(batch.profiles[0].credential.access_token, "session");
        // An older session copy never replaces a newer backup.
        f.put(
            &session.join(".credentials.json"),
            &serde_json::to_vec(
                &json!({"claudeAiOauth":{"accessToken":"stale","expiresAt":10i64}}),
            )
            .unwrap(),
        );
        assert_eq!(
            import(&f, root, false).unwrap().profiles[0]
                .credential
                .access_token,
            "backup"
        );
    }
    #[test]
    fn a_renewed_live_item_reaches_an_existing_plaintext_copy_only() {
        let f = Fixture::new();
        let c = mac_ctx();
        f.put(&c.config, &config("synthetic-account"));
        write_live(&f, &c, &auth("renewed")).unwrap();
        assert_eq!(parse(&auth("renewed")).unwrap(), live(&f, &c));
        assert!(
            !f.files
                .borrow()
                .contains_key(&c.home.join(".credentials.json")),
            "never created"
        );
        f.put(&c.home.join(".credentials.json"), &auth("spent"));
        write_live(&f, &c, &auth("renewed-again")).unwrap();
        assert_eq!(
            f.files.borrow()[&c.home.join(".credentials.json")],
            auth("renewed-again")
        );
    }
    #[test]
    fn a_renewal_takes_only_the_credential_locks() {
        let dir = tempfile::tempdir().unwrap();
        // Lock paths are refused through a symlink (/var → /private/var on macOS).
        let base = dir.path().canonicalize().unwrap();
        let temp = base.as_path();
        let home = temp.join(".claude");
        fs::create_dir_all(&home).unwrap();
        let c = Context {
            home: home.clone(),
            config: temp.join(".claude.json"),
            ..ctx()
        };
        let config_lock = temp.join(".claude.json.lock");
        {
            let locks = Locks::acquire_with(&c, false).unwrap();
            assert!(home.join(".oauth_refresh.lock").is_dir());
            assert!(temp.join(".claude.lock").is_dir());
            assert!(
                !config_lock.exists(),
                "Claude Code can keep writing its config"
            );
            locks.ensure().unwrap();
        }
        let locks = Locks::acquire(&c).unwrap();
        assert!(config_lock.is_dir());
        drop(locks);
        assert!(!config_lock.exists(), "released on drop");
    }
    #[test]
    fn claude_swaps_files_change_their_signature() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".claude-swap-backup");
        assert_eq!(swap_signature_at(&root), None, "no folder, nothing to read");
        fs::create_dir_all(root.join("credentials")).unwrap();
        fs::write(root.join("sequence.json"), b"{}").unwrap();
        let first = swap_signature_at(&root).unwrap();
        assert_eq!(
            swap_signature_at(&root).unwrap(),
            first,
            "stable while untouched"
        );
        fs::write(root.join("credentials/.creds-1-a.enc"), b"renewed").unwrap();
        assert_ne!(swap_signature_at(&root).unwrap(), first);
        let second = swap_signature_at(&root).unwrap();
        fs::create_dir_all(root.join("sessions/1-a")).unwrap();
        fs::write(root.join("sessions/1-a/.credentials.json"), b"x").unwrap();
        assert_ne!(swap_signature_at(&root).unwrap(), second);
    }
    #[test]
    fn claude_swap_switching_is_told_from_merely_running() {
        let ps = "/usr/libexec/foo\n/Users/x/.local/bin/cswap\n";
        assert_eq!(
            swap_activity(ps, None),
            SwapActivity {
                running: true,
                switching: false
            }
        );
        let auto = "/opt/homebrew/bin/python3 -m claude_swap auto --interval 60\n";
        assert!(swap_activity(auto, None).switching);
        let script = "python3 /x/claude_swap/__main__.py --verbose auto\n";
        assert!(swap_activity(script, None).switching);
        // The menu bar switches only when its own setting says so.
        let menubar = "/Users/x/.local/bin/cswap menubar\n";
        assert!(!swap_activity(menubar, None).switching);
        assert!(!swap_activity(menubar, Some(r#"{"auto_switch_enabled": false}"#)).switching);
        assert!(swap_activity(menubar, Some(r#"{"auto_switch_enabled": true}"#)).switching);
        assert!(!swap_activity(menubar, Some("not json")).switching);
        // A word later on someone else's command line is not Claude Swap.
        assert_eq!(
            swap_activity("vim notes-about-cswap auto\n", None),
            SwapActivity::default()
        );
        assert_eq!(swap_activity("", None), SwapActivity::default());
    }
    #[test]
    fn a_home_inside_switchboards_data_folder_is_refused() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().join("ai.passioncode.fabric-switchboard");
        let own = root.join("homes").join("claude-1");
        std::fs::create_dir_all(&own).unwrap();
        let refused = refuse_home_inside(&own, Some(root.clone())).unwrap_err();
        assert!(refused.contains("Switchboard home"), "{refused}");
        // A sibling whose name only starts the same, and no data folder at all, are fine.
        let sibling = data.path().join("ai.passioncode.fabric-switchboard-other");
        std::fs::create_dir_all(&sibling).unwrap();
        assert!(refuse_home_inside(&sibling, Some(root.clone())).is_ok());
        assert!(refuse_home_inside(&own, None).is_ok());
        // A path through a symlink into the data folder is still inside.
        #[cfg(unix)]
        {
            let link = data.path().join("link");
            std::os::unix::fs::symlink(&own, &link).unwrap();
            assert!(refuse_home_inside(&link, Some(root)).is_err());
        }
    }
    #[test]
    fn context_resolves_homes_configs_and_keychain_services_like_the_cli() {
        let f = Fixture::new();
        let home = Path::new("/fixture/user");
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.to_string())
            }
        };
        // Default: ~/.claude with ~/.claude.json and the plain service name.
        let c = context_from(Provider::Claude, None, &env(&[]), home, &f, "u".into()).unwrap();
        assert_eq!(
            (c.home.as_path(), c.config.as_path()),
            (
                Path::new("/fixture/user/.claude"),
                Path::new("/fixture/user/.claude.json")
            )
        );
        assert_eq!(c.service, "Claude Code-credentials");
        // CLAUDE_CONFIG_DIR: config inside it and a hashed service suffix.
        let c = context_from(
            Provider::Claude,
            None,
            &env(&[("CLAUDE_CONFIG_DIR", "/fixture/alt")]),
            home,
            &f,
            "u".into(),
        )
        .unwrap();
        assert_eq!(c.config, Path::new("/fixture/alt/.claude.json"));
        assert_eq!(c.service, service("/fixture/alt"));
        assert!(
            c.service.starts_with("Claude Code-credentials-")
                && c.service.len() == "Claude Code-credentials-".len() + 8
        );
        // A legacy .config.json wins over .claude.json.
        f.put(Path::new("/fixture/alt/.config.json"), b"{}");
        let c = context_from(
            Provider::Claude,
            None,
            &env(&[("CLAUDE_CONFIG_DIR", "/fixture/alt")]),
            home,
            &f,
            "u".into(),
        )
        .unwrap();
        assert_eq!(c.config, Path::new("/fixture/alt/.config.json"));
        // Secure storage pointing at another profile is refused; matching is fine.
        assert!(context_from(
            Provider::Claude,
            None,
            &env(&[
                ("CLAUDE_CONFIG_DIR", "/fixture/alt"),
                ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/fixture/other")
            ]),
            home,
            &f,
            "u".into()
        )
        .is_err());
        assert!(context_from(
            Provider::Claude,
            None,
            &env(&[
                ("CLAUDE_CONFIG_DIR", "/fixture/alt"),
                ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/fixture/alt")
            ]),
            home,
            &f,
            "u".into()
        )
        .is_ok());
        // A relative home is refused; Codex uses CODEX_HOME and config.toml.
        assert!(context_from(
            Provider::Claude,
            None,
            &env(&[("CLAUDE_CONFIG_DIR", "relative")]),
            home,
            &f,
            "u".into()
        )
        .is_err());
        let c = context_from(
            Provider::Codex,
            None,
            &env(&[("CODEX_HOME", "/fixture/codex")]),
            home,
            &f,
            "u".into(),
        )
        .unwrap();
        assert_eq!(c.config, Path::new("/fixture/codex/config.toml"));
        // An explicit home (an isolated sign-in) ignores the environment.
        let c = context_from(
            Provider::Claude,
            Some(Path::new("/fixture/login")),
            &env(&[
                ("CLAUDE_CONFIG_DIR", "/fixture/alt"),
                ("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/elsewhere"),
            ]),
            home,
            &f,
            "u".into(),
        )
        .unwrap();
        assert_eq!(c.service, service("/fixture/login"));
    }
    #[test]
    fn backup_file_precedes_keychain() {
        let f = Fixture::new();
        let root = Path::new("/fixture/swap");
        f.put(
            &root.join("sequence.json"),
            br#"{"accounts":{"1":{"email":"a@example.test"}}}"#,
        );
        f.put(
            &root.join("credentials/.creds-1-a@example.test.enc"),
            STANDARD.encode(auth("new-token")).as_bytes(),
        );
        f.put(
            &root.join("configs/.claude-config-1-a@example.test.json"),
            &config("a@example.test"),
        );
        f.keys.borrow_mut().insert(
            ("claude-swap".into(), "account-1-a@example.test".into()),
            auth("old-token"),
        );
        assert_eq!(
            import(&f, root, true).unwrap().profiles[0]
                .credential
                .access_token,
            "new-token"
        );
    }
    #[test]
    fn lock_contention_never_steals_existing_lock() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut c = ctx();
        c.home = root.join(".claude");
        c.config = root.join(".claude.json");
        fs::create_dir(&c.home).unwrap();
        let existing = c.home.join(".oauth_refresh.lock");
        fs::create_dir(&existing).unwrap();
        assert_eq!(
            Locks::acquire(&c).err().unwrap(),
            "Claude is updating its account. Wait for login or refresh to finish, then retry."
        );
        assert!(existing.is_dir());
        fs::remove_dir(existing).unwrap();
        {
            let _locks = Locks::acquire(&c).unwrap();
            assert!(c.home.join(".oauth_refresh.lock").is_dir());
        }
        assert!(!c.home.join(".oauth_refresh.lock").exists());
    }
    #[cfg(unix)]
    #[test]
    fn unwritable_lock_directory_is_not_reported_as_claude_updating() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut c = ctx();
        c.home = root.join(".claude");
        c.config = root.join(".claude.json");
        fs::create_dir(&c.home).unwrap();
        fs::set_permissions(&c.home, fs::Permissions::from_mode(0o500)).unwrap();
        let error = Locks::acquire(&c).err();
        fs::set_permissions(&c.home, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            error.unwrap(),
            "Claude account lock is unavailable. Check permissions of the Claude config directory."
        );
    }
    #[cfg(unix)]
    #[test]
    fn source_reads_refuse_symlinks_and_preserve_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let file = root.join("auth.json");
        fs::write(&file, b"synthetic").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(Native.read(&file, 32).unwrap().unwrap(), b"synthetic");
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let link = root.join("link");
        symlink(&file, &link).unwrap();
        assert!(Native.read(&link, 32).is_err());
    }
    #[test]
    fn lock_heartbeat_and_replacement_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut c = ctx();
        c.home = root.join(".claude");
        c.config = root.join(".claude.json");
        fs::create_dir(&c.home).unwrap();
        let locks = Locks::acquire(&c).unwrap();
        let owned = &locks.paths[0];
        owned
            .owned_file()
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
            .unwrap();
        owned.heartbeat().unwrap();
        assert!(fs::metadata(&owned.path).unwrap().modified().unwrap() > std::time::UNIX_EPOCH);
        fs::rename(&owned.path, root.join("displaced")).unwrap();
        fs::create_dir(&owned.path).unwrap();
        assert!(locks.ensure().is_err());
        let replacement = owned.path.clone();
        drop(locks);
        assert!(replacement.is_dir());
    }
    #[test]
    fn codex_store_selection_never_reads_stale_file_over_keyring() {
        let f = Fixture::new();
        let mut c = ctx();
        c.mac = true;
        c.home = "/fixture/codex".into();
        f.put(
            &c.home.join("auth.json"),
            br#"{"tokens":{"access_token":"stale-file","account_id":"old"}}"#,
        );
        f.put(
            &c.home.join("config.toml"),
            b"cli_auth_credentials_store = 'auto'",
        );
        let key = format!(
            "cli|{}",
            &format!("{:x}", Sha256::digest(c.home.to_string_lossy().as_bytes()))[..16]
        );
        f.keys.borrow_mut().insert(
            ("Codex Auth".into(), key),
            br#"{"tokens":{"access_token":"live-keychain","account_id":"current"}}"#.to_vec(),
        );
        assert_eq!(
            capture(&f, Provider::Codex, &c)
                .unwrap()
                .credential
                .access_token,
            "live-keychain"
        );
        f.put(
            &c.home.join("config.toml"),
            b"cli_auth_credentials_store = 'auto'\n[features]\nsecret_auth_storage = true",
        );
        assert!(capture(&f, Provider::Codex, &c).is_err());
    }
    #[test]
    fn source_drift_is_not_captured_under_unchanged_index() {
        struct Drifting {
            base: Fixture,
            reads: Cell<usize>,
        }
        impl Reader for Drifting {
            fn read(&self, p: &Path, cap: usize) -> Result<Option<Vec<u8>>, String> {
                if p.file_name()
                    .and_then(|p| p.to_str())
                    .is_some_and(|p| p.ends_with(".enc"))
                {
                    let n = self.reads.get();
                    self.reads.set(n + 1);
                    if n > 0 {
                        return Ok(Some(STANDARD.encode(auth("rotated-token")).into_bytes()));
                    }
                }
                self.base.read(p, cap)
            }
            fn keychain(&self, s: &str, a: &str) -> Result<Option<Vec<u8>>, String> {
                self.base.keychain(s, a)
            }
        }
        let f = Drifting {
            base: Fixture::new(),
            reads: Cell::new(0),
        };
        let root = Path::new("/fixture/swap");
        f.base.put(
            &root.join("sequence.json"),
            br#"{"accounts":{"1":{"email":"a@example.test"}}}"#,
        );
        f.base.put(
            &root.join("credentials/.creds-1-a@example.test.enc"),
            STANDARD.encode(auth("old-token")).as_bytes(),
        );
        f.base.put(
            &root.join("configs/.claude-config-1-a@example.test.json"),
            &config("a@example.test"),
        );
        let result = import(&f, root, true).unwrap();
        assert!(result.profiles.is_empty());
        assert_eq!(result.failed, 1);
    }
    #[test]
    fn lost_lock_before_rollback_never_overwrites_new_owner() {
        let f = Fixture::new();
        let c = ctx();
        f.put(&c.config, &config("old@example.test"));
        f.put(&c.home.join(".credentials.json"), &auth("old-token"));
        let old = capture(&f, Provider::Claude, &c).unwrap();
        let new = claude_profile(&auth("new-token"), &config("new@example.test"), None).unwrap();
        f.fail_config.set(true);
        let calls = Cell::new(0);
        let result = activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old.identity),
            &mut |_| Ok(()),
            || {
                let n = calls.get() + 1;
                calls.set(n);
                if n >= 3 {
                    Err("lost".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.unwrap_err().contains("lock was lost"));
        let auth = parse(&f.files.borrow()[&c.home.join(".credentials.json")]).unwrap();
        assert_eq!(auth["claudeAiOauth"]["accessToken"], "new-token");
    }
}
