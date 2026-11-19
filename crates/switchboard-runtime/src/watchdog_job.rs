//! SB-510: the system job that runs `switchboard watchdog --once` every five minutes, apart from
//! the owner, so the owner stopping or hanging is noticed by something that is not the owner.
//!
//! Only the installed app's owner of the app's own data folder installs it (`Runtime::enable_watchdog`):
//! never a development build, `serve`, another `--data-dir` or a test, which passes a `Host` with
//! a temporary home and a recording runner. On macOS it is the LaunchAgent
//! `~/Library/LaunchAgents/ai.passioncode.fabric-switchboard.watchdog.plist` (`StartInterval`
//! 300), Switchboard's first own launchd label; on Linux the systemd user units
//! `~/.config/systemd/user/fabric-switchboard-watchdog.{service,timer}`. Windows has no job yet
//! (SB-543). `switchboard watchdog off` and `switchboard uninstall` take it out; the app's next
//! start writes it again only while the setting is on.
use std::path::{Path, PathBuf};

pub const LABEL: &str = "ai.passioncode.fabric-switchboard.watchdog";
pub const MACOS_PLIST: &str =
    "Library/LaunchAgents/ai.passioncode.fabric-switchboard.watchdog.plist";
pub const LINUX_DIR: &str = ".config/systemd/user";
pub const LINUX_UNIT: &str = "fabric-switchboard-watchdog";
const LAUNCHCTL: &str = "/bin/launchctl";
const SYSTEMCTL: &str = "systemctl";

/// Runs a program with its arguments and answers whether it succeeded; tests pass their own.
pub type Runner = fn(&str, &[String]) -> bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Macos,
    Linux,
    Other,
}

/// Where the job lives and how it is loaded.
#[derive(Clone, Debug)]
pub struct Host {
    pub home: PathBuf,
    pub uid: Option<u32>,
    /// The `switchboard` CLI the job runs; needed to install, not to remove.
    pub program: Option<PathBuf>,
    pub os: Os,
    pub run: Runner,
}

impl Host {
    /// This process's person: their home, their user id and the real `launchctl` or `systemctl`.
    pub fn process(program: Option<PathBuf>) -> Option<Host> {
        let os = if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Other
        };
        #[cfg(unix)]
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = Some(unsafe { libc::getuid() });
        #[cfg(not(unix))]
        let uid = None;
        Some(Host {
            home: dirs::home_dir()?,
            uid,
            program,
            os,
            run: run_quietly,
        })
    }
}

fn run_quietly(program: &str, args: &[String]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The LaunchAgent: `<program> watchdog --once` at load and every 300 s, in the background, its
/// output discarded (the report is the file it writes).
pub fn plist(program: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>watchdog</string>
    <string>--once</string>
  </array>
  <key>StartInterval</key>
  <integer>{}</integer>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Background</string>
  <key>LowPriorityIO</key>
  <true/>
  <key>Nice</key>
  <integer>10</integer>
  <key>StandardOutPath</key>
  <string>/dev/null</string>
  <key>StandardErrorPath</key>
  <string>/dev/null</string>
</dict>
</plist>
"#,
        xml(&program.to_string_lossy()),
        crate::watchdog::INTERVAL_SECONDS
    )
}

/// A path as one word of a systemd `ExecStart=` line.
fn unit_word(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

/// The systemd user service the timer starts.
pub fn service(program: &Path) -> String {
    format!(
        "[Unit]\nDescription=Fabric Switchboard watchdog\n\n[Service]\nType=oneshot\nExecStart={} watchdog --once\nNice=10\n",
        unit_word(&program.to_string_lossy())
    )
}

/// The systemd user timer: five minutes after boot, then every five minutes.
pub fn timer() -> String {
    let every = crate::watchdog::INTERVAL_SECONDS;
    format!(
        "[Unit]\nDescription=Fabric Switchboard watchdog, every five minutes\n\n[Timer]\nOnBootSec={every}s\nOnUnitActiveSec={every}s\n\n[Install]\nWantedBy=timers.target\n"
    )
}

fn timer_name() -> String {
    format!("{LINUX_UNIT}.timer")
}

/// The job's files on this host, each with what it must hold.
fn wanted(host: &Host, program: &Path) -> Vec<(PathBuf, String)> {
    match host.os {
        Os::Macos => vec![(host.home.join(MACOS_PLIST), plist(program))],
        Os::Linux => {
            let dir = host.home.join(LINUX_DIR);
            vec![
                (dir.join(format!("{LINUX_UNIT}.service")), service(program)),
                (dir.join(timer_name()), timer()),
            ]
        }
        Os::Other => Vec::new(),
    }
}

/// The job's files on this host, whatever they hold.
fn paths(host: &Host) -> Vec<PathBuf> {
    match host.os {
        Os::Macos => vec![host.home.join(MACOS_PLIST)],
        Os::Linux => {
            let dir = host.home.join(LINUX_DIR);
            vec![
                dir.join(format!("{LINUX_UNIT}.service")),
                dir.join(timer_name()),
            ]
        }
        Os::Other => Vec::new(),
    }
}

/// Writes `text` to `path` by a rename in its folder, never through a link.
fn write(path: &Path, text: &str) -> Result<(), ()> {
    let dir = path.parent().ok_or(())?;
    std::fs::create_dir_all(dir).map_err(|_| ())?;
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(());
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text)
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
}

fn service_target(host: &Host) -> Option<(String, String)> {
    let target = format!("gui/{}", host.uid?);
    Some((format!("{target}/{LABEL}"), target))
}

/// Whether the job's files are on this host.
pub fn state(host: &Host) -> &'static str {
    if host.os == Os::Other {
        return "unsupported";
    }
    if paths(host).iter().all(|p| p.is_file()) {
        "installed"
    } else {
        "absent"
    }
}

/// Puts the job in place and loaded: `installed`, `unchanged` (its files already hold this and it
/// is loaded), `failed`, `unsupported` (Windows, SB-543) or `no_program` (no CLI to run).
pub fn install(host: &Host) -> &'static str {
    if host.os == Os::Other {
        return "unsupported";
    }
    let Some(program) = host.program.as_deref() else {
        return "no_program";
    };
    let files = wanted(host, program);
    let same = files
        .iter()
        .all(|(path, text)| std::fs::read_to_string(path).is_ok_and(|t| &t == text));
    match host.os {
        Os::Macos => {
            let Some((service, target)) = service_target(host) else {
                return "failed";
            };
            if same && (host.run)(LAUNCHCTL, &["print".into(), service.clone()]) {
                return "unchanged";
            }
            if !same {
                for (path, text) in &files {
                    if write(path, text).is_err() {
                        return "failed";
                    }
                }
                // An earlier copy stays loaded until it is booted out; absent is fine.
                (host.run)(LAUNCHCTL, &["bootout".into(), service]);
            }
            let plist = files[0].0.to_string_lossy().into_owned();
            if (host.run)(LAUNCHCTL, &["bootstrap".into(), target, plist]) {
                "installed"
            } else {
                "failed"
            }
        }
        Os::Linux => {
            let systemctl = |args: &[&str]| {
                let mut all = vec!["--user".to_owned()];
                all.extend(args.iter().map(|a| (*a).to_owned()));
                (host.run)(SYSTEMCTL, &all)
            };
            if same && systemctl(&["is-active", "--quiet", &timer_name()]) {
                return "unchanged";
            }
            if !same {
                for (path, text) in &files {
                    if write(path, text).is_err() {
                        return "failed";
                    }
                }
                systemctl(&["daemon-reload"]);
            }
            if systemctl(&["enable", "--now", &timer_name()]) {
                "installed"
            } else {
                "failed"
            }
        }
        Os::Other => "unsupported",
    }
}

/// Takes the job out: unloaded, its files removed. `removed`, `absent` (nothing was there) or an
/// error naming a file that stayed.
pub fn remove(host: &Host) -> Result<&'static str, String> {
    let present: Vec<PathBuf> = paths(host)
        .into_iter()
        .filter(|p| std::fs::symlink_metadata(p).is_ok())
        .collect();
    if present.is_empty() {
        return Ok("absent");
    }
    match host.os {
        Os::Macos => {
            if let Some((service, _)) = service_target(host) {
                (host.run)(LAUNCHCTL, &["bootout".into(), service]);
            }
        }
        Os::Linux => {
            (host.run)(
                SYSTEMCTL,
                &[
                    "--user".into(),
                    "disable".into(),
                    "--now".into(),
                    timer_name(),
                ],
            );
        }
        Os::Other => {}
    }
    for path in &present {
        if std::fs::remove_file(path).is_err() {
            return Err("Could not remove the watchdog's job.".into());
        }
    }
    if host.os == Os::Linux {
        (host.run)(SYSTEMCTL, &["--user".into(), "daemon-reload".into()]);
    }
    Ok("removed")
}

/// The job as the setting says: installed while on, removed while off.
pub fn apply(host: &Host, on: bool) -> &'static str {
    if on {
        install(host)
    } else {
        remove(host).unwrap_or("failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Every command a test host ran, in order; one test at a time reads it.
    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static SERIAL: Mutex<()> = Mutex::new(());
    /// What the fake `launchctl print` and `systemctl is-active` answer.
    static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn record(program: &str, args: &[String]) -> bool {
        RAN.lock()
            .unwrap()
            .push(format!("{program} {}", args.join(" ")));
        if args.iter().any(|a| a == "print" || a == "is-active") {
            return LOADED.load(std::sync::atomic::Ordering::SeqCst);
        }
        true
    }

    fn host(home: &Path, os: Os) -> Host {
        Host {
            home: home.to_owned(),
            uid: Some(501),
            program: Some(PathBuf::from(
                "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard",
            )),
            os,
            run: record,
        }
    }

    fn ran() -> Vec<String> {
        std::mem::take(&mut *RAN.lock().unwrap())
    }

    #[test]
    fn the_launch_agent_runs_the_cli_every_five_minutes_and_is_loaded_once() {
        let _one = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        ran();
        LOADED.store(false, std::sync::atomic::Ordering::SeqCst);
        let home = tempfile::tempdir().unwrap();
        let h = host(home.path(), Os::Macos);
        assert_eq!(state(&h), "absent");
        assert_eq!(install(&h), "installed");
        let file = home.path().join(MACOS_PLIST);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(&format!("<string>{LABEL}</string>")));
        assert!(text.contains(
            "<string>/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard</string>\n    <string>watchdog</string>\n    <string>--once</string>"
        ));
        assert!(text.contains("<key>StartInterval</key>\n  <integer>300</integer>"));
        assert_eq!(
            ran(),
            [
                format!("{LAUNCHCTL} bootout gui/501/{LABEL}"),
                format!("{LAUNCHCTL} bootstrap gui/501 {}", file.display()),
            ]
        );
        assert_eq!(state(&h), "installed");
        // The next start finds it as it should be and loaded: one look, nothing written.
        LOADED.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(install(&h), "unchanged");
        assert_eq!(ran(), [format!("{LAUNCHCTL} print gui/501/{LABEL}")]);
        // A moved app rewrites it and loads it again.
        let mut moved = h.clone();
        moved.program = Some(PathBuf::from("/Users/a/Applications/S & B.app/switchboard"));
        assert_eq!(install(&moved), "installed");
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .contains("<string>/Users/a/Applications/S &amp; B.app/switchboard</string>"));
        ran();
        // Off: unloaded and removed; a second removal finds nothing.
        assert_eq!(apply(&h, false), "removed");
        assert_eq!(ran(), [format!("{LAUNCHCTL} bootout gui/501/{LABEL}")]);
        assert!(!file.exists());
        assert_eq!(remove(&h), Ok("absent"));
        assert!(ran().is_empty());
        // Without a CLI to name there is nothing to install.
        let mut none = h.clone();
        none.program = None;
        assert_eq!(install(&none), "no_program");
    }

    #[test]
    fn the_systemd_timer_runs_the_cli_every_five_minutes() {
        let _one = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        ran();
        LOADED.store(false, std::sync::atomic::Ordering::SeqCst);
        let home = tempfile::tempdir().unwrap();
        let mut h = host(home.path(), Os::Linux);
        h.program = Some(PathBuf::from("/opt/Fabric Switchboard/switch%board"));
        assert_eq!(install(&h), "installed");
        let dir = home.path().join(LINUX_DIR);
        let unit = std::fs::read_to_string(dir.join(format!("{LINUX_UNIT}.service"))).unwrap();
        assert!(
            unit.contains("ExecStart=\"/opt/Fabric Switchboard/switch%%board\" watchdog --once\n")
        );
        let timer = std::fs::read_to_string(dir.join(format!("{LINUX_UNIT}.timer"))).unwrap();
        assert!(timer.contains("OnUnitActiveSec=300s"));
        assert!(timer.contains("WantedBy=timers.target"));
        assert_eq!(
            ran(),
            [
                "systemctl --user daemon-reload".to_owned(),
                format!("systemctl --user enable --now {LINUX_UNIT}.timer"),
            ]
        );
        LOADED.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(install(&h), "unchanged");
        ran();
        assert_eq!(remove(&h), Ok("removed"));
        assert_eq!(
            ran(),
            [
                format!("systemctl --user disable --now {LINUX_UNIT}.timer"),
                "systemctl --user daemon-reload".to_owned(),
            ]
        );
        assert_eq!(state(&h), "absent");
        assert_eq!(install(&host(home.path(), Os::Other)), "unsupported");
        assert_eq!(state(&host(home.path(), Os::Other)), "unsupported");
    }
}
