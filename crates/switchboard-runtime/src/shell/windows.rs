//! PowerShell on Windows (SB-183, node A-8): where each edition's profile is and whether its
//! execution policy lets a profile load.
//!
//! - Profiles: `<Documents>\WindowsPowerShell\Microsoft.PowerShell_profile.ps1` (Windows
//!   PowerShell 5.1) and `<Documents>\PowerShell\Microsoft.PowerShell_profile.ps1` (PowerShell 7),
//!   the console host's own profile for the current user, which Windows Terminal and the
//!   PowerShell window load. `<Documents>` is the Documents known folder (`FOLDERID_Documents`,
//!   through `dirs::document_dir`, which asks `SHGetKnownFolderPath`), so a Documents folder that
//!   OneDrive or a domain policy redirected is the one PowerShell itself reads.
//! - The execution policy is read, never changed (A-Q4, decision (a)): the scopes
//!   `Get-ExecutionPolicy -List` gives, in its order (MachinePolicy, UserPolicy, Process,
//!   CurrentUser, LocalMachine), the first one set wins, and none set is `Restricted` on a
//!   Windows client (`RemoteSigned` on Windows Server). Windows PowerShell 5.1 keeps
//!   CurrentUser and LocalMachine in the registry (`Software\Microsoft\PowerShell\1\ShellIds\
//!   Microsoft.PowerShell`, value `ExecutionPolicy`, under HKCU and HKLM); PowerShell 7 in
//!   `powershell.config.json` (`<Documents>\PowerShell\` for the user, `$PSHOME` for the
//!   machine). Group Policy sits under `Software\Policies\Microsoft\Windows\PowerShell` (5.1)
//!   and `Software\Policies\Microsoft\PowerShellCore` (7), `EnableScripts` and
//!   `ExecutionPolicy`. The Process scope (`PSExecutionPolicyPreference`) is the reading
//!   process's own, not a new terminal's, so it is never read ([`profile_policy`]). `Restricted` loads no profile
//!   and `AllSigned` refuses the unsigned hook: both read as `policy_blocked`, and nothing is
//!   written to that profile. No child process: registry values and files only.
//!
//! Everything but the reading of this computer's registry, environment and files takes its
//! inputs as arguments, so it is tested on any platform.
use std::path::{Path, PathBuf};

/// The profile of the console host for the current user, in each edition's folder.
pub const PROFILE: &str = "Microsoft.PowerShell_profile.ps1";
/// The setting PowerShell 7 reads in `powershell.config.json`.
const CONFIG_KEY: &str = "Microsoft.PowerShell:ExecutionPolicy";

/// A PowerShell edition with a profile of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Edition {
    /// Windows PowerShell 5.1, part of Windows.
    Desktop,
    /// PowerShell 7 (`pwsh`), installed apart.
    Core,
}
impl Edition {
    pub const ALL: [Edition; 2] = [Edition::Desktop, Edition::Core];
    /// The edition's folder under Documents.
    pub fn folder(self) -> &'static str {
        match self {
            Edition::Desktop => "WindowsPowerShell",
            Edition::Core => "PowerShell",
        }
    }
}

/// The edition's profile under `documents` (the Documents known folder).
pub fn profile_path(documents: &Path, edition: Edition) -> PathBuf {
    documents.join(edition.folder()).join(PROFILE)
}
/// The profiles of the editions installed, in [`Edition::ALL`]'s order.
pub fn profile_paths(documents: &Path, installed: &[Edition]) -> Vec<(Edition, PathBuf)> {
    Edition::ALL
        .into_iter()
        .filter(|e| installed.contains(e))
        .map(|e| (e, profile_path(documents, e)))
        .collect()
}

/// An execution policy as PowerShell names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Restricted,
    AllSigned,
    RemoteSigned,
    Unrestricted,
    Bypass,
    /// `Default`: what an unset policy means on this kind of Windows.
    Default,
}
impl Policy {
    /// A policy's name (any case, surrounding space ignored); None for `Undefined`, an empty
    /// value or a word PowerShell does not know.
    pub fn parse(text: &str) -> Option<Policy> {
        let word = text.trim().to_ascii_lowercase();
        Some(match word.as_str() {
            "restricted" => Policy::Restricted,
            "allsigned" => Policy::AllSigned,
            "remotesigned" => Policy::RemoteSigned,
            "unrestricted" => Policy::Unrestricted,
            "bypass" => Policy::Bypass,
            "default" => Policy::Default,
            _ => return None,
        })
    }
    /// Whether the person's profile, or the unsigned hook it reads, is kept from running.
    pub fn blocks_profiles(self) -> bool {
        matches!(self, Policy::Restricted | Policy::AllSigned)
    }
}

/// The scopes `Get-ExecutionPolicy -List` reads, highest first. None: `Undefined`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scopes {
    pub machine_policy: Option<Policy>,
    pub user_policy: Option<Policy>,
    pub process: Option<Policy>,
    pub current_user: Option<Policy>,
    pub local_machine: Option<Policy>,
}
/// The policy in force: the first scope set, highest first; none set (or `Default`) is
/// `Restricted` on a Windows client and `RemoteSigned` on Windows Server.
pub fn effective(scopes: &Scopes, server: bool) -> Policy {
    let set = [
        scopes.machine_policy,
        scopes.user_policy,
        scopes.process,
        scopes.current_user,
        scopes.local_machine,
    ]
    .into_iter()
    .flatten()
    .next();
    match set {
        Some(Policy::Default) | None if server => Policy::RemoteSigned,
        Some(Policy::Default) | None => Policy::Restricted,
        Some(policy) => policy,
    }
}
/// The policy a new terminal's profile meets: [`effective`] without the Process scope. That
/// scope (`PSExecutionPolicyPreference`) belongs to the process that reads it, here the app,
/// and a terminal the person opens starts with its own.
pub fn profile_policy(scopes: &Scopes, server: bool) -> Policy {
    let terminal = Scopes {
        process: None,
        ..*scopes
    };
    effective(&terminal, server)
}
/// A Group Policy scope: `EnableScripts` 0 turns scripts off (`Restricted`), 1 applies
/// `ExecutionPolicy`; without `EnableScripts` the policy is not configured.
pub fn group_policy(enable_scripts: Option<u32>, policy: Option<&str>) -> Option<Policy> {
    match enable_scripts? {
        0 => Some(Policy::Restricted),
        _ => policy.and_then(Policy::parse),
    }
}
/// The policy a `powershell.config.json` sets, if any.
pub fn config_policy(bytes: &[u8]) -> Option<Policy> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    value.get(CONFIG_KEY)?.as_str().and_then(Policy::parse)
}

/// PowerShell on this computer as the installer sees it. Injected: a test never names the real
/// Documents folder or reads the real registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    /// The Documents known folder.
    pub documents: PathBuf,
    /// The editions installed and the policy in force for each.
    pub editions: Vec<(Edition, Policy)>,
}
/// One profile the installer looks after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub edition: Edition,
    pub path: PathBuf,
    /// The execution policy keeps this profile (or the hook it reads) from running.
    pub blocked: bool,
}
impl Host {
    /// The profiles of the editions installed, each with whether its policy blocks it.
    pub fn profiles(&self) -> Vec<Profile> {
        let installed: Vec<Edition> = self.editions.iter().map(|(e, _)| *e).collect();
        profile_paths(&self.documents, &installed)
            .into_iter()
            .map(|(edition, path)| Profile {
                edition,
                path,
                blocked: self
                    .editions
                    .iter()
                    .find(|(e, _)| *e == edition)
                    .is_some_and(|(_, policy)| policy.blocks_profiles()),
            })
            .collect()
    }
}

/// This computer's PowerShell: the Documents known folder, the editions installed and each
/// one's policy. None when the Documents folder cannot be found.
///
/// Installed: Windows PowerShell 5.1 when `%SystemRoot%\System32\WindowsPowerShell\v1.0\
/// powershell.exe` exists; PowerShell 7 when `pwsh.exe` is in `%ProgramFiles%\PowerShell\7` or
/// on `PATH`, or its profile folder exists under Documents.
#[cfg(windows)]
pub fn process_host() -> Option<Host> {
    let documents = dirs::document_dir()?;
    let server = registry::string(
        registry::Root::Machine,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "InstallationType",
    )
    .is_some_and(|kind| kind.starts_with("Server"));
    let mut editions = Vec::new();
    let desktop = std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
    if desktop.is_some_and(|exe| exe.is_file()) {
        editions.push((Edition::Desktop, profile_policy(&desktop_scopes(), server)));
    }
    let pshome = core_home();
    if pshome.is_some() || documents.join(Edition::Core.folder()).is_dir() {
        let scopes = core_scopes(&documents, pshome.as_deref());
        editions.push((Edition::Core, profile_policy(&scopes, server)));
    }
    Some(Host {
        documents,
        editions,
    })
}

/// Windows PowerShell 5.1's scopes, from the registry.
#[cfg(windows)]
fn desktop_scopes() -> Scopes {
    use registry::Root;
    const SHELL: &str = r"SOFTWARE\Microsoft\PowerShell\1\ShellIds\Microsoft.PowerShell";
    const POLICY: &str = r"SOFTWARE\Policies\Microsoft\Windows\PowerShell";
    let gpo = |root: Root| {
        group_policy(
            registry::dword(root, POLICY, "EnableScripts"),
            registry::string(root, POLICY, "ExecutionPolicy").as_deref(),
        )
    };
    let set = |root: Root| {
        registry::string(root, SHELL, "ExecutionPolicy").and_then(|v| Policy::parse(&v))
    };
    Scopes {
        machine_policy: gpo(Root::Machine),
        user_policy: gpo(Root::User),
        // The app's own Process scope is not the terminal's (profile_policy).
        process: None,
        current_user: set(Root::User),
        local_machine: set(Root::Machine),
    }
}

/// PowerShell 7's scopes: Group Policy from the registry (or Windows PowerShell's, when
/// `UseWindowsPowerShellPolicySetting` says so), the user's and the machine's
/// `powershell.config.json`. A `$PSHOME` that cannot be found (an install reached only
/// through an app alias on `PATH`) is read as the file PowerShell 7 ships, `RemoteSigned`.
#[cfg(windows)]
fn core_scopes(documents: &Path, pshome: Option<&Path>) -> Scopes {
    use registry::Root;
    const CORE: &str = r"SOFTWARE\Policies\Microsoft\PowerShellCore";
    const DESKTOP: &str = r"SOFTWARE\Policies\Microsoft\Windows\PowerShell";
    let gpo = |root: Root| {
        let key = if registry::dword(root, CORE, "UseWindowsPowerShellPolicySetting") == Some(1) {
            DESKTOP
        } else {
            CORE
        };
        group_policy(
            registry::dword(root, key, "EnableScripts"),
            registry::string(root, key, "ExecutionPolicy").as_deref(),
        )
    };
    let file = |path: &Path| {
        std::fs::metadata(path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= 64 * 1024)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|bytes| config_policy(&bytes))
    };
    let config = "powershell.config.json";
    Scopes {
        machine_policy: gpo(Root::Machine),
        user_policy: gpo(Root::User),
        // The app's own Process scope is not the terminal's (profile_policy).
        process: None,
        current_user: file(&documents.join(Edition::Core.folder()).join(config)),
        local_machine: match pshome {
            Some(home) => file(&home.join(config)),
            None => Some(Policy::RemoteSigned),
        },
    }
}

/// `$PSHOME` of PowerShell 7: `%ProgramFiles%\PowerShell\7`, else the folder of a `pwsh.exe`
/// on `PATH` that is not an app alias (`WindowsApps`).
#[cfg(windows)]
fn core_home() -> Option<PathBuf> {
    let program_files = std::env::var_os("ProgramFiles")
        .map(|p| PathBuf::from(p).join("PowerShell").join("7"))
        .filter(|home| home.join("pwsh.exe").is_file());
    program_files.or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .filter(|dir| {
                !dir.components()
                    .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
            })
            .find(|dir| dir.join("pwsh.exe").is_file())
    })
}

/// Reading one registry value, nothing else.
#[cfg(windows)]
mod registry {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };
    #[derive(Clone, Copy)]
    pub enum Root {
        Machine,
        User,
    }
    fn handle(root: Root) -> HKEY {
        match root {
            Root::Machine => HKEY_LOCAL_MACHINE,
            Root::User => HKEY_CURRENT_USER,
        }
    }
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    /// A `REG_SZ` value of at most 32 KiB; None when absent or of another type.
    pub fn string(root: Root, key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut size: u32 = 0;
        // SAFETY: the key and value names are NUL-terminated and live for the call; a null
        // buffer with a size pointer asks for the size only.
        let status = unsafe {
            RegGetValueW(
                handle(root),
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != 0 || size == 0 || size > 32 * 1024 {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer holds `size` bytes and lives for the call.
        let status = unsafe {
            RegGetValueW(
                handle(root),
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16(&buffer[..end]).ok()
    }
    /// A `REG_DWORD` value; None when absent or of another type.
    pub fn dword(root: Root, key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: `data` holds four bytes and lives for the call.
        let status = unsafe {
            RegGetValueW(
                handle(root),
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_paths_follow_the_documents_folder_for_each_edition() {
        // A Documents folder OneDrive redirected: the profiles follow it, not the home.
        let documents = Path::new("/Users/ana/OneDrive - Acme/Documents");
        assert_eq!(
            profile_path(documents, Edition::Desktop),
            documents
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        );
        assert_eq!(
            profile_path(documents, Edition::Core),
            documents
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        );
        // Only the editions installed, in a fixed order whatever the order given.
        assert_eq!(
            profile_paths(documents, &[Edition::Core, Edition::Desktop]),
            vec![
                (Edition::Desktop, profile_path(documents, Edition::Desktop)),
                (Edition::Core, profile_path(documents, Edition::Core)),
            ]
        );
        assert_eq!(
            profile_paths(documents, &[Edition::Desktop]),
            vec![(Edition::Desktop, profile_path(documents, Edition::Desktop))]
        );
        assert!(profile_paths(documents, &[]).is_empty());
        let host = Host {
            documents: documents.to_owned(),
            editions: vec![
                (Edition::Core, Policy::RemoteSigned),
                (Edition::Desktop, Policy::Restricted),
            ],
        };
        assert_eq!(
            host.profiles(),
            vec![
                Profile {
                    edition: Edition::Desktop,
                    path: profile_path(documents, Edition::Desktop),
                    blocked: true,
                },
                Profile {
                    edition: Edition::Core,
                    path: profile_path(documents, Edition::Core),
                    blocked: false,
                },
            ]
        );
        // On Windows the folder is the Documents known folder, as PowerShell reads it.
        #[cfg(windows)]
        {
            let found = process_host().map(|h| h.documents);
            assert_eq!(found, dirs::document_dir());
        }
    }

    #[test]
    fn a_restricted_or_allsigned_policy_reads_as_policy_blocked() {
        for (name, policy, blocks) in [
            ("Restricted", Policy::Restricted, true),
            ("allsigned", Policy::AllSigned, true),
            (" RemoteSigned ", Policy::RemoteSigned, false),
            ("Unrestricted", Policy::Unrestricted, false),
            ("BYPASS", Policy::Bypass, false),
        ] {
            assert_eq!(Policy::parse(name), Some(policy), "{name}");
            assert_eq!(policy.blocks_profiles(), blocks, "{name}");
        }
        assert_eq!(Policy::parse("Undefined"), None);
        assert_eq!(Policy::parse(""), None);
        assert_eq!(Policy::parse("Lenient"), None);
        // Nothing set: Restricted on a client, RemoteSigned on a server.
        let none = Scopes::default();
        assert_eq!(effective(&none, false), Policy::Restricted);
        assert_eq!(effective(&none, true), Policy::RemoteSigned);
        assert!(effective(&none, false).blocks_profiles());
        // `Default` set at a scope means the same.
        let default = Scopes {
            current_user: Some(Policy::Default),
            local_machine: Some(Policy::RemoteSigned),
            ..Scopes::default()
        };
        assert_eq!(effective(&default, false), Policy::Restricted);
        // The first scope set wins, in Get-ExecutionPolicy -List's order.
        let user = Scopes {
            current_user: Some(Policy::RemoteSigned),
            local_machine: Some(Policy::Restricted),
            ..Scopes::default()
        };
        assert_eq!(effective(&user, false), Policy::RemoteSigned);
        let process = Scopes {
            process: Some(Policy::AllSigned),
            ..user
        };
        assert_eq!(effective(&process, false), Policy::AllSigned);
        // A profile in a new terminal never meets the app's own Process scope.
        let app = Scopes {
            process: Some(Policy::Bypass),
            current_user: Some(Policy::Restricted),
            ..Scopes::default()
        };
        assert_eq!(profile_policy(&app, false), Policy::Restricted);
        assert!(profile_policy(&app, false).blocks_profiles());
        let app = Scopes {
            process: Some(Policy::AllSigned),
            ..user
        };
        assert_eq!(profile_policy(&app, false), Policy::RemoteSigned);
        assert_eq!(
            profile_policy(
                &Scopes {
                    process: Some(Policy::Bypass),
                    ..Scopes::default()
                },
                true
            ),
            Policy::RemoteSigned
        );
        let machine = Scopes {
            machine_policy: Some(Policy::AllSigned),
            user_policy: Some(Policy::Bypass),
            ..user
        };
        assert_eq!(effective(&machine, false), Policy::AllSigned);
        let user_policy = Scopes {
            user_policy: Some(Policy::Restricted),
            ..user
        };
        assert!(effective(&user_policy, false).blocks_profiles());
        // Group Policy: EnableScripts 0 turns scripts off, 1 applies the policy named.
        assert_eq!(
            group_policy(Some(0), Some("Unrestricted")),
            Some(Policy::Restricted)
        );
        assert_eq!(
            group_policy(Some(1), Some("AllSigned")),
            Some(Policy::AllSigned)
        );
        assert_eq!(group_policy(Some(1), None), None);
        assert_eq!(group_policy(None, Some("Restricted")), None);
        // PowerShell 7's own file.
        assert_eq!(
            config_policy(br#"{"Microsoft.PowerShell:ExecutionPolicy": "AllSigned"}"#),
            Some(Policy::AllSigned)
        );
        assert_eq!(
            config_policy(
                br#"{"WindowsPowerShellCompatibilityModuleDenyList": [], "Microsoft.PowerShell:ExecutionPolicy":"RemoteSigned"}"#
            ),
            Some(Policy::RemoteSigned)
        );
        assert_eq!(config_policy(b"{}"), None);
        assert_eq!(config_policy(b"not json"), None);
        // A UTF-8 byte order mark, as Notepad writes one, is read through.
        assert_eq!(
            config_policy(b"\xef\xbb\xbf{\"Microsoft.PowerShell:ExecutionPolicy\":\"Restricted\"}"),
            Some(Policy::Restricted)
        );
    }

    /// The PowerShell hook as written: the paths quoted for PowerShell, everything it keeps
    /// global, the file starting with a byte order mark. Runs everywhere, PowerShell or not.
    #[test]
    fn the_ps1_hook_names_its_paths_quoted_and_keeps_its_state_global() {
        use super::super::scripts::{self, Kind};
        let cli = Path::new("/opt/it's $HOME `x`/switchboard");
        let data = Path::new("/data/o'brien");
        let text = scripts::script(Kind::Pwsh, cli, data).unwrap();
        assert!(
            text.contains("$global:_switchboard_cli = '/opt/it''s $HOME `x`/switchboard'"),
            "{text}"
        );
        assert!(text.contains("$global:_switchboard_data = '/data/o''brien'"));
        assert!(!text.contains("@CLI@") && !text.contains("@DATA@"));
        // Every function is global, so the block may read the file in any scope.
        for line in text
            .lines()
            .filter(|l| l.trim_start().starts_with("function "))
        {
            assert!(line.trim_start().starts_with("function global:"), "{line}");
        }
        // It asks `shell env` as the other shells do, and wraps the prompt.
        assert!(text.contains("shell env --shell pwsh --dir"));
        assert!(text.contains("$global:_switchboard_prompt = $function:prompt"));
        assert!(text.contains("function global:prompt"));
        // No em dash in what a person may read.
        assert!(!text.contains('\u{2014}'));
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(scripts::write(&root, Kind::Pwsh, cli), Ok(true));
        let path = scripts::hook_path(&root, Kind::Pwsh);
        assert_eq!(path.file_name().unwrap(), "hook.ps1");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"\xef\xbb\xbf# Fabric Switchboard"));
        // Unchanged bytes are not written again.
        assert_eq!(scripts::write(&root, Kind::Pwsh, cli), Ok(false));

        // PowerShell also ends a single-quoted string at U+2018, U+2019, U+201A and U+201B:
        // each one is doubled too.
        let typographic = Path::new("/Users/o\u{2019}brien \u{2018}a\u{201a}b\u{201b}/switchboard");
        let text = scripts::script(Kind::Pwsh, typographic, data).unwrap();
        assert!(
            text.contains(
                "$global:_switchboard_cli = '/Users/o\u{2019}\u{2019}brien \u{2018}\u{2018}a\u{201a}\u{201a}b\u{201b}\u{201b}/switchboard'"
            ),
            "{text}"
        );
        // With PowerShell on this machine, the written file parses with no error.
        #[cfg(unix)]
        let pwsh = transition::pwsh();
        #[cfg(not(unix))]
        let pwsh: Option<std::path::PathBuf> = None;
        if let Some(pwsh) = pwsh {
            let root = tmp.path().join("typographic");
            std::fs::create_dir_all(&root).unwrap();
            assert_eq!(scripts::write(&root, Kind::Pwsh, typographic), Ok(true));
            let hook = scripts::hook_path(&root, Kind::Pwsh);
            let command = format!(
                "$e = $null; $null = [System.Management.Automation.Language.Parser]::ParseFile({}, [ref]$null, [ref]$e); $e | ForEach-Object {{ $_.Message }}",
                scripts::ps_quote(&hook.to_string_lossy())
            );
            let out = std::process::Command::new(pwsh)
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &command,
                ])
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
            let errors = String::from_utf8_lossy(&out.stdout);
            assert!(errors.trim().is_empty(), "{errors}");
            assert!(
                out.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        } else {
            eprintln!("parse skipped: pwsh is not installed on this machine");
        }
    }

    /// The POSIX shells' transition (`scripts::tests`) in a real PowerShell, against the same
    /// stub CLI they use: entering, moving, leaving, a value the person set, a launched session, a
    /// home inherited from a hooked parent, an answer of another form, and the prompt's own
    /// `$?` and `$LASTEXITCODE`. Needs `pwsh` and a POSIX stub, so it runs on macOS or Linux
    /// with PowerShell 7 installed; anywhere else it says why it was skipped (the Windows run is
    /// SB-02).
    #[test]
    fn the_ps1_transition_matches_the_posix_one() {
        #[cfg(unix)]
        transition::run();
        #[cfg(not(unix))]
        eprintln!("skipped: the stub CLI is a POSIX script; the live check on Windows is SB-02");
    }

    #[cfg(unix)]
    mod transition {
        use super::super::super::scripts::{self, ps_quote, sh_quote, Kind};
        use std::collections::BTreeMap;
        use std::path::{Path, PathBuf};
        use std::process::Command;

        pub fn pwsh() -> Option<PathBuf> {
            let on_path = std::env::var_os("PATH").and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("pwsh"))
                    .find(|p| p.is_file())
            });
            on_path.or_else(|| {
                [
                    "/usr/local/bin/pwsh",
                    "/opt/homebrew/bin/pwsh",
                    "/usr/bin/pwsh",
                    "/usr/local/microsoft/powershell/7/pwsh",
                    "/opt/microsoft/powershell/7/pwsh",
                ]
                .into_iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
            })
        }

        struct World {
            _tmp: tempfile::TempDir,
            data: PathBuf,
            cli: PathBuf,
            log: PathBuf,
            acme: PathBuf,
            globex: PathBuf,
            mine: PathBuf,
            acme_claude: String,
            acme_codex: String,
            globex_claude: String,
        }
        fn world() -> World {
            let tmp = tempfile::tempdir().unwrap();
            // Spaces and quotes in every path the hook names.
            let root = tmp
                .path()
                .canonicalize()
                .unwrap()
                .join("it's a \"quoted\" dir");
            let data = root.join("data dir");
            let work = root.join("work");
            let (acme, globex, mine) = (work.join("acme"), work.join("globex"), work.join("mine"));
            for dir in [&acme.join("src"), &globex, &mine, &data] {
                std::fs::create_dir_all(dir).unwrap();
            }
            let s = |p: &Path| p.to_string_lossy().into_owned();
            let acme_claude = s(&data.join("shell").join("claude-acme"));
            let acme_codex = s(&data.join("shell").join("codex-acme"));
            let globex_claude = s(&data.join("shell").join("claude-globex"));
            let cli = root.join("bin dir").join("switchboard");
            std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
            let log = root.join("calls.log");
            let stub = format!(
                "#!/bin/sh\ndir=\nwhile [ $# -gt 0 ]; do case $1 in --dir) dir=$2; shift;; esac; shift; done\nprintf '%s\\n' \"$dir\" >> {log}\ncase $dir in\n  {acme}|{acme}/*) printf 'acme\\torg:o-acme\\t%s\\t%s\\n' {ac} {ax} ;;\n  {globex}|{globex}/*) printf 'globex\\torg:o-globex\\t%s\\t-\\n' {gc} ;;\n  *) echo none ;;\nesac\n",
                log = sh_quote(&s(&log)),
                acme = sh_quote(&s(&acme)),
                globex = sh_quote(&s(&globex)),
                ac = sh_quote(&acme_claude),
                ax = sh_quote(&acme_codex),
                gc = sh_quote(&globex_claude),
            );
            std::fs::write(&cli, stub).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
            World {
                _tmp: tmp,
                data,
                cli,
                log,
                acme,
                globex,
                mine,
                acme_claude,
                acme_codex,
                globex_claude,
            }
        }
        fn show(tag: &str) -> String {
            format!(
                "foreach ($sb_n in 'CLAUDE_CONFIG_DIR', 'CODEX_HOME', 'SWITCHBOARD_CONTEXT') {{ $sb_v = [Environment]::GetEnvironmentVariable($sb_n); if ($null -ne $sb_v) {{ Write-Output ('{tag} ' + $sb_n + '=' + $sb_v) }} else {{ Write-Output ('{tag} ' + $sb_n + ' unset') }} }}"
            )
        }
        /// A change of location and the prompt that follows it.
        fn cd(dir: &Path) -> String {
            format!(
                "Set-Location -LiteralPath {}\n$null = prompt",
                ps_quote(&dir.to_string_lossy())
            )
        }
        fn pwsh_run(w: &World, pwsh: &Path, env: &[(&str, &str)], body: &str) -> (String, String) {
            let hook = scripts::hook_path(&w.data, Kind::Pwsh);
            std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
            std::fs::write(&hook, scripts::script(Kind::Pwsh, &w.cli, &w.data).unwrap()).unwrap();
            let script = format!(
                "Set-StrictMode -Version Latest\n. {}\n{body}",
                ps_quote(&hook.to_string_lossy())
            );
            let mut command = Command::new(pwsh);
            command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &script,
                ])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", w.data.parent().unwrap())
                .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
                .env("POWERSHELL_UPDATECHECK", "Off")
                .current_dir(&w.mine);
            for (k, v) in env {
                command.env(k, v);
            }
            let out = command.output().unwrap();
            (
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        }
        fn seen(out: &str) -> BTreeMap<String, String> {
            out.lines()
                .filter_map(|l| {
                    let (tag, rest) = l.split_once(' ')?;
                    let (name, value) = match rest.split_once('=') {
                        Some((n, v)) => (n, v.to_owned()),
                        None => (rest.strip_suffix(" unset")?, "unset".to_owned()),
                    };
                    Some((format!("{tag} {name}"), value))
                })
                .collect()
        }
        fn calls(w: &World) -> usize {
            std::fs::read_to_string(&w.log)
                .map(|t| t.lines().count())
                .unwrap_or(0)
        }

        pub fn run_all(pwsh: &Path) {
            // Entering, moving and leaving: the person's own value set back, an unset one unset.
            let w = world();
            let body = [
                show("start"),
                cd(&w.acme.join("src")),
                show("acme"),
                cd(&w.globex),
                show("globex"),
                cd(&w.mine),
                show("mine"),
            ]
            .join("\n");
            let (out, err) = pwsh_run(&w, pwsh, &[("CLAUDE_CONFIG_DIR", "/own/claude dir")], &body);
            let v = seen(&out);
            assert_eq!(
                v["start CLAUDE_CONFIG_DIR"], "/own/claude dir",
                "{out}{err}"
            );
            assert_eq!(v["acme CLAUDE_CONFIG_DIR"], w.acme_claude, "{out}{err}");
            assert_eq!(v["acme CODEX_HOME"], w.acme_codex);
            assert_eq!(v["acme SWITCHBOARD_CONTEXT"], "org:o-acme");
            assert_eq!(v["globex CLAUDE_CONFIG_DIR"], w.globex_claude);
            assert_eq!(v["globex CODEX_HOME"], "unset");
            assert_eq!(v["globex SWITCHBOARD_CONTEXT"], "org:o-globex");
            assert_eq!(v["mine CLAUDE_CONFIG_DIR"], "/own/claude dir");
            assert_eq!(v["mine CODEX_HOME"], "unset");
            assert_eq!(v["mine SWITCHBOARD_CONTEXT"], "unset");
            assert!(err.is_empty(), "{err}");
            // Once at load and once per change of location, never more.
            assert_eq!(calls(&w), 4);

            // A value the person set inside is theirs: never restored over.
            let w = world();
            let body = [
                cd(&w.acme),
                "$env:CLAUDE_CONFIG_DIR = '/theirs'".to_owned(),
                cd(&w.acme.join("src")),
                show("inside"),
                cd(&w.mine),
                show("mine"),
            ]
            .join("\n");
            let (out, err) = pwsh_run(&w, pwsh, &[], &body);
            let v = seen(&out);
            assert_eq!(v["inside CLAUDE_CONFIG_DIR"], "/theirs", "{out}{err}");
            assert_eq!(v["inside CODEX_HOME"], w.acme_codex);
            assert_eq!(v["mine CLAUDE_CONFIG_DIR"], "/theirs");
            assert_eq!(v["mine CODEX_HOME"], "unset");

            // A launched session's shell is never touched, and Switchboard is never asked.
            let w = world();
            let body = [
                cd(&w.acme),
                show("acme"),
                "if ($env:SWITCHBOARD_SHELL_HOOK) { 'loaded' }".to_owned(),
            ]
            .join("\n");
            let (out, err) = pwsh_run(
                &w,
                pwsh,
                &[("SWITCHBOARD_SESSION", "managed:claude:acme")],
                &body,
            );
            assert_eq!(seen(&out)["acme CLAUDE_CONFIG_DIR"], "unset", "{out}{err}");
            assert!(!out.contains("loaded"), "{out}");
            assert_eq!(calls(&w), 0);

            // A shell started inside an organization folder gives the home back on leaving.
            let w = world();
            let body = [cd(&w.mine), show("mine")].join("\n");
            let (out, err) = pwsh_run(
                &w,
                pwsh,
                &[
                    ("CLAUDE_CONFIG_DIR", &w.acme_claude),
                    ("CODEX_HOME", &w.acme_codex),
                    ("SWITCHBOARD_CONTEXT", "org:o-acme"),
                ],
                &body,
            );
            let v = seen(&out);
            assert_eq!(v["mine CLAUDE_CONFIG_DIR"], "unset", "{out}{err}");
            assert_eq!(v["mine CODEX_HOME"], "unset");
            assert_eq!(v["mine SWITCHBOARD_CONTEXT"], "unset");

            // An answer of another form changes nothing, and the person's prompt still sees the
            // last command's status and exit code.
            let w = world();
            std::fs::write(
                &w.cli,
                "#!/bin/sh\necho 'acme\torg:o-acme'\necho noise >&2\nexit 3\n",
            )
            .unwrap();
            let body = [
                "function global:prompt { \"ok=$? code=$LASTEXITCODE\" }".to_owned(),
                // Read again, so the wrapper wraps this prompt.
                "Remove-Variable -Name _switchboard_shell -Scope Global".to_owned(),
                format!(
                    ". {}",
                    ps_quote(&scripts::hook_path(&w.data, Kind::Pwsh).to_string_lossy())
                ),
                format!(
                    "Set-Location -LiteralPath {}",
                    ps_quote(&w.acme.to_string_lossy())
                ),
                "/bin/sh -c 'exit 7'".to_owned(),
                "Get-Item -LiteralPath '/no/such/item' -ErrorAction SilentlyContinue".to_owned(),
                "prompt".to_owned(),
                show("acme"),
            ]
            .join("\n");
            let (out, err) = pwsh_run(&w, pwsh, &[("CLAUDE_CONFIG_DIR", "/own")], &body);
            let v = seen(&out);
            assert_eq!(v["acme CLAUDE_CONFIG_DIR"], "/own", "{out}{err}");
            assert_eq!(v["acme SWITCHBOARD_CONTEXT"], "unset");
            assert!(out.contains("ok=False code=7"), "{out}{err}");
            assert!(!err.contains("noise"), "{err}");
        }

        pub fn run() {
            let Some(pwsh) = pwsh() else {
                eprintln!(
                    "skipped: pwsh (PowerShell 7) is not installed on this machine; the live check on Windows is SB-02"
                );
                return;
            };
            run_all(&pwsh);
        }
    }
}
