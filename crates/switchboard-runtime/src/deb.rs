//! SB-408 (studio-agent request 2b5ffe83, 2026-11-17): a copy installed from the .deb package
//! says when it is behind.
//!
//! The package manager owns a .deb install's files, so Switchboard never replaces them
//! (`docs/INSTALL.md`, SB-88). Instead `switchboard update` (with or without `--check`), the
//! window's Settings and a `serve` read the release feed and say which version is out and the
//! exact commands that download, verify and install it; nothing is installed.
//!
//! A .deb install is recognised as dpkg recognises it, without root and without starting `dpkg`:
//! the program's path is a line of the package's file list, `/var/lib/dpkg/info/
//! fabric-switchboard[:<arch>].list` (what `dpkg -S <path>` reads). An AppImage is never one.
use serde_json::{json, Value};
use std::path::Path;

/// Where dpkg keeps each package's file list.
pub const DPKG_INFO: &str = "/var/lib/dpkg/info";
/// The package's name, as the release's .deb installs it (`productName`, lowercased).
pub const PACKAGE: &str = "fabric-switchboard";
const RELEASES: &str = "https://github.com/passioncode-ai/fabric-switchboard/releases";
/// The organization's release key that signs `SHA256SUMS` (`SHA256SUMS.asc`).
pub const RELEASE_KEY: &str = "https://raw.githubusercontent.com/passioncode-ai/.github/main/release-signing/passioncode-release-signing.asc";

/// Whether `exe` is a file of the package, by its list in `info`. `exe` is also matched as
/// resolved, so a program started through a link is recognised.
pub fn owned_by_package(exe: &Path, info: &Path) -> bool {
    let resolved = std::fs::canonicalize(exe).ok();
    let names: Vec<String> = [Some(exe.to_owned()), resolved]
        .into_iter()
        .flatten()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let Ok(entries) = std::fs::read_dir(info) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let ours = name == format!("{PACKAGE}.list")
            || (name.starts_with(&format!("{PACKAGE}:")) && name.ends_with(".list"));
        ours && std::fs::read_to_string(entry.path())
            .is_ok_and(|list| list.lines().any(|line| names.iter().any(|n| n == line)))
    })
}

/// Whether this process runs from a .deb install: Linux, not inside an AppImage, its program in
/// the package's file list.
pub fn this_install() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    if std::env::var_os("APPIMAGE").is_some_and(|image| !image.is_empty()) {
        return false;
    }
    std::env::current_exe().is_ok_and(|exe| owned_by_package(&exe, Path::new(DPKG_INFO)))
}

/// The release's word for this computer's architecture in the package's file name.
pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

/// The PassionCode.ai release key's fingerprint: `SHA256SUMS.asc` counts only when a good
/// signature names this primary key (gpg's `VALIDSIG` status line ends with it).
pub const RELEASE_KEY_FINGERPRINT: &str = "63B30DC324BD697487AA31944FAFB8AEC803B6A7";

/// What the steps run besides coreutils and apt: `curl` (absent from a minimal Debian) and
/// `gpgv` (installed with apt itself). `sudo apt install curl gpgv` adds them.
pub const PREREQUISITES: [&str; 2] = ["curl", "gpgv"];

/// The steps that fetch, verify and install `version`'s .deb for `arch`, in an empty folder: the
/// key goes into a keyring of its own (never the person's), the checksums count only with a good
/// signature by the pinned key, the package only with its checksum, then apt installs it.
pub fn commands(version: &str, arch: &str) -> Vec<String> {
    let base = format!("{RELEASES}/download/v{version}");
    let package = format!("Fabric-Switchboard-{version}-linux-{arch}.deb");
    vec![
        format!("mkdir -p ~/Downloads/fabric-switchboard-{version} && cd ~/Downloads/fabric-switchboard-{version}"),
        format!("curl -fLO {base}/{package}"),
        format!("curl -fLO {base}/SHA256SUMS"),
        format!("curl -fLO {base}/SHA256SUMS.asc"),
        format!("curl -fsSL {RELEASE_KEY} -o release-key.asc"),
        // The armour taken off with sed and base64 (byte for byte what `gpg --dearmor` writes), so
        // a minimal Debian needs no gpg: gpgv comes with apt.
        "sed -e '1,/^$/d' -e '/^=/,$d' release-key.asc | base64 -d > release.gpg".to_owned(),
        format!("gpgv --status-fd 1 --keyring ./release.gpg SHA256SUMS.asc SHA256SUMS | grep -Eq '^\\[GNUPG:\\] VALIDSIG .* {RELEASE_KEY_FINGERPRINT}$'"),
        "sha256sum -c SHA256SUMS --ignore-missing".to_owned(),
        format!("sudo apt install ./{package}"),
    ]
}

/// The steps as one command for a terminal: each runs only when the one before succeeded, so a
/// signature or checksum that fails never reaches `apt install`.
pub fn command(steps: &[String]) -> String {
    steps.join(" && \\\n")
}

/// What a .deb install is told, from the release feed: `{package: deb, current, latest, action:
/// up_to_date|available, release, commands?, needs_migration?}`. Nothing is installed.
pub fn notice(feed: &Value, current: &str, arch: &str) -> Result<Value, String> {
    let latest = feed["version"]
        .as_str()
        .map(|v| v.trim_start_matches('v').to_owned())
        .filter(|v| semver::Version::parse(v).is_ok())
        .ok_or(crate::self_update::FEED_UNREADABLE)?;
    if !crate::copies::older(current, &latest) {
        return Ok(json!({
            "package": "deb", "current": current, "latest": latest, "action": "up_to_date",
            "release": format!("{RELEASES}/latest"),
        }));
    }
    let mut answer = json!({
        "package": "deb", "current": current, "latest": latest, "action": "available",
        "release": format!("{RELEASES}/tag/v{latest}"),
        "commands": commands(&latest, arch),
        "command": command(&commands(&latest, arch)),
        "release_key_fingerprint": RELEASE_KEY_FINGERPRINT,
        "prerequisites": PREREQUISITES,
    });
    // LC-16: a release that needs a person first says so; its notes name the step.
    if let Some(steps) = feed["needs_migration"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
    {
        answer["needs_migration"] = json!(steps);
    }
    Ok(answer)
}

/// Reads the release feed and answers `notice`.
pub async fn check(current: &str) -> Result<Value, String> {
    let feed = crate::self_update::feed().await?;
    notice(&feed, current, arch())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_in_the_packages_file_list_is_a_deb_install() {
        let info = tempfile::tempdir().unwrap();
        let exe = Path::new("/usr/bin/switchboard");
        assert!(!owned_by_package(exe, info.path()), "no list, no package");
        std::fs::write(
            info.path().join("other.list"),
            "/usr/bin/switchboard\n/usr/bin/other\n",
        )
        .unwrap();
        assert!(
            !owned_by_package(exe, info.path()),
            "another package's list does not count"
        );
        std::fs::write(
            info.path().join("fabric-switchboard:amd64.list"),
            "/.\n/usr\n/usr/bin\n/usr/bin/fabric-switchboard\n/usr/bin/switchboard\n",
        )
        .unwrap();
        assert!(owned_by_package(exe, info.path()));
        assert!(
            !owned_by_package(Path::new("/usr/bin/switch"), info.path()),
            "a whole line, never a prefix"
        );
        assert!(!owned_by_package(
            Path::new("/home/u/Fabric-Switchboard.AppImage"),
            info.path()
        ));
        let plain = tempfile::tempdir().unwrap();
        std::fs::write(
            plain.path().join("fabric-switchboard.list"),
            "/usr/bin/fabric-switchboard\n",
        )
        .unwrap();
        assert!(owned_by_package(
            Path::new("/usr/bin/fabric-switchboard"),
            plain.path()
        ));
        assert!(!owned_by_package(exe, Path::new("/no/such/dpkg/info")));
    }

    #[test]
    fn a_newer_release_comes_with_the_commands_that_verify_and_install_it() {
        let feed = json!({"version": "0.7.115"});
        let answer = notice(&feed, "0.7.114", "x64").unwrap();
        assert_eq!(answer["action"], "available");
        assert_eq!(answer["current"], "0.7.114");
        assert_eq!(answer["latest"], "0.7.115");
        let commands: Vec<&str> = answer["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert!(commands.contains(&"curl -fLO https://github.com/passioncode-ai/fabric-switchboard/releases/download/v0.7.115/Fabric-Switchboard-0.7.115-linux-x64.deb"));
        assert!(commands.contains(
            &"gpgv --status-fd 1 --keyring ./release.gpg SHA256SUMS.asc SHA256SUMS | grep -Eq '^\\[GNUPG:\\] VALIDSIG .* 63B30DC324BD697487AA31944FAFB8AEC803B6A7$'"
        ));
        assert!(commands.contains(&"sha256sum -c SHA256SUMS --ignore-missing"));
        assert_eq!(
            answer["release_key_fingerprint"],
            "63B30DC324BD697487AA31944FAFB8AEC803B6A7"
        );
        // One command: every step joined with &&, so a failed check never reaches apt.
        let one = answer["command"].as_str().unwrap();
        assert_eq!(one.lines().count(), commands.len());
        for line in one.lines().take(commands.len() - 1) {
            assert!(line.ends_with(" && \\"), "{line}");
        }
        assert!(one.lines().last().unwrap().starts_with("sudo apt install"));
        assert!(!one.contains("set -e"));
        // No gpg: gpgv alone, which apt itself depends on.
        assert!(!one.contains("gpg --"), "{one}");
        assert!(one.contains("| base64 -d > release.gpg"));
        assert_eq!(answer["prerequisites"], json!(["curl", "gpgv"]));
        assert_eq!(
            commands.last(),
            Some(&"sudo apt install ./Fabric-Switchboard-0.7.115-linux-x64.deb")
        );
        // The checksums are verified before the package is checked against them.
        let verify = commands.iter().position(|c| c.starts_with("gpgv "));
        let sums = commands.iter().position(|c| c.starts_with("sha256sum"));
        assert!(verify < sums);
        assert!(answer.get("needs_migration").is_none());
        let arm = notice(&feed, "0.7.114", "arm64").unwrap();
        assert!(arm["commands"]
            .to_string()
            .contains("Fabric-Switchboard-0.7.115-linux-arm64.deb"));
    }

    #[test]
    fn the_same_or_an_older_release_is_up_to_date_and_a_bad_feed_says_so() {
        for version in ["0.7.114", "v0.7.114", "0.7.113"] {
            let answer = notice(&json!({"version": version}), "0.7.114", "x64").unwrap();
            assert_eq!(answer["action"], "up_to_date");
            assert!(answer.get("commands").is_none());
        }
        assert_eq!(
            notice(&json!({}), "0.7.114", "x64").unwrap_err(),
            crate::self_update::FEED_UNREADABLE
        );
        let held = notice(
            &json!({"version": "0.7.115", "needs_migration": "move the data first"}),
            "0.7.114",
            "x64",
        )
        .unwrap();
        assert_eq!(held["needs_migration"], "move the data first");
    }
}
