//! The real [`Backend`]: the file-based login keychain through `SecItem*`, with an explicit
//! `SecAccess` on every item Switchboard creates. Why the file-based keychain and not the
//! data protection keychain: docs/KEYCHAIN.md.
//!
//! `ask: false` runs with `SecKeychainSetUserInteractionAllowed(false)`, so Keychain returns
//! an error instead of showing a dialog. That switch is process-wide; every operation here
//! holds one lock so a quiet read never overlaps an operation that may ask.
use crate::keychain::{Backend, Build, Create, Read, Removal};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
    url::CFURL,
};
use security_framework::os::macos::{
    code_signing::{Flags, SecCode, SecRequirement, SecStaticCode},
    keychain::SecKeychain,
};
use security_framework_sys::{
    base::SecAccessRef,
    item::{
        kSecAttrAccount, kSecAttrLabel, kSecAttrService, kSecClass, kSecClassGenericPassword,
        kSecMatchLimit, kSecMatchSearchList, kSecReturnData, kSecUseKeychain, kSecValueData,
    },
    keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate},
};
use std::{
    ffi::{c_void, CString},
    os::{raw::c_char, unix::ffi::OsStrExt},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[repr(C)]
struct OpaqueTrustedApplication(c_void);

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecAttrAccess: CFStringRef;
    static kSecMatchLimitOne: CFStringRef;
    // Deprecated in the macOS 10.10 SDK and still the only way to give a file-based keychain
    // item an access list (Apple: SecAccessCreate, kSecAttrAccess; fetched 2026-10-01).
    fn SecAccessCreate(
        descriptor: CFStringRef,
        trusted: CFArrayRef,
        access: *mut SecAccessRef,
    ) -> i32;
    fn SecTrustedApplicationCreateFromPath(
        path: *const c_char,
        app: *mut *mut OpaqueTrustedApplication,
    ) -> i32;
}

/// The release team. A fork signs with its own team by setting this at build time.
const SIGNING_TEAM: &str = match option_env!("SWITCHBOARD_SIGNING_TEAM") {
    Some(team) => team,
    None => "KJ35UYYL22",
};
/// Where the release is installed, and the CLI it carries (`build_macos.py`).
const INSTALLED_APP: &str = "/Applications/Fabric Switchboard.app";
const BUNDLED_CLI: &str = "Contents/MacOS/switchboard";
/// What macOS shows in a dialog about one of these items.
const DESCRIPTOR: &str = "Fabric Switchboard account";

const ERR_ITEM_NOT_FOUND: i32 = -25300;
const ERR_DUPLICATE_ITEM: i32 = -25299;
/// Codes that mean "Keychain wants (or was refused) a person's consent". With user
/// interaction disabled, measured on macOS 26.6.2 in a throwaway keychain: an untrusted read
/// returns errSecAuthFailed, an untrusted delete errSecInvalidOwnerEdit.
const REFUSED: [i32; 4] = [
    -25293, // errSecAuthFailed
    -25308, // errSecInteractionNotAllowed (also: keychain locked)
    -25244, // errSecInvalidOwnerEdit
    -128,   // errSecUserCanceled
];

static KEYCHAIN: Mutex<()> = Mutex::new(());

/// Which executables an item created by this build trusts, besides the caller itself.
pub(crate) enum Trust {
    CallerOnly,
    Executables(Vec<PathBuf>),
}

pub(crate) struct MacKeychain {
    /// `None`: the user's default keychain and search list. Tests pass a throwaway file.
    file: Option<SecKeychain>,
    trust: Trust,
}

impl MacKeychain {
    pub(crate) fn detect() -> (Build, Self) {
        let (build, trust) = detect_build();
        (build, Self { file: None, trust })
    }
    #[cfg(test)]
    pub(crate) fn in_file(file: SecKeychain, trust: Trust) -> Self {
        Self {
            file: Some(file),
            trust,
        }
    }

    fn query(&self, service: &str, account: &str) -> Vec<(CFString, CFType)> {
        let mut pairs = unsafe {
            vec![
                (
                    CFString::wrap_under_get_rule(kSecClass),
                    CFType::wrap_under_get_rule(kSecClassGenericPassword as CFTypeRef),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrService),
                    CFString::new(service).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrAccount),
                    CFString::new(account).as_CFType(),
                ),
            ]
        };
        if let Some(file) = &self.file {
            pairs.push(unsafe {
                (
                    CFString::wrap_under_get_rule(kSecMatchSearchList),
                    CFArray::from_CFTypes(std::slice::from_ref(file)).as_CFType(),
                )
            });
        }
        pairs
    }

    /// The item's access list: the caller plus every trusted executable that still exists.
    fn access(&self) -> Option<CFType> {
        let mut paths: Vec<Option<&Path>> = vec![None];
        if let Trust::Executables(list) = &self.trust {
            paths.extend(list.iter().map(|p| Some(p.as_path())));
        }
        let mut apps = Vec::new();
        for path in paths {
            let c_path = match path {
                None => None,
                Some(p) => Some(CString::new(p.as_os_str().as_bytes()).ok()?),
            };
            let mut app = std::ptr::null_mut();
            let status = unsafe {
                SecTrustedApplicationCreateFromPath(
                    c_path.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
                    &mut app,
                )
            };
            match (status, path) {
                (0, _) if !app.is_null() => {
                    apps.push(unsafe { CFType::wrap_under_create_rule(app as CFTypeRef) })
                }
                // The caller must always be trusted; a vanished sibling is simply left out.
                (_, None) => return None,
                (_, Some(_)) => {}
            }
        }
        let list = CFArray::from_CFTypes(&apps);
        let descriptor = CFString::new(DESCRIPTOR);
        let mut access: SecAccessRef = std::ptr::null_mut();
        let status = unsafe {
            SecAccessCreate(
                descriptor.as_concrete_TypeRef(),
                list.as_concrete_TypeRef(),
                &mut access,
            )
        };
        (status == 0 && !access.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(access as CFTypeRef) })
    }
}

/// The module lock and, unless the operation may ask, Keychain's "no dialogs" switch.
/// Field order matters: interaction is re-enabled before the lock is released.
struct Held {
    _silence: Option<security_framework::os::macos::keychain::KeychainUserInteractionLock>,
    _guard: std::sync::MutexGuard<'static, ()>,
}
fn quiet(ask: bool) -> Option<Held> {
    let guard = KEYCHAIN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let silence = if ask {
        None
    } else {
        Some(SecKeychain::disable_user_interaction().ok()?)
    };
    Some(Held {
        _silence: silence,
        _guard: guard,
    })
}

impl Backend for MacKeychain {
    fn read(&self, service: &str, account: &str, ask: bool) -> Read {
        let Some(_held) = quiet(ask) else {
            return Read::Failed;
        };
        let mut pairs = self.query(service, account);
        unsafe {
            pairs.push((
                CFString::wrap_under_get_rule(kSecReturnData),
                CFBoolean::true_value().as_CFType(),
            ));
            pairs.push((
                CFString::wrap_under_get_rule(kSecMatchLimit),
                CFString::wrap_under_get_rule(kSecMatchLimitOne).as_CFType(),
            ));
        }
        let query = CFDictionary::from_CFType_pairs(&pairs);
        let mut result: CFTypeRef = std::ptr::null();
        let status = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) };
        match status {
            0 if !result.is_null() => {
                let data = unsafe { CFType::wrap_under_create_rule(result) };
                match data.downcast::<CFData>() {
                    Some(data) => Read::Found(data.bytes().to_vec()),
                    None => Read::Failed,
                }
            }
            ERR_ITEM_NOT_FOUND => Read::Absent,
            code if REFUSED.contains(&code) => Read::Refused,
            _ => Read::Failed,
        }
    }

    fn create(&self, service: &str, account: &str, data: &[u8]) -> Create {
        let Some(_held) = quiet(false) else {
            return Create::Failed;
        };
        let Some(access) = self.access() else {
            return Create::Failed;
        };
        let mut pairs = self.query(service, account);
        // An add takes the target keychain, not a search list.
        pairs.retain(|(key, _)| {
            key != unsafe { &CFString::wrap_under_get_rule(kSecMatchSearchList) }
        });
        unsafe {
            pairs.push((
                CFString::wrap_under_get_rule(kSecValueData),
                CFData::from_buffer(data).as_CFType(),
            ));
            pairs.push((
                CFString::wrap_under_get_rule(kSecAttrLabel),
                CFString::new(service).as_CFType(),
            ));
            pairs.push((CFString::wrap_under_get_rule(kSecAttrAccess), access));
            if let Some(file) = &self.file {
                pairs.push((
                    CFString::wrap_under_get_rule(kSecUseKeychain),
                    file.as_CFType(),
                ));
            }
        }
        let attributes = CFDictionary::from_CFType_pairs(&pairs);
        match unsafe { SecItemAdd(attributes.as_concrete_TypeRef(), std::ptr::null_mut()) } {
            0 => Create::Created,
            ERR_DUPLICATE_ITEM => Create::Exists,
            _ => Create::Failed,
        }
    }

    fn update(&self, service: &str, account: &str, data: &[u8]) -> bool {
        let Some(_held) = quiet(false) else {
            return false;
        };
        let query = CFDictionary::from_CFType_pairs(&self.query(service, account));
        let change = unsafe {
            CFDictionary::from_CFType_pairs(&[(
                CFString::wrap_under_get_rule(kSecValueData),
                CFData::from_buffer(data).as_CFType(),
            )])
        };
        unsafe { SecItemUpdate(query.as_concrete_TypeRef(), change.as_concrete_TypeRef()) == 0 }
    }

    fn remove(&self, service: &str, account: &str, ask: bool) -> Removal {
        let Some(_held) = quiet(ask) else {
            return Removal::Failed;
        };
        let query = CFDictionary::from_CFType_pairs(&self.query(service, account));
        match unsafe { SecItemDelete(query.as_concrete_TypeRef()) } {
            0 => Removal::Removed,
            ERR_ITEM_NOT_FOUND => Removal::Absent,
            code if REFUSED.contains(&code) => Removal::Refused,
            _ => Removal::Failed,
        }
    }
}

fn team_requirement() -> Option<SecRequirement> {
    let valid = SIGNING_TEAM.len() == 10
        && SIGNING_TEAM
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    if !valid {
        return None;
    }
    format!("anchor apple generic and certificate leaf[subject.OU] = \"{SIGNING_TEAM}\"")
        .parse()
        .ok()
}

fn detect_build() -> (Build, Trust) {
    let Some(requirement) = team_requirement() else {
        return (Build::Development, Trust::CallerOnly);
    };
    let signed = SecCode::for_self(Flags::NONE)
        .and_then(|code| code.check_validity(Flags::NONE, &requirement))
        .is_ok();
    if !signed {
        return (Build::Development, Trust::CallerOnly);
    }
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).ok();
    let trusted = candidates(exe.as_deref())
        .into_iter()
        .filter(|path| signed_by_team(path, &requirement))
        .collect();
    (Build::Signed, Trust::Executables(trusted))
}

/// The app bundle and its CLI — the running one's and the installed one's.
pub(crate) fn candidates(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut bundles = Vec::new();
    if let Some(bundle) = exe.and_then(enclosing_app) {
        bundles.push(bundle);
    }
    bundles.push(PathBuf::from(INSTALLED_APP));
    let mut out = Vec::new();
    for bundle in bundles {
        for path in [bundle.join(BUNDLED_CLI), bundle] {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// `/x/Name.app/Contents/MacOS/exe` → `/x/Name.app`.
fn enclosing_app(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_owned())
}

fn signed_by_team(path: &Path, requirement: &SecRequirement) -> bool {
    let Some(url) = CFURL::from_path(path, path.is_dir()) else {
        return false;
    };
    path.exists()
        && SecStaticCode::from_path(&url, Flags::NONE)
            .and_then(|code| code.check_validity(Flags::BASIC_VALIDATE_ONLY, requirement))
            .is_ok()
}

/// Reads a generic-password item another program owns (Codex's "Codex Auth") from the default
/// keychain list without ever showing a dialog: an item that does not trust Switchboard reads
/// as refused. Ok(None): no such item. Spawning `/usr/bin/security` would instead make macOS
/// ask — that executable is not on such an item's access list either.
pub fn read_external_quietly(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    let backend = MacKeychain {
        file: None,
        trust: Trust::CallerOnly,
    };
    match backend.read(service, account, false) {
        Read::Found(data) => Ok(Some(data)),
        Read::Absent => Ok(None),
        Read::Refused => Err(EXTERNAL_REFUSED.into()),
        Read::Failed => Err("Keychain unavailable. Unlock it, then retry.".into()),
    }
}
/// Keychain answers the same way whether it is locked or the item does not trust Switchboard.
pub const EXTERNAL_REFUSED: &str =
    "Keychain is locked or does not let Switchboard read this sign-in without asking. Unlock it and retry, or add the account with official sign-in.";

#[cfg(test)]
mod tests {
    //! Live checks against a throwaway keychain file; the user's keychains are not touched
    //! and nothing here can show a dialog (every call is `ask: false`).
    use super::*;
    use crate::keychain::{Consent, Policy, LEGACY_SERVICE, SHARED_SERVICE};
    use security_framework::os::macos::keychain::CreateOptions;

    struct Throwaway {
        _dir: tempfile::TempDir,
        path: PathBuf,
        keychain: SecKeychain,
    }
    impl Throwaway {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("switchboard-test.keychain-db");
            let mut keychain = CreateOptions::new()
                .password("synthetic-test-only")
                .create(&path)
                .unwrap();
            keychain.unlock(Some("synthetic-test-only")).unwrap();
            Self {
                _dir: dir,
                path,
                keychain,
            }
        }
        fn backend(&self, trust: Trust) -> MacKeychain {
            MacKeychain::in_file(self.keychain.clone(), trust)
        }
        /// ACL metadata only (`-a` without `-d` never prints a secret).
        fn acl(&self) -> String {
            let out = std::process::Command::new("/usr/bin/security")
                .args(["dump-keychain", "-a"])
                .arg(&self.path)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        }
    }
    impl Drop for Throwaway {
        fn drop(&mut self) {
            let _ = std::process::Command::new("/usr/bin/security")
                .arg("delete-keychain")
                .arg(&self.path)
                .output();
        }
    }
    const ID: &str = "0f6a4c1e-2b9d-4e0a-8f57-6c3d2b1a9e84";

    #[test]
    fn created_items_trust_the_listed_executables_by_requirement() {
        let kc = Throwaway::new();
        // Any existing signed executable stands in for the bundled CLI.
        let sibling = PathBuf::from("/usr/bin/true");
        let backend = kc.backend(Trust::Executables(vec![sibling.clone()]));
        assert_eq!(backend.create("svc", ID, b"fixture-only"), Create::Created);
        assert_eq!(backend.create("svc", ID, b"fixture-two"), Create::Exists);
        let acl = kc.acl();
        assert!(
            acl.contains("/usr/bin/true") && acl.contains("identifier \"com.apple.true\""),
            "the sibling must be in the item's access list"
        );
        let exe = std::env::current_exe().unwrap();
        assert!(acl.contains(exe.file_name().unwrap().to_str().unwrap()));
        assert_eq!(
            backend.read("svc", ID, false),
            Read::Found(b"fixture-only".to_vec())
        );
        assert!(backend.update("svc", ID, b"fixture-two"));
        assert_eq!(
            backend.read("svc", ID, false),
            Read::Found(b"fixture-two".to_vec())
        );
        assert_eq!(backend.remove("svc", ID, false), Removal::Removed);
        assert_eq!(backend.read("svc", ID, false), Read::Absent);
        assert_eq!(backend.remove("svc", ID, false), Removal::Absent);
    }

    #[test]
    fn an_untrusted_read_fails_closed_without_a_dialog() {
        let kc = Throwaway::new();
        let backend = kc.backend(Trust::CallerOnly);
        // An access list that trusts nobody: even the creator would need consent.
        let nobody = {
            let list = CFArray::<CFType>::from_CFTypes(&[]);
            let mut access: SecAccessRef = std::ptr::null_mut();
            let descriptor = CFString::new(DESCRIPTOR);
            assert_eq!(
                unsafe {
                    SecAccessCreate(
                        descriptor.as_concrete_TypeRef(),
                        list.as_concrete_TypeRef(),
                        &mut access,
                    )
                },
                0
            );
            unsafe { CFType::wrap_under_create_rule(access as CFTypeRef) }
        };
        let mut pairs = backend.query("svc", ID);
        pairs.retain(|(key, _)| {
            key != unsafe { &CFString::wrap_under_get_rule(kSecMatchSearchList) }
        });
        unsafe {
            pairs.push((
                CFString::wrap_under_get_rule(kSecValueData),
                CFData::from_buffer(b"fixture-only").as_CFType(),
            ));
            pairs.push((CFString::wrap_under_get_rule(kSecAttrAccess), nobody));
            pairs.push((
                CFString::wrap_under_get_rule(kSecUseKeychain),
                kc.keychain.as_CFType(),
            ));
            let attributes = CFDictionary::from_CFType_pairs(&pairs);
            assert_eq!(
                SecItemAdd(attributes.as_concrete_TypeRef(), std::ptr::null_mut()),
                0
            );
        }
        let started = std::time::Instant::now();
        assert_eq!(backend.read("svc", ID, false), Read::Refused);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(backend.remove("svc", ID, false), Removal::Removed);
    }

    #[test]
    fn a_legacy_item_moves_into_the_shared_namespace_and_verifies() {
        let kc = Throwaway::new();
        let backend = kc.backend(Trust::CallerOnly);
        // A legacy item as 0.4.0 wrote it: default access, trusting only its creator.
        kc.keychain
            .add_generic_password(LEGACY_SERVICE, ID, b"fixture-only")
            .unwrap();
        let policy = Policy::new(backend, Build::Signed, Consent::Never);
        assert_eq!(policy.get(ID).unwrap(), b"fixture-only");
        let check = kc.backend(Trust::CallerOnly);
        assert_eq!(check.read(LEGACY_SERVICE, ID, false), Read::Absent);
        assert_eq!(
            check.read(SHARED_SERVICE, ID, false),
            Read::Found(b"fixture-only".to_vec())
        );
        assert_eq!(policy.get(ID).unwrap(), b"fixture-only");
        policy.delete(ID).unwrap();
        assert_eq!(check.read(SHARED_SERVICE, ID, false), Read::Absent);
    }

    #[test]
    fn candidates_cover_the_running_bundle_and_the_installed_app() {
        let exe = Path::new(
            "/Users/example/Apps/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard",
        );
        assert_eq!(
            candidates(Some(exe)),
            [
                "/Users/example/Apps/Fabric Switchboard.app/Contents/MacOS/switchboard",
                "/Users/example/Apps/Fabric Switchboard.app",
                "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard",
                "/Applications/Fabric Switchboard.app",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            candidates(Some(Path::new("/opt/bin/switchboard"))),
            [
                "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard",
                "/Applications/Fabric Switchboard.app",
            ]
            .map(PathBuf::from)
        );
    }

    /// Signed-bundle acceptance, run by hand from Developer ID signed copies of this test
    /// binary laid out as an app bundle (procedure: docs/KEYCHAIN.md). Never in the gate:
    /// it needs the release signing identity.
    #[test]
    #[ignore = "needs Developer ID signed copies of this binary; see docs/KEYCHAIN.md"]
    fn signed_bundle_acceptance() {
        let path = std::env::var_os("SWITCHBOARD_ACCEPTANCE_KEYCHAIN").expect("keychain path");
        let role = std::env::var("SWITCHBOARD_ACCEPTANCE_ROLE").expect("write|read|refused");
        let mut keychain = SecKeychain::open(PathBuf::from(path)).unwrap();
        keychain.unlock(Some("synthetic-test-only")).unwrap();
        let (build, trust) = detect_build();
        let trusted = match &trust {
            Trust::CallerOnly => 0,
            Trust::Executables(list) => list.len(),
        };
        eprintln!("build={build:?} trusted_siblings={trusted}");
        let backend = MacKeychain::in_file(keychain, trust);
        match role.as_str() {
            "write" => {
                assert_eq!(build, Build::Signed);
                assert!(trusted >= 2, "the bundle and its CLI");
                assert_eq!(
                    backend.create(SHARED_SERVICE, ID, b"fixture-only"),
                    Create::Created
                );
            }
            "read" => assert_eq!(
                backend.read(SHARED_SERVICE, ID, false),
                Read::Found(b"fixture-only".to_vec())
            ),
            "refused" => assert_eq!(backend.read(SHARED_SERVICE, ID, false), Read::Refused),
            other => panic!("unknown role {other}"),
        }
    }

    #[test]
    fn unsigned_test_binaries_are_development_builds() {
        assert_eq!(detect_build().0, Build::Development);
    }
}
