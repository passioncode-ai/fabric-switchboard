//! The person's own Claude Code and Codex folders (SB-183, node A-5).
//!
//! Several places take "the process's `CLAUDE_CONFIG_DIR` or `CODEX_HOME`, else the default
//! folder" as the person's own: what a launched home links from, the limit scan's transcripts,
//! where SB-143 writes Codex's provider, the user scope a launch copies its MCP servers from.
//! A process started in a terminal with Switchboard's shell hook, or inside a session
//! Switchboard launched, sees one of Switchboard's own homes there. That home is never the
//! person's: a value inside Switchboard's data folder gives way to the default folder.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Where the answer comes from. Explicit, so every branch is testable without the real home.
pub(crate) struct Sources<'a> {
    /// A variable of the process's environment.
    pub env: &'a dyn Fn(&str) -> Option<OsString>,
    /// The person's home folder.
    pub home: Option<PathBuf>,
    /// Switchboard's data folder.
    pub data: Option<PathBuf>,
}
impl Sources<'static> {
    pub(crate) fn process() -> Self {
        Sources {
            env: &|name| std::env::var_os(name),
            home: dirs::home_dir(),
            data: crate::default_root().ok(),
        }
    }
}

/// The person's Claude Code folder: their `CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    claude_dir_from(&Sources::process())
}
/// The person's Codex folder: their `CODEX_HOME`, else `~/.codex`.
pub fn codex_dir() -> Option<PathBuf> {
    codex_dir_from(&Sources::process())
}
/// The `CODEX_HOME` the person set themselves, if any (None: Codex uses `~/.codex`).
pub fn codex_home() -> Option<PathBuf> {
    own("CODEX_HOME", &Sources::process())
}

pub(crate) fn claude_dir_from(sources: &Sources) -> Option<PathBuf> {
    own("CLAUDE_CONFIG_DIR", sources).or_else(|| sources.home.as_ref().map(|h| h.join(".claude")))
}
pub(crate) fn codex_dir_from(sources: &Sources) -> Option<PathBuf> {
    own("CODEX_HOME", sources).or_else(|| sources.home.as_ref().map(|h| h.join(".codex")))
}
/// The person's Pi folder: their `PI_CODING_AGENT_DIR`, else `~/.pi/agent`.
pub(crate) fn pi_dir_from(sources: &Sources) -> Option<PathBuf> {
    own("PI_CODING_AGENT_DIR", sources)
        .or_else(|| sources.home.as_ref().map(|h| h.join(".pi").join("agent")))
}

/// The variable's value when it is set and the person's own: never a folder inside
/// Switchboard's data folder (a shell home, a launched home).
fn own(name: &str, sources: &Sources) -> Option<PathBuf> {
    (sources.env)(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|path| {
            !sources
                .data
                .as_deref()
                .is_some_and(|data| inside(path, data))
        })
}

/// Whether `path` lies inside `data`, compared whole component by whole component, as written
/// and with links resolved (macOS's `/var` is `/private/var`; a home may not exist yet, so its
/// nearest existing parent is resolved).
pub(crate) fn inside(path: &Path, data: &Path) -> bool {
    let datas = [Some(data.to_owned()), data.canonicalize().ok()];
    let paths = [Some(path.to_owned()), resolved(path)];
    paths
        .iter()
        .flatten()
        .any(|path| datas.iter().flatten().any(|data| path.starts_with(data)))
}
/// `path` with links resolved through its nearest existing ancestor.
fn resolved(path: &Path) -> Option<PathBuf> {
    let mut rest = Vec::new();
    let mut here = path;
    loop {
        if let Ok(real) = here.canonicalize() {
            return Some(rest.iter().rev().fold(real, |p, part| p.join(part)));
        }
        rest.push(here.file_name()?.to_owned());
        here = here.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sources<'a>(
        env: &'a dyn Fn(&str) -> Option<OsString>,
        home: &Path,
        data: &Path,
    ) -> Sources<'a> {
        Sources {
            env,
            home: Some(home.to_owned()),
            data: Some(data.to_owned()),
        }
    }

    #[test]
    fn a_config_dir_inside_switchboards_data_is_never_the_persons_own() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let shell_claude = data.path().join("shell").join("claude-acme");
        let shell_codex = data.path().join("shell").join("codex-acme");
        let launched = data.path().join("runtimes").join("claude-team");
        for dir in [&shell_claude, &shell_codex, &launched] {
            std::fs::create_dir_all(dir).unwrap();
        }
        // A terminal with the shell hook: both variables name Switchboard's shell homes.
        let hooked = BTreeMap::from([
            ("CLAUDE_CONFIG_DIR", shell_claude.clone().into_os_string()),
            ("CODEX_HOME", shell_codex.clone().into_os_string()),
        ]);
        let env = |name: &str| hooked.get(name).cloned();
        let s = sources(&env, home.path(), data.path());
        assert_eq!(claude_dir_from(&s), Some(home.path().join(".claude")));
        assert_eq!(codex_dir_from(&s), Some(home.path().join(".codex")));
        assert_eq!(own("CODEX_HOME", &s), None, "not the person's CODEX_HOME");
        // A session Switchboard launched: its home is Switchboard's too.
        let session = BTreeMap::from([("CLAUDE_CONFIG_DIR", launched.into_os_string())]);
        let env = |name: &str| session.get(name).cloned();
        let s = sources(&env, home.path(), data.path());
        assert_eq!(claude_dir_from(&s), Some(home.path().join(".claude")));
        // Through a link to the data folder: still Switchboard's.
        #[cfg(unix)]
        {
            let link = home.path().join("data-link");
            std::os::unix::fs::symlink(data.path(), &link).unwrap();
            let linked = BTreeMap::from([(
                "CLAUDE_CONFIG_DIR",
                link.join("shell").join("claude-acme").into_os_string(),
            )]);
            let env = |name: &str| linked.get(name).cloned();
            let s = sources(&env, home.path(), data.path());
            assert_eq!(claude_dir_from(&s), Some(home.path().join(".claude")));
        }
        // The person's own choice elsewhere is kept, and so is an unset or empty one.
        let own_dirs = BTreeMap::from([
            ("CLAUDE_CONFIG_DIR", OsString::from("/elsewhere/claude")),
            ("CODEX_HOME", OsString::from("/elsewhere/codex")),
        ]);
        let env = |name: &str| own_dirs.get(name).cloned();
        let s = sources(&env, home.path(), data.path());
        assert_eq!(
            claude_dir_from(&s),
            Some(PathBuf::from("/elsewhere/claude"))
        );
        assert_eq!(
            own("CODEX_HOME", &s),
            Some(PathBuf::from("/elsewhere/codex"))
        );
        let empty = |_: &str| Some(OsString::new());
        let s = sources(&empty, home.path(), data.path());
        assert_eq!(codex_dir_from(&s), Some(home.path().join(".codex")));
        // A shell home not created yet is Switchboard's as well.
        let later = data.path().join("shell").join("claude-later");
        assert!(inside(&later, data.path()));
        // A data folder's sibling with a longer name is not inside it.
        let sibling = PathBuf::from(format!("{}-other", data.path().display()));
        assert!(!inside(&sibling, data.path()));
        // No data folder known: the variable is taken as it is.
        let s = Sources {
            env: &env,
            home: Some(home.path().to_owned()),
            data: None,
        };
        assert_eq!(
            claude_dir_from(&s),
            Some(PathBuf::from("/elsewhere/claude"))
        );
    }
}
