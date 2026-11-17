#![cfg_attr(not(unix), allow(dead_code))]
//! SB-403 S2 (DEC-0024): `switchboard ssh-serve`, the restricted entry another machine reaches
//! over ssh to delegate a seat here.
//!
//! The person puts it in that machine's key line in `~/.ssh/authorized_keys` as a forced command:
//!
//! ```text
//! command="/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard ssh-serve --sender-key KEY",restrict ssh-ed25519 AAAA… other-machine
//! ```
//!
//! sshd then runs it whatever the other machine asked, with the asked command in
//! `SSH_ORIGINAL_COMMAND`. That command is read here as words (the single-quoted words
//! `account_send::remote` writes), never given to a shell, and answered only when it is one of the
//! calls a delegation needs: the machine's proof, the account, group and organization lists (each
//! cut to the delegated seats and to names, no folder), saving a delegated seat, its usage check,
//! its lease, its call back and its removal. Every call that names a seat must name the key the
//! forced command names, and acts only on seats that key delegated. Anything else is refused
//! before the store is opened.
use serde_json::{json, Value};

pub const NOT_THROUGH_SSH: &str = "switchboard ssh-serve runs only as the forced command of an ssh key, which hands it the command asked in SSH_ORIGINAL_COMMAND.";
pub const NOT_ALLOWED: &str = "This machine answers another machine only for delegated seats: moving one here, reading, extending and calling it back. Nothing was run.";
pub const KEY_MISMATCH: &str =
    "That call names another key than the one this machine accepts from the calling machine.";
pub const DATA_DIR_MISMATCH: &str =
    "That call names another data folder than the one this machine serves.";
#[cfg_attr(unix, allow(dead_code))]
pub const UNIX_ONLY: &str = "switchboard ssh-serve runs on macOS and Linux.";
/// The longest command read; a delegation's longest call is a few hundred characters.
const MAX_COMMAND: usize = 16 * 1024;

/// One call another machine may make here.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Prove {
        nonce: String,
    },
    AccountsList,
    ProjectList,
    OrgList,
    OrgAssign {
        id: String,
        contexts: Vec<String>,
    },
    Add {
        provider: String,
        label: String,
        pool: String,
        lease_hours: Option<u32>,
    },
    Remove {
        id: String,
    },
    Usage {
        id: String,
    },
    Surrender {
        id: String,
        undo: bool,
    },
    Lease {
        id: String,
        hours: u32,
    },
}

/// The words of `command` as a POSIX shell would split single-quoted words: plain words of safe
/// characters, `'…'` taken literally and `\'` for a quote. Anything a shell would expand or join
/// (`$`, backquotes, `;`, `|`, `&`, redirections, double quotes, globs, a line end) is refused.
pub fn words(command: &str) -> Result<Vec<String>, String> {
    if command.len() > MAX_COMMAND {
        return Err(NOT_ALLOWED.into());
    }
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err(NOT_ALLOWED.into()),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\'') => {
                    in_word = true;
                    word.push('\'');
                }
                _ => return Err(NOT_ALLOWED.into()),
            },
            c if c.is_ascii_alphanumeric() || "-_./:@=+,".contains(c) => {
                in_word = true;
                word.push(c);
            }
            _ => return Err(NOT_ALLOWED.into()),
        }
    }
    if in_word {
        out.push(word);
    }
    if out.iter().any(|w| w.contains(['\n', '\r', '\0'])) {
        return Err(NOT_ALLOWED.into());
    }
    Ok(out)
}

/// An account id here or there: a UUID's characters, at most 64.
fn id_word(w: &str) -> Option<String> {
    (!w.is_empty() && w.len() <= 64 && w.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
        .then(|| w.to_owned())
}

/// Reads `--flag value` and bare `--flag` pairs, each at most once, only from `allowed`.
fn flags<'a>(
    rest: &'a [String],
    with_value: &[&str],
    bare: &[&str],
) -> Result<std::collections::BTreeMap<&'a str, Option<&'a str>>, String> {
    let mut seen = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < rest.len() {
        let name = rest[i].as_str();
        let value = if with_value.contains(&name) {
            i += 1;
            Some(rest.get(i).ok_or(NOT_ALLOWED)?.as_str())
        } else if bare.contains(&name) {
            None
        } else {
            return Err(NOT_ALLOWED.into());
        };
        if seen.insert(name, value).is_some() {
            return Err(NOT_ALLOWED.into());
        }
        i += 1;
    }
    Ok(seen)
}

/// The asked command as one allowed call. `root` is the data folder this entry serves: a call
/// naming another one with `--data-dir` is refused; `sender_key` the key the forced command
/// names, which every call naming a seat must name too.
pub fn parse(
    words: &[String],
    sender_key: &str,
    root: &std::path::Path,
) -> Result<Request, String> {
    // The first word is the program the other machine names; this one answers instead.
    let mut rest = words.get(1..).ok_or(NOT_ALLOWED)?;
    if rest.first().map(String::as_str) == Some("--data-dir") {
        let dir = rest.get(1).ok_or(NOT_ALLOWED)?;
        if std::path::Path::new(dir) != root {
            return Err(DATA_DIR_MISMATCH.into());
        }
        rest = &rest[2..];
    }
    if rest.first().map(String::as_str) != Some("--json") {
        return Err(NOT_ALLOWED.into());
    }
    let rest = &rest[1..];
    let key_named = |given: Option<Option<&str>>| match given {
        Some(Some(k)) if k == sender_key => Ok(()),
        Some(Some(_)) => Err(KEY_MISMATCH.to_string()),
        _ => Err(NOT_ALLOWED.to_string()),
    };
    let w: Vec<&str> = rest.iter().map(String::as_str).collect();
    Ok(match w.as_slice() {
        ["machines", "prove", "--nonce", nonce] => Request::Prove {
            nonce: (*nonce).to_owned(),
        },
        ["accounts", "list"] => Request::AccountsList,
        ["project", "list"] => Request::ProjectList,
        ["org", "list"] => Request::OrgList,
        ["org", "assign", id, contexts @ ..] if !contexts.is_empty() && contexts.len() <= 16 => {
            Request::OrgAssign {
                id: id_word(id).ok_or(NOT_ALLOWED)?,
                contexts: contexts.iter().map(|c| (*c).to_owned()).collect(),
            }
        }
        ["usage", id] => Request::Usage {
            id: id_word(id).ok_or(NOT_ALLOWED)?,
        },
        ["accounts", "add", ..] => {
            let f = flags(
                &rest[2..],
                &[
                    "--provider",
                    "--kind",
                    "--label",
                    "--pool",
                    "--sender-key",
                    "--lease-hours",
                ],
                &["--secret-stdin", "--delegated"],
            )?;
            // A seat comes here only delegated, under the forced key.
            if !f.contains_key("--delegated") || !f.contains_key("--secret-stdin") {
                return Err(switchboard_runtime::account_send::DELEGATE_ONLY.into());
            }
            key_named(f.get("--sender-key").copied())?;
            if f.get("--kind").copied().flatten() != Some("oauth") {
                return Err(switchboard_runtime::account_send::NOT_A_SIGN_IN.into());
            }
            let provider = f.get("--provider").copied().flatten().ok_or(NOT_ALLOWED)?;
            if !matches!(provider, "claude" | "codex") {
                return Err(NOT_ALLOWED.into());
            }
            let lease_hours = match f.get("--lease-hours").copied().flatten() {
                Some(h) => Some(
                    h.parse::<u32>()
                        .map_err(|_| switchboard_core::delegation::LEASE_INVALID.to_string())?,
                ),
                None => None,
            };
            Request::Add {
                provider: provider.to_owned(),
                label: f
                    .get("--label")
                    .copied()
                    .flatten()
                    .ok_or(NOT_ALLOWED)?
                    .to_owned(),
                pool: f
                    .get("--pool")
                    .copied()
                    .flatten()
                    .ok_or(NOT_ALLOWED)?
                    .to_owned(),
                lease_hours,
            }
        }
        ["accounts", "remove", id, ..] => {
            let f = flags(&rest[3..], &["--sender-key"], &[])?;
            key_named(f.get("--sender-key").copied())?;
            Request::Remove {
                id: id_word(id).ok_or(NOT_ALLOWED)?,
            }
        }
        ["accounts", "surrender", id, ..] => {
            let f = flags(&rest[3..], &["--sender-key"], &["--undo"])?;
            key_named(f.get("--sender-key").copied())?;
            Request::Surrender {
                id: id_word(id).ok_or(NOT_ALLOWED)?,
                undo: f.contains_key("--undo"),
            }
        }
        ["accounts", "lease", id, ..] => {
            let f = flags(&rest[3..], &["--hours", "--sender-key"], &[])?;
            key_named(f.get("--sender-key").copied())?;
            let hours = f
                .get("--hours")
                .copied()
                .flatten()
                .and_then(|h| h.parse::<u32>().ok())
                .ok_or(switchboard_core::delegation::LEASE_INVALID)?;
            Request::Lease {
                id: id_word(id).ok_or(NOT_ALLOWED)?,
                hours,
            }
        }
        _ => return Err(NOT_ALLOWED.into()),
    })
}

/// The seats `sender_key` delegated here, by id.
fn seats_of(snapshot: &Value, sender_key: &str) -> Vec<String> {
    snapshot["delegated"]
        .as_object()
        .map(|d| {
            d.iter()
                .filter(|(_, v)| v["sender_key"] == sender_key)
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The account list another machine sees: only the seats it delegated, with their lease.
pub fn accounts_view(snapshot: &Value, sender_key: &str) -> Value {
    let ours = seats_of(snapshot, sender_key);
    let accounts: Vec<Value> = snapshot["accounts"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|a| {
                    a["id"]
                        .as_str()
                        .is_some_and(|id| ours.iter().any(|o| o == id))
                })
                .map(|a| {
                    let id = a["id"].as_str().unwrap_or_default();
                    let d = &snapshot["delegated"][id];
                    json!({"id": a["id"], "label": a["label"], "provider": a["provider"],
                        "kind": a["kind"], "pool": a["pool"], "enabled": a["enabled"],
                        "usage": a["usage"], "usage_health": a["usage_health"],
                        "delegated": true, "lease_until": d["lease_until"],
                        "lease_ended": !d["ended_at"].is_null()})
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"accounts": accounts})
}

/// The groups another machine sees: id, name and organizations; never a folder or an account.
pub fn projects_view(snapshot: &Value) -> Value {
    let projects: Vec<Value> = snapshot["projects"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|p| p["tokens"] != true)
                .map(|p| json!({"pool": p["pool"], "name": p["name"], "organizations": p["organizations"]}))
                .collect()
        })
        .unwrap_or_default();
    json!({"projects": projects})
}

/// The organizations another machine sees: ids and names only.
pub fn organizations_view(organizations: &Value) -> Value {
    let list: Vec<Value> = organizations["organizations"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|o| json!({"id": o["id"], "name": o["name"]}))
                .collect()
        })
        .unwrap_or_default();
    json!({"organizations": list})
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const ID: &str = "0b1edeee-4125-403c-9c12-44c6aadaeb2b";

    /// What the delegating machine sends: `account_send::remote`'s words, as sshd hands them on.
    fn asked(args: &[&str]) -> Vec<String> {
        let program = switchboard_runtime::account_send::Program {
            path: switchboard_runtime::account_send::REMOTE_DEFAULT,
            data_dir: None,
        };
        words(&switchboard_runtime::account_send::remote(&program, args)).unwrap()
    }
    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from("/data/switchboard")
    }

    #[test]
    fn the_quoted_words_a_delegation_sends_read_back_exactly() {
        let w = asked(&["--json", "accounts", "add", "--label", "it's a seat"]);
        assert_eq!(w[0], switchboard_runtime::account_send::REMOTE_DEFAULT);
        assert_eq!(w[5], "it's a seat");
        // Anything a shell would expand or chain is refused, never split around.
        for bad in [
            "switchboard --json accounts list; rm -rf ~",
            "switchboard --json accounts list | sh",
            "switchboard $(id)",
            "switchboard `id`",
            "switchboard \"--json\"",
            "switchboard --json accounts list\nrm -rf ~",
            "switchboard 'unterminated",
            "switchboard --json accounts list > /tmp/x",
            "switchboard --json accounts list && id",
            "switchboard *",
        ] {
            assert_eq!(words(bad).unwrap_err(), NOT_ALLOWED, "{bad}");
        }
    }

    #[test]
    fn only_the_calls_a_delegation_needs_are_answered() {
        let root = root();
        let ok = |args: &[&str]| parse(&asked(args), KEY, &root);
        assert_eq!(
            ok(&["--json", "accounts", "list"]).unwrap(),
            Request::AccountsList
        );
        assert_eq!(
            ok(&["--json", "project", "list"]).unwrap(),
            Request::ProjectList
        );
        assert_eq!(ok(&["--json", "org", "list"]).unwrap(), Request::OrgList);
        assert!(matches!(
            ok(&["--json", "machines", "prove", "--nonce", "ab"]).unwrap(),
            Request::Prove { .. }
        ));
        assert_eq!(
            ok(&["--json", "usage", ID]).unwrap(),
            Request::Usage { id: ID.into() }
        );
        assert_eq!(
            ok(&[
                "--json",
                "accounts",
                "add",
                "--provider",
                "claude",
                "--kind",
                "oauth",
                "--label",
                "seat",
                "--pool",
                "org",
                "--secret-stdin",
                "--delegated",
                "--sender-key",
                KEY,
                "--lease-hours",
                "48"
            ])
            .unwrap(),
            Request::Add {
                provider: "claude".into(),
                label: "seat".into(),
                pool: "org".into(),
                lease_hours: Some(48)
            }
        );
        assert_eq!(
            ok(&[
                "--json",
                "accounts",
                "surrender",
                ID,
                "--undo",
                "--sender-key",
                KEY
            ])
            .unwrap(),
            Request::Surrender {
                id: ID.into(),
                undo: true
            }
        );
        assert_eq!(
            ok(&[
                "--json",
                "accounts",
                "lease",
                ID,
                "--hours",
                "48",
                "--sender-key",
                KEY
            ])
            .unwrap(),
            Request::Lease {
                id: ID.into(),
                hours: 48
            }
        );
        assert_eq!(
            ok(&["--json", "accounts", "remove", ID, "--sender-key", KEY]).unwrap(),
            Request::Remove { id: ID.into() }
        );
        // Everything else, the person's own accounts and sign-in included, is refused.
        for refused in [
            vec!["--json", "accounts", "activate", ID],
            vec!["--json", "accounts", "capture", "--provider", "claude"],
            vec!["--json", "accounts", "send", ID, "--to", "x"],
            vec!["--json", "accounts", "update", ID, "--enabled", "false"],
            vec!["--json", "launch", "--provider", "claude"],
            vec![
                "--json",
                "settings",
                "import",
                "/dev/stdin",
                "--signer",
                KEY,
            ],
            vec!["--json", "sessions", "--rows"],
            vec!["--json", "status"],
            vec!["--json", "own-sign-in", "on"],
            vec!["--json", "org", "add", "Acme"],
            vec!["--json", "ssh-serve", "--sender-key", KEY],
            vec!["accounts", "list"],
            vec!["--json", "accounts", "list", "--extra"],
            vec!["--json", "usage", "../../etc/passwd"],
        ] {
            assert_eq!(ok(&refused).unwrap_err(), NOT_ALLOWED, "{refused:?}");
        }
    }

    #[test]
    fn a_seat_is_saved_or_touched_only_delegated_and_under_the_forced_key() {
        let root = root();
        let ok = |args: &[&str]| parse(&asked(args), KEY, &root);
        let other = "HxweHRwbGhkYFxYVFBMSERAPDg0MCwoJCAcGBQQDAgE=";
        assert_eq!(
            ok(&[
                "--json",
                "accounts",
                "add",
                "--provider",
                "claude",
                "--kind",
                "oauth",
                "--label",
                "seat",
                "--pool",
                "org",
                "--secret-stdin"
            ])
            .unwrap_err(),
            switchboard_runtime::account_send::DELEGATE_ONLY
        );
        assert_eq!(
            ok(&[
                "--json",
                "accounts",
                "add",
                "--provider",
                "claude",
                "--kind",
                "api-key",
                "--label",
                "seat",
                "--pool",
                "org",
                "--secret-stdin",
                "--delegated",
                "--sender-key",
                KEY
            ])
            .unwrap_err(),
            switchboard_runtime::account_send::NOT_A_SIGN_IN
        );
        assert_eq!(
            ok(&["--json", "accounts", "surrender", ID, "--sender-key", other]).unwrap_err(),
            KEY_MISMATCH
        );
        assert_eq!(
            ok(&["--json", "accounts", "surrender", ID]).unwrap_err(),
            NOT_ALLOWED
        );
        assert_eq!(
            ok(&["--json", "accounts", "remove", ID]).unwrap_err(),
            NOT_ALLOWED
        );
        // A call naming another data folder is refused; this one's is accepted.
        let program = switchboard_runtime::account_send::Program {
            path: "/usr/local/bin/switchboard",
            data_dir: Some("/elsewhere"),
        };
        let line =
            switchboard_runtime::account_send::remote(&program, &["--json", "accounts", "list"]);
        assert_eq!(
            parse(&words(&line).unwrap(), KEY, &root).unwrap_err(),
            DATA_DIR_MISMATCH
        );
        let program = switchboard_runtime::account_send::Program {
            path: "/usr/local/bin/switchboard",
            data_dir: Some("/data/switchboard"),
        };
        let line =
            switchboard_runtime::account_send::remote(&program, &["--json", "accounts", "list"]);
        assert_eq!(
            parse(&words(&line).unwrap(), KEY, &root).unwrap(),
            Request::AccountsList
        );
    }

    #[test]
    fn the_lists_show_only_the_callers_seats_and_no_folder() {
        let snapshot = json!({
            "accounts": [
                {"id": "a", "label": "mine", "provider": "claude", "kind": "oauth", "pool": "p", "enabled": true},
                {"id": "b", "label": "seat", "provider": "claude", "kind": "oauth", "pool": "org", "enabled": true},
                {"id": "c", "label": "another's", "provider": "claude", "kind": "oauth", "pool": "org", "enabled": true}
            ],
            "delegated": {
                "b": {"sender_key": KEY, "pool": "org", "granted_at": 1, "lease_until": 9},
                "c": {"sender_key": "x", "pool": "org", "granted_at": 1, "lease_until": 9}
            },
            "projects": [{"pool": "org", "name": "Org", "folders": ["/Users/someone/work"], "organizations": ["o1"]}]
        });
        let accounts = accounts_view(&snapshot, KEY);
        let list = accounts["accounts"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["label"], "seat");
        assert_eq!(list[0]["lease_until"], 9);
        let projects = projects_view(&snapshot).to_string();
        assert!(projects.contains("\"org\"") && !projects.contains("/Users/"));
    }
}
