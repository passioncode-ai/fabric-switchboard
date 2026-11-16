//! Opening a terminal window for a script, and finding programs (SB-397 D-2: the Terminal and
//! script helpers of switchboard-runtime's `launch`, moved unchanged). The core's modules open
//! windows through `Accounts::open_terminal`, so the app decides whether a session runs in a host.
use std::path::{Path, PathBuf};

/// The shell a session script runs in: zsh, macOS's own, on macOS; POSIX `sh` elsewhere, since
/// many Linux systems have no zsh (SB-88). The script uses only POSIX constructs in either.
#[cfg(target_os = "macos")]
pub const SCRIPT_SHELL: &str = "/bin/zsh";

#[cfg(all(unix, not(target_os = "macos")))]
pub const SCRIPT_SHELL: &str = "/bin/sh";

/// Windows runs no session script; the duty officer's and Continue's resume scripts are written
/// only on Unix, but the text they share is built on every platform (SB-396 wave B).
#[cfg(windows)]
pub const SCRIPT_SHELL: &str = "/bin/sh";

/// An executable named `name` on PATH or in the usual per-user and Homebrew folders.
pub fn find_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|s| std::env::split_paths(&s).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
        ]);
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    // After Homebrew, so a program found before keeps being the one launched: where agents' own
    // installers put theirs, which the desktop app's PATH lacks (SB-120: OpenCode's is
    // ~/.opencode/bin, Kimi Code's ~/.kimi-code/bin). Version-manager shims (Volta, mise, asdf)
    // are left out: they resolve to the manager, not the agent.
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".opencode/bin"),
            home.join(".kimi-code/bin"),
            home.join(".hermes/bin"),
            home.join(".deno/bin"),
            home.join("Library/pnpm"),
            home.join(".yarn/bin"),
        ]);
    }
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
                return path.canonicalize().ok();
            }
        }
        #[cfg(not(windows))]
        {
            let path = dir.join(name);
            if path.is_file() {
                return path.canonicalize().ok();
            }
        }
    }
    None
}

/// Opens `path` (a script) in a terminal window of its own: Terminal on macOS (through window
/// settings in `cache`, when given, so the window closes on a clean exit), a new PowerShell console
/// on Windows, the terminal emulator `linux_terminal` chooses on Linux. `strip` names environment
/// variables the window must not inherit (an app's own credentials). Whether the script runs a
/// session through a host is the caller's choice (`Accounts::open_terminal`).
pub fn open(path: &Path, cache: Option<&Path>, strip: &[&str]) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = strip;
        // SB-342 H-0: a window that closes when the session exits cleanly (one that failed stays,
        // so its message can be read); the plain script where the window settings cannot be made.
        let opened = cache
            .and_then(|dir| window_settings_in(dir, path))
            .unwrap_or_else(|| path.to_owned());
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-a", "Terminal"])
            .arg(&opened)
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
        let _ = cache;
        let shell = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or("Windows system directory unavailable.")?,
        )
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = std::process::Command::new(shell);
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
        for variable in strip {
            command.env_remove(variable);
        }
        command.spawn().map_err(|_| "Terminal could not open.")?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let _ = cache;
        spawn_linux_terminal(path, std::env::var_os("TERMINAL"), find_program, strip)
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        let _ = (path, cache, strip);
        Err("Terminal launch is currently supported on macOS, Windows and Linux only.".into())
    }
}

/// A script at `path`, owner-only and executable (0700 on Unix).
pub fn write_script(path: &Path, content: &str) -> Result<(), String> {
    crate::private_fs::private_write(path, content.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Managed file permissions unavailable.")?;
    }
    Ok(())
}

/// SB-342 H-0: Terminal window settings (`.terminal`) in `dir` that run `script` as the window's
/// process and close the window when it exits with status 0, so the windows an app opens do not
/// pile up. Terminal runs `CommandString` as words split on whitespace with no quoting, so the
/// script is reached through a link in `dir`; None when that path would hold whitespace.
#[cfg(any(target_os = "macos", all(unix, test)))]
pub fn window_settings_in(dir: &Path, script: &Path) -> Option<PathBuf> {
    let name = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(script.to_string_lossy().as_bytes())
    )[..16]
        .to_owned();
    let link = dir.join(format!("{name}.command"));
    let text = link.to_string_lossy().into_owned();
    if text.chars().any(char::is_whitespace) || !text.is_ascii() {
        return None;
    }
    crate::private_fs::private_dir(dir).ok()?;
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(script, &link).ok()?;
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let settings = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>CommandString</key>\n\t<string>{escaped}</string>\n\t<key>ProfileCurrentVersion</key>\n\t<real>2.07</real>\n\t<key>RunCommandAsShell</key>\n\t<true/>\n\t<key>name</key>\n\t<string>Switchboard session</string>\n\t<key>shellExitAction</key>\n\t<integer>1</integer>\n\t<key>type</key>\n\t<string>Window Settings</string>\n\t<key>warnOnShellCloseAction</key>\n\t<integer>0</integer>\n</dict>\n</plist>\n"
    );
    let window = dir.join(format!("{name}.terminal"));
    crate::private_fs::private_write(&window, settings.as_bytes()).ok()?;
    Some(window)
}

/// No terminal emulator could be found on Linux (SB-88). Shown verbatim (src/adapter.ts).
pub const LINUX_TERMINAL_MISSING: &str = "No terminal app was found. Install one, such as GNOME Terminal, Konsole or xterm, or set TERMINAL to yours, then retry.";

/// Linux terminal emulators and how each is told to run one program: `$TERMINAL` first (a known
/// name gets its own arguments, an unknown one the common `-e`), then Debian's
/// `x-terminal-emulator` alternative, then the common desktops' own terminals.
#[cfg(any(target_os = "linux", test))]
pub const LINUX_TERMINALS: [(&str, &[&str]); 11] = [
    ("x-terminal-emulator", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("mate-terminal", &["-x"]),
    ("tilix", &["-e"]),
    ("kitty", &[]),
    ("alacritty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("foot", &[]),
    ("xterm", &["-e"]),
];

#[cfg(any(target_os = "linux", test))]
pub fn linux_terminal(
    preferred: Option<std::ffi::OsString>,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<(PathBuf, &'static [&'static str]), String> {
    if let Some(preferred) = preferred.filter(|p| !p.is_empty()) {
        let preferred = PathBuf::from(preferred);
        let name = preferred
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let arguments = LINUX_TERMINALS
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(&["-e"][..], |(_, arguments)| *arguments);
        let program = if preferred.is_absolute() {
            preferred.is_file().then_some(preferred)
        } else {
            find(&name)
        };
        if let Some(program) = program {
            return Ok((program, arguments));
        }
    }
    LINUX_TERMINALS
        .iter()
        .find_map(|(name, arguments)| find(name).map(|program| (program, *arguments)))
        .ok_or_else(|| LINUX_TERMINAL_MISSING.into())
}

/// Opens `path` in the Linux terminal `linux_terminal` chooses; the seam the Linux test drives.
#[cfg(target_os = "linux")]
pub fn spawn_linux_terminal(
    path: &Path,
    preferred: Option<std::ffi::OsString>,
    find: impl Fn(&str) -> Option<PathBuf>,
    strip: &[&str],
) -> Result<(), String> {
    {
        use std::os::unix::process::CommandExt;
        let (program, arguments) = linux_terminal(preferred, find)?;
        let mut command = std::process::Command::new(program);
        command
            .args(arguments)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // Its own process group: the Terminal outlives the app that opened it, as on macOS.
            .process_group(0);
        for variable in strip {
            command.env_remove(variable);
        }
        let mut child = command.spawn().map_err(|_| "Terminal could not open.")?;
        // Reaped when it exits, so no zombie stays behind for a server-style terminal either.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::fs;

    #[test]
    fn a_linux_terminal_is_chosen_by_terminal_then_by_the_known_list() {
        let only = |names: &'static [&'static str]| {
            move |name: &str| {
                names
                    .contains(&name)
                    .then(|| PathBuf::from(format!("/usr/bin/{name}")))
            }
        };
        let none = only(&[]);
        assert_eq!(
            linux_terminal(None, none).unwrap_err(),
            LINUX_TERMINAL_MISSING
        );
        // Debian's alternative wins over a desktop's own terminal; each gets its own arguments.
        let (program, arguments) =
            linux_terminal(None, only(&["konsole", "x-terminal-emulator"])).unwrap();
        assert_eq!(
            (program, arguments),
            (PathBuf::from("/usr/bin/x-terminal-emulator"), &["-e"][..])
        );
        let (program, arguments) =
            linux_terminal(None, only(&["gnome-terminal", "xterm"])).unwrap();
        assert_eq!(
            (program, arguments),
            (PathBuf::from("/usr/bin/gnome-terminal"), &["--"][..])
        );
        // TERMINAL is honoured first: a known name keeps its arguments, an unknown one gets `-e`.
        let (program, arguments) =
            linux_terminal(Some("kitty".into()), only(&["kitty", "xterm"])).unwrap();
        assert_eq!(program, PathBuf::from("/usr/bin/kitty"));
        assert!(arguments.is_empty());
        let (_, arguments) = linux_terminal(Some("myterm".into()), only(&["myterm"])).unwrap();
        assert_eq!(arguments, &["-e"][..]);
        // A TERMINAL that is not installed falls back to the list instead of failing.
        let (program, _) = linux_terminal(Some("missing-term".into()), only(&["xterm"])).unwrap();
        assert_eq!(program, PathBuf::from("/usr/bin/xterm"));
        assert!(linux_terminal(Some("".into()), only(&["foot"]))
            .unwrap()
            .1
            .is_empty());
    }

    /// SB-342 H-0: the window settings run the script through a link without spaces and close
    /// the window on a clean exit; a folder whose path holds a space gives none.
    #[cfg(unix)]
    #[test]
    fn a_terminal_window_closes_on_a_clean_exit_and_its_command_has_no_space() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp
            .path()
            .join("Application Support")
            .join("launch.command");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        let dir = temp.path().join("terminal");
        let window = window_settings_in(&dir, &script).unwrap();
        let text = fs::read_to_string(&window).unwrap();
        assert!(text.contains("<key>shellExitAction</key>\n\t<integer>1</integer>"));
        assert!(text.contains("<key>RunCommandAsShell</key>\n\t<true/>"));
        let command = text
            .split("<key>CommandString</key>\n\t<string>")
            .nth(1)
            .unwrap()
            .split("</string>")
            .next()
            .unwrap();
        assert!(!command.contains(' '), "{command}");
        assert_eq!(fs::read_link(command).unwrap(), script);
        // The same script gets the same link, so links never pile up.
        assert_eq!(window_settings_in(&dir, &script).unwrap(), window);
        assert!(window_settings_in(&temp.path().join("with space"), &script).is_none());
    }
}
