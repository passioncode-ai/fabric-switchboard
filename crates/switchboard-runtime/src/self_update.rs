//! SB-305: updating a computer that runs only `switchboard serve`.
//!
//! The installed app checks for updates itself (`src-tauri/src/updates.rs`); a headless install
//! (a macOS user with no graphical login running `serve` under a LaunchDaemon, SB-287; a Linux
//! VM) never opened the app, so it stayed on the version it was installed with. `switchboard
//! update` does the app's check from the CLI, run by whoever owns the install: it reads the same
//! `latest.json`, offers only a newer version, refuses a release marked `needs_migration`, checks
//! the archive's minisign signature against the public key the app ships and the version signed
//! into it, then replaces the app bundle (macOS) or AppImage (Linux) with one rename. A `serve`
//! already running keeps the old binary until it is restarted.

use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The feed the app reads (`plugins.updater.endpoints` in `src-tauri/tauri.conf.json`).
pub const FEED: &str =
    "https://github.com/passioncode-ai/fabric-switchboard/releases/latest/download/latest.json";
/// The app's updater public key (`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`); a
/// test keeps the two equal.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU3MTg1MDlCMTQ0MUI4OTcKUldTWHVFRVVtMUFZNXc5Ny9aeHFHYmFLdUhBVU9lRG4vVzlGZ3VUR1hVd0hueWtXL1FFdG1WT0QK";

/// The feed is a few kilobytes; the archive about 30 MB.
const FEED_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
const ARCHIVE_WITHIN: std::time::Duration = std::time::Duration::from_secs(600);
pub const FEED_UNREADABLE: &str =
    "Could not read the release feed. Check the network and try again.";
pub const NO_PACKAGE: &str = "The release has no package for this computer.";
pub const HELD: &str = "This version needs a step by a person before it installs; its release notes say what to do. Nothing was changed.";
pub const DOWNLOAD_FAILED: &str = "Could not download the update. Nothing was changed.";
pub const SIGNATURE_INVALID: &str =
    "The update's signature does not match Switchboard's key. Nothing was changed.";
pub const NOT_AN_APP: &str =
    "Name the installed app: the Fabric Switchboard.app folder on macOS, the AppImage on Linux.";
pub const NOT_WRITABLE: &str =
    "This account cannot replace the installed app. Run the update as the account that owns it.";
pub const INSTALL_FAILED: &str =
    "Could not install the update. The installed app was left as it was.";
pub const PLATFORM_UNSUPPORTED: &str =
    "Here the app updates itself; this command is for a computer that runs only serve.";

/// The feed's key for this computer's package, or none where the CLI does not update.
pub fn platform_key() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("darwin-universal")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-aarch64")
    } else {
        None
    }
}

/// One release this computer may install.
#[derive(Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub url: String,
    pub signature: String,
}

/// What the feed offers this computer: `None` when its version is not newer than `current`.
pub fn choose(feed: &Value, current: &str, key: &str) -> Result<Option<Offer>, String> {
    let announced = feed["version"]
        .as_str()
        .and_then(|v| semver::Version::parse(v.trim_start_matches('v')).ok())
        .ok_or(FEED_UNREADABLE)?;
    let current = semver::Version::parse(current).map_err(|_| FEED_UNREADABLE)?;
    if announced <= current {
        return Ok(None);
    }
    // LC-16: a release that needs a person first is never installed by a program.
    if feed["needs_migration"]
        .as_str()
        .is_some_and(|steps| !steps.trim().is_empty())
    {
        return Err(HELD.into());
    }
    let package = &feed["platforms"][key];
    let (Some(url), Some(signature)) = (package["url"].as_str(), package["signature"].as_str())
    else {
        return Err(NO_PACKAGE.into());
    };
    if !url.starts_with("https://") {
        return Err(FEED_UNREADABLE.into());
    }
    Ok(Some(Offer {
        version: announced.to_string(),
        url: url.into(),
        signature: signature.into(),
    }))
}

fn base64_text(value: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| SIGNATURE_INVALID.to_string())?;
    String::from_utf8(bytes).map_err(|_| SIGNATURE_INVALID.into())
}

/// The archive is the one Switchboard's release signed for `version`: the minisign signature
/// verifies against `public_key`, and the version in its signed comment is `version`, so an
/// unsigned feed cannot pair a new number with an older package (the app's
/// `requireSignedVersion`).
pub fn verify(data: &[u8], signature: &str, public_key: &str, version: &str) -> Result<(), String> {
    let key = minisign_verify::PublicKey::decode(&base64_text(public_key)?)
        .map_err(|_| SIGNATURE_INVALID.to_string())?;
    let signature = minisign_verify::Signature::decode(&base64_text(signature)?)
        .map_err(|_| SIGNATURE_INVALID.to_string())?;
    key.verify(data, &signature, true)
        .map_err(|_| SIGNATURE_INVALID.to_string())?;
    let signed = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"));
    if signed != Some(version) {
        return Err(SIGNATURE_INVALID.into());
    }
    Ok(())
}

/// The app bundle a binary runs from: the nearest folder above it named `*.app`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// A folder beside `target` for staging, removed on drop: renames stay on one file system.
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn staging(target: &Path) -> Result<Staging, String> {
    let parent = target.parent().ok_or(NOT_AN_APP)?;
    let dir = parent.join(format!(".fabric-switchboard-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => NOT_WRITABLE.to_string(),
        _ => INSTALL_FAILED.to_string(),
    })?;
    Ok(Staging(dir))
}

/// The rest of an archive path below its top folder, only when every part stays inside it (no
/// `..`, no root, no prefix), checked before anything is written.
fn inside(parts: std::path::Components<'_>) -> Option<PathBuf> {
    let mut inner = PathBuf::new();
    for part in parts {
        match part {
            std::path::Component::Normal(name) => inner.push(name),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(inner)
}

/// Replaces the app bundle `bundle` with the one in the updater archive (`.app.tar.gz`, one
/// top-level `*.app` folder): extracted beside it, then moved in with the old bundle kept until
/// the move succeeded and put back if it did not.
pub fn install_bundle(archive: &[u8], bundle: &Path) -> Result<(), String> {
    if !bundle.is_dir() || bundle.extension().is_none_or(|e| e != "app") {
        return Err(NOT_AN_APP.into());
    }
    let stage = staging(bundle)?;
    let new = stage.0.join("new.app");
    std::fs::create_dir(&new).map_err(|_| INSTALL_FAILED.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let entries = tar.entries().map_err(|_| INSTALL_FAILED.to_string())?;
    let mut found = false;
    for entry in entries {
        let mut entry = entry.map_err(|_| INSTALL_FAILED.to_string())?;
        let path = entry
            .path()
            .map_err(|_| INSTALL_FAILED.to_string())?
            .into_owned();
        let mut parts = path.components();
        let top = parts.next();
        if !top.is_some_and(|t| {
            Path::new(t.as_os_str())
                .extension()
                .is_some_and(|e| e == "app")
        }) {
            return Err(INSTALL_FAILED.into());
        }
        let Some(inner) = inside(parts) else {
            return Err(INSTALL_FAILED.into());
        };
        if inner.as_os_str().is_empty() {
            continue;
        }
        let target = new.join(&inner);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| INSTALL_FAILED.to_string())?;
        }
        entry
            .unpack(&target)
            .map_err(|_| INSTALL_FAILED.to_string())?;
        found = true;
    }
    if !found || !new.join("Contents").is_dir() {
        return Err(INSTALL_FAILED.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    let old = stage.0.join("old.app");
    std::fs::rename(bundle, &old).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => NOT_WRITABLE.to_string(),
        _ => INSTALL_FAILED.to_string(),
    })?;
    if std::fs::rename(&new, bundle).is_err() {
        let _ = std::fs::rename(&old, bundle);
        return Err(INSTALL_FAILED.into());
    }
    Ok(())
}

/// Replaces the AppImage at `image` with `data`: written beside it, made executable, renamed over.
pub fn install_appimage(data: &[u8], image: &Path) -> Result<(), String> {
    if !image.is_file() {
        return Err(NOT_AN_APP.into());
    }
    let stage = staging(image)?;
    let new = stage.0.join("new.AppImage");
    std::fs::write(&new, data).map_err(|_| INSTALL_FAILED.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
            .map_err(|_| INSTALL_FAILED.to_string())?;
    }
    std::fs::rename(&new, image).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => NOT_WRITABLE.to_string(),
        _ => INSTALL_FAILED.to_string(),
    })
}

async fn fetch(
    client: &reqwest::Client,
    url: &str,
    within: std::time::Duration,
    error: &str,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .timeout(within)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| error.to_string())?;
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|_| error.to_string())
}

/// Files fetched by other means (`gh release download` where github.com refuses anonymous
/// downloads): the release's `latest.json` and its package. They are checked exactly as
/// downloaded ones are.
pub struct Local {
    pub feed: PathBuf,
    pub archive: PathBuf,
}
pub const LOCAL_UNREADABLE: &str = "Could not read the release files given. Name the release's latest.json and its package.";

/// `switchboard update`: what the feed offers this computer and, unless `check`, its install
/// into `app` (default: the bundle or AppImage this binary runs from).
pub async fn run(
    app: Option<PathBuf>,
    check: bool,
    current: &str,
    local: Option<Local>,
) -> Result<Value, String> {
    let key = platform_key().ok_or(PLATFORM_UNSUPPORTED)?;
    let client = reqwest::Client::builder()
        .user_agent("fabric-switchboard-cli-updater")
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| FEED_UNREADABLE.to_string())?;
    let feed_bytes = match &local {
        Some(local) => std::fs::read(&local.feed).map_err(|_| LOCAL_UNREADABLE.to_string())?,
        None => fetch(&client, FEED, FEED_WITHIN, FEED_UNREADABLE).await?,
    };
    let feed: Value =
        serde_json::from_slice(&feed_bytes).map_err(|_| FEED_UNREADABLE.to_string())?;
    let Some(offer) = choose(&feed, current, key)? else {
        return Ok(json!({"current": current, "latest": feed["version"], "action": "up_to_date"}));
    };
    let target = match app {
        Some(path) => path,
        None => {
            let exe = std::env::current_exe().map_err(|_| NOT_AN_APP.to_string())?;
            if cfg!(target_os = "macos") {
                bundle_of(&exe).ok_or(NOT_AN_APP)?
            } else {
                std::env::var_os("APPIMAGE")
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from)
                    .ok_or(NOT_AN_APP)?
            }
        }
    };
    if check {
        return Ok(json!({
            "current": current, "latest": offer.version, "action": "available",
            "app": target.to_string_lossy(),
        }));
    }
    let archive = match &local {
        Some(local) => std::fs::read(&local.archive).map_err(|_| LOCAL_UNREADABLE.to_string())?,
        None => fetch(&client, &offer.url, ARCHIVE_WITHIN, DOWNLOAD_FAILED).await?,
    };
    verify(&archive, &offer.signature, PUBLIC_KEY, &offer.version)?;
    let place = target.clone();
    let installed = tokio::task::spawn_blocking(move || {
        if cfg!(target_os = "macos") {
            install_bundle(&archive, &place)
        } else {
            install_appimage(&archive, &place)
        }
    })
    .await
    .map_err(|_| INSTALL_FAILED.to_string())?;
    installed?;
    crate::oplog::event(
        "update_install",
        &[("result", crate::oplog::Field::Code("installed_by_cli"))],
    );
    Ok(json!({
        "current": current, "latest": offer.version, "action": "installed",
        "app": target.to_string_lossy(), "restart_needed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::io::Write;

    const KEYNUM: [u8; 8] = [7, 6, 5, 4, 3, 2, 1, 0];

    /// A minisign key pair, and signatures made with it in minisign's legacy (`Ed`) form, so the
    /// check runs the real verifier on a real signature with no release involved.
    fn signer() -> (Ed25519KeyPair, String) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let mut key = b"Ed".to_vec();
        key.extend_from_slice(&KEYNUM);
        key.extend_from_slice(pair.public_key().as_ref());
        let b64 = base64::engine::general_purpose::STANDARD;
        let text = format!("untrusted comment: test key\n{}\n", b64.encode(key));
        (pair, b64.encode(text))
    }
    fn sign(pair: &Ed25519KeyPair, data: &[u8], version: &str) -> String {
        let b64 = base64::engine::general_purpose::STANDARD;
        let sig = pair.sign(data);
        let mut line = b"Ed".to_vec();
        line.extend_from_slice(&KEYNUM);
        line.extend_from_slice(sig.as_ref());
        let comment = format!("timestamp:1\tfile:app.tar.gz\tversion:{version}");
        let mut global = sig.as_ref().to_vec();
        global.extend_from_slice(comment.as_bytes());
        let text = format!(
            "untrusted comment: test\n{}\ntrusted comment: {comment}\n{}\n",
            b64.encode(line),
            b64.encode(pair.sign(&global).as_ref())
        );
        b64.encode(text)
    }

    #[test]
    fn the_cli_and_the_app_check_against_one_key() {
        let conf: Value =
            serde_json::from_str(include_str!("../../../src-tauri/tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["pubkey"], PUBLIC_KEY);
        assert_eq!(conf["plugins"]["updater"]["endpoints"][0], FEED);
    }

    #[test]
    fn only_a_newer_release_with_a_package_here_is_offered() {
        let feed = |version: &str| {
            json!({"version": version, "platforms": {"darwin-universal": {
                "url": "https://example.invalid/app.tar.gz", "signature": "c2ln"}}})
        };
        assert_eq!(
            choose(&feed("0.7.33"), "0.7.33", "darwin-universal"),
            Ok(None)
        );
        assert_eq!(
            choose(&feed("0.7.32"), "0.7.33", "darwin-universal"),
            Ok(None)
        );
        let offer = choose(&feed("0.7.34"), "0.7.33", "darwin-universal")
            .unwrap()
            .unwrap();
        assert_eq!(offer.version, "0.7.34");
        assert_eq!(
            choose(&feed("0.7.34"), "0.7.33", "linux-x86_64").unwrap_err(),
            NO_PACKAGE
        );
        let mut held = feed("0.7.34");
        held["needs_migration"] = json!("run the migration first");
        assert_eq!(
            choose(&held, "0.7.33", "darwin-universal").unwrap_err(),
            HELD
        );
        let mut plain = feed("0.7.34");
        plain["platforms"]["darwin-universal"]["url"] = json!("http://example.invalid/app");
        assert_eq!(
            choose(&plain, "0.7.33", "darwin-universal").unwrap_err(),
            FEED_UNREADABLE
        );
        assert_eq!(
            choose(&json!({}), "0.7.33", "darwin-universal").unwrap_err(),
            FEED_UNREADABLE
        );
    }

    #[test]
    fn a_package_installs_only_with_its_signature_and_signed_version() {
        let (pair, key) = signer();
        let data = b"the archive";
        let signature = sign(&pair, data, "0.7.34");
        assert_eq!(verify(data, &signature, &key, "0.7.34"), Ok(()));
        assert_eq!(
            verify(b"another archive", &signature, &key, "0.7.34").unwrap_err(),
            SIGNATURE_INVALID
        );
        assert_eq!(
            verify(data, &signature, &key, "0.7.35").unwrap_err(),
            SIGNATURE_INVALID,
            "a feed cannot pair a new number with an older package"
        );
        let (_, other) = signer();
        assert_eq!(
            verify(data, &signature, &other, "0.7.34").unwrap_err(),
            SIGNATURE_INVALID
        );
        assert_eq!(
            verify(data, "not base64", &key, "0.7.34").unwrap_err(),
            SIGNATURE_INVALID
        );
    }

    /// A published release checks with the app's key: `SB305_FEED` and `SB305_ARCHIVE` name its
    /// `latest.json` and its macOS archive. Ignored by default: it needs the files downloaded.
    #[test]
    #[ignore]
    fn a_published_release_verifies_with_the_apps_key() {
        let feed: Value =
            serde_json::from_slice(&std::fs::read(std::env::var("SB305_FEED").unwrap()).unwrap())
                .unwrap();
        let data = std::fs::read(std::env::var("SB305_ARCHIVE").unwrap()).unwrap();
        let version = feed["version"].as_str().unwrap();
        let signature = feed["platforms"]["darwin-universal"]["signature"]
            .as_str()
            .unwrap();
        assert_eq!(verify(&data, signature, PUBLIC_KEY, version), Ok(()));
        assert!(verify(&data[1..], signature, PUBLIC_KEY, version).is_err());
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Fabric Switchboard.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        install_bundle(&data, &bundle).unwrap();
        assert!(bundle.join("Contents/MacOS/switchboard").is_file());
    }

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn the_bundle_is_replaced_in_one_move_and_kept_when_the_archive_is_bad() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Fabric Switchboard.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/switchboard"), "old").unwrap();
        assert_eq!(
            bundle_of(&bundle.join("Contents/MacOS/switchboard")),
            Some(bundle.clone())
        );
        assert_eq!(bundle_of(Path::new("/usr/local/bin/switchboard")), None);
        // A bad archive leaves the installed bundle as it was, and no staging folder behind.
        assert!(install_bundle(b"not a tar.gz", &bundle).is_err());
        assert!(inside(Path::new("../../escaped").components()).is_none());
        assert!(inside(Path::new("/etc/passwd").components()).is_none());
        assert_eq!(
            inside(Path::new("Contents/./MacOS/switchboard").components()),
            Some(PathBuf::from("Contents/MacOS/switchboard"))
        );
        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/MacOS/switchboard")).unwrap(),
            "old"
        );
        let good = archive(&[
            ("Fabric Switchboard.app/Contents/MacOS/switchboard", b"new"),
            ("Fabric Switchboard.app/Contents/Info.plist", b"<plist/>"),
        ]);
        install_bundle(&good, &bundle).unwrap();
        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/MacOS/switchboard")).unwrap(),
            "new"
        );
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            left,
            [std::ffi::OsString::from("Fabric Switchboard.app")],
            "{left:?}"
        );
        assert_eq!(
            install_bundle(&good, &dir.path().join("missing.app")).unwrap_err(),
            NOT_AN_APP
        );
    }

    #[test]
    fn an_appimage_is_replaced_and_stays_executable() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("Fabric-Switchboard.AppImage");
        std::fs::File::create(&image)
            .unwrap()
            .write_all(b"old")
            .unwrap();
        install_appimage(b"new", &image).unwrap();
        assert_eq!(std::fs::read(&image).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&image).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            install_appimage(b"new", &dir.path().join("none")).unwrap_err(),
            NOT_AN_APP
        );
    }
}
