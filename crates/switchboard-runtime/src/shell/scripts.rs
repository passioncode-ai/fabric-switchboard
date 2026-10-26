//! The terminal hook's scripts for zsh, bash and fish (SB-183, node A-2), and
//! `switchboard shell init <shell>`.
//!
//! On every change of folder a hooked terminal asks `switchboard shell env` (the folder map, no
//! owner, no lock) what applies, and moves `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
//! `SWITCHBOARD_CONTEXT`:
//! - entering a group's folder: the person's own values (a value, or "was not set") are kept in
//!   shell variables that are never exported, then the group's homes and context are exported;
//! - moving to another group's folder: the new values; the saved ones stay;
//! - leaving: each variable the hook holds is set back, or unset, exactly as it was;
//! - a variable the person changed while the hook held it is theirs: never written over;
//! - a terminal Switchboard launched (`SWITCHBOARD_SESSION` set) is never touched;
//! - a home inherited from a hooked parent terminal is held, with "not set" as the person's own;
//! - the CLI missing, or an answer of another form: nothing changes, nothing is printed.
//!
//! The scripts are templates (`hook.zsh`, `hook.bash`, `hook.fish`, with `core.sh` shared by
//! zsh and bash) holding the CLI's and the data folder's absolute paths, quoted for the shell.
use std::path::{Path, PathBuf};

const CORE: &str = include_str!("core.sh");
const ZSH: &str = include_str!("hook.zsh");
const BASH: &str = include_str!("hook.bash");
const FISH: &str = include_str!("hook.fish");

/// A shell the hook is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Zsh,
    Bash,
    Fish,
}
impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Zsh, Kind::Bash, Kind::Fish];
    pub fn name(self) -> &'static str {
        match self {
            Kind::Zsh => "zsh",
            Kind::Bash => "bash",
            Kind::Fish => "fish",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// `<data>/shell/hook.<shell>`: the file a start file's block reads.
pub fn hook_path(root: &Path, kind: Kind) -> PathBuf {
    root.join(super::map::DIR)
        .join(format!("hook.{}", kind.name()))
}

/// `value` as one single-quoted word of a POSIX shell.
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
/// `value` as one single-quoted word of fish (`\\` and `\'` are its only escapes there).
fn fish_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The hook script for `kind`, calling the CLI at `cli` with the data folder `data`. None when
/// either path is not absolute or not valid text (a shell script cannot name it reliably) or
/// holds a control character.
pub fn script(kind: Kind, cli: &Path, data: &Path) -> Option<String> {
    let text = |p: &Path| {
        p.is_absolute()
            .then(|| p.to_str())
            .flatten()
            .filter(|t| !t.chars().any(char::is_control))
            .map(str::to_owned)
    };
    let (cli, data) = (text(cli)?, text(data)?);
    let quote = match kind {
        Kind::Fish => fish_quote,
        Kind::Zsh | Kind::Bash => sh_quote,
    };
    let template = match kind {
        Kind::Zsh => ZSH,
        Kind::Bash => BASH,
        Kind::Fish => FISH,
    };
    Some(
        template
            .replace("@CORE@", CORE)
            .replace("@CLI@", &quote(&cli))
            .replace("@DATA@", &quote(&data)),
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::process::Command;

    /// A world for one shell run: a data folder, an answering stub CLI and three folders.
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
    /// `base` is a name for the temporary folder's subfolder: spaces and quotes test quoting.
    fn world_in(base: &str) -> World {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap().join(base);
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
        let bin = root.join("bin dir");
        std::fs::create_dir_all(&bin).unwrap();
        let cli = bin.join("switchboard");
        let log = root.join("calls.log");
        // The stub answers as `switchboard shell env` would and logs each question.
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
    fn world() -> World {
        world_in("plain")
    }

    fn shell_path(kind: Kind) -> Option<&'static str> {
        let path = match kind {
            Kind::Zsh => "/bin/zsh",
            Kind::Bash => "/bin/bash",
            Kind::Fish => [
                "/opt/homebrew/bin/fish",
                "/usr/local/bin/fish",
                "/usr/bin/fish",
            ]
            .into_iter()
            .find(|p| Path::new(p).exists())?,
        };
        Path::new(path).exists().then_some(path)
    }
    /// The shells present here; a missing one is named once, so the skip is visible.
    fn shells() -> Vec<Kind> {
        Kind::ALL
            .into_iter()
            .filter(|k| {
                let found = shell_path(*k).is_some();
                if !found {
                    eprintln!("skipped: {} is not installed on this machine", k.name());
                }
                found
            })
            .collect()
    }

    /// Runs `body` in `kind` after loading the hook from a file, starting in `start`, with a
    /// clean environment plus `env`. `cd` is the shell's own; `@prompt` stands for a prompt
    /// (bash runs its prompt command there; zsh and fish act on the change of folder itself).
    fn run(
        w: &World,
        kind: Kind,
        cli: &Path,
        env: &[(&str, &str)],
        body: &str,
    ) -> (String, String) {
        let hook = w.data.join(format!("hook.{}", kind.name()));
        std::fs::write(&hook, script(kind, cli, &w.data).unwrap()).unwrap();
        let prompt = match kind {
            Kind::Bash => "eval \"$PROMPT_COMMAND\"",
            Kind::Zsh | Kind::Fish => ":",
        };
        let load = match kind {
            Kind::Fish => format!("source {}", fish_quote(&hook.to_string_lossy())),
            _ => format!(". {}", sh_quote(&hook.to_string_lossy())),
        };
        let script = format!("{load}\n{}", body.replace("@prompt", prompt));
        let mut command = Command::new(shell_path(kind).unwrap());
        match kind {
            Kind::Zsh => command.args(["-f", "-c", &script]),
            Kind::Bash => command.args(["--noprofile", "--norc", "-c", &script]),
            Kind::Fish => command.args(["--no-config", "-c", &script]),
        };
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", w.data.parent().unwrap())
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
    /// A line that prints the three variables, each `name=value` or `name unset`.
    fn show(kind: Kind, tag: &str) -> String {
        let one = |name: &str| {
            match kind {
            Kind::Fish => format!(
                "if set -q {name}; echo '{tag} {name}='\"${name}\"; else; echo '{tag} {name} unset'; end"
            ),
            _ => format!(
                "if [ -n \"${{{name}+x}}\" ]; then printf '%s\\n' '{tag} {name}='\"${name}\"; else echo '{tag} {name} unset'; fi"
            ),
        }
        };
        ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "SWITCHBOARD_CONTEXT"]
            .map(one)
            .join("\n")
    }
    fn cd(kind: Kind, dir: &Path) -> String {
        let quoted = match kind {
            Kind::Fish => fish_quote(&dir.to_string_lossy()),
            _ => sh_quote(&dir.to_string_lossy()),
        };
        format!("cd {quoted}\n@prompt")
    }
    /// `tag name` → value (or `unset`).
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

    #[test]
    fn entering_and_leaving_restores_unset_and_set_values() {
        for kind in shells() {
            let w = world();
            let body = [
                show(kind, "start"),
                cd(kind, &w.acme.join("src")),
                show(kind, "acme"),
                cd(kind, &w.globex),
                show(kind, "globex"),
                cd(kind, &w.mine),
                show(kind, "mine"),
            ]
            .join("\n");
            let (out, err) = run(
                &w,
                kind,
                &w.cli,
                &[("CLAUDE_CONFIG_DIR", "/own/claude dir")],
                &body,
            );
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(
                v["start CLAUDE_CONFIG_DIR"], "/own/claude dir",
                "{k}: {out}{err}"
            );
            assert_eq!(
                v["acme CLAUDE_CONFIG_DIR"], w.acme_claude,
                "{k}: {out}{err}"
            );
            assert_eq!(v["acme CODEX_HOME"], w.acme_codex, "{k}");
            assert_eq!(v["acme SWITCHBOARD_CONTEXT"], "org:o-acme", "{k}");
            assert_eq!(v["globex CLAUDE_CONFIG_DIR"], w.globex_claude, "{k}");
            assert_eq!(v["globex CODEX_HOME"], "unset", "{k}: no Codex home there");
            assert_eq!(v["globex SWITCHBOARD_CONTEXT"], "org:o-globex", "{k}");
            assert_eq!(
                v["mine CLAUDE_CONFIG_DIR"], "/own/claude dir",
                "{k}: set back"
            );
            assert_eq!(v["mine CODEX_HOME"], "unset", "{k}: unset again");
            assert_eq!(v["mine SWITCHBOARD_CONTEXT"], "unset", "{k}");
            assert!(err.is_empty(), "{k}: {err}");
            // Once at load and once per change of folder, never more.
            assert_eq!(calls(&w), 4, "{k}");
        }
    }

    #[test]
    fn a_value_the_person_set_inside_is_never_restored_over() {
        for kind in shells() {
            let w = world();
            let set = match kind {
                Kind::Fish => "set -gx CLAUDE_CONFIG_DIR /theirs".to_owned(),
                _ => "export CLAUDE_CONFIG_DIR=/theirs".to_owned(),
            };
            let body = [
                cd(kind, &w.acme),
                set,
                cd(kind, &w.acme.join("src")),
                show(kind, "inside"),
                cd(kind, &w.mine),
                show(kind, "mine"),
                cd(kind, &w.acme),
                cd(kind, &w.mine),
                show(kind, "again"),
            ]
            .join("\n");
            let (out, err) = run(&w, kind, &w.cli, &[], &body);
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(v["inside CLAUDE_CONFIG_DIR"], "/theirs", "{k}: {out}{err}");
            assert_eq!(v["inside CODEX_HOME"], w.acme_codex, "{k}");
            assert_eq!(
                v["mine CLAUDE_CONFIG_DIR"], "/theirs",
                "{k}: not restored over"
            );
            assert_eq!(v["mine CODEX_HOME"], "unset", "{k}: the others are");
            // Entering again keeps their value as their own and gives it back on leaving.
            assert_eq!(v["again CLAUDE_CONFIG_DIR"], "/theirs", "{k}");
        }
    }

    #[test]
    fn a_launched_sessions_shell_is_never_touched() {
        for kind in shells() {
            let w = world();
            let body = [
                cd(kind, &w.acme),
                show(kind, "acme"),
                match kind {
                    Kind::Fish => "set -q SWITCHBOARD_SHELL_HOOK; and echo loaded".into(),
                    _ => "[ -n \"${SWITCHBOARD_SHELL_HOOK+x}\" ] && echo loaded".to_owned(),
                },
            ]
            .join("\n");
            let (out, err) = run(
                &w,
                kind,
                &w.cli,
                &[("SWITCHBOARD_SESSION", "managed:claude:acme")],
                &body,
            );
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(v["acme CLAUDE_CONFIG_DIR"], "unset", "{k}: {out}{err}");
            assert_eq!(v["acme SWITCHBOARD_CONTEXT"], "unset", "{k}");
            assert!(!out.contains("loaded"), "{k}: the hook did not load");
            assert_eq!(calls(&w), 0, "{k}: Switchboard was never asked");
        }
    }

    #[test]
    fn the_saved_values_are_not_exported() {
        for kind in shells() {
            let w = world();
            let allexport = match kind {
                Kind::Bash | Kind::Zsh => "set -a\n",
                Kind::Fish => "",
            };
            let body = format!(
                "{allexport}{}\n/usr/bin/env\necho ---\n{}\n/usr/bin/env",
                cd(kind, &w.acme),
                cd(kind, &w.mine)
            );
            // With `set -a` on in bash and zsh, the hook's own variables are still never
            // exported, loaded before or after it.
            let (out, err) = run(
                &w,
                kind,
                &w.cli,
                &[("CLAUDE_CONFIG_DIR", "/own/secret-home")],
                &body,
            );
            let k = kind.name();
            let (inside, outside) = out.split_once("---").unwrap();
            assert!(
                inside.contains(&format!("CLAUDE_CONFIG_DIR={}", w.acme_claude)),
                "{k}: {out}{err}"
            );
            assert!(
                !inside.contains("/own/secret-home"),
                "{k}: the saved value leaked: {inside}"
            );
            assert!(!inside.contains("_switchboard"), "{k}: {inside}");
            assert!(inside.contains("SWITCHBOARD_SHELL_HOOK=1"), "{k}");
            assert!(
                outside.contains("CLAUDE_CONFIG_DIR=/own/secret-home"),
                "{k}: {outside}"
            );
            assert!(!outside.contains("_switchboard"), "{k}: {outside}");
        }
    }

    #[test]
    fn a_missing_cli_prints_nothing() {
        for kind in shells() {
            let w = world();
            let missing = w.data.join("gone").join("switchboard");
            let body = [cd(kind, &w.acme), show(kind, "acme")].join("\n");
            let (out, err) = run(&w, kind, &missing, &[("CODEX_HOME", "/own/codex")], &body);
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(v["acme CODEX_HOME"], "/own/codex", "{k}: {out}{err}");
            assert_eq!(v["acme CLAUDE_CONFIG_DIR"], "unset", "{k}");
            assert_eq!(
                out.lines().count(),
                3,
                "{k}: only the test's own lines: {out}"
            );
            assert!(err.is_empty(), "{k}: {err}");
        }
    }

    #[test]
    fn bash_3_2_syntax_is_accepted() {
        let w = world();
        let hook = w.data.join("hook.bash");
        std::fs::write(&hook, script(Kind::Bash, &w.cli, &w.data).unwrap()).unwrap();
        let out = Command::new("/bin/bash")
            .arg("-n")
            .arg(&hook)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let version = Command::new("/bin/bash").arg("--version").output().unwrap();
        eprintln!(
            "{}",
            String::from_utf8_lossy(&version.stdout)
                .lines()
                .next()
                .unwrap_or("")
        );
        let hook = w.data.join("hook.zsh");
        std::fs::write(&hook, script(Kind::Zsh, &w.cli, &w.data).unwrap()).unwrap();
        if Path::new("/bin/zsh").exists() {
            let out = Command::new("/bin/zsh")
                .arg("-n")
                .arg(&hook)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    // Adversarial cases of our own.

    #[test]
    fn paths_with_spaces_and_quotes_survive_every_shell() {
        for kind in shells() {
            let w = world_in("it's a \"quoted\" $HOME `dir`");
            let body = [
                cd(kind, &w.acme),
                show(kind, "acme"),
                cd(kind, &w.mine),
                show(kind, "mine"),
            ]
            .join("\n");
            let (out, err) = run(&w, kind, &w.cli, &[], &body);
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(
                v["acme CLAUDE_CONFIG_DIR"], w.acme_claude,
                "{k}: {out}{err}"
            );
            assert_eq!(v["mine CLAUDE_CONFIG_DIR"], "unset", "{k}");
            assert!(err.is_empty(), "{k}: {err}");
        }
    }

    #[test]
    fn a_shell_started_inside_an_organization_folder_gives_the_home_back_on_leaving() {
        // The parent terminal saved the person's values; a child shell inherits only the
        // exported homes. Leaving the folder there must not keep the group's home.
        for kind in shells() {
            let w = world();
            let body = [cd(kind, &w.mine), show(kind, "mine")].join("\n");
            let (out, err) = run(
                &w,
                kind,
                &w.cli,
                &[
                    ("CLAUDE_CONFIG_DIR", &w.acme_claude),
                    ("CODEX_HOME", &w.acme_codex),
                    ("SWITCHBOARD_CONTEXT", "org:o-acme"),
                ],
                &body,
            );
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(v["mine CLAUDE_CONFIG_DIR"], "unset", "{k}: {out}{err}");
            assert_eq!(v["mine CODEX_HOME"], "unset", "{k}");
            assert_eq!(v["mine SWITCHBOARD_CONTEXT"], "unset", "{k}");
        }
    }

    #[test]
    fn an_answer_of_another_form_changes_nothing_and_the_prompt_keeps_its_status() {
        for kind in shells() {
            let w = world();
            // A CLI that answers garbage on stdout, noise on stderr, and fails.
            std::fs::write(
                &w.cli,
                "#!/bin/sh\necho 'acme\torg:o-acme'\necho noise >&2\nexit 3\n",
            )
            .unwrap();
            let status = match kind {
                Kind::Bash => "false\neval \"$PROMPT_COMMAND\"\necho status=$?".to_owned(),
                Kind::Zsh => "false\necho status=$?".to_owned(),
                Kind::Fish => "false\necho status=$status".to_owned(),
            };
            let body = [cd(kind, &w.acme), show(kind, "acme"), status].join("\n");
            let (out, err) = run(&w, kind, &w.cli, &[("CLAUDE_CONFIG_DIR", "/own")], &body);
            let v = seen(&out);
            let k = kind.name();
            assert_eq!(v["acme CLAUDE_CONFIG_DIR"], "/own", "{k}: {out}{err}");
            assert!(out.contains("status=1"), "{k}: {out}");
            assert!(err.is_empty(), "{k}: {err}");
            // An answer with too many fields, or a home that is not absolute, is refused too.
            for line in ["acme\torg:x\t/a\t-\textra", "acme\torg:x\trelative\t-"] {
                std::fs::write(
                    &w.cli,
                    format!("#!/bin/sh\nprintf '%s\\n' {}\n", sh_quote(line)),
                )
                .unwrap();
                let (out, _) = run(
                    &w,
                    kind,
                    &w.cli,
                    &[],
                    &[cd(kind, &w.acme), show(kind, "x")].join("\n"),
                );
                assert_eq!(seen(&out)["x CLAUDE_CONFIG_DIR"], "unset", "{k}: {line:?}");
            }
        }
    }

    #[test]
    fn loading_twice_registers_once_and_bash_keeps_the_persons_prompt_command() {
        for kind in shells() {
            let w = world();
            let hook = w.data.join(format!("hook.{}", kind.name()));
            let load_again = match kind {
                Kind::Fish => format!("source {}", fish_quote(&hook.to_string_lossy())),
                _ => format!(". {}", sh_quote(&hook.to_string_lossy())),
            };
            let pc = match kind {
                Kind::Bash => "echo \"pc=$PROMPT_COMMAND\"",
                _ => ":",
            };
            let body = [load_again, cd(kind, &w.acme), pc.to_owned()].join("\n");
            let (out, err) = run(&w, kind, &w.cli, &[("PROMPT_COMMAND", "true")], &body);
            let k = kind.name();
            assert!(err.is_empty(), "{k}: {err}");
            assert_eq!(
                calls(&w),
                2,
                "{k}: once at load, once for the change: {out}"
            );
            if kind == Kind::Bash {
                assert!(out.contains("pc=_switchboard_prompt;true"), "{out}");
            }
        }
    }

    #[test]
    fn a_path_that_is_not_text_gets_no_script() {
        use std::os::unix::ffi::OsStrExt;
        let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/\xff/data"));
        assert!(script(Kind::Zsh, Path::new("/bin/switchboard"), &bad).is_none());
        assert!(script(Kind::Bash, &bad, Path::new("/data")).is_none());
        assert!(script(Kind::Fish, Path::new("relative"), Path::new("/data")).is_none());
        assert!(script(Kind::Fish, Path::new("/a\nb"), Path::new("/data")).is_none());
        // The quoting itself.
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(fish_quote(r"a\b'c"), r"'a\\b\'c'");
    }
}
