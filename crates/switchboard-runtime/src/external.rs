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
            match security_framework::passwords::get_generic_password(service, account) {
                Ok(bytes) if bytes.len() <= SECRET_CAP => Ok(Some(bytes)),
                Err(e) if e.code() == -25300 => Ok(None),
                _ => Err("Keychain unavailable. Unlock it and allow access, then retry.".into()),
            }
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
    let env = std::env::var(if provider == Provider::Claude {
        "CLAUDE_CONFIG_DIR"
    } else {
        "CODEX_HOME"
    })
    .ok()
    .filter(|s| !s.is_empty());
    let home = explicit
        .map(Path::to_owned)
        .or_else(|| env.as_ref().map(PathBuf::from))
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
    let raw = explicit.map(|p| p.to_string_lossy().into_owned()).or(env);
    if provider == Provider::Claude && explicit.is_none() {
        if let Ok(secure) = std::env::var("CLAUDE_SECURESTORAGE_CONFIG_DIR") {
            if secure != raw.clone().unwrap_or_default() {
                return Err("Claude secure storage points at another profile. Use a shell with matching Claude settings.".into());
            }
        }
    }
    let config = if provider == Provider::Codex {
        home.join("config.toml")
    } else if Native.read(&home.join(".config.json"), CAP)?.is_some() {
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
        user: username()?,
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
            let profile = claude_profile(&secret, &cfg, Some(label))?;
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
            return match value {
                Some(bytes) => {
                    security_framework::passwords::set_generic_password(&c.service, &c.user, bytes)
                        .map_err(|_| "Claude credential write needs Keychain access.".into())
                }
                None => match security_framework::passwords::delete_generic_password(
                    &c.service, &c.user,
                ) {
                    Ok(()) => Ok(()),
                    Err(e) if e.code() == -25300 => Ok(()),
                    _ => Err("Claude credential rollback needs Keychain access.".into()),
                },
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
        for path in paths {
            checked_path(&path)?;
            fs::create_dir(&path).map_err(|_| {
                "Claude is updating its account. Wait for login or refresh to finish, then retry."
            })?;
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
fn activate(
    writer: &dyn Writer,
    c: &Context,
    credential: &Credential,
    target: &ExternalIdentity,
    expected: Option<&ExternalIdentity>,
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
    let mut auth = native
        .get("auth")
        .cloned()
        .ok_or("Claude native credential is missing.")?;
    let tokens = auth
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or("Claude native credential is invalid.")?;
    tokens.insert("accessToken".into(), json!(credential.access_token));
    if let Some(refresh) = &credential.refresh_token {
        tokens.insert("refreshToken".into(), json!(refresh));
    } else {
        tokens.remove("refreshToken");
    }
    if let Some(expiry) = credential.expires_at {
        tokens.insert("expiresAt".into(), json!(expiry.saturating_mul(1000)));
    }
    let new_auth = serde_json::to_vec(&auth).map_err(|_| UNAVAILABLE)?;
    if new_auth.len() > SECRET_CAP {
        return Err("Claude credential exceeds size limit.".into());
    }
    Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        std::str::from_utf8(&new_auth).map_err(|_| UNAVAILABLE)?,
    )?;
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
    if match (expected, current.as_ref()) {
        (Some(a), Some(b)) => !same_identity(a, b),
        (None, None) => false,
        _ => true,
    } {
        return Err("Current Claude account changed. Refresh the account list, then retry.".into());
    }
    let old_auth = if c.mac {
        writer.keychain(&c.service, &c.user)?
    } else {
        writer.read(&c.home.join(".credentials.json"), SECRET_CAP)?
    };
    // An existing config with no readable credential must not be overwritten.
    if current.is_some() && old_auth.is_none() {
        return Err("Current Claude credential is unavailable; activation was cancelled.".into());
    }
    config
        .as_object_mut()
        .ok_or("Claude config format is invalid.")?
        .insert("oauthAccount".into(), oauth.clone());
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
    writer.auth_write(c, Some(&new_auth))?;
    // Losing ownership forbids both continuation and rollback: another writer
    // may now own the live generation. Surface the partial result explicitly.
    ensure_lock().map_err(|_|"Claude account lock was lost after credential write. Sign in through Claude before retrying.")?;
    if writer.config_write(&c.config, Some(&new_config)).is_err() {
        ensure_lock().map_err(|_| "Claude account lock was lost after credential write. Sign in through Claude before retrying.")?;
        let auth_restored = writer.auth_write(c, old_auth.as_deref()).is_ok();
        ensure_lock().map_err(|_| "Claude rollback lost its account lock. Check the current Claude sign-in before retrying.")?;
        // Atomic config writes leave the old file intact on failure; restore as
        // well for writer backends where the failure can occur after commit.
        let config_restored = writer
            .config_write(&c.config, old_config.as_deref())
            .is_ok();
        return Err(if auth_restored && config_restored { "Claude activation failed; previous account restored." } else { "Claude activation failed and rollback needs attention. Sign in through Claude before retrying." }.into());
    }
    Ok(())
}
pub fn activate_claude(
    credential: &Credential,
    identity: &ExternalIdentity,
    expected_current: Option<&ExternalIdentity>,
) -> Result<(), String> {
    let c = context(Provider::Claude, None)?;
    checked_path(&c.home)?;
    if !c.home.is_dir() {
        return Err("Open Claude Code once before activating a profile.".into());
    }
    let locks = Locks::acquire(&c)?;
    activate(&Native, &c, credential, identity, expected_current, || {
        locks.ensure()
    })
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
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                files: RefCell::new(BTreeMap::new()),
                keys: RefCell::new(BTreeMap::new()),
                fail_config: Cell::new(false),
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
            let mut files = self.files.borrow_mut();
            if let Some(v) = v {
                files.insert(c.home.join(".credentials.json"), v.into());
            } else {
                files.remove(&c.home.join(".credentials.json"));
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
        assert!(activate(&f, &c, &new.credential, &new.identity, None, || Ok(())).is_err());
        assert_eq!(*f.files.borrow(), before);
        f.fail_config.set(true);
        assert!(activate(
            &f,
            &c,
            &new.credential,
            &new.identity,
            Some(&old.identity),
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
            || Ok(()),
        )
        .unwrap();
        let cfg = parse(&f.files.borrow()[&c.config]).unwrap();
        assert_eq!(cfg["projects"]["keep"], true);
        assert_eq!(cfg["oauthAccount"]["emailAddress"], "new@example.test");
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
        assert!(Locks::acquire(&c).is_err());
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
