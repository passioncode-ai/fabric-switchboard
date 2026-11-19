//! SB-580: the Sessions page hands a session to Fabric through `fabric-session-links/1`
//! (Fabric's proposal of 2026-11-19 on the project board, built on fabric-agent-contract
//! DEC-0040): `fabric://session/new`, `fabric://session/open` and `fabric://session/continue`,
//! each with `from=fabric-switchboard`. A link names ids only, never a word, a path or a secret,
//! and it opens a view in Fabric: Fabric asks the person before it starts or moves a session.
//! Switchboard never starts a session in Fabric itself.
//!
//! Also the script of *Open terminal*: a shell in the session's folder, here or over ssh.

/// The app id Fabric's launch journal records as `app`.
pub const FROM: &str = "fabric-switchboard";

pub const PROJECT_INVALID: &str =
    "This folder's name cannot name a Fabric project: use letters, digits, dots, dashes, underscores or colons.";
pub const SESSION_INVALID: &str = "This session has no id Fabric can open.";
pub const RUNNER_INVALID: &str = "That agent is not one Fabric can start.";
pub const FOLDER_INVALID: &str =
    "The session's folder is not known, so no terminal can open in it.";
pub const OPEN_FAILED: &str = "Fabric could not be opened. Check that Fabric is installed.";

/// What a link asks Fabric to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The launch dialog of a project (`project` required, `runner` optional).
    New,
    /// The session's view: Fabric's console when Fabric holds it, else its last messages.
    Open,
    /// *Continue in Fabric?*: after the person presses it, Fabric resumes the session there.
    Continue,
}

/// DEC-0040's project reference: an id or a slug, no slash, no space.
fn project_ok(p: &str) -> bool {
    let b = p.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'-'))
}

/// A session id as Fabric reads it (`^[A-Za-z0-9._-]{1,128}$`), starting with a letter or digit.
fn session_ok(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// A runner kind of the runner catalogue: a lowercase slug.
fn runner_ok(r: &str) -> bool {
    let b = r.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// The link, or the sentence that says why there is none. Every value is checked against its
/// grammar, so nothing in it needs escaping.
pub fn link(
    kind: Kind,
    project: Option<&str>,
    runner: Option<&str>,
    session: Option<&str>,
) -> Result<String, &'static str> {
    match kind {
        Kind::New => {
            let project = project.filter(|p| project_ok(p)).ok_or(PROJECT_INVALID)?;
            let runner = match runner {
                Some(r) if !runner_ok(r) => return Err(RUNNER_INVALID),
                Some(r) => format!("&runner={r}"),
                None => String::new(),
            };
            Ok(format!(
                "fabric://session/new?project={project}{runner}&from={FROM}"
            ))
        }
        Kind::Open | Kind::Continue => {
            let session = session.filter(|s| session_ok(s)).ok_or(SESSION_INVALID)?;
            let verb = if kind == Kind::Open {
                "open"
            } else {
                "continue"
            };
            Ok(format!(
                "fabric://session/{verb}?session={session}&from={FROM}"
            ))
        }
    }
}

/// The Fabric project a folder names: its last component, when that is a valid reference.
/// Fabric names a project after its repository's folder.
pub fn project_of(folder: &str) -> Option<String> {
    let name = std::path::Path::new(folder).file_name()?.to_str()?;
    project_ok(name).then(|| name.to_owned())
}

/// Opens `url` with the system's handler for `fabric://` (Fabric), and returns at once.
pub fn open(url: &str) -> Result<(), &'static str> {
    if !url.starts_with("fabric://session/") {
        return Err(OPEN_FAILED);
    }
    crate::vesta::open_browser(url).map_err(|_| OPEN_FAILED)
}

/// The script of *Open terminal*: the person's login shell in `folder`, here, or on `target`
/// through `ssh -t`. Every word is quoted; the folder must be absolute.
pub fn terminal_script(folder: &str, target: Option<&str>) -> Result<String, &'static str> {
    if !folder.starts_with('/') || folder.contains('\n') || folder.contains('\0') {
        return Err(FOLDER_INVALID);
    }
    let q = fabric_sessions_core::cli::quoted;
    Ok(match target {
        None => format!(
            "#!/bin/sh\ncd {} || exit 1\nexec \"${{SHELL:-/bin/sh}}\" -l\n",
            q(folder)
        ),
        Some(t) => {
            let there = format!("cd {} && exec \"${{SHELL:-/bin/sh}}\" -l", q(folder));
            format!("#!/bin/sh\nexec ssh -t {} {}\n", q(t), q(&there))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_link_names_ids_only_and_says_it_came_from_switchboard() {
        assert_eq!(
            link(Kind::New, Some("fabric-switchboard"), Some("claude-code"), None),
            Ok("fabric://session/new?project=fabric-switchboard&runner=claude-code&from=fabric-switchboard".into())
        );
        assert_eq!(
            link(Kind::New, Some("p1"), None, None),
            Ok("fabric://session/new?project=p1&from=fabric-switchboard".into())
        );
        let id = "d4a341f6-0124-47dc-9b0d-04dea0b255d1";
        assert_eq!(
            link(Kind::Open, None, None, Some(id)),
            Ok(format!(
                "fabric://session/open?session={id}&from=fabric-switchboard"
            ))
        );
        assert_eq!(
            link(Kind::Continue, None, None, Some(id)),
            Ok(format!(
                "fabric://session/continue?session={id}&from=fabric-switchboard"
            ))
        );
    }

    #[test]
    fn a_value_outside_its_grammar_makes_no_link() {
        for bad in [
            "",
            "a b",
            "a/b",
            "-x",
            "a&b=c",
            "a?b",
            "ä",
            &"x".repeat(129),
        ] {
            assert_eq!(
                link(Kind::New, Some(bad), None, None),
                Err(PROJECT_INVALID),
                "{bad}"
            );
            assert_eq!(
                link(Kind::Continue, None, None, Some(bad)),
                Err(SESSION_INVALID),
                "{bad}"
            );
        }
        assert_eq!(link(Kind::New, None, None, None), Err(PROJECT_INVALID));
        assert_eq!(link(Kind::Open, None, None, None), Err(SESSION_INVALID));
        assert_eq!(
            link(Kind::New, Some("p"), Some("Claude Code"), None),
            Err(RUNNER_INVALID)
        );
        assert_eq!(
            link(Kind::Continue, None, None, Some("a:b")),
            Err(SESSION_INVALID)
        );
    }

    #[test]
    fn a_folder_names_its_project_by_its_last_component() {
        assert_eq!(
            project_of("/Users/x/DATA/fabric-switchboard"),
            Some("fabric-switchboard".into())
        );
        assert_eq!(project_of("/Users/x/My Project"), None);
        assert_eq!(project_of("/"), None);
    }

    #[test]
    fn only_a_session_link_is_opened() {
        assert_eq!(open("https://example.com"), Err(OPEN_FAILED));
        assert_eq!(open("file:///etc/passwd"), Err(OPEN_FAILED));
    }

    #[test]
    fn the_terminal_opens_in_the_folder_here_or_over_ssh_with_every_word_quoted() {
        assert_eq!(
            terminal_script("/w/it's", None).unwrap(),
            "#!/bin/sh\ncd '/w/it'\\''s' || exit 1\nexec \"${SHELL:-/bin/sh}\" -l\n"
        );
        let there = terminal_script("/w/a b", Some("studio")).unwrap();
        assert!(
            there.starts_with("#!/bin/sh\nexec ssh -t 'studio' 'cd '\\''/w/a b'\\'' && exec"),
            "{there}"
        );
        assert_eq!(terminal_script("relative", None), Err(FOLDER_INVALID));
        assert_eq!(terminal_script("/a\nb", None), Err(FOLDER_INVALID));
    }
}
